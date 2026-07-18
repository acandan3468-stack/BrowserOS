use std::sync::Arc;

use browseros_bridge::error::BridgeError;
use browseros_bridge::identifiers::{BrowserId, NavigationId, PageId, SessionId};
use browseros_bridge::traits::{
    BrowserPort, DialogPort, DownloadPort, ElementPort, FramePort, InputPort, LocatorPort,
    NetworkPort, PagePort, SessionPort, StoragePort,
};
use browseros_bridge::types::{
    BrowserInfo, JsResult, LaunchOptions, NavigationState, NavigationStatus, PdfOptions,
    ScreenshotOptions, SessionConfig, Viewport, WaitCondition,
};
use browseros_event_bus::EventBus;
use browseros_observability::{LevelFilter, LogLevel, LogRecord, Logger, OutputSink};
use browseros_types::event::{DomainEvent, Event};
use browseros_types::identifiers::{CorrelationId, ModuleId};
use browseros_types::value::SemVer;

use browseros_browser::backend::BackendFactory;
use browseros_browser::config::BrowserConfig;
use browseros_browser::events::{BrowserClosed, BrowserStarted, PageCreated};
use browseros_browser::lifecycle::BrowserState;
use browseros_browser::{BackendRegistry, BrowserManager, TransportManager};

// ——— Mock Bridge Types ———

struct MockBrowser {
    info: BrowserInfo,
}

impl MockBrowser {
    fn new() -> Self {
        Self {
            info: BrowserInfo {
                executable: std::path::PathBuf::from("/usr/bin/chrome"),
                version: "130.0.0".into(),
                user_data_dir: std::path::PathBuf::from("/tmp/chrome-test"),
            },
        }
    }
}

impl BrowserPort for MockBrowser {
    fn info(&self) -> BrowserInfo {
        self.info.clone()
    }
    fn create_session(&self, _config: SessionConfig) -> Result<Box<dyn SessionPort>, BridgeError> {
        Ok(Box::new(MockSession::new()))
    }
    fn sessions(&self) -> Vec<Box<dyn SessionPort>> {
        vec![Box::new(MockSession::new())]
    }
    fn close(&self) -> Result<(), BridgeError> {
        Ok(())
    }
    fn kill(&self) -> Result<(), BridgeError> {
        Ok(())
    }
    fn is_alive(&self) -> bool {
        true
    }
    fn launch(&self, _options: LaunchOptions) -> Result<(), BridgeError> {
        Ok(())
    }
}

struct MockSession {
    config: SessionConfig,
}

impl MockSession {
    fn new() -> Self {
        Self {
            config: SessionConfig::default(),
        }
    }
}

impl SessionPort for MockSession {
    fn pages(&self) -> Vec<Box<dyn PagePort>> {
        vec![Box::new(MockPage(PageId::new()))]
    }
    fn create_page(&self) -> Result<Box<dyn PagePort>, BridgeError> {
        Ok(Box::new(MockPage(PageId::new())))
    }
    fn close_page(&self, _page_id: &PageId) -> Result<(), BridgeError> {
        Ok(())
    }
    fn activate_page(&self, _page_id: &PageId) -> Result<(), BridgeError> {
        Ok(())
    }
    fn close(&self) -> Result<(), BridgeError> {
        Ok(())
    }
    fn config(&self) -> &SessionConfig {
        &self.config
    }
}

struct MockPage(PageId);

