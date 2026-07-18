# Network Lifetime Model

**Status:** DESIGN — Lifetime model defined. Not yet implemented.

---

## 1. Identity

### 1.1 RequestId

Every HTTP request observed on a page receives a `RequestId` — a UUID v7 assigned at the moment the request is first detected. The `RequestId` is the **stable identity** of the request from the consumer's perspective.

The `RequestId` survives:
- Request body chunks (same request, same id)
- Cross-frame attribution (same request, same id)

The `RequestId` does NOT survive:
- **Redirect hops** — each redirect hop receives a **new** `RequestId`. The chain is tracked via `RequestStartedPayload.original_request_id` and the redirecting tracker's `redirect_chain`. (See §2.2)
- Across page navigations (new document, new request context)
- Across page sessions (new page, new request context)

### 1.2 NetworkHandle Identity

`NetworkHandle` has no persistent identity — it is a proxy to the backend `NetworkPort`. There is no generation counter, no stale detection. If the page closes, all operations return `NetworkError::BridgeError(BridgeError::PageClosed(...))`.

The handle is valid as long as:
- The owning page is open
- The backend connection is active
- The page has not navigated away from its current session (the backend `NetworkPort` remains valid across navigations within the same page)

---

## 2. Request Lifetime

```
┌──────────────────────────────────────────────────────────────────┐
│ Request Lifetime                                                  │
│                                                                  │
│  RequestStarted (ID=A, url=original)                              │
│     │                                                            │
│     ├──→ ResponseStarted (ID=A) → RequestRedirected (ID=A)       │
│     │       │                                                    │
│     │       └──→ RequestTracker for A finalized                  │
│     │       └──→ RequestStarted (ID=B, original_id=A, url=target)│
│     │                 │                                          │
│     │                 ├──→ ResponseStarted (ID=B)                │
│     │                 │       │                                  │
│     │                 │       └──→ ResponseFinished (ID=B)       │
│     │                 │               │                          │
│     │                 │               └──→ RequestCompleted (ID=B)│
│     │                 │                                          │
│     │                 ├──→ RequestFailed (ID=B)                  │
│     │                 │                                          │
│     │                 └──→ RequestAborted (ID=B)                 │
│     │                                                            │
│     ├──→ ResponseStarted (no redirect) → RequestCompleted        │
│     │                                                            │
│     ├──→ RequestFailed                                           │
│     │                                                            │
│     └──→ RequestAborted                                          │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
```

### 2.1 Creation

A `RequestTracker` is created when the backend emits a "request will be sent" event. The `NetworkHandle` assigns a `RequestId`, populates the tracker with the initial `RequestInfo`, and emits a `RequestStarted` event.

### 2.2 Redirects

Each redirect hop receives its own `RequestId`. The chain is connected via `original_request_id` on `RequestStartedPayload`. When a redirect response is received:

1. **Original request (ID=A):**
   - A `ResponseStarted` event is emitted as usual for the 3xx response
   - A `RequestRedirected { request_id: A, from_url, to_url, status }` event is emitted
   - The redirect URL is appended to tracker A's `redirect_chain`
   - Tracker A is finalized

2. **Redirect target (ID=B):**
   - A new `RequestStarted { request_id: B, original_request_id: Some(A), url: redirect_target }` event is emitted
   - A new `RequestTracker` is created for ID=B
   - Request B follows the standard lifecycle (response, completion, failure, or further redirects)
   - If the redirect target itself redirects, the chain continues: C gets `original_request_id: Some(B)`, and so on

This creates a chain: A → B → C → ... Each hop is independently tracked, and the `original_request_id` field allows consumers to follow the chain backward. The `redirect_chain` on the original tracker records the forward path.

### 2.3 Completion

A request completes when the full response body has been received. The tracker records the final `ResponseInfo`, `TimingInfo`, and body size. A `RequestCompleted` event is emitted. The tracker is finalized and made available for HAR export.

### 2.4 Failure

A request fails due to DNS failure, connection refused, timeout, TLS error, or protocol error. A `RequestFailed` event is emitted with a description. The tracker is finalized with `RequestState::Failed`.

### 2.5 Abortion

A request is aborted when the page cancels it (e.g., navigation away, `window.stop()`, or consumer interception). A `RequestAborted` event is emitted. The tracker is finalized with `RequestState::Aborted`.

---

## 3. Response Lifetime

