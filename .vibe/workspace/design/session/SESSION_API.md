# SessionManager Public API Design

> **Design Phase:** Phase 6.2  
> **Status:** Design Freeze Candidate  

---

## 1. API Surface

```
SessionManager {
    // ——— Lifecycle ———

    /// Create a new session with the given config.
    /// Returns the SessionId of the created session.
    /// Fails if system is shutting down or max sessions reached.
    create(config: SessionConfig) -> Result<SessionId, SessionError>

    /// Destroy a session by ID.
    /// force=false: fails if session is Busy or has active resources.
    /// force=true: transitions to Closing immediately, releases all resources.
    destroy(id: SessionId, force: bool) -> Result<(), SessionError>

    // ——— Query ———

    /// Get a session handle for direct state inspection.
    /// Returns None if session does not exist or is Closed/Failed with TTL expired.
    get(id: SessionId) -> Result<Option<SessionHandle>, SessionError>

    /// List sessions matching optional filter criteria.
    list(filter: SessionFilter) -> Result<Vec<SessionSummary>, SessionError>

    /// Check if a session exists and is not in a terminal state.
    exists(id: SessionId) -> Result<bool, SessionError>

    // ——— Access ———

    /// Acquire a session for exclusive use.
    /// Transitions state to Busy. Returns a SessionGuard.
    /// Fails if session is Closing, Closed, or Failed.
    /// Blocks if session is Busy (configurable timeout).
    acquire(id: SessionId) -> Result<SessionGuard, SessionError>

    /// Release a session from exclusive use.
    /// Transitions state to Ready or Idle (based on config).
    /// Called implicitly when SessionGuard is dropped.
    release(id: SessionId) -> Result<(), SessionError>

    /// Get a snapshot of all resource bindings for a session.
    /// Useful for debugging and observability.
    resources(id: SessionId) -> Result<ResourceSnapshot, SessionError>

    // ——— Administration ———

    /// Get comprehensive health report across all sessions.
    health() -> SessionHealthReport

    /// Get aggregate statistics across sessions.
    statistics() -> SessionStatistics

    /// Run garbage collection: close expired idle sessions,
    /// clean up Failed session tombstones past TTL,
    /// release orphaned resources.
    garbage_collect() -> Result<GarbageCollectReport, SessionError>

    /// Graceful shutdown of all sessions.
    /// wait_timeout: max time to wait for Busy sessions.
    /// force_after: after this duration, force-close remaining Busy sessions.
    shutdown(wait_timeout: Duration, force_after: Duration) -> Result<ShutdownReport, SessionError>
}
```

---

## 2. Supporting Types

### 2.1 SessionId

```
SessionId {
    // Wrapper around uuid::Uuid (v7)
    inner: Uuid
}
```

Methods: `new()` (generates UUID v7), `from_string(s)`, `to_string()`, `Display`, `FromStr`, `Serialize`, `Deserialize`, `PartialEq`, `Eq`, `Hash`, `Ord` (based on timestamp prefix).

### 2.2 SessionConfig

```
SessionConfig {
    // Resource acquisition strategy
    resources: ResourceAcquisitionConfig,
    // Session timeout (no activity → auto-idle)
    idle_timeout: Duration,
    // Max session lifetime (session auto-closes after this)
    max_lifetime: Duration,
    // Heartbeat interval (for orphan detection)
    heartbeat_interval: Option<Duration>,
    // Client identifier (owner)
    client_id: String,
    // User-defined metadata
    metadata: HashMap<String, String>,
}

ResourceAcquisitionConfig {
    // Acquire browser eagerly (during Starting) or lazily (on first use)
    acquire_browser: AcquisitionStrategy,  // Eager | Lazy | None
    // Acquire network session eagerly or lazily
    acquire_network: AcquisitionStrategy,
    // Maximum browsers this session can own
    max_browsers: u32,
    // Storage namespace prefix
    storage_namespace: Option<String>,
}

AcquisitionStrategy {
    Eager,    // Acquire during Starting phase
    Lazy,     // Acquire on first resource() call
    None,     // Do not acquire this resource type
}
```

### 2.3 SessionFilter

```
SessionFilter {
    states: Option<Vec<SessionState>>,     // Filter by state(s)
    client_id: Option<String>,              // Filter by owning client
    created_after: Option<Instant>,         // Created after timestamp
    created_before: Option<Instant>,        // Created before timestamp
    tags: Option<HashMap<String, String>>,  // Filter by metadata k:v pairs
    limit: Option<usize>,                   // Max results
    offset: Option<usize>,                  // Pagination offset
}
```

### 2.4 SessionGuard

