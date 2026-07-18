use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};

use browseros_event_bus::EventBus;
use browseros_observability::logger::Logger;
use browseros_observability::metrics::MetricsRegistry;
use browseros_observability::tracer::Tracer;
use browseros_types::clock::CancellationToken;
use browseros_types::error::RetryPolicy;
use browseros_types::identifiers::{CorrelationId, ExecutionId, NodeId};

use crate::error::DagError;
use crate::events::{
    DagExecutionCancelled, DagExecutionCompleted, DagExecutionStarted, DagNodeCompleted,
    DagNodeFailed, DagNodeRetrying, DagNodeSkipped, DagNodeStarted,
};
use crate::graph::DagGraph;
use crate::node::{DagExecutionState, DagResult, NodeKind, NodeState};

type NodeResult = (NodeId, Result<(), DagError>);

pub(crate) struct SyncExecutor {
    max_threads: usize,
}

impl SyncExecutor {
    pub fn new(max_threads: usize) -> Self {
        Self {
            max_threads: max_threads.max(1),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn execute(
        &self,
        graph: &RwLock<DagGraph>,
        schedule: &[Vec<NodeId>],
        _entry_nodes: &[NodeId],
        event_bus: &EventBus,
        logger: &Logger,
        metrics: &MetricsRegistry,
        tracer: &Tracer,
        cancellation_token: &CancellationToken,
        execution_id: ExecutionId,
        initial_states: &HashMap<NodeId, NodeState>,
    ) -> Result<DagResult, DagError> {
        let start = std::time::Instant::now();
        let _span = tracer.start_span("dag.execute");
        let log = logger.child("dag-executor");

        let correlation_id = CorrelationId::new();
        let node_count = schedule.iter().map(|l| l.len() as u32).sum::<u32>();

        metrics.counter("dag.executions.total").increment();

        let started_event =
            DagExecutionStarted::new(execution_id, correlation_id, node_count, Vec::new());
        event_bus.publish(Box::new(started_event));
        log.info(format!("execution started: {execution_id}"));

        let mut node_states = initial_states.clone();

        for layer in schedule {
            if cancellation_token.is_cancelled() {
                return self.finalise_cancelled(
                    &mut node_states,
                    schedule,
                    execution_id,
                    correlation_id,
                    event_bus,
                    &log,
                    metrics,
                    start,
                );
            }

            self.process_layer(
                layer,
                graph,
                &mut node_states,
                execution_id,
                correlation_id,
                event_bus,
                &log,
                metrics,
                tracer,
                cancellation_token,
                schedule,
            )?;
        }

        let total_duration = start.elapsed();
        let success_count = node_states
            .values()
            .filter(|s| **s == NodeState::Completed)
            .count() as u32;
        let failure_count = node_states
            .values()
            .filter(|s| matches!(s, NodeState::Failed(_)))
            .count() as u32;

        let state = if cancellation_token.is_cancelled() {
            DagExecutionState::Cancelled
        } else if failure_count > 0 {
            DagExecutionState::Failed
        } else {
            DagExecutionState::Completed
        };

        metrics.counter("dag.executions.completed").increment();

        let completed_event = DagExecutionCompleted::new(
            execution_id,
            correlation_id,
            total_duration,
            success_count,
            node_count,
        );
        event_bus.publish(Box::new(completed_event));
        log.info(format!(
            "execution {execution_id} completed: {state:?} in {total_duration:?}"
        ));

        Ok(DagResult {
            execution_id,
            state,
            node_results: node_states,
            total_duration,
            node_count,
            success_count,
            failure_count,
        })
    }

    /// Process a single layer of the schedule.
    /// Dispatches to sequential or parallel execution based on layer size
    /// and max_threads configuration.
    #[allow(clippy::too_many_arguments)]
    fn process_layer(
        &self,
        layer: &[NodeId],
        graph: &RwLock<DagGraph>,
        node_states: &mut HashMap<NodeId, NodeState>,
        execution_id: ExecutionId,
        correlation_id: CorrelationId,
        event_bus: &EventBus,
        log: &Logger,
        metrics: &MetricsRegistry,
        tracer: &Tracer,
        cancellation_token: &CancellationToken,
        schedule: &[Vec<NodeId>],
    ) -> Result<(), DagError> {
        if layer.len() <= 1 || self.max_threads <= 1 {
            self.process_layer_sequential(
                layer,
                graph,
                node_states,
                execution_id,
                correlation_id,
                event_bus,
                log,
                metrics,
                tracer,
                cancellation_token,
                schedule,
            )
        } else {
            self.process_layer_parallel(
                layer,
                graph,
                node_states,
                execution_id,
                correlation_id,
                event_bus,
                log,
                metrics,
                tracer,
                cancellation_token,
                schedule,
            )
        }
    }

    /// Sequential per-node execution within a layer.
    /// Maintains exact existing behavior (break on first failure).
    #[allow(clippy::too_many_arguments)]
    fn process_layer_sequential(
        &self,
        layer: &[NodeId],
        graph: &RwLock<DagGraph>,
        node_states: &mut HashMap<NodeId, NodeState>,
        execution_id: ExecutionId,
        correlation_id: CorrelationId,
        event_bus: &EventBus,
        log: &Logger,
        metrics: &MetricsRegistry,
        tracer: &Tracer,
        cancellation_token: &CancellationToken,
        schedule: &[Vec<NodeId>],
    ) -> Result<(), DagError> {
        for node_id in layer {
            if cancellation_token.is_cancelled() {
                return Err(DagError::Cancelled(execution_id));
            }

            let deps: Vec<NodeId> = {
                let g = graph
                    .read()
                    .map_err(|_| DagError::DagLocked(execution_id))?;
                g.dependencies_of(node_id)
            };

            let all_deps_completed = deps
                .iter()
                .all(|dep| node_states.get(dep) == Some(&NodeState::Completed));

            if !all_deps_completed {
                node_states.insert(node_id.clone(), NodeState::Skipped);
                let node_name = self
                    .node_name(graph, node_id, execution_id)
                    .unwrap_or_else(|_| node_id.to_string());
                let skipped = DagNodeSkipped::new(
                    execution_id,
                    correlation_id,
                    node_id.clone(),
                    node_name,
                    "dependency_failed".into(),
                );
                event_bus.publish(Box::new(skipped));
                log.info(format!("node {node_id} skipped (dependency failed)"));
                continue;
            }

            let result = {
                let g = graph
                    .read()
                    .map_err(|_| DagError::DagLocked(execution_id))?;
                let node = g.get_node(node_id)?;
                execute_node_inner(
                    &node.id,
                    &node.name,
                    &node.kind,
                    node.retry_policy.as_ref(),
                    node.timeout,
                    execution_id,
                    correlation_id,
                    event_bus,
                    log,
                    metrics,
                    tracer,
                    cancellation_token,
                )
            };

            match result {
                Ok(()) => {
                    node_states.insert(node_id.clone(), NodeState::Completed);
                }
                Err(DagError::Cancelled(_)) => {
                    return Err(DagError::Cancelled(execution_id));
                }
                Err(_) => {
                    node_states.insert(
                        node_id.clone(),
                        NodeState::Failed("execution_failed".into()),
                    );
                    self.mark_dependents_skipped(
                        graph,
                        node_id,
                        node_states,
                        schedule,
                        execution_id,
                        correlation_id,
                        event_bus,
                        log,
                    )?;
                    return Ok(());
                }
            }
        }
        Ok(())
    }

    /// Parallel per-node execution within a layer.
    /// All nodes in the layer execute concurrently, bounded by max_threads.
    #[allow(clippy::too_many_arguments)]
    fn process_layer_parallel(
        &self,
        layer: &[NodeId],
        graph: &RwLock<DagGraph>,
        node_states: &mut HashMap<NodeId, NodeState>,
        execution_id: ExecutionId,
        correlation_id: CorrelationId,
        event_bus: &EventBus,
        log: &Logger,
        metrics: &MetricsRegistry,
        tracer: &Tracer,
        cancellation_token: &CancellationToken,
        schedule: &[Vec<NodeId>],
    ) -> Result<(), DagError> {
        let n = layer.len();
        let semaphore = AtomicUsize::new(self.max_threads.min(n));
        let results_collector: Arc<std::sync::Mutex<Vec<NodeResult>>> =
            Arc::new(std::sync::Mutex::new(Vec::with_capacity(n)));

        std::thread::scope(|scope| {
            for node_id in layer {
                Self::acquire_semaphore(&semaphore);

                let nid = node_id.clone();
                let results = results_collector.clone();
                let sem = &semaphore;
                let cancel = cancellation_token.clone();

                scope.spawn(move || {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        if cancel.is_cancelled() {
                            return Err(DagError::Cancelled(execution_id));
                        }

                        let g = graph
                            .read()
                            .map_err(|_| DagError::DagLocked(execution_id))?;
                        let node = g.get_node(&nid)?;
                        execute_node_inner(
                            &node.id,
                            &node.name,
                            &node.kind,
                            node.retry_policy.as_ref(),
                            node.timeout,
                            execution_id,
                            correlation_id,
                            event_bus,
                            log,
                            metrics,
                            tracer,
                            &cancel,
                        )
                    }));

                    let node_result = match result {
                        Ok(r) => r,
                        Err(_) => Err(DagError::ExecutionFailed {
                            node_id: nid.clone(),
                            reason: "node panicked".into(),
                        }),
                    };

                    results.lock().unwrap().push((nid, node_result));
                    Self::release_semaphore(sem);
                });
            }
        });

