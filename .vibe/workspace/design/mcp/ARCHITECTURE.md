# browseros-mcp Architecture

**Document:** ARCHITECTURE.md  
**Phase:** Design Freeze  
**Date:** 2026-07-16  

---

## 1. Crate Identity

### 1.1 Responsibility

`browseros-mcp` is the sole external MCP interface for the entire BrowserOS platform. Every BrowserOS capability — browser automation, DOM inspection, workflow execution, session management, LLM integration, network interception, storage management — is exposed through this crate's MCP server.

### 1.2 Non-Responsibilities

- Does NOT implement LLM providers (delegates to `browseros-llm`)
- Does NOT implement browser protocol (delegates to `browseros-cdp` via `browseros-bridge`)
- Does NOT implement workflow engine (delegates to `browseros-dag`)
- Does NOT implement DOM operations (delegates to `browseros-dom` via `browseros-bridge`)
- Does NOT implement storage backends (delegates to `browseros-storage`)
- Does NOT implement observability (delegates to `browseros-observability`)

### 1.3 Public API Surface

The crate exposes exactly one public entry point:

```rust
pub struct McpServerBuilder;
impl McpServerBuilder {
    pub fn new() -> Self;
    pub fn with_runtime(rt: Arc<RuntimeContext>) -> Self;
    pub fn with_tool(name: &str, tool: Box<dyn McpTool>) -> Self;
    pub fn build() -> McpServer;
}

pub struct McpServer;
impl McpServer {
    pub fn start(self) -> Result<(), McpError>;
    pub fn shutdown(&self);
    pub fn health(&self) -> ServerHealth;
}

// Re-exported from browseros-llm::mcp for protocol conformance:
pub use browseros_llm::mcp::{JsonRpcMessage, JsonRpcId, McpToolDef, McpContentItem};
```

Everything else is internal.

---

## 2. Module Tree

```
browseros-mcp/src/
├── bin/
│   └── browseros_mcp_server.rs      # Entry point: parse args, build McpServer, start()
├── lib.rs                            # Re-exports McpServerBuilder, McpServer, McpTool trait
├── server/
│   ├── mod.rs                        # McpServer struct, start(), shutdown()
│   ├── transport.rs                  # StdioReader, stdio line read/write loop
│   ├── dispatcher.rs                 # JSON-RPC method → handler dispatch
│   ├── lifecycle.rs                  # Startup/shutdown orchestration, signal handling
│   └── metrics.rs                    # Server-level metrics (connections, requests, latency)
├── session/
│   ├── mod.rs                        # SessionManager, SessionId, SessionHandle
│   ├── state.rs                      # SessionState — per-session RuntimeContext wrapper
│   ├── pool.rs                       # BrowserPool integration, per-session browser allocation
│   └── credentials.rs                # Per-session credential store (in-memory, encrypted)
├── tools/
│   ├── mod.rs                        # ToolRegistry, McpTool trait
│   ├── system/                       # system/health, system/config, system/metrics, system/version
│   │   ├── mod.rs
│   │   ├── health.rs
│   │   ├── config.rs
│   │   ├── metrics.rs
│   │   └── version.rs
│   ├── session/                      # session/create, session/close, session/list, session/config
│   │   ├── mod.rs
│   │   ├── create.rs
│   │   ├── close.rs
│   │   ├── list.rs
│   │   └── configure.rs
│   ├── browser/                      # browser/navigate, browser/screenshot, browser/evaluate
│   │   ├── mod.rs
│   │   ├── navigate.rs
│   │   ├── screenshot.rs
│   │   ├── evaluate.rs
│   │   ├── pdf.rs
│   │   └── tabs.rs
│   ├── dom/                          # dom/query, dom/click, dom/type, dom/snapshot, dom/observe
│   │   ├── mod.rs
│   │   ├── query.rs
│   │   ├── interact.rs               # click, type, select, hover, focus, scroll
│   │   ├── snapshot.rs
│   │   └── observe.rs
│   ├── network/                      # network/intercept, network/conditions, network/cookies
│   │   ├── mod.rs
│   │   ├── intercept.rs
│   │   ├── conditions.rs
│   │   ├── har.rs
│   │   └── cookies.rs
│   ├── workflow/                     # workflow/execute, workflow/plan, workflow/status
│   │   ├── mod.rs
│   │   ├── execute.rs
│   │   ├── plan.rs
│   │   ├── status.rs
│   │   └── cancel.rs
│   └── llm/                          # llm/models (diagnostic only)
│       ├── mod.rs
│       └── models.rs
├── notify/
│   ├── mod.rs                        # NotificationDispatcher, SubscriberMap
│   ├── progress.rs                   # Progress notification builder
│   ├── events.rs                     # EventBus → MCP notification bridge
│   └── throttle.rs                   # Rate limiting for high-frequency notifications
├── error.rs                          # McpServerError — all server-level error types
└── types.rs                          # Shared internal types (SessionId, ToolResult, etc.)
```

