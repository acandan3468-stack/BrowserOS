/// P13 — Planner & MCP Integration Foundation
///
/// Defines trait-based orchestration contracts for future Planner
/// (LLM, MCP, Plugin, CLI) implementations.  All types are strongly
/// typed and serde-serialisable — no `Any`, no `Box<dyn ...>`,
/// no `serde_json::Value`.
///
/// # Design invariants
/// - `PlannerBridge` exposes **only** traits, no implementations
/// - `ExecutionPlan` can be produced by any future planner without
///   changing `browseros-dag`
/// - `build_dag()` converts an `ExecutionPlan` into a `DagDefinition`
///   via `NodeFactory`
/// - Zero runtime behaviour changes — no executor/scheduler/event-bus
///   modifications
/// - Reuses `exec.rs` abstractions (`ExecutionContext`, `ExecutableNode`,
///   `NodeRegistry`, `NodeFactory`, `VariableStore`)
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use browseros_types::identifiers::{CorrelationId, ExecutionId, NodeId};

use crate::exec::{ExecutionContext, ExecutionInput, NodeFactory, NodeRegistry, VariableStore};
use crate::node::DagDefinition;

// ── Phantom type impls ──────────────────────────────────────────────

fn _phantom_send_sync<T: Send + Sync>() {}

#[allow(dead_code)]
fn _planner_types_are_send_sync() {
    _phantom_send_sync::<PlannerRequest>();
    _phantom_send_sync::<PlannerResponse>();
    _phantom_send_sync::<PlanningContext>();
    _phantom_send_sync::<ExecutionPlan>();
    _phantom_send_sync::<ExecutionIntent>();
    _phantom_send_sync::<ExecutionHints>();
    _phantom_send_sync::<ExecutionConstraints>();
    _phantom_send_sync::<PlannerMetadata>();
    _phantom_send_sync::<PlanValidationResult>();
    _phantom_send_sync::<StepValidationResult>();
}

// ── Core request / response ─────────────────────────────────────────

/// Request to the planner system.
///
/// Contains a natural-language goal along with context, parameters, and
/// metadata about the requesting component.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannerRequest {
    /// Natural-language description of the goal.
    pub goal: String,
    /// Planning context (available capabilities, constraints, mode).
    pub context: PlanningContext,
    /// Additional key-value parameters (e.g. model name, temperature).
    pub parameters: HashMap<String, String>,
    /// Metadata about the component making the request.
    pub metadata: PlannerMetadata,
}

impl PlannerRequest {
    pub fn new(
        goal: impl Into<String>,
        context: PlanningContext,
        metadata: PlannerMetadata,
    ) -> Self {
        Self {
            goal: goal.into(),
            context,
            parameters: HashMap::new(),
            metadata,
        }
    }

    pub fn with_parameter(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.parameters.insert(key.into(), value.into());
        self
    }
}

/// Response from the planner system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannerResponse {
    /// The generated execution plan.
    pub plan: ExecutionPlan,
    /// Validation result for the plan.
    pub validation: PlanValidationResult,
    /// Metadata about the planner that produced this response.
    pub metadata: PlannerMetadata,
    /// How long planning took (milliseconds).
    pub duration_ms: u64,
}

impl PlannerResponse {
    pub fn new(
        plan: ExecutionPlan,
        validation: PlanValidationResult,
        metadata: PlannerMetadata,
        duration_ms: u64,
    ) -> Self {
        Self {
            plan,
            validation,
            metadata,
            duration_ms,
        }
    }
}

// ── Context ─────────────────────────────────────────────────────────

/// Context for a planning session.
///
/// Carries the list of available capabilities, global constraints, the
/// desired execution mode, and a correlation ID for tracing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanningContext {
    /// Capability names available for planning.
    pub available_capabilities: Vec<String>,
    /// Global constraints applied to all intents.
    pub global_constraints: ExecutionConstraints,
    /// Desired execution mode.
    pub execution_mode: ExecutionMode,
    /// Correlation ID for request tracing.
    pub correlation_id: CorrelationId,
}

impl PlanningContext {
    pub fn new(correlation_id: CorrelationId) -> Self {
        Self {
            available_capabilities: Vec::new(),
            global_constraints: ExecutionConstraints::default(),
            execution_mode: ExecutionMode::Sequential,
            correlation_id,
        }
    }

    pub fn with_capability(mut self, cap: impl Into<String>) -> Self {
        self.available_capabilities.push(cap.into());
        self
    }

    pub fn with_constraints(mut self, constraints: ExecutionConstraints) -> Self {
        self.global_constraints = constraints;
        self
    }

    pub fn with_mode(mut self, mode: ExecutionMode) -> Self {
        self.execution_mode = mode;
        self
    }
}

// ── Execution plan ──────────────────────────────────────────────────

/// A complete execution plan produced by a planner.
///
/// Contains an ordered list of intents, dependency edges between them,
/// initial variable bindings, and global constraints.
///
/// # DAG conversion
/// [`build_dag`](ExecutionPlan::build_dag) converts this plan into a
/// [`DagDefinition`] that the DAG engine can execute, using
/// [`NodeFactory::create_node`] to wrap each [`ExecutionIntent`] into
/// an executable [`DagNode`](crate::node::DagNode).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPlan {
    /// Unique plan execution ID.
    pub id: ExecutionId,
    /// Ordered list of intents in this plan.
    pub intents: Vec<ExecutionIntent>,
    /// Dependency edges: `(from_intent_index, to_intent_index)`.
    pub dependencies: Vec<(usize, usize)>,
    /// Initial variable bindings (overridden by resolved values).
    pub variables: HashMap<String, String>,
    /// Metadata about the planner that produced this plan.
    pub metadata: PlannerMetadata,
    /// Global constraints applied to all intents.
    pub constraints: ExecutionConstraints,
}

impl ExecutionPlan {
    pub fn new(metadata: PlannerMetadata) -> Self {
        Self {
            id: ExecutionId::new(),
            intents: Vec::new(),
            dependencies: Vec::new(),
            variables: HashMap::new(),
            metadata,
            constraints: ExecutionConstraints::default(),
        }
    }

    pub fn with_intent(mut self, intent: ExecutionIntent) -> Self {
        self.intents.push(intent);
        self
    }

