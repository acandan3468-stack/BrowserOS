use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use browseros_event_bus::EventBus;
use browseros_observability::logger::Logger;
use browseros_observability::metrics::MetricsRegistry;
use browseros_observability::tracer::Tracer;
use browseros_types::clock::CancellationToken;
use browseros_types::identifiers::{CausationId, CorrelationId, ExecutionId, NodeId};

use crate::error::DagError;
use crate::node::DagNode;

/// Lightweight context passed to executable nodes.
///
/// Does NOT own [`RuntimeContext`](crate::RuntimeContext) — borrows
/// observability and identity through [`Arc`] references and copyable IDs.
///
/// # Thread safety
/// `ExecutionContext` is [`Send`] + [`Sync`] + [`Clone`].  All fields are
/// either [`Arc`]-wrapped (shareable) or primitive/id types.
#[derive(Clone)]
pub struct ExecutionContext {
    pub event_bus: Arc<EventBus>,
    pub logger: Arc<Logger>,
    pub metrics: Arc<MetricsRegistry>,
    pub tracer: Arc<Tracer>,
    pub cancellation_token: CancellationToken,
    pub correlation_id: CorrelationId,
    pub causation_id: Option<CausationId>,
    pub execution_id: ExecutionId,
}

impl std::fmt::Debug for ExecutionContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecutionContext")
            .field("correlation_id", &self.correlation_id)
            .field("causation_id", &self.causation_id)
            .field("execution_id", &self.execution_id)
            .finish_non_exhaustive()
    }
}

/// Serializable input to an executable node.
///
/// Contains typed parameters and runtime-resolved variable bindings.
/// Both maps are flat key-value strings; structured data is serialised
/// into the value (e.g. JSON-encoded).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionInput {
    pub params: HashMap<String, String>,
    pub variables: HashMap<String, String>,
}

impl ExecutionInput {
    pub fn new() -> Self {
        Self {
            params: HashMap::new(),
            variables: HashMap::new(),
        }
    }

    /// Insert a parameter.
    pub fn with_param(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.params.insert(key.into(), value.into());
        self
    }

    /// Insert a variable binding.
    pub fn with_variable(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.variables.insert(key.into(), value.into());
        self
    }

    /// Resolve variable placeholders against a [`VariableStore`].
    ///
    /// Any value whose key exists in the store is replaced with the store's
    /// current value.  Unknown keys are left as-is.
    pub fn resolve(&self, store: &VariableStore) -> Self {
        let mut resolved = self.clone();
        for key in self.variables.keys() {
            if let Some(value) = store.resolve(key) {
                resolved.variables.insert(key.clone(), value);
            }
        }
        resolved
    }
}

impl Default for ExecutionInput {
    fn default() -> Self {
        Self::new()
    }
}

/// Serializable output from an executable node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionOutput {
    pub values: HashMap<String, String>,
    pub metadata: ExecutionMetadata,
}

impl ExecutionOutput {
    pub fn new(metadata: ExecutionMetadata) -> Self {
        Self {
            values: HashMap::new(),
            metadata,
        }
    }

    /// Insert an output value.
    pub fn with_value(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.values.insert(key.into(), value.into());
        self
    }
}

/// Errors that can occur during execution abstraction.
#[derive(Debug, Clone)]
pub enum ExecutionError {
    ValidationFailed(String),
    ExecutionFailed(String),
    VariableNotFound(String),
    Cancelled,
    Timeout,
}

impl std::fmt::Display for ExecutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecutionError::ValidationFailed(msg) => write!(f, "validation failed: {msg}"),
            ExecutionError::ExecutionFailed(msg) => write!(f, "execution failed: {msg}"),
            ExecutionError::VariableNotFound(key) => write!(f, "variable not found: {key}"),
            ExecutionError::Cancelled => write!(f, "execution cancelled"),
            ExecutionError::Timeout => write!(f, "execution timed out"),
        }
    }
}

impl std::error::Error for ExecutionError {}

/// Metadata about a single node execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionMetadata {
    pub started_at: DateTime<Utc>,
    pub duration_ms: u64,
    pub retry_attempt: u32,
    pub node_id: Option<String>,
    pub node_name: Option<String>,
}

impl ExecutionMetadata {
    pub fn new(started_at: DateTime<Utc>) -> Self {
        Self {
            started_at,
            duration_ms: 0,
            retry_attempt: 0,
            node_id: None,
            node_name: None,
        }
    }
}

/// Declared capabilities of an executable node.
///
/// Used by the NodeRegistry to advertise which operations are available,
/// and by the future MCP Server and Planner to discover capabilities.
///
/// Extended in P13 with `version` and `constraints` fields for Planner
/// capability negotiation (RULES 5.1.5, 5.2.2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityMetadata {
    pub name: String,
    pub description: String,
    pub required_params: Vec<String>,
    pub optional_params: Vec<String>,
    pub output_keys: Vec<String>,
    pub tags: Vec<String>,
    pub timeout_ms: Option<u64>,
    pub retryable: bool,
    /// Semantic version of this capability (P13+).
    pub version: Option<String>,
    /// Implementation-specific constraints (P13+).
    pub constraints: HashMap<String, String>,
}