impl PagePort for MockPage {
    fn id(&self) -> PageId {
        self.0
    }
    fn url(&self) -> String {
        "about:blank".into()
    }
    fn title(&self) -> String {
        String::new()
    }
    fn navigate(&self, _url: &str) -> Result<NavigationState, BridgeError> {
        Ok(NavigationState {
            navigation_id: NavigationId::new(),
            url: "about:blank".into(),
            status: NavigationStatus::Finished,
        })
    }
    fn reload(&self) -> Result<NavigationState, BridgeError> {
        Ok(NavigationState {
            navigation_id: NavigationId::new(),
            url: "about:blank".into(),
            status: NavigationStatus::Finished,
        })
    }
    fn go_back(&self) -> Result<NavigationState, BridgeError> {
        Ok(NavigationState {
            navigation_id: NavigationId::new(),
            url: "about:blank".into(),
            status: NavigationStatus::Finished,
        })
    }
    fn go_forward(&self) -> Result<NavigationState, BridgeError> {
        Ok(NavigationState {
            navigation_id: NavigationId::new(),
            url: "about:blank".into(),
            status: NavigationStatus::Finished,
        })
    }
    fn evaluate(
        &self,
        _script: &str,
        _arg: Option<&serde_json::Value>,
    ) -> Result<JsResult, BridgeError> {
        Ok(JsResult {
            value: serde_json::Value::Null,
            exception_details: None,
        })
    }
    fn evaluate_handle(
        &self,
        _script: &str,
        _arg: Option<&serde_json::Value>,
    ) -> Result<Box<dyn ElementPort>, BridgeError> {
        Err(BridgeError::NotImplemented("mock"))
    }
    fn screenshot(&self, _options: ScreenshotOptions) -> Result<Vec<u8>, BridgeError> {
        Ok(vec![0u8; 100])
    }
    fn pdf(&self, _options: PdfOptions) -> Result<Vec<u8>, BridgeError> {
        Ok(vec![0u8; 50])
    }
    fn content(&self) -> Result<String, BridgeError> {
        Ok("<html></html>".into())
    }
    fn set_content(&self, _html: &str) -> Result<(), BridgeError> {
        Ok(())
    }
    fn set_viewport(&self, _viewport: Viewport) -> Result<(), BridgeError> {
        Ok(())
    }
    fn wait_for(&self, _condition: WaitCondition) -> Result<(), BridgeError> {
        Ok(())
    }
    fn frames(&self) -> Vec<Box<dyn FramePort>> {
        Vec::new()
    }
    fn main_frame(&self) -> Box<dyn FramePort> {
        panic!("no main frame")
    }
    fn close(&self) -> Result<(), BridgeError> {
        Ok(())
    }
    fn locator(&self) -> Box<dyn LocatorPort> {
        panic!("no locator")
    }
    fn network(&self) -> Box<dyn NetworkPort> {
        panic!("no network")
    }
    fn input(&self) -> Box<dyn InputPort> {
        panic!("no input")
    }
    fn storage(&self) -> Box<dyn StoragePort> {
        panic!("no storage")
    }
    fn dialog(&self) -> Box<dyn DialogPort> {
        panic!("no dialog")
    }
    fn download(&self) -> Box<dyn DownloadPort> {
        panic!("no download")
    }
}

// ——— Backend for MockBrowser ———

struct MockBackendFactory;

impl BackendFactory for MockBackendFactory {
    fn name(&self) -> &str {
        "mock"
    }
    fn launch(&self, _options: LaunchOptions) -> Result<Box<dyn BrowserPort>, BridgeError> {
        Ok(Box::new(MockBrowser::new()))
    }
    fn connect(&self, _endpoint: &str) -> Result<Box<dyn BrowserPort>, BridgeError> {
        Ok(Box::new(MockBrowser::new()))
    }
}

// ——— Test Helpers ———

struct NoopSink;

impl OutputSink for NoopSink {
    fn write(&self, _record: &LogRecord) {}
    fn flush(&self) {}
}

fn test_module() -> ModuleId {
    ModuleId::new("test", SemVer::new(0, 1, 0))
}

fn test_manager() -> BrowserManager {
    let bus = Arc::new(EventBus::new());
    let logger = Arc::new(Logger::new(
        Arc::new(NoopSink),
        Arc::new(LevelFilter::new(LogLevel::Warn)),
        "test",
    ));
    let config = BrowserConfig::default();
    let manager = BrowserManager::new(bus, logger, &config);
    manager.register_backend(Box::new(MockBackendFactory));
    manager
}

