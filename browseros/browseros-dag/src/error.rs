use std::time::Duration;

use browseros_types::identifiers::{ExecutionId, NodeId};
use thiserror::Error;

impl DagError {
    /// Returns `true` when the error is transient and the operation may
    /// succeed on retry.
    ///
    /// Only [`ExecutionFailed`](DagError::ExecutionFailed) and
    /// [`ExecutionTimedOut`](DagError::ExecutionTimedOut) are considered
    /// retryable.  Structural errors (node not found, cycle, etc.),
    /// [`Cancelled`](DagError::Cancelled), and
    /// [`RetriesExhausted`](DagError::RetriesExhausted) are permanent.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            DagError::ExecutionFailed { .. } | DagError::ExecutionTimedOut { .. }
        )
    }
}

#[derive(Debug, Clone, Error)]
pub enum DagError {
    #[error("node '{0}' already exists")]
    NodeAlreadyExists(NodeId),
    #[error("node '{0}' not found")]
    NodeNotFound(NodeId),
    #[error("edge {from} -> {to} creates a cycle")]
    CycleDetected { from: NodeId, to: NodeId },
    #[error("self-loop on node '{0}' is not allowed")]
    SelfLoop(NodeId),
    #[error("duplicate edge {from} -> {to} already exists")]
    DuplicateEdge { from: NodeId, to: NodeId },
    #[error("node '{node_id}' execution failed: {reason}")]
    ExecutionFailed { node_id: NodeId, reason: String },
    #[error("node '{node_id}' timed out after {timeout:?}")]
    ExecutionTimedOut { node_id: NodeId, timeout: Duration },
    #[error("node '{node_id}' retries exhausted after {attempts} attempts")]
    RetriesExhausted { node_id: NodeId, attempts: u32 },
    #[error("execution {0} cancelled")]
    Cancelled(ExecutionId),
    #[error("DAG locked by execution {0}")]
    DagLocked(ExecutionId),
    #[error("node '{0}' is not a valid entry node (has incoming edges)")]
    InvalidEntry(NodeId),
    #[error("node '{0}' is not a valid exit node (has outgoing edges)")]
    InvalidExit(NodeId),
    #[error("graph internal consistency check failed")]
    GraphInconsistent,
    #[error("invalid node configuration: {0}")]
    InvalidNodeConfig(String),
}
