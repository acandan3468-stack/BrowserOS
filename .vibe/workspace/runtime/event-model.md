# Event Model — Phase 2 Runtime Events

**Principle:** Every significant browser state change produces a domain event on the EventBus. Events are typed, structured, and integrate with the existing `Event` trait + `EventMetadata` from `browseros-types`.

---

## 1. Event Categories

| Category | Prefix | Purpose |
|----------|--------|---------|
| `Domain` | `browser.*` | Browser lifecycle events |
| `Domain` | `session.*` | Session lifecycle events |
| `Domain` | `page.*` | Page lifecycle and navigation events |
| `Domain` | `frame.*` | Frame attach/detach events |
| `Domain` | `dom.*` | DOM mutation and element events |
| `Domain` | `network.*` | Network request/response events |
| `Domain` | `input.*` | Input dispatching events |
| `Domain` | `dialog.*` | JavaScript dialog events |
| `Domain` | `download.*` | File download events |
| `Domain` | `storage.*` | Cookie/storage change events |
| `Domain` | `plugin.*` | Plugin lifecycle events |
| `System` | `runtime.*` | Runtime configuration and error events |

---

## 2. Complete Event Catalog

### 2.1 Browser Events (`browser.*`)

```rust
// ——— Emitted when a browser process starts ———
pub struct BrowserStarted {
    pub metadata: EventMetadata,
    pub browser_id: BrowserId,
    pub version: String,
    pub executable: String,
    pub ws_endpoint: String,
}
// kind: "browser.started"

// ——— Emitted when a browser process closes ———
pub struct BrowserClosed {
    pub metadata: EventMetadata,
    pub browser_id: BrowserId,
    pub exit_code: Option<i32>,
    pub reason: String,
}
// kind: "browser.closed"

// ——— Emitted when browser crashes ———
pub struct BrowserCrashed {
    pub metadata: EventMetadata,
    pub browser_id: BrowserId,
    pub crash_reason: String,
    pub dump_path: Option<String>,
}
// kind: "browser.crashed"

// ——— Emitted when browser disconnects unexpectedly ———
pub struct BrowserDisconnected {
    pub metadata: EventMetadata,
    pub browser_id: BrowserId,
    pub last_known_state: String,
}
// kind: "browser.disconnected"
```

### 2.2 Session Events (`session.*`)

```rust
// ——— Emitted when a new browser session (context) is created ———
pub struct SessionCreated {
    pub metadata: EventMetadata,
    pub session_id: SessionId,
    pub browser_id: BrowserId,
    pub incognito: bool,
    pub user_agent: Option<String>,
}
// kind: "session.created"

// ——— Emitted when a session is closed ———
pub struct SessionClosed {
    pub metadata: EventMetadata,
    pub session_id: SessionId,
    pub browser_id: BrowserId,
    pub page_count: u32,
}
// kind: "session.closed"
```

### 2.3 Page Events (`page.*`)

