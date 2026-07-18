# Phase 2 — Crate Map

---

## 1. `browseros-bridge` (NEW)

**Responsibility:** Pure trait and type definitions for the browser abstraction layer. No protocol implementation. No CDP dependency. No runtime dependency. This is the **contract layer** between BrowserOS and any browser automation protocol.

**Dependencies:** `browseros-types`, `serde`, `thiserror`

**Public API:**

```rust
// ——— Session & Tab ———
pub struct SessionConfig { ... }
pub struct PageDescriptor { id: PageId, url: String, title: String, ... }
pub struct FrameDescriptor { id: FrameId, parent_id: Option<FrameId>, url: String, ... }

pub trait SessionPort: Send + Sync {
    fn pages(&self) -> Vec<Box<dyn PagePort>>;
    fn create_page(&self) -> Result<Box<dyn PagePort>>;
    fn close_page(&self, page_id: &PageId) -> Result<()>;
    fn activate_page(&self, page_id: &PageId) -> Result<()>;
    fn close(&self) -> Result<()>;
    fn config(&self) -> &SessionConfig;
}

// ——— Browser ———
pub struct BrowserInfo { executable: String, version: String, user_data_dir: PathBuf }

pub trait BrowserPort: Send + Sync {
    fn info(&self) -> BrowserInfo;
    fn launch(&self, options: LaunchOptions) -> Result<()>;
    fn create_session(&self, config: SessionConfig) -> Result<Box<dyn SessionPort>>;
    fn sessions(&self) -> Vec<Box<dyn SessionPort>>;
    fn close(&self) -> Result<()>;
    fn kill(&self) -> Result<()>;
    fn is_alive(&self) -> bool;
}

// ——— Page ———
pub struct NavigationState { url: String, status: NavigationStatus, ... }
pub enum NavigationStatus { Started, Committed, Finished(Bool), Failed(String) }
pub struct ScreenshotOptions { format, quality, full_page, clip, ... }
pub struct PdfOptions { scale, print_background, landscape, format, margin, ... }
pub struct JsResult<T> { value: T, exception_details: Option<ExceptionDetails> }
pub struct FileChooserOptions { accept_types, multi_select, ... }

pub trait PagePort: Send + Sync {
    fn id(&self) -> PageId;
    fn url(&self) -> String;
    fn title(&self) -> String;
    fn navigate(&self, url: &str) -> Result<NavigationState>;
    fn reload(&self) -> Result<NavigationState>;
    fn go_back(&self) -> Result<NavigationState>;
    fn go_forward(&self) -> Result<NavigationState>;
    fn evaluate(&self, script: &str, arg: Option<&Value>) -> Result<JsResult>;
    fn screenshot(&self, options: ScreenshotOptions) -> Result<Vec<u8>>;
    fn pdf(&self, options: PdfOptions) -> Result<Vec<u8>>;
    fn content(&self) -> Result<String>;
    fn set_content(&self, html: &str) -> Result<()>;
    fn close(&self) -> Result<()>;
    fn frames(&self) -> Vec<Box<dyn FramePort>>;
}

// ——— Frame ———
pub trait FramePort: Send + Sync {
    fn id(&self) -> FrameId;
    fn url(&self) -> String;
    fn title(&self) -> String;
    fn parent_id(&self) -> Option<FrameId>;
    fn evaluate(&self, script: &str, arg: Option<&Value>) -> Result<JsResult>;
    fn content(&self) -> Result<String>;
    fn set_content(&self, html: &str) -> Result<()>;
    fn page(&self) -> Box<dyn PagePort>;
}

// ——— DOM ———
pub enum NodeType { Element, Text, Document, DocumentFragment, ... }
pub struct NodeInfo { node_id: NodeId, node_type: NodeType, tag_name: String, ... }
pub struct BoxModel { x, y, width, height, padding, margin, border }

pub trait LocatorEngine: Send + Sync {
    fn query_selector(&self, selector: &str) -> Result<Option<Box<dyn ElementPort>>>;
    fn query_selector_all(&self, selector: &str) -> Result<Vec<Box<dyn ElementPort>>>;
    fn query_by_text(&self, text: &str, exact: bool) -> Result<Vec<Box<dyn ElementPort>>>;
    fn query_by_xpath(&self, expression: &str) -> Result<Vec<Box<dyn ElementPort>>>;
}

pub trait ElementPort: Send + Sync {
    fn id(&self) -> ElementId;
    fn tag_name(&self) -> String;
    fn text_content(&self) -> Result<String>;
    fn get_attribute(&self, name: &str) -> Result<Option<String>>;
    fn set_attribute(&self, name: &str, value: &str) -> Result<()>;
    fn bounding_box(&self) -> Result<Option<BoxModel>>;
    fn focus(&self) -> Result<()>;
    fn hover(&self) -> Result<()>;
    fn scroll_into_view(&self) -> Result<()>;
    fn is_visible(&self) -> Result<bool>;
    fn is_enabled(&self) -> Result<bool>;
    fn snapshot(&self) -> Result<NodeInfo>;
    fn query_selector(&self, selector: &str) -> Result<Option<Box<dyn ElementPort>>>;
    fn query_selector_all(&self, selector: &str) -> Result<Vec<Box<dyn ElementPort>>>;
}

// ——— Network ———
pub struct RequestInfo { url, method, headers, post_data, resource_type, ... }
pub struct ResponseInfo { url, status, status_text, headers, remote_address, ... }
pub struct WebSocketInfo { url, messages: Vec<WebSocketMessage> }
pub struct InterceptionRule { url_pattern, resource_types, action: InterceptionAction }
pub enum InterceptionAction { Block, Continue, Respond { status, headers, body } }
pub struct NetworkConditions { offline, latency, download_throughput, upload_throughput }

pub trait NetworkPort: Send + Sync {
    fn set_offline(&self, offline: bool) -> Result<()>;
    fn set_conditions(&self, conditions: NetworkConditions) -> Result<()>;
    fn add_interception_rule(&self, rule: InterceptionRule) -> Result<InterceptionHandle>;
    fn remove_interception_rule(&self, handle: &InterceptionHandle) -> Result<()>;
    fn clear_cache(&self) -> Result<()>;
    fn clear_cookies(&self) -> Result<()>;
}

// ——— Input ———
pub enum MouseAction { Click { button, count }, DblClick, Down, Up, Move, Wheel }
pub enum KeyboardAction { Press, Down, Up, Type { text } }
pub enum TouchAction { Tap, Press, Move, Release, Cancel }
pub struct InputConfig { delay, timeout, ... }

pub trait InputPort: Send + Sync {
    fn click(&self, element: &dyn ElementPort, options: ClickOptions) -> Result<()>;
    fn dblclick(&self, element: &dyn ElementPort) -> Result<()>;
    fn type_text(&self, element: &dyn ElementPort, text: &str) -> Result<()>;
    fn press_key(&self, key: &str) -> Result<()>;
    fn hover(&self, element: &dyn ElementPort) -> Result<()>;
    fn scroll(&self, delta_x: f64, delta_y: f64) -> Result<()>;
    fn drag_and_drop(&self, source: &dyn ElementPort, target: &dyn ElementPort) -> Result<()>;
    fn upload_file(&self, element: &dyn ElementPort, paths: &[PathBuf]) -> Result<()>;
}

// ——— Storage ———
pub struct Cookie { name, value, domain, path, secure, http_only, same_site, expires, ... }
pub struct StorageEntry { key, value }

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

// ——— Dialog ———
pub enum DialogType { Alert, Confirm, Prompt, BeforeUnload }
pub struct DialogInfo { type: DialogType, message: String, default_value: Option<String> }

pub trait DialogPort: Send + Sync {
    fn accept(&self, prompt_text: Option<&str>) -> Result<()>;
    fn dismiss(&self) -> Result<()>;
    fn default_timeout(&self) -> Duration;
    fn set_default_timeout(&self, timeout: Duration);
}

// ——— Download ———
pub struct DownloadInfo { url, suggested_filename, file_path, mime_type, total_bytes, received_bytes, state: DownloadState }
pub enum DownloadState { InProgress, Completed, Cancelled, Failed(String) }

pub trait DownloadPort: Send + Sync {
    fn downloads(&self) -> Vec<DownloadInfo>;
    fn cancel_download(&self, id: &str) -> Result<()>;
    fn download_path(&self) -> PathBuf;
    fn set_download_path(&self, path: PathBuf) -> Result<()>;
    fn wait_for_completion(&self, timeout: Duration) -> Result<Vec<DownloadInfo>>;
}

// ——— Artifact ———
pub trait ArtifactPort: Send + Sync {
    fn store(&self, name: &str, data: Vec<u8>, artifact_type: ArtifactType) -> Result<ArtifactId>;
    fn retrieve(&self, id: &ArtifactId) -> Result<Vec<u8>>;
    fn list(&self, filter: Option<ArtifactFilter>) -> Vec<ArtifactMeta>;
    fn delete(&self, id: &ArtifactId) -> Result<()>;
    fn storage_path(&self) -> PathBuf;
}

// ——— Locator (High-Level, in bridge because it's in the contract) ———
pub enum LocatorStrategy {
    Css(String),
    XPath(String),
    Text(String, bool),     // text, exact_match
    Role(String),            // ARIA role
    TestId(String),
    Placeholder(String),
    Label(String),
    AltText(String),
    Title(String),
    Nested(Box<LocatorStrategy>, Box<LocatorStrategy>),  // parent → child
}

pub trait LocatorPort: Send + Sync {
    fn locate(&self, strategy: &LocatorStrategy) -> Result<Option<Box<dyn ElementPort>>>;
    fn locate_all(&self, strategy: &LocatorStrategy) -> Result<Vec<Box<dyn ElementPort>>>;
    fn wait_for(&self, strategy: &LocatorStrategy, timeout: Duration) -> Result<Box<dyn ElementPort>>;
}
```

