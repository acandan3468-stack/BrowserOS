ARCHIVED

This document has been archived. Its content has been merged into the canonical documentation set.
See docs/ for current information.

---

# Soak Test Report

**Phase:** 1.4.2 — Runtime Soak & Real Workload Validation
**Date:** 2026-06-30
**Suite:** browseros-stress-tests (soak tests)

---

## Summary

| Metric | Value |
|--------|-------|
| Soak test files | 5 |
| Soak test cases | 14 |
| Tests requiring long run | 14 (all `#[ignore]` by default) |
| Default event count | 100,000 |
| Default duration | 60–120s per test |
| Environment overrides | `SOAK_EVENTS`, `SOAK_SECS` |

All soak tests compile cleanly. Full execution requires `-- --ignored --nocapture` and 10–30 minutes of wall time.

---

## Test Scenarios

### 1. Event Bus Soak (`soak_event_bus.rs`) — 4 tests

| Test | Description |
|------|-------------|
| `sustained_high_volume_no_memory_growth` | 100k+ events across 5 kinds, periodic memory sampling, correlation_id tracking |
| `sustained_mixed_producer_consumer` | 4 producers × 25k events, single consumer with lightweight processing |
| `correlation_id_integrity_over_long_chain` | Chain depth 3 (chain_1→chain_2→chain_3), verify all leaf correlation_ids match roots |
| `throughput_stable_over_time` | 10 windows of 10k events each, measure rate variance |

**Failure policy:** Memory growth >10MB = FAIL. Any event loss = FAIL. Correlation mismatch = FAIL.

### 2. Mixed System Load (`soak_mixed_load.rs`) — 2 tests

| Test | Description |
|------|-------------|
| `all_subsystems_simultaneous_no_interference` | Event Bus + Lifecycle + Scheduler + Logger + Metrics + Tracer all running concurrently for 90s |
| `scheduler_and_event_bus_no_starvation` | Scheduler delayed tasks interleaved with high-frequency event publishing |

**Failure policy:** Starvation of any subsystem = FAIL. Metrics counter mismatch = FAIL.

### 3. Memory Stability (`soak_memory_stability.rs`) — 3 tests

| Test | Description |
|------|-------------|
| `memory_stable_under_continuous_events` | 120s of 100 events/ms, memory sampled every 5s, trend compared first-half vs second-half |
| `no_retained_event_references` | 5 runs of 50k events each with new subscribers; memory compared across runs |
| `no_arc_cycle_leaks` | 20 cycles of creating/dropping Scheduler+Lifecycle+EventBus; memory trend checked |

**Failure policy:** Growth >5MB between halves = FAIL. Growth >3MB across cycles = FAIL.

### 4. Degraded Environment (`soak_degraded_env.rs`) — 3 tests

| Test | Description |
|------|-------------|
| `slow_handlers_graceful_degradation` | Fast handlers (yield) + slow handlers (5ms sleep) interleaved; fast must dominate |
| `delayed_scheduler_under_load` | Scheduler delayed tasks running under saturated event bus |
| `high_contention_subscribers_graceful` | 50 subscribers across 5 event kinds, 10–60μs processing variance |

**Failure policy:** System collapse = FAIL. Slow handlers starving fast = FAIL.

### 5. Real Agent Pattern (`soak_agent_pattern.rs`) — 4 tests

| Test | Description |
|------|-------------|
| `browser_like_event_bursts` | 20 bursts of 5k events across 10 browser event kinds (click, input, scroll, fetch, etc.) |
| `dom_hierarchical_event_propagation` | 10k DOM propagations (document→body→div→span) with capture and bubble phases |
| `correlated_event_chains_perfect_propagation` | 2000 chains of depth 5, correlation_id propagated through all links |
| `full_agent_pattern_simulation` | Browser events + lifecycle transitions + scheduler timers + tracing, 45s continuous |

**Failure policy:** Broken correlation chain = FAIL. Missing event kinds = FAIL.

---

## Execution Instructions

```powershell
# Quick check (lower volume, shorter duration)
$env:SOAK_EVENTS=10000; $env:SOAK_SECS=30; cargo test --test soak_event_bus -- --ignored --nocapture

# Full soak (all tests, high volume)
$env:SOAK_EVENTS=500000; $env:SOAK_SECS=300
cargo test --test soak_event_bus -- --ignored --nocapture
cargo test --test soak_mixed_load -- --ignored --nocapture
cargo test --test soak_memory_stability -- --ignored --nocapture
cargo test --test soak_degraded_env -- --ignored --nocapture
cargo test --test soak_agent_pattern -- --ignored --nocapture
```

