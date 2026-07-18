# Session Resource Ownership Model

> **Design Phase:** Phase 6.2  
> **Status:** Design Freeze Candidate  

---

## 1. Resource Ownership Matrix

| Resource | Owner | Created | Destroyed | Sharing | Thread Safety |
|----------|-------|---------|-----------|---------|---------------|
| BrowserInstance | Session (borrowed from BrowserPool) | Starting (eager) or first resource() call (lazy) | Closing (returned to pool) | Exclusive to one session at a time. Not shareable between sessions. | `Arc<Mutex<BrowserInstance>>` |
| DOM snapshot | Session | On demand via snapshot() | Session Close + GC | Intra-session sharing via SessionGuard. Not cross-session. | `Arc<RwLock<NodeSnapshot>>` |
| Network session | Session (borrowed from NetworkManager) | Starting (eager) | Closing (returned to manager) | Exclusive to session. Not shareable. | `Arc<NetworkSession>` |
| LLM conversation | Session (via LlGateway) | On first LLM call | Session Close | Intra-session sharing (multiple tools in same sesion share conversation history by default). | `Arc<Mutex<ConversationContext>>` |
| Workflow execution | Session | On Planner submit to session | Workflow complete or session Close | One active workflow per session by default. Session can own multiple if configured. | `Arc<RwLock<WorkflowState>>` |
| Event subscriptions | Session | On first subscribe() call from session | Session Close | Subscriptions are session-scoped by default. Cross-session events use global bus. | Handled by EventBus (internal locks) |
| Storage namespace | Session (via StorageManager) | Starting (eager) | Closing (released to manager, data may persist) | Exclusive write namespace per session. Read-only shared access possible. | `Arc<StorageNamespace>` |
| Tool context | Tool itself (session is scope) | On first tool execution | When tool completes | Per-tool, per-session. Not shared between tools. | Local to tool execution thread |
| Plugin state | Session | On first plugin access | Session Close | Intra-session sharing between plugins if explicitly allowed. | `Arc<Mutex<PluginStateMap>>` |

---

## 2. Detailed Ownership Analysis

### 2.1 BrowserInstance

```
Owner:           Session (borrowed)
Creation:        Pool.acquire(session_id) — called during Starting or on first browser access
Destruction:     Pool.release(browser_id) — called during Closing
Sharing:         EXCLUSIVE — one session at a time
Thread Safety:   Arc<Mutex<BrowserInstance>>
Implementation:  BrowserPoolProvider::acquire(session_id) -> Result<Arc<Mutex<BrowserInstance>>>
                 BrowserPoolProvider::release(session_id, browser_id) -> Result<()>
```

The session does NOT own the BrowserInstance in the Rust ownership sense. The pool retains ownership and loans it to the session. The session holds a `BrowserLease` (similar to SessionGuard but for browser) that, when dropped, returns the browser to the pool.

If a session requests a second browser (max_browsers > 1), each browser is a separate lease.

On session crash (process dies), the pool detects the missing heartbeat and reclaims the browser.

### 2.2 DOM Snapshot

```
Owner:           Session
Creation:        On demand — guard.resource::<DomSnapshot>() triggers snapshot
Destruction:     On Session Close (Arc dropped) or GC
Sharing:         Intra-session: all tools in the same session can access the cached snapshot
Thread Safety:   Arc<RwLock<NodeSnapshot>>
Implementation:  Internally: Session holds HashMap<ResourceType, Arc<dyn Any + Send + Sync>>
```

DOM snapshots are cached in the session's resource map. A tool can request a fresh snapshot (invalidating cache) or use the cached one. The snapshot is a point-in-time capture — it does not auto-refresh.

### 2.3 Network Session

```
Owner:           Session (borrowed from NetworkManager)
Creation:        Starting (eager strategy) or on first network access (lazy)
Destruction:     Closing — returned to NetworkManager
Sharing:         EXCLUSIVE to session
Thread Safety:   Arc<NetworkSession> (internal Mutex for cookies/auth state)
Implementation:  NetworkManager.create_session(session_id) -> Result<Arc<NetworkSession>>
                 NetworkManager.destroy_session(session_id) -> Result<()>
```

Each session gets an isolated network context: separate cookie jar, separate auth state, separate connection pool. This prevents cross-session data leakage.

### 2.4 LLM Conversation

```
Owner:           Session
Creation:        On first LLM call from session
Destruction:     On session Close
Sharing:         Intra-session: tools can share conversation context
Thread Safety:   Arc<Mutex<ConversationContext>>
Implementation:  ConversationContext { messages: Vec<Message>, model: String, system_prompt: Option<String> }
```

Multiple tools within the same session share the LLM conversation history by default. A tool can opt out by creating an isolated sub-conversation. The conversation context supports branching (fork from message N).

### 2.5 Workflow Execution

