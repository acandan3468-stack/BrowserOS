use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use browseros_types::clock::CancellationToken;
use browseros_types::error::RetryPolicy;
use browseros_types::identifiers::{ExecutionId, NodeId};
use serde::{Deserialize, Serialize};

use crate::engine::DagEngine;
use crate::error::DagError;

/// The kind of work a node performs.
#[derive(Clone)]
pub enum NodeKind {
    /// A synchronous function call.
    Command(Arc<dyn Fn() -> Result<(), DagError> + Send + Sync>),
    /// A nested DAG (sub-DAG).
    SubDag(Arc<DagEngine>),
}

impl std::fmt::Debug for NodeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NodeKind::Command(_) => f.debug_tuple("Command").finish(),
            NodeKind::SubDag(engine) => f.debug_tuple("SubDag").field(engine).finish(),
        }
    }
}

/// Metadata attached to a DAG node for documentation and filtering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeMetadata {
    /// Human-readable description of the node's purpose.
    pub description: Option<String>,
    /// Tags for categorization and filtering.
    pub tags: Vec<String>,
}

impl NodeMetadata {
    pub fn new() -> Self {
        Self {
            description: None,
            tags: Vec::new(),
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }
}

impl Default for NodeMetadata {
    fn default() -> Self {
        Self::new()
    }
}

/// A single unit of work in a DAG.
#[derive(Clone)]
pub struct DagNode {
    pub id: NodeId,
    pub name: String,
    pub kind: NodeKind,
    pub retry_policy: Option<RetryPolicy>,
    pub timeout: Option<Duration>,
    pub max_retries: u32,
    pub cancellation_token: Option<CancellationToken>,
    pub metadata: Option<NodeMetadata>,
}

impl std::fmt::Debug for DagNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DagNode")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("kind", &self.kind)
            .field("retry_policy", &self.retry_policy)
            .field("timeout", &self.timeout)
            .field("max_retries", &self.max_retries)
            .field("cancellation_token", &self.cancellation_token)
            .field("metadata", &self.metadata)
            .finish()
    }
}

