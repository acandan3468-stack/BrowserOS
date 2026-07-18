use crate::error::BridgeResult;
use crate::identifiers::PageId;
use crate::traits::PagePort;
use crate::types::SessionConfig;

/// Interface for an isolated browser session (browsing context).
///
/// A session corresponds to a browser context such as an incognito window.
/// Sessions provide cookie, storage, and cache isolation. All pages (tabs)
/// created within a session share the same session state.
pub trait SessionPort: Send + Sync {
    /// Returns all pages (tabs) in this session.
    fn pages(&self) -> Vec<Box<dyn PagePort>>;

    /// Creates a new page (tab) in this session.
    fn create_page(&self) -> BridgeResult<Box<dyn PagePort>>;

    /// Closes a specific page by ID.
    fn close_page(&self, page_id: &PageId) -> BridgeResult<()>;

    /// Brings a page to foreground.
    fn activate_page(&self, page_id: &PageId) -> BridgeResult<()>;

    /// Closes this session and all its pages.
    fn close(&self) -> BridgeResult<()>;

    /// Returns the session configuration.
    fn config(&self) -> &SessionConfig;
}
