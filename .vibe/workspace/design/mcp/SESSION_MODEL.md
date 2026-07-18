# browseros-mcp Session Model

**Document:** SESSION_MODEL.md  
**Phase:** Design Freeze  
**Date:** 2026-07-16  

---

## 1. Session Identity

```rust
pub struct SessionId(Uuid);  // UUID v7 (time-ordered, unique)

impl SessionId {
    pub fn new() -> Self;
    pub fn from_string(s: &str) -> Result<Self, ParseError>;
    pub fn as_string(&self) -> String;  // "sess_01J3YF..."
}

// Display format: "sess_<uuid7_hex>" — 41 characters total
// URL-safe, sortable by creation time
```

---

## 2. SessionState

```rust
pub struct SessionState {
    // Identity
    id: SessionId,

    // Creation context
    created_at: chrono::DateTime<chrono::Utc>,
    last_active: AtomicI64,           // Unix timestamp, updated on each tool call

    // Configuration (immutable after creation)
    config: SessionConfig,

    // Owned resources
    browser: Option<BrowserInstance>, // Dropped on session close
    credentials: RwLock<CredentialStore>,

    // Observability per session
    telemetry: SessionTelemetry,

    // Cancellation
    cancel_root: CancelToken,

    // Runtime
    stderr_buffer: Arc<Mutex<String>>,
}

pub struct SessionConfig {
    browser_type: String,        // "chromium" | "firefox"
    headless: bool,
    viewport: Viewport,
    locale: Option<String>,
    timezone: Option<String>,
    geolocation: Option<Geolocation>,
    idle_timeout_secs: u64,      // Default: 300 (5 min)
    max_concurrent_tools: usize, // Default: 4
}
```

---

## 3. SessionManager

```rust
pub struct SessionManager {
    sessions: RwLock<HashMap<SessionId, Arc<SessionState>>>,
    idle_checker: Option<JoinHandle<()>>,
    shutdown: Arc<AtomicBool>,
}

impl SessionManager {
    pub fn new() -> Self;
    pub fn create(&self, config: SessionConfig) -> Result<SessionId, SessionError>;
    pub fn get(&self, id: &SessionId) -> Option<Arc<SessionState>>;
    pub fn close(&self, id: &SessionId) -> Result<SessionSummary, SessionError>;
    pub fn list(&self) -> Vec<SessionSummary>;
    pub fn active_count(&self) -> usize;
}
```

---

## 4. Session Lifecycle

```
                         ┌──────────────────┐
                         │    NOT_CREATED    │
                         └────────┬─────────┘
                                  │ session/create
                                  ▼
                         ┌──────────────────┐
                         │   ALLOCATING      │
                         │   (browser spinup)│
                         └────────┬─────────┘
                                  │ success
                                  ▼
 ┌─────────────────────────────────────────────────────────────────┐
 │                         ACTIVE                                  │
 │                                                                 │
 │  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐         │
 │  │ Tool Call 1 │    │ Tool Call 2 │    │ Tool Call N │         │
 │  │ (worker)    │    │ (worker)    │    │ (worker)    │         │
 │  └──────┬──────┘    └──────┬──────┘    └──────┬──────┘         │
 │         │                  │                   │                 │
 │         └──────────────────┴───────────────────┘                 │
 │                           │                                     │
 │                    last_active updated                           │
 │                    idle timer reset                              │
 └─────────────────────────────────────────────────────────────────┘
          │                        │                     │
          │ session/close          │ idle timeout        │ server shutdown
          ▼                        ▼                     ▼
  ┌──────────────┐       ┌──────────────┐       ┌──────────────┐
  │  CLOSING     │       │  EXPIRING    │       │  FORCE_CLOSE │
  │  (browser    │       │  (browser    │       │  (browser    │
  │   shutdown)  │       │   shutdown)  │       │   shutdown)  │
  └──────┬───────┘       └──────┬───────┘       └──────┬───────┘
         │                      │                       │
         └──────────────────────┴───────────────────────┘
                          │
                          ▼
                 ┌──────────────────┐
                 │   DESTROYED      │
                 │   (memory freed) │
                 └──────────────────┘
```

---

## 5. Browser Allocation

```
SessionManager::create(config)
  │
  ├── 1. Generate SessionId
  │
  ├── 2. Acquire browser from BrowserPool
  │       ├── BrowserPool::allocate(config) → Result<BrowserInstance, BrowserError>
  │       ├── On success: browser instance is owned by this session
  │       ├── On failure: SessionError::BrowserAllocationFailed
  │       └── BrowserPool tracks total concurrent instances globally
  │
  ├── 3. Build SessionState
  │       ├── Store BrowserInstance (owned, no clone)
  │       ├── Initialize empty CredentialStore
  │       ├── Initialize SessionTelemetry
  │       └── Set last_active = now
  │
  ├── 4. Register in sessions HashMap
  │       └── id → Arc<SessionState>
  │
  └── 5. Return SessionId
```

### 5.1 BrowserInstance Ownership

