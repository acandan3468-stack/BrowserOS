# Browser Pool Design

> **Design Phase:** Phase 6.2  
> **Status:** Design Freeze Candidate  
> **Target Crate:** `browseros-browser` (modify) or new `browseros-browser-pool`  

---

## 1. Architectural Position

```
SessionManager
     │
     │  (via BrowserPoolProvider trait)
     ▼
BrowserPool
     │
     ├── acquire(session_id) → BrowserLease
     ├── release(lease) → ()
     ├── health() → PoolHealth
     └── resize(min_idle, max_total)
          │
          ├── BrowserInstance 1  (launched / idle / claimed)
          ├── BrowserInstance 2
          └── BrowserInstance N
```

BrowserPool is an independent crate (or module within `browseros-browser`). It does NOT depend on SessionManager. It does NOT know about sessions — it only tracks which BrowserInstance is assigned to which session via a `claimed_by: Option<SessionId>` field.

---

## 2. Pool State Machine

```
┌───────────┐
│  Launching│ ←── Pool.resize() / prewarm()
└─────┬─────┘
      │  (launch complete)
      ▼
┌───────────┐
│   Idle    │ ←── available for assignment
└─────┬─────┘
      │  acquire(session_id)
      ▼
┌───────────┐
│  Claimed  │ ←── assigned to a session
└─────┬─────┘
      │  release()
      ▼
┌───────────┐
│   Idle    │
└─────┬─────┘
      │  (idle > max_idle_timeout AND count > min_idle)
      ▼
┌───────────┐
│  Closing  │ ←── graceful browser shutdown
└─────┬─────┘
      │
      ▼
┌───────────┐
│  Dead     │ ←── removed from pool
└───────────┘

Additionally:
  Any state ──(crash detected)──→ Dead
  Launching ──(launch failure)──→ Dead
```

---

## 3. Public API

```rust
trait BrowserPoolProvider: Send + Sync {
    /// Acquire a browser for a session.
    /// Returns a BrowserLease with an active browser.
    /// Blocks if no idle browser available and at max_total.
    /// Timeout after acquisition_timeout.
    fn acquire(&self, session_id: SessionId) -> Result<BrowserLease, BrowserPoolError>;

    /// Release a browser back to the pool.
    /// Called implicitly when BrowserLease is dropped.
    fn release(&self, lease: BrowserLease) -> Result<(), BrowserPoolError>;

    /// Prewarm the pool to min_idle browsers.
    fn prewarm(&self) -> Result<(), BrowserPoolError>;

    /// Resize pool boundaries.
    fn resize(&self, min_idle: Option<usize>, max_total: Option<usize>) -> Result<(), BrowserPoolError>;

    /// Health check across all browsers.
    fn health(&self) -> PoolHealthReport;

    /// Claim a specific browser from idle pool by browser_id.
    /// Used for reconnection and reserved browsers.
    fn claim_by_id(&self, browser_id: BrowserId, session_id: SessionId) -> Result<BrowserLease, BrowserPoolError>;
}

struct BrowserLease {
    browser_id: BrowserId,
    instance: Arc<Mutex<BrowserInstance>>,
    session_id: SessionId,
    // When dropped, returns browser to pool
}

impl Drop for BrowserLease {
    fn drop(&self) {
        // Notify pool to return browser to idle
        // If pool is at max_idle, close the browser
    }
}
```

---

## 4. Configuration

```rust
struct BrowserPoolConfig {
    /// Minimum idle browsers to maintain.
    pub min_idle: usize,           // default: 1
    /// Maximum total browsers (idle + claimed).
    pub max_total: usize,          // default: 10
    /// Maximum time a browser stays idle before being closed.
    pub max_idle_timeout: Duration, // default: 300s
    /// Timeout for acquiring a browser (includes launch time).
    pub acquisition_timeout: Duration, // default: 30s
    /// Browser launch timeout.
    pub launch_timeout: Duration,      // default: 15s
    /// Health check interval.
    pub health_check_interval: Duration, // default: 30s
    /// Browser backend config (passed to BrowserBuilder).
    pub browser_config: BrowserConfig,
    /// Strategy when pool is full.
    pub on_full: FullPoolStrategy,
}

enum FullPoolStrategy {
    /// Queue the request (block until a browser is released).
    Queue { max_queue_size: usize, queue_timeout: Duration },
    /// Reject the request immediately.
    Reject,
    /// Grow beyond max_total temporarily (elastic).
    Elastic { max_elastic: usize },
}
```

---

## 5. Internal Architecture

```rust
struct BrowserPool {
    config: BrowserPoolConfig,
    /// All browsers tracked by the pool.
    browsers: RwLock<HashMap<BrowserId, BrowserState>>,
    /// Queue for pending acquire requests.
    wait_queue: Mutex<VecDeque<AcquireRequest>>,
    /// Event bus for pool events.
    event_bus: Arc<InMemoryEventBus>,
    /// Browser launcher (abstracted for testability).
    launcher: Box<dyn BrowserLauncher>,
    /// Background health checker.
    health_checker: Mutex<Option<JoinHandle<()>>>,
}

struct BrowserState {
    instance: Arc<Mutex<BrowserInstance>>,
    status: BrowserStatus,       // Launching | Idle | Claimed(SessionId) | Closing | Dead
    claimed_at: Option<Instant>,
    idle_since: Option<Instant>,
    crash_count: u32,
    last_health_check: Instant,
}
```

