# DAG Engine API Design — browseros-dag

**Status:** DESIGN  
**Date:** 2026-07-13  

---

## 1. Public API Surface

### 1.1 DagEngine

```rust
/// DAG execution engine for BrowserOS.
///
/// Orchestrates multi-step tasks with dependency ordering,
/// parallel execution, retry, and cancellation.
pub struct DagEngine { /* ... */ }

impl DagEngine {
    /// Create a new DAG engine with observability integration.
    pub fn new(
        event_bus: Arc<EventBus>,
        logger: Arc<Logger>,
        metrics: Arc<MetricsRegistry>,
        tracer: Arc<Tracer>,
    ) -> Self;

    /// Register a new node type in the DAG.
    /// Nodes are identified by their NodeId.
    pub fn register_node(&self, node: DagNode) -> Result<(), DagError>;

    /// Add a dependency edge: `from` must complete before `to` starts.
    /// Returns error if edge creates a cycle.
    pub fn add_edge(&self, from: &NodeId, to: &NodeId) -> Result<(), DagError>;

    /// Remove a dependency edge.
    pub fn remove_edge(&self, from: &NodeId, to: &NodeId) -> Result<(), DagError>;

    /// Execute a registered DAG starting from the given entry nodes.
    /// Blocks until all nodes complete or fail.
    /// Returns a DagResult with per-node state.
    pub fn execute(&self, entry_nodes: &[NodeId]) -> Result<DagResult, DagError>;

    /// Execute a DAG with an explicit set of nodes (creates ephemeral DAG).
    /// Useful for one-shot task execution.
    pub fn execute_dag(&self, dag: DagDefinition) -> Result<DagResult, DagError>;

    /// Cancel a running DAG execution.
    pub fn cancel(&self, execution_id: &ExecutionId) -> Result<(), DagError>;

    /// Get the current state of a DAG execution.
    pub fn execution_state(&self, execution_id: &ExecutionId) -> Option<DagExecutionState>;

    /// Get the state of a specific node within an execution.
    pub fn node_state(&self, execution_id: &ExecutionId, node_id: &NodeId) -> Option<NodeState>;

    /// List all tracked execution IDs.
    pub fn list_executions(&self) -> Vec<ExecutionId>;
}
```

### 1.2 DagNode

```rust
/// A single unit of work in a DAG.
pub struct DagNode {
    pub id: NodeId,
    pub name: String,
    pub kind: NodeKind,
    pub retry_policy: Option<RetryPolicy>,
    pub timeout: Option<Duration>,
    pub max_retries: u32,
}

/// The kind of work a node performs.
pub enum NodeKind {
    /// A synchronous function call.
    Command(Box<dyn Fn() -> Result<(), DagError> + Send + Sync>),
    /// A nested DAG (sub-DAG).
    SubDag(Box<DagEngine>),
}

impl DagNode {
    /// Create a new command node.
    pub fn command(
        id: NodeId,
        name: impl Into<String>,
        f: Box<dyn Fn() -> Result<(), DagError> + Send + Sync>,
    ) -> Self;

    /// Create a new sub-DAG node.
    pub fn sub_dag(
        id: NodeId,
        name: impl Into<String>,
        sub_dag: DagEngine,
    ) -> Self;

    /// Set retry policy for this node.
    pub fn with_retry(mut self, policy: RetryPolicy) -> Self;

    /// Set execution timeout for this node.
    pub fn with_timeout(mut self, timeout: Duration) -> Self;
}
```

### 1.3 DagDefinition

```rust
/// An ephemeral DAG definition for one-shot execution.
pub struct DagDefinition {
    pub nodes: Vec<DagNode>,
    pub edges: Vec<(NodeId, NodeId)>,  // (from, to)
}

impl DagDefinition {
    pub fn new() -> Self;
    pub fn add_node(mut self, node: DagNode) -> Self;
    pub fn add_edge(mut self, from: NodeId, to: NodeId) -> Self;
}
```

### 1.4 DagResult

```rust
/// Result of a DAG execution.
#[derive(Debug, Clone)]
pub struct DagResult {
    pub execution_id: ExecutionId,
    pub state: DagExecutionState,
    pub node_results: HashMap<NodeId, NodeState>,
    pub total_duration: Duration,
    pub node_count: u32,
    pub success_count: u32,
    pub failure_count: u32,
}
```

### 1.5 State Enums

```rust
/// Overall state of a DAG execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DagExecutionState {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// State of a single node within a DAG execution.
#[derive(Debug, Clone, PartialEq, Eq)]
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

## 2. Error Types

```rust
/// Errors that can occur during DAG operations.
#[derive(Debug, Clone)]
pub enum DagError {
    /// Node with the given ID already exists.
    NodeAlreadyExists(NodeId),
    /// Node with the given ID was not found.
    NodeNotFound(NodeId),
    /// Edge creates a cycle in the DAG.
    CycleDetected { from: NodeId, to: NodeId },
    /// Node execution returned an error.
    ExecutionFailed { node_id: NodeId, reason: String },
    /// Node execution timed out.
    ExecutionTimedOut { node_id: NodeId, timeout: Duration },
    /// Retry budget exhausted for a node.
    RetriesExhausted { node_id: NodeId, attempts: u32 },
    /// Cancellation requested during execution.
    Cancelled(ExecutionId),
    /// DAG is already running and cannot be modified.
    DagLocked(ExecutionId),
}