    pub fn with_dependency(mut self, from: usize, to: usize) -> Self {
        self.dependencies.push((from, to));
        self
    }

    pub fn with_variable(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.variables.insert(key.into(), value.into());
        self
    }

    pub fn with_constraints(mut self, constraints: ExecutionConstraints) -> Self {
        self.constraints = constraints;
        self
    }

    /// Convert this plan into a [`DagDefinition`] the DAG engine can execute.
    ///
    /// Each intent is converted via [`NodeFactory::create_node`] with
    /// variable bindings resolved through the [`VariableStore`].
    /// Dependency edges control execution ordering.
    ///
    /// # Errors
    /// - [`PlanningError::CapabilityNotFound`] if any intent references
    ///   a capability not in the [`NodeRegistry`]
    /// - [`PlanningError::DagConstructionFailed`] if dependency indices
    ///   are out of bounds
    pub fn build_dag(
        &self,
        registry: &NodeRegistry,
        ctx: &ExecutionContext,
        store: &VariableStore,
    ) -> Result<DagDefinition, PlanningError> {
        let mut dag_def = DagDefinition::new();
        let mut node_ids: Vec<NodeId> = Vec::with_capacity(self.intents.len());

        for intent in &self.intents {
            let executable = registry
                .find(&intent.capability)
                .ok_or_else(|| PlanningError::CapabilityNotFound(intent.capability.clone()))?;

            let node_id = NodeId::from_string(&intent.id);
            let dag_node = NodeFactory::create_node(
                executable,
                node_id.clone(),
                &intent.id,
                intent.input.clone(),
                Some(store.clone()),
                ctx.clone(),
            );

            dag_def = dag_def.add_node(dag_node);
            node_ids.push(node_id);
        }

        for &(from, to) in &self.dependencies {
            if from >= node_ids.len() || to >= node_ids.len() {
                return Err(PlanningError::DagConstructionFailed(format!(
                    "dependency index out of bounds: {from} -> {to}, total nodes: {}",
                    node_ids.len()
                )));
            }
            dag_def = dag_def.add_edge(node_ids[from].clone(), node_ids[to].clone());
        }

        Ok(dag_def)
    }

    /// Validate this plan against a [`NodeRegistry`].
    ///
    /// Checks that all referenced capabilities exist, inputs are valid,
    /// and dependency indices are within bounds.
    pub fn validate(&self, registry: &NodeRegistry) -> PlanValidationResult {
        let mut step_results = Vec::with_capacity(self.intents.len());
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        for intent in &self.intents {
            let executable = registry.find(&intent.capability);
            let capability_found = executable.is_some();

            let params_valid = match &executable {
                Some(node) => node.validate(&intent.input).is_ok(),
                None => true,
            };

            let step = StepValidationResult {
                intent_id: intent.id.clone(),
                valid: capability_found && params_valid,
                capability_found,
                params_valid,
                constraints_satisfied: true,
                errors: {
                    let mut errs = Vec::new();
                    if !capability_found {
                        errs.push(format!(
                            "capability '{}' not found in registry",
                            intent.capability
                        ));
                    }
                    if !params_valid {
                        errs.push(format!(
                            "params invalid for capability '{}'",
                            intent.capability
                        ));
                    }
                    errs
                },
            };

            if !step.valid {
                errors.extend(step.errors.clone());
            }

            step_results.push(step);
        }

        for &(from, to) in &self.dependencies {
            if from >= self.intents.len() || to >= self.intents.len() {
                errors.push(format!(
                    "dependency {from} -> {to} references non-existent intent (max index: {})",
                    self.intents.len().saturating_sub(1)
                ));
            } else if from == to {
                warnings.push(format!(
                    "self-referencing dependency on intent {from} has no effect"
                ));
            }
        }

        PlanValidationResult {
            valid: errors.is_empty(),
            step_results,
            errors,
            warnings,
        }
    }
}

// ── Execution intent ────────────────────────────────────────────────

/// A single step in an execution plan.
///
/// Specifies which capability to invoke, with what input, under what
/// constraints, and where to execute.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionIntent {
    /// Unique identifier for this intent within the plan.
    pub id: String,
    /// Capability name (e.g. `"browser.navigate"`, `"dom.click"`).
    pub capability: String,
    /// Input parameters and variable bindings.
    pub input: ExecutionInput,
    /// Where this intent should be executed.
    pub target: ExecutionTarget,
    /// Execution constraints (timeout, retries, etc.).
    pub constraints: ExecutionConstraints,
    /// AI-friendly hints (complexity, duration, resource estimates).
    pub hints: ExecutionHints,
    /// Metadata about this intent.
    pub metadata: PlannerMetadata,
}

impl ExecutionIntent {
    pub fn new(
        id: impl Into<String>,
        capability: impl Into<String>,
        input: ExecutionInput,
        metadata: PlannerMetadata,
    ) -> Self {
        Self {
            id: id.into(),
            capability: capability.into(),
            input,
            target: ExecutionTarget::Local,
            constraints: ExecutionConstraints::default(),
            hints: ExecutionHints::default(),
            metadata,
        }
    }

    pub fn with_target(mut self, target: ExecutionTarget) -> Self {
        self.target = target;
        self
    }

    pub fn with_constraints(mut self, constraints: ExecutionConstraints) -> Self {
        self.constraints = constraints;
        self
    }

    pub fn with_hints(mut self, hints: ExecutionHints) -> Self {
        self.hints = hints;
        self
    }
}

// ── Execution target ────────────────────────────────────────────────

/// Where an intent should be executed.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExecutionTarget {
    /// Execute locally (default).
    #[default]
    Local,
    /// Execute on a specific remote peer.
    Remote(String),
    /// Execute on any available executor.
    Any,
}

// ── Execution mode ──────────────────────────────────────────────────

/// How a plan is executed.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExecutionMode {
    /// Execute intents one at a time.
    #[default]
    Sequential,
    /// Execute all independent intents in parallel.
    Parallel,
    /// Hybrid: at most `max_concurrent` intents in parallel.
    Hybrid { max_concurrent: usize },
}

// ── Execution priority ──────────────────────────────────────────────

/// Priority of a plan or intent.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum ExecutionPriority {
    Low,
    #[default]
    Normal,
    High,
    Critical,
}

