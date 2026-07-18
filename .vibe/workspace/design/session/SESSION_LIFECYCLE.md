# Session Lifecycle

> **Design Phase:** Phase 6.2  
> **Status:** Design Freeze Candidate  

---

## 1. State Diagram

```
                    ┌─────────────────────────────────────┐
                    │                                     │
                    v                                     │
    ┌─────────┐  ┌──────────┐  ┌───────┐  ┌────┐  ┌──────┴───┐
    │ Created │─→│ Starting │─→│ Ready │─→│Busy│─→│  Ready   │
    └─────────┘  └──────────┘  └───┬───┘  └──┬─┘  └──────────┘
        │              │            │         │
        │              │            v         │
        │              │         ┌──────┐     │
        │              │         │ Idle │←────┘
        │              │         └──┬───┘
        │              │            │
        │              │            v
        │              │         ┌───────────┐
        │              ├────────→│ Suspended │
        │              │         └─────┬─────┘
        │              │               │
        v              v               v
    ┌────────┐    ┌──────────┐    ┌──────────┐
    │ Failed │    │ Closing  │───→│  Closed  │
    └────────┘    └──────────┘    └──────────┘
        │              │
        │              │  (after TTL)
        v              v
    [Tombstone]    [Tombstone]
    (TTL-based)    (TTL-based)
```

---

## 2. State Definitions

### Created
- **Meaning:** Session object allocated in memory. No resources acquired.
- **Resources:** None (SessionId only).
- **Observability:** `session.created` event published.
- **Timeout:** If no transition to Starting within configurable `creation_timeout` (default: 60s), auto-transition to Failed with reason "creation_timeout".
- **Entry guard:** System must not be in Shutdown. Session count must be below max.

### Starting
- **Meaning:** Resources being acquired per SessionConfig (eager strategy).
- **Resources:** Browser allocation, network session creation, storage namespace provisioning occur here.
- **Observability:** `session.starting` event published. Each successful resource acquisition emits `session.resource_bound`.
- **Timeout:** Configurable `startup_timeout` (default: 30s). Failure to acquire all eager resources within timeout → Failed.
- **Partial failure:** If an eager resource fails to acquire but is not critical (NonCritical flag in config), session can transition to Ready with warnings. If critical resource fails → Failed.
- **Entry guard:** State must be Created.

### Ready
- **Meaning:** Session is fully initialized and available for work. All eager resources acquired.
- **Resources:** All configured eager resources are bound. Lazy resources are bound on first access.
- **Observability:** `session.ready` event published.
- **Transition to Idle:** After `idle_timeout` (configurable, default: 300s) with no acquire() calls. Reset on each acquire()/release() cycle.
- **Entry guard:** State must be Starting (all resources) or Busy (release()) or Idle (acquire()) or Suspended (resume()).

### Busy
- **Meaning:** Session is exclusively held by a client/task via SessionGuard. No other caller can acquire this session.
- **Resources:** All bound resources accessible via SessionGuard.
- **Observability:** `session.busy` event published with optional task_id metadata.
- **Timeout:** Configurable `busy_timeout` (default: 300s). If a single acquire() call holds the session longer than this, a warning is logged. No auto-transition (may be long-running task).
- **Re-entrancy:** NOT supported. Same caller cannot acquire() a session twice. This prevents accidental deadlocks.
- **Entry guard:** State must be Ready or Idle.

### Idle
- **Meaning:** Session is alive but has no active work. Resources are retained (browser stays open, network session stays connected).
- **Resources:** All bound resources are retained. Browser may transition to background mode if supported by BrowserPool.
- **Observability:** `session.idle` event published with idle_duration_ms.
- **Timeout:** Configurable `session_idle_timeout` (default: 900s). After timeout → transition to Suspended (if suspend enabled) or Closing.
- **Browser pooling hint:** Session signals to BrowserPool that the browser is idle. Pool may choose to swap browser to lower-resource mode if the browser supports it.
- **Entry guard:** State must be Ready or Suspended.

### Suspended
- **Meaning:** Session resources partially released to reduce footprint. Browser returned to pool (but pool remembers the assignment for fast re-acquire). Network session may be disconnected. LLM conversation context serialized.
- **Resources:** Only lightweight metadata retained. Heavy resources (browser, network) are surrendered to their respective pools with a reservation token.
- **Observability:** `session.suspended` event published with reason (timeout | resource_pressure | admin).
- **Resume:** On acquire(), session re-acquires surrendered resources using reservation tokens. Faster than cold start but slower than warm reuse.
- **Timeout:** Configurable `suspend_timeout` (default: 3600s). After timeout → Closing.
- **Entry guard:** State must be Idle.

### Closing
- **Meaning:** Graceful resource teardown in progress. All bound resources are released in dependency order (reverse acquisition order).
- **Resources:** Resources released one by one. Each release emits `session.resource_released`. Failure of individual resource release does NOT abort the closing process — error is logged and next resource is released.
- **Observability:** `session.closing` event published with reason (explicit_destroy | timeout | shutdown | error).
- **Timeout:** Configurable `close_timeout` (default: 30s). If not all resources released within timeout → force remaining → transition to Closed (with warnings).
- **Entry guard:** State must be any non-terminal state (except Failed). If Busy and force=false → error. If Busy and force=true → task receives cancellation signal → release resources → continue.

