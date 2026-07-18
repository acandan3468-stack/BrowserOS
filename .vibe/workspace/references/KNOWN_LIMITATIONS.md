# Known Limitations

BrowserOS Phase 2 known limitations and constraints.

---

## 1. Chrome Dependency

Integration tests require Chrome/Chromium on PATH. Not bundled. Tests gracefully skip when Chrome is unavailable (`smoke_browser_launch_and_connect`).

## 2. Windows-Specific Backend Discovery

CDP backend discovers Chrome via Windows registry. Linux/macOS backend discovery not implemented. Manual endpoint configuration via `connect()` is available as a workaround.

## 3. Opaque Origin Storage

Cookies and localStorage write operations silently degrade on `about:blank` and other opaque-origin pages. This is a browser security constraint, not a bug.

## 4. Synchronous API

All public API is synchronous (std threads). No async/future support. Planned for Phase 4.

## 5. CDP Backend Module Visibility

`browseros-browser::cdp_backend` is `pub mod`, exposing `CdpBrowserBackend` and `CdpConfig` through the public API. This is intentional — `browseros-browser` is the composition layer that registers backends. Feature-gating behind `#[cfg(feature = "cdp")]` is planned for Phase 3.

## 6. `cdp_node_id` Hardcoded in DOM

The DOM crate's JavaScript queries hardcode `cdp_node_id` as an attribute name, coupling the DOM layer to CDP implementation details. A bridge abstraction for element identification is needed for alternative backends (WebDriver BiDi). Planned for Phase 3.

## 7. `set_session_storage` Missing

`StorageManager::set_session_storage` is not implemented because the `StoragePort` bridge trait lacks a session-storage write method. Requires bridge extension.

## 8. `FrameHandle::is_cross_origin()` Always Returns `false`

Placeholder implementation. Cross-origin frame detection requires bridge extension.

## 9. `selector_filter` No-Op on `FrameHandle::snapshot`

The `selector_filter` parameter is accepted but not forwarded to the backend. Backend implementation pending.

## 10. Wildcard Re-Exports in Bridge

`browseros-bridge` uses `pub use identifiers::*`, `pub use traits::*`, `pub use types::*`. Any new type added to these modules silently becomes public API. Acceptable for frozen API; documented for future maintenance.

## 11. `MutationObserver` Partially Implemented

`MutationObserver` struct and options are defined, but callback dispatch is not wired. The observer struct exists as scaffolding for Phase 2+.

## 12. Dead Code Staging

14 items have `#[allow(dead_code)]` — all are intentional scaffolding for future phases (DOM traversal, CDP element port, event-driven waiting, lazy node info).

## 13. No Cross-Origin Frame Tests

No dedicated tests verify cross-origin frame behavior.

## 14. No Shadow DOM Deep Traversal Tests

Only basic shadow DOM query is tested. Deep traversal and closed shadow root edge cases are untested.

## 15. No Concurrent Page Access Tests

Race condition testing under concurrent page access is not covered.

## 16. No EventBus Cross-Crate Integration Tests

Event flow from browser events through EventBus to subscribers across crate boundaries is untested.
