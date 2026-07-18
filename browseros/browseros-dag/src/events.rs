use std::any::Any;
use std::time::Duration;

use browseros_types::event::{Event, EventCategory, EventMetadata};
use browseros_types::identifiers::{ExecutionId, NodeId};
use chrono::Utc;

fn dag_module_id() -> browseros_types::identifiers::ModuleId {
    browseros_types::identifiers::ModuleId::new(
        "browseros-dag",
        browseros_types::value::SemVer::new(0, 1, 0),
    )
}

fn default_metadata(correlation_id: browseros_types::identifiers::CorrelationId) -> EventMetadata {
    EventMetadata::new(
        dag_module_id(),
        correlation_id,
        None,
        browseros_types::value::ContentType::new("application/x.browseros.dag.event.v1+json"),
        Utc::now(),
    )
}

macro_rules! impl_event {
    ($ty:ty, $kind:expr) => {
        impl Event for $ty {
            fn kind(&self) -> &'static str {
                $kind
            }
            fn category(&self) -> EventCategory {
                EventCategory::Domain
            }
            fn metadata(&self) -> &EventMetadata {
                &self.metadata
            }
            fn as_any(&self) -> &dyn Any {
                self
            }
        }
    };
}

#[derive(Debug, Clone)]
pub struct DagExecutionStarted {
    pub metadata: EventMetadata,
    pub execution_id: ExecutionId,
    pub node_count: u32,
    pub entry_nodes: Vec<NodeId>,
}

impl DagExecutionStarted {
    pub fn new(
        execution_id: ExecutionId,
        correlation_id: browseros_types::identifiers::CorrelationId,
        node_count: u32,
        entry_nodes: Vec<NodeId>,
    ) -> Self {
        Self {
            metadata: default_metadata(correlation_id),
            execution_id,
            node_count,
            entry_nodes,
        }
    }
}

impl_event!(DagExecutionStarted, "dag.execution_started");

#[derive(Debug, Clone)]
pub struct DagExecutionCompleted {
    pub metadata: EventMetadata,
    pub execution_id: ExecutionId,
    pub total_duration: Duration,
    pub success_count: u32,
    pub node_count: u32,
}

impl DagExecutionCompleted {
    pub fn new(
        execution_id: ExecutionId,
        correlation_id: browseros_types::identifiers::CorrelationId,
        total_duration: Duration,
        success_count: u32,
        node_count: u32,
    ) -> Self {
        Self {
            metadata: default_metadata(correlation_id),
            execution_id,
            total_duration,
            success_count,
            node_count,
        }
    }
}

impl_event!(DagExecutionCompleted, "dag.execution_completed");

#[derive(Debug, Clone)]
pub struct DagExecutionFailed {
    pub metadata: EventMetadata,
    pub execution_id: ExecutionId,
    pub failed_node: NodeId,
    pub reason: String,
    pub success_count: u32,
    pub failure_count: u32,
}

impl DagExecutionFailed {
    pub fn new(
        execution_id: ExecutionId,
        correlation_id: browseros_types::identifiers::CorrelationId,
        failed_node: NodeId,
        reason: String,
        success_count: u32,
        failure_count: u32,
    ) -> Self {
        Self {
            metadata: default_metadata(correlation_id),
            execution_id,
            failed_node,
            reason,
            success_count,
            failure_count,
        }
    }
}

impl_event!(DagExecutionFailed, "dag.execution_failed");

#[derive(Debug, Clone)]
pub struct DagExecutionCancelled {
    pub metadata: EventMetadata,
    pub execution_id: ExecutionId,
    pub completed_nodes: u32,
}

impl DagExecutionCancelled {
    pub fn new(
        execution_id: ExecutionId,
        correlation_id: browseros_types::identifiers::CorrelationId,
        completed_nodes: u32,
    ) -> Self {
        Self {
            metadata: default_metadata(correlation_id),
            execution_id,
            completed_nodes,
        }
    }
}

impl_event!(DagExecutionCancelled, "dag.execution_cancelled");

#[derive(Debug, Clone)]
pub struct DagNodeStarted {
    pub metadata: EventMetadata,
    pub execution_id: ExecutionId,
    pub node_id: NodeId,
    pub node_name: String,
    pub attempt: u32,
}

impl DagNodeStarted {
    pub fn new(
        execution_id: ExecutionId,
        correlation_id: browseros_types::identifiers::CorrelationId,
        node_id: NodeId,
        node_name: String,
        attempt: u32,
    ) -> Self {
        Self {
            metadata: default_metadata(correlation_id),
            execution_id,
            node_id,
            node_name,
            attempt,
        }
    }
}

impl_event!(DagNodeStarted, "dag.node.started");

#[derive(Debug, Clone)]
pub struct DagNodeCompleted {
    pub metadata: EventMetadata,
    pub execution_id: ExecutionId,
    pub node_id: NodeId,
    pub node_name: String,
    pub duration: Duration,
    pub attempt: u32,
}

impl DagNodeCompleted {
    pub fn new(
        execution_id: ExecutionId,
        correlation_id: browseros_types::identifiers::CorrelationId,
        node_id: NodeId,
        node_name: String,
        duration: Duration,
        attempt: u32,
    ) -> Self {
        Self {
            metadata: default_metadata(correlation_id),
            execution_id,
            node_id,
            node_name,
            duration,
            attempt,
        }
    }
}

impl_event!(DagNodeCompleted, "dag.node.completed");

#[derive(Debug, Clone)]
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

impl DagNodeRetrying {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        execution_id: ExecutionId,
        correlation_id: browseros_types::identifiers::CorrelationId,
        node_id: NodeId,
        node_name: String,
        attempt: u32,
        max_retries: u32,
        next_delay_ms: u64,
        error: String,
    ) -> Self {
        Self {
            metadata: default_metadata(correlation_id),
            execution_id,
            node_id,
            node_name,
            attempt,
            max_retries,
            next_delay_ms,
            error,
        }
    }
}

impl_event!(DagNodeRetrying, "dag.node.retrying");

#[derive(Debug, Clone)]
pub struct DagNodeFailed {
    pub metadata: EventMetadata,
    pub execution_id: ExecutionId,
    pub node_id: NodeId,
    pub node_name: String,
    pub attempt: u32,
    pub error: String,
}

impl DagNodeFailed {
    pub fn new(
        execution_id: ExecutionId,
        correlation_id: browseros_types::identifiers::CorrelationId,
        node_id: NodeId,
        node_name: String,
        attempt: u32,
        error: String,
    ) -> Self {
        Self {
            metadata: default_metadata(correlation_id),
            execution_id,
            node_id,
            node_name,
            attempt,
            error,
        }
    }
}

impl_event!(DagNodeFailed, "dag.node.failed");

#[derive(Debug, Clone)]
pub struct DagNodeSkipped {
    pub metadata: EventMetadata,
    pub execution_id: ExecutionId,
    pub node_id: NodeId,
    pub node_name: String,
    pub reason: String,
}

impl DagNodeSkipped {
    pub fn new(
        execution_id: ExecutionId,
        correlation_id: browseros_types::identifiers::CorrelationId,
        node_id: NodeId,
        node_name: String,
        reason: String,
    ) -> Self {
        Self {
            metadata: default_metadata(correlation_id),
            execution_id,
            node_id,
            node_name,
            reason,
        }
    }
}

impl_event!(DagNodeSkipped, "dag.node.skipped");