impl CapabilityMetadata {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            required_params: Vec::new(),
            optional_params: Vec::new(),
            output_keys: Vec::new(),
            tags: Vec::new(),
            timeout_ms: None,
            retryable: false,
            version: None,
            constraints: HashMap::new(),
        }
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    pub fn with_required_param(mut self, param: impl Into<String>) -> Self {
        self.required_params.push(param.into());
        self
    }

    pub fn with_optional_param(mut self, param: impl Into<String>) -> Self {
        self.optional_params.push(param.into());
        self
    }

    pub fn with_output_key(mut self, key: impl Into<String>) -> Self {
        self.output_keys.push(key.into());
        self
    }

    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    pub fn with_timeout(mut self, ms: u64) -> Self {
        self.timeout_ms = Some(ms);
        self
    }

    pub fn with_retryable(mut self) -> Self {
        self.retryable = true;
        self
    }

    /// Set the semantic version of this capability.
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// Add a named constraint (e.g. `"memory" -> "512MB"`, `"concurrent" -> "4"`).
    pub fn with_constraint(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.constraints.insert(key.into(), value.into());
        self
    }
}

/// An executable node that can be registered and executed by the DAG engine.
///
/// # Thread safety
/// Implementations must be [`Send`] + [`Sync`] + `'static` so they can be
/// shared across DAG worker threads and stored in [`NodeRegistry`].
///
/// # Plugin readiness
/// The Plugin System will register new capabilities by implementing this
/// trait and inserting instances into [`NodeRegistry`].
pub trait ExecutableNode: Send + Sync + 'static {
    /// Unique capability name (e.g. `"browser.navigate"`, `"dom.click"`).
    fn capability(&self) -> &str;

    /// Validate the input before execution.
    fn validate(&self, input: &ExecutionInput) -> Result<(), ExecutionError>;

    /// Execute this node with the given context and input.
    fn execute(
        &self,
        ctx: &ExecutionContext,
        input: ExecutionInput,
    ) -> Result<ExecutionOutput, ExecutionError>;

    /// Metadata about this node's capabilities.
    fn metadata(&self) -> CapabilityMetadata;
}

/// Thread-safe registry of executable node capabilities.
///
/// The Plugin System registers new capabilities via [`register`](NodeRegistry::register).
/// The future MCP Server discovers available capabilities via
/// [`list_capabilities`](NodeRegistry::list_capabilities).
/// The Planner generates executable DAG nodes via [`NodeFactory`].
#[derive(Clone)]
pub struct NodeRegistry {
    inner: Arc<RwLock<HashMap<String, Arc<dyn ExecutableNode>>>>,
    event_bus: Arc<EventBus>,
    logger: Arc<Logger>,
}

impl std::fmt::Debug for NodeRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeRegistry")
            .field("capability_count", &self.capability_count())
            .finish()
    }
}

impl NodeRegistry {
    pub fn new(event_bus: Arc<EventBus>, logger: Arc<Logger>) -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
            event_bus,
            logger,
        }
    }

    /// Register an executable node by its capability name.
    ///
    /// Returns `true` if the capability was newly inserted, `false` if an
    /// existing entry was overwritten.
    pub fn register(&self, node: Arc<dyn ExecutableNode>) -> bool {
        let cap = node.capability().to_string();
        let mut map = self.inner.write().expect("NodeRegistry lock poisoned");
        map.insert(cap, node).is_some()
    }

    /// Find an executable node by capability name.
    pub fn find(&self, capability: &str) -> Option<Arc<dyn ExecutableNode>> {
        let map = self.inner.read().expect("NodeRegistry lock poisoned");
        map.get(capability).cloned()
    }

    /// List all registered capability names.
    pub fn list_capabilities(&self) -> Vec<String> {
        let map = self.inner.read().expect("NodeRegistry lock poisoned");
        let mut caps: Vec<String> = map.keys().cloned().collect();
        caps.sort();
        caps
    }

    /// List all registered capability metadata (for MCP discovery).
    pub fn list_capability_metadata(&self) -> Vec<CapabilityMetadata> {
        let map = self.inner.read().expect("NodeRegistry lock poisoned");
        let mut meta: Vec<CapabilityMetadata> = map.values().map(|n| n.metadata()).collect();
        meta.sort_by(|a, b| a.name.cmp(&b.name));
        meta
    }

    /// Number of registered capabilities.
    pub fn capability_count(&self) -> usize {
        let map = self.inner.read().expect("NodeRegistry lock poisoned");
        map.len()
    }

    /// Reference to the event bus (used by NodeFactory).
    pub fn event_bus(&self) -> &Arc<EventBus> {
        &self.event_bus
    }

    /// Reference to the logger (used by NodeFactory).
    pub fn logger(&self) -> &Arc<Logger> {
        &self.logger
    }
}

/// Thread-safe variable store for binding and resolving runtime values.
///
/// Each node in a DAG execution can read variables bound by upstream nodes
/// and write its own outputs.  The store uses interior mutability so that
/// parallel nodes in the same topological layer can write concurrently
/// (writes to different keys do not conflict).
#[derive(Clone, Default)]
pub struct VariableStore {
    inner: Arc<RwLock<HashMap<String, String>>>,
}