// ── Execution hints ─────────────────────────────────────────────────

/// AI-friendly hints for execution optimisation.
///
/// Carries estimated complexity, duration, resource intensity, and tags
/// that a future Planner (LLM, rule engine, or plugin) can use to make
/// scheduling and execution decisions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionHints {
    /// Estimated complexity (1–10).
    pub estimated_complexity: Option<u32>,
    /// Estimated duration in milliseconds.
    pub estimated_duration_ms: Option<u64>,
    /// Estimated resource intensity (1–10).
    pub resource_intensity: Option<u32>,
    /// Optional retry strategy description.
    pub retry_strategy: Option<String>,
    /// Free-form tags for categorisation.
    pub tags: Vec<String>,
}

impl ExecutionHints {
    pub fn new() -> Self {
        Self {
            estimated_complexity: None,
            estimated_duration_ms: None,
            resource_intensity: None,
            retry_strategy: None,
            tags: Vec::new(),
        }
    }

    pub fn with_complexity(mut self, level: u32) -> Self {
        self.estimated_complexity = Some(level);
        self
    }

    pub fn with_duration(mut self, ms: u64) -> Self {
        self.estimated_duration_ms = Some(ms);
        self
    }

    pub fn with_intensity(mut self, level: u32) -> Self {
        self.resource_intensity = Some(level);
        self
    }

    pub fn with_retry_strategy(mut self, strategy: impl Into<String>) -> Self {
        self.retry_strategy = Some(strategy.into());
        self
    }

    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }
}

impl Default for ExecutionHints {
    fn default() -> Self {
        Self::new()
    }
}

// ── Execution constraints ───────────────────────────────────────────

/// Constraints on plan or intent execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionConstraints {
    /// Maximum execution time in milliseconds.
    pub timeout_ms: Option<u64>,
    /// Maximum number of retries on failure.
    pub max_retries: Option<u32>,
    /// Maximum parallel execution width (only used in hybrid mode).
    pub max_concurrency: Option<usize>,
    /// Capabilities that must be available.
    pub required_capabilities: Vec<String>,
    /// Resource limits (e.g. `"memory" -> "512MB"`, `"cpu" -> "2"`).
    pub resource_limits: HashMap<String, String>,
}

impl ExecutionConstraints {
    pub fn new() -> Self {
        Self {
            timeout_ms: None,
            max_retries: None,
            max_concurrency: None,
            required_capabilities: Vec::new(),
            resource_limits: HashMap::new(),
        }
    }

    pub fn with_timeout(mut self, ms: u64) -> Self {
        self.timeout_ms = Some(ms);
        self
    }

    pub fn with_max_retries(mut self, retries: u32) -> Self {
        self.max_retries = Some(retries);
        self
    }

    pub fn with_max_concurrency(mut self, n: usize) -> Self {
        self.max_concurrency = Some(n);
        self
    }

    pub fn with_required_capability(mut self, cap: impl Into<String>) -> Self {
        self.required_capabilities.push(cap.into());
        self
    }

    pub fn with_resource_limit(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.resource_limits.insert(key.into(), value.into());
        self
    }
}

impl Default for ExecutionConstraints {
    fn default() -> Self {
        Self::new()
    }
}

// ── Planner metadata ────────────────────────────────────────────────

/// Metadata about a planner component.
///
/// Identifies which planner produced a plan or response, for auditability
/// and multi-planner orchestration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannerMetadata {
    /// Human-readable planner name.
    pub name: String,
    /// Semantic version of the planner.
    pub version: String,
    /// Type of planner (LLM, MCP, Plugin, etc.).
    pub planner_type: PlannerType,
}

impl PlannerMetadata {
    pub fn new(
        name: impl Into<String>,
        version: impl Into<String>,
        planner_type: PlannerType,
    ) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            planner_type,
        }
    }
}

/// Type of planner component.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PlannerType {
    /// Large Language Model planner.
    LLM,
    /// MCP Server planner.
    MCP,
    /// Plugin-based planner.
    Plugin,
    /// Rule-engine planner.
    RuleEngine,
    /// CLI-invoked planner.
    CLI,
    /// Test planner.
    Test,
    /// Custom planner type.
    Custom(String),
}

// ── Planning result ─────────────────────────────────────────────────

/// Result of a planning operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlanningResult {
    /// Planning succeeded with a valid plan.
    Success(Box<ExecutionPlan>),
    /// Planning failed because some capabilities are unavailable.
    InsufficientCapabilities(Vec<String>),
    /// Planning produced a plan that failed validation.
    ValidationFailure(PlanValidationResult),
    /// Error constructing the DAG from the plan.
    GraphError(String),
    /// Planning was cancelled.
    Cancelled,
}

// ── Validation types ────────────────────────────────────────────────

/// Result of plan validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanValidationResult {
    /// Whether the entire plan is valid.
    pub valid: bool,
    /// Per-step validation results.
    pub step_results: Vec<StepValidationResult>,
    /// Global validation errors.
    pub errors: Vec<String>,
    /// Non-fatal warnings.
    pub warnings: Vec<String>,
}