**Identifiers (defined in `browseros-bridge::identifiers`):**

```rust
pub struct PageId(Uuid);
pub struct FrameId(Uuid);
pub struct ElementId(u64);
pub struct NodeId(u64);
pub struct InterceptionHandle(Uuid);
pub struct ArtifactId(Uuid);
pub struct SessionId(Uuid);
```

---

## 2. `browseros-cdp` (NEW)

**Responsibility:** Chrome DevTools Protocol transport, session management, and trait implementation. The **only** crate that depends on CDP-specific concepts.

**Dependencies:** `browseros-types`, `browseros-bridge`, `serde`, `serde_json`, `websocket` (or `tungstenite`), `url`, `thiserror`

**Public API:**

```rust
// ——— Connection ———
pub struct CdpConnection { ... }
impl CdpConnection {
    pub fn connect(ws_endpoint: &str) -> Result<Self>;
    pub fn close(&self) -> Result<()>;
}

// ——— Session ———
pub struct CdpSession { ... }
impl CdpSession {
    pub fn new(conn: &CdpConnection, target_id: &str) -> Result<Self>;
    pub fn send<T: DeserializeOwned>(&self, command: &str, params: Value) -> Result<T>;
    pub fn on_event<F>(&self, callback: F) where F: Fn(&str, &Value) + Send + 'static;
}

// ——— Trait Implementations ———
impl SessionPort for CdpSession { ... }
impl PagePort for CdpPage { ... }
impl FramePort for CdpFrame { ... }
impl ElementPort for CdpElement { ... }
impl NetworkPort for CdpNetworkManager { ... }
impl InputPort for CdpInputSimulator { ... }
impl StoragePort for CdpStorageManager { ... }
impl DialogPort for CdpDialogHandler { ... }
impl DownloadPort for CdpDownloadManager { ... }
impl LocatorPort for CdpLocatorEngine { ... }
impl LocatorEngine for CdpLocatorEngine { ... }  // lower-level

// ——— Target Management ———
impl CdpConnection {
    pub fn list_targets(&self) -> Result<Vec<TargetInfo>>;
    pub fn attach_to_target(&self, target_id: &str) -> Result<CdpSession>;
    pub fn create_target(&self, url: &str) -> Result<CdpSession>;
    pub fn close_target(&self, target_id: &str) -> Result<()>;
}

// ——— Browser Process Management ———
pub struct CdpBrowserProcess { ... }
impl CdpBrowserProcess {
    pub fn launch(options: LaunchOptions) -> Result<Self>;
    pub fn endpoint(&self) -> &str;  // ws://host:port/devtools/browser/...
    pub fn close(&self) -> Result<()>;
    pub fn kill(&self) -> Result<()>;
}
impl BrowserPort for CdpBrowserProcess { ... }
```

