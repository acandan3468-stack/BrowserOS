# Network Event Model

**Status:** DESIGN — Event model defined. Not yet implemented.

---

## 1. Design

- `browseros-network` defines **event payload types only**. It does NOT depend on `EventBus`.
- Upper layers (runtime, plugin system) create EventBus event variants from these payloads.
- Payloads are `Clone + Send + Sync` plain data structures (no trait objects, no backreferences).
- Every event carries the source `PageId` and relevant `FrameId` and `RequestId` where applicable.

---

## 2. Event Catalog

### 2.1 Request Lifecycle

| Event | Payload | Trigger |
|-------|---------|---------|
| `RequestStarted` | `page_id, frame_id, request_id, url, method, resource_type, headers` | New HTTP request initiated |
| `RequestRedirected` | `page_id, frame_id, request_id, from_url, to_url, status` | Request received a redirect response |
| `RequestCompleted` | `page_id, frame_id, request_id, url, status, mime_type, timing, body_size` | Response fully received |
| `RequestFailed` | `page_id, frame_id, request_id, url, error` | Request failed (DNS, connection, timeout, etc.) |
| `RequestAborted` | `page_id, frame_id, request_id, url` | Request cancelled by page or consumer |

### 2.2 Response Lifecycle

| Event | Payload | Trigger |
|-------|---------|---------|
| `ResponseStarted` | `page_id, frame_id, request_id, url, status, status_text, headers` | First bytes of response received |
| `ResponseFinished` | `page_id, frame_id, request_id, url, status, body_size, timing` | Response body fully received |

### 2.3 Interception Events

| Event | Payload | Trigger |
|-------|---------|---------|
| `RequestIntercepted` | `page_id, frame_id, request_id, url, method, resource_type, interception_handle` | Request matched an interception rule |
| `InterceptionHandled` | `page_id, frame_id, request_id, action` | Interception action applied (Block/Continue/Respond) |

### 2.4 Navigation Events

| Event | Payload | Trigger |
|-------|---------|---------|
| `NavigationRequested` | `page_id, frame_id, request_id, navigation_id, url` | Page navigation request initiated |
| `NavigationCommitted` | `page_id, frame_id, request_id, navigation_id, url` | Navigation response committed |
| `NavigationCompleted` | `page_id, frame_id, request_id, navigation_id, url, status` | Navigation fully completed |

The `navigation_id: Option<NavigationId>` field carries the navigation identifier for correlation with `page.navigate()` calls. The network crate populates it when the bridge provides it; it may be `None` if the bridge does not expose navigation IDs for a particular backend. The `NavigationId` type is from `browseros-bridge::identifiers`.

### 2.5 WebSocket Events

| Event | Payload | Trigger |
|-------|---------|---------|
| `WebSocketCreated` | `page_id, frame_id, url, request_id` | WebSocket connection established |
| `WebSocketMessageSent` | `page_id, frame_id, url, data, request_id` | Message sent to server |
| `WebSocketMessageReceived` | `page_id, frame_id, url, data, request_id` | Message received from server |
| `WebSocketClosed` | `page_id, frame_id, url, code, reason, request_id` | WebSocket connection closed |
| `WebSocketError` | `page_id, frame_id, url, error, request_id` | WebSocket error occurred |

### 2.6 Download Events

| Event | Payload | Trigger |
|-------|---------|---------|
| `DownloadStarted` | `page_id, url, download_id, suggested_filename, mime_type, total_bytes` | New download begins |
| `DownloadProgress` | `page_id, download_id, received_bytes, total_bytes` | Download progress update |
| `DownloadCompleted` | `page_id, download_id, file_path` | Download finished successfully |
| `DownloadFailed` | `page_id, download_id, error` | Download failed |
| `DownloadCancelled` | `page_id, download_id` | Download cancelled by consumer |

### 2.7 Network Condition Events

| Event | Payload | Trigger |
|-------|---------|---------|
| `NetworkConditionsChanged` | `page_id, offline, latency_ms, download_throughput, upload_throughput` | Network conditions updated |
| `NetworkOffline` | `page_id` | Network set offline |
| `NetworkOnline` | `page_id` | Network restored online |

