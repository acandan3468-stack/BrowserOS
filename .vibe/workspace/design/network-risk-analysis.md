# Network Risk Analysis

**Status:** DESIGN — Risk analysis defined. Not yet implemented.

---

## 1. Architectural Risks

### R1: Request Flood — Unbounded Tracker Memory

**Risk:** If a page makes thousands of requests (e.g., an ad-heavy site, streaming metrics endpoint, or page with many resources), the crate's internal `RequestTracker` storage grows without bound. Each tracker holds URL strings, header maps, timing data, and redirect chains.

**Severity:** High

**Likelihood:** High — modern web pages routinely make 100+ requests. Streaming/live pages can make thousands.

**Mitigation:**
- Trackers are removed when the request completes/fails/aborts. Only in-flight requests consume tracker memory.
- An optional `max_tracked_requests` configuration limits the total number of stored completed-trackers for HAR export purposes.
- Completed trackers beyond the limit are dropped (HAR export must be triggered before the limit is reached).
- Consumers who need full capture configure `max_tracked_requests` appropriately or export HAR incrementally.

### R2: Interception Rule Priority Ambiguity

**Risk:** Multiple interception rules may match the same request. The order of rule evaluation is backend-defined (CDP evaluates rules in registration order). If the consumer registers rules A and B, and both match, the result depends on the backend's internal ordering, which may differ across backends.

**Severity:** Medium

**Likelihood:** Medium — consumers with complex interception setups may register overlapping rules.

**Mitigation:**
- Document that rules are evaluated in registration order on CDP; other backends may differ.
- Rules are intentionally simple (url_pattern + resource_types + action). Complex interception logic should be implemented by the consumer via a single catch-all rule and a callback.
- Future improvement: add `InterceptionRule::priority: u32` field for explicit ordering.

### R3: Cross-Origin Request Attribution

**Risk:** Requests from cross-origin iframes may not carry accurate `FrameId` attribution. The backend may report these requests as originating from the main frame or with a null frame ID, depending on security policies.

**Severity:** Low

**Likelihood:** Medium — cross-origin iframes are common.

**Mitigation:**
- `RequestInfo.frame_id` is `Option<FrameId>`. Missing attribution is handled gracefully.
- The DOM crate's `FrameHandle.is_cross_origin()` can cross-reference with network events.
- Documentation warns that cross-origin request attribution may be incomplete.

### R4: Backend Event Stream Overflow

**Risk:** The backend (CDP) can emit network events faster than the consumer can process them. The event subscriber in the bridge layer may fall behind, leading to dropped events or unbounded event queue growth.

**Severity:** Medium

**Likelihood:** Low — network events on a typical page are not high-frequency enough to overflow. High-frequency scenarios (WebSocket messaging at 1000 msg/s) are possible.

**Mitigation:**
- Event processing is synchronous in the bridge layer (no separate event queue).
- The synchronous API naturally backpressures the backend's event emission.
- If profiling shows event loss, an internal bounded event ring buffer can be added.
- WebSocket message events are coalesced: `WebSocketMessage` carries individual messages; no additional buffering at the network crate level.

### R5: NavigationId Correlation Gap

**Risk:** Navigation event payloads carry `navigation_id: Option<NavigationId>`, but the bridge may not always populate this field. If the bridge does not expose CDP `navigationId` in network events, the `NavigationId` will always be `None`, making page-network navigation correlation impossible through this field. Consumers must fall back to `url + frame_id + request_id` heuristics.

**Severity:** Medium

**Likelihood:** Medium — CDP's `Network.requestWillBeSent` includes `navigationId` in recent versions, but the bridge may not surface it. Firefox WebDriver BiDi may not provide it at all.

**Mitigation:**
- `navigation_id` is `Option<NavigationId>`. A `None` value is handled gracefully — consumers can still correlate by URL, frame ID, and request ID.
- Documented as best-effort correlation only.
- When the bridge adds reliable `NavigationId` support, consumers get strengthened correlation without API changes.

---

## 2. Implementation Risks

### I1: Synchronous API Hidden Async Cost

**Risk:** The synchronous API (`NetworkHandle::set_conditions()`, `add_interception_rule()`, etc.) blocks the calling thread while the backend processes the operation. For CDP, this means a CDP command round-trip (send → wait → receive), which can take 1-100ms. If consumers call network operations in tight loops, thread blocking accumulates.

**Severity:** Low

**Likelihood:** Low — network operations are not frequent enough to cause thread starvation.

**Mitigation:**
- Network operations are inherently low-frequency (condition changes, interception registration, cookie access).
- Request monitoring is event-driven, not poll-based. No synchronous wait.
- The CDP transport thread handles network event dispatch asynchronously.

### I2: Interception Callback Thread Safety