fn default_launch_options() -> LaunchOptions {
    LaunchOptions {
        executable: None,
        headless: true,
        args: Vec::new(),
        env: std::collections::HashMap::new(),
        user_data_dir: None,
        timeout: std::time::Duration::from_secs(30),
    }
}

// ——— Lifecycle State Machine Tests ———

#[test]
fn test_startup_to_connected() {
    assert_eq!(
        BrowserState::Startup.transition_to(BrowserState::Connected),
        Ok(BrowserState::Connected)
    );
}

#[test]
fn test_startup_to_crashed() {
    assert_eq!(
        BrowserState::Startup.transition_to(BrowserState::Crashed),
        Ok(BrowserState::Crashed)
    );
}

#[test]
fn test_connected_to_running() {
    assert_eq!(
        BrowserState::Connected.transition_to(BrowserState::Running),
        Ok(BrowserState::Running)
    );
}

#[test]
fn test_running_to_closing() {
    assert_eq!(
        BrowserState::Running.transition_to(BrowserState::Closing),
        Ok(BrowserState::Closing)
    );
}

#[test]
fn test_closing_to_closed() {
    assert_eq!(
        BrowserState::Closing.transition_to(BrowserState::Closed),
        Ok(BrowserState::Closed)
    );
}

#[test]
fn test_invalid_transition_from_closed() {
    assert!(BrowserState::Closed
        .transition_to(BrowserState::Running)
        .is_err());
}

#[test]
fn test_crashed_to_recovering() {
    assert_eq!(
        BrowserState::Crashed.transition_to(BrowserState::Recovering),
        Ok(BrowserState::Recovering)
    );
}

#[test]
fn test_is_active() {
    assert!(BrowserState::Running.is_active());
    assert!(!BrowserState::Crashed.is_active());
    assert!(!BrowserState::Closed.is_active());
}

#[test]
fn test_is_terminal() {
    assert!(BrowserState::Closed.is_terminal());
    assert!(!BrowserState::Running.is_terminal());
}

#[test]
fn test_is_failure() {
    assert!(BrowserState::Crashed.is_failure());
    assert!(!BrowserState::Running.is_failure());
}

#[test]
fn test_display() {
    assert_eq!(BrowserState::Startup.to_string(), "startup");
    assert_eq!(BrowserState::Crashed.to_string(), "crashed");
    assert_eq!(BrowserState::Closed.to_string(), "closed");
}

// ——— BrowserManager Tests ———

#[test]
fn test_launch_browser() {
    let mgr = test_manager();
    let handle = mgr.launch(default_launch_options());
    assert!(handle.is_ok());
    let handle = handle.unwrap();
    let info = handle.info();
    assert_eq!(info.version, "130.0.0");
}

#[test]
fn test_connect_browser() {
    let mgr = test_manager();
    let handle = mgr.connect("ws://localhost:9222/devtools/browser/abc123");
    assert!(handle.is_ok());
    let handle = handle.unwrap();
    let info = handle.info();
    assert_eq!(info.version, "130.0.0");
}

#[test]
fn test_default_browser() {
    let mgr = test_manager();
    assert!(mgr.default_browser().is_none());
    let _handle = mgr.launch(default_launch_options()).unwrap();
    assert!(mgr.default_browser().is_some());
}

#[test]
fn test_browsers_list() {
    let mgr = test_manager();
    assert!(mgr.browsers().is_empty());
    let _h1 = mgr.launch(default_launch_options()).unwrap();
    assert_eq!(mgr.browsers().len(), 1);
    let _h2 = mgr.launch(default_launch_options()).unwrap();
    assert_eq!(mgr.browsers().len(), 2);
}

