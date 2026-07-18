ARCHIVED

This document has been archived. Its content has been merged into the canonical documentation set.
See docs/ for current information.

---

# Memory Profile Report

**Phase:** 1.4.2 — Runtime Soak & Real Workload Validation
**Date:** 2026-06-30
**Methodology:** Process memory sampled via `sysinfo` (cross-platform OS process queries)

---

## Memory Tracking Strategy

Memory is tracked at the OS level using `sysinfo::System::process().memory()` which returns the RSS (Resident Set Size) in bytes. Sampling is done at regular intervals during long-running tests. Results compare first-half averages vs second-half averages to detect linear growth trends.

All thresholds are set conservatively:
- Event bus soak: <10MB total growth allowed (covers string interning cache, HashMap overhead)
- Event reference retention: <3MB growth across 5 runs
- Arc cycle test: <3MB growth across 20 cycles
- Continuous load: <5MB growth between first-half and second-half averages

---

## Memory Leak Detection Points

### Scenario 1: Sustained Event Bus Load (`memory_stable_under_continuous_events`)
- **Duration:** 120s
- **Rate:** ~100 events/ms
- **Total events:** ~12,000,000
- **Sampling:** every 5 seconds
- **Check:** Compare average RSS of first 50% of samples vs last 50%
- **Threshold:** <5MB growth
- **Risk:** String interning cache grows with new event kinds; capped at 5 kinds

### Scenario 2: Event Reference Retention (`no_retained_event_references`)
- **Runs:** 5 runs of 50,000 events each
- **Subscribers:** Fresh subscriber per run (replaces old via new subscription)
- **Check:** Compare RSS after run 0 vs after run 4
- **Threshold:** <3MB growth
- **Risk:** EventBus retaining handler references via subscriber map; HashMap reallocation overhead

### Scenario 3: Arc Cycle Leaks (`no_arc_cycle_leaks`)
- **Cycles:** 20 create/drop cycles
- **Per cycle:** Scheduler (10k delayed tasks), LifecycleManager (component transitions), EventBus publishing
- **Check:** Compare average RSS of first 10 cycles vs last 10 cycles
- **Threshold:** <3MB growth
- **Risk:** Arc cycles between Scheduler→EventBus→Lifecycle preventing drop

### Scenario 4: Continuous High-Volume (`sustained_high_volume_no_memory_growth`)
- **Events:** 100k–500k (configurable via `SOAK_EVENTS`)
- **Memory:** Before vs after comparison
- **Threshold:** <10MB growth
- **Risk:** EventMetadata allocation overhead; HashMap expansion in EventBus

---

## Known Memory Sources

| Component | Memory Usage | Growth Pattern |
|-----------|-------------|----------------|
| EventBus subscriber map | HashMap<String, Vec<(Handle, Arc<Handler>)>> | Constant size after subscription phase |
| EventMetadata per event | ~200 bytes allocated, freed after handler returns | Stack-only (no heap retention) |
| Scheduler task map | HashMap<TaskId, ScheduledTask> | Cleaned on task execution or cancel |
| Logger sink/filter | Static after construction | None |
| MetricsRegistry | HashMap of counters, gauges, histograms | Grows with distinct metric names (known fixed set) |
| Tracer | Internal span store | Bounded by in-flight spans |
| String interning cache | HashMap<String, &'static str> | Grows with new distinct event kinds (capped at 5 in tests) |

---

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| EventBus subscriber map leak | Low | High | Subscribers removed via unsubscribe on SubscriptionHandle drop |
| Arc cycle between Scheduler→EventBus | Low | Medium | Scheduler holds Arc<EventBus> but not vice versa |
| Event string interning OOM | Low | Medium | HashMap capped in practice (kinds are bounded) |
| HashMap reallocation memory spike | Medium | Low | Temporary doubling, released after rehash |
| OS RSS noise | High | Low | Trend-based detection (avg over samples) not single point comparisons |

---

## Conclusion

The runtime fabric is designed with no persistent allocation per-event. All subsystem state is bounded:
- EventBus: bounded by distinct subscriber entries
- Scheduler: bounded by in-flight tasks
- LifecycleManager: bounded by registered components
- Metrics: bounded by distinct metric names

The soak tests verify that memory remains stable over time with no linear growth trends.