```
SessionGuard {
    // RAII guard — when dropped, releases the session.
    session_id: SessionId,
    session: Arc<Session>,
    manager: Arc<SessionManager>,
}

impl Drop for SessionGuard {
    // On drop, calls manager.release(self.session_id)
    // If panic occurs during release, session is force-closed.
}
```

Methods:
- `id() -> SessionId`
- `resource<T>(&self) -> Result<Arc<T>, SessionError>` — access a bound resource
- `state(&self) -> SessionState` — current state snapshot
- `metadata(&self) -> &HashMap<String, String>`

### 2.5 SessionHandle

```
SessionHandle {
    // Lightweight handle — does NOT acquire the session.
    // State snapshots may be stale.
    session_id: SessionId,
    state: SessionState,
    resources: Vec<ResourceBinding>,
    created_at: Instant,
    last_activity: Instant,
    client_id: String,
    metadata: HashMap<String, String>,
}
```

### 2.6 SessionSummary

```
SessionSummary {
    id: SessionId,
    state: SessionState,
    created_at: Instant,
    last_activity: Instant,
    client_id: String,
    resource_count: usize,
    tags: HashMap<String, String>,
}
```

### 2.7 SessionHealthReport

```
SessionHealthReport {
    total_sessions: usize,
    by_state: HashMap<SessionState, usize>,
    broken_sessions: Vec<SessionId>,      // Failed state
    stale_sessions: Vec<SessionId>,       // No activity > idle_timeout
    orphan_sessions: Vec<SessionId>,      // Client disconnected
    resource_bindings: ResourceBindingSummary,
}
```

### 2.8 SessionStatistics

```
SessionStatistics {
    total_created: u64,
    total_destroyed: u64,
    total_failed: u64,
    currently_active: usize,
    peak_active: usize,
    avg_lifetime: Duration,
    avg_busy_duration: Duration,
    total_acquire_count: u64,
    total_release_count: u64,
    resource_usage: HashMap<String, u64>,   // resource_type → count
    gc_runs: u64,
    gc_reclaimed: u64,
    errors_by_type: HashMap<String, u64>,
}
```

### 2.9 GarbageCollectReport

```
GarbageCollectReport {
    sessions_closed: Vec<SessionId>,
    tombstones_removed: usize,
    orphaned_resources_released: usize,
    duration: Duration,
}
```

### 2.10 ShutdownReport

```
ShutdownReport {
    graceful: Vec<SessionId>,      // Cleanly closed
    forced: Vec<SessionId>,        // Force-closed after timeout
    failed: Vec<SessionId>,        // Error during shutdown
    duration: Duration,
}
```

### 2.11 SessionError

```
SessionError {
    // Non-exhaustive enum
    Not Found(id: SessionId),
    AlreadyExists(id: SessionId),       // On create with duplicate id
    InvalidState { id, current, expected },
    ResourceAcquisitionFailed { resource_type, reason },
    ResourceReleaseFailed { resource_type, reason },
    ShutdownInProgress,
    MaxSessionsReached(limit: usize),
    SessionBusy(id: SessionId),         // On destroy without force
    Timeout { operation, duration },
    Internal(reason: String),
}
```

---

## 3. Usage Patterns

### 3.1 Basic MCP Tool Usage

```
// MCP tool handler:
fn handle_my_tool(session_manager: Arc<SessionManager>, params: ToolParams) {
    let session_id = params.session_id;
    let guard = session_manager.acquire(session_id)?;
    let browser = guard.resource::<BrowserHandle>()?;
    // ... use browser ...
    drop(guard);  // Explicit release, or let Drop handle it
}
```

### 3.2 Multi-Step Workflow

```
// Workflow engine:
let session_id = session_manager.create(config)?;
for step in workflow.steps {
    let guard = session_manager.acquire(session_id)?;
    // Each step shares the same browser/network/LLM context
    step.execute(&guard)?;
    drop(guard);
}
session_manager.destroy(session_id, false)?;
```

---

## 4. API Versioning Strategy

The SessionManager API is versioned at the trait level:

```rust
pub trait SessionManager: Send + Sync {
    // Minimal v1 API
    fn create(&self, config: SessionConfig) -> Result<SessionId, SessionError>;
    fn destroy(&self, id: SessionId, force: bool) -> Result<(), SessionError>;
    fn acquire(&self, id: SessionId) -> Result<SessionGuard, SessionError>;
    fn release(&self, id: SessionId) -> Result<(), SessionError>;
    fn get(&self, id: SessionId) -> Result<Option<SessionHandle>, SessionError>;
    fn shutdown(&self, wait: Duration, force: Duration) -> Result<ShutdownReport, SessionError>;
}
```

Optional methods (health, statistics, list, garbage_collect) have default implementations in the trait. The concrete `DefaultSessionManager` implements the full interface. This allows the trait to grow without breaking existing consumers.