**Internal modules** (not public):
- `commands.rs` — CDP method name constants and strongly-typed parameter builders
- `events.rs` — CDP event deserialization and dispatching
- `transport.rs` — WebSocket I/O, message framing
- `target.rs` — Target discovery and attachment
- `browser.rs` — Browser-level CDP commands (Browser.*, Target.*)
- `page.rs` — Page-level CDP commands (Page.*, Runtime.*, DOM.*)
- `network.rs` — Network-level CDP commands (Network.*, Fetch.*)
- `input.rs` — Input CDP commands (Input.*)
- `dom.rs` — DOM CDP commands (DOM.*, CSS.*)
- `storage.rs` — Storage CDP commands (Storage.*, Page.getCookies, etc.)

---

## 3. `browseros-browser` (NEW)

**Responsibility:** Browser lifecycle management, session management, tab/page lifecycle. Orchestrates the bridge layer and emits domain events.

**Dependencies:** `browseros-types`, `browseros-bridge`, `browseros-event-bus`, `browseros-observability`, `browseros-config`

**Public API:**

```rust
pub struct BrowserManager { ... }
impl BrowserManager {
    pub fn new(bus: Arc<EventBus>, config: &BrowserConfig) -> Self;
    pub fn launch(&self, options: LaunchOptions) -> Result<BrowserHandle>;
    pub fn connect(&self, endpoint: &str) -> Result<BrowserHandle>;
    pub fn browsers(&self) -> Vec<BrowserHandle>;
    pub fn default_browser(&self) -> Option<BrowserHandle>;
}

#[derive(Clone)]
pub struct BrowserHandle { inner: Arc<dyn BrowserPort> }
impl BrowserHandle {
    pub fn info(&self) -> BrowserInfo;
    pub fn new_session(&self, config: SessionConfig) -> Result<SessionHandle>;
    pub fn sessions(&self) -> Vec<SessionHandle>;
    pub fn close(&self) -> Result<()>;
}

#[derive(Clone)]
pub struct SessionHandle { inner: Arc<dyn SessionPort> }
impl SessionHandle {
    pub fn pages(&self) -> Vec<PageHandle>;
    pub fn new_page(&self) -> Result<PageHandle>;
    pub fn close(&self) -> Result<()>;
}

#[derive(Clone)]
pub struct PageHandle { inner: Arc<dyn PagePort> }
impl PageHandle {
    pub fn id(&self) -> PageId;
    pub fn url(&self) -> String;
    pub fn title(&self) -> String;
    pub fn navigate(&self, url: &str) -> Result<NavigationState>;
    pub fn evaluate(&self, script: &str) -> Result<JsResult>;
    pub fn screenshot(&self, options: ScreenshotOptions) -> Result<Vec<u8>>;
    pub fn pdf(&self, options: PdfOptions) -> Result<Vec<u8>>;
    pub fn locator(&self) -> Box<dyn LocatorPort>;
    pub fn network(&self) -> Box<dyn NetworkPort>;
    pub fn input(&self) -> Box<dyn InputPort>;
    pub fn storage(&self) -> Box<dyn StoragePort>;
    pub fn dialog(&self) -> Box<dyn DialogPort>;
    pub fn download(&self) -> Box<dyn DownloadPort>;
    pub fn frames(&self) -> Vec<FrameHandle>;
    pub fn close(&self) -> Result<()>;
}
```

