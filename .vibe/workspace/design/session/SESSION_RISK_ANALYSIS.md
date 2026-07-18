# Session Manager Risk Analysis

> **Design Phase:** Phase 6.2  
> **Status:** Design Freeze Candidate  

---

## 1. Performance Risks

### PR-001: RwLock contention on SessionRegistry

| Field | Value |
|-------|-------|
| **Risk** | High-frequency `acquire()`/`release()` calls (e.g., LLM streaming with per-token session access) create contention on the Registry read lock. Each call does a HashMap lookup under read lock. |
| **Severity** | Medium |
| **Likelihood** | Low (acquire/release per tool call, not per-token) |
| **Mitigation** | Session lookup uses `Arc<Session>` — the HashMap lookup is pointer-sized. Under 10,000 sessions, contention is negligible. If it becomes an issue, switch to `DashMap`. |
| **Detection** | Lock contention profiling via `perf` or `parking_lot` metrics. |

### PR-002: Blocked acquire() on busy session

| Field | Value |
|-------|-------|
| **Risk** | `acquire(id)` blocks the caller (thread) if the session is Busy. In a single-threaded MCP server, this blocks ALL clients. |
| **Severity** | High |
| **Likelihood** | Certain (MCP server is single-threaded per CON-003 in risk register) |
| **Mitigation** | Use `try_acquire(id, timeout)` instead of blocking `acquire()`. MCP server spawns tool execution to a worker thread. |
| **Detection** | MCP server latency monitoring. |

### PR-003: Garbage collection blocking

| Field | Value |
|-------|-------|
| **Risk** | `garbage_collect()` holds Registry write lock while iterating and closing sessions. With many sessions, this blocks all `create()` and `destroy()` calls. |
| **Severity** | Low |
| **Likelihood** | Low (GC is infrequent — default: every 60s) |
| **Mitigation** | GC processes sessions in batches, releasing the write lock between batches. If there are many sessions to close, GC yields periodically. |
| **Detection** | GC duration metric. |

---

## 2. Memory Risks

### MR-001: SessionStateMap unbounded growth

| Field | Value |
|-------|-------|
| **Risk** | Malicious or buggy client creates sessions without destroying them. Registry grows until OOM. |
| **Severity** | High |
| **Likelihood** | Possible (unauthenticated MCP client) |
| **Mitigation** | `SessionConfig.max_sessions_per_client` (default: 10). `SessionManager.max_sessions_global` (default: 1000). Both enforced in `create()`. GC closes sessions past global max. |
| **Detection** | Session count metric with alert threshold. |

### MR-002: Tombstone accumulation

| Field | Value |
|-------|-------|
| **Risk** | Closed and Failed sessions accumulate as tombstones until TTL expires. Under high churn, tombstones dominate memory. |
| **Severity** | Low |
| **Likelihood** | Possible (high churn scenarios) |
| **Mitigation** | Tombstone TTL (default: 3600s for Closed, 7200s for Failed). GC removes tombstones eagerly. Configurable tombstone limit (force-expire oldest if exceeded). |
| **Detection** | Tombstone count metric. |

### MR-003: LLM conversation context bloat

| Field | Value |
|-------|-------|
| **Risk** | Long-running sessions accumulate unbounded LLM conversation history. A session running for hours with continuous LLM calls accumulates megabytes of message history. |
| **Severity** | Medium |
| **Likelihood** | Likely (Planner sessions run long) |
| **Mitigation** | Configurable `max_conversation_tokens` or `max_conversation_messages`. Session enforces context window trimming (oldest messages dropped or summarized). Supported downstream: LlGateway context window parameter. |
| **Detection** | Per-session conversation size metric. |

---

## 3. Resource Leak Risks

### RL-001: BrowserLeak on SessionGuard panic

| Field | Value |
|-------|-------|
| **Risk** | A panic inside `SessionInner::lock()` while holding the lock causes Mutex poison. Subsequent `acquire()` calls fail, and the browser is stranded in the Claimed state. |
| **Severity** | High |
| **Likelihood** | Low (Rust panics in production are rare; Mutex poisoning is handled) |
| **Mitigation** | `SessionGuard::Drop` runs even on panic (assuming no second panic). `SessionInner` uses `std::panic::catch_unwind` around critical sections. BrowserPool health checker reclaims orphaned browsers after `stale_lease_timeout`. |
| **Detection** | BrowserPool health check detects Claimed sessions without heartbeat. |

### RL-002: Event subscription leak

| Field | Value |
|-------|-------|
| **Risk** | If a session closes without unsubscribing its event handlers, the handlers remain registered on the EventBus, holding `Arc<Session>` references and preventing GC. |
| **Severity** | Medium |
| **Likelihood** | Possible (if close() path skips cleanup) |
| **Mitigation** | Event subscriptions are stored in SessionInner and explicitly unsubscribed during Closing phase. The EventBus supports automatic cleanup via subscription tokens that are dropped. If all else fails, the EventBus subscriber list is a weak set (or GC-scanned). |
| **Detection** | EventBus subscription count metric. Alerts on growing orphan subscriptions. |

### RL-003: Storage namespace leak

| Field | Value |
|-------|-------|
| **Risk** | Ephemeral sessions leave storage namespace handles open. If the session crashes before Closing, the namespace remains allocated in StorageManager. |
| **Severity** | Medium |
| **Likelihood** | Possible (process crash) |
| **Mitigation** | StorageManager implements its own lease mechanism with heartbeat. Namespaces without heartbeat for `namespace_timeout` are released. Data persistence is configurable (ephemeral: delete data on release; persistent: keep data). |
| **Detection** | StorageManager namespace count metric. |

