# DAG Engine Analysis — browseros-dag

**Status:** DESIGN  
**Date:** 2026-07-13  
**Source:** Codebase reconstruction and architecture audit  

---

## 1. Why a DAG Engine?

BrowserOS currently has **no execution layer**. The runtime provides event routing (EventBus), timing (Scheduler), and state tracking (LifecycleManager), but there is no way to define, execute, or observe a multi-step task.

### Current Capabilities

| Capability | Status | Limitation |
|-----------|--------|------------|
| Event pub/sub | ✅ EventBus | Fire-and-forget, no causal ordering |
| Delayed execution | ✅ Scheduler | Single-shot only |
| Event-triggered hooks | ✅ Scheduler | Single event → single callback |
| Lifecycle tracking | ✅ LifecycleManager | String-keyed, no dependency ordering |
| **Multi-step task orchestration** | ❌ Missing | **No DAG engine** |
| **Parallel execution** | ❌ Missing | No concurrency control |
| **Retry with backoff** | ❌ Missing | RetryPolicy exists in types but unused |
| **Cancellation** | ⚠️ Partial | CancellationToken exists, no DAG-level cancel |
| **Execution tracing** | ❌ Missing | No span of execution across steps |

### What a DAG Engine Unlocks

| Downstream System | DAG Dependency | Priority |
|------------------|----------------|----------|
| Plugin System (browseros-plugin) | DAG provides execution primitives for plugin tasks | Critical |
| Agent Task Execution | Agent plans are DAGs of atomic operations | Critical |
| Parallel Browser Actions | DAG enables concurrent page/session operations | High |
| Retry/Recovery | Per-node retry policies from DAG | High |
| Integration Tests | Multi-step workflow testing | High |
| Network crate (Phase 2.6) | DAG for request interception chains | Medium |
| Perception Layer | Multi-step observation pipelines | Medium |

---

## 2. Existing Infrastructure That DAG Integrates With

### 2.1 browseros-types

Already provides:
- `NodeId` (string_id!) — DAG node identification
- `ExecutionId` (uuid_id!) — DAG execution run ID
- `TaskId` (uuid_id!) — Compatible with Scheduler task IDs
- `RetryPolicy` — ExponentialBackoff + Immediate retry types
- `RetryPolicy::next_delay(attempt)` — Delay calculation
- `CancellationToken` — Cooperative cancellation
- `BrowserOsError` — ErrorKind::Transient for retryable errors
- `EventId`, `CorrelationId`, `CausationId` — Event tracing
- `ErrorSeverity` — Error classification

### 2.2 browseros-event-bus

Provides:
- `EventBus::publish()` — DAG lifecycle event emission
- `EventBus::subscribe()` — DAG state observation
- `EventBus::unsubscribe()` — Cleanup

### 2.3 browseros-scheduler

Provides:
- `Scheduler::schedule_after()` — Delayed node execution (retry backoff)
- `Scheduler::on_event()` — Event-triggered node wake-up

### 2.4 browseros-lifecycle

Provides:
- `LifecycleManager::register_component()` — DAG engine component registration
- `LifecycleManager::transition_to()` — DAG engine state changes
- `LifecycleManager::current_state()` — DAG engine health query

### 2.5 browseros-observability

Provides:
- `Logger` — Structured DAG execution logging
- `MetricsRegistry` — DAG execution counters/gauges
- `Tracer` — Span-based DAG execution tracing

---

## 3. Architectural Constraints

The DAG engine must follow all existing architecture rules:

| Rule | Constraint |
|------|-----------|
| **Sync API** | All public methods are synchronous |
| **No async runtime** | Uses `std::thread` like Scheduler |
| **Arc-based ownership** | All shared state behind Arc |
| **Send + Sync** | All public traits require Send + Sync |
| **Event emission** | DAG lifecycle events on EventBus |
| **No persistence** | In-memory only (like Scheduler) |
| **No RuntimeContext dependency** | Receives components via constructor |
| **No browser crate dependency** | Pure runtime crate |

---

## 4. Dependency Direction

```
browseros-types
    │
    ├── browseros-event-bus
    ├── browseros-scheduler
    │
    ├── browseros-dag (NEW)
    │   ├── EventBus (for event emission)
    │   ├── Scheduler (for retry timing)
    │   └── types (NodeId, ExecutionId, RetryPolicy, CancellationToken)
    │
    ├── browseros-plugin (FUTURE — depends on DAG)
    └── browseros-runtime (adds dag: Arc<DagEngine> field)
```

**Circular dependency risk:** NONE. DAG depends only on types, event-bus, and scheduler. Nothing depends on DAG yet (plugin system is future).

---

## 5. Key Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Execution model | Synchronous, thread-pool | Consistent with existing sync runtime |
| Node runners | `Box<dyn Fn() -> Result<(), DagError>>` | Simple, type-safe, no trait required |
| DAG representation | Adjacency list (HashMap) | Simple, sufficient for <1000 nodes |
| Cycle detection | Tarjan's algorithm | O(V+E), detects all cycles |
| Retry per node | Optional RetryPolicy on DagNode | Reuses existing types::RetryPolicy |
| Parallelism | Tokio-free, custom thread pool | Avoids async runtime dependency |
| Event emission | One-way DAG → EventBus | No EventBus dependency for operation |
| Cancellation | CancellationToken per execution | Consistent with existing pattern |
| Nesting | SubDag node type | Reuses same DagEngine recursively |

---

## 6. Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Cycle detection bugs | Low | Medium | Tarjan's algorithm is well-understood; property-based tests |
| Thread pool starvation | Low | High | Configurable thread count; task timeout per node |
| Memory leak from large DAGs | Low | Low | DAGs are bounded by task count; explicit cleanup API |
| Lock contention on state | Low | Medium | RwLock for read-heavy state access |
| EventBus overflow from DAG events | Low | Medium | One event per node lifecycle transition (bounded) |

---

## 7. Scope Boundaries

### In Scope (Phase 1)

- Sequential DAG execution (topological order)
- Parallel execution of independent nodes
- Cycle detection at registration and execution time
- Per-node retry with RetryPolicy
- Cooperative cancellation via CancellationToken
- Event emission (node.started, node.completed, node.failed, dag.completed)
- Observability integration (Logger, Metrics, Tracer)
- SubDag node type for nested DAGs
- Public API: register_node, add_edge, execute, cancel, state, list

### Out of Scope (Phase 1)

- DAG persistence (consistent with Scheduler)
- Distributed execution (in-process only)
- Dynamic DAG mutation during execution
- Priority-based scheduling
- Deadline per DAG (per-node timeout only)
- DAG visualization/graph export
- Plugin integration (future crate)
- Event sourcing for DAG execution history

---

## 8. Success Criteria

- [ ] 10+ node DAG executes in correct topological order
- [ ] Parallel nodes execute concurrently (not sequentially)
- [ ] Cycle detection rejects invalid DAGs at registration time
- [ ] Node failure triggers retry when RetryPolicy configured
- [ ] Cancellation stops execution mid-DAG
- [ ] DAG lifecycle events emitted on EventBus (node.*, dag.*)
- [ ] Metrics counters for nodes started/completed/failed
- [ ] Trace spans for each node execution
- [ ] Zero unwrap()/expect()/panic!() in production code
- [ ] All public APIs have doc comments