#[test]
fn test_close_browser() {
    let mgr = test_manager();
    let _handle = mgr.launch(default_launch_options()).unwrap();
    assert_eq!(mgr.browsers().len(), 1);

    let results = mgr.close_all();
    assert_eq!(results.len(), 1);
    assert!(results[0].is_ok());
    assert!(mgr.browsers().is_empty());
}

#[test]
fn test_shutdown_closes_all() {
    let mgr = test_manager();
    let _h1 = mgr.launch(default_launch_options()).unwrap();
    let _h2 = mgr.launch(default_launch_options()).unwrap();
    let results = mgr.shutdown();
    assert_eq!(results.len(), 2);
    assert!(results.into_iter().all(|r| r.is_ok()));
    assert!(mgr.browsers().is_empty());
}

#[test]
fn test_no_backend_returns_error() {
    let bus = Arc::new(EventBus::new());
    let logger = Arc::new(Logger::new(
        Arc::new(NoopSink),
        Arc::new(LevelFilter::new(LogLevel::Warn)),
        "test",
    ));
    let mgr = BrowserManager::new(bus, logger, &BrowserConfig::default());
    let result = mgr.launch(default_launch_options());
    assert!(result.is_err());
    assert!(matches!(result, Err(BridgeError::NotImplemented(_))));
}

// ——— BrowserHandle Tests ———

#[test]
fn test_browser_handle_info() {
    let mgr = test_manager();
    let handle = mgr.launch(default_launch_options()).unwrap();
    let info = handle.info();
    assert_eq!(info.version, "130.0.0");
}

#[test]
fn test_browser_handle_new_session() {
    let mgr = test_manager();
    let handle = mgr.launch(default_launch_options()).unwrap();
    let session = handle.new_session(SessionConfig::default());
    assert!(session.is_ok());
}

#[test]
fn test_browser_handle_sessions() {
    let mgr = test_manager();
    let handle = mgr.launch(default_launch_options()).unwrap();
    let sessions = handle.sessions();
    assert!(!sessions.is_empty());
}

// ——— SessionHandle Tests ———

#[test]
fn test_session_new_page() {
    let mgr = test_manager();
    let handle = mgr.launch(default_launch_options()).unwrap();
    let session = handle.new_session(SessionConfig::default()).unwrap();
    let page = session.new_page();
    assert!(page.is_ok());
}

#[test]
fn test_session_pages() {
    let mgr = test_manager();
    let handle = mgr.launch(default_launch_options()).unwrap();
    let session = handle.new_session(SessionConfig::default()).unwrap();
    let pages = session.pages();
    assert!(!pages.is_empty());
}

// ——— PageHandle Tests ———

#[test]
fn test_page_handle_url() {
    let mgr = test_manager();
    let handle = mgr.launch(default_launch_options()).unwrap();
    let session = handle.new_session(SessionConfig::default()).unwrap();
    let page = session.new_page().unwrap();
    assert!(page.url() == "about:blank");
}

#[test]
fn test_page_handle_title() {
    let mgr = test_manager();
    let handle = mgr.launch(default_launch_options()).unwrap();
    let session = handle.new_session(SessionConfig::default()).unwrap();
    let page = session.new_page().unwrap();
    assert!(page.title().is_empty());
}

#[test]
fn test_page_navigate() {
    let mgr = test_manager();
    let handle = mgr.launch(default_launch_options()).unwrap();
    let session = handle.new_session(SessionConfig::default()).unwrap();
    let page = session.new_page().unwrap();
    let nav = page.navigate("https://example.com");
    assert!(nav.is_ok());
    assert_eq!(nav.unwrap().status, NavigationStatus::Finished);
}

#[test]
fn test_page_screenshot() {
    let mgr = test_manager();
    let handle = mgr.launch(default_launch_options()).unwrap();
    let session = handle.new_session(SessionConfig::default()).unwrap();
    let page = session.new_page().unwrap();
    let img = page.screenshot(ScreenshotOptions::default());
    assert!(img.is_ok());
    assert!(img.unwrap().len() == 100);
}