---

## 6. Key Algorithms

### 6.1 Acquire

```
1. Lock browsers (read)
2. Find any browser in Idle state
3. If found:
   a. Upgrade to write lock
   b. Set status to Claimed(session_id)
   c. Return BrowserLease
4. If not found AND count < max_total:
   a. Upgrade to write lock
   b. Set status to Launching
   c. Launch browser asynchronously
   d. Return BrowserLease when launch completes (block caller)
5. If not found AND count >= max_total:
   a. Apply FullPoolStrategy:
      - Queue: add to wait_queue, block caller with timeout
      - Reject: return PoolFull error
      - Elastic: if count < max_total + max_elastic, launch (goto 4)
                 else: return PoolFull error
```

### 6.2 Release

```
1. Lock browsers (write)
2. Find browser by BrowserLease.browser_id
3. If pool has > max_idle browsers OR browser is unhealthy:
   a. Set status to Closing
   b. Close browser (async)
   c. Set status to Dead
   d. Remove from map
4. Else:
   a. Set status to Idle
   b. Set idle_since = now
5. If wait_queue is not empty:
   a. Dequeue next AcquireRequest
   b. Assign the newly-idle browser to the waiting session
   c. Wake waiting thread
```

### 6.3 Health Check (Background)

```
Every health_check_interval:
1. Iterate all browsers (read lock)
2. For each Claimed browser: send CDP ping (non-blocking)
   - Timeout: 5s
   - On failure: increment crash_count, close browser, mark Dead
   - Notify session of browser failure via EventBus
3. For each Idle browser:
   - If idle_since + max_idle_timeout < now AND count > min_idle:
     Close browser, mark Dead
4. For each Dead browser: remove from map
5. If count < min_idle: launch replacement browser(s)
```

### 6.4 Prewarm

```
1. Lock browsers (write)
2. current_idle = count(Idle)
3. needed = min_idle - current_idle
4. For i in 0..needed:
   a. Launch browser
   b. Set status to Idle
5. Return launched count
```

---

## 7. Failure Recovery

| Failure Scenario | Detection | Recovery |
|-----------------|-----------|----------|
| Browser crash (process dies) | CDP ping timeout | Mark Dead. Acquire caller gets replacement from pool. Session notified via event. |
| Browser hang (process alive, CDP unresponsive) | CDP command timeout | Kill process. Mark Dead. Same recovery as crash. |
| Launch failure | Launch timeout or error | Retry (up to 3 times). On repeated failure, return error to acquire(). |
| Network partition (CDP unreachable) | Transport error | Same as crash. Browser marked Dead. |
| Orphan browser (session disconnects without release) | Heartbeat timeout in SessionManager | SessionManager releases session's browsers. Pool receives release() and reclaims. |
| Pool state corruption | Detected on health check | Log critical error. Reset pool. All browsers closed and relaunched. |

---

## 8. BrowserLease Safety Guarantees

1. **Double-release safe:** If release() is called twice for the same browser, the second call is a no-op (BrowserState is already Idle/Dead).
2. **Leak-safe:** If a BrowserLease is forgotten (not dropped), the pool's health checker detects the lingering Claimed state and reclaims the browser after a stale_lease_timeout.
3. **Panic-safe:** If the code using BrowserLease panics, Drop runs and returns the browser to the pool.
4. **Thread-safe:** BrowserLease requires &mut self for access (linear type in practice). Arc<Mutex<...>> ensures concurrent safety for the underlying instance.

---

## 9. Integration with SessionManager

SessionManager calls BrowserPool through the `BrowserPoolProvider` trait:

```rust
// Inside Session::start() — eager acquisition:
fn start(&self, pool: &dyn BrowserPoolProvider) -> Result<()> {
    let lease = pool.acquire(self.id)?;
    self.inner.lock().bind_resource(BrowserResource::new(lease));
    Ok(())
}

// Inside Session::close() — release:
fn close(&self, pool: &dyn BrowserPoolProvider) -> Result<()> {
    // Resources released in dependency order
    let lease = self.inner.lock().unbind_resource(ResourceType::Browser)?;
    pool.release(lease)?;
    Ok(())
}
```

SessionManager never holds the pool's internal lock while holding a session lock. It acquires resources one at a time, outside the session's inner lock.

---

## 10. BrowserPool Independence

BrowserPool is designed to function without SessionManager:

- Direct acquire/release API for callers that do not need sessions.
- Browser instances can be used without session context.
- Pool health, resize, and prewarm work independently.
- SessionManager is just one consumer of the pool.

This independence enables:
- Testing pool logic without session overhead.
- MCP tools that manage browsers directly.
- Future Planner that might have its own allocation logic.
- Distributed runtime where pool is remote.