```rust
// ——— Emitted when a new page (tab) is created ———
pub struct PageCreated {
    pub metadata: EventMetadata,
    pub page_id: PageId,
    pub session_id: SessionId,
    pub url: String,
    pub about_blank: bool,
}
// kind: "page.created"

// ——— Emitted when a page is closed ———
pub struct PageClosed {
    pub metadata: EventMetadata,
    pub page_id: PageId,
    pub session_id: SessionId,
}
// kind: "page.closed"

// ——— Emitted when page navigation starts ———
pub struct NavigationStarted {
    pub metadata: EventMetadata,
    pub page_id: PageId,
    pub url: String,
    pub navigation_id: NavigationId,
}
// kind: "navigation.started"

// ——— Emitted when navigation commits (HTTP response received) ———
pub struct NavigationCommitted {
    pub metadata: EventMetadata,
    pub page_id: PageId,
    pub url: String,
    pub navigation_id: NavigationId,
    pub status_code: u16,
    pub is_same_document: bool,
}
// kind: "navigation.committed"

// ——— Emitted when navigation finishes (page fully loaded) ———
pub struct NavigationFinished {
    pub metadata: EventMetadata,
    pub page_id: PageId,
    pub url: String,
    pub navigation_id: NavigationId,
    pub status_code: u16,
    pub load_time_ms: u64,
    pub dom_content_loaded_ms: u64,
    pub success: bool,
    pub error_text: Option<String>,
}
// kind: "navigation.finished"

// ——— Emitted when page title changes ———
pub struct TitleChanged {
    pub metadata: EventMetadata,
    pub page_id: PageId,
    pub title: String,
}
// kind: "page.title_changed"

// ——— Emitted when page URL changes (not via navigation) ———
pub struct PageUrlChanged {
    pub metadata: EventMetadata,
    pub page_id: PageId,
    pub url: String,
}
// kind: "page.url_changed"

// ——— Emitted when page load state changes ———
pub struct PageLoadState {
    pub metadata: EventMetadata,
    pub page_id: PageId,
    pub state: LoadState,
}
pub enum LoadState { Loading, DomContentLoaded, Loaded, NetworkAlmostIdle, NetworkIdle }
// kind: "page.load_state"

// ——— Emitted when a new target (page, worker, etc.) is created ———
pub struct TargetCreated {
    pub metadata: EventMetadata,
    pub target_id: String,
    pub target_type: TargetType,
    pub url: String,
}
pub enum TargetType { Page, BackgroundPage, ServiceWorker, SharedWorker, Other }
// kind: "target.created"

// ——— Emitted when a target is destroyed ———
pub struct TargetDestroyed {
    pub metadata: EventMetadata,
    pub target_id: String,
    pub target_type: TargetType,
}
// kind: "target.destroyed"
```

### 2.4 Frame Events (`frame.*`)

```rust
// ——— Emitted when a frame is attached (iframe loaded) ———
pub struct FrameAttached {
    pub metadata: EventMetadata,
    pub frame_id: FrameId,
    pub page_id: PageId,
    pub parent_frame_id: Option<FrameId>,
    pub url: String,
}
// kind: "frame.attached"

// ——— Emitted when a frame navigation starts ———
pub struct FrameNavigationStarted {
    pub metadata: EventMetadata,
    pub frame_id: FrameId,
    pub page_id: PageId,
    pub url: String,
}
// kind: "frame.navigation.started"

// ——— Emitted when a frame navigation finishes ———
pub struct FrameNavigationFinished {
    pub metadata: EventMetadata,
    pub frame_id: FrameId,
    pub page_id: PageId,
    pub url: String,
    pub success: bool,
}
// kind: "frame.navigation.finished"

// ——— Emitted when a frame is detached ———
pub struct FrameDetached {
    pub metadata: EventMetadata,
    pub frame_id: FrameId,
    pub page_id: PageId,
}
// kind: "frame.detached"
```

### 2.5 DOM Events (`dom.*`)

```rust
// ——— Emitted when a new element appears in the DOM ———
pub struct ElementAttached {
    pub metadata: EventMetadata,
    pub page_id: PageId,
    pub frame_id: FrameId,
    pub node_id: NodeId,
    pub tag_name: String,
    pub attributes: HashMap<String, String>,
    pub parent_node_id: Option<NodeId>,
}
// kind: "dom.element_attached"

// ——— Emitted when an element is removed from the DOM ———
pub struct ElementDetached {
    pub metadata: EventMetadata,
    pub page_id: PageId,
    pub node_id: NodeId,
}
// kind: "dom.element_detached"

// ——— Emitted when an element's attributes change ———
pub struct AttributeModified {
    pub metadata: EventMetadata,
    pub page_id: PageId,
    pub node_id: NodeId,
    pub name: String,
    pub value: String,
}
// kind: "dom.attribute_modified"

// ——— Emitted when an element's attribute is removed ———
pub struct AttributeRemoved {
    pub metadata: EventMetadata,
    pub page_id: PageId,
    pub node_id: NodeId,
    pub name: String,
}
// kind: "dom.attribute_removed"

// ——— Emitted when character data changes ———
pub struct CharacterDataModified {
    pub metadata: EventMetadata,
    pub page_id: PageId,
    pub node_id: NodeId,
    pub new_value: String,
}
// kind: "dom.character_data_modified"

// ——— Emitted when child node count changes ———
pub struct ChildNodeCountUpdated {
    pub metadata: EventMetadata,
    pub page_id: PageId,
    pub node_id: NodeId,
    pub child_node_count: u32,
}
// kind: "dom.child_node_count_updated"

// ——— Emitted when a shadow root is attached ———
pub struct ShadowRootAttached {
    pub metadata: EventMetadata,
    pub page_id: PageId,
    pub host_node_id: NodeId,
    pub shadow_root_type: String,  // "open" or "closed"
}
// kind: "dom.shadow_root_attached"

// ——— Emitted when document is updated ———
pub struct DocumentUpdated {
    pub metadata: EventMetadata,
    pub page_id: PageId,
}
// kind: "dom.document_updated"
```

