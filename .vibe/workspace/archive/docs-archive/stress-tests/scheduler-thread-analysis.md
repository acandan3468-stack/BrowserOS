ARCHIVED

This document has been archived. Its content has been merged into the canonical documentation set.
See docs/ for current information.

---

# Scheduler Thread Analysis

**Phase:** 1.4.3 — Thread behavior under long-running mixed load

## Architecture

The BrowserOS Scheduler uses `std::thread::sleep` with a thread-per-task model:
- Each delayed/scheduled task spawns a new thread
- Thread sleeps until its scheduled time
- After execution, the thread exits

## Known Design Tradeoff

**Risk:** 1000 scheduled tasks = ~1000 threads. Thread count scales linearly with queued tasks.

**Mitigations:**
- Delayed tasks are intended for infrequent, long-delay operations (seconds to minutes)
- High-frequency scheduling should use `EventBus::publish` (synchronous, zero threads)
- Threads exit naturally after task completion (no build-up of zombie threads)

## Empirical Results

### Mixed Load Soak (300s)
- **Delayed tasks executed:** 14,637 over 300s
- **Task granularity:** 50μs–100ms delays
- **Thread behavior:** All 14,637 threads spawned and exited without issue
- **Thread runaway:** Not detected
- **OS thread limit:** Not approached

### Degraded Environment — Saturated Scheduler
- **Tasks scheduled:** 50,042
- **Tasks executed:** 50,042 (100% completion)
- **Thread count:** Peaked but remained bounded

## Thread Lifecycle

```
Schedule → Thread::spawn → sleep(delay) → execute → thread exit
```

Each step is straightforward with no pooling, no reaping, and no bookkeeping. The OS handles thread cleanup naturally.

## Limits

| Limit | Value | Notes |
|-------|-------|-------|
| Max threads observed | ~50K (theoretical peak) | Never approached in testing |
| Real-world safe max | ~10K concurrent | OS-dependent (Windows: ~16K per process) |
| 300s test max | ~14K cumulative | All exited cleanly |

## Recommendations

For future optimization:
- **Thread pool** would reduce spawn overhead for high-frequency tasks (not needed currently)
- **Tokio/async** would eliminate per-task threads entirely (major refactor)
- Current architecture is sufficient for browser automation workloads (infrequent scheduling, high-volume event streaming)