---

## 3. Ownership Model

### 3.1 Resource Ownership Map

| Resource | Owner | Lifetime | Thread Safety |
|----------|-------|----------|---------------|
| RuntimeContext | McpServer (Arc) | Server lifetime | Send + Sync |
| SessionManager | McpServer | Server lifetime | Mutex<HashMap> |
| ToolRegistry | McpServer (Arc) | Server lifetime | Arc<RwLock<>> |
| BrowserPool (global) | RuntimeContext | Process lifetime | Send + Sync |
| Per-session BrowserInstance | SessionState | Session lifetime | Owned, not shared |
| Per-session LLM config | SessionState | Session lifetime | Owned |
| Per-session credential store | SessionState | Session lifetime | RwLock inside |
| NotificationDispatcher | McpServer (Arc) | Server lifetime | Arc<RwLock<>> |
| Tool instances | ToolRegistry | Server lifetime | Arc<dyn McpTool> |
| EventBus subscription | NotificationDispatcher | Per-subscriber | mpsc channel |
| Stderr log buffer | SessionState | Session lifetime | Arc<Mutex<String>> |

### 3.2 Ownership Rules

1. **McpServer owns everything.** It creates the SessionManager, ToolRegistry, and NotificationDispatcher. These are dropped when the server stops.
2. **Sessions own their runtime state.** A SessionState holds the per-session browser instance, credentials, and configuration. When a session is destroyed, all its resources are released.
3. **Tools are stateless singletons.** Tool implementations hold no per-request state. They receive session context through the `McpToolContext` parameter.
4. **RuntimeContext is shared, not cloned.** All subsystems that need RuntimeContext receive an `Arc<RuntimeContext>`.
5. **Notifications are fanned out.** The NotificationDispatcher holds subscriber channels. Each subscriber gets an independent clone of each notification.

---

## 4. Lifecycles

### 4.1 Server Startup Lifecycle

```
browseros_mcp_server::main()
  │
  ├── 1. Parse CLI args (config path, log level)
  │
  ├── 2. Initialize observability (stderr logger)
  │
  ├── 3. Load RootConfig from file
  │
  ├── 4. Create RuntimeContext via RuntimeContext::init(config)
  │       ├── LlGateway::build(config.llm)
  │       ├── BrowserPool::new(config.browser)
  │       ├── DagEngine::new(config.dag)
  │       ├── LifecycleManager::new()
  │       ├── Scheduler::new()
  │       └── EventBus::new()
  │
  ├── 5. Build McpServer
  │       ├── McpServerBuilder::new()
  │       │     .with_runtime(Arc<RuntimeContext>)
  │       │     .with_tool("system/health", SystemHealthTool::new())
  │       │     .with_tool("system/config", SystemConfigTool::new())
  │       │     .with_tool("session/create", SessionCreateTool::new())
  │       │     .build()
  │       └── ToolRegistry registers all tools
  │
  ├── 6. Start McpServer
  │       ├── Spawn signal handler thread (SIGTERM, SIGINT)
  │       ├── Spawn stdin reader thread
  │       └── Block on stdin loop
  │
  └── 7. Server running (accepting JSON-RPC messages)
```

### 4.2 Server Shutdown Lifecycle

```
SIGTERM received or stdin EOF
  │
  ├── 1. Set shutdown flag (AtomicBool)
  │
  ├── 2. Stop accepting new messages (drain stdin buffer)
  │
  ├── 3. Cancel all active tool executions
  │       ├── Send cancel signal to running operations
  │       └── Wait for completion with timeout (5s)
  │
  ├── 4. Destroy all sessions
  │       ├── For each session: close browser, release resources
  │       └── Clear SessionManager
  │
  ├── 5. Shutdown RuntimeContext
  │       ├── LifecycleManager::shutdown()
  │       ├── BrowserPool::shutdown()
  │       └── LlGateway::clear_cache()
  │
  ├── 6. Flush and close logger
  │
  └── 7. Exit with code 0
```

### 4.3 Session Lifecycle