impl DagNode {
    /// Create a new command node.
    ///
    /// # Validation
    /// The node is validated upon construction. An empty name or invalid
    /// configuration returns `DagError::InvalidNodeConfig`.
    pub fn command(
        id: NodeId,
        name: impl Into<String>,
        f: Box<dyn Fn() -> Result<(), DagError> + Send + Sync>,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            kind: NodeKind::Command(Arc::from(f)),
            retry_policy: None,
            timeout: None,
            max_retries: 0,
            cancellation_token: None,
            metadata: None,
        }
    }

    /// Create a new sub-DAG node.
    ///
    /// # Validation
    /// The node is validated upon construction. An empty name or invalid
    /// configuration returns `DagError::InvalidNodeConfig`.
    pub fn sub_dag(id: NodeId, name: impl Into<String>, sub_dag: DagEngine) -> Self {
        Self {
            id,
            name: name.into(),
            kind: NodeKind::SubDag(Arc::new(sub_dag)),
            retry_policy: None,
            timeout: None,
            max_retries: 0,
            cancellation_token: None,
            metadata: None,
        }
    }

    /// Set retry policy for this node.
    ///
    /// Validates that the policy has `max_retries > 0` and, for
    /// `ExponentialBackoff`, `initial_delay_ms > 0` and `multiplier >= 1.0`.
    pub fn with_retry(mut self, policy: RetryPolicy) -> Self {
        self.retry_policy = Some(policy);
        self
    }

    /// Set execution timeout for this node.
    ///
    /// The timeout must be non-zero (use `with_retry` instead of zero delay).
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Associate a cancellation token with this node.
    ///
    /// The token is checked cooperatively before execution and between
    /// retry attempts. Definition only — cancellation logic is implemented
    /// in the executor (P10).
    pub fn with_cancellation_token(mut self, token: CancellationToken) -> Self {
        self.cancellation_token = Some(token);
        self
    }

    /// Attach metadata to this node.
    pub fn with_metadata(mut self, metadata: NodeMetadata) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Validate this node's configuration.
    ///
    /// Returns `Ok(())` if all fields are valid, or
    /// `Err(DagError::InvalidNodeConfig(reason))` with a description of
    /// the first invalid field.
    ///
    /// # Validation Rules
    /// 1. Name must be non-empty
    /// 2. If `max_retries > 0`, a `retry_policy` must be set
    /// 3. Retry policy `max_retries` must be > 0 when set via policy
    /// 4. `ExponentialBackoff`: `initial_delay_ms > 0`, `multiplier >= 1.0`
    /// 5. Timeout must be non-zero if set
    pub fn validate(&self) -> Result<(), DagError> {
        if self.name.is_empty() {
            return Err(DagError::InvalidNodeConfig(
                "node name must not be empty".into(),
            ));
        }
        if self.max_retries > 0 && self.retry_policy.is_none() {
            return Err(DagError::InvalidNodeConfig(
                "max_retries > 0 requires a retry_policy to be set".into(),
            ));
        }
        if let Some(ref policy) = self.retry_policy {
            match policy {
                RetryPolicy::Immediate { max_retries } => {
                    if *max_retries == 0 {
                        return Err(DagError::InvalidNodeConfig(
                            "retry_policy max_retries must be > 0".into(),
                        ));
                    }
                }
                RetryPolicy::ExponentialBackoff {
                    initial_delay_ms,
                    max_delay_ms,
                    multiplier,
                    max_retries,
                    jitter: _,
                } => {
                    if *max_retries == 0 {
                        return Err(DagError::InvalidNodeConfig(
                            "retry_policy max_retries must be > 0".into(),
                        ));
                    }
                    if *initial_delay_ms == 0 {
                        return Err(DagError::InvalidNodeConfig(
                            "ExponentialBackoff initial_delay_ms must be > 0".into(),
                        ));
                    }
                    if *max_delay_ms == 0 {
                        return Err(DagError::InvalidNodeConfig(
                            "ExponentialBackoff max_delay_ms must be > 0".into(),
                        ));
                    }
                    if *multiplier < 1.0 {
                        return Err(DagError::InvalidNodeConfig(
                            "ExponentialBackoff multiplier must be >= 1.0".into(),
                        ));
                    }
                    if *max_delay_ms < *initial_delay_ms {
                        return Err(DagError::InvalidNodeConfig(
                            "ExponentialBackoff max_delay_ms must be >= initial_delay_ms".into(),
                        ));
                    }
                }
            }
        }
        if let Some(timeout) = self.timeout {
            if timeout.is_zero() {
                return Err(DagError::InvalidNodeConfig(
                    "timeout must be non-zero".into(),
                ));
            }
        }
        Ok(())
    }
}

/// State of a single node within a DAG execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeState {
    Pending,
    Running,
    Completed,
    Failed(String),
    Skipped,
    Cancelled,
}

/// Overall state of a DAG execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DagExecutionState {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// An ephemeral DAG definition for one-shot execution.
#[derive(Debug)]
pub struct DagDefinition {
    pub nodes: Vec<DagNode>,
    pub edges: Vec<(NodeId, NodeId)>,
}

impl DagDefinition {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }

    pub fn add_node(mut self, node: DagNode) -> Self {
        self.nodes.push(node);
        self
    }

    pub fn add_edge(mut self, from: NodeId, to: NodeId) -> Self {
        self.edges.push((from, to));
        self
    }

    /// Returns the number of nodes in this definition.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Returns the number of edges in this definition.
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Collects all unique NodeIds referenced in edges.
    pub fn referenced_node_ids(&self) -> Vec<NodeId> {
        let mut ids: Vec<NodeId> = self.nodes.iter().map(|n| n.id.clone()).collect();
        for (from, to) in &self.edges {
            if !ids.contains(from) {
                ids.push(from.clone());
            }
            if !ids.contains(to) {
                ids.push(to.clone());
            }
        }
        ids
    }
}

impl Default for DagDefinition {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of a DAG execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagResult {
    pub execution_id: ExecutionId,
    pub state: DagExecutionState,
    pub node_results: HashMap<NodeId, NodeState>,
    pub total_duration: Duration,
    pub node_count: u32,
    pub success_count: u32,
    pub failure_count: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(id: &str) -> NodeId {
        NodeId::from_string(id)
    }

