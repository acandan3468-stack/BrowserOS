use crate::error::BridgeResult;
use crate::types::{Cookie, StorageEntry};

/// Interface for cookie and web storage management on a page.
///
/// `StoragePort` provides read/write/delete access to cookies, local
/// storage, and session storage for the page's origin.
pub trait StoragePort: Send + Sync {
    // ——— Cookies ———

    /// Returns all cookies for the page's origin.
    fn cookies(&self) -> BridgeResult<Vec<Cookie>>;

    /// Set one or more cookies.
    fn set_cookies(&self, cookies: &[Cookie]) -> BridgeResult<()>;

    /// Delete a single cookie by name and URL.
    fn delete_cookie(&self, name: &str, url: &str) -> BridgeResult<()>;

    /// Delete all cookies.
    fn delete_all_cookies(&self) -> BridgeResult<()>;

    // ——— Local Storage ———

    /// Returns all local storage entries.
    fn local_storage(&self) -> BridgeResult<Vec<StorageEntry>>;

    /// Set multiple local storage entries.
    fn set_local_storage(&self, entries: &[StorageEntry]) -> BridgeResult<()>;

    /// Clear all local storage.
    fn clear_local_storage(&self) -> BridgeResult<()>;

    // ——— Session Storage ———

    /// Returns all session storage entries.
    fn session_storage(&self) -> BridgeResult<Vec<StorageEntry>>;

    /// Clear all session storage.
    fn clear_session_storage(&self) -> BridgeResult<()>;
}