impl PlanValidationResult {
    pub fn new() -> Self {
        Self {
            valid: true,
            step_results: Vec::new(),
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Combine multiple validation results into one.
    ///
    /// Merges step results, errors, and warnings; `valid` is true only
    /// if all inputs are valid.
    pub fn combine(results: &[PlanValidationResult]) -> Self {
        let mut combined = PlanValidationResult::new();
        combined.valid = results.iter().all(|r| r.valid);
        for r in results {
            combined.step_results.extend(r.step_results.clone());
            combined.errors.extend(r.errors.clone());
            combined.warnings.extend(r.warnings.clone());
        }
        combined
    }
}

impl Default for PlanValidationResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Validation result for a single intent / step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepValidationResult {
    /// Intent ID this result corresponds to.
    pub intent_id: String,
    /// Whether this step is valid.
    pub valid: bool,
    /// Whether the capability exists in the registry.
    pub capability_found: bool,
    /// Whether the input parameters are valid.
    pub params_valid: bool,
    /// Whether execution constraints are satisfied.
    pub constraints_satisfied: bool,
    /// Validation errors for this step.
    pub errors: Vec<String>,
}

// ── Error type ──────────────────────────────────────────────────────

/// Error type for planning operations.
#[derive(Debug, Clone)]
pub enum PlanningError {
    /// A required capability was not found in the registry.
    CapabilityNotFound(String),
    /// Plan validation failed.
    ValidationFailed(PlanValidationResult),
    /// DAG construction failed (invalid indices, etc.).
    DagConstructionFailed(String),
    /// Variable resolution failed.
    VariableResolutionFailed(String),
    /// Planning was cancelled.
    Cancelled,
}

impl std::fmt::Display for PlanningError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlanningError::CapabilityNotFound(cap) => {
                write!(f, "capability not found in registry: {cap}")
            }
            PlanningError::ValidationFailed(result) => {
                write!(
                    f,
                    "plan validation failed: {} error(s)",
                    result.errors.len()
                )
            }
            PlanningError::DagConstructionFailed(msg) => {
                write!(f, "DAG construction failed: {msg}")
            }
            PlanningError::VariableResolutionFailed(msg) => {
                write!(f, "variable resolution failed: {msg}")
            }
            PlanningError::Cancelled => write!(f, "planning cancelled"),
        }
    }
}

impl std::error::Error for PlanningError {}

// ── Bridge trait ────────────────────────────────────────────────────

/// The planner bridge trait.
///
/// Exposes only trait methods; **all** implementations (LLM, MCP, Plugin,
/// CLI, Test) are external to `browseros-dag`.
///
/// # Thread safety
/// Implementations must be [`Send`] + [`Sync`] so they can be shared across
/// DAG worker threads.
pub trait PlannerBridge: Send + Sync {
    /// Metadata about this planner component.
    fn metadata(&self) -> PlannerMetadata;

    /// Produce an execution plan for the given request.
    fn plan(&self, request: PlannerRequest) -> Result<PlanningResult, PlanningError>;

    /// Validate a plan against the available capabilities.
    fn validate_plan(&self, plan: &ExecutionPlan, registry: &NodeRegistry) -> PlanValidationResult;

    /// List goal patterns this planner supports.
    fn list_supported_goals(&self) -> Vec<String>;
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use browseros_event_bus::EventBus;
    use browseros_observability::export::StdoutSink;
    use browseros_observability::logger::{FilterDecision, LogFilter, LogRecord, Logger};
    use browseros_observability::metrics::MetricsRegistry;
    use browseros_observability::tracer::Tracer;
    use browseros_types::clock::CancellationToken;
    use browseros_types::identifiers::{CorrelationId, ExecutionId};
    use chrono::Utc;

    use crate::engine::DagEngine;
    use crate::exec::{
        CapabilityMetadata, ExecutableNode, ExecutionError, ExecutionInput, ExecutionMetadata,
        ExecutionOutput, NodeRegistry, VariableStore,
    };
    use crate::node::DagExecutionState;

    use super::*;

    // ── Test helpers ─────────────────────────────────────────────────

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

    fn test_planner_meta() -> PlannerMetadata {
        PlannerMetadata::new("test-planner", "1.0.0", PlannerType::Test)
    }

    // ── MockExecutableNode (simplified) ──────────────────────────────

    struct MockExecutable {
        capability: String,
    }

    impl MockExecutable {
        fn new(name: &str) -> Self {
            Self {
                capability: name.to_string(),
            }
        }
    }

    impl ExecutableNode for MockExecutable {
        fn capability(&self) -> &str {
            &self.capability
        }

        fn validate(&self, _input: &ExecutionInput) -> Result<(), ExecutionError> {
            Ok(())
        }

        fn execute(
            &self,
            _ctx: &ExecutionContext,
            _input: ExecutionInput,
        ) -> Result<ExecutionOutput, ExecutionError> {
            let meta = ExecutionMetadata::new(Utc::now());
            Ok(ExecutionOutput::new(meta).with_value("result", "ok"))
        }

        fn metadata(&self) -> CapabilityMetadata {
            CapabilityMetadata::new(&self.capability)
        }
    }

    // ── MockPlannerBridge ────────────────────────────────────────────

    struct MockPlanner {
        meta: PlannerMetadata,
        supported_goals: Vec<String>,
    }

    impl MockPlanner {
        fn new() -> Self {
            Self {
                meta: test_planner_meta(),
                supported_goals: vec!["test.*".into()],
            }
        }
    }

    impl PlannerBridge for MockPlanner {
        fn metadata(&self) -> PlannerMetadata {
            self.meta.clone()
        }

        fn plan(&self, request: PlannerRequest) -> Result<PlanningResult, PlanningError> {
            let intent = ExecutionIntent::new(
                "intent-1",
                "test.capability",
                request
                    .context
                    .available_capabilities
                    .first()
                    .map(|_| ExecutionInput::new().with_param("action", "test"))
                    .unwrap_or_default(),
                self.meta.clone(),
            );

            let plan = ExecutionPlan::new(self.meta.clone()).with_intent(intent);

            Ok(PlanningResult::Success(Box::new(plan)))
        }

        fn validate_plan(
            &self,
            plan: &ExecutionPlan,
            registry: &NodeRegistry,
        ) -> PlanValidationResult {
            plan.validate(registry)
        }

        fn list_supported_goals(&self) -> Vec<String> {
            self.supported_goals.clone()
        }
    }

    // ── PlannerRequest ──────────────────────────────────────────────

    #[test]
    fn planner_request_new() {
        let ctx = PlanningContext::new(CorrelationId::new());
        let meta = test_planner_meta();
        let req = PlannerRequest::new("navigate to example.com", ctx, meta);
        assert_eq!(req.goal, "navigate to example.com");
        assert!(req.parameters.is_empty());
    }

    #[test]
    fn planner_request_with_parameter() {
        let ctx = PlanningContext::new(CorrelationId::new());
        let meta = test_planner_meta();
        let req = PlannerRequest::new("test", ctx, meta).with_parameter("model", "gpt-4");
        assert_eq!(req.parameters.get("model").unwrap(), "gpt-4");
    }

