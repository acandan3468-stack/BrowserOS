# Technical Debt — v2 (Canonical)

**Status:** LIVE — Current technical debt inventory for BrowserOS.  
**Supersedes:** browseros/technical-debt-report.md  

---

## Severity Legend

| Severity | Meaning |
|----------|---------|
| 🔴 MUST FIX | Production blocker |
| 🟠 SHOULD FIX | Significant risk |
| 🟡 CONSIDER | Medium concern |
| 🔵 WATCH | Minor, monitor over time |

---

## 🔴 MUST FIX

### TD-001: Builder panics instead of returning Result
**File:** `browseros-types/src/message.rs` (lines 327, 338, 349)  
**Issue:** `MessageEnvelopeBuilder::build()` panics if `source`, `content_type`, or `payload` are missing.  
**Fix:** Return `Result<MessageEnvelope, BuilderError>` instead.  
**Risk:** Any programming error causes runtime crash. Violates INV-019 and INV-022.

### TD-002: ~120 unwrap() calls in production code
**Files:** browseros-cdp (connection, session, backend), browseros-browser (manager, process), browseros-page (waiter)  
**Issue:** `unwrap()` on `Result` and `Option` causes panics on any unexpected state.  
**Fix:** Replace with proper error handling (`?` operator, pattern matching, or `expect` with context).  
**Risk:** Single largest source of potential runtime crashes.

### TD-003: ~50 expect() calls in production code
**Files:** browseros-cdp, browseros-browser, browseros-config, browseros-event-bus, browseros-lifecycle  
**Issue:** `expect()` panics with a message but still crashes the process. Lock poisoning expects are particularly dangerous.  
**Fix:** Replace with proper error propagation.  
**Risk:** System crash on lock poisoning or unexpected state.

### TD-004: 1 unsafe block in production code
**File:** `browseros-dom/src/events.rs` (~line 200)  
**Issue:** `unsafe { transmute::<_, _>(...) }` for DomEvent type conversion. No `#[deny(unsafe_code)]` anywhere.  
**Fix:** Use safe conversion or document why transmute is safe. Add `#[deny(unsafe_code)]` lint.  
**Risk:** Undefined behavior if enum layout changes.

### TD-005: Storage is a stub with 0 tests
**File:** `browseros-storage/` (entire crate)  
**Issue:** No EventStore, no StateStore, no persistence. Single TODO comment. Zero tests.  
**Fix:** Implement EventStore/StateStore or remove crate and document as future scope.  
**Risk:** Core runtime crate with zero functionality.

### TD-006: 12/32 architecture invariants violated
**Files:** Multiple crates  
**Issue:** INV-019 (panics), INV-020 (no metrics), INV-021 (no tracing), INV-022 (errors swallowed), INV-023 (no config events), INV-024 (no quotas), INV-027 (no capability registry), INV-031 (no graceful degradation) are all violated.  
**Fix:** Systematic audit and remediation of violated invariants.  
**Risk:** Architecture drift makes invariants meaningless.

---

## 🟠 SHOULD FIX

### TD-007: EventBus subscribes by EventId, not category
**File:** `browseros-event-bus/src/lib.rs`  
**Issue:** Subscribers must know exact EventId (UUID). Cannot subscribe to "all lifecycle events".  
**Fix:** Add category-based subscription or routing rules.  
**Risk:** Limits usefulness of event system.

### TD-008: No ManagedComponent trait in LifecycleManager
**File:** `browseros-lifecycle/src/lib.rs`  
**Issue:** LifecycleManager tracks string-keyed states, not typed components. No start/stop hooks.  
**Fix:** Add `ManagedComponent` trait with `init()`, `start()`, `stop()` methods.  
**Risk:** Components cannot participate in lifecycle management.

### TD-009: Monolithic god files
**Files:**
- `browseros-cdp/src/traits.rs` (~3,700 lines)
- `browseros-cdp/src/command.rs` (~563 lines)
- `browseros-page/tests/page_tests.rs` (~1,211 lines)
- `browseros-event-bus/src/lib.rs` (~312 lines, all code)
- `browseros-lifecycle/src/lib.rs` (~385 lines, all code)
- `browseros-scheduler/src/lib.rs` (~273 lines, all code)
- `browseros-runtime/src/lib.rs` (~380 lines, all code)

**Fix:** Split into multiple modules.  
**Risk:** Maintainability, merge conflicts, comprehension.

