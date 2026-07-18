# Browser Abstractions — Interface Definitions

**Principle:** BrowserOS must never depend directly on Chromium, CDP, or any specific browser engine. All browser interaction is through stable trait interfaces defined in `browseros-bridge`.

**Status:** Implemented. Signatures below match `browseros-bridge` source code.

---

## 1. Trait Hierarchy

```
BrowserPort
  └── SessionPort (1:N)
        └── PagePort (1:N)
              ├── FramePort (1:N, tree)
              ├── ElementPort (N, tree via query)
              ├── LocatorPort (1 per page)
              ├── LocatorEngine (1 per page, low-level)
              ├── NetworkPort (1 per page)
              ├── InputPort (1 per page)
              ├── StoragePort (1 per page)
              ├── DialogPort (1 per page)
              └── DownloadPort (1 per page)
```

All traits are `Send + Sync`. All fallible methods return `BridgeResult<T>` (which is `Result<T, BridgeError>`).

---

## 2. BrowserPort — Browser Process Lifecycle

```rust
pub trait BrowserPort: Send + Sync {
    fn info(&self) -> BrowserInfo;
    fn launch(&self, options: LaunchOptions) -> BridgeResult<()>;
    fn create_session(&self, config: SessionConfig) -> BridgeResult<Box<dyn SessionPort>>;
    fn sessions(&self) -> Vec<Box<dyn SessionPort>>;
    fn close(&self) -> BridgeResult<()>;
    fn kill(&self) -> BridgeResult<()>;
    fn is_alive(&self) -> bool;
}
```

---

## 3. SessionPort — Browser Session (Context)

```rust
pub trait SessionPort: Send + Sync {
    fn pages(&self) -> Vec<Box<dyn PagePort>>;
    fn create_page(&self) -> BridgeResult<Box<dyn PagePort>>;
    fn close_page(&self, page_id: &PageId) -> BridgeResult<()>;
    fn activate_page(&self, page_id: &PageId) -> BridgeResult<()>;
    fn close(&self) -> BridgeResult<()>;
    fn config(&self) -> &SessionConfig;
}
```

---

## 4. PagePort — Single Tab

```rust
pub trait PagePort: Send + Sync {
    fn id(&self) -> PageId;
    fn url(&self) -> String;
    fn title(&self) -> String;
    fn navigate(&self, url: &str) -> BridgeResult<NavigationState>;
    fn reload(&self) -> BridgeResult<NavigationState>;
    fn go_back(&self) -> BridgeResult<NavigationState>;
    fn go_forward(&self) -> BridgeResult<NavigationState>;
    fn evaluate(&self, script: &str, arg: Option<&serde_json::Value>) -> BridgeResult<JsResult>;
    fn evaluate_handle(&self, script: &str, arg: Option<&serde_json::Value>)
        -> BridgeResult<Box<dyn ElementPort>>;
    fn screenshot(&self, options: ScreenshotOptions) -> BridgeResult<Vec<u8>>;
    fn pdf(&self, options: PdfOptions) -> BridgeResult<Vec<u8>>;
    fn content(&self) -> BridgeResult<String>;
    fn set_content(&self, html: &str) -> BridgeResult<()>;
    fn set_viewport(&self, viewport: Viewport) -> BridgeResult<()>;
    fn wait_for(&self, condition: WaitCondition) -> BridgeResult<()>;
    fn frames(&self) -> Vec<Box<dyn FramePort>>;
    fn main_frame(&self) -> Box<dyn FramePort>;
    fn close(&self) -> BridgeResult<()>;
    fn locator(&self) -> Box<dyn LocatorPort>;
    fn network(&self) -> Box<dyn NetworkPort>;
    fn input(&self) -> Box<dyn InputPort>;
    fn storage(&self) -> Box<dyn StoragePort>;
    fn dialog(&self) -> Box<dyn DialogPort>;
    fn download(&self) -> Box<dyn DownloadPort>;
}
```

---

## 5. FramePort — Frame (iframes, nested browsing contexts)