---

## 4. Deadlock Risks

### DL-001: Cross-session acquire (user error)

| Field | Value |
|-------|-------|
| **Risk** | A tool calls `acquire(id_A)` while holding `SessionGuard_B`. If another thread does the reverse, deadlock. |
| **Severity** | High |
| **Likelihood** | Low (acquire in SessionGuard method returns error, not block, for already-Busy sessions) |
| **Mitigation** | SessionGuard does NOT provide an `acquire()` method. `SessionManager::acquire()` is the only entry point. Re-entrancy is forbidden and returns error immediately. Cross-session deadlock requires two threads and two SessionManagers — structurally impossible in single-SessionManager design. |
| **Detection** | Not needed (structurally prevented). |

### DL-002: EventBus re-entrancy

| Field | Value |
|-------|-------|
| **Risk** | A SessionManager event handler calls back into SessionManager. If the event was published while holding a session lock, this creates a deadlock or panic (if Mutex is not re-entrant). |
| **Severity** | High |
| **Likelihood** | Possible (misconfigured EventBus handler) |
| **Mitigation** | SessionManager NEVER publishes events while holding any lock. Events are constructed under lock, queued, and published after the lock is released. This is enforced by design pattern, not by compiler. A linter or runtime assertion can detect violations. |
| **Detection** | Runtime assertion: `assert!(!any_session_lock_held)` before publish. |

---

## 5. Future Compatibility Risks

### FC-001: Distributed session migration

| Field | Value |
|-------|-------|
| **Risk** | Phase 8 requires session migration between processes. Current design assumes local Arc references. Migration requires serializing all resources (browser, network, LLM, storage) — which may not be possible for all resource types. |
| **Severity** | Medium |
| **Likelihood** | Likely (Phase 8 is planned) |
| **Mitigation** | Design Session as a state bundle from the start. Resources that cannot be serialized (BrowserInstance) must be re-acquired at the target. Session state machine supports remote transition (serialize state, transfer to remote, resume). The SessionConfig includes a `remote_allowed` flag. |
| **Detection** | Not needed (design-time concern). |

### FC-002: Plugin API assumes local execution

| Field | Value |
|-------|-------|
| **Risk** | Plugin state is stored as `Arc<Mutex<HashMap<...>>>`. If plugins run in separate processes (Phase 7+), this direct memory model breaks. |
| **Severity** | Medium |
| **Likelihood** | Possible (Phase 7 may require out-of-process plugins) |
| **Mitigation** | Plugin state access goes through a trait (`PluginStateProvider`), not direct Mutex access. The trait can be implemented for in-process (Mutex) or out-of-process (RPC) state. |
| **Detection** | Not needed (design-time concern). |

### FC-003: Session-based authentication

| Field | Value |
|-------|-------|
| **Risk** | Session is currently associated with a `client_id` string, not a verifiable identity. If authentication is added later (Phase 7+), the Session's ownership model must be updated to bind to an authenticated principal. |
| **Severity** | Medium |
| **Likelihood** | Likely (authentication is planned for production deployment) |
| **Mitigation** | SessionConfig.client_id is a string — it can represent any identifier. Add `principal_id: Option<AuthenticatedPrincipal>` in a later phase without breaking the API. Sessions are associated with principals, not transport connections. |
| **Detection** | Not needed (design-time concern). |

---

## 6. Risk Summary

| ID | Risk | Severity | Likelihood | Priority | Mitigation Phase |
|----|------|----------|------------|----------|-----------------|
| PR-001 | RwLock contention on Registry | Medium | Low | P3 | Phase 6.3+ |
| PR-002 | Blocked acquire on busy session | High | Certain | **P1** | Phase 6.2 |
| PR-003 | GC blocking | Low | Low | P4 | Phase 6.2 |
| MR-001 | Session count unbounded growth | High | Possible | **P1** | Phase 6.2 |
| MR-002 | Tombstone accumulation | Low | Possible | P4 | Phase 6.2 |
| MR-003 | LLM conversation bloat | Medium | Likely | **P2** | Phase 6.2 |
| RL-001 | Browser leak on panic | High | Low | P2 | Phase 6.2 |
| RL-002 | Event subscription leak | Medium | Possible | P2 | Phase 6.2 |
| RL-003 | Storage namespace leak | Medium | Possible | P2 | Phase 6.2 |
| DL-001 | Cross-session deadlock | High | Low | P3 | Phase 6.2 (prevented by design) |
| DL-002 | EventBus re-entrancy | High | Possible | **P1** | Phase 6.2 |
| FC-001 | Distributed migration | Medium | Likely | P3 | Phase 8 design phase |
| FC-002 | Plugin API local-only | Medium | Possible | P3 | Phase 7 design phase |
| FC-003 | Authentication retrofit | Medium | Likely | P3 | Phase 6.2 |

---

## 7. Acceptance Criteria for Risk Mitigation

1. SessionManager limits sessions per client (max_sessions_per_client) and globally (max_sessions_global). Enforced at create() time.
2. SessionManager publishes events AFTER releasing all locks. Verified by code review.
3. SessionManager's acquire() returns SessionError::Timeout (not block) after configurable timeout.
4. BrowserPool health check reclaims browsers from dead sessions within `health_check_interval`.
5. GC removes tombstones older than their TTL.
6. Session enforces max_conversation_messages across LLM calls.
7. Event subscriptions are verified to be cleaned up on session Close (unit test).
8. Duplicate resource release is idempotent (no panic, no error).
