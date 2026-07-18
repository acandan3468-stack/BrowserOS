# Session Concurrency Model

> **Design Phase:** Phase 6.2  
> **Status:** Design Freeze Candidate  

---

## 1. Lock Hierarchy

Three distinct lock domains, ordered by acquisition priority:

```
Level 1 (highest): SessionRegistry    — RwLock<HashMap<SessionId, Arc<Session>>>
Level 2:            BrowserPool        — RwLock<HashMap<BrowserId, BrowserState>>
Level 3 (lowest):  SessionInner       — Mutex<SessionInner>
```

**Rule: Locks must be acquired in level order (1→2→3). Never acquire in reverse order.**

This prevents deadlocks because there is no cycle in the lock graph.

### Lock-Free Zones
- EventBus publish: holds internal write lock — must not be called while holding any session/pool lock.
- Timer firing (Scheduler callback): runs without session locks. The callback enqueues a state transition and returns immediately.

---

## 2. Locking by Operation

| Operation | Locks Acquired | Duration | Notes |
|-----------|---------------|----------|-------|
| `create()` | Registry (write) | Brief | Insert new session. No pool locks. |
| `destroy()` | Registry (read) → SessionInner (mutex) → Pool (write, for browser release) | Can block on pool | Pool lock held only during browser release, outside session mutex. |
| `get()` | Registry (read) | Brief | Clone Arc, snapshot state. |
| `list()` | Registry (read) | Brief | Iterate, collect summaries. |
| `acquire()` | Registry (read) → SessionInner (mutex) | Brief | State transition only. Resource access separately. |
| `release()` | SessionInner (mutex) | Brief | State transition. |
| `resource()` | SessionInner (mutex, via guard) | Per resource access | Guard holds mutex while accessing resource. |
| `health()` | Registry (read) | Brief | Iterate, collect status. |
| `statistics()` | Registry (read) + atomic counters | Brief | Mostly atomic reads. |
| `garbage_collect()` | Registry (write) | Can block | Excludes sessions being actively used. |
| `shutdown()` | Registry (write) → per-session acquire | Can block | Waits for busy sessions with timeout. |

---

## 3. Arc Ownership

```
Arc<SessionManager>           — Owned by RuntimeContext, cloned for MCP, Planner, Workflow
Arc<Session>                   — Owned by SessionRegistry, cloned for SessionGuard
Arc<dyn SessionResource>       — Owned by SessionInner::resources
Arc<Mutex<BrowserInstance>>    — Owned by BrowserPool, cloned for BrowserLease
Arc<InMemoryEventBus>         — Owned by RuntimeContext, cloned for SessionManager
Arc<Configuration>            — Owned by RuntimeContext, cloned for SessionManager
```

### Arc Drop Order (on session close)

```
1. SessionGuard dropped (caller releases reference)
2. SessionInner::release_all() called
3. Resource Arcs dropped in dependency order
4. BrowserLease dropped → Pool notified
5. NetworkSession Arc dropped → NetworkManager notified
6. StorageNamespace Arc dropped → StorageManager notified
7. Event subscriptions cancelled
8. Session Arc removed from SessionRegistry
9. SessionInner dropped → all remaining Arcs dropped
```

---

## 4. RwLock vs Mutex

| Structure | Lock Type | Rationale |
|-----------|-----------|-----------|
| SessionRegistry (`HashMap<SessionId, Arc<Session>>`) | **RwLock** | Read-heavy (get/list/exists). Write on create/destroy/gc. |
| BrowserPool (`HashMap<BrowserId, BrowserState>`) | **RwLock** | Read-heavy (health check iteration). Write on acquire/release. |
| SessionInner (state, resources, timers) | **Mutex** | Write-heavy (every acquire/release/resource changes state). Read-lock would rarely be useful. |
| BrowserPool wait queue | **Mutex** | Brief operations only. |
| Per-session timers | **Mutex** (part of SessionInner) | Guarded by SessionInner's Mutex. |
| BrowserInstance (CDP handle) | **Mutex** (external, from existing code) | Kept unchanged. |

---

## 5. Deadlock Prevention

### 5.1 Lock Ordering Protocol

Enforced by convention and (optionally) by lock ordering wrapper types:

```rust
// Lock ordering marker — not a real lock, just a compile-time marker
struct Level1; // SessionRegistry
struct Level2; // BrowserPool
struct Level3; // SessionInner
```

### 5.2 Anti-Patterns (MAY NEVER DO)