**Event emission** (via EventBus in background threads):
- `BrowserStarted` / `BrowserClosed`
- `SessionCreated` / `SessionClosed`
- `PageCreated` / `PageClosed`
- `FrameAttached` / `FrameDetached`

---

## 4. `browseros-dom` (NEW)

**Responsibility:** DOM tree modeling, element querying, locator strategies, shadow DOM traversal. Higher-level operations on top of bridge's `ElementPort` and `LocatorEngine`.

**Dependencies:** `browseros-types`, `browseros-bridge`

**Public API:**

```rust
// ——— DOM Snapshot ———
pub struct DomSnapshot { root: NodeSnapshot, timestamp: DateTime<Utc> }
pub struct NodeSnapshot { node_id: NodeId, node_type: NodeType, tag_name: String, attributes: HashMap<String, String>, text: String, children: Vec<NodeSnapshot>, ... }

impl DomSnapshot {
    pub fn root(&self) -> &NodeSnapshot;
    pub fn find_id(&self, id: &str) -> Option<&NodeSnapshot>;
    pub fn find_by_tag(&self, tag: &str) -> Vec<&NodeSnapshot>;
    pub fn find_by_text(&self, text: &str, exact: bool) -> Vec<&NodeSnapshot>;
    pub fn find_by_attr(&self, name: &str, value: &str) -> Vec<&NodeSnapshot>;
}

// ——— Locator Builder ———
pub struct LocatorBuilder { ... }
impl LocatorBuilder {
    pub fn css(selector: &str) -> Self;
    pub fn xpath(expr: &str) -> Self;
    pub fn text(text: &str) -> Self;
    pub fn exact_text(text: &str) -> Self;
    pub fn role(role: &str) -> Self;
    pub fn test_id(id: &str) -> Self;
    pub fn placeholder(text: &str) -> Self;
    pub fn label(text: &str) -> Self;
    pub fn alt_text(text: &str) -> Self;
    pub fn and(self, other: Self) -> Self;      // logical AND
    pub fn or(self, other: Self) -> Self;       // logical OR
    pub fn child(self, parent: Self) -> Self;   // parent > child
    pub fn nth(self, index: usize) -> Self;
    pub fn has_text(self, text: &str) -> Self;
}

// ——— Element Operations ———
pub struct ElementOps { ... }
impl ElementOps {
    pub fn wait_for_element(locator: &LocatorPort, strategy: &LocatorStrategy, timeout: Duration) -> Result<ElementHandle>;
    pub fn wait_for_element_state(element: &ElementPort, state: ElementState, timeout: Duration) -> Result<()>;
    pub fn get_attribute_safe(element: &ElementPort, name: &str) -> Result<Option<String>>;
    pub fn is_matches(element: &ElementPort, selector: &str) -> Result<bool>;
    pub fn get_all_attributes(element: &ElementPort) -> Result<HashMap<String, String>>;
}

// ——— Shadow DOM ———
pub struct ShadowDomSupport { ... }
impl ShadowDomSupport {
    pub fn open_shadow_hosts(page: &PagePort) -> Result<Vec<ElementHandle>>;
    pub fn traverse_shadow_dom(host: &ElementPort) -> Result<Vec<NodeSnapshot>>;
}

pub enum ElementState { Visible, Hidden, Stable, Enabled, Disabled, Editable, Selected }
```