    fn ok_fn() -> Box<dyn Fn() -> Result<(), DagError> + Send + Sync> {
        Box::new(|| Ok(()))
    }

    // ── Command Node ───────────────────────────────────────────────────

    #[test]
    fn command_node_creation() {
        let node = DagNode::command(n("test"), "Test Node", ok_fn());
        assert_eq!(node.id, n("test"));
        assert_eq!(node.name, "Test Node");
        assert!(matches!(node.kind, NodeKind::Command(_)));
        assert!(node.retry_policy.is_none());
        assert!(node.timeout.is_none());
        assert_eq!(node.max_retries, 0);
        assert!(node.cancellation_token.is_none());
        assert!(node.metadata.is_none());
    }

    #[test]
    fn command_node_debug() {
        let node = DagNode::command(n("a"), "alpha", ok_fn());
        let debug = format!("{node:?}");
        assert!(debug.contains("DagNode"));
        assert!(debug.contains("alpha"));
    }

    // ── Sub-Dag Node ──────────────────────────────────────────────────

    #[test]
    fn sub_dag_node_creation() {
        let inner = DagEngine::new(
            crate::engine::tests::noop_event_bus(),
            crate::engine::tests::noop_logger(),
            crate::engine::tests::noop_metrics(),
            crate::engine::tests::noop_tracer(),
        );
        let node = DagNode::sub_dag(n("sub"), "Sub Workflow", inner);
        assert_eq!(node.id, n("sub"));
        assert_eq!(node.name, "Sub Workflow");
        assert!(matches!(node.kind, NodeKind::SubDag(_)));
    }

    #[test]
    fn sub_dag_node_debug() {
        let inner = DagEngine::new(
            crate::engine::tests::noop_event_bus(),
            crate::engine::tests::noop_logger(),
            crate::engine::tests::noop_metrics(),
            crate::engine::tests::noop_tracer(),
        );
        let node = DagNode::sub_dag(n("sub"), "Sub", inner);
        let debug = format!("{node:?}");
        assert!(debug.contains("SubDag"));
    }

    // ── Builder Methods ───────────────────────────────────────────────

    #[test]
    fn with_retry_immediate() {
        let node = DagNode::command(n("r"), "Retry", ok_fn())
            .with_retry(RetryPolicy::Immediate { max_retries: 3 });
        assert!(node.retry_policy.is_some());
        match node.retry_policy.unwrap() {
            RetryPolicy::Immediate { max_retries } => assert_eq!(max_retries, 3),
            _ => panic!("expected Immediate"),
        }
    }

    #[test]
    fn with_retry_exponential() {
        let node =
            DagNode::command(n("e"), "Exp", ok_fn()).with_retry(RetryPolicy::ExponentialBackoff {
                initial_delay_ms: 100,
                max_delay_ms: 5_000,
                multiplier: 2.0,
                max_retries: 3,
                jitter: true,
            });
        assert!(node.retry_policy.is_some());
    }

    #[test]
    fn with_timeout() {
        let node = DagNode::command(n("t"), "Timed", ok_fn()).with_timeout(Duration::from_secs(30));
        assert_eq!(node.timeout, Some(Duration::from_secs(30)));
    }

    #[test]
    fn with_cancellation_token() {
        let token = CancellationToken::new();
        let node =
            DagNode::command(n("c"), "Cancellable", ok_fn()).with_cancellation_token(token.clone());
        assert!(node.cancellation_token.is_some());
    }

    #[test]
    fn with_metadata() {
        let meta = NodeMetadata::new()
            .with_description("processes data")
            .with_tag("etl")
            .with_tag("critical");
        let node = DagNode::command(n("m"), "Meta", ok_fn()).with_metadata(meta);
        let m = node.metadata.unwrap();
        assert_eq!(m.description.unwrap(), "processes data");
        assert_eq!(m.tags, vec!["etl".to_string(), "critical".to_string()]);
    }

