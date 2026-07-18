# Network API Design

**Status:** DESIGN — API defined. Not yet implemented.  
**Goal:** Complete public API for `browseros-network`. Every type, method, and ownership model specified.

---

## 1. Core Types

### 1.1 NetworkHandle

**Purpose:** Primary user-facing type for network monitoring, interception, and management on a page. Wraps the bridge `NetworkPort` and `DownloadPort` and adds identity tracking, request correlation, and high-level operations.

**Ownership:** Consumer-owned. `Clone + Send + Sync`. Cloning creates a new handle pointing to the same backend.

**Mutability:** All methods take `&self`. The handle is logically immutable — the underlying browser network state changes via the browser, but the handle itself does not mutate.

**Thread Safety:** `Send + Sync`. Multiple threads can hold handles to the same page's network. All backend operations are serialized through the bridge ports.

```rust
struct NetworkHandle {
    inner: Arc<dyn NetworkPort>,
    download_port: Arc<dyn DownloadPort>,
    storage_port: Option<Arc<dyn StoragePort>>,  // For cookie access
    page_id: PageId,
}
```

**Methods:**

| Method | Signature | Description |
|--------|-----------|-------------|
| `set_offline` | `&self, offline: bool -> NetworkResult<()>` | Enable/disable offline mode |
| `set_conditions` | `&self, conditions: NetworkConditions -> NetworkResult<()>` | Set latency/throttling |
| `add_interception_rule` | `&self, rule: InterceptionRule -> NetworkResult<InterceptionHandle>` | Register interception |
| `remove_interception_rule` | `&self, handle: &InterceptionHandle -> NetworkResult<()>` | Remove interception |
| `clear_cache` | `&self -> NetworkResult<()>` | Clear browser cache |
| `clear_cookies` | `&self -> NetworkResult<()>` | Clear all cookies |
| `cookies` | `&self -> CookieManager` | Get cookie manager (requires storage_port) |
| `downloads` | `&self -> Vec<DownloadInfo>` | All active/completed downloads |
| `cancel_download` | `&self, id: &str -> NetworkResult<()>` | Cancel a download |
| `set_download_path` | `&self, path: PathBuf -> NetworkResult<()>` | Set download directory |
| `download_path` | `&self -> PathBuf` | Current download directory |
| `wait_for_completion` | `&self, timeout: Duration -> NetworkResult<Vec<DownloadInfo>>` | Wait for in-progress downloads |
| `page_id` | `&self -> PageId` | Owning page |
| `backend_port` | `&self -> &dyn NetworkPort` | Escape hatch, `pub(crate)` only |

> Network conditions are write-only through `set_conditions()`. Consumers that need readback should cache the last applied `NetworkConditions` themselves.

### 1.2 RequestId

**Purpose:** Unique identifier for each HTTP request observed on a page. UUID v7.

```rust
// Uses the uuid_id! macro pattern from browseros-types.
// Defined in id.rs within browseros-network, or added to browseros-types.
// Choice: defined in browseros-network/id.rs to keep browseros-types minimal.
struct RequestId(Uuid);
```

**Ownership:** `Clone + Copy + Send + Sync`. Value type.

### 1.3 RequestTracker

**Purpose:** Tracks the state of a single HTTP request from initiation through response. Created when a request starts, updated as response data arrives, finalized when the request completes or fails.

**Ownership:** Internal to the crate. Not exposed publicly. Consumers observe request state through events.

```rust
// Internal, not pub
struct RequestTracker {
    request_id: RequestId,
    page_id: PageId,
    frame_id: Option<FrameId>,
    request: RequestInfo,
    response: Option<ResponseTracker>,
    redirect_chain: Vec<ResponseInfo>,
    start_time: DateTime<Utc>,
    end_time: Option<DateTime<Utc>>,
    state: RequestState,
}
```

### 1.4 ResponseTracker

**Purpose:** Immutable snapshot of a completed HTTP response. Internal type used by `RequestTracker`. Not part of the public API — response data is exposed through event payloads.

```rust
// Internal, not pub
struct ResponseTracker {
    request_id: RequestId,
    response: ResponseInfo,
    body_size: u64,
    timestamp: DateTime<Utc>,
}
```

