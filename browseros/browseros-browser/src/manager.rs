use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use browseros_bridge::error::{BridgeError, BridgeResult};
use browseros_bridge::identifiers::BrowserId;
use browseros_bridge::traits::BrowserPort;
use browseros_bridge::types::LaunchOptions;
use browseros_event_bus::EventBus;
use browseros_observability::{LogLevel, LogRecord, Logger};
use browseros_types::event::EventMetadata;
use browseros_types::identifiers::{CorrelationId, ModuleId};
use browseros_types::value::SemVer;

use crate::backend::SharedBackendRegistry;
use crate::config::BrowserConfig;
use crate::events::{BrowserClosed, BrowserStarted};
use crate::handle::BrowserHandle;
use crate::lifecycle::BrowserState;
use crate::process::BrowserProcess;
use crate::transport::TransportManager;

static BROWSER_MODULE_ID: std::sync::LazyLock<ModuleId> =
    std::sync::LazyLock::new(|| ModuleId::new("browseros-browser", SemVer::new(0, 1, 0)));

/// A managed browser instance tracked by [`BrowserManager`].
pub struct BrowserInstance {
    /// Unique identifier for this browser instance.
    pub id: BrowserId,
    /// The bridge port.
    pub port: Arc<dyn BrowserPort>,
    /// Current lifecycle state.
    pub state: BrowserState,
    /// The underlying OS process (if launched locally).
    pub process: Option<Arc<BrowserProcess>>,
    /// Transport manager.
    pub transport: TransportManager,
}

impl BrowserInstance {
    fn new(port: Arc<dyn BrowserPort>, process: Option<Arc<BrowserProcess>>) -> Self {
        Self {
            id: BrowserId::new(),
            port,
            state: BrowserState::Connected,
            process,
            transport: TransportManager::new(),
        }
    }
}

/// Manages browser lifecycle, sessions, and page tracking.
///
/// `BrowserManager` is the entry point for all browser operations.
/// It maintains a registry of active browser instances and emits
/// domain events through the EventBus for every lifecycle transition.
pub struct BrowserManager {
    bus: Arc<EventBus>,
    logger: Arc<Logger>,
    config: BrowserConfig,
    instances: RwLock<Vec<Arc<Mutex<BrowserInstance>>>>,
    backends: SharedBackendRegistry,
    closed: AtomicBool,
}

impl BrowserManager {
    /// Create a new BrowserManager.
    pub fn new(bus: Arc<EventBus>, logger: Arc<Logger>, config: &BrowserConfig) -> Self {
        Self {
            bus,
            logger,
            config: config.clone(),
            instances: RwLock::new(Vec::new()),
            backends: crate::backend::new_shared_registry(),
            closed: AtomicBool::new(false),
        }
    }

    /// Register a backend factory.
    pub fn register_backend(&self, factory: Box<dyn crate::backend::BackendFactory>) {
        if let Ok(mut reg) = self.backends.write() {
            reg.register(factory);
        }
    }

    /// Returns the shared backend registry.
    pub fn backends(&self) -> &SharedBackendRegistry {
        &self.backends
    }