```rust
pub trait FramePort: Send + Sync {
    fn id(&self) -> FrameId;
    fn url(&self) -> String;
    fn title(&self) -> String;
    fn parent_id(&self) -> Option<FrameId>;
    fn content(&self) -> BridgeResult<String>;
    fn set_content(&self, html: &str) -> BridgeResult<()>;
    fn evaluate(&self, script: &str, arg: Option<&serde_json::Value>) -> BridgeResult<JsResult>;
    fn child_frames(&self) -> Vec<Box<dyn FramePort>>;
    fn page(&self) -> Box<dyn PagePort>;
}
```

---

## 6. ElementPort — DOM Element Handle

```rust
pub trait ElementPort: Send + Sync {
    fn id(&self) -> ElementId;
    fn tag_name(&self) -> String;
    fn text_content(&self) -> BridgeResult<String>;
    fn inner_html(&self) -> BridgeResult<String>;
    fn outer_html(&self) -> BridgeResult<String>;
    fn get_attribute(&self, name: &str) -> BridgeResult<Option<String>>;
    fn set_attribute(&self, name: &str, value: &str) -> BridgeResult<()>;
    fn has_attribute(&self, name: &str) -> BridgeResult<bool>;
    fn attributes(&self) -> BridgeResult<HashMap<String, String>>;
    fn bounding_box(&self) -> BridgeResult<Option<BoxModel>>;
    fn is_visible(&self) -> BridgeResult<bool>;
    fn is_enabled(&self) -> BridgeResult<bool>;
    fn is_checked(&self) -> BridgeResult<bool>;
    fn is_selected(&self) -> BridgeResult<bool>;
    fn is_stable(&self) -> BridgeResult<bool>;
    fn scroll_into_view(&self) -> BridgeResult<()>;
    fn click_point(&self) -> BridgeResult<Point>;
    fn query_selector(&self, selector: &str) -> BridgeResult<Option<Box<dyn ElementPort>>>;
    fn query_selector_all(&self, selector: &str) -> BridgeResult<Vec<Box<dyn ElementPort>>>;
    fn focus(&self) -> BridgeResult<()>;
    fn hover(&self) -> BridgeResult<()>;
    fn snapshot(&self) -> BridgeResult<NodeInfo>;
    fn owning_frame(&self) -> Box<dyn FramePort>;
}
```

---

## 7. LocatorPort — Declarative Element Finding

```rust
pub trait LocatorPort: Send + Sync {
    fn locate(&self, strategy: &LocatorStrategy) -> BridgeResult<Option<Box<dyn ElementPort>>>;
    fn locate_all(&self, strategy: &LocatorStrategy) -> BridgeResult<Vec<Box<dyn ElementPort>>>;
    fn wait_for(&self, strategy: &LocatorStrategy, timeout: Duration)
        -> BridgeResult<Box<dyn ElementPort>>;
    fn wait_for_absence(&self, strategy: &LocatorStrategy, timeout: Duration)
        -> BridgeResult<()>;
}
```

---

## 8. LocatorEngine — Low-Level DOM Querying

```rust
pub trait LocatorEngine: Send + Sync {
    fn query_selector(&self, selector: &str) -> BridgeResult<Option<Box<dyn ElementPort>>>;
    fn query_selector_all(&self, selector: &str) -> BridgeResult<Vec<Box<dyn ElementPort>>>;
    fn query_by_text(&self, text: &str, exact: bool) -> BridgeResult<Vec<Box<dyn ElementPort>>>;
    fn query_by_xpath(&self, expression: &str) -> BridgeResult<Vec<Box<dyn ElementPort>>>;
}
```

---

## 9. NetworkPort — Network Monitoring & Interception

```rust
pub trait NetworkPort: Send + Sync {
    fn set_offline(&self, offline: bool) -> BridgeResult<()>;
    fn set_conditions(&self, conditions: NetworkConditions) -> BridgeResult<()>;
    fn add_interception_rule(&self, rule: InterceptionRule) -> BridgeResult<InterceptionHandle>;
    fn remove_interception_rule(&self, handle: &InterceptionHandle) -> BridgeResult<()>;
    fn clear_cache(&self) -> BridgeResult<()>;
    fn clear_cookies(&self) -> BridgeResult<()>;
}
```

---