impl std::fmt::Debug for VariableStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let snapshot = self.snapshot();
        f.debug_struct("VariableStore")
            .field("count", &snapshot.len())
            .finish()
    }
}

impl VariableStore {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Bind a variable (insert or overwrite).
    pub fn bind(&self, key: impl Into<String>, value: impl Into<String>) {
        let mut map = self.inner.write().expect("VariableStore lock poisoned");
        map.insert(key.into(), value.into());
    }

    /// Resolve a variable by key.
    pub fn resolve(&self, key: &str) -> Option<String> {
        let map = self.inner.read().expect("VariableStore lock poisoned");
        map.get(key).cloned()
    }

    /// Resolve multiple variables, collecting only those present.
    pub fn resolve_all(&self, keys: &[String]) -> HashMap<String, String> {
        let map = self.inner.read().expect("VariableStore lock poisoned");
        keys.iter()
            .filter_map(|k| map.get(k).map(|v| (k.clone(), v.clone())))
            .collect()
    }

    /// Take an atomic snapshot of all current bindings.
    pub fn snapshot(&self) -> HashMap<String, String> {
        let map = self.inner.read().expect("VariableStore lock poisoned");
        map.clone()
    }

    /// Merge all output values from an [`ExecutionOutput`] into the store.
    pub fn bind_output(&self, output: &ExecutionOutput) {
        let mut map = self.inner.write().expect("VariableStore lock poisoned");
        for (k, v) in &output.values {
            map.insert(k.clone(), v.clone());
        }
    }
}

/// Factory that converts [`ExecutableNode`] instances into [`DagNode`]
/// instances that the DAG engine can execute.
///
/// Each created [`DagNode`] wraps a call to
/// [`ExecutableNode::validate`] followed by
/// [`ExecutableNode::execute`], resolving variables from the
/// [`VariableStore`] at execution time.
pub struct NodeFactory;

impl NodeFactory {
    /// Create a [`DagNode`] from an executable node, input, and context.
    ///
    /// The returned `DagNode`, when executed by the DAG engine:
    /// 1. Resolves variable placeholders in `input` from the `variable_store`
    /// 2. Calls `executable.validate(&resolved_input)`
    /// 3. Calls `executable.execute(&ctx, resolved_input)`
    /// 4. Binds output values back into the `variable_store`
    ///
    /// # Errors
    /// Any validation or execution error is mapped to
    /// [`DagError::ExecutionFailed`].
    pub fn create_node(
        executable: Arc<dyn ExecutableNode>,
        node_id: NodeId,
        name: impl Into<String>,
        input: ExecutionInput,
        variable_store: Option<VariableStore>,
        ctx: ExecutionContext,
    ) -> DagNode {
        let name: String = name.into();

        DagNode::command(
            node_id.clone(),
            name.clone(),
            Box::new(move || -> Result<(), DagError> {
                // Resolve variables
                let resolved_input = match variable_store {
                    Some(ref store) => input.resolve(store),
                    None => input.clone(),
                };

                // Validate before execution
                executable
                    .validate(&resolved_input)
                    .map_err(|e| DagError::ExecutionFailed {
                        node_id: node_id.clone(),
                        reason: e.to_string(),
                    })?;

                // Execute
                let output = executable.execute(&ctx, resolved_input).map_err(|e| {
                    DagError::ExecutionFailed {
                        node_id: node_id.clone(),
                        reason: e.to_string(),
                    }
                })?;

                // Bind output values to the variable store
                if let Some(ref store) = variable_store {
                    store.bind_output(&output);
                }

                Ok(())
            }),
        )
    }
}

/// Propagates execution context between DAG steps.
///
/// The propagator creates a child [`ExecutionContext`] for a downstream
/// node, setting the parent's execution ID as the child's causation ID
/// and inheriting the correlation ID.
pub struct ContextPropagator;

impl ContextPropagator {
    /// Create a child execution context from a parent.
    ///
    /// The child inherits:
    /// - `correlation_id` from the parent (unchanged chain)
    /// - `causation_id` set to `Some(parent.execution_id)`
    /// - A fresh `execution_id`
    /// - All observable references (`event_bus`, `logger`, `metrics`, `tracer`)
    /// - A fresh `cancellation_token`
    pub fn propagate(parent: &ExecutionContext) -> ExecutionContext {
        ExecutionContext {
            event_bus: Arc::clone(&parent.event_bus),
            logger: Arc::clone(&parent.logger),
            metrics: Arc::clone(&parent.metrics),
            tracer: Arc::clone(&parent.tracer),
            cancellation_token: CancellationToken::new(),
            correlation_id: parent.correlation_id,
            causation_id: Some(CausationId::from_uuid(*parent.execution_id.as_uuid())),
            execution_id: ExecutionId::new(),
        }
    }