---

## 5. `browseros-page` (NEW)

**Responsibility:** Page-level operations beyond navigation — waiting for conditions, content extraction, dialogs, permissions.

**Dependencies:** `browseros-types`, `browseros-bridge`

**Public API:**

```rust
// ——— Wait Strategies ———
pub enum WaitCondition {
    Navigation(Duration),
    Selector(&'static str, ElementState),
    Url(String),
    Title(String),
    NetworkIdle(Duration),
    Function(&'static str),   // JS predicate
    All(Vec<WaitCondition>),
    Any(Vec<WaitCondition>),
}

pub struct PageWaiter { ... }
impl PageWaiter {
    pub fn new(page: &PagePort, event_bus: &EventBus) -> Self;
    pub fn wait(condition: WaitCondition) -> Result<()>;
    pub fn wait_for_navigation(timeout: Duration) -> Result<NavigationState>;
    pub fn wait_for_selector(selector: &str, state: ElementState, timeout: Duration) -> Result<ElementHandle>;
    pub fn wait_for_url(url_pattern: &str, timeout: Duration) -> Result<String>;
    pub fn wait_for_function(fn_body: &str, timeout: Duration) -> Result<Value>;
}

// ——— Content Extraction ———
pub struct PageContent { url, title, text, html, screenshots_taken, metadata }

pub trait ContentExtractor: Send + Sync {
    fn extract_text(&self, page: &PagePort) -> Result<String>;
    fn extract_html(&self, page: &PagePort) -> Result<String>;
    fn extract_structured(&self, page: &PagePort, schema: &ExtractionSchema) -> Result<Value>;
    fn extract_links(&self, page: &PagePort) -> Result<Vec<LinkInfo>>;
    fn extract_metadata(&self, page: &PagePort) -> Result<HashMap<String, String>>;
}

pub struct ExtractionSchema { fields: Vec<ExtractionField> }
pub struct ExtractionField { name, selector, attribute, transform }

// ——— Dialog Auto-Handler ———
pub enum DialogStrategy { Accept, Dismiss, AcceptWithText(String), AutoDismiss }
pub struct DialogAutoHandler { ... }
impl DialogAutoHandler {
    pub fn new(page: &PagePort, event_bus: &EventBus) -> Self;
    pub fn auto_handle(strategy: DialogStrategy) -> Result<()>;
    pub fn stop_auto_handle() -> Result<()>;
}
```