#[test]
fn test_page_pdf() {
    let mgr = test_manager();
    let handle = mgr.launch(default_launch_options()).unwrap();
    let session = handle.new_session(SessionConfig::default()).unwrap();
    let page = session.new_page().unwrap();
    let pdf = page.pdf(PdfOptions::default());
    assert!(pdf.is_ok());
}

#[test]
fn test_page_frames() {
    let mgr = test_manager();
    let handle = mgr.launch(default_launch_options()).unwrap();
    let session = handle.new_session(SessionConfig::default()).unwrap();
    let page = session.new_page().unwrap();
    let frames = page.frames();
    assert!(frames.is_empty());
}

#[test]
fn test_page_close() {
    let mgr = test_manager();
    let handle = mgr.launch(default_launch_options()).unwrap();
    let session = handle.new_session(SessionConfig::default()).unwrap();
    let page = session.new_page().unwrap();
    assert!(page.close().is_ok());
}

// ——— Event Bus Integration Tests ———

#[test]
fn test_browser_started_event_emitted() {
    let bus = Arc::new(EventBus::new());
    let logger = Arc::new(Logger::new(
        Arc::new(NoopSink),
        Arc::new(LevelFilter::new(LogLevel::Warn)),
        "test",
    ));
    let config = BrowserConfig::default();
    let mgr = BrowserManager::new(bus.clone(), logger, &config);
    mgr.register_backend(Box::new(MockBackendFactory));

    let received = Arc::new(std::sync::Mutex::new(false));
    let r = received.clone();
    bus.subscribe(
        "browser.started",
        Arc::new(move |_: &dyn Event| {
            *r.lock().unwrap() = true;
        }),
    );

    let _ = mgr.launch(default_launch_options());
    assert!(*received.lock().unwrap());
}

#[test]
fn test_browser_closed_event_emitted() {
    let bus = Arc::new(EventBus::new());
    let logger = Arc::new(Logger::new(
        Arc::new(NoopSink),
        Arc::new(LevelFilter::new(LogLevel::Warn)),
        "test",
    ));
    let config = BrowserConfig::default();
    let mgr = BrowserManager::new(bus.clone(), logger, &config);
    mgr.register_backend(Box::new(MockBackendFactory));

    let received = Arc::new(std::sync::Mutex::new(false));
    let r = received.clone();
    bus.subscribe(
        "browser.closed",
        Arc::new(move |_: &dyn Event| {
            *r.lock().unwrap() = true;
        }),
    );

    let _handle = mgr.launch(default_launch_options()).unwrap();

    let results = mgr.close_all();
    assert!(!results.is_empty());
    assert!(results[0].is_ok());
    assert!(*received.lock().unwrap());
}

// ——— BrowserConfig Tests ———

#[test]
fn test_browser_config_default() {
    let config = BrowserConfig::default();
    assert_eq!(config.launch_timeout, std::time::Duration::from_secs(30));
    assert!(config.crash_detection);
    assert!(config.cleanup_on_drop);
}

#[test]
fn test_browser_config_custom() {
    let config = BrowserConfig {
        launch_timeout: std::time::Duration::from_secs(60),
        shutdown_timeout: std::time::Duration::from_secs(30),
        connect_timeout: std::time::Duration::from_secs(20),
        crash_detection: false,
        crash_poll_interval: std::time::Duration::from_secs(5),
        cleanup_on_drop: false,
    };
    assert_eq!(config.launch_timeout.as_secs(), 60);
    assert!(!config.crash_detection);
    assert!(!config.cleanup_on_drop);
}

// ——— BackendRegistry Tests ———

#[test]
fn test_backend_registry_empty() {
    let reg = BackendRegistry::new();
    assert!(reg.is_empty());
    assert_eq!(reg.len(), 0);
}

#[test]
fn test_backend_registry_register() {
    let mut reg = BackendRegistry::new();
    reg.register(Box::new(MockBackendFactory));
    assert!(!reg.is_empty());
    assert_eq!(reg.len(), 1);
    assert!(reg.get("mock").is_some());
}