### 2.8 Auth Events

| Event | Payload | Trigger |
|-------|---------|---------|
| `AuthChallengeReceived` | `page_id, frame_id, request_id, url, scheme, realm, challenge` | Server requested authentication |
| `AuthCredentialsSupplied` | `page_id, frame_id, request_id` | Credentials provided for auth challenge |
| `AuthFailed` | `page_id, frame_id, request_id, reason` | Authentication failed |

### 2.9 Cache Events

| Event | Payload | Trigger |
|-------|---------|---------|
| `CacheCleared` | `page_id` | Browser cache cleared |

---

## 3. Payload Structures

```rust
use browseros_bridge::identifiers::{FrameId, NavigationId, PageId};
use browseros_bridge::types::{InterceptionAction, ResourceType, TimingInfo};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use chrono::{DateTime, Utc};

// ——— Request Lifecycle ———

struct RequestStartedPayload {
    page_id: PageId,
    frame_id: Option<FrameId>,
    request_id: RequestId,
    original_request_id: Option<RequestId>,  // Set when this request is a redirect target — links back to the original
    url: String,
    method: String,
    resource_type: ResourceType,
    headers: HashMap<String, String>,
}

struct RequestRedirectedPayload {
    page_id: PageId,
    frame_id: Option<FrameId>,
    request_id: RequestId,
    from_url: String,
    to_url: String,
    status: u16,
}

struct RequestCompletedPayload {
    page_id: PageId,
    frame_id: Option<FrameId>,
    request_id: RequestId,
    url: String,
    status: u16,
    mime_type: String,
    timing: TimingInfo,
    body_size: u64,
}

struct RequestFailedPayload {
    page_id: PageId,
    frame_id: Option<FrameId>,
    request_id: RequestId,
    url: String,
    error: String,
}

struct RequestAbortedPayload {
    page_id: PageId,
    frame_id: Option<FrameId>,
    request_id: RequestId,
    url: String,
}

// ——— Response Lifecycle ———

struct ResponseStartedPayload {
    page_id: PageId,
    frame_id: Option<FrameId>,
    request_id: RequestId,
    url: String,
    status: u16,
    status_text: String,
    headers: HashMap<String, String>,
}

struct ResponseFinishedPayload {
    page_id: PageId,
    frame_id: Option<FrameId>,
    request_id: RequestId,
    url: String,
    status: u16,
    body_size: u64,
    timing: TimingInfo,
}

// ——— Interception ———

struct RequestInterceptedPayload {
    page_id: PageId,
    frame_id: Option<FrameId>,
    request_id: RequestId,
    url: String,
    method: String,
    resource_type: ResourceType,
    interception_handle: InterceptionHandle,
}

struct InterceptionHandledPayload {
    page_id: PageId,
    frame_id: Option<FrameId>,
    request_id: RequestId,
    action: InterceptionAction,
}

// ——— Navigation ———

struct NavigationRequestedPayload {
    page_id: PageId,
    frame_id: Option<FrameId>,
    request_id: RequestId,
    navigation_id: Option<NavigationId>,
    url: String,
}

struct NavigationCommittedPayload {
    page_id: PageId,
    frame_id: Option<FrameId>,
    request_id: RequestId,
    navigation_id: Option<NavigationId>,
    url: String,
}

struct NavigationCompletedPayload {
    page_id: PageId,
    frame_id: Option<FrameId>,
    request_id: RequestId,
    navigation_id: Option<NavigationId>,
    url: String,
    status: u16,
}

// ——— WebSocket ———

struct WebSocketCreatedPayload {
    page_id: PageId,
    frame_id: Option<FrameId>,
    url: String,
    request_id: RequestId,
}

struct WebSocketMessageSentPayload {
    page_id: PageId,
    frame_id: Option<FrameId>,
    url: String,
    data: String,
    request_id: RequestId,
}

struct WebSocketMessageReceivedPayload {
    page_id: PageId,
    frame_id: Option<FrameId>,
    url: String,
    data: String,
    request_id: RequestId,
}

struct WebSocketClosedPayload {
    page_id: PageId,
    frame_id: Option<FrameId>,
    url: String,
    code: u16,
    reason: String,
    request_id: RequestId,
}

struct WebSocketErrorPayload {
    page_id: PageId,
    frame_id: Option<FrameId>,
    url: String,
    error: String,
    request_id: RequestId,
}

// ——— Download ———

struct DownloadStartedPayload {
    page_id: PageId,
    url: String,
    download_id: String,
    suggested_filename: String,
    mime_type: String,
    total_bytes: f64,
}

struct DownloadProgressPayload {
    page_id: PageId,
    download_id: String,
    received_bytes: f64,
    total_bytes: f64,
}

struct DownloadCompletedPayload {
    page_id: PageId,
    download_id: String,
    file_path: PathBuf,
}

struct DownloadFailedPayload {
    page_id: PageId,
    download_id: String,
    error: String,
}

struct DownloadCancelledPayload {
    page_id: PageId,
    download_id: String,
}

// ——— Network Conditions ———

struct NetworkConditionsChangedPayload {
    page_id: PageId,
    offline: bool,
    latency_ms: u64,
    download_throughput: Option<f64>,
    upload_throughput: Option<f64>,
}

struct NetworkOfflinePayload {
    page_id: PageId,
}

struct NetworkOnlinePayload {
    page_id: PageId,
}

// ——— Auth ———

struct AuthChallengeReceivedPayload {
    page_id: PageId,
    frame_id: Option<FrameId>,
    request_id: RequestId,
    url: String,
    scheme: String,
    realm: Option<String>,
    challenge: Option<String>,
}

struct AuthCredentialsSuppliedPayload {
    page_id: PageId,
    frame_id: Option<FrameId>,
    request_id: RequestId,
}

struct AuthFailedPayload {
    page_id: PageId,
    frame_id: Option<FrameId>,
    request_id: RequestId,
    reason: String,
}

// ——— Cache ———

struct CacheClearedPayload {
    page_id: PageId,
}

// ——— NetworkEvent Enum ———

#[non_exhaustive]
enum NetworkEvent {
    // Request Lifecycle
    RequestStarted(RequestStartedPayload),
    RequestRedirected(RequestRedirectedPayload),
    RequestCompleted(RequestCompletedPayload),
    RequestFailed(RequestFailedPayload),
    RequestAborted(RequestAbortedPayload),

    // Response Lifecycle
    ResponseStarted(ResponseStartedPayload),
    ResponseFinished(ResponseFinishedPayload),

    // Interception
    RequestIntercepted(RequestInterceptedPayload),
    InterceptionHandled(InterceptionHandledPayload),

    // Navigation
    NavigationRequested(NavigationRequestedPayload),
    NavigationCommitted(NavigationCommittedPayload),
    NavigationCompleted(NavigationCompletedPayload),

    // WebSocket
    WebSocketCreated(WebSocketCreatedPayload),
    WebSocketMessageSent(WebSocketMessageSentPayload),
    WebSocketMessageReceived(WebSocketMessageReceivedPayload),
    WebSocketClosed(WebSocketClosedPayload),
    WebSocketError(WebSocketErrorPayload),

    // Download
    DownloadStarted(DownloadStartedPayload),
    DownloadProgress(DownloadProgressPayload),
    DownloadCompleted(DownloadCompletedPayload),
    DownloadFailed(DownloadFailedPayload),
    DownloadCancelled(DownloadCancelledPayload),

    // Network Conditions
    NetworkConditionsChanged(NetworkConditionsChangedPayload),
    NetworkOffline(NetworkOfflinePayload),
    NetworkOnline(NetworkOnlinePayload),

    // Auth
    AuthChallengeReceived(AuthChallengeReceivedPayload),
    AuthCredentialsSupplied(AuthCredentialsSuppliedPayload),
    AuthFailed(AuthFailedPayload),

    // Cache
    CacheCleared(CacheClearedPayload),
}
```