    /// Create a child context with an explicit causation override.
    pub fn propagate_with_causation(
        parent: &ExecutionContext,
        causation: CausationId,
    ) -> ExecutionContext {
        ExecutionContext {
            event_bus: Arc::clone(&parent.event_bus),
            logger: Arc::clone(&parent.logger),
            metrics: Arc::clone(&parent.metrics),
            tracer: Arc::clone(&parent.tracer),
            cancellation_token: CancellationToken::new(),
            correlation_id: parent.correlation_id,
            causation_id: Some(causation),
            execution_id: ExecutionId::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use browseros_event_bus::EventBus;
    use browseros_observability::export::StdoutSink;
    use browseros_observability::logger::{FilterDecision, LogFilter, LogRecord, Logger};
    use browseros_observability::metrics::MetricsRegistry;
    use browseros_observability::tracer::Tracer;

    use crate::engine::DagEngine;
    use crate::node::{DagExecutionState, DagNode};

    use super::*;

    struct NoopFilter;
    impl LogFilter for NoopFilter {
        fn should_log(&self, _record: &LogRecord) -> FilterDecision {
            FilterDecision::Reject
        }
    }

    fn noop_logger() -> Arc<Logger> {
        Arc::new(Logger::new(
            Arc::new(StdoutSink::new()),
            Arc::new(NoopFilter),
            "test",
        ))
    }

    fn test_ctx() -> ExecutionContext {
        ExecutionContext {
            event_bus: Arc::new(EventBus::new()),
            logger: noop_logger(),
            metrics: Arc::new(MetricsRegistry::new()),
            tracer: Arc::new(Tracer::new()),
            cancellation_token: CancellationToken::new(),
            correlation_id: CorrelationId::new(),
            causation_id: None,
            execution_id: ExecutionId::new(),
        }
    }

    // ── Mock ExecutableNode ───────────────────────────────────────────

    struct MockNode {
        capability: String,
        should_fail: AtomicBool,
        metadata: CapabilityMetadata,
    }

    impl MockNode {
        fn new(name: &str) -> Self {
            Self {
                capability: name.to_string(),
                should_fail: AtomicBool::new(false),
                metadata: CapabilityMetadata::new(name),
            }
        }

        fn with_failure(mut self) -> Self {
            self.should_fail = AtomicBool::new(true);
            self
        }
    }

    impl ExecutableNode for MockNode {
        fn capability(&self) -> &str {
            &self.capability
        }

        fn validate(&self, input: &ExecutionInput) -> Result<(), ExecutionError> {
            if input.params.contains_key("invalid") {
                return Err(ExecutionError::ValidationFailed(
                    "invalid param present".into(),
                ));
            }
            Ok(())
        }

        fn execute(
            &self,
            _ctx: &ExecutionContext,
            _input: ExecutionInput,
        ) -> Result<ExecutionOutput, ExecutionError> {
            if self.should_fail.load(Ordering::SeqCst) {
                return Err(ExecutionError::ExecutionFailed("mock node failed".into()));
            }
            let meta = ExecutionMetadata::new(Utc::now());
            Ok(ExecutionOutput::new(meta).with_value("result", "ok"))
        }

        fn metadata(&self) -> CapabilityMetadata {
            self.metadata.clone()
        }
    }

    // ── ExecutionContext ──────────────────────────────────────────────

    #[test]
    fn execution_context_is_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<ExecutionContext>();
        assert_sync::<ExecutionContext>();
    }

    #[test]
    fn execution_context_clone() {
        let ctx = test_ctx();
        let cloned = ctx.clone();
        assert_eq!(
            ctx.correlation_id.as_uuid(),
            cloned.correlation_id.as_uuid()
        );
        assert_eq!(ctx.execution_id.as_uuid(), cloned.execution_id.as_uuid());
    }

    #[test]
    fn execution_context_debug_redacts_internals() {
        let ctx = test_ctx();
        let debug = format!("{ctx:?}");
        assert!(debug.contains("correlation_id"));
        assert!(!debug.contains("event_bus"));
    }

    // ── ExecutionInput ────────────────────────────────────────────────

    #[test]
    fn execution_input_default() {
        let input = ExecutionInput::default();
        assert!(input.params.is_empty());
        assert!(input.variables.is_empty());
    }

    #[test]
    fn execution_input_with_param() {
        let input = ExecutionInput::new()
            .with_param("url", "https://example.com")
            .with_param("timeout", "30");
        assert_eq!(input.params.get("url").unwrap(), "https://example.com");
        assert_eq!(input.params.get("timeout").unwrap(), "30");
    }

    #[test]
    fn execution_input_serde_roundtrip() {
        let input = ExecutionInput::new()
            .with_param("url", "https://example.com")
            .with_variable("output", "{{result}}");
        let json = serde_json::to_string(&input).unwrap();
        let deserialized: ExecutionInput = serde_json::from_str(&json).unwrap();
        assert_eq!(
            deserialized.params.get("url").unwrap(),
            "https://example.com"
        );
        assert_eq!(deserialized.variables.get("output").unwrap(), "{{result}}");
    }

    // ── ExecutionOutput ───────────────────────────────────────────────

    #[test]
    fn execution_output_with_values() {
        let meta = ExecutionMetadata::new(Utc::now());
        let output = ExecutionOutput::new(meta)
            .with_value("title", "Example")
            .with_value("status", "200");
        assert_eq!(output.values.get("title").unwrap(), "Example");
    }

    #[test]
    fn execution_output_serde_roundtrip() {
        let meta = ExecutionMetadata::new(Utc::now());
        let output = ExecutionOutput::new(meta.clone()).with_value("result", "ok");
        let json = serde_json::to_string(&output).unwrap();
        let deserialized: ExecutionOutput = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.values.get("result").unwrap(), "ok");
    }