---

## 6. `browseros-network` (NEW)

**Responsibility:** Network traffic monitoring, request interception, WebSocket message capture.

**Dependencies:** `browseros-types`, `browseros-bridge`

**Public API:**

```rust
// ——— Traffic Capture ———
pub struct NetworkCapture { requests: Vec<RequestRecord>, ... }
pub struct RequestRecord { request: RequestInfo, response: Option<ResponseInfo>, timing: TimingInfo, ... }
pub struct TimingInfo { dns, connect, ssl, send, wait, receive }

impl NetworkCapture {
    pub fn start(page: &PagePort, event_bus: &EventBus) -> Result<Self>;
    pub fn stop(&self) -> Result<Vec<RequestRecord>>;
    pub fn filter(&self, predicate: Box<dyn Fn(&RequestRecord) -> bool>) -> Vec<&RequestRecord>;
    pub fn find_request(url_pattern: &str) -> Vec<&RequestRecord>;
    pub fn clear(&self) -> Result<()>;
}

// ——— Request Pattern Builder ———
pub struct RequestPattern { ... }
impl RequestPattern {
    pub fn url(pattern: &str) -> Self;
    pub fn method(method: &str) -> Self;
    pub fn resource_type(r#type: ResourceType) -> Self;
    pub fn header(name: &str, value: &str) -> Self;
}

// ——— WebSocket Monitor ———
pub struct WebSocketMonitor { ... }
impl WebSocketMonitor {
    pub fn start(page: &PagePort, event_bus: &EventBus) -> Result<Self>;
    pub fn messages(&self) -> Vec<WebSocketMessage>;
    pub fn stop(&self) -> Result<Vec<WebSocketMessage>>;
}

pub enum ResourceType { Document, Stylesheet, Image, Media, Font, Script, XHR, Fetch, WebSocket, Other }
```

---

## 7. `browseros-input` (NEW)

**Responsibility:** Input action sequences, gesture simulation, file upload.

**Dependencies:** `browseros-types`, `browseros-bridge`

**Public API:**

```rust
// ——— Action Sequences ———
pub struct ActionSequence { actions: Vec<Action> }
pub enum Action {
    Mouse(MouseAction, ActionOptions),
    Keyboard(KeyboardAction, ActionOptions),
    Touch(TouchAction, ActionOptions),
    Wait(Duration),
}

pub struct ActionOptions { delay: Duration, steps: u32, relative_to: Option<ElementId> }
pub struct ClickOptions { button: MouseButton, click_count: u32, delay: Duration, force: bool, ... }
pub enum MouseButton { Left, Right, Middle, Back, Forward }

pub struct InputSimulator { ... }
impl InputSimulator {
    pub fn new(page: &PagePort) -> Self;
    pub fn click(element: &ElementPort, options: ClickOptions) -> Result<()>;
    pub fn fill(element: &ElementPort, text: &str) -> Result<()>;
    pub fn type_text(element: &ElementPort, text: &str, delay: Duration) -> Result<()>;
    pub fn select_option(element: &ElementPort, values: &[&str]) -> Result<()>;
    pub fn check(element: &ElementPort) -> Result<()>;
    pub fn uncheck(element: &ElementPort) -> Result<()>;
    pub fn select_all_text(element: &ElementPort) -> Result<()>;
    pub fn press_sequentially(keys: &[&str]) -> Result<()>;
    pub fn run_sequence(sequence: ActionSequence) -> Result<()>;
}

// ——— File Upload ———
pub struct FileChooser { ... }
impl FileChooser {
    pub fn set_files(paths: &[PathBuf]) -> Result<()>;
    pub fn cancel() -> Result<()>;
    pub fn is_multiple() -> bool;
}
```

