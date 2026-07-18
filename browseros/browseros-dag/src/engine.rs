use std::collections::HashMap;
use std::sync::{Mutex, RwLock};

use browseros_event_bus::EventBus;
use browseros_observability::logger::Logger;
use browseros_observability::metrics::MetricsRegistry;
use browseros_observability::tracer::Tracer;
use browseros_types::clock::CancellationToken;
use browseros_types::identifiers::{ExecutionId, NodeId};

use crate::error::DagError;
use crate::executor::SyncExecutor;
use crate::graph::DagGraph;
use crate::node::{DagDefinition, DagExecutionState, DagNode, DagResult, NodeState};

struct ExecutionInfo {
    state: DagExecutionState,
    node_states: HashMap<NodeId, NodeState>,
    cancellation_token: CancellationToken,
}

pub struct DagEngine {
    graph: RwLock<DagGraph>,
    executor: SyncExecutor,
    event_bus: EventBus,
    logger: Logger,
    metrics: MetricsRegistry,
    tracer: Tracer,
    executions: Mutex<HashMap<ExecutionId, ExecutionInfo>>,
}

impl std::fmt::Debug for DagEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DagEngine").finish()
    }
}

impl DagEngine {
    pub fn new(
        event_bus: EventBus,
        logger: Logger,
        metrics: MetricsRegistry,
        tracer: Tracer,
    ) -> Self {
        Self::with_max_threads(event_bus, logger, metrics, tracer, 4)
    }

    pub fn with_max_threads(
        event_bus: EventBus,
        logger: Logger,
        metrics: MetricsRegistry,
        tracer: Tracer,
        max_threads: usize,
    ) -> Self {
        Self {
            graph: RwLock::new(DagGraph::new()),
            executor: SyncExecutor::new(max_threads),
            event_bus,
            logger,
            metrics,
            tracer,
            executions: Mutex::new(HashMap::new()),
        }
    }

    pub fn register_node(&self, node: DagNode) -> Result<(), DagError> {
        let mut graph = self
            .graph
            .write()
            .map_err(|_| DagError::DagLocked(ExecutionId::new()))?;
        graph.register_node(node)
    }

    pub fn add_edge(&self, from: &NodeId, to: &NodeId) -> Result<(), DagError> {
        let mut graph = self
            .graph
            .write()
            .map_err(|_| DagError::DagLocked(ExecutionId::new()))?;
        graph.add_edge(from, to)
    }

    pub fn remove_edge(&self, from: &NodeId, to: &NodeId) -> Result<(), DagError> {
        let mut graph = self
            .graph
            .write()
            .map_err(|_| DagError::DagLocked(ExecutionId::new()))?;
        graph.remove_edge(from, to)
    }

    pub fn execute(&self, entry_nodes: &[NodeId]) -> Result<DagResult, DagError> {
        let execution_id = ExecutionId::new();
        let cancellation_token = CancellationToken::new();

        let schedule = {
            let graph = self
                .graph
                .read()
                .map_err(|_| DagError::DagLocked(execution_id))?;
            graph.topological_sort()?
        };

        let mut node_states: HashMap<NodeId, NodeState> = HashMap::new();
        for layer in &schedule {
            for node_id in layer {
                node_states.insert(node_id.clone(), NodeState::Pending);
            }
        }

        {
            let mut execs = self
                .executions
                .lock()
                .map_err(|_| DagError::DagLocked(execution_id))?;
            execs.insert(
                execution_id,
                ExecutionInfo {
                    state: DagExecutionState::Pending,
                    node_states: node_states.clone(),
                    cancellation_token: cancellation_token.clone(),
                },
            );
        }

        let result = self.executor.execute(
            &self.graph,
            &schedule,
            entry_nodes,
            &self.event_bus,
            &self.logger,
            &self.metrics,
            &self.tracer,
            &cancellation_token,
            execution_id,
            &node_states,
        );

        match result {
            Ok(dag_result) => {
                let mut execs = self
                    .executions
                    .lock()
                    .map_err(|_| DagError::DagLocked(execution_id))?;
                if let Some(info) = execs.get_mut(&execution_id) {
                    info.state = dag_result.state;
                    info.node_states = dag_result.node_results.clone();
                }
                Ok(dag_result)
            }
            Err(e) => {
                let mut execs = self
                    .executions
                    .lock()
                    .map_err(|_| DagError::DagLocked(execution_id))?;
                if let Some(info) = execs.get_mut(&execution_id) {
                    info.state = DagExecutionState::Failed;
                    info.node_states = node_states;
                }
                Err(e)
            }
        }
    }