    // ── Validation ────────────────────────────────────────────────────

    #[test]
    fn validate_valid_command_node() {
        let node = DagNode::command(n("v"), "Valid", ok_fn());
        assert!(node.validate().is_ok());
    }

    #[test]
    fn validate_empty_name() {
        let node = DagNode::command(n("e"), "", ok_fn());
        let err = node.validate().unwrap_err();
        match err {
            DagError::InvalidNodeConfig(msg) => assert!(msg.contains("name")),
            other => panic!("expected InvalidNodeConfig, got {other:?}"),
        }
    }

    #[test]
    fn validate_max_retries_without_policy() {
        let mut node = DagNode::command(n("m"), "Misconfig", ok_fn());
        node.max_retries = 3;
        let err = node.validate().unwrap_err();
        match err {
            DagError::InvalidNodeConfig(msg) => assert!(msg.contains("max_retries")),
            other => panic!("expected InvalidNodeConfig, got {other:?}"),
        }
    }

    #[test]
    fn validate_immediate_zero_retries() {
        let node = DagNode::command(n("z"), "Zero", ok_fn())
            .with_retry(RetryPolicy::Immediate { max_retries: 0 });
        let err = node.validate().unwrap_err();
        match err {
            DagError::InvalidNodeConfig(msg) => assert!(msg.contains("max_retries")),
            other => panic!("expected InvalidNodeConfig, got {other:?}"),
        }
    }

    #[test]
    fn validate_exponential_zero_retries() {
        let node =
            DagNode::command(n("z"), "Zero", ok_fn()).with_retry(RetryPolicy::ExponentialBackoff {
                initial_delay_ms: 100,
                max_delay_ms: 5_000,
                multiplier: 2.0,
                max_retries: 0,
                jitter: true,
            });
        let err = node.validate().unwrap_err();
        match err {
            DagError::InvalidNodeConfig(msg) => assert!(msg.contains("max_retries")),
            other => panic!("expected InvalidNodeConfig, got {other:?}"),
        }
    }

    #[test]
    fn validate_exponential_zero_initial_delay() {
        let node = DagNode::command(n("d"), "Delay", ok_fn()).with_retry(
            RetryPolicy::ExponentialBackoff {
                initial_delay_ms: 0,
                max_delay_ms: 5_000,
                multiplier: 2.0,
                max_retries: 3,
                jitter: true,
            },
        );
        let err = node.validate().unwrap_err();
        match err {
            DagError::InvalidNodeConfig(msg) => assert!(msg.contains("initial_delay_ms")),
            other => panic!("expected InvalidNodeConfig, got {other:?}"),
        }
    }

    #[test]
    fn validate_exponential_zero_max_delay() {
        let node = DagNode::command(n("d"), "Delay", ok_fn()).with_retry(
            RetryPolicy::ExponentialBackoff {
                initial_delay_ms: 100,
                max_delay_ms: 0,
                multiplier: 2.0,
                max_retries: 3,
                jitter: true,
            },
        );
        let err = node.validate().unwrap_err();
        match err {
            DagError::InvalidNodeConfig(msg) => assert!(msg.contains("max_delay_ms")),
            other => panic!("expected InvalidNodeConfig, got {other:?}"),
        }
    }

    #[test]
    fn validate_exponential_multiplier_too_low() {
        let node =
            DagNode::command(n("m"), "Mult", ok_fn()).with_retry(RetryPolicy::ExponentialBackoff {
                initial_delay_ms: 100,
                max_delay_ms: 5_000,
                multiplier: 0.5,
                max_retries: 3,
                jitter: true,
            });
        let err = node.validate().unwrap_err();
        match err {
            DagError::InvalidNodeConfig(msg) => assert!(msg.contains("multiplier")),
            other => panic!("expected InvalidNodeConfig, got {other:?}"),
        }
    }