```
SessionManager (HashMap)        BrowserPool (global counter)
        │                              │
        │   create()                    │
        │ ──allocate(config)──────────▶│
        │ ◀──BrowserInstance────────────│
        │                              │
        │   (arc increment)            │ (counter +1)
        │                              │
        │   close()                    │
        │ ──drop BrowserInstance──────▶│
        │                              │
        │   (arc decrement to 0)       │ (counter -1)
        │                              │
```

**Key rule:** `BrowserInstance` is owned by exactly one `SessionState`. When `SessionState` is dropped (session close), `BrowserInstance::drop()` releases the browser. `BrowserPool` maintains a global counter for capacity management but holds no reference to the instance itself.

---

## 6. Resource Ownership Per Session

| Resource | Ownership | Scope | Released When |
|----------|-----------|-------|---------------|
| BrowserInstance | Owned field in SessionState | Session lifetime | Session::close() |
| CredentialStore | RwLock<HashMap> in SessionState | Session lifetime | Session::close() |
| Stderr buffer | Arc<Mutex<String>> | Session lifetime | Session::close() |
| Telemetry counters | Embedded in SessionTelemetry | Session lifetime | Session::close() |
| BrowserPool (ref) | Arc<BrowserPool> in RuntimeContext | Server lifetime | Never released |
| Browser process | OS process owned by BrowserInstance | BrowserInstance lifetime | BrowserInstance::drop() |
| CDP connection | Owned by BrowserInstance internals | BrowserInstance lifetime | BrowserInstance::drop() |

### 6.1 What is NOT Owned by Session

| Resource | Owner | Rationale |
|----------|-------|-----------|
| LlGateway | RuntimeContext | Shared across sessions (stateless gateway) |
| DagEngine | RuntimeContext | Shared across sessions (workflow definitions are global) |
| EventBus | RuntimeContext | Shared — notifications are filtered by session_id |
| ToolRegistry | McpServer | Shared — tools are stateless singletons |
| Cache (LLM) | LlGateway | Shared — cache hit rate improves with sharing |
| Cache (DOM) | BrowserInstance (per-session) | Per-session — DOM cache is browser-specific |
| RootConfig | RuntimeContext | Read-only, globally shared |

---

## 7. Idle Timeout

```rust
impl SessionManager {
    fn idle_checker_loop(&self) {
        let interval = Duration::from_secs(30);
        loop {
            thread::sleep(interval);
            if self.shutdown.load(Ordering::Relaxed) { break; }

            let now = Utc::now().timestamp();
            let expired: Vec<SessionId> = self.sessions.read().iter()
                .filter(|(_, state)| {
                    let idle = now - state.last_active.load(Ordering::Relaxed);
                    idle > state.config.idle_timeout_secs as i64
                })
                .map(|(id, _)| id.clone())
                .collect();

            for id in expired {
                self.close(&id); // emits notification/session/expired
            }
        }
    }
}
```

Timeout notification format:

```json
{
  "jsonrpc": "2.0",
  "method": "notifications/session/expired",
  "params": {
    "session_id": "sess_01J3YF...",
    "reason": "idle_timeout",
    "idle_seconds": 310,
    "usage_summary": {
      "duration_secs": 600,
      "tool_calls": 15,
      "navigations": 3
    }
  }
}
```

---

## 8. Reconnection

BrowserOS sessions are NOT re-connectable in Phase 6. Rationale:

- Sessions own an OS browser process. Process state cannot be serialized and restored.
- BrowserOS is a local runtime. There is no network-addressable session proxy.
- If an MCP client disconnects and reconnects, it creates a new session.

### 8.1 Session Persistence Hooks (Phase 8)

Future enhancement: `SessionStore` trait for inspecting past sessions:

```rust
trait SessionStore {
    fn record_creation(&self, id: &SessionId, config: &SessionConfig) -> Result<()>;
    fn record_close(&self, id: &SessionId, summary: &SessionSummary) -> Result<()>;
    fn get_history(&self, filter: SessionFilter) -> Vec<SessionRecord>;
}
```

Default implementation: in-memory VecDeque (last 1000 sessions).  
Optional implementations: SQLite, Redis (for distributed audit).

### 8.2 Future Distributed Sessions (Phase 8+)

Not in scope for Phase 6. Distributed sessions would require:
- BrowserState serialization protocol
- Remote CDP proxy
- Coordinated BrowserPool across machines
- Session migration protocol

---

## 9. Session Limits

| Limit | Default | Maximum | Enforcement |
|-------|---------|---------|-------------|
| Concurrent sessions | 10 | Configurable | SessionManager::create() rejects beyond max |
| Idle timeout | 300s | 30–86400s | SessionChecker thread |
| Tool calls per session | Unlimited | — | — |
| Browser instances globally | 10 | Configurable | BrowserPool capacity |
| Credentials per session | 1000 | Configurable | CredentialStore capacity |

---

*This document defines the complete Session Model. It contains no Rust code, no Cargo.toml, and no placeholders.*