| Anti-Pattern | Why Dangerous |
|--------------|---------------|
| Hold SessionInner lock while calling Pool.acquire() | Pool may block (launching browser). Other sessions can't read this session. |
| Hold Pool lock while calling SessionManager.get() | Creates Level2→Level1 reversal. |
| Hold EventBus lock while holding any session lock | EventBus handler may call SessionManager (re-entrancy). |
| Acquire two SessionGuards simultaneously | Not supported (no re-entrancy). Crash with clear error. |
| Lock SessionInner, then call EventBus.publish() | Handler may call back into SessionManager. Use async publish or publish after releasing lock. |

### 5.3 Event Publishing Safety

Pattern to avoid re-entrancy deadlocks:

```rust
// SAFE: Collect events while holding lock, publish after releasing
fn state_transition(&self, new_state: State) {
    let event = {
        let mut inner = self.inner.lock();
        inner.state = new_state;
        SessionEvent::StateChanged { id: self.id, new_state }
    };  // <-- lock released here
    self.event_bus.publish(event);  // Safe: no locks held
}
```

---

## 6. SessionGuard Concurrency

```
┌─────────────────────────────────────────────────────┐
│ Thread A                    Thread B                 │
│                                                     │
│ acquire(id=42) → guard      acquire(id=42) → BLOCK  │
│   (session moves to Busy)     (waits on Mutex)       │
│                                                     │
│ guard.resource::<Browser>()   ...blocked...          │
│   (uses browser)                                     │
│                                                     │
│ drop(guard)                   ...unblocked...        │
│   (session moves to Ready)    acquire returns guard  │
│                              (session moves to Busy) │
└─────────────────────────────────────────────────────┘
```

Blocking behavior is intentional: a session can only serve one caller at a time. This prevents:
- Conflicting CDP commands on the same page
- Interleaved LLM conversation context
- Race conditions on storage namespace

---

## 7. Timer Safety

Timers (idle_timeout, max_lifetime) fire from Scheduler background threads. They must not hold locks when firing:

```rust
// Inside Session:
fn schedule_idle_timer(&self) {
    let session_id = self.id;
    let weak_self = Arc::downgrade(&self.self_arc);  // Weak ref to avoid cycle
    let bus = self.event_bus.clone();
    
    self.scheduler.schedule_after(IDLE_TIMEOUT, move || {
        // No locks held here
        if let Some(session) = weak_self.upgrade() {
            // Enqueue transition — will be processed on next acquire/release or GC
            session.enqueue_transition(SessionCommand::CheckIdle);
        }
    });
}
```

Transitions are not applied immediately in the timer callback. Instead, a `Command` is enqueued (lock-free channel or atomic flag). The command is processed on the next `acquire()`, `release()`, or `garbage_collect()` call.

This design avoids:
- Timer thread blocking on session lock
- Timer thread racing with acquire/release
- Need for async in timer thread

---

## 8. Atomic Operations

```rust
struct Session {
    id: SessionId,
    inner: Mutex<SessionInner>,    // Heavy state
    last_activity: AtomicI64,      // epoch ms — lock-free reads
    event_bus: Arc<InMemoryEventBus>,
    config: SessionConfig,
}

struct SessionManager {
    sessions: RwLock<HashMap<SessionId, Arc<Session>>>,
    stats: SessionStatsAtomic,     // Atomic counters
    config: Arc<Configuration>,
    providers: ProviderSet,
}

struct SessionStatsAtomic {
    total_created: AtomicU64,
    total_destroyed: AtomicU64,
    total_failed: AtomicU64,
    current_active: AtomicI64,     // Can go negative briefly (sync lag)
    peak_active: AtomicU64,
}
```

`last_activity` uses `AtomicI64` (epoch milliseconds via `chrono::Utc::now().timestamp_millis()`) for lock-free read access during health checks and GC iteration. The exact Instant is stored in SessionInner for precision, but the atomic value is sufficient for staleness detection.

---

## 9. SessionRegistry Scalability

For the initial implementation (expected <1000 concurrent sessions), `RwLock<HashMap>` is sufficient.

If scaling beyond 10,000 concurrent sessions is required:
- Replace `HashMap` with `DashMap` (sharded concurrent map, no global lock).
- Lazy migration: only change the storage backend, keep the API unchanged.

---

## 10. Concurrency Invariants

1. **No double-acquire:** Calling `acquire()` on an already-Busy session returns `SessionError::SessionBusy` (not a block).
2. **No lock inversion:** Lock levels 1→2→3 are never violated.
3. **No lock hand-over:** A lock is never released and re-acquired in the same operation to avoid TOCTOU races.
4. **No lock-free resource access:** All resource access goes through SessionGuard, which holds the session mutex.
5. **Timer-released locks:** Timer callbacks never hold a session lock. They enqueue commands for lock-holder threads.
6. **Atomic last_activity:** Updated on every acquire/release without needing the session mutex.
7. **Weak Arc for timers:** Timer closures hold `Weak<Session>` to prevent timer closure from keeping a session alive.