    // ── ExecutionError ────────────────────────────────────────────────

    #[test]
    fn execution_error_display() {
        assert_eq!(
            ExecutionError::ValidationFailed("bad input".into()).to_string(),
            "validation failed: bad input"
        );
        assert_eq!(
            ExecutionError::ExecutionFailed("oops".into()).to_string(),
            "execution failed: oops"
        );
        assert_eq!(
            ExecutionError::VariableNotFound("x".into()).to_string(),
            "variable not found: x"
        );
        assert_eq!(ExecutionError::Cancelled.to_string(), "execution cancelled");
        assert_eq!(ExecutionError::Timeout.to_string(), "execution timed out");
    }

    #[test]
    fn execution_error_is_error() {
        fn assert_error<E: std::error::Error>() {}
        assert_error::<ExecutionError>();
    }

    // ── CapabilityMetadata ────────────────────────────────────────────

    #[test]
    fn capability_metadata_builder() {
        let meta = CapabilityMetadata::new("dom.click")
            .with_description("Click an element")
            .with_required_param("selector")
            .with_optional_param("timeout")
            .with_output_key("success")
            .with_tag("interaction")
            .with_timeout(5000)
            .with_retryable();
        assert_eq!(meta.name, "dom.click");
        assert_eq!(meta.required_params, vec!["selector"]);
        assert!(meta.retryable);
    }

