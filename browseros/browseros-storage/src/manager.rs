use std::collections::HashMap;

use browseros_bridge::traits::{PagePort, StoragePort};
use browseros_bridge::types::Cookie;

use crate::error::{StorageError, StorageResult};

/// Higher-level cookie and web storage manager.
///
/// `StorageManager` wraps a `StoragePort` obtained from a `PagePort` and
/// provides a more ergonomic API: singular `set_cookie`, key-based local
/// storage operations, and `HashMap` return types instead of `Vec`s.
pub struct StorageManager {
    inner: Box<dyn StoragePort>,
}

impl StorageManager {
    /// Create a new `StorageManager` for the given page.
    pub fn new(page: &dyn PagePort) -> Self {
        Self {
            inner: page.storage(),
        }
    }

    /// Create a `StorageManager` from an existing `StoragePort`.
    ///
    /// Useful when the caller already has a reference to a storage port
    /// and wants to use the higher-level API without going through a page.
    pub fn from_port(port: Box<dyn StoragePort>) -> Self {
        Self { inner: port }
    }

    // ——— Cookies ———

    /// Return all cookies for the page's origin.
    pub fn get_cookies(&self) -> StorageResult<Vec<Cookie>> {
        self.inner.cookies().map_err(Into::into)
    }

    /// Set a single cookie.
    pub fn set_cookie(&self, cookie: Cookie) -> StorageResult<()> {
        self.inner.set_cookies(&[cookie]).map_err(Into::into)
    }

    /// Delete a cookie by name and URL.
    pub fn delete_cookie(&self, name: &str, url: &str) -> StorageResult<()> {
        self.inner.delete_cookie(name, url).map_err(Into::into)
    }

    /// Delete all cookies.
    pub fn delete_all_cookies(&self) -> StorageResult<()> {
        self.inner.delete_all_cookies().map_err(Into::into)
    }

    // ——— Local Storage ———

    /// Return all local storage entries as a `HashMap`.
    pub fn get_local_storage(&self) -> StorageResult<HashMap<String, String>> {
        let entries = self.inner.local_storage().map_err(StorageError::from)?;
        Ok(entries.into_iter().map(|e| (e.key, e.value)).collect())
    }

    /// Set a single key/value pair in local storage.
    pub fn set_local_storage(&self, key: &str, value: &str) -> StorageResult<()> {
        let entry = browseros_bridge::types::StorageEntry {
            key: key.to_string(),
            value: value.to_string(),
        };
        self.inner.set_local_storage(&[entry]).map_err(Into::into)
    }

    /// Remove a single key from local storage.
    ///
    /// Implementation reads all entries, clears storage, then writes back
    /// all entries except the one to remove. This works around the fact
    /// that `StoragePort` does not expose a single-key delete for local
    /// storage.
    pub fn remove_local_storage(&self, key: &str) -> StorageResult<()> {
        let entries = self.inner.local_storage().map_err(StorageError::from)?;
        let remaining: Vec<_> = entries.into_iter().filter(|e| e.key != key).collect();
        self.inner
            .clear_local_storage()
            .map_err(StorageError::from)?;
        self.inner
            .set_local_storage(&remaining)
            .map_err(StorageError::from)
    }

    /// Clear all local storage.
    pub fn clear_local_storage(&self) -> StorageResult<()> {
        self.inner.clear_local_storage().map_err(Into::into)
    }

    // ——— Session Storage ———

    /// Return all session storage entries as a `HashMap`.
    pub fn get_session_storage(&self) -> StorageResult<HashMap<String, String>> {
        let entries = self.inner.session_storage().map_err(StorageError::from)?;
        Ok(entries.into_iter().map(|e| (e.key, e.value)).collect())
    }

    /// Clear all session storage.
    pub fn clear_session_storage(&self) -> StorageResult<()> {
        self.inner.clear_session_storage().map_err(Into::into)
    }

    // TODO(post-phase-2): add `set_session_storage(key, value)` once
    // `StoragePort` exposes a session-storage write method. The frozen
    // bridge trait currently lacks one, and adding a write via the
    // existing `clear_session_storage` + `session_storage` read would
    // require an atomic read-modify-write that is racy. Deferred until
    // the bridge API is extended — see phase2-crate-map.md:591.
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::StorageError;
    use browseros_bridge::error::{BridgeError, BridgeResult};
    use browseros_bridge::identifiers::{NavigationId, PageId};
    use browseros_bridge::traits::{
        DialogPort, DownloadPort, ElementPort, FramePort, InputPort, LocatorPort, NetworkPort,
    };
    use browseros_bridge::types::{
        JsResult, NavigationState, NavigationStatus, PdfOptions, ScreenshotOptions, Viewport,
        WaitCondition,
    };