    #[test]
    fn validate_exponential_max_delay_less_than_initial() {
        let node =
            DagNode::command(n("x"), "X", ok_fn()).with_retry(RetryPolicy::ExponentialBackoff {
                initial_delay_ms: 5_000,
                max_delay_ms: 100,
                multiplier: 2.0,
                max_retries: 3,
                jitter: true,
            });
        let err = node.validate().unwrap_err();
        match err {
            DagError::InvalidNodeConfig(msg) => assert!(msg.contains("max_delay_ms")),
            other => panic!("expected InvalidNodeConfig, got {other:?}"),
        }
    }

    #[test]
    fn validate_zero_timeout() {
        let node = DagNode::command(n("t"), "Time", ok_fn()).with_timeout(Duration::ZERO);
        let err = node.validate().unwrap_err();
        match err {
            DagError::InvalidNodeConfig(msg) => assert!(msg.contains("timeout")),
            other => panic!("expected InvalidNodeConfig, got {other:?}"),
        }
    }

    #[test]
    fn validate_valid_retry_config() {
        let node = DagNode::command(n("v"), "Valid Retry", ok_fn()).with_retry(
            RetryPolicy::ExponentialBackoff {
                initial_delay_ms: 100,
                max_delay_ms: 10_000,
                multiplier: 2.0,
                max_retries: 5,
                jitter: true,
            },
        );
        assert!(node.validate().is_ok());
    }

    #[test]
    fn validate_command_with_all_options() {
        let token = CancellationToken::new();
        let node = DagNode::command(n("full"), "Fully Configured", ok_fn())
            .with_retry(RetryPolicy::Immediate { max_retries: 3 })
            .with_timeout(Duration::from_secs(60))
            .with_cancellation_token(token)
            .with_metadata(
                NodeMetadata::new()
                    .with_description("full node")
                    .with_tag("demo"),
            );
        assert!(node.validate().is_ok());
    }

    // ── NodeMetadata ──────────────────────────────────────────────────

    #[test]
    fn node_metadata_default() {
        let m = NodeMetadata::default();
        assert!(m.description.is_none());
        assert!(m.tags.is_empty());
    }

    #[test]
    fn node_metadata_builder() {
        let m = NodeMetadata::new()
            .with_description("critical path")
            .with_tag("high-priority")
            .with_tag("core");
        assert_eq!(m.description.unwrap(), "critical path");
        assert_eq!(m.tags.len(), 2);
    }

    #[test]
    fn node_metadata_with_tags() {
        let tags = vec!["a".into(), "b".into()];
        let m = NodeMetadata::new().with_tags(tags);
        assert_eq!(m.tags.len(), 2);
    }

    #[test]
    fn node_metadata_debug() {
        let m = NodeMetadata::new().with_description("test");
        let debug = format!("{m:?}");
        assert!(debug.contains("test"));
    }

    #[test]
    fn node_metadata_clone() {
        let m1 = NodeMetadata::new().with_description("clone").with_tag("x");
        let m2 = m1.clone();
        assert_eq!(m1, m2);
    }

    // ── NodeState ─────────────────────────────────────────────────────

    #[test]
    fn node_state_variants() {
        assert_eq!(format!("{:?}", NodeState::Pending), "Pending");
        assert_eq!(format!("{:?}", NodeState::Running), "Running");
        assert_eq!(format!("{:?}", NodeState::Completed), "Completed");
        assert_eq!(
            format!("{:?}", NodeState::Failed("err".into())),
            "Failed(\"err\")"
        );
        assert_eq!(format!("{:?}", NodeState::Skipped), "Skipped");
        assert_eq!(format!("{:?}", NodeState::Cancelled), "Cancelled");
    }

    #[test]
    fn node_state_clone() {
        let s = NodeState::Failed("oops".into());
        assert_eq!(s.clone(), s);
    }

    #[test]
    fn node_state_eq() {
        assert_eq!(NodeState::Pending, NodeState::Pending);
        assert_ne!(NodeState::Pending, NodeState::Running);
        assert_eq!(NodeState::Failed("a".into()), NodeState::Failed("a".into()));
        assert_ne!(NodeState::Failed("a".into()), NodeState::Failed("b".into()));
    }

    // ── DagExecutionState ─────────────────────────────────────────────

