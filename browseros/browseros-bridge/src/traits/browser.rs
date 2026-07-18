use crate::error::BridgeResult;
use crate::traits::SessionPort;
use crate::types::{BrowserInfo, LaunchOptions, SessionConfig};

/// Interface for a browser process.
///
/// `BrowserPort` represents the entire browser process. It is the root of
/// the port hierarchy. From a `BrowserPort`, callers create isolated
/// sessions (incognito contexts) and manage the browser lifecycle.
pub trait BrowserPort: Send + Sync {
    /// Returns metadata about the browser process.
    fn info(&self) -> BrowserInfo;

    /// Launches or connects to a browser instance.
    fn launch(&self, options: LaunchOptions) -> BridgeResult<()>;

    /// Creates a new isolated session (browsing context).
    fn create_session(&self, config: SessionConfig) -> BridgeResult<Box<dyn SessionPort>>;

    /// Returns all active sessions.
    fn sessions(&self) -> Vec<Box<dyn SessionPort>>;

    /// Gracefully closes the browser process.
    fn close(&self) -> BridgeResult<()>;

    /// Forcefully terminates the browser process.
    fn kill(&self) -> BridgeResult<()>;

    /// Returns `true` if the browser process is still alive.
    fn is_alive(&self) -> bool;
}
