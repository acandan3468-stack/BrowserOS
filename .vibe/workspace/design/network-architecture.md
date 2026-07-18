# Phase 2.6 — Network Architecture

**Status:** DESIGN — Architecture defined. Not yet implemented.  
**Goal:** Define the complete architecture of `browseros-network` before any code is written.  
**Constraints:** No implementation code. No Rust code generation. Architecture freeze only.

---

## 1. Responsibilities

### 1.1 What browseros-network OWNS

- **Request/Response Lifecycle** — observe, track, and correlate HTTP requests and responses at the page level. Each request gets a unique `RequestId`. Response timing, headers, status, body metadata are tracked.
- **Request Interception** — high-level, protocol-agnostic interception rules. Consumers register `InterceptionRule` values; intercepted requests can be blocked, continued, or responded to with custom data.
- **Network Condition Emulation** — control offline mode, latency, and bandwidth throttling via `NetworkConditions`.
- **Cache Management** — clear browser cache and cookies.
- **Cookie Management** — list, set, delete cookies through the bridge `StoragePort`.
- **WebSocket Monitoring** — observe WebSocket open/close/message events at the page level.
- **Download Management** — track active and completed downloads, configure download path, cancel downloads.
- **Network Event Payloads** — typed event structs for request started, response received, request failed, navigation, redirect, WebSocket, download, interception. No EventBus dependency — event types are exported for upper layers to wire.
- **Authentication Support (EXPERIMENTAL)** — auth challenge observation via typed events (`AuthChallengeReceived`, `AuthCredentialsSupplied`, `AuthFailed`). **Credential delivery is NOT implemented in Phase 2.6.** The frozen bridge `NetworkPort` does not expose auth challenge response methods, and `InterceptionAction` lacks a credential variant. Consumers can observe auth challenges but cannot respond. Credential delivery will be added in a future phase when the bridge extends `InterceptionAction` with auth support (e.g., `ProvideCredentials { username, password }`).

### 1.2 What browseros-network MUST NOT own

- **CDP protocol types** — zero CDP types. All protocol interaction goes through `browseros-bridge` `NetworkPort` and `DownloadPort` traits.
- **Transport** — no WebSocket client, no TCP, no HTTP client. The crate is a consumer of bridge interfaces.
- **EventBus** — no direct EventBus dependency. Network events are payload structs only. Upper layers wire them to EventBus.
- **Async runtime** — all API is synchronous. No async/future.
- **Artifact storage** — HAR persistence is `browseros-artifact` responsibility.
- **Request body manipulation** — interception actions support `Respond` with body bytes, but full body streaming/manipulation is backend-specific and not exposed.
- **TLS/certificate management** — handled by the browser backend, not exposed.
- **DNS resolution** — handled by the browser backend.
- **Proxy configuration** — handled by the browser backend or launch options.
- **Network mocking/replay** — future crate or plugin, not frozen here.

### 1.3 Boundary Clarifications

| Neighboring Crate | What browseros-network consumes | What browseros-network provides |
|---|---|---|
| `browseros-bridge` | `NetworkPort`, `DownloadPort`, `StoragePort`, `RequestInfo`, `ResponseInfo`, `TimingInfo`, `ResourceType`, `InterceptionRule`, `InterceptionAction`, `InterceptionHandle`, `NetworkConditions`, `WebSocketMessage`, `DownloadInfo`, `Cookie`, `PageId`, `FrameId` | — (bridge is the contract) |
| `browseros-page` | `PageId`, `FrameId` for event attribution | Network event payloads for page context |
| `browseros-dom` | — (sibling) | FrameId-attributed request events (both consume `RequestInfo.frame_id` from shared bridge types; no direct crate dependency) |
| `browseros-browser` | — (sibling, higher-level) | `NetworkHandle` for page-level network operations |
| `browseros-cdp` | — (isolated by bridge) | — (no direct interaction) |
| `browseros-input` | — (sibling) | — (no interaction) |
| `browseros-storage` | — (sibling) | Cookie management may call storage operations |
| `browseros-artifact` | — (sibling) | HAR-ready data structures for artifact export |
| `browseros-plugin` | — (sibling) | Network event payloads for plugin callbacks |
| `browseros-runtime` | Composition | Public API integration |

---

## 2. Module Layout

```
browseros-network/src/
├── lib.rs                  # Crate root, public re-exports
├── error.rs                # NetworkError (#[non_exhaustive]) + NetworkResult
├── handle.rs               # NetworkHandle (wraps NetworkPort + DownloadPort)
├── request.rs              # RequestId, RequestTracker, RequestState
├── response.rs             # ResponseTracker, ResponseBody
├── interception.rs         # InterceptionBuilder, high-level interception API
├── websocket.rs            # WebSocketTracker, WebSocketEvent
├── download.rs             # DownloadTracker, download event payloads
├── cookie.rs               # CookieManager (wraps StoragePort cookie methods)
├── auth.rs                 # AuthChallengeHandler, credential types
├── condition.rs            # NetworkConditionBuilder (fluent condition config)
├── har.rs                  # HarEntry, HarLog, HarPage — data structures for HAR export
├── events.rs               # NetworkEvent types (payloads only, no EventBus)
└── id.rs                   # RequestId (re-exported from browseros-types or newtype)
```