```
Owner:           Session
Creation:        Planner calls session.bind_workflow(workflow_id)
Destruction:     Workflow completes or session Close
Sharing:         One active workflow per session (configurable)
Thread Safety:   Arc<RwLock<WorkflowState>>
Implementation:  WorkflowState { dag_id, current_step, status, errors: Vec<WorkflowError> }
```

The workflow engine binds a DAG execution to the session. The session provides the resource context for each workflow step. When the session closes, the workflow receives a cancellation signal.

### 2.6 Event Subscriptions

```
Owner:           Session
Creation:        On first subscribe() with session-scoped filter
Destruction:     On session Close (all subscriptions cancelled)
Sharing:         Not shared — each subscription is unique per session
Thread Safety:   Handled by EventBus
Implementation:  Session-scoped EventFilter: { session_id: Some(id) }
                 On Close: SessionManager unsubscribes all session-scoped handlers
```

Session-scoped events are filtered by session_id on the EventBus. When a session closes, all its subscriptions are automatically cleaned up to prevent handler leaks.

### 2.7 Storage Namespace

```
Owner:           Session (data persisted beyond session)
Creation:        Starting (eager) — assigns namespace prefix based on session_id
Destruction:     Closing — flushes buffers, releases namespace handle
Sharing:         Write-exclusive per session. Read-only can be shared.
Thread Safety:   Arc<StorageNamespace>
Implementation:  namespace = storage_manager.namespace(format!("session/{}", session_id))
                 On close: storage_manager.release_namespace(namespace)
```

Storage data survives the session (persistent). The session ID maps to a storage prefix, enabling data recovery and audit. On session close, the namespace handle is released but the data remains (configurable: ephemeral sessions delete data on close).

### 2.8 Tool Context

```
Owner:           Individual tool execution (scoped to session)
Creation:        When a tool acquires the session and starts execution
Destruction:     When tool execution completes
Sharing:         Not shared — each tool invocation gets fresh context
Thread Safety:   Local to the executing thread
Implementation:  ToolContext { session_id, tool_name, parameters, state: HashMap<String, Value> }
```

Each tool invocation gets its own context that is scoped to the session but not shared between different tool calls. Tools can store per-invocation state in the context. This state is NOT serialized across tool boundaries.

### 2.9 Plugin State

```
Owner:           Session
Creation:        On first plugin access within the session
Destruction:     On session Close
Sharing:         Intra-session: plugins can share state explicitly
Thread Safety:   Arc<Mutex<HashMap<PluginId, PluginState>>>
Implementation:  Lazy-init HashMap; state is Plugin-specific serializable blob
```

Plugin instances are per-session. A plugin loaded in Session A has no access to the state of the same plugin in Session B. This provides isolation while allowing plugins to maintain state across multiple tool calls within the same session.

---

## 3. Resource Trait

```rust
trait SessionResource: Send + Sync {
    fn resource_type(&self) -> ResourceType;
    fn is_alive(&self) -> bool;                 // Health check
    fn on_acquire(&self) -> Result<(), ResourceError>;
    fn on_release(&self) -> Result<(), ResourceError>;
}
```

All bound resources implement this trait. The Session uses it for lifecycle management:
- During acquire(): calls on_acquire() on all bound resources.
- During release(): calls on_release() in reverse dependency order.
- Health: calls is_alive() during health check.

---

## 4. Resource Dependency Order

Resources have implicit dependencies that determine release order:

```
BrowserInstance (no deps)
DOM Snapshot (depends on BrowserInstance — needs browser for fresh snapshots)
Network Session (no deps)
LLM Conversation (no deps)
Storage Namespace (no deps)
Event Subscriptions (no deps)
Tool Context (depends on all above — must be released first)
Plugin State (depends on all above — must be released first)
Workflow Execution (depends on all above — must be released first)
```

Release order: Workflow → Plugin → Tool → Browser → Network → LLM → Storage → Events → DOM.

Acquisition order is the reverse: DOM → Events → Storage → LLM → Network → Browser → Tool → Plugin → Workflow.

This ensures that when a session closes, active workflows are cancelled before browsers are released, and browsers are released before the network session that might be needed for CDP communication.

---

## 5. Resource Binding Internal API

Internal to Session (not public):

```rust
struct SessionInner {
    state: SessionState,
    config: SessionConfig,
    resources: HashMap<ResourceType, Arc<dyn SessionResource>>,
    lease_tokens: HashMap<ResourceType, LeaseToken>,   // For pool-returned resources
    timers: TimerSet,
    last_activity: Instant,
    metadata: HashMap<String, String>,
}

impl SessionInner {
    fn bind_resource(&mut self, resource: Arc<dyn SessionResource>);
    fn unbind_resource(&mut self, resource_type: ResourceType) -> Result<LeaseToken>;
    fn release_all(&mut self) -> Vec<ResourceReleaseResult>;
    fn resource<T: SessionResource>(&self, resource_type: ResourceType) -> Option<Arc<T>>;
}
```

The public API exposes only `SessionGuard::resource::<T>()` which internally looks up the resource by TypeId in the HashMap.
