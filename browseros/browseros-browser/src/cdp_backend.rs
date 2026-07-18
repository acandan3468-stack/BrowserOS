use browseros_bridge::error::BridgeResult;
use browseros_bridge::traits::BrowserPort;
use browseros_bridge::types::LaunchOptions;

use browseros_cdp::config::CdpConfig;
use browseros_cdp::factory::{connect_to_endpoint, spawn_chrome};

use crate::backend::BackendFactory;

/// A [`BackendFactory`] that launches or connects to Chromium-based
/// browsers via the Chrome DevTools Protocol.
pub struct CdpBrowserBackend {
    config: CdpConfig,
}

impl CdpBrowserBackend {
    pub fn new() -> Self {
        Self {
            config: CdpConfig::default(),
        }
    }

    pub fn with_config(config: CdpConfig) -> Self {
        Self { config }
    }
}

impl Default for CdpBrowserBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl BackendFactory for CdpBrowserBackend {
    fn name(&self) -> &str {
        "chromium"
    }

    fn launch(&self, options: LaunchOptions) -> BridgeResult<Box<dyn BrowserPort>> {
        let (child, endpoint) = spawn_chrome(&options)?;
        let browser = connect_to_endpoint(&endpoint, &self.config, Some(child))?;
        Ok(Box::new(browser))
    }

    fn connect(&self, endpoint: &str) -> BridgeResult<Box<dyn BrowserPort>> {
        let browser = connect_to_endpoint(endpoint, &self.config, None)?;
        Ok(Box::new(browser))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_name() {
        let backend = CdpBrowserBackend::new();
        assert_eq!(backend.name(), "chromium");
    }

    #[test]
    fn test_backend_with_config() {
        let config = CdpConfig::default().with_connect_timeout(std::time::Duration::from_secs(30));
        let backend = CdpBrowserBackend::with_config(config);
        assert_eq!(backend.name(), "chromium");
    }

    #[test]
    fn test_backend_default() {
        let backend = CdpBrowserBackend::default();
        assert_eq!(backend.name(), "chromium");
    }

    #[test]
    fn test_backend_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<CdpBrowserBackend>();
    }

    #[test]
    fn test_backend_implements_trait() {
        fn assert_trait<T: BackendFactory>() {}
        assert_trait::<CdpBrowserBackend>();
    }

    #[test]
    fn test_connect_to_nonexistent_endpoint() {
        let backend = CdpBrowserBackend::new();
        let result = backend.connect("ws://127.0.0.1:1");
        assert!(result.is_err());
    }
}
