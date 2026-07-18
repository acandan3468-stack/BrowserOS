# Architecture Invariants — v2 (Canonical)

**Status:** LIVE — Code-verified invariants for BrowserOS.  
**Supersedes:** .vibe/workspace/plans/architecture-invariants.md  

---

## Legend

| Tag | Meaning |
|-----|---------|
| ✅ KEEP | Invariant is valid and enforced |
| ⚠️ UPDATE | Invariant needs modification to match code |
| ❌ REMOVE | Invariant is not applicable or not enforced |
| ➕ NEW | New invariant derived from code analysis |

---

## Data Flow

### INV-001: Components communicate through events. (✅ KEEP)
Runtime components communicate through the EventBus. Direct method calls between runtime components are avoided. *Exception: utility crates (browseros-types) define shared types but contain no runtime logic.*

**Status:** Partially enforced. EventBus exists but lifecycle, scheduler, and runtime don't use it for inter-component communication.

### INV-002: Events are immutable after publication. (✅ KEEP)
Once published via `EventBus::publish`, no component may modify an event. Handlers receive read-only references.

**Status:** Enforced by Rust's ownership model.

### INV-003: Messages carry correlation_id and causation_id. (✅ KEEP)
`correlation_id` traces an entire operation. `causation_id` records causality. Both are mandatory in `MessageEnvelope`.

**Status:** Enforced by `MessageEnvelopeBuilder`.

### INV-004: Time comes from injectable Clock. (⚠️ UPDATE)
**Old:** No component calls `std::time::Instant::now()`, `tokio::time::sleep`, or `chrono::Utc::now()` directly.

**Updated:** Components SHOULD use the `Clock` trait from `RuntimeContext` for testability. However, `EventMetadata::new()` and `MessageEnvelopeBuilder::build()` currently use `Utc::now()` directly. This is a known technical debt item.

**Status:** Partially enforced. `Clock` trait exists and is used in some places, but not universally.

### INV-005: No shared mutable state. (✅ KEEP)
All shared state is behind `Arc<dyn Trait>` with interior mutability (`RwLock`, `Mutex`). No component holds `&mut` to shared state.

**Status:** Enforced by design.

### INV-006: State is derived from events. (❌ REMOVE)
**Old:** Event Store is the system of record. State Store is a derived cache.

**Reason for removal:** No EventStore or StateStore exists. `browseros-storage` is a stub. This invariant describes a future state, not current reality.

**Replaced by:** INV-033 (below).

---

## Module Boundaries

### INV-007: Public APIs are backward compatible. (✅ KEEP)
Breaking changes require a major version bump. Adding items is always safe.

**Status:** Convention, not enforced.

### INV-008: Every public API has a doc comment. (✅ KEEP)
No `pub` item exists without a doc comment.

**Status:** Mostly followed. Some items missing docs.

### INV-009: Single public facade module per crate. (✅ KEEP)
Top-level `lib.rs` re-exports only the intended public API. Internal modules use `pub(crate)`.

**Status:** Enforced by crate design.

### INV-010: No crate depends on another's internal modules. (✅ KEEP)
Cross-crate visibility is always through public API.

**Status:** Enforced by Rust's visibility rules.

### INV-011: Components never own other components. (✅ KEEP)
Ownership is through `Arc` references. `LifecycleManager` orchestrates start/stop but does not own components.

**Status:** Enforced by design. Exception: `BrowserProcess` owns the OS child process.

---

## Testing

### INV-012: Every component is independently testable. (✅ KEEP)
External dependencies are traits. Tests can replace any dependency with a mock.

**Status:** Mostly followed. Some crates (storage) have no tests.

### INV-013: Integration tests use real implementations. (⚠️ UPDATE)
**Old:** Unit tests use mocks; integration tests use real implementations.

**Updated:** Integration tests SHOULD use real implementations where feasible. However, CDP and browser tests use mocks because real browser endpoints are not available in CI.

**Status:** Partially followed. No workspace-level integration tests exist.

### INV-014: State machines validate illegal transitions. (✅ KEEP)
Invalid state transitions are caught at compile time or via explicit error returns.

**Status:** Enforced in all state machines (LifecycleState, BrowserState, PageState, ComponentState).

### INV-015: Time-dependent tests use MockClock. (⚠️ UPDATE)
**Old:** Tests must not use `tokio::time::sleep`.

**Updated:** Time-dependent tests SHOULD use `MockClock` from `browseros-types::clock`. Real-time waits make tests slow and flaky.

**Status:** `MockClock` exists but is not universally used.

---

## Execution

### INV-016: Long-running operations support cancellation. (✅ KEEP)
Operations must periodically check `CancellationToken`. On cancellation, clean up and return promptly.

**Status:** `CancellationToken` exists. Not universally checked.

