# Network Freeze Plan

**Status:** DESIGN — Freeze plan defined. Not yet implemented.

---

## 1. Freeze Classification by API Surface

### 1.1 SAFE TO FREEZE (Stable Design, Minimal Change Expected)

| API | Rationale |
|-----|-----------|
| `NetworkHandle` | Core proxy pattern is stable. Wraps frozen bridge `NetworkPort` + `DownloadPort`. No generation counters, no stale detection needed. |
| `RequestId` | UUID v7 identity. Simple and final. |
| `RequestState` | Three terminal states (Completed, Failed, Aborted) plus Initiated and Received. Redirects create a new `RequestId` per hop (see redirect model). Stable. |
| `InterceptionRule` | Re-exported from bridge. Frozen in bridge. |
| `InterceptionAction` | Re-exported from bridge. Frozen in bridge. |
| `InterceptionHandle` | Re-exported from bridge. Frozen in bridge. |
| `InterceptionBuilder` | Fluent builder for constructing rules. Straightforward. |
| `NetworkConditions` | Re-exported from bridge. Frozen in bridge. |
| `NetworkConditionBuilder` | Fluent builder for condition configuration. |
| `DownloadInfo` | Re-exported from bridge. Frozen in bridge. |
| `DownloadState` | Re-exported from bridge. Frozen in bridge. |
| `ResourceType` | Re-exported from bridge. Frozen in bridge. |
| `TimingInfo` | Re-exported from bridge. Frozen in bridge. |
| `WebSocketMessage` | Re-exported from bridge. Frozen in bridge. |
| `NetworkError` | Error variants cover all known failure modes. `#[non_exhaustive]` for forward compat. |
| `NetworkEvent` payloads | Pure data. No EventBus. Stable by design. |

### 1.2 KEEP FLEXIBLE (Design Reasonably Stable But May Evolve)

| API | Rationale | Expected Change |
|-----|-----------|-----------------|
| `CookieManager` | Cookie API is generally stable, but future backends may support additional cookie attributes. | New methods for partitioned cookies, cookie store isolation. |
| `WebSocketInfo` | WebSocket monitoring is well-understood, but additional metrics (message count, bandwidth) may be added. | New fields for cumulative metrics. |
| `HarLog`, `HarEntry`, etc. | HAR 1.2 spec is frozen, but serialization format and optional fields may vary. Gated behind `har` feature. | Additional optional HAR fields for backend-specific data. |
| `RequestTracker` (internal) | Internal implementation. May gain fields for performance metrics. | Additional timing breakdown fields. |

### 1.3 EXPERIMENTAL (Design May Change Significantly)

| API | Rationale |
|-----|-----------|
| Auth events (`AuthChallengeReceived`, `AuthCredentialsSupplied`, `AuthFailed`) | **Credential delivery is not implemented in Phase 2.6.** The frozen bridge `NetworkPort` does not expose auth challenge response methods. The event types are observational only — consumers can see auth challenges but cannot respond. Delivery requires bridge extension (adding `ProvideCredentials` to `InterceptionAction` or a dedicated auth API), which is deferred to a future phase. Auth challenge semantics also vary significantly across backends. |
| Response body streaming | Not implemented in Phase 2.6. May be added in a future phase when streaming backpressure models are better understood. |
| Network mocking/replay | Entirely future work. Anticipated but not designed. Will be its own crate or plugin. |
| Request prioritization | HTTP/2 and HTTP/3 support request prioritization, but the bridge does not expose this. May be added when the bridge supports it. |

---

## 2. Crate Dependency Freeze

| Dependency | Status | Notes |
|------------|--------|-------|
| `browseros-types` | FROZEN | `HandleId`, `PageId`, `FrameId` all exist. No new identifiers needed in types crate — `RequestId` defined in-network. |
| `browseros-bridge` | FROZEN | `NetworkPort`, `DownloadPort`, `StoragePort`, `RequestInfo`, `ResponseInfo`, `TimingInfo`, `ResourceType`, `InterceptionRule`, `InterceptionAction`, `InterceptionHandle`, `NetworkConditions`, `WebSocketMessage`, `DownloadInfo`, `Cookie`, `PageId`, `FrameId` all frozen. |
| `chrono` | FROZEN | Timestamps for event payloads and HAR data. |
| `thiserror` | FROZEN | Error type derive. |
| `serde` / `serde_json` | OPTIONAL | Gated behind `har` feature. Only needed for HAR serialization. |

---

## 3. What We Gain by Freezing Now

1. **Network monitoring is predictable.** Consumers know exactly which events fire at each stage of the request lifecycle.
2. **Interception model is locked.** `InterceptionRule` + `InterceptionAction` from bridge are consumed directly. No wrapping, no indirection.
3. **Cross-crate contracts are clear.** `browseros-page` knows it exposes `NetworkHandle` via `page.network()`. `browseros-dom` knows requests carry `FrameId` for attribution.
4. **HAR export is planned but not frozen.** The data model exists behind a feature gate. Export implementation is deferred.
5. **Event integration is straightforward.** Upper layers map `NetworkEvent` variants to EventBus events. No runtime dependency from the network crate.

---

## 4. Freeze Exceptions

The following MAY change after freeze without a second architecture review:

- **EXPERIMENTAL APIs** (listed above) may change, be removed, or be replaced
- `CookieManager` may gain new methods (adding is not breaking)
- `WebSocketInfo` may gain new fields (adding optional fields is not breaking)
- Error messages in `NetworkError` variants may be refined for clarity
- HAR types behind `#[cfg(feature = "har")]` may gain optional fields for backend-specific data
- Performance optimization may change internal APIs (not public)