    #[test]
    fn planner_request_serde_roundtrip() {
        let ctx = PlanningContext::new(CorrelationId::new());
        let meta = test_planner_meta();
        let req = PlannerRequest::new("serde test", ctx, meta);
        let json = serde_json::to_string(&req).unwrap();
        let deserialized: PlannerRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.goal, "serde test");
    }

    // ── PlannerResponse ─────────────────────────────────────────────

    #[test]
    fn planner_response_new() {
        let meta = test_planner_meta();
        let plan = ExecutionPlan::new(meta.clone());
        let validation = PlanValidationResult::new();
        let response = PlannerResponse::new(plan, validation, meta.clone(), 42);
        assert_eq!(response.metadata.name, "test-planner");
        assert_eq!(response.duration_ms, 42);
    }

    #[test]
    fn planner_response_serde_roundtrip() {
        let meta = test_planner_meta();
        let plan = ExecutionPlan::new(meta.clone());
        let validation = PlanValidationResult::new();
        let response = PlannerResponse::new(plan, validation, meta, 100);
        let json = serde_json::to_string(&response).unwrap();
        let deserialized: PlannerResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.duration_ms, 100);
    }

    // ── PlanningContext ─────────────────────────────────────────────

    #[test]
    fn planning_context_new() {
        let corr_id = CorrelationId::new();
        let ctx = PlanningContext::new(corr_id);
        assert!(ctx.available_capabilities.is_empty());
        assert_eq!(ctx.execution_mode, ExecutionMode::Sequential);
    }

    #[test]
    fn planning_context_builder() {
        let ctx = PlanningContext::new(CorrelationId::new())
            .with_capability("browser.navigate")
            .with_capability("dom.click")
            .with_mode(ExecutionMode::Parallel);
        assert!(ctx
            .available_capabilities
            .contains(&"browser.navigate".into()));
        assert_eq!(ctx.execution_mode, ExecutionMode::Parallel);
    }

    #[test]
    fn planning_context_serde() {
        let ctx = PlanningContext::new(CorrelationId::new()).with_capability("test.cap");
        let json = serde_json::to_string(&ctx).unwrap();
        let deserialized: PlanningContext = serde_json::from_str(&json).unwrap();
        assert!(deserialized
            .available_capabilities
            .contains(&"test.cap".into()));
    }

    // ── ExecutionPlan ───────────────────────────────────────────────

    #[test]
    fn execution_plan_new() {
        let meta = test_planner_meta();
        let plan = ExecutionPlan::new(meta);
        assert!(plan.intents.is_empty());
        assert!(plan.dependencies.is_empty());
    }

    #[test]
    fn execution_plan_builder() {
        let meta = test_planner_meta();
        let intent1 = ExecutionIntent::new("step-1", "cap.a", ExecutionInput::new(), meta.clone());
        let intent2 = ExecutionIntent::new("step-2", "cap.b", ExecutionInput::new(), meta.clone());
        let plan = ExecutionPlan::new(meta)
            .with_intent(intent1)
            .with_intent(intent2)
            .with_dependency(0, 1)
            .with_variable("api_key", "secret-123")
            .with_constraints(ExecutionConstraints::new().with_timeout(5000));
        assert_eq!(plan.intents.len(), 2);
        assert_eq!(plan.dependencies, vec![(0, 1)]);
        assert_eq!(plan.variables.get("api_key").unwrap(), "secret-123");
        assert_eq!(plan.constraints.timeout_ms, Some(5000));
    }

    #[test]
    fn execution_plan_validate_valid() {
        let bus = Arc::new(EventBus::new());
        let logger = noop_logger();
        let registry = NodeRegistry::new(bus, logger);
        registry.register(Arc::new(MockExecutable::new("cap.a")));

        let meta = test_planner_meta();
        let intent = ExecutionIntent::new("step-1", "cap.a", ExecutionInput::new(), meta.clone());
        let plan = ExecutionPlan::new(meta).with_intent(intent);
        let result = plan.validate(&registry);
        assert!(result.valid);
        assert!(result.step_results[0].capability_found);
        assert!(result.step_results[0].params_valid);
    }

    #[test]
    fn execution_plan_validate_missing_capability() {
        let bus = Arc::new(EventBus::new());
        let logger = noop_logger();
        let registry = NodeRegistry::new(bus, logger);

        let meta = test_planner_meta();
        let intent =
            ExecutionIntent::new("step-1", "cap.missing", ExecutionInput::new(), meta.clone());
        let plan = ExecutionPlan::new(meta).with_intent(intent);
        let result = plan.validate(&registry);
        assert!(!result.valid);
        assert!(!result.step_results[0].capability_found);
    }

    #[test]
    fn execution_plan_validate_out_of_bounds_dependency() {
        let bus = Arc::new(EventBus::new());
        let logger = noop_logger();
        let registry = NodeRegistry::new(bus, logger);

        let meta = test_planner_meta();
        let plan = ExecutionPlan::new(meta).with_dependency(0, 5);
        let result = plan.validate(&registry);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.contains("non-existent")));
    }

    #[test]
    fn execution_plan_validate_self_dependency_warning() {
        let bus = Arc::new(EventBus::new());
        let logger = noop_logger();
        let registry = NodeRegistry::new(bus, logger);
        registry.register(Arc::new(MockExecutable::new("cap.a")));

        let meta = test_planner_meta();
        let intent = ExecutionIntent::new("step-1", "cap.a", ExecutionInput::new(), meta.clone());
        let plan = ExecutionPlan::new(meta)
            .with_intent(intent)
            .with_dependency(0, 0);
        let result = plan.validate(&registry);
        assert!(!result.warnings.is_empty());
    }

    // ── ExecutionPlan.build_dag ─────────────────────────────────────

    #[test]
    fn execution_plan_build_dag_success() {
        let bus = Arc::new(EventBus::new());
        let logger = noop_logger();
        let registry = NodeRegistry::new(Arc::clone(&bus), Arc::clone(&logger));
        registry.register(Arc::new(MockExecutable::new("cap.a")));

        let meta = test_planner_meta();
        let intent = ExecutionIntent::new("node-1", "cap.a", ExecutionInput::new(), meta.clone());
        let plan = ExecutionPlan::new(meta).with_intent(intent);

        let ctx = test_ctx();
        let store = VariableStore::new();
        let dag_def = plan.build_dag(&registry, &ctx, &store);
        assert!(dag_def.is_ok());
    }

    #[test]
    fn execution_plan_build_dag_node_and_edge_counts() {
        let bus = Arc::new(EventBus::new());
        let logger = noop_logger();
        let registry = NodeRegistry::new(Arc::clone(&bus), Arc::clone(&logger));
        registry.register(Arc::new(MockExecutable::new("cap.a")));
        registry.register(Arc::new(MockExecutable::new("cap.b")));

        let meta = test_planner_meta();
        let intent_a = ExecutionIntent::new("node-a", "cap.a", ExecutionInput::new(), meta.clone());
        let intent_b = ExecutionIntent::new("node-b", "cap.b", ExecutionInput::new(), meta.clone());
        let plan = ExecutionPlan::new(meta)
            .with_intent(intent_a)
            .with_intent(intent_b)
            .with_dependency(0, 1);

        let ctx = test_ctx();
        let store = VariableStore::new();
        let dag_def = plan.build_dag(&registry, &ctx, &store).unwrap();
        assert_eq!(dag_def.node_count(), 2);
        assert_eq!(dag_def.edge_count(), 1);
    }

    #[test]
    fn execution_plan_build_dag_missing_capability() {
        let bus = Arc::new(EventBus::new());
        let logger = noop_logger();
        let registry = NodeRegistry::new(bus, logger);

        let meta = test_planner_meta();
        let intent =
            ExecutionIntent::new("node-1", "cap.missing", ExecutionInput::new(), meta.clone());
        let plan = ExecutionPlan::new(meta).with_intent(intent);

        let ctx = test_ctx();
        let store = VariableStore::new();
        let result = plan.build_dag(&registry, &ctx, &store);
        assert!(result.is_err());
        match result {
            Err(PlanningError::CapabilityNotFound(cap)) => assert_eq!(cap, "cap.missing"),
            _ => panic!("expected CapabilityNotFound"),
        }
    }

    #[test]
    fn execution_plan_build_dag_out_of_bounds_dependency() {
        let bus = Arc::new(EventBus::new());
        let logger = noop_logger();
        let registry = NodeRegistry::new(Arc::clone(&bus), Arc::clone(&logger));
        registry.register(Arc::new(MockExecutable::new("cap.a")));

        let meta = test_planner_meta();
        let intent = ExecutionIntent::new("node-1", "cap.a", ExecutionInput::new(), meta.clone());
        let plan = ExecutionPlan::new(meta)
            .with_intent(intent)
            .with_dependency(0, 5);

        let ctx = test_ctx();
        let store = VariableStore::new();
        let result = plan.build_dag(&registry, &ctx, &store);
        assert!(result.is_err());
        match result {
            Err(PlanningError::DagConstructionFailed(msg)) => {
                assert!(msg.contains("out of bounds"))
            }
            _ => panic!("expected DagConstructionFailed"),
        }
    }

    // ── ExecutionIntent ─────────────────────────────────────────────

    #[test]
    fn execution_intent_new() {
        let meta = test_planner_meta();
        let input = ExecutionInput::new().with_param("url", "https://example.com");
        let intent = ExecutionIntent::new("step-1", "browser.navigate", input, meta);
        assert_eq!(intent.id, "step-1");
        assert_eq!(intent.target, ExecutionTarget::Local);
    }

    #[test]
    fn execution_intent_builder() {
        let meta = test_planner_meta();
        let intent =
            ExecutionIntent::new("step-1", "browser.navigate", ExecutionInput::new(), meta)
                .with_target(ExecutionTarget::Any)
                .with_constraints(ExecutionConstraints::new().with_timeout(10000))
                .with_hints(ExecutionHints::new().with_complexity(5));
        assert_eq!(intent.target, ExecutionTarget::Any);
        assert_eq!(intent.constraints.timeout_ms, Some(10000));
        assert_eq!(intent.hints.estimated_complexity, Some(5));
    }

    #[test]
    fn execution_intent_serde_roundtrip() {
        let meta = test_planner_meta();
        let intent = ExecutionIntent::new("test", "cap.test", ExecutionInput::new(), meta);
        let json = serde_json::to_string(&intent).unwrap();
        let deserialized: ExecutionIntent = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, "test");
        assert_eq!(deserialized.capability, "cap.test");
    }

    // ── ExecutionTarget ─────────────────────────────────────────────

    #[test]
    fn execution_target_default_local() {
        assert_eq!(ExecutionTarget::default(), ExecutionTarget::Local);
    }

    #[test]
    fn execution_target_serde() {
        let targets = vec![
            ExecutionTarget::Local,
            ExecutionTarget::Remote("peer-1".into()),
            ExecutionTarget::Any,
        ];
        for target in &targets {
            let json = serde_json::to_string(target).unwrap();
            let deserialized: ExecutionTarget = serde_json::from_str(&json).unwrap();
            assert_eq!(&deserialized, target);
        }
    }

    // ── ExecutionMode ───────────────────────────────────────────────

    #[test]
    fn execution_mode_default_sequential() {
        assert_eq!(ExecutionMode::default(), ExecutionMode::Sequential);
    }

    #[test]
    fn execution_mode_serde() {
        let modes = vec![
            ExecutionMode::Sequential,
            ExecutionMode::Parallel,
            ExecutionMode::Hybrid { max_concurrent: 4 },
        ];
        for mode in &modes {
            let json = serde_json::to_string(mode).unwrap();
            let deserialized: ExecutionMode = serde_json::from_str(&json).unwrap();
            assert_eq!(&deserialized, mode);
        }
    }

    // ── ExecutionPriority ───────────────────────────────────────────

    #[test]
    fn execution_priority_default() {
        assert_eq!(ExecutionPriority::default(), ExecutionPriority::Normal);
    }

    #[test]
    fn execution_priority_ordering() {
        assert!(ExecutionPriority::Low < ExecutionPriority::Normal);
        assert!(ExecutionPriority::Normal < ExecutionPriority::High);
        assert!(ExecutionPriority::High < ExecutionPriority::Critical);
    }

    #[test]
    fn execution_priority_serde() {
        let json = serde_json::to_string(&ExecutionPriority::Critical).unwrap();
        let deserialized: ExecutionPriority = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, ExecutionPriority::Critical);
    }

    // ── ExecutionHints ──────────────────────────────────────────────

    #[test]
    fn execution_hints_new() {
        let hints = ExecutionHints::new();
        assert!(hints.estimated_complexity.is_none());
        assert!(hints.tags.is_empty());
    }

    #[test]
    fn execution_hints_builder() {
        let hints = ExecutionHints::new()
            .with_complexity(7)
            .with_duration(5000)
            .with_intensity(3)
            .with_retry_strategy("exponential-backoff")
            .with_tag("critical");
        assert_eq!(hints.estimated_complexity, Some(7));
        assert_eq!(hints.estimated_duration_ms, Some(5000));
        assert_eq!(hints.resource_intensity, Some(3));
        assert_eq!(hints.retry_strategy.unwrap(), "exponential-backoff");
        assert_eq!(hints.tags, vec!["critical"]);
    }

    #[test]
    fn execution_hints_serde() {
        let hints = ExecutionHints::new().with_complexity(5);
        let json = serde_json::to_string(&hints).unwrap();
        let deserialized: ExecutionHints = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.estimated_complexity, Some(5));
    }

    // ── ExecutionConstraints ────────────────────────────────────────

    #[test]
    fn execution_constraints_new() {
        let c = ExecutionConstraints::new();
        assert!(c.timeout_ms.is_none());
        assert!(c.required_capabilities.is_empty());
    }

    #[test]
    fn execution_constraints_builder() {
        let c = ExecutionConstraints::new()
            .with_timeout(30000)
            .with_max_retries(3)
            .with_max_concurrency(2)
            .with_required_capability("browser.navigate")
            .with_resource_limit("memory", "512MB");
        assert_eq!(c.timeout_ms, Some(30000));
        assert_eq!(c.max_retries, Some(3));
        assert_eq!(c.max_concurrency, Some(2));
        assert!(c.required_capabilities.contains(&"browser.navigate".into()));
        assert_eq!(c.resource_limits.get("memory").unwrap(), "512MB");
    }

    #[test]
    fn execution_constraints_serde() {
        let c = ExecutionConstraints::new().with_timeout(5000);
        let json = serde_json::to_string(&c).unwrap();
        let deserialized: ExecutionConstraints = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.timeout_ms, Some(5000));
    }

    // ── PlannerMetadata ─────────────────────────────────────────────

    #[test]
    fn planner_metadata_new() {
        let meta = PlannerMetadata::new("my-planner", "0.1.0", PlannerType::LLM);
        assert_eq!(meta.name, "my-planner");
        assert_eq!(meta.planner_type, PlannerType::LLM);
    }

    #[test]
    fn planner_metadata_serde() {
        let meta = PlannerMetadata::new("p", "1.0", PlannerType::MCP);
        let json = serde_json::to_string(&meta).unwrap();
        let deserialized: PlannerMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.planner_type, PlannerType::MCP);
    }

    // ── PlannerType ─────────────────────────────────────────────────

    #[test]
    fn planner_type_serde_all_variants() {
        let types = vec![
            PlannerType::LLM,
            PlannerType::MCP,
            PlannerType::Plugin,
            PlannerType::RuleEngine,
            PlannerType::CLI,
            PlannerType::Test,
            PlannerType::Custom("hybrid".into()),
        ];
        for pt in &types {
            let json = serde_json::to_string(pt).unwrap();
            let deserialized: PlannerType = serde_json::from_str(&json).unwrap();
            assert_eq!(&deserialized, pt);
        }
    }

    // ── PlanningResult ──────────────────────────────────────────────

    #[test]
    fn planning_result_serde_all_variants() {
        let meta = test_planner_meta();

        let success = PlanningResult::Success(Box::new(ExecutionPlan::new(meta.clone())));
        let json = serde_json::to_string(&success).unwrap();
        let deserialized: PlanningResult = serde_json::from_str(&json).unwrap();
        match deserialized {
            PlanningResult::Success(_) => {}
            _ => panic!("expected Success"),
        }

        let insufficient =
            PlanningResult::InsufficientCapabilities(vec!["browser.navigate".into()]);
        let json = serde_json::to_string(&insufficient).unwrap();
        let deserialized: PlanningResult = serde_json::from_str(&json).unwrap();
        match deserialized {
            PlanningResult::InsufficientCapabilities(caps) => {
                assert_eq!(caps, vec!["browser.navigate"])
            }
            _ => panic!("expected InsufficientCapabilities"),
        }

        let cancelled = PlanningResult::Cancelled;
        let json = serde_json::to_string(&cancelled).unwrap();
        let deserialized: PlanningResult = serde_json::from_str(&json).unwrap();
        match deserialized {
            PlanningResult::Cancelled => {}
            _ => panic!("expected Cancelled"),
        }
    }

    // ── PlanValidationResult ────────────────────────────────────────

    #[test]
    fn plan_validation_result_new() {
        let result = PlanValidationResult::new();
        assert!(result.valid);
        assert!(result.step_results.is_empty());
    }

    #[test]
    fn plan_validation_result_combine() {
        let mut r1 = PlanValidationResult::new();
        r1.valid = false;
        r1.errors.push("err1".into());

        let r2 = PlanValidationResult::new();

        let combined = PlanValidationResult::combine(&[r1, r2]);
        assert!(!combined.valid);
        assert_eq!(combined.errors, vec!["err1"]);
    }

    #[test]
    fn plan_validation_result_serde() {
        let mut result = PlanValidationResult::new();
        result.errors.push("test error".into());
        result.valid = false;
        let json = serde_json::to_string(&result).unwrap();
        let deserialized: PlanValidationResult = serde_json::from_str(&json).unwrap();
        assert!(!deserialized.valid);
        assert_eq!(deserialized.errors, vec!["test error"]);
    }

    // ── StepValidationResult ────────────────────────────────────────

    #[test]
    fn step_validation_result_serde() {
        let step = StepValidationResult {
            intent_id: "step-1".into(),
            valid: true,
            capability_found: true,
            params_valid: true,
            constraints_satisfied: true,
            errors: Vec::new(),
        };
        let json = serde_json::to_string(&step).unwrap();
        let deserialized: StepValidationResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.intent_id, "step-1");
        assert!(deserialized.valid);
    }

    // ── PlanningError ───────────────────────────────────────────────

    #[test]
    fn planning_error_display() {
        assert_eq!(
            PlanningError::CapabilityNotFound("browser.navigate".into()).to_string(),
            "capability not found in registry: browser.navigate"
        );
        assert_eq!(
            PlanningError::DagConstructionFailed("bad index".into()).to_string(),
            "DAG construction failed: bad index"
        );
        assert_eq!(PlanningError::Cancelled.to_string(), "planning cancelled");
    }

    #[test]
    fn planning_error_is_error() {
        fn assert_error<E: std::error::Error>() {}
        assert_error::<PlanningError>();
    }

    // ── PlannerBridge trait ─────────────────────────────────────────

    #[test]
    fn planner_bridge_is_object_safe() {
        fn assert_object_safe(_: &dyn PlannerBridge) {}
        let planner = MockPlanner::new();
        assert_object_safe(&planner);
    }

    #[test]
    fn planner_bridge_metadata() {
        let planner = MockPlanner::new();
        let meta = planner.metadata();
        assert_eq!(meta.name, "test-planner");
        assert_eq!(meta.planner_type, PlannerType::Test);
    }

    #[test]
    fn planner_bridge_plan() {
        let planner = MockPlanner::new();
        let ctx = PlanningContext::new(CorrelationId::new()).with_capability("test.capability");
        let request = PlannerRequest::new("test goal", ctx, test_planner_meta());
        let result = planner.plan(request).unwrap();
        match result {
            PlanningResult::Success(plan) => {
                assert_eq!(plan.intents.len(), 1);
                assert_eq!(plan.intents[0].id, "intent-1");
            }
            _ => panic!("expected success"),
        }
    }

    #[test]
    fn planner_bridge_validate_plan() {
        let bus = Arc::new(EventBus::new());
        let logger = noop_logger();
        let registry = NodeRegistry::new(bus, logger);
        registry.register(Arc::new(MockExecutable::new("cap.a")));

        let planner = MockPlanner::new();
        let meta = test_planner_meta();
        let intent = ExecutionIntent::new("step-1", "cap.a", ExecutionInput::new(), meta.clone());
        let plan = ExecutionPlan::new(meta).with_intent(intent);
        let result = planner.validate_plan(&plan, &registry);
        assert!(result.valid);
    }

    #[test]
    fn planner_bridge_list_supported_goals() {
        let planner = MockPlanner::new();
        let goals = planner.list_supported_goals();
        assert!(goals.contains(&"test.*".into()));
    }

    // ── Send + Sync ─────────────────────────────────────────────────

    #[test]
    fn planner_types_are_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<PlannerRequest>();
        assert_sync::<PlannerRequest>();
        assert_send::<PlannerResponse>();
        assert_sync::<PlannerResponse>();
        assert_send::<ExecutionPlan>();
        assert_sync::<ExecutionPlan>();
        assert_send::<ExecutionIntent>();
        assert_sync::<ExecutionIntent>();
        assert_send::<ExecutionHints>();
        assert_sync::<ExecutionHints>();
        assert_send::<ExecutionConstraints>();
        assert_sync::<ExecutionConstraints>();
        assert_send::<PlannerMetadata>();
        assert_sync::<PlannerMetadata>();
        assert_send::<PlanValidationResult>();
        assert_sync::<PlanValidationResult>();
        assert_send::<MockPlanner>();
        assert_sync::<MockPlanner>();
    }

    // ── Integration: Planner -> build_dag -> DagEngine ──────────────

    #[test]
    fn planner_to_dag_engine_integration() {
        let bus = Arc::new(EventBus::new());
        let logger = noop_logger();
        let registry = NodeRegistry::new(Arc::clone(&bus), Arc::clone(&logger));
        registry.register(Arc::new(MockExecutable::new("cap.a")));

        let meta = test_planner_meta();
        let intent = ExecutionIntent::new("node-1", "cap.a", ExecutionInput::new(), meta.clone());
        let plan = ExecutionPlan::new(meta).with_intent(intent);

        let ctx = test_ctx();
        let store = VariableStore::new();
        let dag_def = plan.build_dag(&registry, &ctx, &store).unwrap();

        assert_eq!(dag_def.node_count(), 1);

        let engine = DagEngine::new(
            bus.as_ref().clone(),
            Logger::new(Arc::new(StdoutSink::new()), Arc::new(NoopFilter), "test"),
            MetricsRegistry::new(),
            Tracer::new(),
        );

        let executable = registry.find("cap.a").unwrap();
        let dag_node = NodeFactory::create_node(
            executable,
            NodeId::from_string("integration-node"),
            "integration-node",
            ExecutionInput::new(),
            Some(store),
            ctx,
        );
        engine.register_node(dag_node).unwrap();
        let result = engine
            .execute(&[NodeId::from_string("integration-node")])
            .unwrap();
        assert_eq!(result.state, DagExecutionState::Completed);
    }

    // ── Edge cases ──────────────────────────────────────────────────

    #[test]
    fn empty_plan_is_valid() {
        let meta = test_planner_meta();
        let plan = ExecutionPlan::new(meta);
        let bus = Arc::new(EventBus::new());
        let logger = noop_logger();
        let registry = NodeRegistry::new(bus, logger);
        let result = plan.validate(&registry);
        assert!(result.valid);
    }

    #[test]
    fn empty_plan_build_dag_succeeds() {
        let meta = test_planner_meta();
        let plan = ExecutionPlan::new(meta);
        let bus = Arc::new(EventBus::new());
        let logger = noop_logger();
        let registry = NodeRegistry::new(bus, logger);
        let ctx = test_ctx();
        let store = VariableStore::new();
        let dag_def = plan.build_dag(&registry, &ctx, &store).unwrap();
        assert_eq!(dag_def.node_count(), 0);
        assert_eq!(dag_def.edge_count(), 0);
    }

    #[test]
    fn plan_validation_result_default_is_valid() {
        let result = PlanValidationResult::default();
        assert!(result.valid);
    }

    #[test]
    fn execution_hints_default() {
        let hints = ExecutionHints::default();
        assert!(hints.estimated_complexity.is_none());
    }

    #[test]
    fn execution_constraints_default() {
        let c = ExecutionConstraints::default();
        assert!(c.timeout_ms.is_none());
    }
}