**Ownership:** Internal. `Clone + Send + Sync`. Immutable after creation.

### 1.5 RequestState

```rust
enum RequestState {
    Initiated,
    Received,
    Completed,
    Failed(String),
    Aborted,
}
```

---

## 2. Interception Types

### 2.1 Re-exported from browseros-bridge

```rust
pub use browseros_bridge::types::{InterceptionRule, InterceptionAction, ResourceType};
pub use browseros_bridge::identifiers::InterceptionHandle;
```

| Type | Origin | Description |
|------|--------|-------------|
| `InterceptionRule` | bridge | Shared: url_pattern, resource_types, action |
| `InterceptionAction` | bridge | Shared: Block, Continue, Respond { status, headers, body } |
| `InterceptionHandle` | bridge | Shared: UUID-based handle for rule removal |
| `ResourceType` | bridge | Shared: Document, Stylesheet, Image, Script, etc. |

No wrapping or newtype — these are consumed and produced directly.

### 2.2 InterceptionBuilder

**Purpose:** Fluent builder for constructing interception rules with URL pattern matching, resource type filtering, and action configuration.

```rust
struct InterceptionBuilder {
    rule: InterceptionRule,
}

impl InterceptionBuilder {
    fn new() -> Self;
    fn url_pattern(mut self, pattern: &str) -> Self;
    fn resource_types(mut self, types: &[ResourceType]) -> Self;
    fn block(self) -> NetworkResult<InterceptionRule>;
    fn continue_request(self) -> NetworkResult<InterceptionRule>;
    fn respond(self, status: u16, headers: HashMap<String, String>, body: Vec<u8>) -> NetworkResult<InterceptionRule>;
}
```

---

## 3. Cookie Management

### 3.1 CookieManager

**Purpose:** Typed API for reading, writing, and deleting cookies. Wraps `Arc<dyn StoragePort>`.

```rust
struct CookieManager {
    inner: Arc<dyn StoragePort>,
}

impl CookieManager {
    fn all(&self) -> NetworkResult<Vec<Cookie>>;
    fn get(&self, name: &str) -> NetworkResult<Option<Cookie>>;
    fn set(&self, cookie: &Cookie) -> NetworkResult<()>;
    fn delete(&self, name: &str, url: &str) -> NetworkResult<()>;
    fn delete_all(&self) -> NetworkResult<()>;
}
```

**Re-exports from bridge:**
```rust
pub use browseros_bridge::types::{Cookie, SameSitePolicy};
```

---

## 4. Condition Emulation

### 4.1 Re-exported from browseros-bridge

```rust
pub use browseros_bridge::types::NetworkConditions;
```

### 4.2 NetworkConditionBuilder

**Purpose:** Fluent builder for `NetworkConditions`.

```rust
struct NetworkConditionBuilder {
    conditions: NetworkConditions,
}

impl NetworkConditionBuilder {
    fn new() -> Self;
    fn offline(mut self) -> Self;
    fn latency(mut self, ms: u64) -> Self;
    fn download_speed(mut self, bps: f64) -> Self;
    fn upload_speed(mut self, bps: f64) -> Self;
    fn build(&self) -> NetworkConditions;
}
```

---

## 5. WebSocket Types

### 5.1 Re-exported from browseros-bridge

```rust
pub use browseros_bridge::types::WebSocketMessage;
```

### 5.2 WebSocketInfo

**Purpose:** Metadata about an active or closed WebSocket connection.

```rust
struct WebSocketInfo {
    url: String,
    frame_id: Option<FrameId>,
    messages: Vec<WebSocketMessage>,
    open_time: DateTime<Utc>,
    close_time: Option<DateTime<Utc>>,
    close_reason: Option<String>,
}
```

---

## 6. Download Types

### 6.1 Re-exported from browseros-bridge

```rust
pub use browseros_bridge::types::{DownloadInfo, DownloadState};
```

---

## 7. HAR Data Structures

### 7.1 Feature Gate

All HAR types are gated behind `#[cfg(feature = "har")]`.

### 7.2 Data Model

