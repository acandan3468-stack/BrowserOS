ARCHIVED

This document has been archived. Its content has been merged into the canonical documentation set.
See docs/ for current information.

---

# Failure Root Cause Analysis

**Phase:** 1.4.3 — Real Soak Execution Failures

## Summary

**Total soak tests executed:** 16 (14 soak + 1 rerun of corrected test)  
**Tests failed initially:** 1  
**Tests failed after correction:** 0  

## Failure 1: `dom_hierarchical_event_propagation`

| Field | Value |
|-------|-------|
| **Test file** | `browseros-stress-tests/tests/soak_agent_pattern.rs` |
| **Assertion** | Line 162: `assert_eq!(captured, total_events, "Capture phase should receive all events")` |
| **Expected** | `captured == 40_000` |
| **Actual** | `captured == 10_000` |
| **Root cause** | **Test bug (not a runtime bug)** |

### Root Cause Detail

The test modeled DOM event propagation with "capture phase" and "bubble phase":

```rust
// Capture phase subscriber — subscribes to "dom.document"
bus.subscribe("dom.document", Arc::new(move |_: &dyn Event| {
    cc.fetch_add(1, Ordering::Relaxed);
}));
```

The test assumed that subscribing to `dom.document` would receive ALL events in the DOM hierarchy (capture phase semantics). However, the EventBus implements **exact topic matching** — a subscriber on `dom.document` receives only events published to `dom.document`, not events published to `dom.body`, `dom.div`, or `dom.span`.

**Expected vs Actual:**

- Test expected: `captured` = 40,000 (all 4 levels × 10,000 iterations)
- Actual: `captured` = 10,000 (only `dom.document` events — correct EventBus behavior)

### Correction

Changed the assertion to reflect actual EventBus topic routing semantics:

```rust
assert_eq!(captured, num_iterations as u64, "Document receives only dom.document events");
assert_eq!(bubbled, num_iterations as u64, "Span receives only dom.span events");
```

**After correction:** Test passes.

### Lesson

The EventBus does NOT implement DOM-like capture/bubble propagation. Subscriber topics are exact-match. If hierarchical routing is needed in the future, it must be built as a separate middleware layer on top of the base EventBus.

---

## No Runtime Failures Found

Beyond the test assertion bug above, **zero runtime failures** were found across all 14 soak tests:

- No memory leaks
- No event loss
- No correlation ID breakage
- No deadlocks
- No thread runaway
- No stalls
- No panics in library code