### 2.6 Network Events (`network.*`)

```rust
// ——— Emitted when a network request is initiated ———
pub struct RequestStarted {
    pub metadata: EventMetadata,
    pub page_id: PageId,
    pub request_id: String,
    pub url: String,
    pub method: String,
    pub headers: HashMap<String, String>,
    pub post_data: Option<String>,
    pub resource_type: String,
    pub frame_id: FrameId,
    pub timestamp: f64,
    pub wall_time: DateTime<Utc>,
}
// kind: "network.request_started"

// ——— Emitted when HTTP response headers are received ———
pub struct ResponseReceived {
    pub metadata: EventMetadata,
    pub page_id: PageId,
    pub request_id: String,
    pub url: String,
    pub status: u16,
    pub status_text: String,
    pub headers: HashMap<String, String>,
    pub mime_type: String,
    pub remote_address: Option<String>,
    pub timing: TimingInfo,
    pub response_time: f64,
}
// kind: "network.response_received"

// ——— Emitted when response body data is received ———
pub struct ResponseDataReceived {
    pub metadata: EventMetadata,
    pub page_id: PageId,
    pub request_id: String,
    pub data_length: u64,
    pub encoded_data_length: u64,
}
// kind: "network.data_received"

// ——— Emitted when a network request finishes ———
pub struct RequestFinished {
    pub metadata: EventMetadata,
    pub page_id: PageId,
    pub request_id: String,
    pub url: String,
    pub total_received: u64,
    pub duration_ms: f64,
    pub encoded_body_length: u64,
    pub decoded_body_length: u64,
    pub success: bool,
}
// kind: "network.request_finished"

// ——— Emitted when a network request fails ———
pub struct RequestFailed {
    pub metadata: EventMetadata,
    pub page_id: PageId,
    pub request_id: String,
    pub url: String,
    pub error_text: String,
    pub blocked_reason: Option<String>,
    pub cancelled: bool,
}
// kind: "network.request_failed"

// ——— Emitted when a WebSocket is created ———
pub struct WebSocketCreated {
    pub metadata: EventMetadata,
    pub page_id: PageId,
    pub request_id: String,
    pub url: String,
}
// kind: "network.web_socket_created"

// ——— Emitted on WebSocket message ———
pub struct WebSocketMessageSent {
    pub metadata: EventMetadata,
    pub page_id: PageId,
    pub request_id: String,
    pub timestamp: f64,
    pub data: String,
}
// kind: "network.web_socket_message_sent"

pub struct WebSocketMessageReceived {
    pub metadata: EventMetadata,
    pub page_id: PageId,
    pub request_id: String,
    pub timestamp: f64,
    pub data: String,
}
// kind: "network.web_socket_message_received"

// ——— Emitted when WebSocket closes ———
pub struct WebSocketClosed {
    pub metadata: EventMetadata,
    pub page_id: PageId,
    pub request_id: String,
    pub timestamp: f64,
    pub code: u16,
    pub reason: String,
}
// kind: "network.web_socket_closed"

// ——— Emitted when a request is intercepted ———
pub struct RequestIntercepted {
    pub metadata: EventMetadata,
    pub page_id: PageId,
    pub interception_id: String,
    pub request: RequestInfo,
    pub resource_type: String,
    pub frame_id: FrameId,
}
// kind: "network.request_intercepted"

// ——— Emitted when auth is needed ———
pub struct AuthRequired {
    pub metadata: EventMetadata,
    pub page_id: PageId,
    pub request_id: String,
    pub url: String,
    pub auth_challenge_source: String,
}
// kind: "network.auth_required"

// ——— Emitted when network conditions change ———
pub struct NetworkConditionsChanged {
    pub metadata: EventMetadata,
    pub page_id: PageId,
    pub offline: bool,
    pub latency_ms: Option<u64>,
    pub download_throughput: Option<f64>,
    pub upload_throughput: Option<f64>,
}
// kind: "network.conditions_changed"
```

