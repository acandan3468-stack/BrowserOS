# Technical Debt Report — BrowserOS

**Date:** 2026-07-08  
**Method:** Static analysis of all .rs files in browseros/ workspace  

---

## Severity Legend

| Severity | Meaning |
|----------|---------|
| 🔴 CRITICAL | Must fix before production |
| 🟠 HIGH | Significant risk, should fix soon |
| 🟡 MEDIUM | Moderate concern, plan to fix |
| 🔵 LOW | Minor, fix when convenient |
| ⚪ INFO | Observation, no action needed |

---

## 1. panic!() Calls — 83 occurrences

### Production Code (🔴 CRITICAL)

| File | Line | Code | Risk |
|------|------|------|------|
| browseros-types/src/message.rs | 327 | `panic!("MessageEnvelopeBuilder: source is required")` | Builder panics instead of returning Result |
| browseros-types/src/message.rs | 338 | `panic!("MessageEnvelopeBuilder: content_type is required")` | Builder panics instead of returning Result |
| browseros-types/src/message.rs | 349 | `panic!("MessageEnvelopeBuilder: payload is required")` | Builder panics instead of returning Result |
| browseros-types/src/identifiers.rs | 24 | `panic!("not implemented")` in ModuleId::default() | Unreachable but dangerous |
| browseros-dom/src/events.rs | ~line 200 | `unsafe { ... }` with transmute | 1 unsafe block in production code |

**Verdict:** 3 builder panics in browseros-types are the most dangerous — any missing field causes a runtime panic instead of a proper error. This violates INV-022 (Errors are never silently swallowed).

### Test Code (🟡 MEDIUM)

- browseros-cdp: ~40 panic! calls in tests (expected for test assertions)
- browseros-config: ~6 panic! calls in tests
- browseros-observability: ~5 panic! calls in tests
- browseros-types: ~15 panic! calls in tests
- browseros-dom: ~5 panic! calls in tests
- browseros-bridge: ~5 panic! calls in tests
- browseros-browser: ~3 panic! calls in tests
- browseros-page: ~2 panic! calls in tests

**Note:** Test panics are acceptable, but the high count suggests tests use `panic!` instead of `assert!`/`assert_eq!` in many cases.

---

## 2. unwrap() Calls — ~120 occurrences

### Production Code (🔴 CRITICAL)

| File | Approx Count | Pattern |
|------|-------------|---------|
| browseros-cdp/src/connection.rs | ~15 | `result.unwrap()` on transport operations |
| browseros-cdp/src/session.rs | ~10 | `result.unwrap()` on command execution |
| browseros-cdp/src/backend.rs | ~8 | `option.unwrap()` on browser info |
| browseros-cdp/src/transport_ws.rs | ~5 | `result.unwrap()` on WebSocket operations |
| browseros-browser/src/manager.rs | ~8 | `result.unwrap()` on process operations |
| browseros-browser/src/process.rs | ~5 | `result.unwrap()` on OS commands |
| browseros-page/src/waiter.rs | ~5 | `result.unwrap()` on wait conditions |
| browseros-config/src/source.rs | ~3 | `result.unwrap()` on file reads |
| browseros-observability/src/export.rs | ~3 | `result.unwrap()` on timer operations |
| browseros-runtime/src/lib.rs | ~2 | `result.unwrap()` on builder |

**Verdict:** Heavy unwrap usage in CDP and browser crates means any unexpected error causes a panic. This is the single largest source of potential runtime crashes.

### Test Code (🟡 MEDIUM)

- browseros-cdp/traits.rs: ~30 unwrap() calls in tests
- browseros-page/tests/page_tests.rs: ~15 unwrap() calls
- browseros-browser/tests/: ~10 unwrap() calls
- browseros-types/tests/integration.rs: ~5 unwrap() calls

---

## 3. expect() Calls — ~50 occurrences

### Production Code (🟠 HIGH)

| File | Approx Count | Pattern |
|------|-------------|---------|
| browseros-cdp/src/connection.rs | ~8 | `.expect("reader thread failed")` |
| browseros-cdp/src/session.rs | ~5 | `.expect("command failed")` |
| browseros-cdp/src/backend.rs | ~4 | `.expect("browser launch failed")` |
| browseros-browser/src/manager.rs | ~5 | `.expect("process died")` |
| browseros-browser/src/process.rs | ~3 | `.expect("cannot kill browser")` |
| browseros-config/src/source.rs | ~2 | `.expect("config file missing")` |
| browseros-event-bus/src/lib.rs | ~2 | `.expect("lock poisoned")` |
| browseros-lifecycle/src/lib.rs | ~2 | `.expect("lock poisoned")` |
| browseros-scheduler/src/lib.rs | ~2 | `.expect("lock poisoned")` |
| browseros-runtime/src/lib.rs | ~2 | `.expect("builder failed")` |

