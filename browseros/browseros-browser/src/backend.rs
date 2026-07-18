use std::sync::Arc;

use browseros_bridge::error::BridgeResult;
use browseros_bridge::traits::BrowserPort;
use browseros_bridge::types::LaunchOptions;

/// A factory that creates browser backends.
///
/// Each backend knows how to launch or connect to a specific browser
/// engine (Chromium, WebDriver BiDi, etc.) and returns a [`BrowserPort`]
/// implementation.  This trait is the extension point for adding new
/// browser backends.
pub trait BackendFactory: Send + Sync {
    /// A human-readable name for this backend (e.g. `"chromium"`, `"firefox"`).
    fn name(&self) -> &str;

    /// Launch a new browser process with the given options.
    fn launch(&self, options: LaunchOptions) -> BridgeResult<Box<dyn BrowserPort>>;

    /// Connect to an already-running browser at the given endpoint.
    fn connect(&self, endpoint: &str) -> BridgeResult<Box<dyn BrowserPort>>;
}

/// Registry of available browser backends.
///
/// Backends are registered by name.  `BrowserManager` uses this registry
/// to select the appropriate backend at launch/connect time.
pub struct BackendRegistry {
    backends: Vec<Box<dyn BackendFactory>>,
}

impl BackendRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            backends: Vec::new(),
        }
    }

    /// Register a backend factory.
    pub fn register(&mut self, factory: Box<dyn BackendFactory>) {
        self.backends.push(factory);
    }

    /// Find a backend by name.
    pub fn get(&self, name: &str) -> Option<&dyn BackendFactory> {
        self.backends
            .iter()
            .find(|f| f.name() == name)
            .map(|f| f.as_ref())
    }

    /// Returns all registered backend names.
    pub fn names(&self) -> Vec<&str> {
        self.backends.iter().map(|f| f.name()).collect()
    }

    /// Returns `true` if no backends are registered.
    pub fn is_empty(&self) -> bool {
        self.backends.is_empty()
    }

    /// Returns the number of registered backends.
    pub fn len(&self) -> usize {
        self.backends.len()
    }
}

impl Default for BackendRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// A wrapper to make BackendRegistry thread-safe.
pub type SharedBackendRegistry = Arc<std::sync::RwLock<BackendRegistry>>;

/// Create a new shared backend registry.
pub fn new_shared_registry() -> SharedBackendRegistry {
    Arc::new(std::sync::RwLock::new(BackendRegistry::new()))
}