impl std::error::Error for DagError {}
impl std::fmt::Display for DagError { /* ... */ }
```

---

## 3. Event Payloads

### 3.1 DAG Execution Events

```rust
// ——— Emitted when a DAG execution starts ———
pub struct DagExecutionStarted {
    pub metadata: EventMetadata,
    pub execution_id: ExecutionId,
    pub node_count: u32,
    pub entry_nodes: Vec<NodeId>,
}
// kind: "dag.execution_started"

// ——— Emitted when a DAG execution completes successfully ———
pub struct DagExecutionCompleted {
    pub metadata: EventMetadata,
    pub execution_id: ExecutionId,
    pub total_duration: Duration,
    pub success_count: u32,
    pub node_count: u32,
}
// kind: "dag.execution_completed"

// ——— Emitted when a DAG execution fails ———
pub struct DagExecutionFailed {
    pub metadata: EventMetadata,
    pub execution_id: ExecutionId,
    pub failed_node: NodeId,
    pub reason: String,
    pub success_count: u32,
    pub failure_count: u32,
}
// kind: "dag.execution_failed"

// ——— Emitted when a DAG execution is cancelled ———
pub struct DagExecutionCancelled {
    pub metadata: EventMetadata,
    pub execution_id: ExecutionId,
    pub completed_nodes: u32,
}
// kind: "dag.execution_cancelled"
```

### 3.2 Node Lifecycle Events

```rust
// ——— Emitted when a node starts execution ———
pub struct DagNodeStarted {
    pub metadata: EventMetadata,
    pub execution_id: ExecutionId,
    pub node_id: NodeId,
    pub node_name: String,
    pub attempt: u32,
}
// kind: "dag.node.started"

// ——— Emitted when a node completes successfully ———
pub struct DagNodeCompleted {
    pub metadata: EventMetadata,
    pub execution_id: ExecutionId,
    pub node_id: NodeId,
    pub node_name: String,
    pub duration: Duration,
    pub attempt: u32,
}
// kind: "dag.node.completed"

// ——— Emitted when a node fails (retryable) ———
pub struct DagNodeRetrying {
    pub metadata: EventMetadata,
    pub execution_id: ExecutionId,
    pub node_id: NodeId,
    pub node_name: String,
    pub attempt: u32,
    pub max_retries: u32,
    pub next_delay_ms: u64,
    pub error: String,
}
// kind: "dag.node.retrying"

// ——— Emitted when a node fails permanently ———
pub struct DagNodeFailed {
    pub metadata: EventMetadata,
    pub execution_id: ExecutionId,
    pub node_id: NodeId,
    pub node_name: String,
    pub attempt: u32,
    pub error: String,
}
// kind: "dag.node.failed"

// ——— Emitted when a node is skipped (upstream failure) ———
pub struct DagNodeSkipped {
    pub metadata: EventMetadata,
    pub execution_id: ExecutionId,
    pub node_id: NodeId,
    pub node_name: String,
    pub reason: String,
}
// kind: "dag.node.skipped"
```

---

## 4. Internal Module API (pub(crate))

### 4.1 ScheduleResult (Scheduler)

```rust
// Internal — not part of public API
pub(crate) struct ScheduleResult {
    pub layers: Vec<Vec<NodeId>>,       // topological layers
    pub execution_order: Vec<NodeId>,   // flattened execution order
}
```

### 4.2 Graph Validation (Graph)

```rust
// Internal — not part of public API
impl DagGraph {
    pub(crate) fn validate(&self) -> Result<(), DagError>;
    pub(crate) fn has_cycle(&self) -> bool;
    pub(crate) fn topological_sort(&self) -> Result<ScheduleResult, DagError>;
    pub(crate) fn dependencies_of(&self, node: &NodeId) -> Vec<NodeId>;
    pub(crate) fn dependents_of(&self, node: &NodeId) -> Vec<NodeId>;
}
```

---

## 5. Usage Examples

### 5.1 Basic DAG

```rust
let dag = DagEngine::new(bus, logger, metrics, tracer);

let node_a = DagNode::command("node_a", "Fetch config", Box::new(|| {
    println!("fetching config...");
    Ok(())
}));
let node_b = DagNode::command("node_b", "Process data", Box::new(|| {
    println!("processing...");
    Ok(())
}));

dag.register_node(node_a)?;
dag.register_node(node_b)?;
dag.add_edge(&"node_a".parse()?, &"node_b".parse()?)?;

let result = dag.execute(&["node_a".parse()?])?;
assert_eq!(result.state, DagExecutionState::Completed);
```

### 5.2 Retry Policy

```rust
let node = DagNode::command("retry_node", "Unreliable operation", Box::new(|| {
    unreliable_operation()
}))
.with_retry(RetryPolicy::ExponentialBackoff {
    initial_delay_ms: 100,
    max_delay_ms: 5_000,
    multiplier: 2.0,
    max_retries: 3,
    jitter: true,
});
```

### 5.3 One-Shot DAG

```rust
let result = dag.execute_dag(DagDefinition::new()
    .add_node(DagNode::command("a", "Step A", Box::new(|| Ok(()))))
    .add_node(DagNode::command("b", "Step B", Box::new(|| Ok(()))))
    .add_edge("a".parse()?, "b".parse()?)
)?;
```

### 5.4 Sub-DAG

```rust
let inner_dag = DagEngine::new(bus.clone(), logger.clone(), metrics.clone(), tracer.clone());
inner_dag.register_node(DagNode::command("inner1", "Inner step", Box::new(|| Ok(()))))?;

let outer_node = DagNode::sub_dag("outer", "Nested workflow", inner_dag);
dag.register_node(outer_node)?;
```
