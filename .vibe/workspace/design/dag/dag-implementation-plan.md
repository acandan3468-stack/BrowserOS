# DAG Engine Implementation Plan — browseros-dag

**Status:** DESIGN  
**Date:** 2026-07-13  
**Estimated effort:** ~7 days (single developer)  

---

## 1. Implementation Order

| Phase | Task | Effort | Dependencies | Tests |
|-------|------|--------|-------------|-------|
| **P1** | Crate scaffolding + Cargo.toml | 0.5d | None | 0 |
| **P2** | Error types (error.rs) | 0.5d | P1 | 2 |
| **P3** | Graph data structure (graph.rs) | 1d | P2 | 8 |
| **P4** | Node types (node.rs) | 0.5d | P2 | 4 |
| **P5** | Topological sort (scheduler.rs) | 0.5d | P3 | 5 |
| **P6** | Event types (events.rs) | 0.5d | P2 | 0 |
| **P7** | Sequential executor (executor.rs) | 1d | P4, P5 | 4 |
| **P8** | Retry logic | 0.5d | P7 | 3 |
| **P9** | Parallel executor | 1d | P7 | 3 |
| **P10** | Cancellation support | 0.5d | P7 | 2 |
| **P11** | Event emission | 0.5d | P6, P9 | 3 |
| **P12** | Observability integration | 0.5d | P9 | 2 |
| **P13** | DagEngine public API (engine.rs) | 1d | P3-P12 | 8 |
| **P14** | RuntimeContext integration | 0.5d | P13 | 2 |
| **P15** | Integration tests | 1d | P14 | 7 |
| **P16** | Stress + property tests | 1d | P15 | 10 |
| **P17** | Polish + documentation | 0.5d | P16 | 0 |
| **Total** | | **~10d** | | **~45** |

---

## 2. Day-by-Day Plan

### Day 1: Foundation (P1-P3)

**Objective:** Buildable crate with graph data structure.

1. **Crate scaffolding** — Cargo.toml, lib.rs, module declarations
2. **DagError** — Define all error variants, Display impl, Error impl
3. **DagGraph** — Adjacency list, node registry, edge validation
4. **Cycle detection** — Tarjan's algorithm implementation
5. **Unit tests** — 8 graph tests, 2 error tests

**Files created:**
- `browseros-dag/Cargo.toml`
- `browseros-dag/src/lib.rs`
- `browseros-dag/src/error.rs`
- `browseros-dag/src/graph.rs`

**Verification:** `cargo build`, `cargo test -p browseros-dag` — 10 tests passing

### Day 2: Node + Scheduling (P4-P6)

**Objective:** Node definitions and topological sort.

1. **DagNode** — NodeKind (Command, SubDag), retry policy, timeout
2. **NodeState** — Pending, Running, Completed, Failed, Skipped, Cancelled
3. **Topological sort** — Kahn's algorithm, layer generation
4. **DAG event types** — All 9 event structs implementing Event trait
5. **Unit tests** — 4 node tests, 5 scheduler tests

**Files created:**
- `browseros-dag/src/node.rs`
- `browseros-dag/src/scheduler.rs`
- `browseros-dag/src/events.rs`

**Verification:** `cargo test -p browseros-dag` — 19 tests passing

### Day 3: Execution Engine (P7-P10)

**Objective:** Working sequential + parallel execution with retry and cancellation.

1. **SyncExecutor** — Thread pool, layer-by-layer execution
2. **Sequential execution** — Single-threaded fallback
3. **Parallel execution** — Multi-threaded layer dispatch
4. **Retry logic** — RetryPolicy integration, delay via Scheduler
5. **Cancellation** — CancellationToken, skip pending nodes
6. **Unit tests** — 12 executor tests

**Files created:**
- `browseros-dag/src/executor.rs`

**Verification:** `cargo test -p browseros-dag` — 31 tests passing

### Day 4: API + Integration (P11-P14)

**Objective:** Complete public API with RuntimeContext integration.

1. **Event emission** — Wire executor to EventBus publish
2. **Observability** — Logger calls, metrics counters, trace spans
3. **DagEngine** — Register, add_edge, execute, cancel, state
4. **RuntimeBuilder update** — Add DagEngine construction
5. **RuntimeContext** — Add `dag` field + accessor
6. **Unit tests** — 8 engine tests, 2 runtime tests

**Files created:**
- `browseros-dag/src/engine.rs`

**Files modified:**
- `browseros-runtime/src/lib.rs`

**Verification:** `cargo build --workspace`, `cargo test --workspace` — all existing tests pass

### Day 5: Integration Tests (P15)

**Objective:** Cross-component integration tests.

1. **DAG + EventBus** — Events emitted correctly
2. **DAG + Scheduler** — Retry timing works
3. **DAG + Observability** — Metrics + tracing
4. **DAG + RuntimeContext** — Full integration

**Files created:**
- `browseros-dag/tests/dag_integration.rs`
- `browseros-dag/tests/dag_observability.rs`
- `browseros-dag/tests/dag_runtime.rs`

**Verification:** `cargo test --workspace` — all 40+ tests passing

### Day 6: Stress + Property Tests (P16)

**Objective:** High-load verification.

1. **Large DAG stress** — 100-node DAG
2. **High parallelism** — 20 concurrent nodes
3. **Concurrent executions** — 10 simultaneous DAGs
4. **Retry storm** — 10 retrying nodes
5. **Property-based tests** — Random DAG properties