    // ——— Mock StoragePort ———

    #[derive(Clone)]
    struct MockData {
        cookies: Vec<Cookie>,
        local: Vec<browseros_bridge::types::StorageEntry>,
        session: Vec<browseros_bridge::types::StorageEntry>,
    }

    struct MockStoragePort {
        data: std::sync::Arc<std::sync::Mutex<MockData>>,
    }

    impl MockStoragePort {
        fn new() -> Self {
            Self {
                data: std::sync::Arc::new(std::sync::Mutex::new(MockData {
                    cookies: Vec::new(),
                    local: Vec::new(),
                    session: Vec::new(),
                })),
            }
        }
    }

    impl StoragePort for MockStoragePort {
        fn cookies(&self) -> BridgeResult<Vec<Cookie>> {
            Ok(self.data.lock().unwrap().cookies.clone())
        }

        fn set_cookies(&self, cookies: &[Cookie]) -> BridgeResult<()> {
            self.data.lock().unwrap().cookies = cookies.to_vec();
            Ok(())
        }

        fn delete_cookie(&self, name: &str, _url: &str) -> BridgeResult<()> {
            self.data.lock().unwrap().cookies.retain(|c| c.name != name);
            Ok(())
        }

        fn delete_all_cookies(&self) -> BridgeResult<()> {
            self.data.lock().unwrap().cookies.clear();
            Ok(())
        }

        fn local_storage(&self) -> BridgeResult<Vec<browseros_bridge::types::StorageEntry>> {
            Ok(self.data.lock().unwrap().local.clone())
        }

        fn set_local_storage(
            &self,
            entries: &[browseros_bridge::types::StorageEntry],
        ) -> BridgeResult<()> {
            let mut data = self.data.lock().unwrap();
            for entry in entries {
                if let Some(existing) = data.local.iter_mut().find(|e| e.key == entry.key) {
                    existing.value = entry.value.clone();
                } else {
                    data.local.push(entry.clone());
                }
            }
            Ok(())
        }

        fn clear_local_storage(&self) -> BridgeResult<()> {
            self.data.lock().unwrap().local.clear();
            Ok(())
        }

        fn session_storage(&self) -> BridgeResult<Vec<browseros_bridge::types::StorageEntry>> {
            Ok(self.data.lock().unwrap().session.clone())
        }

        fn clear_session_storage(&self) -> BridgeResult<()> {
            self.data.lock().unwrap().session.clear();
            Ok(())
        }
    }

    // ——— Mock PagePort ———

    struct MockPagePort {
        _page_id: PageId,
        data: std::sync::Arc<std::sync::Mutex<MockData>>,
    }

    impl MockPagePort {
        fn new() -> Self {
            Self {
                _page_id: PageId::new(),
                data: std::sync::Arc::new(std::sync::Mutex::new(MockData {
                    cookies: Vec::new(),
                    local: Vec::new(),
                    session: Vec::new(),
                })),
            }
        }
    }

    impl PagePort for MockPagePort {
        fn id(&self) -> PageId {
            self._page_id
        }

        fn url(&self) -> String {
            "about:blank".to_string()
        }

        fn title(&self) -> String {
            String::new()
        }

        fn navigate(&self, _url: &str) -> BridgeResult<NavigationState> {
            Ok(nav_state())
        }

        fn reload(&self) -> BridgeResult<NavigationState> {
            Ok(nav_state())
        }

        fn go_back(&self) -> BridgeResult<NavigationState> {
            Ok(nav_state())
        }

        fn go_forward(&self) -> BridgeResult<NavigationState> {
            Ok(nav_state())
        }

        fn evaluate(
            &self,
            _script: &str,
            _arg: Option<&serde_json::Value>,
        ) -> BridgeResult<JsResult> {
            Err(BridgeError::NotImplemented("mock evaluate"))
        }

        fn evaluate_handle(
            &self,
            _script: &str,
            _arg: Option<&serde_json::Value>,
        ) -> BridgeResult<Box<dyn ElementPort>> {
            Err(BridgeError::NotImplemented("mock evaluate_handle"))
        }