    pub fn execute_dag(&self, dag: DagDefinition) -> Result<DagResult, DagError> {
        for node in dag.nodes {
            let mut graph = self
                .graph
                .write()
                .map_err(|_| DagError::DagLocked(ExecutionId::new()))?;
            graph.register_node(node)?;
        }
        for (from, to) in dag.edges {
            let mut graph = self
                .graph
                .write()
                .map_err(|_| DagError::DagLocked(ExecutionId::new()))?;
            graph.add_edge(&from, &to)?;
        }
        let roots = {
            let graph = self
                .graph
                .read()
                .map_err(|_| DagError::DagLocked(ExecutionId::new()))?;
            graph.find_roots()
        };
        self.execute(&roots)
    }

    pub fn cancel(&self, execution_id: &ExecutionId) -> Result<(), DagError> {
        let mut execs = self
            .executions
            .lock()
            .map_err(|_| DagError::DagLocked(*execution_id))?;
        if let Some(info) = execs.get_mut(execution_id) {
            info.cancellation_token.cancel();
            info.state = DagExecutionState::Cancelled;
            for state in info.node_states.values_mut() {
                if *state == NodeState::Pending {
                    *state = NodeState::Cancelled;
                }
            }
        }
        Ok(())
    }

    pub fn execution_state(&self, execution_id: &ExecutionId) -> Option<DagExecutionState> {
        let execs = self.executions.lock().ok()?;
        execs.get(execution_id).map(|info| info.state)
    }

    pub fn node_state(&self, execution_id: &ExecutionId, node_id: &NodeId) -> Option<NodeState> {
        let execs = self.executions.lock().ok()?;
        execs
            .get(execution_id)
            .and_then(|info| info.node_states.get(node_id).cloned())
    }

    pub fn list_executions(&self) -> Vec<ExecutionId> {
        self.executions
            .lock()
            .map(|execs| execs.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Returns all root nodes (in-degree 0) from the graph.
    /// Used by the executor for sub-DAG execution.
    pub(crate) fn root_nodes(&self) -> Result<Vec<NodeId>, DagError> {
        let graph = self
            .graph
            .read()
            .map_err(|_| DagError::DagLocked(ExecutionId::new()))?;
        Ok(graph.find_roots())
    }

    pub fn event_bus(&self) -> &EventBus {
        &self.event_bus
    }

    pub fn logger(&self) -> &Logger {
        &self.logger
    }

    pub fn metrics(&self) -> &MetricsRegistry {
        &self.metrics
    }

    pub fn tracer(&self) -> &Tracer {
        &self.tracer
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::Arc;

    use browseros_event_bus::EventBus;
    use browseros_observability::export::StdoutSink;
    use browseros_observability::logger::{FilterDecision, LogFilter, LogRecord, Logger};
    use browseros_observability::metrics::MetricsRegistry;
    use browseros_observability::tracer::Tracer;

    struct NoopFilter;

    impl LogFilter for NoopFilter {
        fn should_log(&self, _record: &LogRecord) -> FilterDecision {
            FilterDecision::Reject
        }
    }

    pub fn noop_event_bus() -> EventBus {
        EventBus::new()
    }

    pub fn noop_logger() -> Logger {
        Logger::new(Arc::new(StdoutSink::new()), Arc::new(NoopFilter), "test")
    }

    pub fn noop_metrics() -> MetricsRegistry {
        MetricsRegistry::new()
    }

    pub fn noop_tracer() -> Tracer {
        Tracer::new()
    }
}