#[test]
fn test_backend_registry_names() {
    let mut reg = BackendRegistry::new();
    reg.register(Box::new(MockBackendFactory));
    let names = reg.names();
    assert_eq!(names, vec!["mock"]);
}

// ——— BrowserState Properties ———

#[test]
fn test_browser_state_all_active() {
    assert!(BrowserState::Startup.is_active());
    assert!(BrowserState::Connected.is_active());
    assert!(BrowserState::Running.is_active());
    assert!(BrowserState::Recovering.is_active());
    assert!(!BrowserState::Closing.is_active());
    assert!(!BrowserState::Closed.is_active());
    assert!(!BrowserState::Crashed.is_active());
}

#[test]
fn test_browser_state_terminal_and_failure() {
    assert!(BrowserState::Closed.is_terminal());
    assert!(BrowserState::Crashed.is_failure());
}

// ——— TransportManager Tests ———

#[test]
fn test_transport_manager_default_disconnected() {
    let tm = TransportManager::new();
    assert!(!tm.is_connected());
}

#[test]
fn test_transport_manager_connect_disconnect() {
    let tm = TransportManager::new();
    tm.mark_connected();
    assert!(tm.is_connected());
    tm.mark_disconnected();
    assert!(!tm.is_connected());
}

// ——— Event Types Tests ———

#[test]
fn test_browser_started_event_kind() {
    let ev = BrowserStarted {
        metadata: EventBus::new_metadata(test_module(), CorrelationId::new()),
        browser_id: BrowserId::new(),
        version: "130.0.0".into(),
        executable: "/usr/bin/chrome".into(),
        ws_endpoint: "ws://localhost:9222".into(),
    };
    assert_eq!(ev.kind(), "browser.started");
}

#[test]
fn test_browser_closed_event_kind() {
    let ev = BrowserClosed {
        metadata: EventBus::new_metadata(test_module(), CorrelationId::new()),
        browser_id: BrowserId::new(),
        exit_code: Some(0),
        reason: "closed".into(),
    };
    assert_eq!(ev.kind(), "browser.closed");
}

#[test]
fn test_page_created_event_kind() {
    let ev = PageCreated {
        metadata: EventBus::new_metadata(test_module(), CorrelationId::new()),
        page_id: PageId::new(),
        session_id: SessionId::new(),
        url: "about:blank".into(),
        about_blank: true,
        created_at: chrono::Utc::now(),
    };
    assert_eq!(ev.kind(), "page.created");
}

#[test]
fn test_event_domain_trait() {
    let ev = BrowserStarted {
        metadata: EventBus::new_metadata(test_module(), CorrelationId::new()),
        browser_id: BrowserId::new(),
        version: "1.0".into(),
        executable: "/bin/chrome".into(),
        ws_endpoint: "".into(),
    };
    fn assert_domain(_: &dyn DomainEvent) {}
    assert_domain(&ev);
}

// ——— Handle Send/Sync/Clone Tests ———

#[test]
fn test_browser_handle_send_sync() {
    fn assert_send<T: Send>(_: &T) {}
    fn assert_sync<T: Sync>(_: &T) {}

    let mgr = test_manager();
    let handle = mgr.launch(default_launch_options()).unwrap();
    assert_send(&handle);
    assert_sync(&handle);
}

#[test]
fn test_handle_clone() {
    let mgr = test_manager();
    let handle = mgr.launch(default_launch_options()).unwrap();
    let handle2 = handle.clone();
    assert_eq!(handle.info().version, handle2.info().version);
}

#[test]
fn test_browser_manager_send_sync() {
    fn assert_send<T: Send>(_: &T) {}
    fn assert_sync<T: Sync>(_: &T) {}

    let mgr = test_manager();
    assert_send(&mgr);
    assert_sync(&mgr);
}
