# Session Manager Architecture

> **Design Phase:** Phase 6.2  
> **Status:** Design Freeze Candidate  
> **Target Crates:** `browseros-session` (new), `browseros-runtime` (modify), `browseros-browser` (modify)  

---

## 1. Overview

The SessionManager is the central resource orchestrator for BrowserOS. Every long-lived interaction between a client and the runtime is modeled as a Session. Sessions own, bind, and coordinate browser instances, network contexts, LLM conversations, storage namespaces, tool state, and plugin instances.

```
      MCP Client          Planner          Workflow Engine
          │                   │                   │
    ┌─────┴───────────────────┴───────────────────┴─────┐
    │                  SessionManager                    │
    │  ┌──────────────────────────────────────────────┐  │
    │  │  SessionRegistry (Map<SessionId, Session>)   │  │
    │  │  + LifecycleManager                          │  │
    │  │  + ResourceTracker                           │  │
    │  │  + GarbageCollector                          │  │
    │  └──────┬───────────────────────────────────────┘  │
    └─────────┼──────────────────────────────────────────┘
              │
    ┌─────────┴──────────────────────────────────────────┐
    │               Resource Providers                    │
    │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌───────┐ │
    │  │BrowserPool│ │NetManager│ │LlGateway │ │Storage│ │
    │  └──────────┘ └──────────┘ └──────────┘ └───────┘ │
    └────────────────────────────────────────────────────┘
```

---

## 2. Design Principles

### 2.1 Sessions are resources, not identities
A Session is a container for runtime resources allocated to complete a unit of work. It is not a user identity or authentication principal. Authentication belongs to a separate layer (not in Phase 6.2).

### 2.2 One session, one owner
Every session has exactly one owning client at any time. Ownership can outlive the client connection (surviving disconnect). Sessions are never shared across uncoordinated clients.

### 2.3 Pool independence
SessionManager USES BrowserPool, LlGateway, StorageManager — it does NOT own them. Resource providers remain independent, testable, and swappable.

### 2.4 Explicit state machine
Session state transitions are governed by a compile-time validated state machine. Illegal transitions are rejected at runtime with typed errors.

### 2.5 Leak resistance
Every transition that acquires a resource must have a corresponding release path. The system enforces that resources are freed when a session closes, even on failure.

---

## 3. System Context

### 3.1 Crate Dependency Graph

```
browseros-session (NEW)
  depends_on:
    - browseros-types       (SessionId, errors)
    - browseros-event-bus   (publish session events)
    - browseros-config      (pool size, timeouts)
    - browseros-browser?    (BrowserPool trait — or define trait in
                             browseros-types/browseros-session)
    - browseros-llm?        (LlGateway trait — indirect)
    - browseros-storage?    (StorageNamespace trait — indirect)

RuntimeContext (MODIFY)
  adds: session_manager: Arc<SessionManager>

BrowserPool (MODIFY)
  adds: acquire(), release(), health(), resize()
```

### 3.2 Crate Boundary Strategy

`browseros-session` defines `BrowserPoolProvider` and `StorageProvider` as traits. Concrete implementations live in `browseros-browser` and `browseros-storage`. This prevents `browseros-session` from depending on implementation crates.

```
browseros-session
  ├── trait BrowserPoolProvider
  ├── trait StorageProvider  
  └── trait LlmProvider

browseros-browser → impl BrowserPoolProvider
browseros-storage → impl StorageProvider
browseros-llm     → impl LlmProvider
```

Alternatively, if the trait set is small and stable, traits live in `browseros-types` to minimize crate count.

---

## 4. Module Structure

```
browseros-session/
├── Cargo.toml
├── src/
│   ├── lib.rs              # Re-exports
│   ├── manager.rs          # SessionManager impl
│   ├── session.rs          # Session struct + state machine
│   ├── state.rs            # State enum + transition validation
│   ├── resource.rs         # ResourceSet + Resource trait
│   ├── registry.rs         # SessionRegistry (concurrent map)
│   ├── gc.rs               # GarbageCollector
│   ├── config.rs           # SessionConfig, PoolConfig
│   ├── error.rs            # SessionError enum
│   ├── event.rs            # Event definitions
│   ├── stats.rs            # SessionStatistics collector
│   └── providers/
│       ├── mod.rs          # Re-exports
│       ├── browser_pool.rs # BrowserPoolProvider trait
│       ├── storage.rs      # StorageProvider trait
│       └── llm.rs          # LlmProvider trait
```

---

## 5. Key Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| SessionId type | UUID v7 | Time-ordered, no-collision, DB-indexable, no coordination needed |
| State machine | Enum + transition matrix | Compile-time safety, impossible transitions rejected at compile time |
| Session storage | `HashMap<SessionId, Arc<Session>>` + RwLock | Read-heavy workload (get, list, exists) |
| Per-session state | `Mutex<SessionInner>` | Fine-grained locking, no global lock contention |
| BrowserPool coupling | Trait (not direct dep) | Keeps browseros-session independent of browseros-browser |
| Event publishing | Always async via EventBus | Decouples SessionManager from event consumers |
| MCP session passing | Tool parameter (SessionId) | Explicit, no hidden global state |
| Session persistence | Optional (Phase 6.3 feature) | Not required for initial implementation |
| Heartbeat mechanism | Optional (Planner/Workflow phase) | Not required for initial SessionManager |
| Resource transfer | Explicit transfer() API | Prevents accidental resource leaks |

---

## 6. Session Structure (Logical)

```
Session {
    id: SessionId (UUID v7)
    state: Mutex<SessionState>
    config: SessionConfig
    resources: ResourceSet
    created_at: Instant
    last_activity: AtomicInstant (AtomicI64 for epoch ms)
    event_bus: Arc<InMemoryEventBus> (for publishing session events)
    metadata: HashMap<String, String> (user-defined tags)
}
```

ResourceSet contains Option<Arc<T>> for each resource type. Resources are acquired lazily or eagerly depending on SessionConfig.

---

## 7. Future-Proofing

### Support for Planner (Phase 6.3+)
- Sessions can own workflow execution state
- Planner creates a Session per workflow run
- Workflow steps acquire/release the same session

### Support for Plugin System (Phase 7)
- Sessions can own plugin instances
- Plugin state is scoped to a session, not global
- Session provides capability context to plugins

### Support for Distributed Runtime (Phase 8)
- Sessions can be serialized and migrated
- SessionId is globally unique (UUID v7)
- Session state machine supports remote transitions
- Resource providers can be remote proxies

---

## 8. Architectural Invariants

1. A session MUST NOT leak resources on Close — every acquire has a paired release.
2. A session MUST NOT transition from Closed or Failed to any other state (terminal states).
3. SessionManager MUST NOT hold a lock on SessionRegistry while acquiring resources from a provider.
4. BrowserPool MUST operate correctly without a SessionManager (standalone).
5. Session MUST NOT depend on RuntimeContext — only on specific provider traits.
6. Session API MUST NOT expose internal Mutex locks to callers (no borrow of locked data).
7. SessionManager.create() MUST return an error if the system is in shutdown.
8. SessionManager.shutdown() MUST wait for all Busy sessions to reach a terminal state (with timeout).
