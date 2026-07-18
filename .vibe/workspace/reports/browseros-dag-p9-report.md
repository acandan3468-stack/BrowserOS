# P9 Report — Bounded Parallel Execution

## Deliverables

### 1. `NodeKind` Arc Conversion (`node.rs`)
- Changed `NodeKind::Command(Box<dyn Fn ...>)` → `NodeKind::Command(Arc<dyn Fn ...>)`
- Changed `NodeKind::SubDag(Box<DagEngine>)` → `NodeKind::SubDag(Arc<DagEngine>)`
- Added `#[derive(Clone)]` to `NodeKind` and `DagNode`
- Updated `command()` and `sub_dag()` constructors accordingly
- **Backward compatible**: existing call sites unchanged (Deref via Arc works identically to Box)

### 2. `SyncExecutor` Parallel Execution (`executor.rs`)
- Added `max_threads: usize` field to `SyncExecutor` (clamped to ≥ 1)
- Changed `SyncExecutor::new()` → `SyncExecutor::new(max_threads: usize)`
- Extracted `execute_node_inner()` as a free function — reusable from both sequential and parallel paths without borrowing issues
- Added `process_layer()` dispatcher — selects sequential vs parallel per layer based on size and `max_threads`
- **Sequential path** (`process_layer_sequential`): preserves exact existing behavior (break on first failure, mark_dependents_skipped)
- **Parallel path** (`process_layer_parallel`): uses `std::thread::scope` + CAS semaphore (`AtomicUsize` with `compare_exchange_weak`) for bounded concurrency
  - Each node thread acquires semaphore before executing, releases after completion
  - Panic safety via `catch_unwind`
  - Results collected in shared `Arc<Mutex<Vec<NodeResult>>>`
  - Cancellation checked per-thread before execution
  - Failed nodes propagate to dependents via `mark_dependents_skipped()`
- **Zero unwrap/expect/panic/unsafe** in production code

### 3. `DagEngine` Configuration (`engine.rs`)
- Added `with_max_threads(event_bus, logger, metrics, tracer, max_threads)` constructor
- `new()` delegates to `with_max_threads(..., 4)` — default pool size of 4

### 4. P9 Tests (15 new, all passing)

| Test | Coverage |
|------|----------|
| `parallel_executes_independent_nodes_in_same_layer` | Two roots execute concurrently |
| `parallel_maintains_dependency_ordering` | Sequential layers still ordered |
| `parallel_respects_max_threads` | max_threads=1 falls through to sequential |
| `parallel_diamond_dag` | Diamond shape, B and C in same layer |
| `parallel_disconnected_subgraphs` | Two independent chains |
| `parallel_with_retry_succeeds` | Retry in parallel mode |
| `parallel_with_retry_exhaustion` | Retry exhaustion in parallel mode |
| `parallel_with_timeout_exceeded` | Timeout in parallel mode |
| `parallel_failure_propagates_to_dependents` | Failed node → dependents skipped |
| `parallel_sub_dag_node` | SubDag node in parallel engine |
| `parallel_large_linear_chain` | 50-node chain (all single-node layers) |
| `parallel_stress_many_independent_nodes` | 20 roots, pool=4 |
| `parallel_stress_no_duplicate_executions` | Atomic counter — each node runs exactly once |
| `parallel_pool_smaller_than_layer` | 8 nodes, pool=2 |
| `parallel_metrics_and_events` | Metrics counters work in parallel |

## Verification

```
cargo build -p browseros-dag   → OK
cargo clippy -p browseros-dag  → 0 warnings
cargo fmt --all --check        → clean
cargo test -p browseros-dag    → 167 passed, 0 failed
```

## Invariant Audit

| Invariant | Status |
|-----------|--------|
| DAG-INV-001 (no cycles) | Unchanged — graph.rs not modified |
| DAG-INV-007 (parallel layers) | ✅ Implemented: independent nodes in same layer execute concurrently via bounded thread pool |
| DAG-INV-012 (Scheduler delay) | Deferred — handled by std::thread::sleep in retry path (consistent with P7 design decision) |
| DAG-INV-014 (Timeout enforcement) | ✅ Cumulative timeout enforced in `execute_node_inner` (same as P8) |
| DAG-INV-028 (bounded pool) | ✅ configurable via `with_max_threads()`, default 4, CAS semaphore, std::thread::scope |
| No tokio/rayon/crossbeam/async | ✅ All std-based |
| No unwrap/expect/panic/unsafe | ✅ Zero in production code |
| No global mutable state | ✅ No statics |
| Thread safety | ✅ `std::thread::scope` guarantees join; CAS semaphore prevents race; panic safety via `catch_unwind` |

## Files Modified

| File | Changes |
|------|---------|
| `browseros/browseros-dag/src/node.rs` | Arc in NodeKind, Clone derives, constructor updates |
| `browseros/browseros-dag/src/executor.rs` | max_threads, process_layer dispatcher, parallel path, execute_node_inner, 15 tests |
| `browseros/browseros-dag/src/engine.rs` | with_max_threads() constructor, default pool=4 |

## Test Counts (Total: 167)

- graph.rs: 39 → 39 (unchanged)
- node.rs: 39 → 39 (unchanged)
- scheduler.rs: 12 → 12 (unchanged)
- executor.rs: 18 → 33 (+15 P9 tests)
- integration: 19 → 19 (unchanged)
- types: 24 → 24 (unchanged)

## API Compatibility

All public APIs preserved:
- `DagEngine::new()` — unchanged (default max_threads=4)
- `DagEngine::with_max_threads()` — new
- `DagNode::command()` — unchanged signature
- `DagNode::sub_dag()` — unchanged signature
- `DagNode` — now Clone (backward-compatible enhancement)
- `NodeKind` — now Clone (backward-compatible enhancement)