        fn screenshot(&self, _options: ScreenshotOptions) -> BridgeResult<Vec<u8>> {
            Err(BridgeError::NotImplemented("mock screenshot"))
        }

        fn pdf(&self, _options: PdfOptions) -> BridgeResult<Vec<u8>> {
            Err(BridgeError::NotImplemented("mock pdf"))
        }

        fn content(&self) -> BridgeResult<String> {
            Err(BridgeError::NotImplemented("mock content"))
        }

        fn set_content(&self, _html: &str) -> BridgeResult<()> {
            Err(BridgeError::NotImplemented("mock set_content"))
        }

        fn set_viewport(&self, _viewport: Viewport) -> BridgeResult<()> {
            Err(BridgeError::NotImplemented("mock set_viewport"))
        }

        fn wait_for(&self, _condition: WaitCondition) -> BridgeResult<()> {
            Err(BridgeError::NotImplemented("mock wait_for"))
        }

        fn frames(&self) -> Vec<Box<dyn FramePort>> {
            Vec::new()
        }

        fn main_frame(&self) -> Box<dyn FramePort> {
            panic!("not implemented in mock")
        }

        fn close(&self) -> BridgeResult<()> {
            Ok(())
        }

        fn locator(&self) -> Box<dyn LocatorPort> {
            panic!("not implemented in mock")
        }

        fn network(&self) -> Box<dyn NetworkPort> {
            panic!("not implemented in mock")
        }

        fn input(&self) -> Box<dyn InputPort> {
            panic!("not implemented in mock")
        }

        fn storage(&self) -> Box<dyn StoragePort> {
            Box::new(MockStoragePort {
                data: self.data.clone(),
            })
        }

        fn dialog(&self) -> Box<dyn DialogPort> {
            panic!("not implemented in mock")
        }

        fn download(&self) -> Box<dyn DownloadPort> {
            panic!("not implemented in mock")
        }
    }

    // ——— Failing mock ———

    struct MockFailingStorage;

    impl StoragePort for MockFailingStorage {
        fn cookies(&self) -> BridgeResult<Vec<Cookie>> {
            Err(BridgeError::NotImplemented("mock cookies"))
        }

        fn set_cookies(&self, _: &[Cookie]) -> BridgeResult<()> {
            Err(BridgeError::NotImplemented("mock set_cookies"))
        }

        fn delete_cookie(&self, _: &str, _: &str) -> BridgeResult<()> {
            Err(BridgeError::NotImplemented("mock delete_cookie"))
        }

        fn delete_all_cookies(&self) -> BridgeResult<()> {
            Err(BridgeError::NotImplemented("mock delete_all_cookies"))
        }

        fn local_storage(&self) -> BridgeResult<Vec<browseros_bridge::types::StorageEntry>> {
            Err(BridgeError::NotImplemented("mock local_storage"))
        }

        fn set_local_storage(
            &self,
            _: &[browseros_bridge::types::StorageEntry],
        ) -> BridgeResult<()> {
            Err(BridgeError::NotImplemented("mock set_local_storage"))
        }

        fn clear_local_storage(&self) -> BridgeResult<()> {
            Err(BridgeError::NotImplemented("mock clear_local_storage"))
        }

        fn session_storage(&self) -> BridgeResult<Vec<browseros_bridge::types::StorageEntry>> {
            Err(BridgeError::NotImplemented("mock session_storage"))
        }

        fn clear_session_storage(&self) -> BridgeResult<()> {
            Err(BridgeError::NotImplemented("mock clear_session_storage"))
        }
    }

    fn nav_state() -> NavigationState {
        NavigationState {
            navigation_id: NavigationId::new(),
            url: "about:blank".to_string(),
            status: NavigationStatus::Finished,
        }
    }

    fn make_cookie(name: &str, value: &str) -> Cookie {
        Cookie {
            name: name.to_string(),
            value: value.to_string(),
            domain: "example.com".to_string(),
            path: "/".to_string(),
            secure: false,
            http_only: false,
            same_site: browseros_bridge::types::SameSitePolicy::Lax,
            expires: None,
        }
    }

    // ——— Tests ———

    #[test]
    fn test_get_cookies_empty() {
        let page = MockPagePort::new();
        let mgr = StorageManager::new(&page);
        let cookies = mgr.get_cookies().unwrap();
        assert!(cookies.is_empty());
    }