### INV-017: Async operations have configurable timeouts. (⚠️ UPDATE)
**Old:** No `tokio::spawn` or `Scheduler::schedule` runs without an associated timeout.

**Updated:** Operations SHOULD have configurable timeouts. The Scheduler and CDP connection have timeouts. Not universal.

**Status:** Partially enforced.

### INV-018: No blocking I/O in async contexts. (✅ KEEP)
Blocking operations run on `tokio::task::spawn_blocking`.

**Status:** No async runtime in core crates. Vacuously true.

### INV-019: Panics never cross component boundaries. (⚠️ UPDATE)
**Old:** Every async task catches panics and converts to errors.

**Updated:** Production code MUST NOT use `unwrap()`, `expect()`, or `panic!()` in error paths. Builder methods should return `Result` instead of panicking.

**Status:** ❌ VIOLATED — ~120 `unwrap()`, ~50 `expect()`, 4 `panic!()` in production code.

---

## Observability

### INV-020: Every component exposes metrics. (⚠️ UPDATE)
**Old:** At minimum: operation count, error count, latency histogram.

**Updated:** Core runtime components SHOULD expose metrics. Currently, only `browseros-observability` has metrics infrastructure. No other crate exposes metrics.

**Status:** ❌ VIOLATED — No metrics in most crates.

### INV-021: Every component produces trace spans. (⚠️ UPDATE)
**Old:** At minimum: one span per public method.

**Updated:** Components SHOULD produce trace spans. Currently, only `browseros-observability` has tracer infrastructure.

**Status:** ❌ VIOLATED — No tracing in most crates.

### INV-022: Errors are never silently swallowed. (⚠️ UPDATE)
**Old:** Every `Result::Err` is handled. Minimum handling is logging.

**Updated:** Errors MUST be handled. `unwrap()`/`expect()` are NOT acceptable error handling — they crash the process.

**Status:** ❌ VIOLATED — Heavy unwrap/expect usage.

### INV-023: Configuration changes are observable. (❌ REMOVE)
**Reason for removal:** No config reload mechanism exists. No `ConfigChanged` event is emitted. This describes a future feature.

---

## Resource Management

### INV-024: Components respect resource quotas. (❌ REMOVE)
**Reason for removal:** No resource tracking implemented. No quotas exist.

### INV-025: Resources released in reverse acquisition order. (✅ KEEP)
Resources use RAII wrappers. Manual `release()` is an anti-pattern.

**Status:** Enforced by Rust's RAII model.

---

## Plugin & Extension

### INV-026: Plugins never access internal APIs. (✅ KEEP)
**Status:** No plugin system exists (vacuously true).

### INV-027: Capability registration is explicit. (❌ REMOVE)
**Reason for removal:** No CapabilityRegistry exists.

---

## Configuration

### INV-028: Default config produces a working system. (✅ KEEP)
**Status:** Default configs provided for all components.

### INV-029: Config keys use dot notation with BROWSEROS_ prefix. (✅ KEEP)
**Status:** Enforced by convention.

---

## Evolution

### INV-030: Experimental features gated behind Cargo features. (⚠️ UPDATE)
**Status:** Partially followed. Not systematic.

### INV-031: System degrades gracefully. (⚠️ UPDATE)
**Old:** Failure of any single component must not crash the entire runtime.

**Updated:** The system SHOULD degrade gracefully. Currently, panics from `unwrap()`/`expect()` crash the process.

**Status:** ❌ VIOLATED — Panics cause full crash.

### INV-032: Thread safety is explicit. (✅ KEEP)
All public traits require `Send + Sync`.

**Status:** Enforced.

---

## New Invariants

### ➕ INV-033: Storage is explicitly a stub. (NEW)
`browseros-storage` provides only `StorageManager` scaffolding. No EventStore, StateStore, or persistence exists. Future implementations must not break the existing API.

### ➕ INV-034: DOM crate has no EventBus dependency. (NEW)
`browseros-dom` defines event payload types only. Upper layers wire events to EventBus. This keeps DOM a pure leaf crate.

### ➕ INV-035: Bridge traits are protocol-agnostic. (NEW)
No CDP type leaks into `browseros-bridge` or any crate except `browseros-cdp`. Alternative backends (Playwright, WebDriver BiDi) can be added without changing consumer code.

### ➕ INV-036: No cyclic dependencies between Phase 1 and Phase 2 crates. (NEW)
Phase 1 runtime crates (event-bus, lifecycle, scheduler, runtime) must not depend on Phase 2 browser crates (bridge, cdp, browser, page, dom).

---

## Summary

| Status | Count |
|--------|-------|
| ✅ KEEP | 18 |
| ⚠️ UPDATE | 10 |
| ❌ REMOVE | 5 |
| ➕ NEW | 4 |
| **Total** | **37** |