---

## 8. `browseros-storage` (NEW)

**Responsibility:** Cookie management, web storage read/write/clear.

**Dependencies:** `browseros-types`, `browseros-bridge`

**Public API:**

```rust
pub struct StorageManager { ... }
impl StorageManager {
    pub fn new(page: &PagePort) -> Self;

    // — Cookies —
    pub fn get_cookies(&self) -> Result<Vec<Cookie>>;
    pub fn set_cookie(&self, cookie: Cookie) -> Result<()>;
    pub fn delete_cookie(&self, name: &str, url: &str) -> Result<()>;
    pub fn delete_all_cookies(&self) -> Result<()>;

    // — Local Storage —
    pub fn get_local_storage(&self) -> Result<HashMap<String, String>>;
    pub fn set_local_storage(&self, key: &str, value: &str) -> Result<()>;
    pub fn remove_local_storage(&self, key: &str) -> Result<()>;
    pub fn clear_local_storage(&self) -> Result<()>;

    // — Session Storage —
    pub fn get_session_storage(&self) -> Result<HashMap<String, String>>;
    pub fn set_session_storage(&self, key: &str, value: &str) -> Result<()>;
    pub fn clear_session_storage(&self) -> Result<()>;
}
```

---

## 9. `browseros-artifact` (NEW)

**Responsibility:** Persistent storage for screenshots, PDFs, traces, downloads, and session recordings.

**Dependencies:** `browseros-types`, `browseros-bridge`

**Public API:**

```rust
pub enum ArtifactType { Screenshot, Pdf, Trace, Har, Download, Recording, Snapshot }
pub struct ArtifactMeta { id, name, artifact_type, size, created_at, mime_type, source_url }

pub struct ArtifactStore { ... }
impl ArtifactStore {
    pub fn new(base_path: PathBuf) -> Self;

    // — CRUD —
    pub fn store(&self, name: &str, data: Vec<u8>, r#type: ArtifactType) -> Result<ArtifactId>;
    pub fn retrieve(&self, id: &ArtifactId) -> Result<Vec<u8>>;
    pub fn delete(&self, id: &ArtifactId) -> Result<()>;
    pub fn list(&self, filter: ArtifactFilter) -> Result<Vec<ArtifactMeta>>;

    // — Aliases —
    pub fn store_screenshot(&self, name: &str, data: Vec<u8>) -> Result<ArtifactId>;
    pub fn store_pdf(&self, name: &str, data: Vec<u8>) -> Result<ArtifactId>;
    pub fn store_trace(&self, name: &str, data: Vec<u8>) -> Result<ArtifactId>;
    pub fn store_recording(&self, name: &str, data: Vec<u8>) -> Result<ArtifactId>;

    // — Storage lifecycle —
    pub fn storage_used(&self) -> u64;
    pub fn prune_older_than(&self, age: Duration) -> Result<u64>;
    pub fn prune_to_fit(&self, max_bytes: u64) -> Result<u64>;
}

pub struct ArtifactFilter { types: Option<HashSet<ArtifactType>>, before: Option<DateTime<Utc>>, after: Option<DateTime<Utc>>, name_contains: Option<String> }

// — Download Manager —
pub struct DownloadManager { ... }
impl DownloadManager {
    pub fn new(page: &PagePort, store: &ArtifactStore) -> Self;
    pub fn downloads(&self) -> Vec<DownloadInfo>;
    pub fn wait_for_download(&self, url_pattern: &str, timeout: Duration) -> Result<ArtifactId>;
    pub fn cancel_all(&self) -> Result<()>;
}
```

---