        let collected = results_collector.lock().unwrap();
        let mut failed_nodes: Vec<NodeId> = Vec::new();
        let mut cancelled = false;

        for (node_id, result) in collected.iter() {
            match result {
                Ok(()) => {
                    node_states.insert(node_id.clone(), NodeState::Completed);
                }
                Err(DagError::Cancelled(_)) => {
                    cancelled = true;
                }
                Err(_) => {
                    node_states.insert(
                        node_id.clone(),
                        NodeState::Failed("execution_failed".into()),
                    );
                    failed_nodes.push(node_id.clone());
                }
            }
        }

        if cancelled {
            return Err(DagError::Cancelled(execution_id));
        }

        for failed_id in &failed_nodes {
            self.mark_dependents_skipped(
                graph,
                failed_id,
                node_states,
                schedule,
                execution_id,
                correlation_id,
                event_bus,
                log,
            )?;
        }

        Ok(())
    }

    /// CAS-based semaphore acquire (spin + yield to avoid busy-wait).
    fn acquire_semaphore(sem: &AtomicUsize) {
        loop {
            let current = sem.load(Ordering::Acquire);
            if current == 0 {
                std::thread::yield_now();
                continue;
            }
            if sem
                .compare_exchange_weak(current, current - 1, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return;
            }
        }
    }

    fn release_semaphore(sem: &AtomicUsize) {
        sem.fetch_add(1, Ordering::Release);
    }

    #[allow(clippy::too_many_arguments)]
    fn mark_dependents_skipped(
        &self,
        graph: &RwLock<DagGraph>,
        failed_node: &NodeId,
        node_states: &mut HashMap<NodeId, NodeState>,
        schedule: &[Vec<NodeId>],
        execution_id: ExecutionId,
        correlation_id: CorrelationId,
        event_bus: &EventBus,
        log: &Logger,
    ) -> Result<(), DagError> {
        let successors: HashSet<NodeId> = {
            let g = graph
                .read()
                .map_err(|_| DagError::DagLocked(execution_id))?;
            let mut visited = HashSet::new();
            let mut stack = vec![failed_node.clone()];
            while let Some(current) = stack.pop() {
                for dep in g.dependents_of(&current) {
                    if visited.insert(dep.clone()) {
                        stack.push(dep);
                    }
                }
            }
            visited
        };

        for layer in schedule {
            for node_id in layer {
                if successors.contains(node_id)
                    && node_states.get(node_id) == Some(&NodeState::Pending)
                {
                    node_states.insert(node_id.clone(), NodeState::Skipped);
                    let node_name = self
                        .node_name(graph, node_id, execution_id)
                        .unwrap_or_else(|_| node_id.to_string());
                    let skipped = DagNodeSkipped::new(
                        execution_id,
                        correlation_id,
                        node_id.clone(),
                        node_name,
                        "upstream_failed".into(),
                    );
                    event_bus.publish(Box::new(skipped));
                    log.debug(format!("node {node_id} skipped (upstream failed)"));
                }
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn finalise_cancelled(
        &self,
        node_states: &mut HashMap<NodeId, NodeState>,
        schedule: &[Vec<NodeId>],
        execution_id: ExecutionId,
        correlation_id: CorrelationId,
        event_bus: &EventBus,
        log: &Logger,
        metrics: &MetricsRegistry,
        start: std::time::Instant,
    ) -> Result<DagResult, DagError> {
        for layer in schedule {
            for node_id in layer {
                if node_states.get(node_id) == Some(&NodeState::Pending) {
                    node_states.insert(node_id.clone(), NodeState::Cancelled);
                }
            }
        }

        let total_duration = start.elapsed();
        let completed_count = node_states
            .values()
            .filter(|s| **s == NodeState::Completed)
            .count() as u32;

        metrics.counter("dag.executions.cancelled").increment();

        let cancelled_event =
            DagExecutionCancelled::new(execution_id, correlation_id, completed_count);
        event_bus.publish(Box::new(cancelled_event));
        log.info(format!("execution {execution_id} cancelled"));

        Ok(DagResult {
            execution_id,
            state: DagExecutionState::Cancelled,
            node_results: node_states.clone(),
            total_duration,
            node_count: 0,
            success_count: completed_count,
            failure_count: 0,
        })
    }

    fn node_name(
        &self,
        graph: &RwLock<DagGraph>,
        node_id: &NodeId,
        execution_id: ExecutionId,
    ) -> Result<String, DagError> {
        let g = graph
            .read()
            .map_err(|_| DagError::DagLocked(execution_id))?;
        g.get_node(node_id).map(|n| n.name.clone())
    }
}

/// Execute a single node with retry and timeout logic.
/// Extracted as a free function so it can be called from both sequential
/// and parallel paths without borrowing issues.
#[allow(clippy::too_many_arguments)]
fn execute_node_inner(
    node_id: &NodeId,
    node_name: &str,
    kind: &NodeKind,
    retry_policy: Option<&RetryPolicy>,
    timeout: Option<std::time::Duration>,
    execution_id: ExecutionId,
    correlation_id: CorrelationId,
    event_bus: &EventBus,
    log: &Logger,
    metrics: &MetricsRegistry,
    tracer: &Tracer,
    cancellation_token: &CancellationToken,
) -> Result<(), DagError> {
    let _span = tracer.start_span("dag.node.execute");

    let max_attempts = retry_policy
        .map(|p| match p {
            RetryPolicy::Immediate { max_retries } => *max_retries + 1,
            RetryPolicy::ExponentialBackoff { max_retries, .. } => *max_retries + 1,
        })
        .unwrap_or(1u32);

    let mut attempt: u32 = 0;
    let cumulative_start = std::time::Instant::now();

    loop {
        if let Some(t) = timeout {
            let elapsed = cumulative_start.elapsed();
            if elapsed >= t {
                metrics.counter("dag.nodes.failed").increment();
                let timed_out = DagNodeFailed::new(
                    execution_id,
                    correlation_id,
                    node_id.clone(),
                    node_name.to_owned(),
                    attempt + 1,
                    format!("cumulative timeout exceeded after {elapsed:?}"),
                );
                event_bus.publish(Box::new(timed_out));
                log.error(format!(
                    "node {} timed out after {elapsed:?} (timeout={t:?})",
                    node_name
                ));
                return Err(DagError::ExecutionTimedOut {
                    node_id: node_id.clone(),
                    timeout: t,
                });
            }
        }

        if cancellation_token.is_cancelled() {
            return Err(DagError::Cancelled(execution_id));
        }

        metrics.counter("dag.nodes.started").increment();

        let started = DagNodeStarted::new(
            execution_id,
            correlation_id,
            node_id.clone(),
            node_name.to_owned(),
            attempt + 1,
        );
        event_bus.publish(Box::new(started));
        log.debug(format!(
            "node {} attempt {} starting",
            node_name,
            attempt + 1
        ));

        let node_start = std::time::Instant::now();

        let exec_result = match kind {
            NodeKind::Command(f) => f(),
            NodeKind::SubDag(engine) => match engine.execute(&[]) {
                Ok(sub_result) if sub_result.state == DagExecutionState::Completed => Ok(()),
                Ok(sub_result) => Err(DagError::ExecutionFailed {
                    node_id: node_id.clone(),
                    reason: format!("sub-dag state: {:?}", sub_result.state),
                }),
                Err(e) => Err(DagError::ExecutionFailed {
                    node_id: node_id.clone(),
                    reason: e.to_string(),
                }),
            },
        };

        let elapsed = node_start.elapsed();

        match exec_result {
            Ok(()) => {
                metrics.counter("dag.nodes.completed").increment();
                metrics
                    .histogram("dag.node.duration")
                    .observe(elapsed.as_secs_f64() * 1000.0);

                let completed = DagNodeCompleted::new(
                    execution_id,
                    correlation_id,
                    node_id.clone(),
                    node_name.to_owned(),
                    elapsed,
                    attempt + 1,
                );
                event_bus.publish(Box::new(completed));
                log.info(format!(
                    "node {} completed in {elapsed:?} (attempt {})",
                    node_name,
                    attempt + 1
                ));

                return Ok(());
            }
            Err(ref dag_err) => {
                if !dag_err.is_retryable() {
                    metrics.counter("dag.nodes.failed").increment();
                    let failed = DagNodeFailed::new(
                        execution_id,
                        correlation_id,
                        node_id.clone(),
                        node_name.to_owned(),
                        attempt + 1,
                        dag_err.to_string(),
                    );
                    event_bus.publish(Box::new(failed));
                    log.error(format!(
                        "node {} failed (non-retryable): {dag_err}",
                        node_name
                    ));
                    return Err(dag_err.clone());
                }

                let next_attempt = attempt + 1;
                if next_attempt >= max_attempts {
                    metrics.counter("dag.nodes.failed").increment();
                    let exhausted_msg = format!("retries exhausted after {max_attempts} attempts");
                    let failed = DagNodeFailed::new(
                        execution_id,
                        correlation_id,
                        node_id.clone(),
                        node_name.to_owned(),
                        max_attempts,
                        exhausted_msg.clone(),
                    );
                    event_bus.publish(Box::new(failed));
                    log.error(format!(
                        "node {} retries exhausted after {max_attempts} attempts",
                        node_name
                    ));
                    return Err(DagError::RetriesExhausted {
                        node_id: node_id.clone(),
                        attempts: max_attempts,
                    });
                }

                let delay = retry_policy.and_then(|p| p.next_delay(attempt));

                match delay {
                    Some(d) => {
                        let retrying = DagNodeRetrying::new(
                            execution_id,
                            correlation_id,
                            node_id.clone(),
                            node_name.to_owned(),
                            attempt + 1,
                            max_attempts - 1,
                            d.as_millis() as u64,
                            dag_err.to_string(),
                        );
                        event_bus.publish(Box::new(retrying));
                        log.warn(format!(
                            "node {} retrying after {:?} (attempt {}/{})",
                            node_name,
                            d,
                            attempt + 1,
                            max_attempts
                        ));

                        if !d.is_zero() {
                            std::thread::sleep(d);
                        }
                        attempt = next_attempt;
                    }
                    None => {
                        metrics.counter("dag.nodes.failed").increment();
                        let failed = DagNodeFailed::new(
                            execution_id,
                            correlation_id,
                            node_id.clone(),
                            node_name.to_owned(),
                            attempt + 1,
                            dag_err.to_string(),
                        );
                        event_bus.publish(Box::new(failed));
                        log.error(format!(
                            "node {} failed after {}/{} attempts",
                            node_name,
                            attempt + 1,
                            max_attempts
                        ));
                        return Err(DagError::RetriesExhausted {
                            node_id: node_id.clone(),
                            attempts: attempt + 1,
                        });
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use browseros_event_bus::EventBus;
    use browseros_observability::export::StdoutSink;
    use browseros_observability::logger::{FilterDecision, LogFilter, LogRecord, Logger};
    use browseros_observability::metrics::MetricsRegistry;
    use browseros_observability::tracer::Tracer;
    use browseros_types::error::RetryPolicy;
    use browseros_types::identifiers::{ExecutionId, NodeId};

    use crate::engine::DagEngine;
    use crate::error::DagError;
    use crate::node::{DagExecutionState, DagNode, NodeState};

    struct NoopFilter;

    impl LogFilter for NoopFilter {
        fn should_log(&self, _record: &LogRecord) -> FilterDecision {
            FilterDecision::Reject
        }
    }

    fn noop_event_bus() -> EventBus {
        EventBus::new()
    }

    fn noop_logger() -> Logger {
        Logger::new(Arc::new(StdoutSink::new()), Arc::new(NoopFilter), "test")
    }

    fn noop_metrics() -> MetricsRegistry {
        MetricsRegistry::new()
    }

    fn noop_tracer() -> Tracer {
        Tracer::new()
    }

    fn create_engine() -> DagEngine {
        DagEngine::new(
            noop_event_bus(),
            noop_logger(),
            noop_metrics(),
            noop_tracer(),
        )
    }

    fn n(id: &str) -> NodeId {
        NodeId::from_string(id)
    }

    fn ok_fn() -> Box<dyn Fn() -> Result<(), DagError> + Send + Sync> {
        Box::new(|| Ok(()))
    }

    fn fail_fn(msg: &str) -> Box<dyn Fn() -> Result<(), DagError> + Send + Sync> {
        let m = msg.to_owned();
        Box::new(move || {
            Err(DagError::ExecutionFailed {
                node_id: n("dummy"),
                reason: m.clone(),
            })
        })
    }

    fn permanent_fail_fn(msg: &str) -> Box<dyn Fn() -> Result<(), DagError> + Send + Sync> {
        let m = msg.to_owned();
        Box::new(move || Err(DagError::InvalidNodeConfig(m.clone())))
    }

    fn fail_once_fn() -> Box<dyn Fn() -> Result<(), DagError> + Send + Sync> {
        let called = Arc::new(Mutex::new(false));
        let c = called.clone();
        Box::new(move || {
            let mut was = c.lock().unwrap();
            if *was {
                Ok(())
            } else {
                *was = true;
                Err(DagError::ExecutionFailed {
                    node_id: n("retry"),
                    reason: "first attempt failed".into(),
                })
            }
        })
    }

    // ── Empty Graph ───────────────────────────────────────────────────

    #[test]
    fn execute_empty_graph() {
        let engine = create_engine();
        let result = engine.execute(&[]).unwrap();
        assert_eq!(result.state, DagExecutionState::Completed);
        assert_eq!(result.node_count, 0);
        assert_eq!(result.success_count, 0);
        assert_eq!(result.failure_count, 0);
    }

    // ── Single Node ───────────────────────────────────────────────────

    #[test]
    fn execute_single_node_success() {
        let engine = create_engine();
        engine
            .register_node(DagNode::command(n("a"), "A", ok_fn()))
            .unwrap();
        let result = engine.execute(&[]).unwrap();
        assert_eq!(result.state, DagExecutionState::Completed);
        assert_eq!(result.node_count, 1);
        assert_eq!(result.success_count, 1);
        assert_eq!(result.failure_count, 0);
    }

    #[test]
    fn execute_single_node_with_entry_node() {
        let engine = create_engine();
        engine
            .register_node(DagNode::command(n("a"), "A", ok_fn()))
            .unwrap();
        let result = engine.execute(&[n("a")]).unwrap();
        assert_eq!(result.state, DagExecutionState::Completed);
        assert_eq!(result.success_count, 1);
    }

    #[test]
    fn execute_single_node_fails_non_retryable() {
        let engine = create_engine();
        engine
            .register_node(DagNode::command(n("a"), "A", fail_fn("fail")))
            .unwrap();
        let result = engine.execute(&[]).unwrap();
        assert_eq!(result.state, DagExecutionState::Failed);
        assert_eq!(result.failure_count, 1);
        assert_eq!(result.success_count, 0);
        let state = result.node_results.get(&n("a")).unwrap();
        assert!(matches!(state, NodeState::Failed(_)));
    }

    // ── Linear Chain ──────────────────────────────────────────────────

    #[test]
    fn execute_linear_chain() {
        let engine = create_engine();
        engine
            .register_node(DagNode::command(n("a"), "A", ok_fn()))
            .unwrap();
        engine
            .register_node(DagNode::command(n("b"), "B", ok_fn()))
            .unwrap();
        engine
            .register_node(DagNode::command(n("c"), "C", ok_fn()))
            .unwrap();
        engine.add_edge(&n("a"), &n("b")).unwrap();
        engine.add_edge(&n("b"), &n("c")).unwrap();
        let result = engine.execute(&[]).unwrap();
        assert_eq!(result.state, DagExecutionState::Completed);
        assert_eq!(result.node_count, 3);
        assert_eq!(result.success_count, 3);
        assert_eq!(
            result.node_results.get(&n("a")),
            Some(&NodeState::Completed)
        );
        assert_eq!(
            result.node_results.get(&n("b")),
            Some(&NodeState::Completed)
        );
        assert_eq!(
            result.node_results.get(&n("c")),
            Some(&NodeState::Completed)
        );
    }

    // ── Dependency Failure ────────────────────────────────────────────

    #[test]
    fn execute_mid_chain_failure_skips_dependents() {
        let engine = create_engine();
        engine
            .register_node(DagNode::command(n("a"), "A", ok_fn()))
            .unwrap();
        engine
            .register_node(DagNode::command(n("b"), "B", fail_fn("b failed")))
            .unwrap();
        engine
            .register_node(DagNode::command(n("c"), "C", ok_fn()))
            .unwrap();
        engine.add_edge(&n("a"), &n("b")).unwrap();
        engine.add_edge(&n("b"), &n("c")).unwrap();
        let result = engine.execute(&[]).unwrap();
        assert_eq!(result.state, DagExecutionState::Failed);
        assert_eq!(
            result.node_results.get(&n("a")),
            Some(&NodeState::Completed)
        );
        assert!(matches!(
            result.node_results.get(&n("b")),
            Some(&NodeState::Failed(_))
        ));
        assert_eq!(result.node_results.get(&n("c")), Some(&NodeState::Skipped));
    }

    #[test]
    fn execute_first_node_failure_skips_all() {
        let engine = create_engine();
        engine
            .register_node(DagNode::command(n("a"), "A", fail_fn("a failed")))
            .unwrap();
        engine
            .register_node(DagNode::command(n("b"), "B", ok_fn()))
            .unwrap();
        engine.add_edge(&n("a"), &n("b")).unwrap();
        let result = engine.execute(&[]).unwrap();
        assert_eq!(result.state, DagExecutionState::Failed);
        assert!(matches!(
            result.node_results.get(&n("a")),
            Some(&NodeState::Failed(_))
        ));
        assert_eq!(result.node_results.get(&n("b")), Some(&NodeState::Skipped));
    }

    // ── Timeout ────────────────────────────────────────────────────────

    #[test]
    fn execute_retry_timeout_exceeded() {
        let engine = create_engine();
        // Node fails every time; cumulative timeout is very short
        let node = DagNode::command(n("t"), "Timeout", fail_fn("slow fail"))
            .with_retry(RetryPolicy::Immediate { max_retries: 5 })
            .with_timeout(Duration::from_millis(1));
        engine.register_node(node).unwrap();
        let result = engine.execute(&[]).unwrap();
        assert_eq!(result.state, DagExecutionState::Failed);
        assert_eq!(result.failure_count, 1);
    }

    #[test]
    fn execute_node_with_timeout_succeeds() {
        let engine = create_engine();
        // Node succeeds immediately; timeout is generous
        let node = DagNode::command(n("t"), "Fast", ok_fn()).with_timeout(Duration::from_secs(60));
        engine.register_node(node).unwrap();
        let result = engine.execute(&[]).unwrap();
        assert_eq!(result.state, DagExecutionState::Completed);
        assert_eq!(result.success_count, 1);
    }

    #[test]
    fn execute_retry_timeout_during_retry_chain() {
        let engine = create_engine();
        // Fail twice then succeed; cumulative timeout allows 2 attempts
        let called = Arc::new(Mutex::new(0u32));
        let c = called.clone();
        let f: Box<dyn Fn() -> Result<(), DagError> + Send + Sync> = Box::new(move || {
            let mut count = c.lock().unwrap();
            *count += 1;
            if *count < 3 {
                Err(DagError::ExecutionFailed {
                    node_id: n("rt"),
                    reason: format!("attempt {count}"),
                })
            } else {
                Ok(())
            }
        });
        let node = DagNode::command(n("rt"), "RetryTimeout", f)
            .with_retry(RetryPolicy::Immediate { max_retries: 5 })
            .with_timeout(Duration::from_secs(60));
        engine.register_node(node).unwrap();
        let result = engine.execute(&[]).unwrap();
        assert_eq!(result.state, DagExecutionState::Completed);
        assert_eq!(result.success_count, 1);
    }

    // ── Retry ─────────────────────────────────────────────────────────

    #[test]
    fn execute_retry_eventual_success() {
        let engine = create_engine();
        let node = DagNode::command(n("r"), "Retry", fail_once_fn())
            .with_retry(RetryPolicy::Immediate { max_retries: 3 });
        engine.register_node(node).unwrap();
        let result = engine.execute(&[]).unwrap();
        assert_eq!(result.state, DagExecutionState::Completed);
        assert_eq!(result.success_count, 1);
        assert_eq!(
            result.node_results.get(&n("r")),
            Some(&NodeState::Completed)
        );
    }

    #[test]
    fn execute_retry_exhaustion() {
        let engine = create_engine();
        let node = DagNode::command(n("r"), "Retry", fail_fn("persistent fail"))
            .with_retry(RetryPolicy::Immediate { max_retries: 2 });
        engine.register_node(node).unwrap();
        let result = engine.execute(&[]).unwrap();
        assert_eq!(result.state, DagExecutionState::Failed);
        assert_eq!(result.failure_count, 1);
        assert!(matches!(
            result.node_results.get(&n("r")),
            Some(&NodeState::Failed(_))
        ));
    }

    #[test]
    fn execute_non_retryable_error_does_not_retry() {
        let engine = create_engine();
        // InvalidNodeConfig is non-retryable — won't be retried even with policy
        let node = DagNode::command(n("n"), "NonRetry", permanent_fail_fn("not retried"))
            .with_retry(RetryPolicy::Immediate { max_retries: 3 });
        engine.register_node(node).unwrap();
        let result = engine.execute(&[]).unwrap();
        assert_eq!(result.state, DagExecutionState::Failed);
        assert_eq!(result.failure_count, 1);
    }

    // ── Cancel ────────────────────────────────────────────────────────

    #[test]
    fn cancel_execution_before_start_has_no_effect() {
        let engine = create_engine();
        // Cancelling a non-existent execution is a no-op
        assert!(engine.cancel(&ExecutionId::new()).is_ok());
    }

    // ── Execution State Queries ───────────────────────────────────────

    #[test]
    fn execution_state_queries() {
        let engine = create_engine();
        engine
            .register_node(DagNode::command(n("a"), "A", ok_fn()))
            .unwrap();
        let result = engine.execute(&[]).unwrap();
        let exec_id = result.execution_id;

        assert_eq!(
            engine.execution_state(&exec_id),
            Some(DagExecutionState::Completed)
        );
        assert_eq!(
            engine.node_state(&exec_id, &n("a")),
            Some(NodeState::Completed)
        );
        let executions = engine.list_executions();
        assert!(executions.contains(&exec_id));
    }

    #[test]
    fn execution_state_nonexistent() {
        let engine = create_engine();
        let fake_id = ExecutionId::new();
        assert_eq!(engine.execution_state(&fake_id), None);
        assert_eq!(engine.node_state(&fake_id, &n("x")), None);
    }

    // ── Sub-DAG Execution ─────────────────────────────────────────────

    #[test]
    fn execute_sub_dag_node() {
        let engine = create_engine();
        let inner = DagEngine::new(
            noop_event_bus(),
            noop_logger(),
            noop_metrics(),
            noop_tracer(),
        );
        inner
            .register_node(DagNode::command(n("s1"), "Sub1", ok_fn()))
            .unwrap();
        inner
            .register_node(DagNode::command(n("s2"), "Sub2", ok_fn()))
            .unwrap();
        inner.add_edge(&n("s1"), &n("s2")).unwrap();

        engine
            .register_node(DagNode::sub_dag(n("sub"), "SubDag", inner))
            .unwrap();
        let result = engine.execute(&[]).unwrap();
        assert_eq!(result.state, DagExecutionState::Completed);
        assert_eq!(
            result.node_results.get(&n("sub")),
            Some(&NodeState::Completed)
        );
    }

    // ── Result Fields ─────────────────────────────────────────────────

    #[test]
    fn execute_result_has_duration() {
        let engine = create_engine();
        engine
            .register_node(DagNode::command(n("a"), "A", ok_fn()))
            .unwrap();
        let result = engine.execute(&[]).unwrap();
        assert!(result.total_duration.as_nanos() > 0);
        assert_eq!(result.execution_id.to_string().len(), 36); // UUID v4
    }

    // ── Large Graph ───────────────────────────────────────────────────

    #[test]
    fn execute_large_linear_chain() {
        let engine = create_engine();
        let mut prev = n("n0");
        engine
            .register_node(DagNode::command(n("n0"), "N0", ok_fn()))
            .unwrap();
        for i in 1..50 {
            let id = n(&format!("n{i}"));
            engine
                .register_node(DagNode::command(id.clone(), format!("N{i}"), ok_fn()))
                .unwrap();
            engine.add_edge(&prev, &id).unwrap();
            prev = id;
        }
        let result = engine.execute(&[]).unwrap();
        assert_eq!(result.state, DagExecutionState::Completed);
        assert_eq!(result.success_count, 50);
    }

    // ── Diamond DAG ───────────────────────────────────────────────────

    #[test]
    fn execute_diamond_dag() {
        let engine = create_engine();
        engine
            .register_node(DagNode::command(n("a"), "A", ok_fn()))
            .unwrap();
        engine
            .register_node(DagNode::command(n("b"), "B", ok_fn()))
            .unwrap();
        engine
            .register_node(DagNode::command(n("c"), "C", ok_fn()))
            .unwrap();
        engine
            .register_node(DagNode::command(n("d"), "D", ok_fn()))
            .unwrap();
        engine.add_edge(&n("a"), &n("b")).unwrap();
        engine.add_edge(&n("a"), &n("c")).unwrap();
        engine.add_edge(&n("b"), &n("d")).unwrap();
        engine.add_edge(&n("c"), &n("d")).unwrap();
        let result = engine.execute(&[]).unwrap();
        assert_eq!(result.state, DagExecutionState::Completed);
        assert_eq!(result.success_count, 4);
    }

    // ── Disconnected Subgraphs ────────────────────────────────────────

    #[test]
    fn execute_disconnected_subgraphs() {
        let engine = create_engine();
        // Two independent chains: A→B and X→Y
        engine
            .register_node(DagNode::command(n("a"), "A", ok_fn()))
            .unwrap();
        engine
            .register_node(DagNode::command(n("b"), "B", ok_fn()))
            .unwrap();
        engine
            .register_node(DagNode::command(n("x"), "X", ok_fn()))
            .unwrap();
        engine
            .register_node(DagNode::command(n("y"), "Y", ok_fn()))
            .unwrap();
        engine.add_edge(&n("a"), &n("b")).unwrap();
        engine.add_edge(&n("x"), &n("y")).unwrap();
        let result = engine.execute(&[]).unwrap();
        assert_eq!(result.state, DagExecutionState::Completed);
        assert_eq!(result.success_count, 4);
    }

    // ── Metrics ───────────────────────────────────────────────────────

    #[test]
    fn execute_updates_metrics_counters() {
        let metrics = noop_metrics();
        let engine = DagEngine::new(
            noop_event_bus(),
            noop_logger(),
            metrics.clone(),
            noop_tracer(),
        );
        engine
            .register_node(DagNode::command(n("a"), "A", ok_fn()))
            .unwrap();
        let _result = engine.execute(&[]).unwrap();
        assert_eq!(metrics.counter("dag.executions.total").value(), 1);
        assert_eq!(metrics.counter("dag.executions.completed").value(), 1);
        assert!(metrics.histogram("dag.node.duration").count() >= 1);
    }

    // ═══════════════════════════════════════════════════════════════════
    // P9 — Parallel Execution
    // ═══════════════════════════════════════════════════════════════════

    fn create_parallel_engine(max_threads: usize) -> DagEngine {
        DagEngine::with_max_threads(
            noop_event_bus(),
            noop_logger(),
            noop_metrics(),
            noop_tracer(),
            max_threads,
        )
    }

    fn record_fn(
        records: &Arc<Mutex<Vec<String>>>,
        name: &str,
    ) -> Box<dyn Fn() -> Result<(), DagError> + Send + Sync> {
        let r = records.clone();
        let n = name.to_owned();
        Box::new(move || {
            r.lock().unwrap().push(n.clone());
            Ok(())
        })
    }

    #[test]
    fn parallel_executes_independent_nodes_in_same_layer() {
        // Two independent roots should execute in parallel (both in layer 0)
        let engine = create_parallel_engine(2);
        let records = Arc::new(Mutex::new(Vec::new()));
        engine
            .register_node(DagNode::command(n("a"), "A", record_fn(&records, "A")))
            .unwrap();
        engine
            .register_node(DagNode::command(n("b"), "B", record_fn(&records, "B")))
            .unwrap();
        let result = engine.execute(&[]).unwrap();
        assert_eq!(result.state, DagExecutionState::Completed);
        assert_eq!(result.success_count, 2);
    }

    #[test]
    fn parallel_maintains_dependency_ordering() {
        // A→B→C: each layer single node, should execute sequentially
        let engine = create_parallel_engine(4);
        engine
            .register_node(DagNode::command(n("a"), "A", ok_fn()))
            .unwrap();
        engine
            .register_node(DagNode::command(n("b"), "B", ok_fn()))
            .unwrap();
        engine
            .register_node(DagNode::command(n("c"), "C", ok_fn()))
            .unwrap();
        engine.add_edge(&n("a"), &n("b")).unwrap();
        engine.add_edge(&n("b"), &n("c")).unwrap();
        let result = engine.execute(&[]).unwrap();
        assert_eq!(result.state, DagExecutionState::Completed);
        assert_eq!(result.success_count, 3);
        assert_eq!(
            result.node_results.get(&n("a")),
            Some(&NodeState::Completed)
        );
        assert_eq!(
            result.node_results.get(&n("b")),
            Some(&NodeState::Completed)
        );
        assert_eq!(
            result.node_results.get(&n("c")),
            Some(&NodeState::Completed)
        );
    }

    #[test]
    fn parallel_respects_max_threads() {
        // max_threads=1 should fall through to sequential path
        let engine = create_parallel_engine(1);
        engine
            .register_node(DagNode::command(n("a"), "A", ok_fn()))
            .unwrap();
        engine
            .register_node(DagNode::command(n("b"), "B", ok_fn()))
            .unwrap();
        let result = engine.execute(&[]).unwrap();
        assert_eq!(result.state, DagExecutionState::Completed);
        assert_eq!(result.success_count, 2);
    }

    #[test]
    fn parallel_diamond_dag() {
        // A → B → D  and  A → C → D  (B and C in same layer)
        let engine = create_parallel_engine(2);
        engine
            .register_node(DagNode::command(n("a"), "A", ok_fn()))
            .unwrap();
        engine
            .register_node(DagNode::command(n("b"), "B", ok_fn()))
            .unwrap();
        engine
            .register_node(DagNode::command(n("c"), "C", ok_fn()))
            .unwrap();
        engine
            .register_node(DagNode::command(n("d"), "D", ok_fn()))
            .unwrap();
        engine.add_edge(&n("a"), &n("b")).unwrap();
        engine.add_edge(&n("a"), &n("c")).unwrap();
        engine.add_edge(&n("b"), &n("d")).unwrap();
        engine.add_edge(&n("c"), &n("d")).unwrap();
        let result = engine.execute(&[]).unwrap();
        assert_eq!(result.state, DagExecutionState::Completed);
        assert_eq!(result.success_count, 4);
    }

    #[test]
    fn parallel_disconnected_subgraphs() {
        // Two independent chains: A→B and X→Y
        let engine = create_parallel_engine(2);
        engine
            .register_node(DagNode::command(n("a"), "A", ok_fn()))
            .unwrap();
        engine
            .register_node(DagNode::command(n("b"), "B", ok_fn()))
            .unwrap();
        engine
            .register_node(DagNode::command(n("x"), "X", ok_fn()))
            .unwrap();
        engine
            .register_node(DagNode::command(n("y"), "Y", ok_fn()))
            .unwrap();
        engine.add_edge(&n("a"), &n("b")).unwrap();
        engine.add_edge(&n("x"), &n("y")).unwrap();
        let result = engine.execute(&[]).unwrap();
        assert_eq!(result.state, DagExecutionState::Completed);
        assert_eq!(result.success_count, 4);
    }

    #[test]
    fn parallel_with_retry_succeeds() {
        let engine = create_parallel_engine(2);
        let node = DagNode::command(n("r"), "Retry", fail_once_fn())
            .with_retry(RetryPolicy::Immediate { max_retries: 3 });
        engine.register_node(node).unwrap();
        let result = engine.execute(&[]).unwrap();
        assert_eq!(result.state, DagExecutionState::Completed);
        assert_eq!(result.success_count, 1);
    }

    #[test]
    fn parallel_with_retry_exhaustion() {
        let engine = create_parallel_engine(2);
        let node = DagNode::command(n("r"), "Retry", fail_fn("persistent"))
            .with_retry(RetryPolicy::Immediate { max_retries: 2 });
        engine.register_node(node).unwrap();
        let result = engine.execute(&[]).unwrap();
        assert_eq!(result.state, DagExecutionState::Failed);
        assert_eq!(result.failure_count, 1);
    }

    #[test]
    fn parallel_with_timeout_exceeded() {
        let engine = create_parallel_engine(2);
        let node = DagNode::command(n("t"), "Timeout", fail_fn("slow"))
            .with_retry(RetryPolicy::Immediate { max_retries: 3 })
            .with_timeout(Duration::from_millis(1));
        engine.register_node(node).unwrap();
        let result = engine.execute(&[]).unwrap();
        assert_eq!(result.state, DagExecutionState::Failed);
        assert_eq!(result.failure_count, 1);
    }

    #[test]
    fn parallel_failure_propagates_to_dependents() {
        // A → B → C: B fails, C must be skipped
        let engine = create_parallel_engine(2);
        engine
            .register_node(DagNode::command(n("a"), "A", ok_fn()))
            .unwrap();
        engine
            .register_node(DagNode::command(n("b"), "B", fail_fn("b failed")))
            .unwrap();
        engine
            .register_node(DagNode::command(n("c"), "C", ok_fn()))
            .unwrap();
        engine.add_edge(&n("a"), &n("b")).unwrap();
        engine.add_edge(&n("b"), &n("c")).unwrap();
        let result = engine.execute(&[]).unwrap();
        assert_eq!(result.state, DagExecutionState::Failed);
        assert_eq!(
            result.node_results.get(&n("a")),
            Some(&NodeState::Completed)
        );
        assert!(matches!(
            result.node_results.get(&n("b")),
            Some(&NodeState::Failed(_))
        ));
        assert_eq!(result.node_results.get(&n("c")), Some(&NodeState::Skipped));
    }

    #[test]
    fn parallel_sub_dag_node() {
        let engine = create_parallel_engine(2);
        let inner = DagEngine::new(
            noop_event_bus(),
            noop_logger(),
            noop_metrics(),
            noop_tracer(),
        );
        inner
            .register_node(DagNode::command(n("s1"), "Sub1", ok_fn()))
            .unwrap();
        inner
            .register_node(DagNode::command(n("s2"), "Sub2", ok_fn()))
            .unwrap();
        inner.add_edge(&n("s1"), &n("s2")).unwrap();

        engine
            .register_node(DagNode::sub_dag(n("sub"), "SubDag", inner))
            .unwrap();
        let result = engine.execute(&[]).unwrap();
        assert_eq!(result.state, DagExecutionState::Completed);
        assert_eq!(
            result.node_results.get(&n("sub")),
            Some(&NodeState::Completed)
        );
    }

    #[test]
    fn parallel_large_linear_chain() {
        let engine = create_parallel_engine(4);
        let mut prev = n("n0");
        engine
            .register_node(DagNode::command(n("n0"), "N0", ok_fn()))
            .unwrap();
        for i in 1..50 {
            let id = n(&format!("n{i}"));
            engine
                .register_node(DagNode::command(id.clone(), format!("N{i}"), ok_fn()))
                .unwrap();
            engine.add_edge(&prev, &id).unwrap();
            prev = id;
        }
        let result = engine.execute(&[]).unwrap();
        assert_eq!(result.state, DagExecutionState::Completed);
        assert_eq!(result.success_count, 50);
    }

    #[test]
    fn parallel_stress_many_independent_nodes() {
        // 20 independent roots — all execute in parallel (bounded by pool)
        let engine = create_parallel_engine(4);
        let records = Arc::new(Mutex::new(Vec::new()));
        for i in 0..20 {
            let id = n(&format!("n{i}"));
            let r = records.clone();
            let f: Box<dyn Fn() -> Result<(), DagError> + Send + Sync> = Box::new(move || {
                r.lock().unwrap().push(format!("n{i}"));
                Ok(())
            });
            engine
                .register_node(DagNode::command(id, format!("N{i}"), f))
                .unwrap();
        }
        let result = engine.execute(&[]).unwrap();
        assert_eq!(result.state, DagExecutionState::Completed);
        assert_eq!(result.success_count, 20);
    }

    #[test]
    fn parallel_stress_no_duplicate_executions() {
        // Verify each node executes exactly once by counting total completions
        let engine = create_parallel_engine(4);
        let counter = Arc::new(AtomicUsize::new(0));
        for i in 0..10 {
            let id = n(&format!("n{i}"));
            let c = counter.clone();
            let f: Box<dyn Fn() -> Result<(), DagError> + Send + Sync> = Box::new(move || {
                c.fetch_add(1, Ordering::Relaxed);
                Ok(())
            });
            engine
                .register_node(DagNode::command(id, format!("N{i}"), f))
                .unwrap();
        }
        let result = engine.execute(&[]).unwrap();
        assert_eq!(result.state, DagExecutionState::Completed);
        assert_eq!(counter.load(Ordering::Relaxed), 10);
        assert_eq!(result.success_count, 10);
    }

    #[test]
    fn parallel_pool_smaller_than_layer() {
        // 8 independent nodes with pool of 2 — should all complete
        let engine = create_parallel_engine(2);
        for i in 0..8 {
            let id = n(&format!("n{i}"));
            engine
                .register_node(DagNode::command(id, format!("N{i}"), ok_fn()))
                .unwrap();
        }
        let result = engine.execute(&[]).unwrap();
        assert_eq!(result.state, DagExecutionState::Completed);
        assert_eq!(result.success_count, 8);
    }

    #[test]
    fn parallel_metrics_and_events() {
        let metrics = noop_metrics();
        let engine = DagEngine::with_max_threads(
            noop_event_bus(),
            noop_logger(),
            metrics.clone(),
            noop_tracer(),
            2,
        );
        engine
            .register_node(DagNode::command(n("a"), "A", ok_fn()))
            .unwrap();
        engine
            .register_node(DagNode::command(n("b"), "B", ok_fn()))
            .unwrap();
        let _result = engine.execute(&[]).unwrap();
        assert_eq!(metrics.counter("dag.executions.total").value(), 1);
        assert_eq!(metrics.counter("dag.executions.completed").value(), 1);
        assert!(metrics.counter("dag.nodes.started").value() >= 2);
        assert!(metrics.counter("dag.nodes.completed").value() >= 2);
    }
}