---

## 3. Dependency Graph

```
browseros-types (FROZEN)
  ↑
browseros-bridge (FROZEN)
  ↑
browseros-network (NEW)
  ├── deps: browseros-types, browseros-bridge, chrono, thiserror
  ├── serde, serde_json (for HAR serialization — gated behind "har" feature)
  └── NO: browseros-cdp, browseros-page, browseros-browser, browseros-dom, EventBus, observability
```

**Default dependencies:** `browseros-types`, `browseros-bridge`, `chrono`, `thiserror`

**Optional dependencies:**
- `serde`, `serde_json` — gated behind `har` feature for HAR serialization

---

## 4. Ownership Model

- **NetworkHandle** — consumer-owned. `Clone + Send + Sync`. Wraps `Arc<dyn NetworkPort>` and `Arc<dyn DownloadPort>`. No backend identity — the handle is a routing proxy.
- **RequestTracker** — created internally when a request starts. Stored by the network monitoring subscriber. One per request. Removed when request completes or fails.
- **InterceptionRule** — value type. Registered via `NetworkHandle`. Removed via `InterceptionHandle`.
- **CookieManager** — created by `NetworkHandle::cookies()`. Wraps `Arc<dyn StoragePort>`. Lightweight, cloneable.
- **DownloadTracker** — created internally. Updated by backend events. Accessed via `NetworkHandle::downloads()`.
- **WebSocketTracker** — created when a WebSocket connects. Removed on disconnect.

All stateful objects are owned by the consumer. The crate has no global state, no singletons, no background threads.

---

## 5. Lifecycle

### 5.1 Request Lifecycle

```
Page makes request
  → NetworkPort emits "request started" event (backend → bridge)
  → NetworkHandle assigns RequestId (UUID v7)
  → RequestTracker created
    → If interception rule matches:
      → InterceptionAction applied (Block/Continue/Respond)
  → NetworkPort emits "response received" event
  → ResponseTracker populated from ResponseInfo
  → RequestTracker completed
  → NetworkEvent::RequestCompleted emitted (as payload)

  Redirect (creates new RequestId per hop):
  → ResponseStarted (original request receives 3xx)
  → RequestRedirected emitted (request_id, from_url, to_url, status)
  → Original RequestTracker finalized
  → RequestStarted emitted (new RequestId, original_request_id links back)
  → New RequestTracker created for redirect target
  → Standard lifecycle continues for the new request
```

### 5.2 Interception Lifecycle

```
Consumer builds InterceptionRule
  → NetworkHandle.add_interception_rule() → InterceptionHandle
  → Backend intercepts matching requests
  → Consumer receives InterceptedRequest event (if callback registered)
  → Consumer can respond with custom body, headers, status
  → NetworkHandle.remove_interception_rule(handle) to unregister
```

### 5.3 WebSocket Lifecycle

```
Page opens WebSocket
  → NetworkPort emits ws-open event
  → NetworkHandle creates WebSocketTracker
  → Messages flow (NetworkPort emits ws-message events)
  → WebSocket closes (NetworkPort emits ws-close event)
  → WebSocketTracker removed
```

### 5.4 Download Lifecycle

```
Page initiates download
  → Backend creates download
  → DownloadTracker created with DownloadState::InProgress
  → Progress updates (received_bytes, state changes)
  → Download completes, fails, or is cancelled
  → DownloadTracker finalized
```

---

## 6. Thread Model

| Type | Send | Sync | Clone | Notes |
|------|------|------|-------|-------|
| `NetworkHandle` | Yes | Yes | Yes | All backend calls serialized through Arc<dyn NetworkPort> |
| `RequestTracker` | Yes | Yes | No | Mutable state, one per request |
| `ResponseTracker` | Yes | Yes | Yes | Immutable after creation |
| `CookieManager` | Yes | Yes | Yes | Wraps Arc<dyn StoragePort> |
| `InterceptionRule` | Yes | Yes | Yes | Value type |
| `HarEntry` | Yes | Yes | Yes | Pure data |
| `NetworkError` | Yes | Yes | Yes | Error type |

All API is synchronous. Backend calls block the consumer's thread but are serialized through the bridge port's internal synchronization.

---

## 7. Interaction with browser/page/dom

### 7.1 Access Pattern

Consumers access network functionality through the page:
```
page.network() → NetworkHandle
page.network().set_conditions(my_conditions)
page.network().intercept(rule)
page.network().cookies().all()
page.network().downloads()
page.network().clear_cache()
```

### 7.2 Frame Attribution

Every `RequestInfo` carries `frame_id: Option<FrameId>`, which allows the DOM crate to attribute requests to specific elements or frames. This is a one-way data flow: network → dom (via FrameId).

### 7.3 Navigation Integration

The page handles navigation (navigate, reload, goBack, goForward) in `browseros-page`. Network events (navigation request, response, redirect) are emitted as network events, not page events. The page crate subscribes to navigation-related network events to track navigation state.

### 7.4 Download Integration

Downloads are initiated by the page but managed by the network crate. `DownloadPort` lives on `PagePort` and is surfaced through `NetworkHandle::download_port()` or through dedicated download methods on `NetworkHandle`.