**Files created:**
- `browseros-dag/tests/dag_stress.rs`
- `browseros-dag/tests/dag_proptest.rs`

**Verification:** `cargo test --workspace -- --include-ignored` — all stress tests pass

### Day 7: Polish + Documentation (P17)

**Objective:** Production-ready documentation and hardening.

1. **Doc comments** — All public APIs documented
2. **Code review** — Self-review against invariants
3. **Clippy fix** — Zero warnings
4. **README** — Crate-level documentation
5. **Final verification** — `cargo clippy --workspace`, `cargo fmt --check`

**Verification:** All exit criteria met

---

## 3. Implementation Details

### 3.1 Graph Data Structure

```rust
pub(crate) struct DagGraph {
    nodes: HashMap<NodeId, DagNode>,
    edges: HashMap<NodeId, Vec<NodeId>>,       // adjacency list
    reverse_edges: HashMap<NodeId, Vec<NodeId>>, // reverse for dependency lookup
}

impl DagGraph {
    pub fn new() -> Self;
    
    pub fn register_node(&mut self, node: DagNode) -> Result<(), DagError>;
    
    pub fn add_edge(&mut self, from: NodeId, to: NodeId) -> Result<(), DagError> {
        // 1. Validate both nodes exist
        // 2. Check for cycle (Tarjan's)
        // 3. Add edge
    }
    
    /// Kahn's algorithm for topological sort
    pub fn topological_sort(&self) -> Result<Vec<Vec<NodeId>>, DagError> {
        // 1. Compute in-degree for each node
        // 2. Queue nodes with in-degree 0
        // 3. Process queue, decrement in-degree of dependents
        // 4. Group by layer
    }
}
```

### 3.2 Tarjan's Algorithm for Cycle Detection

```rust
impl DagGraph {
    pub fn has_cycle(&self) -> bool {
        // Tarjan's strongly connected components algorithm
        // If any SCC has more than 1 node, there's a cycle
        // O(V + E) time, O(V) space
    }
}
```

### 3.3 Thread Pool

```rust
pub(crate) struct ThreadPool {
    workers: Vec<std::thread::JoinHandle<()>>,
    sender: crossbeam_channel::Sender<Job>,
    // OR simpler: std::sync::Mutex<Vec<JoinHandle<()>>>
}

impl ThreadPool {
    pub fn new(size: usize) -> Self;
    
    pub fn execute<F>(&self, f: F) 
    where F: FnOnce() + Send + 'static;
}
```

**Note:** To minimize external dependencies, consider using a simple `Mutex<Vec<JoinHandle<()>>>`-based pool that spawns threads per job and reaps completed ones. Only add `crossbeam` or `rayon` if performance requirements demand it.

### 3.4 Execution Algorithm

```rust
impl SyncExecutor {
    pub fn execute(
        &self,
        graph: &DagGraph,
        entry_nodes: &[NodeId],
        cancellation: CancellationToken,
    ) -> Result<DagResult, DagError> {
        // 1. Topological sort → layers
        let layers = graph.topological_sort()?;
        
        // 2. For each layer:
        for layer in &layers {
            if cancellation.is_cancelled() {
                return Err(DagError::Cancelled(execution_id));
            }
            
            // 3. Execute layer nodes in parallel
            let mut handles = Vec::new();
            for node_id in layer {
                let node = graph.get_node(node_id)?;
                handles.push(self.spawn_node(node, cancellation.clone()));
            }
            
            // 4. Wait for all nodes in layer
            for handle in handles {
                let result = handle.join().map_err(|_| DagError::ExecutionFailed { ... })?;
                match result {
                    Ok(()) => { /* next node */ }
                    Err(e) => { /* retry or fail DAG */ }
                }
            }
        }
        
        // 5. Build DagResult
        Ok(DagResult { ... })
    }
}
```

---

## 4. External Dependency Decision

| Candidate | Use | Decision |
|-----------|-----|----------|
| `petgraph` | Graph data structure | ❌ **Rejected** — too heavy for simple adjacency list |
| `rayon` | Parallel execution | ❌ **Rejected** — adds async overhead, std::thread sufficient |
| `crossbeam` | Thread pool channels | ⚠️ **Optional** — only if Mutex-based pool is too slow |
| `proptest` | Property-based tests | ✅ **Dev-dependency** — in tests only |

**Goal:** Zero new production dependencies beyond what's already in the workspace.

---

## 5. Verification Checklist Per Phase

```
Phase complete if:
- [ ] cargo build --workspace succeeds
- [ ] cargo test -p browseros-dag succeeds
- [ ] cargo clippy -p browseros-dag has 0 warnings
- [ ] cargo fmt --check passes
- [ ] No unwrap()/expect()/panic!() in production code
- [ ] All new public APIs have doc comments
```

## 6. Final Exit Criteria

```
Implementation complete if:
- [ ] All 45+ tests pass (unit + integration + stress)
- [ ] DAG execution works for linear, diamond, parallel, and complex graphs
- [ ] Cycle detection catches all invalid graphs
- [ ] Retry with ExponentialBackoff works correctly
- [ ] Cancellation stops execution mid-DAG
- [ ] DAG events emitted on EventBus for every lifecycle transition
- [ ] Metrics counters record all node state changes
- [ ] Trace spans wrap every node execution
- [ ] RuntimeContext.dag() returns working DagEngine
- [ ] LifecycleManager tracks "dag_engine" component
- [ ] clippy --workspace: 0 warnings
- [ ] All existing tests still pass
- [ ] No new production unwrap()/expect()/panic!()
```