```
CREATE
  │
  ├── Client sends session/create
  │
  ├── 1. Validate parameters (browser type, viewport, locale, etc.)
  │
  ├── 2. Generate SessionId (UUID v7)
  │
  ├── 3. Allocate browser instance
  │       ├── BrowserPool::allocate(config) → BrowserInstance
  │       └── On failure: return error, session not created
  │
  ├── 4. Build SessionState
  │       ├── Owned BrowserInstance
  │       ├── Empty credential store
  │       ├── Empty stderr log buffer
  │       ├── Telemetry context (correlation_id = session_id)
  │       └── Created timestamp
  │
  ├── 5. Register in SessionManager
  │       └── session_map.insert(id, SessionState)
  │
  ├── 6. Return { session_id, browser_info, created_at }
  │
  └── ACTIVE state

ACTIVE
  │
  ├── Tools receive session_id in parameters
  ├── Dispatcher resolves session_id → SessionState
  ├── Tools operate on SessionState's BrowserInstance
  ├── Idle timer resets on each tool call
  └── ...until CLOSE or TIMEOUT

CLOSE (explicit)
  │
  ├── Client sends session/close { session_id }
  │
  ├── 1. Remove from SessionManager
  │
  ├── 2. Close browser instance (BrowserInstance::close())
  │
  ├── 3. Clear credentials
  │
  ├── 4. Return usage summary
  │
  └── DESTROYED

CLOSE (idle timeout)
  │
  ├── SessionChecker thread scans every 30s
  ├── Finds sessions with last_active + timeout > now
  │
  ├── 1. Emit notification/session/expired
  ├── 2. Same cleanup as explicit close
  │
  └── DESTROYED

CLOSE (server shutdown)
  │
  ├── Same as explicit close for all active sessions
  │
  └── DESTROYED
```

### 4.4 Request Lifecycle

```
JSON-RPC message received on stdin
  │
  ├── 1. Transport reads line
  │
  ├── 2. Dispatcher parses JSON-RPC
  │       ├── Parse error → respond -32700
  │       └── Invalid JSON-RPC → respond -32600
  │
  ├── 3. Dispatcher matches method
  │       ├── "initialize" → handshake handler
  │       ├── "notifications/..." → notification handler (no response)
  │       ├── "tools/list" → ToolRegistry::list_tools()
  │       ├── "tools/call" → call handler
  │       ├── "ping" → pong handler
  │       └── unknown → respond -32601
  │
  ├── 4. CALL HANDLER (for tools/call)
  │       ├── Validate params (name, arguments)
  │       ├── Resolve tool from ToolRegistry
  │       │     └── Not found → respond -32601
  │       ├── Resolve session_id from arguments
  │       │     └── Not found or invalid → respond -32001
  │       ├── Build McpToolContext { session, cancel_token, notification_sink }
  │       ├── Execute tool
  │       │     ├── Tool may emit progress notifications
  │       │     ├── Tool may be cancelled via cancel_token
  │       │     └── Tool returns Result<ToolOutput, ToolError>
  │       ├── Map ToolError → JSON-RPC error
  │       └── Respond with result or error
  │
  └── 5. Response written to stdout
```

### 4.5 Notification Lifecycle

```
EventBus event emitted (e.g., NavigationCompleted)
  │
  ├── 1. EventBus delivers to all subscribers
  │
  ├── 2. NotificationDispatcher receives event
  │       ├── Match event type to notification type
  │       └── Build notification payload
  │
  ├── 3. For each active subscriber channel
  │       ├── Check subscriber's filter (session_id match?)
  │       ├── Check throttle (high-frequency events)
  │       └── Try to send via mpsc::Sender::try_send()
  │
  ├── 4. On try_send success
  │       └── Subscriber thread writes JSON-RPC notification to stdout
  │
  ├── 5. On try_send failure (full buffer)
  │       ├── If LowPriority: drop notification
  │       ├── If HighPriority: block until space available (with timeout)
  │       └── If buffer overflow: increment dropped counter, drop oldest
  │
  └── 6. Metrics: increment notification counters
```

### 4.6 Tool Execution Lifecycle

```
tool.execute(context, params) called
  │
  ├── 1. Pre-execution
  │       ├── Validate params against schema
  │       ├── Emit notification/tool/started
  │       └── Start execution timer
  │
  ├── 2. Execution
  │       ├── Tool accesses session state (read-only for queries, write for mutations)
  │       ├── Tool may emit progress(0.0..1.0) notifications
  │       ├── Tool checks cancel_token.is_cancelled() periodically
  │       └── On cancel: return ToolError::Cancelled
  │
  ├── 3. Post-execution
  │       ├── Record latency metric
  │       ├── Emit notification/tool/completed
  │       └── Update session idle timer
  │
  └── 4. Return ToolOutput or ToolError
```