### 2.7 Input Events (`input.*`)

```rust
// ——— Emitted when an input action is dispatched ———
pub struct InputDispatched {
    pub metadata: EventMetadata,
    pub page_id: PageId,
    pub input_type: String,  // "click", "type", "keypress", etc.
    pub target_selector: Option<String>,
    pub value: Option<String>,
    pub success: bool,
    pub duration_ms: f64,
}
// kind: "input.dispatched"

// ——— Emitted when a file chooser is opened ———
pub struct FileChooserOpened {
    pub metadata: EventMetadata,
    pub page_id: PageId,
    pub frame_id: FrameId,
    pub mode: String,     // "selectSingle", "selectMultiple"
    pub accept_types: Vec<String>,
}
// kind: "input.file_chooser_opened"
```

### 2.8 Dialog Events (`dialog.*`)

```rust
// ——— Emitted when a JavaScript dialog opens ———
pub struct DialogOpened {
    pub metadata: EventMetadata,
    pub page_id: PageId,
    pub dialog_type: String,      // "alert", "confirm", "prompt", "beforeunload"
    pub message: String,
    pub default_value: Option<String>,
    pub has_expected_handler: bool,
}
// kind: "dialog.opened"

// ——— Emitted when a dialog is closed ———
pub struct DialogClosed {
    pub metadata: EventMetadata,
    pub page_id: PageId,
    pub dialog_type: String,
    pub accepted: bool,
    pub prompt_text: Option<String>,
}
// kind: "dialog.closed"
```

### 2.9 Download Events (`download.*`)

```rust
// ——— Emitted when a download begins ———
pub struct DownloadStarted {
    pub metadata: EventMetadata,
    pub page_id: PageId,
    pub download_id: String,
    pub url: String,
    pub suggested_filename: String,
    pub mime_type: String,
    pub total_bytes: f64,
}
// kind: "download.started"

// ——— Emitted on download progress ———
pub struct DownloadProgress {
    pub metadata: EventMetadata,
    pub page_id: PageId,
    pub download_id: String,
    pub received_bytes: f64,
    pub total_bytes: f64,
}
// kind: "download.progress"

// ——— Emitted when a download finishes ———
pub struct DownloadFinished {
    pub metadata: EventMetadata,
    pub page_id: PageId,
    pub download_id: String,
    pub file_path: PathBuf,
    pub artifact_id: ArtifactId,
    pub total_bytes: u64,
    pub duration_ms: f64,
}
// kind: "download.finished"

// ——— Emitted when a download fails ———
pub struct DownloadFailed {
    pub metadata: EventMetadata,
    pub page_id: PageId,
    pub download_id: String,
    pub error: String,
}
// kind: "download.failed"
```

### 2.10 Storage Events (`storage.*`)

```rust
// ——— Emitted when cookies change ———
pub struct CookieAdded {
    pub metadata: EventMetadata,
    pub page_id: PageId,
    pub cookie: Cookie,
}
// kind: "storage.cookie_added"

pub struct CookieRemoved {
    pub metadata: EventMetadata,
    pub page_id: PageId,
    pub cookie_name: String,
    pub cookie_url: String,
}
// kind: "storage.cookie_removed"

// ——— Emitted when storage is cleared ———
pub struct StorageCleared {
    pub metadata: EventMetadata,
    pub page_id: PageId,
    pub storage_type: String,  // "local", "session", "all"
}
// kind: "storage.cleared"
```

### 2.11 Plugin Events (`plugin.*`)

