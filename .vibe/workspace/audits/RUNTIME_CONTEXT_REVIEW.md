# BrowserOS — RuntimeContext Review

> **Audit Date:** 2026-07-18  
> **Source file:** `browseros-runtime/src/lib.rs`  

---

## 1. Current Structure

```rust
pub struct RuntimeContext {
    pub bus: Arc<InMemoryEventBus>,
    pub logger: Arc<Logger>,
    pub metrics: Arc<MetricsCollector>,
    pub tracer: Arc<Tracer>,
    pub config: Arc<Configuration>,
    pub lifecycle: Arc<LifecycleManager>,
    pub scheduler: Arc<Scheduler>,
    pub dag: Arc<DagEngine>,
}
```

- **Location:** `browseros-runtime/src/lib.rs`  
- **Construction:** `RuntimeBuilder` with builder pattern — each field has a `with_*` method  
- **Default state:** All fields are `Some(...)` — there is no partial construction  
- **Sharing:** All fields are `Arc` — shared by all consumers  
- **Mutability:** `InMemoryEventBus` uses internal `RwLock<HashMap<...>>`, `Scheduler` uses `Mutex<BTreeMap<...>>`, `DagEngine` uses `AtomicBool` — no `Mutex` wrapping at the `RuntimeContext` level  

---

## 2. Projected Phase 6–8 Additions

| Phase | New Field | Type | Notes |
|-------|-----------|------|-------|
| 6 | storage_manager | `Arc<StorageManager>` | Required by MCP tools for persistent storage |
| 6 | browser_pool | `Arc<BrowserPool>` | Required by Planner for parallel browser allocation |
| 6 | ll_gateway | `Arc<LlGateway>` | Required by Planner for LLM integration in workflows |
| 6 | network_handle | `Arc<NetworkHandle>` | Required by Network tools in MCP |
| 6 | session_manager | `Arc<SessionManager>` | Required by Planner for session binding |
| 7 | plugin_registry | `Arc<PluginRegistry>` | Required by Workflow engine |
| 7 | workflow_engine | `Arc<WorkflowEngine>` | Required by Phase 7 |
| 8 | dist_runtime | `Arc<DistributedRuntime>` | Required by Phase 8 |

**Projected total: 16 fields** — up from 8 today.

---

## 3. God Object Risk Assessment

### Risk Level: MODERATE

**Arguments FOR God Object:**
- All subsystems share a single context struct — any subsystem can access any other subsystem's resource
- `RuntimeContext` is passed as-is to all consumers (MCP tools, DAG tasks, scheduler callbacks) — no interface segregation
- Adding a new resource touches `RuntimeContext`, `RuntimeBuilder`, and every consumer that needs the new resource
- Testing requires constructing a full `RuntimeContext` even when only one or two fields are needed
- No scoping mechanism — a scheduler callback has access to the entire runtime

**Arguments AGAINST God Object:**
- Each field is behind `Arc` — zero-cost sharing, no lock contention
- Each field is a distinct type — no monolithic state blob
- Interface segregation can be achieved at the consumer level (each consumer receives only the `Arc` fields it needs, not the whole `RuntimeContext`)
- The builder pattern makes additive changes backward-compatible
- The pattern is idiomatic for Rust application shells (similar to Axum's `AppState`, Actix's `Data`)

---

## 4. Ownership Chains

| Resource | Primary Owner | Shared Via | Consumer Pattern |
|----------|--------------|------------|-----------------|
| EventBus | RuntimeContext | `Arc<InMemoryEventBus>` | `subscribe()` via cloned Arc |
| Logger | RuntimeContext | `Arc<Logger>` | `log_info!()` / `log_error!()` macros |
| MetricsCollector | RuntimeContext | `Arc<MetricsCollector>` | `record_counter!()` / `record_histogram!()` macros |
| Tracer | RuntimeContext | `Arc<Tracer>` | `trace_span!()` macro |
| Configuration | RuntimeContext | `Arc<Configuration>` | `config.get_string("key")` |
| LifecycleManager | RuntimeContext | `Arc<LifecycleManager>` | `lifecycle.transition_to()` |
| Scheduler | RuntimeContext | `Arc<Scheduler>` | `scheduler.schedule_after()` |
| DagEngine | RuntimeContext | `Arc<DagEngine>` | `dag.submit(task)` |