### 4.7 Cancellation Flow

```
Client sends "$/cancelRequest" { id: request_id }
  │
  ├── 1. Dispatcher matches cancel request
  │
  ├── 2. Look up active execution by request_id
  │       └── Not found → respond error (already completed or unknown)
  │
  ├── 3. Set CancelToken::cancel() on matched execution
  │
  ├── 4. Running tool detects cancel_token.is_cancelled()
  │       ├── Clean up resources
  │       ├── Emit progress { status: "cancelled" }
  │       └── Return ToolError::Cancelled
  │
  ├── 5. Dispatcher sends cancelled response to original request
  │
  └── 6. Clean up execution tracking
```

---

## 5. Concurrency Model

### 5.1 Thread Model

```
┌──────────────────────────────────────────────────────────────────┐
│                          MAIN THREAD                             │
│                                                                  │
│  - Reads stdin (blocking line-by-line)                           │
│  - Dispatches each message                                       │
│  - For tools/call: spawns execution on WORKER THREAD             │
│  - Writes response to stdout                                     │
│                                                                  │
│  Output serialization: Mutex<Stdout> ensures no interleaving     │
└──────────────────────────────────────────────────────────────────┘
                │
                │ spawn per-call
                ▼
┌──────────────────────────────────────────────────────────────────┐
│                       WORKER THREAD (per call)                   │
│                                                                  │
│  - Receives McpToolContext (owned)                               │
│  - Executes tool logic                                           │
│  - May read/write session state (via RwLock)                    │
│  - Returns result via oneshot channel                            │
│  - If cancelled: cleanup and return early                        │
│                                                                  │
│  Thread count: unbounded (OS thread per concurrent tool call)    │
│  Max: configurable (default: 16)                                 │
│  Wait: semaphore-based throttling                                │
└──────────────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────────────┐
│                     NOTIFICATION THREAD                          │
│                                                                  │
│  - Subscribes to EventBus                                        │
│  - Receives events, builds notifications                         │
│  - Fans out to subscriber channels                               │
│  - Writes to stdout (same Mutex<Stdout> as main thread)          │
└──────────────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────────────┐
│                     SESSION CHECKER THREAD                       │
│                                                                  │
│  - Scans SessionManager every 30s                                │
│  - Closes expired idle sessions                                  │
│  - Emits session/expired notifications                           │
└──────────────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────────────┐
│                     SIGNAL HANDLER THREAD                        │
│                                                                  │
│  - Blocks on SIGTERM/SIGINT                                      │
│  - Triggers graceful shutdown                                    │
└──────────────────────────────────────────────────────────────────┘
```

### 5.2 Synchronization Strategy

| Resource | Strategy | Justification |
|----------|----------|---------------|
| Stdout | Mutex | Single writer, short hold times |
| SessionManager | RwLock<HashMap> | Read-heavy (every tool call), write-light (create/close) |
| ToolRegistry | RwLock<HashMap> | Read during initialization, rarely modified at runtime |
| SessionState (inner) | RwLock | Concurrent reads (query), exclusive writes (mutate) |
| CredentialStore | RwLock | Read-heavy, rare writes |
| CancelToken | AtomicBool + Condvar | Lock-free check, wait notification |
| EventBus subscriptions | RwLock<HashMap> | Read on event dispatch, write on subscribe/unsubscribe |
| Metrics counters | AtomicU64 | Lock-free increment |
| Shutdown flag | AtomicBool | Lock-free check |

### 5.3 Synchronization Rules

1. **No nested locks.** If a tool needs to lock SessionManager and then SessionState, it must lock SessionState first.
2. **No lock hand-off.** A thread must not release lock A and acquire lock B while expecting A to still be valid.
3. **Short lock hold times.** SessionState locks are held only for the duration of the underlying operation.
4. **Lock ordering:** ToolRegistry → SessionManager → SessionState → CredentialStore. Violation is a bug.
5. **Deadlock detection:** Not implemented. Lock ordering discipline is relied upon.

---

## 6. RuntimeContext Integration