## 10. InputPort — Input Simulation

```rust
pub trait InputPort: Send + Sync {
    fn click(&self, element: &dyn ElementPort, options: ClickOptions) -> BridgeResult<()>;
    fn dblclick(&self, element: &dyn ElementPort, options: ClickOptions) -> BridgeResult<()>;
    fn fill(&self, element: &dyn ElementPort, text: &str) -> BridgeResult<()>;
    fn type_text(&self, element: &dyn ElementPort, text: &str, delay: Duration) -> BridgeResult<()>;
    fn press_key(&self, key: &str) -> BridgeResult<()>;
    fn hover(&self, element: &dyn ElementPort) -> BridgeResult<()>;
    fn scroll(&self, delta_x: f64, delta_y: f64) -> BridgeResult<()>;
    fn drag_and_drop(&self, source: &dyn ElementPort, target: &dyn ElementPort) -> BridgeResult<()>;
    fn upload_file(&self, element: &dyn ElementPort, paths: &[PathBuf]) -> BridgeResult<()>;
    fn select_option(&self, element: &dyn ElementPort, values: &[&str]) -> BridgeResult<()>;
    fn check(&self, element: &dyn ElementPort) -> BridgeResult<()>;
    fn uncheck(&self, element: &dyn ElementPort) -> BridgeResult<()>;
}
```

---

## 11. StoragePort — Cookies & Web Storage

```rust
pub trait StoragePort: Send + Sync {
    fn cookies(&self) -> BridgeResult<Vec<Cookie>>;
    fn set_cookies(&self, cookies: &[Cookie]) -> BridgeResult<()>;
    fn delete_cookie(&self, name: &str, url: &str) -> BridgeResult<()>;
    fn delete_all_cookies(&self) -> BridgeResult<()>;
    fn local_storage(&self) -> BridgeResult<Vec<StorageEntry>>;
    fn set_local_storage(&self, entries: &[StorageEntry]) -> BridgeResult<()>;
    fn clear_local_storage(&self) -> BridgeResult<()>;
    fn session_storage(&self) -> BridgeResult<Vec<StorageEntry>>;
    fn clear_session_storage(&self) -> BridgeResult<()>;
}
```

---

## 12. DialogPort — JavaScript Dialog Handling

```rust
pub trait DialogPort: Send + Sync {
    fn next(&self) -> BridgeResult<Option<DialogInfo>>;
    fn accept(&self, prompt_text: Option<&str>) -> BridgeResult<()>;
    fn dismiss(&self) -> BridgeResult<()>;
}
```

---

## 13. DownloadPort — File Download Management

```rust
pub trait DownloadPort: Send + Sync {
    fn downloads(&self) -> Vec<DownloadInfo>;
    fn cancel_download(&self, id: &str) -> BridgeResult<()>;
    fn set_download_path(&self, path: PathBuf) -> BridgeResult<()>;
    fn download_path(&self) -> PathBuf;
    fn wait_for_completion(&self, timeout: Duration) -> BridgeResult<Vec<DownloadInfo>>;
}
```

---

## 14. Design Rules

### 14.1 No CDP Types in Traits

No `serde_json::Value`, no CDP-specific enums, no raw protocol types in trait method signatures. All parameters and return values are defined in `browseros-bridge` using BrowserOS canonical types.

### 14.2 Error Handling

All fallible methods return `BridgeResult<T>` (i.e., `Result<T, BridgeError>`). CDP protocol errors are translated to `BridgeError` variants.

### 14.3 Thread Safety

All trait objects must be usable from any thread (`Send + Sync`). Internal CDP session state is behind `Arc<Mutex<...>>`.

### 14.4 Object Identity

Each `PagePort`, `FramePort`, `ElementPort` has a stable ID (`PageId`, `FrameId`, `ElementId`). Two trait objects with the same ID refer to the same underlying resource. IDs can be compared without trait object pointer equality.

### 14.5 Stale References

An `ElementPort` becomes stale when the DOM element is removed. Subsequent method calls return `Err(BridgeError::ElementStale(...))`. The caller must re-locate the element. This is a deliberate design choice — automatic re-querying introduces hidden state and makes debugging harder.