    #[test]
    fn test_set_and_get_cookies() {
        let page = MockPagePort::new();
        let mgr = StorageManager::new(&page);
        let c = make_cookie("session_id", "abc123");
        mgr.set_cookie(c).unwrap();
        let cookies = mgr.get_cookies().unwrap();
        assert_eq!(cookies.len(), 1);
        assert_eq!(cookies[0].name, "session_id");
        assert_eq!(cookies[0].value, "abc123");
    }

    #[test]
    fn test_delete_cookie() {
        let page = MockPagePort::new();
        let mgr = StorageManager::new(&page);
        mgr.set_cookie(make_cookie("test", "val")).unwrap();
        mgr.set_cookie(make_cookie("keep", "me")).unwrap();
        mgr.delete_cookie("test", "http://example.com").unwrap();
        let cookies = mgr.get_cookies().unwrap();
        assert_eq!(cookies.len(), 1);
        assert_eq!(cookies[0].name, "keep");
    }

    #[test]
    fn test_delete_all_cookies() {
        let page = MockPagePort::new();
        let mgr = StorageManager::new(&page);
        mgr.set_cookie(make_cookie("a", "1")).unwrap();
        mgr.set_cookie(make_cookie("b", "2")).unwrap();
        mgr.delete_all_cookies().unwrap();
        let cookies = mgr.get_cookies().unwrap();
        assert!(cookies.is_empty());
    }

    #[test]
    fn test_local_storage_empty() {
        let page = MockPagePort::new();
        let mgr = StorageManager::new(&page);
        let map = mgr.get_local_storage().unwrap();
        assert!(map.is_empty());
    }

    #[test]
    fn test_set_and_get_local_storage() {
        let page = MockPagePort::new();
        let mgr = StorageManager::new(&page);
        mgr.set_local_storage("key1", "value1").unwrap();
        mgr.set_local_storage("key2", "value2").unwrap();
        let map = mgr.get_local_storage().unwrap();
        assert_eq!(map.len(), 2);
        assert_eq!(map.get("key1").unwrap(), "value1");
        assert_eq!(map.get("key2").unwrap(), "value2");
    }

    #[test]
    fn test_remove_local_storage() {
        let page = MockPagePort::new();
        let mgr = StorageManager::new(&page);
        mgr.set_local_storage("key1", "value1").unwrap();
        mgr.set_local_storage("key2", "value2").unwrap();
        mgr.remove_local_storage("key1").unwrap();
        let map = mgr.get_local_storage().unwrap();
        assert_eq!(map.len(), 1);
        assert_eq!(map.get("key2").unwrap(), "value2");
    }

    #[test]
    fn test_clear_local_storage() {
        let page = MockPagePort::new();
        let mgr = StorageManager::new(&page);
        mgr.set_local_storage("key1", "value1").unwrap();
        mgr.clear_local_storage().unwrap();
        let map = mgr.get_local_storage().unwrap();
        assert!(map.is_empty());
    }

    #[test]
    fn test_session_storage_empty() {
        let page = MockPagePort::new();
        let mgr = StorageManager::new(&page);
        let map = mgr.get_session_storage().unwrap();
        assert!(map.is_empty());
    }

    #[test]
    fn test_clear_session_storage() {
        let page = MockPagePort::new();
        let mgr = StorageManager::new(&page);
        mgr.clear_session_storage().unwrap();
        let map = mgr.get_session_storage().unwrap();
        assert!(map.is_empty());
    }

    #[test]
    fn test_from_port() {
        let port = Box::new(MockStoragePort::new());
        let mgr = StorageManager::from_port(port);
        let cookies = mgr.get_cookies().unwrap();
        assert!(cookies.is_empty());
    }

    #[test]
    fn test_bridge_error_wrapping() {
        let port = Box::new(MockFailingStorage);
        let mgr = StorageManager::from_port(port);
        let result = mgr.get_cookies();
        assert!(result.is_err());
        match result {
            Err(StorageError::BridgeError(_)) => {}
            _ => panic!("expected BridgeError wrapping"),
        }
    }

    #[test]
    fn test_remove_nonexistent_key() {
        let page = MockPagePort::new();
        let mgr = StorageManager::new(&page);
        // Removing a key that doesn't exist should be a no-op
        mgr.set_local_storage("exists", "val").unwrap();
        mgr.remove_local_storage("nonexistent").unwrap();
        let map = mgr.get_local_storage().unwrap();
        assert_eq!(map.len(), 1);
        assert_eq!(map.get("exists").unwrap(), "val");
    }
}