**Observation:** All resources are `Arc<Mutex<T>>` or `Arc<RwLock<T>>` internally (except Logger and Tracer which use lock-free atomics). The `Arc` at the `RuntimeContext` level is for sharing the outer handle, not for synchronization.

---

## 5. Arc/Mutex Analysis

| Field | Internal Locking | Lock Granularity | Contention Risk |
|-------|-----------------|------------------|-----------------|
| bus | `RwLock<HashMap>` | Hashmap-level | Low — publish is write-lock, subscribe is write-lock (same lock) |
| logger | None (lock-free) | N/A | None |
| metrics | `Mutex<Vec<...>>` | Vec-level | Low — counter/event recording is fast |
| tracer | None (lock-free) | N/A | None |
| config | `RwLock<BTreeMap>` | Map-level | Low — reads dominate writes |
| lifecycle | `Mutex<StateMachine>` | State-machine-level | Low — infrequent transitions |
| scheduler | `Mutex<BTreeMap>` | Map-level | Low — schedule_after / cancel ops are fast |
| dag | `AtomicBool` (running flag) | Boolean-level | None |

**No double-locking risk:** No `RuntimeContext` method locks multiple fields.

**No deadlock risk:** All locks are internal to the component and never acquired across components.

---

## 6. Provider Pattern Recommendation

**Current state:** Each new subsystem adds a field to `RuntimeContext`. Each consumer takes the whole `RuntimeContext` or specific `Arc` fields via constructor injection.

**Recommended evolution (Phase 8 readiness):**

```rust
// Phase 8 target — hierarchical providers
pub struct RuntimeContext {
    // Core infrastructure (always present)
    core: Arc<CoreProvider>,
    // Domain services (created on demand, swappable)
    services: Arc<ServiceProvider>,
    // Extension services (plugin-loaded, optional)
    extensions: Arc<ExtensionRegistry>,
}

pub struct CoreProvider {
    bus: Arc<InMemoryEventBus>,
    config: Arc<Configuration>,
    logger: Arc<Logger>,
    metrics: Arc<MetricsCollector>,
    tracer: Arc<Tracer>,
}

pub struct ServiceProvider {
    lifecycle: Option<Arc<LifecycleManager>>,
    scheduler: Option<Arc<Scheduler>>,
    dag: Option<Arc<DagEngine>>,
    storage: Option<Arc<StorageManager>>,
    browser: Option<Arc<BrowserPool>>,
    llm: Option<Arc<LlGateway>>,
    network: Option<Arc<NetworkHandle>>,
    session: Option<Arc<SessionManager>>,
    workflow: Option<Arc<WorkflowEngine>>,
    plugin: Option<Arc<PluginRegistry>>,
    dist: Option<Arc<DistributedRuntime>>,
}
```

**Benefits:**
- Consumers can be constructed with only the providers they need
- Test construction is lighter (no need to provide all 16 fields)
- Version compatibility: old code that only reads core can work unchanged
- Extension loading via `ExtensionRegistry` for Phase 7 plugins

**Cost:**
- Indirection layer adds mental overhead
- No compile-time guarantee that a service exists (Option<Arc<...>>)
- Initial implementation effort (~2 days)

---

## 7. Conclusions

1. **Current design is acceptable for Phase 6.** With 8 fields and only 4–5 new fields projected for Phase 6, the God Object risk is manageable.

2. **Provider pattern should be introduced before Phase 8.** At 14–16 fields, the struct becomes unwieldy. The hierarchical provider pattern is a clean migration path.

3. **No immediate changes required.** The builder pattern and additive field approach are backward-compatible. New fields can be added today without refactoring existing consumers.

4. **Test construction is the primary pain point.** Writing tests for components that consume `RuntimeContext` requires constructing the full struct. A `RuntimeContext::mock()` or `test_context!()` macro would improve test ergonomics immediately.
