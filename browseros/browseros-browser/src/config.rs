use std::time::Duration;

/// Configuration for the browser subsystem.
#[derive(Debug, Clone)]
pub struct BrowserConfig {
    /// Default timeout for browser launch operations.
    pub launch_timeout: Duration,
    /// Default timeout for browser shutdown operations.
    pub shutdown_timeout: Duration,
    /// Default timeout for connecting to an existing browser.
    pub connect_timeout: Duration,
    /// Whether to enable crash detection polling.
    pub crash_detection: bool,
    /// Interval for crash detection polling.
    pub crash_poll_interval: Duration,
    /// Whether to automatically clean up browser processes on drop.
    pub cleanup_on_drop: bool,
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            launch_timeout: Duration::from_secs(30),
            shutdown_timeout: Duration::from_secs(10),
            connect_timeout: Duration::from_secs(15),
            crash_detection: true,
            crash_poll_interval: Duration::from_secs(2),
            cleanup_on_drop: true,
        }
    }
}
