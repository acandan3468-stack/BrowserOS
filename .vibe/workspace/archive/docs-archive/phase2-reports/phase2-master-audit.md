ARCHIVED

This document has been archived. Its content has been merged into the canonical documentation set.
See docs/ for current information.

---

# Phase 2 — Master Architecture Audit

**Date:** 2026-07-01 (initial), 2026-07-02 (RC cleanup)  
**Scope:** All Phase 2 crates: `browseros-types`, `browseros-bridge`, `browseros-cdp`, `browseros-browser`, `browseros-page`, `browseros-dom`, `browseros-storage`
**Audit Type:** System-wide integration audit
**Status:** RC cleanup complete — all frozen crates implemented, documented, and tested

> **Note:** This document reflects the initial audit before RC cleanup. For the final release
> status, see `phase2-release-report.md`.

---

## 1. Executive Summary

The Phase 2 architecture is well-designed but **not yet production-ready**. The design documents are consistent, layered, and follow clear principles (bridge abstraction, synchronous API, event-driven observation). However, a significant gap exists between the **frozen architecture** and the **current implementation**.

Of the 6 crates under audit, only 3 exist as source code: `browseros-types` (100% complete), `browseros-bridge` (100% trait definitions, 0% implementations), and `browseros-dom` (~85% implemented, 6 design-only gaps). Three crates have **zero implementation**: `browseros-events` (non-existent as a crate; event infra lives in `browseros-types` + `browseros-event-bus`), `browseros-storage` (non-existent), and `browseros-network` (design frozen, no code). The CDP trait implementations have 15+ stubbed methods that return hardcoded values or `NotImplemented`.

**Overall Verdict: REVISION REQUIRED** — not for the architecture design (which passes), but for the implementation readiness assessment. Production release requires closing the implementation gaps and resolving the discrepancies documented below.

---

## 2. Dependency Graph Review

### 2.1 Documented Dependency Graph

```
browseros-types
  ↑
browseros-bridge (depends only on types)
  ↑
browseros-dom, browseros-network, browseros-storage (depends on types + bridge)
  ↑
browseros-browser (depends on types + bridge + event-bus + observability + config)
  ↑
browseros-runtime (composition root)
```

### 2.2 Actual Dependency Graph (from Cargo.toml)

```
browseros-types       ← all Phase 2 crates
browseros-event-bus   ← depends on types, used by browser, page, runtime
browseros-bridge      ← depends on types + serde + thiserror
browseros-dom         ← depends on types + bridge + chrono + thiserror ✓
browseros-cdp         ← depends on types + bridge + serde + websocket libs
browseros-browser     ← depends on types + bridge + event-bus + observability + config ✓
browseros-page        ← depends on types + bridge + event-bus + chrono
browseros-runtime     ← depends on everything except stress-tests
```

### 2.3 Audit Findings

| Finding | Severity | Detail |
|---------|----------|--------|
| **G1 — Missing crates** | HIGH | `browseros-storage`, `browseros-network`, `browseros-events` (as standalone crate) do not exist. The documented dependency graph references non-existent nodes. |
| **G2 — Crate present but not in docs** | LOW | `browseros-page` exists as a working crate but the phase2-crate-map lists it under "crate in design" — this crate has real implementation with PageWaiter, FrameTree, etc. |
| **G3 — No cyclic dependencies** | PASS | Verified: no crate depends on `browseros-runtime`. Dependency direction is strictly downward. |
| **G4 — 3 doc-only crates** | MEDIUM | `browseros-input`, `browseros-artifact`, `browseros-plugin` exist only as design documents. Not production-ready. |
| **G5 — browseros-network not in workspace** | HIGH | Despite Phase 2.6 design being frozen, `browseros-network` is not listed in workspace Cargo.toml and has no directory. |

---

## 3. Cross-Crate API Review

### 3.1 Trait Consistency

All 12 bridge traits are fully defined in `browseros-bridge` with complete method signatures. The trait definitions match the `browser-abstractions.md` document with one discrepancy:

| Documented | Actual | Severity |
|------------|--------|----------|
| `StoragePort::get_cookies()` | `StoragePort::cookies()` (no `get_` prefix) | LOW — naming inconsistency |
| `StoragePort::get_local_storage()` | `StoragePort::local_storage()` (no `get_` prefix) | LOW — naming inconsistency |
| `PagePort::frames()` returns `Vec<Box<dyn FramePort>>` | Same in code | PASS |
| `BrowserPort::is_alive()` documented | Present in actual trait | PASS |

### 3.2 Missing BackendFactory Implementation

`CdpBrowserProcess` implements `BrowserPort` directly but does **not** implement the `BackendFactory` trait that `browseros-browser`'s `BackendRegistry` expects. The bridge between `browseros-cdp` and `browseros-browser` is incomplete:

- `BackendFactory::launch()` — not implemented (CDP's `launch()` returns `NotImplemented`)
- `BackendFactory::connect()` — not implemented
- `BackendFactory::name()` — not implemented

The `browseros-browser` crate creates `BrowserInstance` objects via `BackendFactory`, but no type satisfies this contract. This is a **critical integration gap**.

### 3.3 Identifier Location Fragmentation

| Identifier | Documented Home | Actual Home | Issue |
|------------|----------------|-------------|-------|
| `HandleId` | `browseros-types` | `browseros-types` | PASS |
| `PageId` | `browseros-types` | `browseros-bridge::identifiers` | Fragmented |
| `FrameId` | `browseros-types` | `browseros-bridge::identifiers` | Fragmented |
| `ElementId` | `browseros-types` | `browseros-bridge::identifiers` | Fragmented |
| `NodeId` | `browseros-types` | `browseros-bridge::identifiers` | Fragmented |
| `NavigationId` | `browseros-types` | `browseros-bridge::identifiers` | Fragmented |
| `SessionId` | `browseros-types` | `browseros-bridge::identifiers` | Fragmented |
| `BrowserId` | `browseros-types` | `browseros-bridge::identifiers` | Fragmented |
| `RequestId` | `browseros-network` | Not yet created (Phase 2.6 design only) | Not applicable |

The `phase2-crate-map.md` documented all identifiers as belonging to `browseros-types`, but the actual implementation placed them in `browseros-bridge::identifiers`. This is a valid architectural choice (identifiers where they're used), but the docs are out of date.

---

## 4. Ownership & Lifetime Review

### 4.1 Ownership Patterns

| Pattern | Used By | Status |
|---------|---------|--------|
| `Arc<dyn Port>` for backend | All handles (BrowserHandle, SessionHandle, ElementHandle, etc.) | PASS — consistent |
| `Clone + Send + Sync` for handles | All consumer-facing types | PASS |
| `#[non_exhaustive]` on enums | `DomError`, `NetworkError`, `NetworkEvent`, `ModuleType` | PASS |
| Generation-based stale detection | `ElementHandle` (known_generation vs. generation) | PASS — well-designed |
| No stale detection | `NetworkHandle` (documented: "network operations don't have a concept of stale") | PASS — intentional |
| Internal-only types | `RequestTracker`, `ResponseTracker` | PASS — consistent after Phase 2.6 freeze |

### 4.2 Lifetime Gaps

| Gap | Severity | Detail |
|-----|----------|--------|
| **L1 — FrameHandle generation coordination** | MEDIUM | `FrameHandle` has its own `generation: Arc<AtomicU64>` but it's NOT shared with `ElementHandle` generation. A frame navigation invalidates frame handles but not element handles within that frame. Cross-tracking is missing. |
| **L2 — PageHandle lifecycle** | MEDIUM | `PageHandle` in `browseros-browser` does NOT have a generation counter. If a page is navigated, element handles obtained from the old page document are not invalidated through the page handle — they rely on their own generation check. |
| **L3 — Session lifetime** | LOW | `SessionHandle` does not expose `is_closed()` or `is_active()` — consumers must call a method and handle errors to detect session closure. |
| **L4 — No cross-crate RAII orchestration** | LOW | The `RuntimeContext::drop()` shutdown sequence (Plugin stop → Browser close → Lifecycle → Scheduler → EventBus) exists only as documentation, not as code. Rust's drop order for `Arc`-wrapped fields is not guaranteed. |

---

## 5. Event Flow Review

### 5.1 Event Architecture

The event system has three layers:
1. **Core event infrastructure** — `Event` trait, `EventMetadata`, `EventCategory` in `browseros-types`
2. **EventBus** — pub/sub implementation in `browseros-event-bus`
3. **Domain events** — event payloads in individual crates (`browseros-browser::events`, `browseros-dom::events`)

### 5.2 Event Duplication Across Domains

Several events are defined in **multiple places** with different field sets or names:

| Event Name | Defined In | Notes |
|------------|-----------|-------|
| `NavigationStarted` | `event-model.md`, `browseros-browser::events` | Different field sets |
| `NavigationFinished` | `event-model.md`, `browseros-browser::events`, `page` crate | `event-model.md` uses `load_time_ms`, browser crate does not |
| `NavigationRequested/Committed/Completed` | `network-event-model.md` | Network crate uses 3 events vs. 3 for page crate |
| `RequestStarted` | `event-model.md`, `network-event-model.md` | Central model uses `request_id: String`, network model uses `RequestId` (UUID v7) |
| `ElementAttached` | `event-model.md`, `browseros-dom::events` (via DomEvent) | Different representations (flat struct vs. enum variant) |
| `FrameAttached` | `event-model.md`, `browseros-browser::events` | Same structure |

### 5.3 Critical Event Type Mismatch

The central `event-model.md` defines `RequestStarted.request_id: String` while the `network-event-model.md` defines `RequestId` as a UUID v7 newtype. This type mismatch means the central event catalog is **incompatible** with the network event model. Agents subscribing to `network.request_started` from the central model expect a `String` but the network crate would produce a typed `RequestId`.

### 5.4 Event Emission Architecture

| Aspect | Status | Issue |
|--------|--------|-------|
| EventBus dependency in domain crates | PASS — DOM crate has NO EventBus dep (events are payload-only) | Intentional, clean |
| EventBus dependency in browser crate | PASS — publishes `BrowserStarted`, `SessionCreated`, etc. | Expected |
| Event correlation | PASS — `EventMetadata` includes `causation_id` and `correlation_id` | Well-designed |
| No guaranteed delivery order | PASS — documented as a design rule | Clear contract |

---

## 6. Architectural Risks

### 6.1 Risks from Design Docs (verified)

| ID | Risk | Severity | Status | Mitigation |
|----|------|----------|--------|------------|
| R1 | CDP monolith | MEDIUM | **Confirmed** — 12 modules in CDP crate, already approaching the 20-module signal | Bridge layer allows sub-crate split |
| R2 | Trait proliferation | MEDIUM | **Confirmed** — 12 bridge traits, approaching 15-trait signal | Sticking to documented freeze |
| R3 | Sync API deadlock | CRITICAL | **Confirmed** — no watchdog thread implemented yet | Design rule only, no code |
| R4 | CDP connection state machine | HIGH | **Confirmed** — `CdpConnection` state check partial | `is_connected()` exists |
| R5 | Element handle staleness | HIGH | **Confirmed** — generation-based detection works | Implemented in DOM crate |
| R6 | CDP event flooding | MEDIUM | **Confirmed** — no bounded channel in CDP reader | Event dispatcher exists but no throttling |

### 6.2 New Risks Discovered

| ID | Risk | Severity | Detail |
|----|------|----------|--------|
| **A1 — BackendFactory bridge gap** | CRITICAL | `CdpBrowserProcess` implements `BrowserPort` but not `BackendFactory`. The browser crate cannot register CDP as a backend. Zero production browsers can be launched. |
| **A2 — WebSocket transport missing** | CRITICAL | `CdpTransport` has `NullTransport` and `MockTransport` only. No real WebSocket I/O exists. CDP connections cannot exchange messages. |
| **A3 — Event type system split** | HIGH | Central event catalog uses `String`-typed IDs but domain crates use UUID v7 newtypes. Downstream consumers cannot know which type to expect. |
| **A4 — Stubbed element detection** | HIGH | `CdpElement::is_visible()`, `is_enabled()`, `is_checked()`, `is_selected()`, `is_stable()` all return hardcoded values. DOM state detection is non-functional. |
| **A5 — Frame tracking missing** | HIGH | `CdpPage::frames()` returns empty, `CdpPage::url()` returns "". No child frame tracking exists. Multi-frame page support is non-functional. |
| **A6 — Stale element detection incomplete** | MEDIUM | CDP backend returns errors for stale elements but the DOM crate's generation-based detection is a frontend-only mechanism. No backend-to-frontend generation synchronization exists. |
| **A7 — Dialog and download ports non-functional** | MEDIUM | `CdpDialogPort::next()` returns `None` always. `CdpDownloadPort` is entirely `NotImplemented`. |
| **A8 — No cross-crate EventBus integration test** | MEDIUM | No test verifies that events emitted by `browseros-browser` flow correctly through `EventBus` and are received by subscribers. |
| **A9 — Three design-only crates not workspace members** | MEDIUM | `browseros-input`, `browseros-artifact`, `browseros-storage`, `browseros-plugin` are defined in docs but absent from workspace Cargo.toml. |

---

## 7. Technical Debt

### 7.1 Documentation Debt

| Item | Detail | Impact |
|------|--------|--------|
| D1 — `phase2-crate-map.md` out of date | Identifiers listed as `browseros-types` but live in `browseros-bridge::identifiers` | Low — docs are stale |
| D2 — Ownership table references old API | `network-lifetime-model.md` still references `ResponseTracker` ownership (was just fixed) | Low — already fixed |
| D3 — Event model duplication | 3 separate event models (central, DOM, network) with incompatible types | High — confusing |
| D4 — Crate map lists 10 crates | 4 of 10 crates (`input`, `storage`, `artifact`, `plugin`) have zero code; 2 more (`network`, `events`) are design-only | Medium — misleading |

### 7.2 Implementation Debt

| Item | Detail | Impact |
|------|--------|--------|
| I1 — 15+ stubbed CDP methods | Element state, frame tracking, interception, dialog, download | High — non-functional |
| I2 — No WebSocket transport | `NullTransport` only; no real CDP communication | Critical — blocks production |
| I3 — No BackendFactory impl | CDP cannot register as a browser backend | Critical — blocks production |
| I4 — No snapshot-diff feature code | Feature flag exists in Cargo.toml but no code uses it | Low — harmless |
| I5 — `FrameHandle::snapshot()` ignores `selector_filter` | Parameter documented but no-op | Medium — misleading API |

---

## 8. Release Readiness Assessment

### 8.1 Freeze Status Summary

| Crate | Design Doc Status | Implementation Status | Freeze Gap |
|-------|-------------------|----------------------|------------|
| `browseros-types` | FROZEN | 100% implemented | NONE |
| `browseros-events` | Design only | Event infra in `types` + `event-bus`; no dedicated crate | MEDIUM — no dedicated crate |
| `browseros-bridge` | SAFE TO FREEZE | 100% trait definitions | LOW — 2 naming discrepancies |
| `browseros-storage` | SAFE TO FREEZE | 0% implemented | HIGH — crate does not exist |
| `browseros-dom` | FROZEN | ~85% implemented | MEDIUM — 6 stubbed/design-only items |
| `browseros-network` | FROZEN (Phase 2.6) | 0% implemented | HIGH — crate does not exist |

### 8.2 Production Readiness by Priority

**Blocking (must fix before production):**
- No WebSocket transport for CDP (A2)
- No BackendFactory implementation for CDP (A1)
- 15+ stubbed CDP bridge methods (A4, A5, A7)
- Missing crates: `browseros-storage`, `browseros-network` (G1, G5)

**High Priority (should fix before production):**
- Event type mismatch between central and domain models (A3)
- Browser process launch non-functional (A1)
- Frame tracking missing (A5)
- Element state detection non-functional (A4)

**Medium Priority (fix before first agent release):**
- Dialog and download ports non-functional (A7)
- Shutdown sequence not implemented as code (L4)
- Generation coordination between FrameHandle and ElementHandle (L1)
- No cross-crate EventBus integration test (A8)

**Low Priority (deferred):**
- `FrameHandle::snapshot()` selector_filter no-op (I5)
- Documentation stale references (D1-D4)
- `snapshot-diff` feature flag unused (I4)

### 8.3. What IS Production-Ready Today

- **`browseros-types`** — fully shipped. All types, identifiers, errors, events, messages are implemented and tested.
- **`browseros-bridge`** — fully shipped as an abstraction layer. All 12 traits and 40+ supporting types defined.
- **`browseros-dom`** — mostly shippable. ElementHandle, Locator, Snapshot, traversal, attributes, style, mutation — all working.
- **`browseros-event-bus`** — fully functional pub/sub. Synchronous, thread-safe, tested.
- **`browseros-browser`** — launching mechanism works (BrowserProcess spawns Chrome). State machine, events, handles all implemented. The gap is CDP registration (A1) not the browser crate itself.
- **`browseros-page`** — PageWaiter, FrameTree, NavigationHistory all functional.

---

## 9. Final Go / No-Go Decision

**NO-GO — Production release is not ready.**

The architecture design is sound and should be frozen. The implementation, however, has critical gaps that prevent any production use:

1. **No browser can be launched** — CDP WebSocket transport is missing; BackendFactory bridge gap means CDP cannot register as a backend.
2. **3 of 6 audited crates have zero code** — `browseros-storage` and `browseros-network` do not exist as crates; `browseros-events` is split across `types` and `event-bus` without a dedicated crate.
3. **Core DOM operations are stubbed** — visibility detection, enabled state, frame tracking, dialog handling, download management are all non-functional in the CDP backend.
4. **Event model incompatibility** — central event catalog uses `String`-typed identifiers while domain events use UUID v7 newtypes.

### Path to Go

1. Implement WebSocket transport for `CdpConnection` (minimum: `connect()` → WebSocket I/O → message send/receive)
2. Implement `BackendFactory` for `CdpBrowserProcess` so the browser crate can register it
3. Implement the 15+ stubbed CDP trait methods (element state, frame tracking, interception, dialog, download)
4. Create the missing crates (`browseros-storage`, `browseros-network`) or add them as workspace members
5. Resolve the event type system mismatch (central model vs. domain models)
6. Add cross-crate integration tests for EventBus event flow

**Architecture design confidence: 90/100**
**Implementation readiness: 30/100**
**Documentation consistency: 75/100**

The design deserves its freeze status. The implementation does not.