    /// Launch a new browser instance.
    pub fn launch(&self, options: LaunchOptions) -> BridgeResult<BrowserHandle> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(error_closed("BrowserManager is shut down"));
        }

        let browser_id = BrowserId::new();

        let backend_registry = self
            .backends
            .read()
            .map_err(|_| BridgeError::Internal("backend registry lock poisoned".into()))?;

        let backend = backend_registry.get("chromium").or_else(|| {
            backend_registry
                .names()
                .first()
                .and_then(|name| backend_registry.get(name))
        });

        let port: Arc<dyn BrowserPort> = match backend {
            Some(factory) => {
                let p = factory.launch(options)?;
                Arc::from(p)
            }
            None => {
                return Err(BridgeError::NotImplemented(
                    "no browser backends registered; use register_backend() or browseros-cdp",
                ));
            }
        };

        let info = port.info();
        let _browser_id = browser_id;

        let instance = Arc::new(Mutex::new(BrowserInstance::new(port.clone(), None)));

        {
            let mut instances = self
                .instances
                .write()
                .map_err(|_| BridgeError::Internal("instance list lock poisoned".into()))?;
            instances.push(instance);
        }

        self.bus.publish(Box::new(BrowserStarted {
            metadata: self.new_metadata(),
            browser_id: _browser_id,
            version: info.version.clone(),
            executable: info.executable.display().to_string(),
            ws_endpoint: String::new(),
        }));

        self.logger.log(LogRecord {
            timestamp: chrono::Utc::now(),
            level: LogLevel::Info,
            target: "browseros-browser".into(),
            message: format!("browser launched: {_browser_id} v{}", info.version),
            fields: Vec::new(),
            correlation_id: None,
        });

        Ok(BrowserHandle::new(port))
    }

    /// Connect to an already-running browser at the given CDP endpoint.
    pub fn connect(&self, endpoint: &str) -> BridgeResult<BrowserHandle> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(error_closed("BrowserManager is shut down"));
        }

        let browser_id = BrowserId::new();

        let backend_registry = self
            .backends
            .read()
            .map_err(|_| BridgeError::Internal("backend registry lock poisoned".into()))?;

        let backend = backend_registry.get("chromium").or_else(|| {
            backend_registry
                .names()
                .first()
                .and_then(|name| backend_registry.get(name))
        });

        let port: Arc<dyn BrowserPort> = match backend {
            Some(factory) => {
                let p = factory.connect(endpoint)?;
                Arc::from(p)
            }
            None => {
                return Err(BridgeError::NotImplemented(
                    "no browser backends registered; use register_backend() or browseros-cdp",
                ));
            }
        };

        let info = port.info();
        let ws_endpoint = endpoint.to_owned();

        let instance = Arc::new(Mutex::new(BrowserInstance::new(port.clone(), None)));

        {
            let mut instances = self
                .instances
                .write()
                .map_err(|_| BridgeError::Internal("instance list lock poisoned".into()))?;
            instances.push(instance);
        }

        self.bus.publish(Box::new(BrowserStarted {
            metadata: self.new_metadata(),
            browser_id,
            version: info.version.clone(),
            executable: info.executable.display().to_string(),
            ws_endpoint,
        }));

        self.logger.log(LogRecord {
            timestamp: chrono::Utc::now(),
            level: LogLevel::Info,
            target: "browseros-browser".into(),
            message: format!("browser connected: {browser_id} at {endpoint}"),
            fields: Vec::new(),
            correlation_id: None,
        });

        Ok(BrowserHandle::new(port))
    }

    /// Returns all managed browser instances as handles.
    pub fn browsers(&self) -> Vec<BrowserHandle> {
        let instances = self.instances.read().ok();
        match instances {
            Some(list) => list
                .iter()
                .filter_map(|inst| inst.lock().ok().map(|i| BrowserHandle::new(i.port.clone())))
                .collect(),
            None => Vec::new(),
        }
    }

    /// Returns the first browser instance, if any.
    pub fn default_browser(&self) -> Option<BrowserHandle> {
        let instances = self.instances.read().ok()?;
        let inst = instances.first()?;
        inst.lock().ok().map(|i| BrowserHandle::new(i.port.clone()))
    }

    /// Close a specific browser instance.
    pub fn close_browser(&self, id: &BrowserId) -> BridgeResult<()> {
        let instance = {
            let instances = self
                .instances
                .read()
                .map_err(|_| BridgeError::Internal("instance list lock poisoned".into()))?;
            instances
                .iter()
                .find(|inst| inst.lock().ok().is_some_and(|i| i.id == *id))
                .cloned()
        };

        match instance {
            Some(inst) => {
                let exit_code = {
                    let mut guard = inst
                        .lock()
                        .map_err(|_| BridgeError::Internal("instance lock poisoned".into()))?;
                    guard.state = BrowserState::Closing;
                    guard.port.close()?;
                    guard.state = BrowserState::Closed;
                    guard.process.as_ref().and_then(|p| p.wait().ok()).flatten()
                };

                self.bus.publish(Box::new(BrowserClosed {
                    metadata: self.new_metadata(),
                    browser_id: *id,
                    exit_code,
                    reason: "explicit close".into(),
                }));

                self.logger.log(LogRecord {
                    timestamp: chrono::Utc::now(),
                    level: LogLevel::Info,
                    target: "browseros-browser".into(),
                    message: format!("browser closed: {id}"),
                    fields: Vec::new(),
                    correlation_id: None,
                });

                if let Ok(mut instances) = self.instances.write() {
                    instances.retain(|inst| inst.lock().ok().is_some_and(|i| i.id != *id));
                }

                Ok(())
            }
            None => Err(error_closed("browser not found")),
        }
    }

    /// Close all browser instances.
    pub fn close_all(&self) -> Vec<BridgeResult<()>> {
        let ids: Vec<BrowserId> = {
            let instances = self.instances.read().ok();
            match instances {
                Some(list) => list
                    .iter()
                    .filter_map(|inst| inst.lock().ok().map(|i| i.id))
                    .collect(),
                None => Vec::new(),
            }
        };

        ids.iter().map(|id| self.close_browser(id)).collect()
    }

    /// Shut down the manager.  All browsers are closed.
    pub fn shutdown(&self) -> Vec<BridgeResult<()>> {
        self.closed.store(true, Ordering::SeqCst);
        self.close_all()
    }

    fn new_metadata(&self) -> EventMetadata {
        EventBus::new_metadata(BROWSER_MODULE_ID.clone(), CorrelationId::new())
    }
}

fn error_closed(msg: &str) -> BridgeError {
    BridgeError::Internal(format!("browser closed: {msg}"))
}

impl Drop for BrowserManager {
    fn drop(&mut self) {
        if !self.closed.load(Ordering::SeqCst) && self.config.cleanup_on_drop {
            self.close_all();
        }
    }
}