Responses are paired with requests via `RequestId`. A response begins when the first bytes arrive (`ResponseStarted`) and ends when the full body is received (`ResponseFinished`). For responses without a body (204, 304, HEAD requests), the two events fire in quick succession.

Response tracking is read-only. Consumers cannot modify response data — interception is the mechanism for altering request/response behavior.

---

## 4. Streaming

The network crate does NOT support response body streaming. Response bodies are either:
- **Not captured** (default — only metadata is tracked)
- **Captured on interception** (the `Respond` action provides custom body bytes)

Full body capture is a conscious exclusion. Backend-provided response bodies can be very large (multi-MB images, video, documents). Capturing every response body in memory would be prohibitive. Consumers who need bodies should:
1. Use interception to capture specific requests based on URL/resource type
2. Configure the backend to provide bodies for intercepted requests via `InterceptionAction::Respond`

Future phases may add opt-in body capture via a `BodyCaptureMode` configuration.

---

## 5. Cancellation

### 5.1 Request Cancellation

Requests cannot be cancelled once started. The closest equivalent is interception with `Block` action, which prevents the request from being sent. Once a request is in-flight, the browser backend controls cancellation.

### 5.2 Download Cancellation

Downloads can be cancelled via `NetworkHandle::cancel_download()`. This calls the bridge `DownloadPort::cancel_download()`. Cancellation is best-effort — the backend may not always succeed in stopping an in-progress download.

### 5.3 WebSocket Disconnection

WebSocket connections cannot be programmatically closed through the network crate. The page or the remote server controls WebSocket lifecycle. The crate only observes WebSocket events.

---

## 6. Ownership

### 6.1 Handle Ownership

| Type | Consumer Owned | Clone | Send | Sync | Notes |
|------|---------------|-------|------|------|-------|
| `NetworkHandle` | Yes | Yes | Yes | Yes | Lightweight proxy, no backend state |
| `CookieManager` | Yes | Yes | Yes | Yes | Wraps Arc<dyn StoragePort> |
| `InterceptionRule` | Yes | Yes | Yes | Yes | Value type |
| `InterceptionHandle` | Yes | Yes | Yes | Yes | Returned by add_interception_rule |
| `RequestTracker` | Internal | No | Yes | No | Mutable, one per request |
| `ResponseTracker` | Internal | Yes | Yes | Yes | Immutable after creation |
| `WebSocketInfo` | Yes | Yes | Yes | Yes | Immutable |

### 6.2 Dropping

Dropping a `NetworkHandle` does NOT invalidate the underlying backend. Other clones remain valid. The backend `NetworkPort` is reference-counted through `Arc` and lives as long as any handle or the page itself holds a reference.

Dropping an `InterceptionHandle` does NOT remove the rule — consumers must explicitly call `remove_interception_rule()`.

---

## 7. Stale Detection

`NetworkHandle` has no stale detection mechanism. Unlike `ElementHandle` (which has generation counters for DOM staleness), network operations don't have a concept of "stale" — the backend `NetworkPort` is either connected and working, or returning errors.

Errors from the backend are propagated as `NetworkError::BridgeError`. Consumers should check for:
- `BridgeError::PageClosed(page_id)` — page is gone, handle is useless
- `BridgeError::SessionClosed(session_id)` — session is gone
- `BridgeError::ConnectionRefused` / `ConnectionTimedOut` — transient errors

There is no auto-retry. Consumers retry on transient errors if desired.

---

## 8. Thread Safety Summary

| Type | Send | Sync | Clone | Backend Call Pattern |
|------|------|------|-------|---------------------|
| `NetworkHandle` | Yes | Yes | Yes | Serialized via Arc<dyn NetworkPort> |
| `CookieManager` | Yes | Yes | Yes | Serialized via Arc<dyn StoragePort> |
| `InterceptionRule` | Yes | Yes | Yes | No backend (pure data) |
| `InterceptionBuilder` | Yes | Yes | Yes | No backend (pure data) |
| `NetworkConditionBuilder` | Yes | Yes | Yes | No backend (pure data) |
| `NetworkError` | Yes | Yes | Yes | No backend (error type) |
| `ResponseTracker` | Yes | Yes | Yes | No backend (pure data) |
| `WebSocketInfo` | Yes | Yes | Yes | No backend (pure data) |
| `DownloadInfo` | Yes | Yes | Yes | No backend (pure data) |
| `HarEntry` (gated) | Yes | Yes | Yes | No backend (pure data) |
| `NetworkEvent` payloads | Yes | Yes | Yes | No backend (pure data) |