    #[test]
    fn dag_execution_state_variants() {
        assert_eq!(format!("{:?}", DagExecutionState::Pending), "Pending");
        assert_eq!(format!("{:?}", DagExecutionState::Running), "Running");
        assert_eq!(format!("{:?}", DagExecutionState::Completed), "Completed");
        assert_eq!(format!("{:?}", DagExecutionState::Failed), "Failed");
        assert_eq!(format!("{:?}", DagExecutionState::Cancelled), "Cancelled");
    }

    #[test]
    fn dag_execution_state_clone_copy() {
        let s = DagExecutionState::Completed;
        let c = s;
        assert_eq!(s, c);
    }

    #[test]
    fn dag_execution_state_eq() {
        assert_eq!(DagExecutionState::Pending, DagExecutionState::Pending);
        assert_ne!(DagExecutionState::Pending, DagExecutionState::Running);
    }

    // ── DagDefinition ─────────────────────────────────────────────────

    #[test]
    fn dag_definition_builder() {
        let def = DagDefinition::new()
            .add_node(DagNode::command(n("a"), "A", ok_fn()))
            .add_node(DagNode::command(n("b"), "B", ok_fn()))
            .add_edge(n("a"), n("b"));
        assert_eq!(def.node_count(), 2);
        assert_eq!(def.edge_count(), 1);
    }

    #[test]
    fn dag_definition_default() {
        let def = DagDefinition::default();
        assert!(def.nodes.is_empty());
        assert!(def.edges.is_empty());
    }

    #[test]
    fn dag_definition_debug() {
        let def = DagDefinition::new().add_node(DagNode::command(n("x"), "X", ok_fn()));
        let debug = format!("{def:?}");
        assert!(debug.contains("DagDefinition"));
    }

    #[test]
    fn dag_definition_referenced_ids() {
        let def = DagDefinition::new()
            .add_node(DagNode::command(n("a"), "A", ok_fn()))
            .add_edge(n("a"), n("b"));
        let ids = def.referenced_node_ids();
        assert!(ids.contains(&n("a")));
        assert!(ids.contains(&n("b")));
    }

    // ── DagResult ─────────────────────────────────────────────────────

    #[test]
    fn dag_result_creation() {
        let result = DagResult {
            execution_id: ExecutionId::new(),
            state: DagExecutionState::Completed,
            node_results: HashMap::new(),
            total_duration: Duration::ZERO,
            node_count: 0,
            success_count: 0,
            failure_count: 0,
        };
        assert_eq!(result.state, DagExecutionState::Completed);
    }

    #[test]
    fn dag_result_debug() {
        let result = DagResult {
            execution_id: ExecutionId::new(),
            state: DagExecutionState::Completed,
            node_results: HashMap::new(),
            total_duration: Duration::ZERO,
            node_count: 1,
            success_count: 1,
            failure_count: 0,
        };
        let debug = format!("{result:?}");
        assert!(debug.contains("Completed"));
    }

    #[test]
    fn dag_result_clone() {
        let r1 = DagResult {
            execution_id: ExecutionId::new(),
            state: DagExecutionState::Pending,
            node_results: HashMap::new(),
            total_duration: Duration::ZERO,
            node_count: 0,
            success_count: 0,
            failure_count: 0,
        };
        let r2 = r1.clone();
        assert_eq!(r1.execution_id, r2.execution_id);
    }

    // ── NodeKind Debug ────────────────────────────────────────────────

    #[test]
    fn node_kind_command_debug() {
        let kind = NodeKind::Command(Arc::from(ok_fn()));
        let debug = format!("{kind:?}");
        assert!(debug.contains("Command"));
    }

    // ── Edge Cases ────────────────────────────────────────────────────

    #[test]
    fn node_name_edge_cases() {
        assert!(DagNode::command(n("sp"), "   ", ok_fn()).validate().is_ok());
        assert!(DagNode::command(n("em"), "", ok_fn()).validate().is_err());
        assert!(DagNode::command(n("sp2"), "a b", ok_fn())
            .validate()
            .is_ok());
    }

    #[test]
    fn node_metadata_empty_tags() {
        let m = NodeMetadata::new().with_tags(Vec::new());
        assert!(m.tags.is_empty());
    }
}