```rust
// HAR 1.2 specification-compatible data structures.

struct HarLog {
    version: String,         // "1.2"
    creator: HarCreator,
    pages: Vec<HarPage>,
    entries: Vec<HarEntry>,
}

struct HarCreator {
    name: String,
    version: String,
}

struct HarPage {
    id: String,
    title: String,
    started_date_time: DateTime<Utc>,
    page_timings: HarPageTimings,
}

struct HarPageTimings {
    on_content_load: f64,
    on_load: f64,
}

struct HarEntry {
    page_ref: String,
    started_date_time: DateTime<Utc>,
    time: f64,
    request: HarRequest,
    response: HarResponse,
    cache: HarCache,
    timings: HarTimings,
    server_ip_address: Option<String>,
    connection: Option<String>,
}

struct HarRequest {
    method: String,
    url: String,
    http_version: String,
    headers: Vec<HarHeader>,
    query_string: Vec<HarQueryParam>,
    cookies: Vec<HarCookie>,
    headers_size: i64,
    body_size: i64,
    post_data: Option<HarPostData>,
}

struct HarResponse {
    status: u16,
    status_text: String,
    http_version: String,
    headers: Vec<HarHeader>,
    cookies: Vec<HarCookie>,
    content: HarContent,
    redirect_url: String,
    headers_size: i64,
    body_size: i64,
}

struct HarHeader {
    name: String,
    value: String,
}

struct HarQueryParam {
    name: String,
    value: String,
}

struct HarCookie {
    name: String,
    value: String,
    path: Option<String>,
    domain: Option<String>,
    expires: Option<String>,
    http_only: Option<bool>,
    secure: Option<bool>,
}

struct HarPostData {
    mime_type: String,
    text: String,
}

struct HarContent {
    size: i64,
    mime_type: String,
    text: Option<String>,
    encoding: Option<String>,
}

struct HarCache {
    before_request: Option<HarCacheEntry>,
    after_request: Option<HarCacheEntry>,
}

struct HarCacheEntry {
    expires: Option<String>,
    last_access: String,
    hit_count: i64,
    comment: Option<String>,
}

struct HarTimings {
    dns: f64,
    connect: f64,
    ssl: f64,
    send: f64,
    wait: f64,
    receive: f64,
    blocked: f64,
}
```

---

## 8. Error Type

### 8.1 NetworkError

```rust
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum NetworkError {
    #[error("network operation failed: {0}")]
    OperationFailed(String),

    #[error("request not found: {0}")]
    RequestNotFound(RequestId),

    #[error("cookie operation failed: {0}")]
    CookieError(String),

    #[error("interception rule not found")]
    InterceptionRuleNotFound,

    #[error("download failed: {0}")]
    DownloadFailed(String),

    #[error("download not found: {0}")]
    DownloadNotFound(String),

    #[error("bridge error: {0}")]
    BridgeError(#[from] BridgeError),
}

pub type NetworkResult<T> = Result<T, NetworkError>;
```

---

## 9. Public API Surface Summary

| Category | Types | Classification |
|----------|-------|---------------|
| Network Handle | `NetworkHandle` | SAFE TO FREEZE |
| Request Identity | `RequestId` | SAFE TO FREEZE |
| Request State | `RequestState` | SAFE TO FREEZE |
| Interception | `InterceptionBuilder`, `InterceptionRule`, `InterceptionAction`, `InterceptionHandle` | SAFE TO FREEZE |
| Conditions | `NetworkConditionBuilder`, `NetworkConditions` | SAFE TO FREEZE |
| Cookies | `CookieManager`, `Cookie`, `SameSitePolicy` | KEEP FLEXIBLE |
| WebSocket | `WebSocketInfo`, `WebSocketMessage` | KEEP FLEXIBLE |
| Downloads | `DownloadInfo`, `DownloadState` | SAFE TO FREEZE |
| HAR (gated) | `HarLog`, `HarEntry`, `HarRequest`, `HarResponse`, etc. | KEEP FLEXIBLE |
| Auth events | `AuthChallengeReceived`, `AuthCredentialsSupplied`, `AuthFailed` | EXPERIMENTAL — observational only. Credential delivery not implemented in Phase 2.6. See architecture doc §1.1 for rationale. |
| Error | `NetworkError` | SAFE TO FREEZE |
| Events | `NetworkEvent` payload types | SAFE TO FREEZE |