**Verdict:** expect() messages are descriptive but still cause panics. Lock poisoning expects are particularly concerning — if a lock is poisoned, the system should recover, not crash.

---

## 4. unsafe Code — 1 occurrence (🟠 HIGH)

| File | Line | Code |
|------|------|------|
| browseros-dom/src/events.rs | ~200 | `unsafe { transmute::<_, _>(...) }` |

**Details:** The DomEvent enum uses `unsafe` transmute for event type conversion. While this may be a performance optimization, it bypasses Rust's type safety guarantees. No `#[deny(unsafe_code)]` lint is set anywhere in the workspace.

**Risk:** If the enum layout changes (variant reordering, field type changes), the transmute will produce undefined behavior without any compiler warning.

---

## 5. Dead Code (🟡 MEDIUM)

| Item | Location | Status |
|------|----------|--------|
| `ComponentManifest` | browseros-types/src/component.rs | Defined but never used by any crate |
| `CapabilityDefinition` | browseros-types/src/component.rs | Defined but never used |
| `ResourceRequirements` | browseros-types/src/component.rs | Defined but never used |
| `RetryPolicy` | browseros-types/src/error.rs | Defined but never used |
| `HealthStatus` | browseros-types/src/component.rs | Defined but never used |
| `proptest` dependency | browseros-types/Cargo.toml | In dev-dependencies but never imported |
| `tempfile` dependency | browseros-config/Cargo.toml | In dev-dependencies but never used |
| `tempfile` dependency | browseros-types/Cargo.toml | In dev-dependencies but never used |
| `tokio` in lifecycle | browseros-lifecycle/Cargo.toml | Dependency but no async code in lifecycle |
| `tokio` in scheduler | browseros-scheduler/Cargo.toml | Dependency but no async code in scheduler (uses std::thread) |

---

## 6. Unused Dependencies (🟡 MEDIUM)

| Crate | Dependency | Status |
|-------|-----------|--------|
| browseros-lifecycle | tokio | Listed but never used (no async, no tokio::spawn) |
| browseros-scheduler | tokio | Listed but never used (uses std::thread::spawn) |
| browseros-types | proptest | Dev-dependency but never imported |
| browseros-types | tempfile | Dev-dependency but never used |
| browseros-config | tempfile | Dev-dependency but never used |

---

## 7. God Objects / Monolithic Files (🟠 HIGH)

| File | Lines | Issue |
|------|-------|-------|
| browseros-cdp/src/traits.rs | ~3,700 | All bridge trait implementations in one file |
| browseros-cdp/src/command.rs | ~563 | All CDP command builders in one file |
| browseros-page/tests/page_tests.rs | ~1,211 | All page tests in one file |
| browseros-runtime/src/lib.rs | ~380 | RuntimeContext + RuntimeBuilder + RuntimeError + tests in one file |
| browseros-lifecycle/src/lib.rs | ~385 | LifecycleManager + LifecycleState + tests in one file |
| browseros-event-bus/src/lib.rs | ~312 | EventBus + EventHandler + InMemoryEventBus + tests in one file |
| browseros-scheduler/src/lib.rs | ~273 | Scheduler + ScheduledTask + tests in one file |

---

## 8. Architecture Invariant Violations