```rust
// ——— Emitted when a plugin is registered ———
pub struct PluginRegistered {
    pub metadata: EventMetadata,
    pub plugin_id: PluginId,
    pub plugin_name: String,
    pub plugin_version: SemVer,
    pub capabilities: Vec<CapabilityId>,
}
// kind: "plugin.registered"

// ——— Emitted when a plugin is unregistered ———
pub struct PluginUnregistered {
    pub metadata: EventMetadata,
    pub plugin_id: PluginId,
    pub plugin_name: String,
}
// kind: "plugin.unregistered"

// ——— Emitted on plugin state change ———
pub struct PluginStateChanged {
    pub metadata: EventMetadata,
    pub plugin_id: PluginId,
    pub from_state: PluginState,
    pub to_state: PluginState,
}
pub enum PluginState { Loaded, Initialized, Running, Stopped, Error(String) }
// kind: "plugin.state_changed"

// ——— Emitted when plugin encounters an error ———
pub struct PluginError {
    pub metadata: EventMetadata,
    pub plugin_id: PluginId,
    pub error: String,
    pub recoverable: bool,
}
// kind: "plugin.error"

// ——— Emitted when a capability is registered ———
pub struct CapabilityRegistered {
    pub metadata: EventMetadata,
    pub capability_id: CapabilityId,
    pub plugin_id: Option<PluginId>,
    pub version: SemVer,
}
// kind: "plugin.capability_registered"
```

### 2.12 Runtime Events (`runtime.*`)

```rust
// ——— Emitted on runtime error ———
pub struct RuntimeErrorEvent {
    pub metadata: EventMetadata,
    pub error_kind: String,
    pub message: String,
    pub source_component: String,
    pub recoverable: bool,
}
// kind: "runtime.error"

// ——— Emitted when configuration changes ———
pub struct ConfigChanged {
    pub metadata: EventMetadata,
    pub changed_keys: Vec<String>,
    pub source: String,
}
// kind: "runtime.config_changed"

// ——— Emitted on lifecycle transition ———
pub struct LifecycleTransitionEvent {
    pub metadata: EventMetadata,
    pub component_name: String,
    pub from: LifecycleState,
    pub to: LifecycleState,
}
// kind: "lifecycle.transition" (exists in Phase 1, extended for browser components)
```

---

## 3. Event Integration with browseros-types

Every event struct above implements the `Event` trait:

```rust
impl Event for PageCreated {
    fn kind(&self) -> &'static str { "page.created" }
    fn category(&self) -> EventCategory { EventCategory::Domain }
    fn metadata(&self) -> &EventMetadata { &self.metadata }
    fn as_any(&self) -> &dyn Any { self }
}
```

The `EventMetadata` contains:
```rust
EventMetadata {
    id: EventId::new(),                         // unique, UUID v7
    causation_id: Some(CausationId::from(...)), // what caused this event
    correlation_id: CorrelationId::new(),       // operation tracing
    source: ModuleId::new("browseros-browser"),
    timestamp: Utc::now(),
    content_type: ContentType("application/json"),
}
```

---

## 4. Subscription Patterns

### Pattern 1: React to page creation

```rust
bus.subscribe("page.created", Arc::new(|event: &dyn Event| {
    let page_created = event.as_any().downcast_ref::<PageCreated>().unwrap();
    info!("New page: {} at {}", page_created.page_id, page_created.url);
}));
```

### Pattern 2: Wait for navigation completion

```rust
// Agent sends navigate command, then waits for NavigationFinished
bus.subscribe("navigation.finished", Arc::new(move |event: &dyn Event| {
    let nav = event.as_any().downcast_ref::<NavigationFinished>().unwrap();
    if nav.page_id == target_page_id {
        // navigation completed — proceed
    }
}));
```

### Pattern 3: Monitor network activity

```rust
bus.subscribe("network.request_failed", Arc::new(|event: &dyn Event| {
    let failed = event.as_any().downcast_ref::<RequestFailed>().unwrap();
    warn!("Request failed: {} — {}", failed.url, failed.error_text);
}));
```

---

## 5. Event Emission Design Rules

1. **One event per state change.** A navigation produces: `NavigationStarted` → `NavigationCommitted` → `NavigationFinished`. Not a single `PageLoaded` event.

2. **Events are fire-and-forget.** Subscribers run synchronously in the dispatcher thread. Slow subscribers block subsequent events.

3. **No guaranteed delivery order across event kinds.** `ResponseReceived` may arrive before `NavigationCommitted`. Agents must handle temporal ambiguity.

4. **Correlation IDs connect events.** A navigation command's `CorrelationId` is propagated to all emitted events, enabling end-to-end tracing.

5. **Events are not commands.** Events describe what happened. Commands are API calls on `PagePort`, `BrowserPort`, etc.