    #[test]
    fn capability_metadata_serde() {
        let meta = CapabilityMetadata::new("test.cap").with_description("desc");
        let json = serde_json::to_string(&meta).unwrap();
        let deserialized: CapabilityMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "test.cap");
    }

    // ── ExecutionMetadata ─────────────────────────────────────────────

    #[test]
    fn execution_metadata_creation() {
        let now = Utc::now();
        let meta = ExecutionMetadata::new(now);
        assert!(meta.duration_ms == 0);
        assert!(meta.retry_attempt == 0);
        assert!(meta.node_id.is_none());
    }

    // ── ExecutableNode ────────────────────────────────────────────────

    #[test]
    fn executable_node_trait_is_object_safe() {
        fn assert_object_safe(_: &dyn ExecutableNode) {}
        let node = MockNode::new("test.cap");
        assert_object_safe(&node);
    }

    #[test]
    fn executable_node_validate_passes() {
        let node = MockNode::new("test.validate");
        let input = ExecutionInput::new().with_param("url", "http://example.com");
        assert!(node.validate(&input).is_ok());
    }

    #[test]
    fn executable_node_validate_fails() {
        let node = MockNode::new("test.validate_fail");
        let input = ExecutionInput::new().with_param("invalid", "true");
        assert!(node.validate(&input).is_err());
    }

    #[test]
    fn executable_node_execute_succeeds() {
        let node = MockNode::new("test.execute_ok");
        let ctx = test_ctx();
        let input = ExecutionInput::new();
        let result = node.execute(&ctx, input);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output.values.get("result").unwrap(), "ok");
    }

    #[test]
    fn executable_node_execute_fails() {
        let node = MockNode::new("test.execute_fail").with_failure();
        let ctx = test_ctx();
        let input = ExecutionInput::new();
        let result = node.execute(&ctx, input);
        assert!(result.is_err());
        match result {
            Err(ExecutionError::ExecutionFailed(msg)) => assert!(msg.contains("mock")),
            _ => panic!("expected ExecutionFailed"),
        }
    }

    // ── NodeRegistry ──────────────────────────────────────────────────

    #[test]
    fn node_registry_register_and_find() {
        let bus = Arc::new(EventBus::new());
        let logger = noop_logger();
        let registry = NodeRegistry::new(bus, logger);

        let node = Arc::new(MockNode::new("test.cap"));
        registry.register(node);

        let found = registry.find("test.cap");
        assert!(found.is_some());
        assert_eq!(found.unwrap().capability(), "test.cap");
    }

    #[test]
    fn node_registry_find_missing_returns_none() {
        let bus = Arc::new(EventBus::new());
        let logger = noop_logger();
        let registry = NodeRegistry::new(bus, logger);
        assert!(registry.find("nonexistent").is_none());
    }

    #[test]
    fn node_registry_overwrite_returns_previous() {
        let bus = Arc::new(EventBus::new());
        let logger = noop_logger();
        let registry = NodeRegistry::new(bus, logger);

        let first = Arc::new(MockNode::new("test.overwrite"));
        let second = Arc::new(MockNode::new("test.overwrite"));

        assert!(!registry.register(first));
        assert!(registry.register(second));
    }

    #[test]
    fn node_registry_list_capabilities() {
        let bus = Arc::new(EventBus::new());
        let logger = noop_logger();
        let registry = NodeRegistry::new(bus, logger);

        registry.register(Arc::new(MockNode::new("b.b")));
        registry.register(Arc::new(MockNode::new("a.a")));

        let caps = registry.list_capabilities();
        assert_eq!(caps.len(), 2);
        assert_eq!(caps[0], "a.a");
        assert_eq!(caps[1], "b.b");
    }

    #[test]
    fn node_registry_capability_metadata() {
        let bus = Arc::new(EventBus::new());
        let logger = noop_logger();
        let registry = NodeRegistry::new(bus, logger);

        let mut meta = CapabilityMetadata::new("test.meta");
        meta.description = "a test".into();
        let node = Arc::new(MockNode::new("test.meta"));
        registry.register(node);

        let metas = registry.list_capability_metadata();
        assert_eq!(metas.len(), 1);
        assert_eq!(metas[0].name, "test.meta");
    }

    #[test]
    fn node_registry_empty_list() {
        let bus = Arc::new(EventBus::new());
        let logger = noop_logger();
        let registry = NodeRegistry::new(bus, logger);
        assert!(registry.list_capabilities().is_empty());
        assert!(registry.list_capability_metadata().is_empty());
        assert_eq!(registry.capability_count(), 0);
    }

    #[test]
    fn node_registry_thread_safe() {
        let bus = Arc::new(EventBus::new());
        let logger = noop_logger();
        let registry = NodeRegistry::new(bus, logger);

        let r1 = registry.clone();
        let r2 = registry.clone();

        std::thread::scope(|s| {
            s.spawn(|| {
                r1.register(Arc::new(MockNode::new("thread.a")));
            });
            s.spawn(|| {
                r2.register(Arc::new(MockNode::new("thread.b")));
            });
        });

        assert_eq!(registry.capability_count(), 2);
    }

    // ── VariableStore ─────────────────────────────────────────────────

    #[test]
    fn variable_store_bind_and_resolve() {
        let store = VariableStore::new();
        store.bind("url", "https://example.com");
        assert_eq!(store.resolve("url").unwrap(), "https://example.com");
    }

    #[test]
    fn variable_store_resolve_missing_returns_none() {
        let store = VariableStore::new();
        assert!(store.resolve("nonexistent").is_none());
    }

    #[test]
    fn variable_store_resolve_all() {
        let store = VariableStore::new();
        store.bind("a", "1");
        store.bind("b", "2");
        store.bind("c", "3");
        let keys = vec!["a".into(), "c".into(), "missing".into()];
        let resolved = store.resolve_all(&keys);
        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved.get("a").unwrap(), "1");
        assert_eq!(resolved.get("c").unwrap(), "3");
    }

    #[test]
    fn variable_store_snapshot() {
        let store = VariableStore::new();
        store.bind("x", "10");
        store.bind("y", "20");
        let snap = store.snapshot();
        assert_eq!(snap.len(), 2);
        assert_eq!(snap.get("x").unwrap(), "10");
    }

    #[test]
    fn variable_store_bind_output() {
        let store = VariableStore::new();
        let meta = ExecutionMetadata::new(Utc::now());
        let output = ExecutionOutput::new(meta)
            .with_value("result", "success")
            .with_value("id", "42");
        store.bind_output(&output);
        assert_eq!(store.resolve("result").unwrap(), "success");
        assert_eq!(store.resolve("id").unwrap(), "42");
    }

    #[test]
    fn variable_store_overwrite() {
        let store = VariableStore::new();
        store.bind("key", "old");
        assert_eq!(store.resolve("key").unwrap(), "old");
        store.bind("key", "new");
        assert_eq!(store.resolve("key").unwrap(), "new");
    }

    #[test]
    fn variable_store_thread_safe() {
        let store = VariableStore::new();
        let s1 = store.clone();
        let s2 = store.clone();

        std::thread::scope(|s| {
            s.spawn(|| {
                s1.bind("a", "1");
            });
            s.spawn(|| {
                s2.bind("b", "2");
            });
        });

        let snap = store.snapshot();
        assert_eq!(snap.len(), 2);
    }

    // ── ContextPropagator ─────────────────────────────────────────────

    #[test]
    fn context_propagator_creates_child() {
        let parent = test_ctx();
        let child = ContextPropagator::propagate(&parent);

        // Correlation ID is inherited
        assert_eq!(
            parent.correlation_id.as_uuid(),
            child.correlation_id.as_uuid()
        );

        // Causation ID is set from parent execution ID
        assert!(child.causation_id.is_some());

        // Execution ID is different
        assert_ne!(parent.execution_id.as_uuid(), child.execution_id.as_uuid());

        // Fresh cancellation token (not cancelled)
        assert!(!child.cancellation_token.is_cancelled());
    }

    #[test]
    fn context_propagator_with_causation() {
        let parent = test_ctx();
        let cause = CausationId::new();
        let child = ContextPropagator::propagate_with_causation(&parent, cause);

        assert_eq!(child.causation_id, Some(cause));
        assert_eq!(
            parent.correlation_id.as_uuid(),
            child.correlation_id.as_uuid()
        );
    }

    // ── NodeFactory ───────────────────────────────────────────────────

    #[test]
    fn node_factory_creates_executable_dag_node() {
        let executable = Arc::new(MockNode::new("test.factory"));
        let ctx = test_ctx();
        let store = VariableStore::new();
        let input = ExecutionInput::new().with_param("url", "http://example.com");

        let dag_node = NodeFactory::create_node(
            executable,
            NodeId::from_string("factory-node"),
            "Factory Test",
            input,
            Some(store),
            ctx,
        );

        assert_eq!(dag_node.id, NodeId::from_string("factory-node"));
        assert_eq!(dag_node.name, "Factory Test");
    }

    #[test]
    fn node_factory_execute_via_dag_engine_succeeds() {
        let bus = Arc::new(EventBus::new());
        let logger = noop_logger();
        let _registry = NodeRegistry::new(Arc::clone(&bus), Arc::clone(&logger));

        let executable: Arc<dyn ExecutableNode> = Arc::new(MockNode::new("test.dag_exec"));

        let ctx = ExecutionContext {
            event_bus: Arc::clone(&bus),
            logger,
            metrics: Arc::new(MetricsRegistry::new()),
            tracer: Arc::new(Tracer::new()),
            cancellation_token: CancellationToken::new(),
            correlation_id: CorrelationId::new(),
            causation_id: None,
            execution_id: ExecutionId::new(),
        };

        let store = VariableStore::new();
        let input = ExecutionInput::new().with_param("url", "http://example.com");

        let dag_node = NodeFactory::create_node(
            executable,
            NodeId::from_string("dag-exec"),
            "DAG Execute",
            input,
            Some(store),
            ctx,
        );

        let engine = DagEngine::new(
            bus.as_ref().clone(),
            Logger::new(Arc::new(StdoutSink::new()), Arc::new(NoopFilter), "test"),
            MetricsRegistry::new(),
            Tracer::new(),
        );

        engine.register_node(dag_node).unwrap();
        let result = engine.execute(&[NodeId::from_string("dag-exec")]).unwrap();
        assert_eq!(result.state, DagExecutionState::Completed);
        assert_eq!(result.success_count, 1);
    }

    #[test]
    fn node_factory_validation_failure_propagates() {
        let executable: Arc<dyn ExecutableNode> = Arc::new(MockNode::new("test.val_fail"));

        let ctx = test_ctx();
        let store = VariableStore::new();
        let input = ExecutionInput::new().with_param("invalid", "true");

        let dag_node = NodeFactory::create_node(
            executable,
            NodeId::from_string("val-fail"),
            "Validation Fail",
            input,
            Some(store),
            ctx,
        );

        let bus = EventBus::new();
        let engine = DagEngine::new(
            bus,
            Logger::new(Arc::new(StdoutSink::new()), Arc::new(NoopFilter), "test"),
            MetricsRegistry::new(),
            Tracer::new(),
        );

        engine.register_node(dag_node).unwrap();
        let result = engine.execute(&[NodeId::from_string("val-fail")]).unwrap();
        assert_eq!(result.state, DagExecutionState::Failed);
        assert_eq!(result.failure_count, 1);
    }

    #[test]
    fn node_factory_execution_error_propagates() {
        let executable: Arc<dyn ExecutableNode> =
            Arc::new(MockNode::new("test.exec_fail").with_failure());

        let ctx = test_ctx();
        let input = ExecutionInput::new();

        let dag_node = NodeFactory::create_node(
            executable,
            NodeId::from_string("exec-fail"),
            "Exec Fail",
            input,
            None,
            ctx,
        );

        let bus = EventBus::new();
        let engine = DagEngine::new(
            bus,
            Logger::new(Arc::new(StdoutSink::new()), Arc::new(NoopFilter), "test"),
            MetricsRegistry::new(),
            Tracer::new(),
        );

        engine.register_node(dag_node).unwrap();
        let result = engine.execute(&[NodeId::from_string("exec-fail")]).unwrap();
        assert_eq!(result.state, DagExecutionState::Failed);
    }

    #[test]
    fn node_factory_variable_binding_at_execution() {
        let executable: Arc<dyn ExecutableNode> = Arc::new(MockNode::new("test.variable_bind"));

        let ctx = test_ctx();
        let store = VariableStore::new();
        store.bind("dynamic_url", "https://resolved.example.com");

        let input = ExecutionInput::new()
            .with_param("url", "{{dynamic_url}}")
            .with_variable("url", "dynamic_url");

        let dag_node = NodeFactory::create_node(
            executable,
            NodeId::from_string("var-bind"),
            "Variable Binding",
            input,
            Some(store.clone()),
            ctx,
        );

        let bus = EventBus::new();
        let engine = DagEngine::new(
            bus,
            Logger::new(Arc::new(StdoutSink::new()), Arc::new(NoopFilter), "test"),
            MetricsRegistry::new(),
            Tracer::new(),
        );

        engine.register_node(dag_node).unwrap();
        let result = engine.execute(&[NodeId::from_string("var-bind")]).unwrap();
        assert_eq!(result.state, DagExecutionState::Completed);
    }

    // ── ExecutionInput.resolve() ──────────────────────────────────────

    #[test]
    fn execution_input_resolve_replaces_variables() {
        let store = VariableStore::new();
        store.bind("api_key", "secret-123");

        let input = ExecutionInput::new()
            .with_param("endpoint", "https://api.example.com")
            .with_variable("api_key", "api_key");

        let resolved = input.resolve(&store);
        assert_eq!(
            resolved.params.get("endpoint").unwrap(),
            "https://api.example.com"
        );
        assert_eq!(resolved.variables.get("api_key").unwrap(), "secret-123");
    }

    #[test]
    fn execution_input_resolve_unknown_key_unchanged() {
        let store = VariableStore::new();
        let input = ExecutionInput::new().with_variable("missing", "unknown_key");

        let resolved = input.resolve(&store);
        // Unknown keys keep their original variable name as fallback
        assert_eq!(resolved.variables.get("missing").unwrap(), "unknown_key");
    }

    // ── Edge cases ────────────────────────────────────────────────────

    #[test]
    fn empty_execution_input_serde() {
        let input = ExecutionInput::new();
        let json = serde_json::to_string(&input).unwrap();
        let deserialized: ExecutionInput = serde_json::from_str(&json).unwrap();
        assert!(deserialized.params.is_empty());
        assert!(deserialized.variables.is_empty());
    }

    #[test]
    fn variable_store_default_is_empty() {
        let store: VariableStore = Default::default();
        assert!(store.snapshot().is_empty());
    }

    #[test]
    fn node_factory_dag_node_is_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<DagNode>();
        assert_sync::<DagNode>();
    }

    #[test]
    fn node_registry_is_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<NodeRegistry>();
        assert_sync::<NodeRegistry>();
    }

    #[test]
    fn variable_store_is_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<VariableStore>();
        assert_sync::<VariableStore>();
    }

    // ── Integration: Registry + Factory + DAG Engine ──────────────────

    #[test]
    fn full_integration_registry_factory_engine() {
        let bus = Arc::new(EventBus::new());
        let logger = noop_logger();
        let registry = NodeRegistry::new(Arc::clone(&bus), Arc::clone(&logger));

        let executable: Arc<dyn ExecutableNode> = Arc::new(MockNode::new("test.full_integration"));
        registry.register(Arc::clone(&executable));

        // Find from registry
        let found = registry.find("test.full_integration").unwrap();
        assert_eq!(found.capability(), "test.full_integration");

        // Create DAG node from registered capability
        let ctx = ExecutionContext {
            event_bus: Arc::clone(&bus),
            logger,
            metrics: Arc::new(MetricsRegistry::new()),
            tracer: Arc::new(Tracer::new()),
            cancellation_token: CancellationToken::new(),
            correlation_id: CorrelationId::new(),
            causation_id: None,
            execution_id: ExecutionId::new(),
        };

        let dag_node = NodeFactory::create_node(
            found,
            NodeId::from_string("integration-node"),
            "Integration Test",
            ExecutionInput::new().with_param("action", "test"),
            None,
            ctx,
        );

        let engine = DagEngine::new(
            bus.as_ref().clone(),
            Logger::new(Arc::new(StdoutSink::new()), Arc::new(NoopFilter), "test"),
            MetricsRegistry::new(),
            Tracer::new(),
        );

        // Execute through DAG engine
        engine.register_node(dag_node).unwrap();
        let result = engine
            .execute(&[NodeId::from_string("integration-node")])
            .unwrap();
        assert_eq!(result.state, DagExecutionState::Completed);
    }

    // ── MCP Readiness: capability discovery ──────────────────────────

    #[test]
    fn mcp_readiness_capability_discovery() {
        let bus = Arc::new(EventBus::new());
        let logger = noop_logger();
        let registry = NodeRegistry::new(bus, logger);

        registry.register(Arc::new(MockNode::new("browser.navigate")));
        registry.register(Arc::new(MockNode::new("dom.click")));
        registry.register(Arc::new(MockNode::new("network.request")));

        let caps = registry.list_capabilities();
        assert!(caps.contains(&"browser.navigate".to_string()));
        assert!(caps.contains(&"dom.click".to_string()));
        assert!(caps.contains(&"network.request".to_string()));
        assert_eq!(caps.len(), 3);
    }

    // ── Plugin Readiness: register after engine creation ─────────────

    #[test]
    fn plugin_readiness_register_after_engine_creation() {
        let bus = Arc::new(EventBus::new());
        let logger = noop_logger();
        let registry = NodeRegistry::new(Arc::clone(&bus), Arc::clone(&logger));

        // Create engine first
        let engine = DagEngine::new(
            bus.as_ref().clone(),
            Logger::new(Arc::new(StdoutSink::new()), Arc::new(NoopFilter), "test"),
            MetricsRegistry::new(),
            Tracer::new(),
        );

        // Plugin registers a capability after engine creation
        let plugin_node: Arc<dyn ExecutableNode> = Arc::new(MockNode::new("plugin.capability"));
        let cap = plugin_node.capability().to_string();
        registry.register(plugin_node);

        // Lookup and create a node from the plugin-registered capability
        let found = registry.find(&cap).unwrap();
        let ctx = ExecutionContext {
            event_bus: Arc::clone(&bus),
            logger,
            metrics: Arc::new(MetricsRegistry::new()),
            tracer: Arc::new(Tracer::new()),
            cancellation_token: CancellationToken::new(),
            correlation_id: CorrelationId::new(),
            causation_id: None,
            execution_id: ExecutionId::new(),
        };

        let dag_node = NodeFactory::create_node(
            found,
            NodeId::from_string("plugin-node"),
            "Plugin Node",
            ExecutionInput::new(),
            None,
            ctx,
        );

        engine.register_node(dag_node).unwrap();
        let result = engine
            .execute(&[NodeId::from_string("plugin-node")])
            .unwrap();
        assert_eq!(result.state, DagExecutionState::Completed);
    }
}
