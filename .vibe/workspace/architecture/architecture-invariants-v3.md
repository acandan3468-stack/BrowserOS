# Architecture Invariants v3 — BrowserOS

**Status:** LIVE — Code-verified from direct codebase audit  
**Date:** 2026-07-08  
**Supersedes:** architecture-invariants-v2.md  

---

## Legend

| Result | Meaning |
|--------|---------|
| ✅ PASS | Invariant holds in current codebase |
| ⚠️ PARTIAL | Invariant partially holds, exceptions exist |
| ❌ FAIL | Invariant is violated |
| ➕ NEW | New invariant from codebase audit |
| 🗑️ OBSOLETE | Invariant no longer applicable |

---

## Data Flow

### INV-001: Components communicate through events. (⚠️ PARTIAL)
**Check:** EventBus exists. Lifecycle emits LifecycleTransitionEvent. Scheduler emits events.  
**Violation:** RuntimeContext, Config, Storage do not use EventBus for internal communication.  
**Verdict:** PARTIAL — EventBus is used by some components, not all.

### INV-002: Events are immutable after publication. (✅ PASS)
**Check:** Event trait provides read-only access. EventBus passes &dyn Event to handlers.  
**Verdict:** PASS — Enforced by Rust's ownership model.

### INV-003: Messages carry correlation_id and causation_id. (✅ PASS)
**Check:** MessageEnvelope has both fields. MessageEnvelopeBuilder requires them.  
**Verdict:** PASS — Enforced at construction time.

### INV-004: Time comes from injectable Clock. (⚠️ PARTIAL)
**Check:** Clock trait exists. RuntimeContext has clock field.  
**Violation:** EventMetadata::new() calls Utc::now() directly. MessageEnvelopeBuilder::build() calls Utc::now() directly.  
**Verdict:** PARTIAL — Clock available but not universally used.

### INV-005: No shared mutable state. (✅ PASS)
**Check:** All shared state behind Arc<RwLock<>> or Arc<Mutex<>>.  
**Verdict:** PASS — Enforced by design.

### INV-006: State is derived from events. (🗑️ OBSOLETE)
**Reason:** No EventStore exists. browseros-storage is a stub. Event sourcing not implemented.  
**Verdict:** OBSOLETE — Remove. Replaced by INV-033.

---

## Module Boundaries

### INV-007: Public APIs are backward compatible. (⚠️ PARTIAL)
**Check:** No versioning enforcement. No semver checks in CI.  
**Verdict:** PARTIAL — Convention only, not enforced.

### INV-008: Every public API has a doc comment. (⚠️ PARTIAL)
**Check:** Most public items have doc comments. Some missing in browseros-cdp traits.rs.  
**Verdict:** PARTIAL — Mostly followed, some gaps.

### INV-009: Single public facade module per crate. (✅ PASS)
**Check:** All crates follow lib.rs re-export pattern. Internal modules use pub(crate).  
**Verdict:** PASS — Enforced by crate design.

### INV-010: No crate depends on another's internal modules. (✅ PASS)
**Check:** All cross-crate visibility through public API.  
**Verdict:** PASS — Enforced by Rust's visibility rules.

### INV-011: Components never own other components. (✅ PASS)
**Check:** All references through Arc. No unique ownership between components.  
**Exception:** BrowserProcess owns OS child process.  
**Verdict:** PASS — With documented exception.

---

## Testing

### INV-012: Every component is independently testable. (⚠️ PARTIAL)
**Check:** Most crates have tests. browseros-storage has 0 tests.  
**Verdict:** PARTIAL — Storage is untested.

### INV-013: Integration tests use real implementations. (⚠️ PARTIAL)
**Check:** CDP and browser tests use mocks. No workspace-level integration tests.  
**Verdict:** PARTIAL — Mocks used where real endpoints unavailable.

### INV-014: State machines validate illegal transitions. (✅ PASS)
**Check:** LifecycleState, BrowserState, PageState, ComponentState all validate transitions.  
**Verdict:** PASS — All state machines enforce valid transitions.

### INV-015: Time-dependent tests use MockClock. (⚠️ PARTIAL)
**Check:** MockClock exists. Some tests use it. Not universal.  
**Verdict:** PARTIAL — Available but not required.

---

## Execution

### INV-016: Long-running operations support cancellation. (⚠️ PARTIAL)
**Check:** CancellationToken exists. Scheduler checks it. Not universally used.  
**Verdict:** PARTIAL — Available, partial adoption.

### INV-017: Async operations have configurable timeouts. (⚠️ PARTIAL)
**Check:** CDP connection has timeout. Scheduler has no timeout per task.  
**Verdict:** PARTIAL — Some timeouts, not universal.

### INV-018: No blocking I/O in async contexts. (✅ PASS)
**Check:** No async runtime in core crates. CDP reader thread is separate.  
**Verdict:** PASS — Architecture prevents this.

### INV-019: Panics never cross component boundaries. (❌ FAIL)
**Check:** 4 panic!() in production code (MessageEnvelopeBuilder). ~120 unwrap() calls. ~50 expect() calls.  
**Verdict:** FAIL — Production code panics on error paths.