### TD-010: Dead code — defined but never used
**Items:**
- `ComponentManifest` (browseros-types) — defined, never used
- `CapabilityDefinition` (browseros-types) — defined, never used
- `ResourceRequirements` (browseros-types) — defined, never used
- `RetryPolicy` (browseros-types) — defined, never used
- `HealthStatus` (browseros-types) — defined, never used

**Fix:** Either remove dead code or implement the systems that use it.  
**Risk:** Confusion, misleading API surface.

### TD-011: Unused dependencies
**Items:**
- `tokio` in browseros-lifecycle (listed but never used)
- `tokio` in browseros-scheduler (listed but never used — uses std::thread)
- `proptest` in browseros-types (dev-dependency, never imported)
- `tempfile` in browseros-types (dev-dependency, never used)
- `tempfile` in browseros-config (dev-dependency, never used)

**Fix:** Remove unused dependencies.  
**Risk:** Dependency bloat, slower builds, misleading documentation.

### TD-012: No OTLP export for observability
**File:** `browseros-observability/src/export.rs`  
**Issue:** Logger, Metrics, Tracer are in-memory only. No export pipeline.  
**Fix:** Implement OTLP exporter or document as intentional limitation.  
**Risk:** Production observability requires real export.

---

## 🟡 CONSIDER

### TD-013: No benchmarks anywhere
**Issue:** Zero criterion or proptest usage. No performance regression protection.  
**Fix:** Add benchmarks for EventBus throughput, scheduler latency, CDP command dispatch.

### TD-014: No workspace-level integration tests
**Issue:** All tests are per-crate. No test wires all crates together.  
**Fix:** Create `tests/` directory at workspace root with integration tests.

### TD-015: Soak tests are #[ignore]-gated
**Issue:** 10 soak/stress tests require env vars to run. Not part of CI.  
**Fix:** Integrate soak tests into CI with short durations.

### TD-016: BrowserManager is synchronous
**Issue:** All browser operations block the calling thread.  
**Fix:** This is intentional (ADR-016), but should be documented as a tradeoff.

### TD-017: CDP command coverage is ~20%
**Issue:** Only ~20 of 100+ CDP domains have command builders.  
**Fix:** Add remaining domains as needed. Document known gaps.

### TD-018: CDP traits.rs is a god file
**File:** `browseros-cdp/src/traits.rs` (~3,700 lines)  
**Issue:** All bridge trait implementations in one file.  
**Fix:** Split by bridge trait (browser_impl.rs, page_impl.rs, etc.).

### TD-019: EventMetadata calls Utc::now() directly
**File:** `browseros-types/src/event.rs`  
**Issue:** Violates INV-004 (Clock injection). Non-deterministic in tests.  
**Fix:** Accept timestamp parameter.

### TD-020: MessageEnvelopeBuilder calls Utc::now() directly
**File:** `browseros-types/src/message.rs`  
**Issue:** Same as TD-019. Non-deterministic timestamps.  
**Fix:** Accept timestamp parameter.

---

## 🔵 WATCH

### TD-021: MockClock not universally used
**Issue:** Some tests use real time instead of MockClock.  
**Monitor:** Add time-dependent test linting.

### TD-022: No CI configuration
**Issue:** Only `.vibe/templates/github-actions.yml` exists. No actual CI.  
**Monitor:** Critical before production deployment.

### TD-023: Examples directory is empty
**Issue:** `browseros/examples/` exists but has 0 files.  
**Monitor:** Add examples as API surface stabilizes.

### TD-024: No doc-tests
**Issue:** Minimal doc-test usage. Public API examples not tested.  
**Monitor:** Add doc-tests as API stabilizes.

### TD-025: browseros-lifecycle + browseros-scheduler depend on tokio but don't use async
**Issue:** tokio listed as dependency but code uses std::thread.  
**Monitor:** Either remove tokio or migrate to async execution.

---

## Summary

| Severity | Count | Key Items |
|----------|-------|-----------|
| 🔴 MUST FIX | 6 | Builder panics, unwrap/expect, unsafe, storage stub, invariant violations |
| 🟠 SHOULD FIX | 6 | EventBus category sub, ManagedComponent, god files, dead code, unused deps, OTLP export |
| 🟡 CONSIDER | 8 | Benchmarks, integration tests, soak tests, CDP coverage, CDP god file, Utc::now violations |
| 🔵 WATCH | 5 | MockClock usage, CI, examples, doc-tests, tokio deps |
| **Total** | **25** | |