# DAG Engine Architecture — browseros-dag

**Status:** DESIGN  
**Date:** 2026-07-13  

---

## 1. Module Layout

```
browseros-dag/
├── Cargo.toml
├── src/
│   ├── lib.rs             # Public API re-exports
│   ├── engine.rs          # DagEngine — execution orchestrator
│   ├── graph.rs           # DagGraph — DAG data structure + cycle detection
│   ├── node.rs            # DagNode, NodeState, NodeRunner
│   ├── error.rs           # DagError enum
│   ├── scheduler.rs       # Node scheduling strategy (topological sort)
│   ├── events.rs          # DAG lifecycle event types
│   └── executor.rs        # SyncExecutor — sequential + parallel execution
```

---

## 2. Component Architecture

```
                     ┌───────────────────────┐
                     │      DagEngine         │
                     │  (orchestrator)        │
                     └───┬─────────┬──────────┘
                         │         │
              ┌──────────▼──┐  ┌───▼──────────┐
              │  DagGraph   │  │  SyncExecutor  │
              │  (DAG data) │  │  (runs nodes) │
              └─────────────┘  └───────┬───────┘
                                       │
                              ┌────────▼────────┐
                              │  EventBus/Logger │
                              │  Metrics/Tracer  │
                              └─────────────────┘
```

### 2.1 DagEngine (Orchestrator)

The main public API. Owns the DagGraph and SyncExecutor. Manages the execution lifecycle.

```rust
pub struct DagEngine {
    graph: DagGraph,
    executor: SyncExecutor,
    event_bus: Arc<EventBus>,
    logger: Arc<Logger>,
    metrics: Arc<MetricsRegistry>,
    tracer: Arc<Tracer>,
}
```

### 2.2 DagGraph (Data Structure)

Owns the adjacency list and node registry. Validates DAG constraints (no cycles, valid edges).

```rust
pub struct DagGraph {
    nodes: HashMap<NodeId, DagNode>,
    edges: HashMap<NodeId, Vec<NodeId>>,  // adjacency list
    reverse_edges: HashMap<NodeId, Vec<NodeId>>,  // for reverse traversal
}
```

### 2.3 SyncExecutor (Execution Engine)

Runs a scheduled execution plan. Manages per-execution state (NodeState), cancellations, and retries.

```rust
pub struct SyncExecutor {
    thread_pool: ThreadPool,
}

struct DagExecution {
    id: ExecutionId,
    state: HashMap<NodeId, NodeState>,
    cancellation: CancellationToken,
    dag_id: DagId,
}
```

### 2.4 DagNode (Node Definition)

A single unit of work in the DAG.

```rust
pub struct DagNode {
    pub id: NodeId,
    pub name: String,
    pub kind: NodeKind,
    pub retry_policy: Option<RetryPolicy>,
    pub timeout: Option<Duration>,
    pub max_retries: u32,
}

pub enum NodeKind {
    Command(Box<dyn Fn() -> Result<(), DagError> + Send + Sync>),
    SubDag(Box<DagEngine>),
}
```

---

## 3. Execution Model

### 3.1 Topological Sort

The Scheduler module computes a topological ordering of nodes. Nodes with no dependencies (in-degree = 0) are eligible for immediate execution.

```
Phase 1: TopologicalSort(dag) → Vec<Vec<NodeId>>
  Layer 0: [A, B]       // no dependencies
  Layer 1: [C]          // depends on A
  Layer 2: [D, E]       // depend on C
  Layer 3: [F]          // depends on D, E
```

### 3.2 Execution Flow

```
execute(dag_id)
  │
  ├── 1. Validate DAG (check cycles, check nodes exist)
  │
  ├── 2. Topological sort → execution layers
  │
  ├── 3. For each layer:
  │     ├── Execute all nodes in parallel (thread pool)
  │     │     ├── Node A: start → running → completed
  │     │     └── Node B: start → running → completed
  │     │
  │     └── Wait for all nodes in layer to complete
  │           └── On failure: retry or fail DAG
  │
  ├── 4. All layers complete → DAG completed
  │
  └── 5. Emit DagCompleted event
```

### 3.3 Retry Model

Per-node retry with configurable policy:

```
Node X execution:
  │
  ├── Attempt 1: Run → FAILED (transient)
  │     └── delay(RetryPolicy::next_delay(0))
  │
  ├── Attempt 2: Run → FAILED (transient)
  │     └── delay(RetryPolicy::next_delay(1))
  │
  ├── Attempt 3: Run → COMPLETED
  │     └── success, continue DAG
  │
  └── Max retries exhausted → permanent failure → DagFailed event
```

### 3.4 Cancellation Model

```
cancel(dag_id)
  │
  ├── Set CancellationToken for the execution
  │
  ├── Currently running nodes:
  │     ├── Checks cancellation.is_cancelled() between retries
  │     └── Completes current attempt (cooperative)
  │
  ├── Pending nodes: → NodeState::Cancelled
  │
  └── Emit DagCancelled event
```

---

## 4. State Model

### 4.1 Execution State (per DAG execution)

```rust
pub enum DagExecutionState {
    Pending,
    Running,
    Completed(DagResult),
    Failed(DagError),
    Cancelled,
}
```

### 4.2 Node State (per node within a DAG execution)

```rust
pub enum NodeState {
    Pending,
    Running,
    Completed(Box<dyn Any + Send>),     // node output
    Failed(DagError),
    Skipped,                              // upstream failure
    Cancelled,
}
```

### 4.3 DAG Result

```rust
pub struct DagResult {
    pub execution_id: ExecutionId,
    pub node_results: HashMap<NodeId, NodeState>,
    pub total_duration: Duration,
    pub node_count: u32,
    pub success_count: u32,
    pub failure_count: u32,
}
```

---

## 5. Event Emission

### 5.1 DAG Lifecycle Events

| Event Kind | Payload | When |
|-----------|---------|------|
| `dag.execution_started` | DagExecutionStarted | DAG execution begins |
| `dag.node.started` | DagNodeStarted | Node starts execution |
| `dag.node.completed` | DagNodeCompleted | Node completes successfully |
| `dag.node.failed` | DagNodeFailed | Node fails (permanent) |
| `dag.node.retrying` | DagNodeRetrying | Node failed, retry scheduled |
| `dag.node.skipped` | DagNodeSkipped | Upstream failure, node skipped |
| `dag.execution.completed` | DagExecutionCompleted | All nodes complete successfully |
| `dag.execution.failed` | DagExecutionFailed | DAG execution fails |
| `dag.execution.cancelled` | DagExecutionCancelled | DAG execution cancelled |

### 5.2 Integration with Existing Event Types

Each event carries:
- `EventMetadata` with `correlation_id` tracing
- `ExecutionId` for DAG execution correlation
- `ModuleId` identifying `browseros-dag` as source

---

## 6. Thread Model

```
                  DagEngine::execute()
                         │
                  ┌──────▼──────┐
                  │  Main Thread │  (caller's thread)
                  │  (scheduler) │
                  └──────┬──────┘
                         │
              ┌──────────▼──────────┐
              │   Thread Pool       │
              │  (configurable N)   │
              │                     │
              │  Thread 1: Node A   │
              │  Thread 2: Node B   │  (parallel execution)
              │  Thread 3: Node C   │
              └─────────────────────┘
```

- Main thread: topological sort, layer coordination, event emission
- Worker threads: node execution, retry timing
- Each worker checks CancellationToken before and after execution
- Thread pool size: configurable (default = `std::thread::available_parallelism()`)

---

## 7. Integration with Existing Components

### 7.1 RuntimeContext Integration

```rust
pub struct RuntimeContext {
    // Existing fields (7)
    bus: Arc<EventBus>,
    logger: Arc<Logger>,
    metrics: Arc<MetricsRegistry>,
    tracer: Arc<Tracer>,
    config: Arc<RootConfig>,
    lifecycle: Arc<LifecycleManager>,
    scheduler: Arc<Scheduler>,
    
    // NEW field
    dag: Arc<DagEngine>,              // DAG execution engine
}
```

### 7.2 Construction in RuntimeBuilder

```rust
impl RuntimeBuilder {
    pub fn with_dag_engine(mut self) -> Self {
        let dag = Arc::new(DagEngine::new(
            bus.clone(),
            logger.clone(),
            metrics.clone(),
            tracer.clone(),
        ));
        self.dag = Some(dag);
        self
    }
}
```

### 7.3 Lifecycle Registration

```rust
// In RuntimeContext::init():
lifecycle.register_component("dag_engine");
lifecycle.transition_to("dag_engine", LifecycleState::Created);
```