### Closed
- **Meaning:** All resources released. Session is a tombstone.
- **Resources:** None. All Arcs to resources are dropped.
- **Observability:** `session.closed` event published with duration and close reason.
- **TTL:** Configurable `tombstone_ttl` (default: 3600s). After TTL, session is removed from registry by garbage_collect(). During TTL, get() and exists() return data for audit/logging.
- **Entry guard:** Must be Closing.

### Failed
- **Meaning:** Session entered an unrecoverable error state. Resources may be partially released.
- **Resources:** Best-effort release attempted. Resources that cannot be released are tracked in an incident report.
- **Observability:** `session.failed` event published with error details and phase (starting | busy | closing).
- **TTL:** Configurable `failure_ttl` (default: 7200s, double tombstone_ttl). After TTL, removed by garbage_collect().
- **Recovery:** No automatic recovery. Failed sessions require manual inspection or automated incident response.
- **Entry guard:** Any state except Closed.

---

## 3. Valid Transition Table

| From ↓ \ To → | Created | Starting | Ready | Busy | Idle | Suspended | Closing | Closed | Failed |
|---------------|---------|----------|-------|------|------|-----------|---------|--------|--------|
| **Created**   | -       | auto     | -     | -    | -    | -         | manual  | -      | auto   |
| **Starting**  | -       | -        | auto  | -    | -    | -         | manual  | -      | auto   |
| **Ready**     | -       | -        | -     | auto | auto | -         | manual  | -      | auto   |
| **Busy**      | -       | -        | auto  | -    | -    | -         | force   | -      | auto   |
| **Idle**      | -       | -        | auto  | auto | -    | auto      | auto    | -      | auto   |
| **Suspended** | -       | -        | auto  | -    | -    | -         | auto    | -      | auto   |
| **Closing**   | -       | -        | -     | -    | -    | -         | -       | auto   | auto   |
| **Closed**    | -       | -        | -     | -    | -    | -         | -       | -      | -      |
| **Failed**    | -       | -        | -     | -    | -    | -         | -       | auto   | -      |

- **auto:** Automatic transition caused by normal operation (timeout, completion, release, acquire).
- **manual:** Triggered by explicit API call (destroy(), shutdown()).
- **force:** Triggered by destroy(force=true) or shutdown(force_after=...).
- **-:** Invalid transition. Returns SessionError::InvalidState.

---

## 4. Transition Guards

Each transition has preconditions that must be satisfied:

| Transition | Precondition | Action |
|-----------|-------------|--------|
| Created → Starting | System not shutting down. Session count < max. | Begin resource acquisition. Start startup timer. |
| Starting → Ready | All critical eager resources acquired. | Stop startup timer. Record timestamps. |
| Starting → Failed | Startup timeout OR critical resource failure. | Cancel pending acquisitions. Log error. Publish failure event. |
| Ready → Busy | acquire() called. Session not Closing/Closed/Failed. | Increment acquire counter. Start busy timer. |
| Busy → Ready | release() called or SessionGuard dropped. | Stop busy timer. Update last_activity. |
| Busy → Failed | Unrecoverable error during busy work. | Log error. Attempt resource release. |
| Ready → Idle | No acquire() for idle_timeout duration. | Start suspend timer. |
| Idle → Ready | acquire() called. | Stop suspend timer. Reset idle timer. |
| Idle → Suspended | Suspend timeout reached. | Release non-critical resources to pools. Store reservation tokens. |
| Idle → Closing | Session idle_timeout_suspend disabled AND idle for max_idle_timeout. OR explicit destroy(). | Initiate resource release. |
| Suspended → Ready | acquire() called with resume intent. | Re-acquire resources using reservation tokens. |
| Suspended → Closing | Suspend timeout reached. OR explicit destroy(). | Abandon reservation tokens. Close session. |
| Closing → Closed | All resources released. | Record final statistics. Start tombstone TTL. |
| Closing → Failed | Resource release error (partial cleanup). | Log incident. Record leaked resources. |
| Failed → Closed | Explicit acknowledge() or tombstone TTL expired. | Remove from active registry. |

---

## 5. Timer Management

Three internal timers per session:
1. **startup_timer** — fires if Starting takes too long.
2. **idle_timer** — fires when Ready/Idle with no activity for idle_timeout.
3. **suspend_timer** — fires when Suspended for suspend_timeout.

Timers are implemented via `browseros-scheduler` (schedule_after). Each timer is a `oneshot::Sender<()>` stored in SessionInner. On state transition, the corresponding timer is cancelled by dropping the sender.

If scheduler is unavailable, timers are checked lazily on each acquire()/release() call (polling). The garbage_collect() method also scans for expired timers.
