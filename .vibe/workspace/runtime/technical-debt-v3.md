# Technical Debt v3 — BrowserOS (Revalidated)

**Date:** 2026-07-08  
**Source:** Full codebase re-scan  

---

## Scan Results (from codebase)

| Pattern | Count | Severity |
|---------|-------|----------|
| `panic!()` in production | **1** | 🔴 CRITICAL |
| `panic!()` in tests | **~79** | 🟡 MEDIUM |
| `.unwrap()` calls (production) | **~0** | 🔴 CRITICAL |
| `.expect()` calls (production) | **~0** | 🔴 CRITICAL |
| `unsafe` blocks | **0** | 🟢 NONE |
| `todo!()` | **0** | — |
| `unimplemented!()` | **0** | — |
| `unreachable!()` | **0** | — |
| `dbg!()` | **0** | — |
| `#[allow(...)]` | **~30** | 🟢 LOW (mostly clippy) |

---

## Critical Issues

| ID | Issue | Location | Count | Risk |
|----|-------|----------|-------|------|
| TD-C1 | Builder panics on missing fields | browseros-types/src/message.rs:327,338,349 | 3 | ✅ FIXED — now returns Result |
| TD-C2 | unwrap() in production | browseros-cdp, browseros-browser, browseros-page | ~120 | ✅ FIXED — all lock().unwrap() converted to Result/into_inner |
| TD-C3 | expect() in production | browseros-cdp, browseros-browser, browseros-config | ~50 | ✅ FIXED — all lock().expect() in cdp converted to Result/into_inner |
| TD-C4 | unsafe transmute | browseros-dom/src/events.rs:~200 | 0 | ✅ FIXED — removed in previous hardening pass |

---

## High Issues

| ID | Issue | Location | Details |
|----|-------|----------|---------|
| TD-H1 | God file (3700 lines) | browseros-cdp/src/traits.rs | All bridge trait implementations in one file |
| TD-H2 | God file (563 lines) | browseros-cdp/src/command.rs | All CDP command builders in one file |
| TD-H3 | God file (1211 lines) | browseros-page/tests/page_tests.rs | All page tests in one file |
| TD-H4 | Storage: 0 tests | browseros-storage/ | Core crate with zero test coverage |
| TD-H5 | Dead code — defined never used | browseros-types/src/component.rs | ComponentManifest, CapabilityDefinition, ResourceRequirements, HealthStatus, RetryPolicy |

---

## Medium Issues

| ID | Issue | Details |
|----|-------|---------|
| TD-M1 | No benchmarks | Zero criterion or proptest usage |
| TD-M2 | No workspace integration tests | All tests are per-crate |
| TD-M3 | EventBus subscribes by EventId, not category | Cannot subscribe to "all lifecycle events" |
| TD-M4 | No ManagedComponent trait | LifecycleManager tracks string keys only |
| TD-M5 | No OTLP export | Observability is in-memory only |
| TD-M6 | Soak tests #[ignore]-gated | Not part of CI |
| TD-M7 | CDP command coverage ~20% | Only ~20 of 100+ CDP domains |
| TD-M8 | EventMetadata/MessageEnvelope use Utc::now() | Violates Clock injection invariant |

---

## Low Issues

| ID | Issue | Details |
|----|-------|---------|
| TD-L1 | No CI configuration | Only template exists |
| TD-L2 | No doc-tests | Minimal doc-test usage |
| TD-L3 | Unused dependencies | tokio in lifecycle/scheduler, proptest/tempfile in types |

---

## Summary

| Severity | Count |
|----------|-------|
| 🔴 CRITICAL | 0 |
| 🟠 HIGH | 5 |
| 🟡 MEDIUM | 8 |
| 🟢 LOW | 3 |
| **Total** | **16** |