## 10. `browseros-plugin` (NEW)

**Responsibility:** Dynamic plugin loading, capability registration, lifecycle management for external extensions.

**Dependencies:** `browseros-types`, `browseros-bridge`, `browseros-event-bus`, `browseros-observability`, `browseros-config`  
*(Runtime_ is NOT a dep — plugin system uses EventBus and Bridge, composition happens in runtime)*

**Public API:**

```rust
// ——— Plugin Descriptor ———
pub struct PluginDescriptor {
    pub id: PluginId,
    pub name: String,
    pub version: SemVer,
    pub description: String,
    pub capabilities: Vec<CapabilityDefinition>,
    pub required_capabilities: Vec<CapabilityId>,
    pub entry_point: PluginEntryPoint,
}

pub enum PluginEntryPoint {
    Native(Box<dyn Plugin>),
    Wasm(Vec<u8>),
    External(PathBuf),  // shared library
}

// ——— Plugin Trait ———
pub trait Plugin: Send + Sync {
    fn id(&self) -> PluginId;
    fn name(&self) -> &str;
    fn version(&self) -> SemVer;
    fn capabilities(&self) -> Vec<CapabilityDefinition>;
    fn required_capabilities(&self) -> Vec<CapabilityId>;
    fn init(&self, ctx: PluginContext) -> Result<()>;
    fn start(&self) -> Result<()>;
    fn stop(&self) -> Result<()>;
}

#[derive(Clone)]
pub struct PluginContext {
    pub bus: Arc<EventBus>,
    pub logger: Arc<Logger>,
    pub metrics: Arc<MetricsRegistry>,
    pub config: Arc<dyn PluginConfigAccess>,
}

// ——— Plugin Registry ———
pub struct PluginRegistry { ... }
impl PluginRegistry {
    pub fn new(bus: Arc<EventBus>, logger: Arc<Logger>) -> Self;
    pub fn register(&self, plugin: Box<dyn Plugin>) -> Result<PluginId>;
    pub fn unregister(&self, id: &PluginId) -> Result<()>;
    pub fn get(&self, id: &PluginId) -> Option<Arc<dyn Plugin>>;
    pub fn list(&self) -> Vec<PluginDescriptor>;
    pub fn find_by_capability(&self, capability: &CapabilityId) -> Vec<PluginDescriptor>;
    pub fn start_all(&self) -> Result<()>;
    pub fn stop_all(&self) -> Result<()>;
}

// ——— Capability Registry ———
pub struct CapabilityRegistry { ... }
impl CapabilityRegistry {
    pub fn new() -> Self;
    pub fn register(&self, capability: CapabilityDefinition) -> Result<()>;
    pub fn unregister(&self, id: &CapabilityId) -> Result<()>;
    pub fn get(&self, id: &CapabilityId) -> Option<CapabilityDefinition>;
    pub fn list(&self) -> Vec<CapabilityDefinition>;
    pub fn has(&self, id: &CapabilityId) -> bool;
    pub fn requires(&self, ids: &[CapabilityId]) -> Result<()>; // check all exist
}
```

---

## Dependency Summary

```
browseros-types (FROZEN)
  ↑ deps
browseros-bridge
  ↑ deps
browseros-cdp ──────────────────────────────┐
browseros-dom  ─────────────────────────────┤
browseros-page ─────────────────────────────┤
browseros-network ──────────────────────────┤
browseros-input  ───────────────────────────┤
browseros-storage ──────────────────────────┤
browseros-artifact ─────────────────────────┤
browseros-browser ──(also deps: event-bus,──┤
                     observability, config)  │
browseros-plugin ───(also deps: event-bus,───┤
                     observability, config)  │
                                            │
browseros-runtime ──────────────────────────┘ (depends on ALL of the above)
```

**Every Phase 2 crate** depends on `browseros-types` + `browseros-bridge`.  
`browseros-browser` and `browseros-plugin` additionally depend on `browseros-event-bus` and `browseros-observability` (to emit events/logs).  
`browseros-runtime` is the composition root that depends on everything.

**No circular dependencies.** The dependency direction is strictly downward:  
types → bridge → cdp/dom/page/network/input/storage/artifact → browser/plugin → runtime
