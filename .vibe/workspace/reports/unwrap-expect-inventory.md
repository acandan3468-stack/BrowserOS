# Unwrap/Expect Inventory — BrowserOS

**Date:** 2026-07-09 (updated 2026-07-09)  
**Source:** Full codebase scan  

---

## Summary

| Crate | Production unwrap | Production expect | Test unwrap | Test expect |
|-------|------------------|-------------------|-------------|-------------|
| browseros-types | 0 | 0 | ~15 | 0 |
| browseros-config | 0 | 0 | ~5 | 0 |
| browseros-observability | 0 | 0 | ~5 | 0 |
| browseros-event-bus | 0 | 0 | ~3 | 0 |
| browseros-lifecycle | 0 | 0 | ~3 | 0 |
| browseros-scheduler | 0 | 0 | ~2 | 0 |
| browseros-runtime | 0 | 0 | ~5 | 0 |
| browseros-storage | 0 | 0 | 0 | 0 |
| browseros-bridge | 0 | 0 | ~48 | 0 |
| **browseros-cdp** | **0** | **0** | **~40** | **~20** |
| **browseros-browser** | **0** | **0** | **~10** | **~15** |
| **browseros-page** | **0** | **0** | **~15** | **~10** |
| browseros-dom | 0 | 0 | ~5 | 0 |
| browseros-stress-tests | 0 | 0 | ~20 | 0 |
| **Total** | **~0** | **~0** | **~176** | **~45** |

**Note:** All production unwrap/expect calls in browseros-cdp, browseros-browser, and browseros-page have been eliminated. Remaining `.unwrap()`/`.expect()` calls exist only in `#[cfg(test)]` modules, which is acceptable.

---

## Production Unwrap/Expect by Location

### ✅ ALL FIXED — Zero remaining production unwrap/expect in browseros-cdp, browseros-browser, browseros-page

#### connection.rs — 13 `lock().expect()` converted
All converted using Category A (`map_err(?)` in Result-returning methods) or Category B (`unwrap_or_else(|e| e.into_inner())` in infallible contexts).

#### traits.rs — ~20 `lock().unwrap()` converted
All converted to `map_err(?)` (Category A) for Result-returning methods or `unwrap_or_else(|e| e.into_inner())` (Category B) for callbacks and getters.

#### transport_ws.rs — 1 `lock().unwrap()` converted
Converted to `lock().map_err(?)` (Category A) in `connect()`.

#### backend.rs — Already had no production unwrap/expect (only test code)
#### process.rs — Already had no production unwrap/expect
#### waiter.rs — Already had no production unwrap/expect

---

## Classification

| Category | Count | Action |
|----------|-------|--------|
| `lock().unwrap()` on Mutex/RwLock | ~35 | LOW risk — lock poisoning unlikely in single-process |
| `.expect("...")` on Result | ~15 | MEDIUM risk — descriptive but still panics |
| `.unwrap()` on Option | ~8 | HIGH risk — None causes panic |
| `.unwrap()` on Result (I/O) | ~5 | HIGH risk — I/O errors cause panic |

## Recommended Fix Order

1. **HIGH**: Option unwraps in CDP traits.rs — replace with proper error propagation
2. **MEDIUM**: expect() in CDP backend.rs — replace with proper error types
3. **LOW**: lock().unwrap() — can be replaced with `lock().expect("lock poisoned")` for better messages, or kept as-is (acceptable risk)