**Risk:** If interception callbacks are added (e.g., a closure that receives `RequestInterceptedPayload` and returns an action), the callback must be `Send + Sync` and must not panic. A panicking callback would corrupt the backend state.

**Severity:** Medium

**Likelihood:** Low — callbacks are not part of the Phase 2.6 design. If added later, the risk must be managed.

**Mitigation:**
- Phase 2.6 uses a rule-based interception model (`InterceptionRule` + `InterceptionAction`), not callbacks. Rules are data, not closures. No thread safety risk.
- If callbacks are added in a future phase, they must be `Fn + Send + Sync + 'static` and wrapped in `std::panic::AssertUnwindSafe`.
- Documentation will warn that panicking callbacks may leave the backend in an inconsistent state.

### I3: Cookie Serialization Differences

**Risk:** The `Cookie` struct from `browseros-bridge` may not capture all cookie attributes that a particular backend supports (e.g., Chrome's `Partitioned` attribute, `SameParty`). When round-tripping cookies through `CookieManager::set()` and `CookieManager::all()`, attributes may be lost.

**Severity:** Low

**Likelihood:** Medium — backend-specific cookie attributes exist.

**Mitigation:**
- The bridge `Cookie` struct has the most common attributes. Rare attributes are not exposed.
- `CookieManager::set()` passes the cookie to the backend, which may ignore unsupported attributes.
- If a backend-specific attribute is critical, consumers can use `NetworkHandle::backend_port()` (`pub(crate)`) — but this is strongly discouraged.
- Future bridge revisions can add fields to `Cookie` with `#[serde(default)]` for backward compatibility.

---

## 3. Protocol Risks

### P1: WebSocket Message Coalescing

**Risk:** CDP emits WebSocket messages as individual events (`Network.webSocketFrameReceived`, `Network.webSocketFrameSent`). Non-CDP backends may batch messages or emit them differently, causing inconsistent WebSocket event streams.

**Severity:** Low

**Likelihood:** Medium — WebSocket APIs vary across backends.

**Mitigation:**
- The `WebSocketMessage` struct carries individual messages. Batching is not assumed.
- WebSocket events are informational only. Consumers should not depend on 1:1 correspondence with backend frames.
- If a backend batches messages, the bridge layer can split the batch into individual `WebSocketMessage` values.

### P2: HAR Compatibility with Non-CDP Backends

**Risk:** HAR 1.2 assumes a specific set of timing fields (`dns`, `connect`, `ssl`, `send`, `wait`, `receive`). Non-CDP backends may not provide all these fields with the same precision or at all.

**Severity:** Low

**Likelihood:** Medium — HAR is CDP-centric.

**Mitigation:**
- The `HarTimings` struct mirrors the `TimingInfo` bridge type. Missing fields default to `-1` per HAR 1.2 spec (indicating the timing is not available).
- HAR export is gated behind `#[cfg(feature = "har")]`. Not a core concern.
- Documentation notes that HAR completeness depends on backend support.

### P3: InterceptionAction::Respond Body Size Limits

**Risk:** The `InterceptionAction::Respond` variant carries a `Vec<u8>` body. CDP allows response bodies up to ~50MB. The `Vec<u8>` allocation for large bodies (images, videos) may cause OOM if consumed naively.

**Severity:** Medium

**Likelihood:** Low — typical interception use cases modify API responses (small JSON), not media.

**Mitigation:**
- Document that response bodies should be kept small for interception. Large responses should be served through the actual server.
- Consumers who need to modify large responses should use a local proxy (external to BrowserOS).
- Future improvement: add a body size limit with a configurable maximum.

---

## 4. Concurrency Risks

### C1: RequestId Collision

**Risk:** `RequestId` is UUID v7, which is time-ordered and unique. Collision probability is negligible (`2^122` unique values). However, if the system clock jumps backward (NTP adjustment, VM pause), UUID v7 generation may temporarily produce non-unique IDs.

**Severity:** Low

**Likelihood:** Low — UUID v7 handles clock rewinds with monotonicity guarantees within the same timestamp tick.

**Mitigation:**
- UUID v7 includes 74 random bits in addition to the timestamp. Clock rewind would need to produce the exact same random sequence — astronomically unlikely.
- The `uuid` crate's `Uuid::now_v7()` handles sub-millisecond monotonicity within the same process.

### C2: Concurrent Interception Rule Modification

**Risk:** If two threads simultaneously add and remove interception rules, the rule set could be in an inconsistent state during the operation. The backend `NetworkPort` serializes calls through the `Arc`, but the rule set on the backend side is not transactional — an add followed by a remove on the same rule may see a stale rule list.

**Severity:** Low

**Likelihood:** Low — interception rule modification is rare and typically single-threaded.

**Mitigation:**
- All `NetworkPort` calls are serialized through `Arc<dyn NetworkPort>`. The caller sees a consistent view of the operation's result.
- The backend's rule set consistency is a backend concern. CDP handles rule registration atomically per command.
- Consumers who modify rules concurrently should use external synchronization (the caller's responsibility, not the crate's).

---

## 5. Future Compatibility Risks

### F1: HTTP/2 and HTTP/3 Differences

**Risk:** HTTP/2 and HTTP/3 multiplex requests over a single connection, which changes request timing semantics. `TimingInfo` fields like `connect_ms` and `ssl_ms` apply to the connection, not individual requests. A single connect timing may be reported for all multiplexed requests on the same connection.

**Severity:** Low

**Likelihood:** High — HTTP/2 and HTTP/3 are increasingly common.

**Mitigation:**
- `TimingInfo` maps to the same fields used by CDP's `Network.loadingFinished` and `Network.responseReceived` events, which already handle HTTP/2. The timing values for multiplexed requests reflect connection-level timing where appropriate.
- The HAR format already accounts for multiplexed connections (the `connect` and `ssl` timings may be 0 for requests on an already-established connection).
- No action needed now. Monitor as HTTP/3 adoption increases.

### F2: Browser Extension and Service Worker Requests

**Risk:** Requests initiated by browser extensions or service workers may appear as page-level requests with confusing or incorrect attribution (e.g., `frame_id` pointing to a service worker's context, not a visible frame).

**Severity:** Low

**Likelihood:** Low — most consumer use cases don't involve extensions or service workers.

**Mitigation:**
- All request events carry `resource_type` which distinguishes service worker requests (`ResourceType::Fetch` or `ResourceType::Other`).
- Consumers can filter by `frame_id` to ignore extension/service worker requests.
- Documentation notes that extension requests may appear and should be filtered if not relevant.

### F3: Network Mocking and Replay

**Risk:** Future network mocking/replay functionality may be difficult to integrate if the current design doesn't anticipate it. The interception model (match URL → respond with custom data) is the foundation of mocking, but mocking frameworks typically need pattern matching, state machines, and request recording — which the current design does not provide.

**Severity:** Medium

**Likelihood:** Medium — network mocking is a common testing requirement.

**Mitigation:**
- The interception system is the foundation for future mocking. `InterceptionRule` supports URL patterns and resource type filtering.
- Recorded requests can be exported as HAR entries, which can later be replayed by a future mocking crate.
- Mocking/replay is explicitly classified as EXPERIMENTAL in the freeze plan. It is not frozen. A future crate or plugin can build on the network crate's interception and HAR data without modifying the core APIs.

---

## 6. Risk Summary

| ID | Risk | Severity | Likelihood | Mitigation |
|----|------|----------|------------|------------|
| R1 | Request flood — unbounded tracker memory | High | High | Trackers removed on completion, configurable max_tracked_requests |
| R2 | Interception rule priority ambiguity | Medium | Medium | Documented limitation, rule simplicity, future priority field |
| R3 | Cross-origin request attribution | Low | Medium | Optional frame_id, graceful missing value |
| R4 | Backend event stream overflow | Medium | Low | Synchronous backpressure, future ring buffer |
| R5 | NavigationId correlation gap | Medium | Medium | Option field, graceful None, documented limitation |
| I1 | Synchronous API hidden async cost | Low | Low | Low-frequency operations, event-driven monitoring |
| I2 | Interception callback thread safety | Medium | Low | Rule-based model (no callbacks in Phase 2.6) |
| I3 | Cookie serialization differences | Low | Medium | Common attributes only, future bridge revisions |
| P1 | WebSocket message coalescing | Low | Medium | Individual message model, bridge normalization |
| P2 | HAR compatibility with non-CDP backends | Low | Medium | Default -1 for missing timings, feature-gated |
| P3 | Interception response body size limits | Medium | Low | Documentation, future configurable limit |
| C1 | RequestId collision | Low | Low | UUID v7 randomness, astronomically unlikely |
| C2 | Concurrent interception rule modification | Low | Low | Arc serialization, external sync for multi-rule |
| F1 | HTTP/2 and HTTP/3 differences | Low | High | Already handled by CDP events and HAR format |
| F2 | Extension and service worker requests | Low | Low | ResourceType filtering, frame_id attribution |
| F3 | Network mocking and replay | Medium | Medium | Interception foundation, HAR recording, future crate |

**Overall Risk Level:** Low. No blocking risks identified. All risks have clear mitigations. The highest-severity risk (R1 — request flood) has a straightforward mitigation (tracker cleanup on completion, configurable cap). Risk R5 (NavigationId gap) is a known limitation that does not block implementation.