```
                    ┌─────────────────────────────┐
                    │       RuntimeContext         │
                    │                              │
                    │  llm: Arc<LlGateway>          │
                    │  browser_pool: Arc<BrowserPool>│
                    │  dag: Arc<DagEngine>          │
                    │  scheduler: Arc<Scheduler>    │
                    │  lifecycle: Arc<LifecycleManager>│
                    │  bus: Arc<EventBus>           │
                    │  config: Arc<RootConfig>      │
                    │  logger: Arc<Logger>          │
                    │  metrics: Arc<MetricsRegistry>│
                    └──────────────┬───────────────┘
                                   │
                                   │ Arc reference shared with McpServer
                                   ▼
┌──────────────────────────────────────────────────────────────────┐
│                           McpServer                              │
│                                                                  │
│  ToolRegistry ──── tools query RuntimeContext subsystems         │
│  SessionManager ── each session creates its own browser instance │
│  NotificationDispatcher ── subscribes to RuntimeContext.bus      │
└──────────────────────────────────────────────────────────────────┘
```

### 6.1 RuntimeContext Extension for MCP

`browseros-runtime::RuntimeContext` must expose two new fields that Phase 6 requires:

```rust
impl RuntimeContext {
    pub fn llm(&self) -> Arc<LlGateway>;
    pub fn browser_pool(&self) -> Arc<BrowserPool>;
}
```

These are populated during `RuntimeContext::init()` or via the builder.

---

## 7. Dependency Graph

```
browseros-mcp
├── browseros-runtime (RuntimeContext)
│   ├── browseros-llm (LlGateway)
│   ├── browseros-dag (DagEngine)
│   ├── browseros-lifecycle (LifecycleManager)
│   ├── browseros-scheduler (Scheduler)
│   ├── browseros-event-bus (EventBus)
│   ├── browseros-config (RootConfig)
│   └── browseros-observability (Logger, MetricsRegistry)
├── browseros-llm (re-export: mcp::jsonrpc, mcp::errors)
├── browseros-types (HandleId, EventType)
├── serde + serde_json
└── chrono
```

No transitive dependency on `browseros-bridge`, `browseros-cdp`, `browseros-browser`, `browseros-dom`, `browseros-page`, or `browseros-storage`. All browser access goes through `RuntimeContext.browser_pool()` which returns a `BrowserPool` (defined in `browseros-browser`).

---

## 8. Extension Points

### 8.1 Plugin Registration (Future)

The `McpTool` trait is the extension point for future plugins:

```rust
pub trait McpTool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn input_schema(&self) -> serde_json::Value;
    fn execute(&self, ctx: McpToolContext, params: serde_json::Value) -> Result<ToolOutput, ToolError>;
    fn capabilities(&self) -> Vec<ToolCapability>;
    fn visibility(&self) -> ToolVisibility;
}
```

External crates can implement `McpTool` and register via `McpServerBuilder::with_tool()`.

### 8.2 Transport Extensibility

Transport is an internal abstraction. The `StdioReader` struct can be replaced with a WebSocket or SSE reader in the future without changing any tool code.

### 8.3 Session Store Extensibility

`SessionManager` stores sessions in memory. The `SessionStore` trait can be extracted for Redis or SQLite persistence in future phases.

---

## 9. Event Flow Diagram

```
┌──────────┐    EventBus     ┌──────────────────────┐
│ Browser  │─────emit──────▶│ NotificationDispatcher │
│ (CDP)    │                 │                      │
└──────────┘                 │  Event → Notification │
                             │  mapping              │
┌──────────┐                 │                      │
│ LlGateway│─────emit──────▶│  SubscriberList       │
└──────────┘                 │  ┌────────────────┐   │
                             │  │ Subscriber 1   │   │
┌──────────┐                 │  │ (session A)    │──▶ stdout
│ DagEngine│─────emit──────▶│  └────────────────┘   │
└──────────┘                 │  ┌────────────────┐   │
                             │  │ Subscriber 2   │   │
┌──────────┐                 │  │ (session B)    │──▶ stdout
│ Workflow │─────emit──────▶│  └────────────────┘   │
└──────────┘                 └──────────────────────┘
```

---

## 10. Extension Points Summary

| Point | Mechanism | Phase |
|-------|-----------|-------|
| New tool | Implement `McpTool`, register via `with_tool()` | 6+ |
| New transport | Implement internal `TransportReader` | 8 |
| Persistent sessions | Extract `SessionStore` trait from `SessionManager` | 8 |
| Plugin system | Load dynamic libs, call `register_tools()` | 8+ |
| Custom auth | Implement `AuthProvider` trait | 8 |
| Custom credential store | Implement `CredentialStore` trait | 7 |

---

*This document defines the complete architecture for browseros-mcp. It contains no Rust code, no Cargo.toml, and no placeholders. It is the single source of truth for implementation.*