| Invariant | Status | Evidence |
|-----------|--------|----------|
| INV-001: Components communicate only through events | ⚠️ PARTIAL | EventBus exists but lifecycle, scheduler, and runtime don't use it for inter-component communication |
| INV-002: Events immutable after publication | ✅ OK | Events are passed by reference |
| INV-003: correlation_id and causation_id | ✅ OK | MessageEnvelope enforces both |
| INV-004: Injectable Clock | ⚠️ PARTIAL | Clock trait exists but many crates use std::time directly |
| INV-005: No shared mutable state | ✅ OK | All state behind Arc with interior mutability |
| INV-006: State derived from events | ❌ FAIL | No EventStore exists, no event sourcing |
| INV-007: Backward compatible APIs | ⚠️ PARTIAL | No versioning enforcement |
| INV-008: Every public API documented | ⚠️ PARTIAL | Most have doc comments, some missing |
| INV-009: Single public facade module | ✅ OK | All crates follow this pattern |
| INV-010: No internal module imports | ✅ OK | Verified by crate design |
| INV-011: No component ownership | ✅ OK | All Arc-based |
| INV-012: Independently testable | ⚠️ PARTIAL | Some crates (storage) have no tests |
| INV-013: Integration tests use real implementations | ⚠️ PARTIAL | Some exist, no workspace-level integration tests |
| INV-014: State machine validation | ✅ OK | All state machines validate transitions |
| INV-015: MockClock for time-dependent tests | ⚠️ PARTIAL | MockClock exists but not universally used |
| INV-016: Cancellation support | ⚠️ PARTIAL | CancellationToken exists but not checked everywhere |
| INV-017: Configurable timeouts | ⚠️ PARTIAL | Some timeouts, not universal |
| INV-018: No blocking I/O in async | ✅ OK | No async runtime in core crates |
| INV-019: Panics never cross boundaries | ❌ FAIL | unwrap()/expect()/panic! in production code |
| INV-020: Every component exposes metrics | ❌ FAIL | No metrics in most crates |
| INV-021: Every component produces trace spans | ❌ FAIL | No tracing in most crates |
| INV-022: Errors never silently swallowed | ❌ FAIL | unwrap()/expect() swallow errors as panics |
| INV-023: Config changes observable | ❌ FAIL | No ConfigChanged event emission |
| INV-024: Resource quotas | ❌ FAIL | No resource tracking implemented |
| INV-025: RAII resource release | ✅ OK | Standard Rust RAII patterns |
| INV-026: Plugins don't access internal APIs | ✅ OK | No plugin system exists (vacuously true) |
| INV-027: Explicit capability registration | ❌ FAIL | No CapabilityRegistry exists |
| INV-028: Default config works | ✅ OK | Default configs provided |
| INV-029: Dot notation config keys | ✅ OK | BROWSEROS_ prefix used |
| INV-030: Experimental features gated | ⚠️ PARTIAL | Some features, not systematic |
| INV-031: Graceful degradation | ❌ FAIL | Panics cause full crash |
| INV-032: Thread safety explicit | ✅ OK | Send + Sync on all traits |

**Invariant Score:** 12/32 ✅ PASS, 8/32 ⚠️ PARTIAL, 12/32 ❌ FAIL

---

## 9. Cyclic Dependency Risk (🟡 MEDIUM)

The crate graph is a star pattern centered on browseros-types, which prevents cycles. However:

- browseros-runtime depends on ALL runtime crates
- browseros-stress-tests depends on nearly ALL crates
- If browseros-bridge ever depends on browseros-runtime, a cycle forms

**Current state:** No cycles. Risk is low but must be maintained.

---

## 10. Missing Error Handling Patterns (🟠 HIGH)

| Pattern | Occurrences | Risk |
|---------|-------------|------|
| `.unwrap()` in production | ~60+ | Runtime panic on any error |
| `.expect()` in production | ~35+ | Runtime panic with message |
| `panic!()` in production | 4 | Builder panics on missing fields |
| `let _ = result;` | ~10 | Silently discarding errors |
| `result.ok()` | ~15 | Converting errors to None (losing error info) |

---

## 11. Test Debt (🟡 MEDIUM)

| Issue | Details |
|-------|---------|
| No tests in browseros-storage | 0 tests for a core crate |
| No dedicated test files in browseros-dom | Tests embedded in source modules |
| No benchmarks anywhere | Zero criterion or proptest usage |
| No workspace-level integration tests | All tests are per-crate |
| Soak tests are #[ignore]-gated | Require env vars to run |
| High test-to-code ratio in CDP | ~3,000 test lines for ~6,000 code lines (50%) |

---

## 12. Summary

| Category | Count | Critical Items |
|----------|-------|----------------|
| 🔴 CRITICAL | 4 | Builder panics, unwrap in production, unsafe code, invariant failures |
| 🟠 HIGH | 6 | God files, missing error handling, no health system, no resource tracking |
| 🟡 MEDIUM | 8 | Dead code, unused deps, test debt, cyclic risk |
| 🔵 LOW | 5 | Minor style issues, doc gaps |
| ⚪ INFO | 3 | Architecture simplifications, design decisions |

**Total Technical Debt Items:** 26

**Estimated remediation effort:** 2-3 weeks for a single developer to address all CRITICAL and HIGH items.