---

## Observability

### INV-020: Every component exposes metrics. (❌ FAIL)
**Check:** Only browseros-observability has metrics infrastructure. No other crate exposes metrics.  
**Verdict:** FAIL — Metrics infrastructure exists but unused by other crates.

### INV-021: Every component produces trace spans. (❌ FAIL)
**Check:** Only browseros-observability has tracer. No spans in other crates.  
**Verdict:** FAIL — Tracing infrastructure exists but unused.

### INV-022: Errors are never silently swallowed. (❌ FAIL)
**Check:** unwrap()/expect() crash the process. Errors are not logged before panic.  
**Verdict:** FAIL — Heavy unwrap/expect usage violates this.

### INV-023: Configuration changes are observable. (🗑️ OBSOLETE)
**Reason:** No config reload mechanism exists. No ConfigChanged event emitted.  
**Verdict:** OBSOLETE — Remove.

---

## Resource Management

### INV-024: Components respect resource quotas. (🗑️ OBSOLETE)
**Reason:** No resource tracking implemented. No quotas exist.  
**Verdict:** OBSOLETE — Remove.

### INV-025: Resources released in reverse acquisition order. (✅ PASS)
**Check:** RAII wrappers used throughout. No manual release() patterns.  
**Verdict:** PASS — Enforced by Rust's RAII model.

---

## Plugin & Extension

### INV-026: Plugins never access internal APIs. (✅ PASS)
**Check:** No plugin system exists. Vacuously true.  
**Verdict:** PASS — No plugins to violate.

### INV-027: Capability registration is explicit. (🗑️ OBSOLETE)
**Reason:** No CapabilityRegistry exists.  
**Verdict:** OBSOLETE — Remove.

---

## Configuration

### INV-028: Default config produces a working system. (✅ PASS)
**Check:** All components have default configs. RuntimeContext::init() works with defaults.  
**Verdict:** PASS — Default configs provided.

### INV-029: Config keys use dot notation with BROWSEROS_ prefix. (✅ PASS)
**Check:** Env vars use BROWSEROS_ prefix. Config keys use dot notation.  
**Verdict:** PASS — Enforced by convention.

---

## Evolution

### INV-030: Experimental features gated behind Cargo features. (⚠️ PARTIAL)
**Check:** Some features exist. Not systematic.  
**Verdict:** PARTIAL — Partial adoption.

### INV-031: System degrades gracefully. (❌ FAIL)
**Check:** Panics from unwrap()/expect() crash the process. No recovery mechanism.  
**Verdict:** FAIL — Panics cause full crash.

### INV-032: Thread safety is explicit. (✅ PASS)
**Check:** All public traits require Send + Sync.  
**Verdict:** PASS — Enforced.

---

## New Invariants (from codebase audit)

### ➕ INV-033: Storage is explicitly a stub. (✅ PASS)
**Check:** browseros-storage has only StorageManager scaffold. No EventStore/StateStore.  
**Verdict:** PASS — Documented limitation.

### ➕ INV-034: DOM crate has no EventBus dependency. (✅ PASS)
**Check:** browseros-dom Cargo.toml has no event-bus dependency.  
**Verdict:** PASS — Verified from Cargo.toml.

### ➕ INV-035: Bridge traits are protocol-agnostic. (✅ PASS)
**Check:** No CDP types in browseros-bridge. All types are generic.  
**Verdict:** PASS — Verified from source.

### ➕ INV-036: No cyclic dependencies between Phase 1 and Phase 2. (✅ PASS)
**Check:** Phase 1 crates do not depend on Phase 2 crates.  
**Verdict:** PASS — Verified from Cargo.toml.

### ➕ INV-037: Scheduler uses std::thread, not tokio. (✅ PASS)
**Check:** browseros-scheduler uses std::thread::spawn for delayed tasks.  
**Verdict:** PASS — Verified from source.

### ➕ INV-038: EventBus subscribes by EventId, not category. (✅ PASS)
**Check:** EventBus::subscribe() takes EventId parameter. No category-based subscription.  
**Verdict:** PASS — Verified from source. This is a design limitation, not a bug.

### ➕ INV-039: LifecycleManager has no ManagedComponent trait. (✅ PASS)
**Check:** LifecycleManager tracks string-keyed states. No trait-based component management.  
**Verdict:** PASS — Verified from source. This is a simplification.

---

## Summary

| Result | Count |
|--------|-------|
| ✅ PASS | 18 |
| ⚠️ PARTIAL | 10 |
| ❌ FAIL | 4 |
| 🗑️ OBSOLETE | 5 |
| ➕ NEW | 7 |
| **Total** | **39** |

### Failed Invariants (blockers)

| Invariant | Issue |
|-----------|-------|
| INV-019 | Panics cross boundaries (unwrap/expect/panic! in production) |
| INV-020 | No metrics in most crates |
| INV-021 | No tracing in most crates |
| INV-031 | No graceful degradation (panics crash process) |