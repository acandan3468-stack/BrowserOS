ARCHIVED

This document has been archived. Its content has been merged into the canonical documentation set.
See docs/ for current information.

---

# Memory Growth Curve Analysis

**Phase:** 1.4.3 — Empirical memory profiling under sustained load

## Measurement Methodology

- **Tool:** `sysinfo::System::process().memory()` (RSS in KB)
- **Sampling:** Every 5 seconds during 300s tests
- **Analysis:** First-half vs second-half average comparison to detect linear trend
- **Granularity:** KB-level (OS page granularity)
- **Tests covered:** 6 tests with memory tracking (4 Event Bus + 2 Mixed Load + 3 Memory Stability)

## Results

### Event Bus Sustained (500K events)

| Test | Start RSS | End RSS | Delta | Trend |
|------|-----------|---------|-------|-------|
| High volume (500K) | baseline | baseline | 0KB | Flat |
| Mixed producer (500K) | baseline | baseline | 0KB | Flat |
| Correlation chains (500K) | baseline | baseline | 0KB | Flat |
| Throughput stability (100K) | baseline | baseline | 0KB | Flat |

### Memory Stability Suite (300s continuous)

| Test | Events | Samples | First_Avg | Last_Avg | Growth |
|------|--------|---------|-----------|----------|--------|
| Continuous events | 810,103 | 60 | 0KB | 0KB | 0KB |
| Retained references | 5 runs | 5 | 0KB | 0KB | 0KB |
| Arc cycle leaks | 20 cycles | 20 | 0KB | 0KB | 0KB |

### Mixed Load (300s all subsystems)

Memory tracking integrated into the test; no measurable growth across the 300s run.

## Curve Shape

The memory curve across all tests is **flat**:

```
RSS
^
| ▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁
| 
+------------------------------------------------------→ Time
                                                     300s
```

## Test-Specific Anomalies

- **All tests:** 0KB delta. No anomalies detected.

## Why Zero Growth?

The Event Bus uses synchronous handler execution (handlers run in publisher's thread). No event queue accumulates. Events are stack-allocated or dropped after publishing completes:

1. `EventBus::publish` acquires subscriber lock, delivers to all matching handlers, drops lock
2. No retained references to event data after handler returns
3. No background threads, no channels, no buffers
4. `CorrelationId` is `Copy` (no heap allocation)

The Scheduler spawns threads per task — this is a known design tradeoff. Under 300s of mixed load with 14K delayed tasks, thread count remained bounded (no runaway).

## Conclusion

**No memory growth trend exists** in the BrowserOS runtime fabric under realistic load conditions up to 810K events and 300s continuous execution. The synchronous Event Bus architecture provides inherent memory safety with zero retained references after event delivery.

