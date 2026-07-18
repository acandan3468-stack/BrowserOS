# Next Recommended Milestone — BrowserOS

**Date:** 2026-07-08  

---

## Recommendation: Implement `browseros-dag` (DAG Engine)

**Do NOT jump to Phase 2.6 (Network) implementation.**

**Do NOT start Phase 3 (Perception/Planning/Skills).**

**Do NOT fix technical debt first.**

---

## Why the DAG Engine?

### 1. It is the single largest missing piece of the runtime

The original Phase 1 plan called for a DAG Engine in Phase 1.4. It was never implemented. The project skipped directly to Phase 2 (Browser Automation). This means:

- **No task orchestration** — There is no way to define a multi-step task with dependencies
- **No parallel execution** — Tasks cannot run concurrently with dependency ordering
- **No retry logic** — The Scheduler has no retry capability; DAG nodes would provide retry per step
- **No execution tracing** — No way to track which steps succeeded/failed in a multi-step operation

### 2. Everything else depends on it

| Missing Feature | Depends on DAG? |
|----------------|-----------------|
| Plugin System | ✅ Yes — Plugin execution requires DAG for task orchestration |
| Integration Tests | ✅ Yes — Meaningful integration tests need multi-step workflows |
| Agent Task Execution | ✅ Yes — Agent tasks are DAGs of atomic operations |
| Retry/Recovery | ✅ Yes — DAG provides per-node retry policies |
| Parallel Browser Actions | ✅ Yes — DAG enables concurrent page operations |

### 3. It is low risk and self-contained

- **No browser integration needed** — Pure runtime crate
- **No new external dependencies** — Can use petgraph (already in plan) or implement simple graph
- **No EventBus dependency required** — Can emit events but doesn't need to consume them
- **Well-understood problem** — DAG execution is a solved problem with clear patterns

### 4. It unblocks the entire execution layer

Once `browseros-dag` exists:
- `browseros-plugin` can be built on top (plugins define DAG nodes)
- Integration tests can test multi-step workflows
- The Scheduler can be integrated as the DAG's execution backend
- The EventBus can carry DAG lifecycle events (node started, node completed, node failed)

---

## What to Build

### Minimum Viable DAG Engine

```
browseros-dag/
├── Cargo.toml
└── src/
    ├── lib.rs           # Public API: DagEngine, DagNode, DagEdge
    ├── graph.rs         # Graph data structure (DAG validation, topological sort)
    ├── engine.rs        # Execution engine (sequential + parallel node execution)
    ├── scheduler.rs     # Node scheduling strategy (topological order)
    └── state.rs         # Node execution state (Pending, Running, Completed, Failed, Skipped)
```

### Key Design Decisions

1. **Synchronous execution** — Consistent with the rest of the runtime. No async needed.
2. **Node types** — `Command` (sync function), `EventTrigger` (wait for EventBus event), `SubDag` (nested DAG)
3. **Retry policy per node** — Configurable retry count + backoff
4. **Event emission** — Each node lifecycle change emits a DAG event on EventBus
5. **Cancellation** — Respect CancellationToken for cooperative cancellation
6. **No persistence** — In-memory only (consistent with Scheduler design)

### API Surface

```rust
pub struct DagEngine {
    // ...
}

impl DagEngine {
    pub fn new(event_bus: Arc<EventBus>) -> Self;
    pub fn register_node(&mut self, node: DagNode) -> Result<NodeId, DagError>;
    pub fn add_edge(&mut self, from: NodeId, to: NodeId) -> Result<(), DagError>;
    pub fn execute(&self, dag: DagId) -> Result<DagResult, DagError>;
    pub fn cancel(&self, dag: DagId) -> Result<(), DagError>;
    pub fn state(&self, dag: DagId) -> Result<DagState, DagError>;
}

pub struct DagNode {
    pub id: NodeId,
    pub name: String,
    pub action: Box<dyn Fn() -> Result<(), DagError> + Send + Sync>,
    pub retry_policy: RetryPolicy,
    pub timeout: Duration,
}

pub enum NodeState {
    Pending,
    Running,
    Completed,
    Failed(String),
    Skipped,
    Cancelled,
}
```

---

## What NOT to Build Yet

### ❌ Do NOT implement Phase 2.6 (Network)
- Network crate has no consumers yet
- DOM crate is frozen but not integrated with any runtime
- Building Network before DAG means building on an incomplete foundation

### ❌ Do NOT fix technical debt
- The builder panics, unwrap() calls, and unsafe code are real issues
- But fixing them now doesn't unlock any new capability
- Fix debt AFTER the DAG engine is built and tested

### ❌ Do NOT implement the Plugin system
- Plugin system depends on DAG for task execution
- Build DAG first, then Plugin on top

### ❌ Do NOT add benchmarks
- Benchmarks are valuable but not blocking
- Add after DAG engine is stable

### ❌ Do NOT implement Event Store / State Store
- Storage is a stub and needs full rework
- But no consumer needs it yet (no event sourcing in current architecture)
- Defer until event sourcing is required

---

## Dependencies Already Satisfied

| Dependency | Status | Notes |
|-----------|--------|-------|
| browseros-types | ✅ Complete | NodeId, DagError, RetryPolicy types exist |
| browseros-event-bus | ✅ Complete | Can emit DAG lifecycle events |
| browseros-scheduler | ✅ Complete | Can be used for delayed node execution |
| browseros-observability | ✅ Complete | Logger, Metrics, Tracer for DAG instrumentation |
| Rust standard library | ✅ Available | HashMap, Vec, BTreeSet for graph structures |

---

## Risks

| Risk | Likelihood | Mitigation |
|------|-----------|------------|
| DAG execution order bugs | Medium | Topological sort with cycle detection + property-based testing |
| Concurrent node execution races | Low | Synchronous execution model avoids this entirely |
| Memory leak from large DAGs | Low | DAGs are bounded by task complexity, not infinite |
| Integration with EventBus | Low | Event emission is one-directional (DAG → EventBus) |

---

## Estimated Effort

| Task | Effort | Dependencies |
|------|--------|-------------|
| Graph data structure + cycle detection | 1 day | None |
| Topological sort | 0.5 day | Graph structure |
| Sequential execution engine | 1 day | Topological sort |
| Parallel execution engine | 1 day | Sequential engine |
| Event emission integration | 0.5 day | EventBus |
| Retry policy per node | 0.5 day | Execution engine |
| Cancellation support | 0.5 day | CancellationToken |
| Unit tests | 1 day | All above |
| Integration tests | 1 day | All above |
| **Total** | **~7 days** | |

---

## Success Criteria

- [ ] `cargo build` — zero warnings
- [ ] `cargo clippy` — zero warnings
- [ ] `cargo test` — all tests pass
- [ ] DAG with 10+ nodes executes in correct topological order
- [ ] Cycle detection rejects invalid DAGs
- [ ] Node failure with retry works correctly
- [ ] Cancellation stops execution mid-DAG
- [ ] DAG lifecycle events emitted on EventBus
- [ ] No new unwrap()/expect()/panic!() in production code
- [ ] All public APIs documented

---

## Summary

**Build `browseros-dag` now.** It is the single most impactful missing component. It unblocks the entire execution layer, enables meaningful integration tests, and provides the foundation for plugins, agent tasks, and all future orchestration needs. It is low risk, self-contained, and has all dependencies already satisfied.

**Everything else can wait.**