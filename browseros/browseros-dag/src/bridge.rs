use std::fmt::Display;

use browseros_types::identifiers::NodeId;

use crate::error::DagError;
use crate::node::DagNode;

/// Create a DAG node from a closure that produces a generic `Result`.
///
/// This is the primary integration point for bridge ports: the closure
/// captures whatever ports or handles it needs, calls bridge methods,
/// and returns `Result<(), E>` where `E: Display`.  Errors are
/// automatically wrapped in [`DagError::ExecutionFailed`].
///
/// # Usage
///
/// ```rust,ignore
/// use browseros_dag::bridge_node;
///
/// let node = bridge_node(
///     NodeId::from_string("navigate"),
///     "Navigate to page",
///     move || {
///         page_port.navigate("https://example.com")?;
///         Ok(())
///     },
/// );
/// dag.register_node(node)?;
/// ```
///
/// The closure's error type `E` is inferred from the `?` operator.
/// Bridge ports return [`BridgeResult`](browseros_bridge::BridgeResult)
/// — the `Display` implementation on `BridgeError` produces a readable
/// message that becomes the `DagError::ExecutionFailed` reason.
pub fn bridge_node<F, E>(id: NodeId, name: impl Into<String>, f: F) -> DagNode
where
    F: Fn() -> Result<(), E> + Send + Sync + 'static,
    E: Display,
{
    DagNode::command(
        id.clone(),
        name,
        Box::new(move || {
            f().map_err(|e| DagError::ExecutionFailed {
                node_id: id.clone(),
                reason: e.to_string(),
            })
        }),
    )
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use browseros_types::identifiers::NodeId;

    use crate::bridge::bridge_node;
    use crate::engine::DagEngine;
    use crate::node::{DagExecutionState, DagNode, NodeState};

    fn create_engine() -> DagEngine {
        DagEngine::new(
            crate::engine::tests::noop_event_bus(),
            crate::engine::tests::noop_logger(),
            crate::engine::tests::noop_metrics(),
            crate::engine::tests::noop_tracer(),
        )
    }

    #[test]
    fn bridge_node_executes_successfully() {
        let engine = create_engine();
        let node: DagNode = bridge_node(NodeId::from_string("a"), "bridge-success", move || {
            Ok::<(), std::convert::Infallible>(())
        });
        engine.register_node(node).unwrap();
        let result = engine.execute(&[NodeId::from_string("a")]).unwrap();
        assert_eq!(result.state, DagExecutionState::Completed);
    }

    #[test]
    fn bridge_node_wraps_error() {
        let engine = create_engine();
        #[derive(Debug)]
        struct SimulatedError(String);

        impl std::fmt::Display for SimulatedError {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "SIM: {}", self.0)
            }
        }

        let node = bridge_node(NodeId::from_string("b"), "bridge-fail", move || {
            Err(SimulatedError("nope".to_string()))
        });
        engine.register_node(node).unwrap();
        let result = engine.execute(&[NodeId::from_string("b")]).unwrap();
        assert_eq!(result.state, DagExecutionState::Failed);
        assert_eq!(result.failure_count, 1);
        let state = result.node_results.get(&NodeId::from_string("b")).unwrap();
        assert!(matches!(state, NodeState::Failed(_)));
    }

    #[test]
    fn bridge_node_is_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        let node: DagNode = bridge_node(NodeId::from_string("c"), "send-sync", move || {
            Ok::<(), std::convert::Infallible>(())
        });
        assert_send::<DagNode>();
        assert_sync::<DagNode>();
        let _ = node;
    }

    #[test]
    fn bridge_node_captures_external_state() {
        let engine = create_engine();
        let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let c = counter.clone();

        let node: DagNode = bridge_node(NodeId::from_string("d"), "capture-state", move || {
            c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok::<(), std::convert::Infallible>(())
        });
        engine.register_node(node).unwrap();
        engine.execute(&[NodeId::from_string("d")]).unwrap();
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn bridge_node_works_with_multiple_nodes() {
        let engine = create_engine();

        let node1: DagNode = bridge_node(NodeId::from_string("x"), "step-1", move || {
            Ok::<(), std::convert::Infallible>(())
        });
        let node2: DagNode = bridge_node(NodeId::from_string("y"), "step-2", move || {
            Ok::<(), std::convert::Infallible>(())
        });

        engine.register_node(node1).unwrap();
        engine.register_node(node2).unwrap();
        engine
            .add_edge(&NodeId::from_string("x"), &NodeId::from_string("y"))
            .unwrap();

        let result = engine.execute(&[NodeId::from_string("x")]).unwrap();
        assert_eq!(result.state, DagExecutionState::Completed);
        assert_eq!(result.node_count, 2);
    }
}
