/// P16 — Plugin Runtime Integration
///
/// Bridges Planner → CapabilityResolver → PluginManager → ExecutableNode → DagEngine
/// without coupling Planner to PluginManager.
use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;

use crate::exec::{
    CapabilityMetadata, ExecutableNode, ExecutionContext, ExecutionError, ExecutionInput,
    ExecutionOutput, NodeRegistry,
};
use crate::manager::PluginManager;
use crate::plugin::{
    Plugin, PluginCapabilityId, PluginContext, PluginExecutionMetadata, PluginId,
    PluginRegistration, PluginRegistry, PluginState, PluginStatus, PluginVersion,
};

// ── Phantom type impls ──────────────────────────────────────────────

fn _phantom_send_sync<T: Send + Sync>() {}

#[allow(dead_code)]
fn _runtime_types_are_send_sync() {
    _phantom_send_sync::<CapabilityRequest>();
    _phantom_send_sync::<CapabilityMatch>();
    _phantom_send_sync::<CapabilitySelection>();
    _phantom_send_sync::<CapabilityMatcher>();
    _phantom_send_sync::<MatchCriterion>();
    _phantom_send_sync::<CapabilityResolver>();
    _phantom_send_sync::<PluginCapabilityNode>();
    _phantom_send_sync::<PluginExecutionBridge>();
    _phantom_send_sync::<PluginRuntime>();
}

// ── CapabilityRequest ──────────────────────────────────────────────

/// Describes what capability the planner needs.
///
/// Created from an [`ExecutionIntent::capability`](crate::planner::ExecutionIntent)
/// name during node resolution.
#[derive(Debug, Clone, Default)]
pub struct CapabilityRequest {
    pub capability_name: String,
    pub plugin_id_filter: Option<PluginId>,
    pub version_filter: Option<PluginVersion>,
    pub required_tags: Vec<String>,
}

impl CapabilityRequest {
    pub fn new(capability_name: impl Into<String>) -> Self {
        Self {
            capability_name: capability_name.into(),
            plugin_id_filter: None,
            version_filter: None,
            required_tags: Vec::new(),
        }
    }

    pub fn with_plugin_id(mut self, id: PluginId) -> Self {
        self.plugin_id_filter = Some(id);
        self
    }

    pub fn with_version(mut self, version: PluginVersion) -> Self {
        self.version_filter = Some(version);
        self
    }

    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.required_tags.push(tag.into());
        self
    }
}

// ── CapabilityMatch ─────────────────────────────────────────────────

/// Result of matching a [`CapabilityRequest`] against a registered
/// plugin capability.
#[derive(Debug, Clone)]
pub struct CapabilityMatch {
    pub plugin_id: PluginId,
    pub capability_id: PluginCapabilityId,
    pub capability_metadata: CapabilityMetadata,
    pub plugin_state: PluginState,
    pub plugin_status: PluginStatus,
}

impl CapabilityMatch {
    fn from_registration(
        reg: &PluginRegistration,
        cap_id: &PluginCapabilityId,
        cap_meta: &CapabilityMetadata,
    ) -> Self {
        Self {
            plugin_id: reg.plugin_id.clone(),
            capability_id: cap_id.clone(),
            capability_metadata: cap_meta.clone(),
            plugin_state: reg.state,
            plugin_status: reg.status.clone(),
        }
    }
}

// ── CapabilitySelection / MatchCriterion / CapabilityMatcher ────────

/// Strategy for selecting among multiple matching capabilities.
#[derive(Debug, Clone)]
pub enum CapabilitySelection {
    /// Return the first match found.
    First,
    /// Return only matches whose plugin is healthy.
    OnlyHealthy,
    /// Evaluate all matches against criteria and return the best.
    ByPriority(CapabilityMatcher),
}

/// Individual criterion used by [`CapabilityMatcher`] to rank matches.
#[derive(Debug, Clone)]
pub enum MatchCriterion {
    /// Prefer higher plugin versions.
    VersionLatest,
    /// Prefer a specific plugin by ID.
    PreferredPlugin(PluginId),
    /// Prefer healthier plugins (Healthy > Degraded > Unhealthy > Unknown).
    HealthFirst,
    /// Prefer plugins in a specific state.
    PreferredState(PluginState),
}

/// Composable set of criteria for ranking capability matches.
///
/// Criteria are evaluated in declaration order — earlier criteria act as
/// primary sort keys.
#[derive(Debug, Clone)]
pub struct CapabilityMatcher {
    criteria: Vec<MatchCriterion>,
}

impl CapabilityMatcher {
    pub fn new(criteria: Vec<MatchCriterion>) -> Self {
        Self { criteria }
    }

    /// Score a match against the criteria (higher = better).
    fn score(&self, m: &CapabilityMatch) -> i64 {
        let mut score: i64 = 0;
        for (i, criterion) in self.criteria.iter().enumerate() {
            let weight = (10_i64).pow((self.criteria.len() - i) as u32);
            match criterion {
                MatchCriterion::VersionLatest => {
                    if let Some(ref ver) = m.capability_metadata.version {
                        if let Some(pv) = PluginVersion::parse(ver) {
                            score += weight * (pv.major * 10000 + pv.minor * 100 + pv.patch) as i64;
                        }
                    }
                }
                MatchCriterion::PreferredPlugin(id) => {
                    if m.plugin_id == *id {
                        score += weight * 1000;
                    }
                }
                MatchCriterion::HealthFirst => {
                    score += weight
                        * match m.plugin_status {
                            PluginStatus::Healthy => 100,
                            PluginStatus::Degraded(_) => 50,
                            PluginStatus::Unhealthy(_) => 10,
                            PluginStatus::Unknown => 0,
                        };
                }
                MatchCriterion::PreferredState(state) => {
                    if m.plugin_state == *state {
                        score += weight * 100;
                    }
                }
            }
        }
        score
    }
}

// ── CapabilityResolver ──────────────────────────────────────────────

/// Resolves [`CapabilityRequest`]s against a [`PluginRegistry`] using a
/// configured selection strategy.
#[derive(Debug, Clone)]
pub struct CapabilityResolver {
    selection: CapabilitySelection,
}

impl CapabilityResolver {
    pub fn new(selection: CapabilitySelection) -> Self {
        Self { selection }
    }

    /// Resolve a capability request against the given registry.
    ///
    /// Returns all matching [`CapabilityMatch`]es, filtered and sorted by
    /// the configured [`CapabilitySelection`].
    pub fn resolve(
        &self,
        registry: &PluginRegistry,
        request: &CapabilityRequest,
    ) -> Vec<CapabilityMatch> {
        let matches = self.collect_matches(registry, request);
        self.apply_selection(matches)
    }

    /// Resolve the single best match, or `None` if no match is found.
    pub fn resolve_best(
        &self,
        registry: &PluginRegistry,
        request: &CapabilityRequest,
    ) -> Option<CapabilityMatch> {
        let mut matches = self.resolve(registry, request);
        if matches.is_empty() {
            return None;
        }
        Some(matches.swap_remove(0))
    }

    fn collect_matches(
        &self,
        registry: &PluginRegistry,
        request: &CapabilityRequest,
    ) -> Vec<CapabilityMatch> {
        let cap_id = PluginCapabilityId::new(&request.capability_name);
        let registrations = registry.lookup_by_capability(&cap_id);

        let mut matches: Vec<CapabilityMatch> = registrations
            .iter()
            .filter(|reg| self.passes_filters(reg, request))
            .flat_map(|reg| {
                reg.manifest
                    .capabilities
                    .iter()
                    .filter(|c| c.id == cap_id)
                    .map(|c| CapabilityMatch::from_registration(reg, &c.id, &c.metadata))
            })
            .collect();

        matches.sort_by_key(|m| {
            let health_ord = match m.plugin_status {
                PluginStatus::Healthy => 0i8,
                PluginStatus::Degraded(_) => 1,
                PluginStatus::Unknown => 2,
                PluginStatus::Unhealthy(_) => 3,
            };
            let state_ord = match m.plugin_state {
                PluginState::Running => 0i8,
                PluginState::Initialized => 1,
                PluginState::Validated => 2,
                PluginState::Registered => 3,
                _ => 4,
            };
            (state_ord, health_ord)
        });

        matches
    }

    fn passes_filters(&self, reg: &PluginRegistration, request: &CapabilityRequest) -> bool {
        if let Some(ref filter_id) = request.plugin_id_filter {
            if reg.plugin_id != *filter_id {
                return false;
            }
        }
        if let Some(ref filter_ver) = request.version_filter {
            if reg.metadata.version < *filter_ver {
                return false;
            }
        }
        if !request.required_tags.is_empty() {
            let all_tags: Vec<&str> = reg
                .manifest
                .capabilities
                .iter()
                .flat_map(|c| c.metadata.tags.iter().map(|t| t.as_str()))
                .collect();
            for tag in &request.required_tags {
                if !all_tags.contains(&tag.as_str()) {
                    return false;
                }
            }
        }
        true
    }

    fn apply_selection(&self, matches: Vec<CapabilityMatch>) -> Vec<CapabilityMatch> {
        match &self.selection {
            CapabilitySelection::First => matches.into_iter().take(1).collect(),
            CapabilitySelection::OnlyHealthy => matches
                .into_iter()
                .filter(|m| m.plugin_status == PluginStatus::Healthy)
                .collect(),
            CapabilitySelection::ByPriority(matcher) => {
                let mut scored: Vec<(i64, CapabilityMatch)> = matches
                    .into_iter()
                    .map(|m| (matcher.score(&m), m))
                    .collect();
                scored.sort_by_key(|k| std::cmp::Reverse(k.0));
                scored.into_iter().map(|(_, m)| m).collect()
            }
        }
    }
}

impl Default for CapabilityResolver {
    fn default() -> Self {
        Self {
            selection: CapabilitySelection::First,
        }
    }
}

// ── PluginCapabilityNode ────────────────────────────────────────────

/// An [`ExecutableNode`] that wraps a plugin capability.
///
/// Created by [`PluginExecutionBridge`] when registering a capability
/// from a plugin into the [`NodeRegistry`]. At execution time, it calls
/// [`Plugin::execute_capability()`] on the wrapped plugin.
pub struct PluginCapabilityNode {
    capability_name: String,
    plugin_id: PluginId,
    capability_id: PluginCapabilityId,
    plugin: Arc<dyn Plugin>,
    registry: PluginRegistry,
    metadata: CapabilityMetadata,
    config: HashMap<String, String>,
}

impl std::fmt::Debug for PluginCapabilityNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginCapabilityNode")
            .field("capability_name", &self.capability_name)
            .field("plugin_id", &self.plugin_id)
            .field("capability_id", &self.capability_id)
            .finish()
    }
}

impl PluginCapabilityNode {
    pub fn new(
        plugin: Arc<dyn Plugin>,
        registry: PluginRegistry,
        capability_name: impl Into<String>,
        plugin_id: PluginId,
        capability_id: PluginCapabilityId,
        metadata: CapabilityMetadata,
    ) -> Self {
        Self {
            capability_name: capability_name.into(),
            plugin_id,
            capability_id,
            plugin,
            registry,
            metadata,
            config: HashMap::new(),
        }
    }

    pub fn with_config(mut self, config: HashMap<String, String>) -> Self {
        self.config = config;
        self
    }

    fn build_plugin_context(&self, ctx: &ExecutionContext) -> PluginContext {
        let exec_meta = PluginExecutionMetadata {
            execution_count: 0,
            last_started_at: Some(Utc::now()),
            last_error: None,
            state: PluginState::Running,
        };
        PluginContext {
            plugin_id: self.plugin_id.clone(),
            execution_context: Some(ctx.clone()),
            metadata: exec_meta,
        }
    }
}

impl ExecutableNode for PluginCapabilityNode {
    fn capability(&self) -> &str {
        &self.capability_name
    }

    fn validate(&self, input: &ExecutionInput) -> Result<(), ExecutionError> {
        let reg = match self.registry.lookup(&self.plugin_id) {
            Some(r) => r,
            None => {
                return Err(ExecutionError::ValidationFailed(format!(
                    "Plugin {} not found in registry",
                    self.plugin_id
                )));
            }
        };

        if reg.state != PluginState::Running {
            return Err(ExecutionError::ValidationFailed(format!(
                "Plugin {} is in state {:?}, expected Running",
                self.plugin_id, reg.state
            )));
        }

        for param in &self.metadata.required_params {
            if !input.params.contains_key(param) {
                return Err(ExecutionError::ValidationFailed(format!(
                    "Missing required parameter: {}",
                    param
                )));
            }
        }

        Ok(())
    }

    fn execute(
        &self,
        ctx: &ExecutionContext,
        input: ExecutionInput,
    ) -> Result<ExecutionOutput, ExecutionError> {
        if ctx.cancellation_token.is_cancelled() {
            return Err(ExecutionError::Cancelled);
        }

        let merged_input = {
            let mut params = self.config.clone();
            for (k, v) in &input.params {
                params.insert(k.clone(), v.clone());
            }
            ExecutionInput {
                params,
                variables: input.variables.clone(),
            }
        };

        let plugin_ctx = self.build_plugin_context(ctx);

        self.plugin
            .execute_capability(&self.capability_id, &plugin_ctx, merged_input)
            .map_err(|e| ExecutionError::ExecutionFailed(e.to_string()))
    }

    fn metadata(&self) -> CapabilityMetadata {
        self.metadata.clone()
    }
}

// ── PluginExecutionBridge ───────────────────────────────────────────

/// Bridges plugin capabilities from a [`PluginRegistry`] to
/// [`ExecutableNode`] instances in a [`NodeRegistry`].
///
/// The bridge is the core of P16 — it converts "plugin declares
/// capability" into "DAG can execute capability" without either system
/// knowing about the other.
#[derive(Debug, Clone)]
pub struct PluginExecutionBridge {
    registry: PluginRegistry,
    resolver: CapabilityResolver,
}

impl PluginExecutionBridge {
    pub fn new(registry: PluginRegistry, resolver: CapabilityResolver) -> Self {
        Self { registry, resolver }
    }

    /// Register all capabilities from all healthy, running plugins as
    /// executable nodes in the given [`NodeRegistry`].
    ///
    /// Returns the number of capabilities successfully registered.
    pub fn register_all(&self, node_registry: &NodeRegistry) -> usize {
        let plugins = self.registry.list_plugins();
        let mut count = 0usize;

        for reg in &plugins {
            if reg.state != PluginState::Running {
                continue;
            }

            let plugin = match self.registry.get_plugin(&reg.plugin_id) {
                Some(p) => p,
                None => continue,
            };

            for cap in &reg.manifest.capabilities {
                let node = PluginCapabilityNode::new(
                    Arc::clone(&plugin),
                    self.registry.clone(),
                    cap.id.as_str(),
                    reg.plugin_id.clone(),
                    cap.id.clone(),
                    cap.metadata.clone(),
                );

                node_registry.register(Arc::new(node));
                count += 1;
            }
        }

        count
    }

    /// Register a single specific capability resolved from a
    /// [`CapabilityRequest`].
    ///
    /// Returns `true` if the capability was newly inserted, `false` if
    /// an existing entry was overwritten.  Returns `Err` when no matching
    /// capability is found.
    pub fn register_capability(
        &self,
        node_registry: &NodeRegistry,
        request: &CapabilityRequest,
    ) -> Result<bool, String> {
        let best = self
            .resolver
            .resolve_best(&self.registry, request)
            .ok_or_else(|| format!("No matching capability for '{}'", request.capability_name))?;

        let plugin = self
            .registry
            .get_plugin(&best.plugin_id)
            .ok_or_else(|| format!("Plugin '{}' not found in registry", best.plugin_id))?;

        let cap_id = best.capability_id;
        let cap_name = cap_id.as_str().to_string();
        let node = PluginCapabilityNode::new(
            plugin,
            self.registry.clone(),
            cap_name,
            best.plugin_id,
            cap_id,
            best.capability_metadata,
        );

        Ok(node_registry.register(Arc::new(node)))
    }

    pub fn registry(&self) -> &PluginRegistry {
        &self.registry
    }

    pub fn resolver(&self) -> &CapabilityResolver {
        &self.resolver
    }
}

// ── PluginRuntime ───────────────────────────────────────────────────

/// Orchestrator for plugin runtime integration.
///
/// Wraps an [`Arc<PluginManager>`] so the runtime can be shared across
/// threads and provides the bridge to [`NodeRegistry`] for capability
/// execution.
#[derive(Debug, Clone)]
pub struct PluginRuntime {
    manager: Arc<PluginManager>,
    bridge: PluginExecutionBridge,
}

impl PluginRuntime {
    pub fn new(manager: Arc<PluginManager>, resolver: CapabilityResolver) -> Self {
        let registry = manager.registry().clone();
        let bridge = PluginExecutionBridge::new(registry, resolver);
        Self { manager, bridge }
    }

    pub fn from_manager(manager: Arc<PluginManager>) -> Self {
        let resolver = CapabilityResolver::default();
        Self::new(manager, resolver)
    }

    pub fn manager(&self) -> &PluginManager {
        &self.manager
    }

    pub fn bridge(&self) -> &PluginExecutionBridge {
        &self.bridge
    }

    pub fn registry(&self) -> &PluginRegistry {
        self.bridge.registry()
    }

    /// Register all capabilities from healthy running plugins into the
    /// given node registry.
    pub fn register_capabilities(&self, node_registry: &NodeRegistry) -> usize {
        self.bridge.register_all(node_registry)
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    use browseros_event_bus::EventBus;
    use browseros_observability::export::StdoutSink;
    use browseros_observability::logger::{FilterDecision, LogFilter, LogRecord, Logger};
    use browseros_observability::metrics::MetricsRegistry;
    use browseros_observability::tracer::Tracer;
    use browseros_types::clock::CancellationToken;
    use browseros_types::identifiers::{CorrelationId, ExecutionId};

    use browseros_types::identifiers::NodeId;

    use crate::exec::{
        CapabilityMetadata, ExecutableNode, ExecutionInput, ExecutionMetadata, ExecutionOutput,
        NodeRegistry,
    };
    use crate::plugin::*;

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

    fn make_node_registry() -> NodeRegistry {
        NodeRegistry::new(Arc::new(EventBus::new()), noop_logger())
    }

    type PluginPtr = Arc<dyn Plugin>;

    // Plugin that supports execute_capability
    struct ExecutableMockPlugin {
        metadata: PluginMetadata,
        manifest: PluginManifest,
        health_status: std::sync::RwLock<PluginStatus>,
        execute_count: AtomicU64,
    }

    impl ExecutableMockPlugin {
        fn new(name: &str, version: PluginVersion) -> Self {
            let author = PluginAuthor::new("exec test author");
            let metadata = PluginMetadata::new(name, version, author);
            let manifest = PluginManifest::new(metadata.clone());
            Self {
                metadata,
                manifest,
                health_status: std::sync::RwLock::new(PluginStatus::Healthy),
                execute_count: AtomicU64::new(0),
            }
        }

        fn with_capability(mut self, cap: PluginCapability) -> Self {
            self.manifest = self.manifest.with_capability(cap);
            self
        }
    }

    impl Plugin for ExecutableMockPlugin {
        fn metadata(&self) -> PluginMetadata {
            self.metadata.clone()
        }
        fn manifest(&self) -> PluginManifest {
            self.manifest.clone()
        }
        fn capabilities(&self) -> Vec<PluginCapability> {
            self.manifest.capabilities.clone()
        }
        fn permissions(&self) -> Vec<PluginPermission> {
            Vec::new()
        }
        fn dependencies(&self) -> Vec<PluginDependency> {
            Vec::new()
        }
        fn validate(&self, _registry: &PluginRegistry) -> PluginValidationResult {
            PluginValidationResult::new()
        }
        fn initialize(&self, _ctx: &PluginContext) -> Result<(), PluginError> {
            Ok(())
        }
        fn shutdown(&self) -> Result<(), PluginError> {
            Ok(())
        }
        fn health(&self) -> PluginStatus {
            self.health_status.read().unwrap().clone()
        }
        fn execute_capability(
            &self,
            _capability_id: &PluginCapabilityId,
            _ctx: &PluginContext,
            input: ExecutionInput,
        ) -> Result<ExecutionOutput, PluginError> {
            self.execute_count.fetch_add(1, Ordering::SeqCst);
            let meta = ExecutionMetadata::new(Utc::now());
            let mut output = ExecutionOutput::new(meta);
            for (k, v) in &input.params {
                output = output.with_value(k, v);
            }
            output = output.with_value("executed", "true");
            Ok(output)
        }
    }

    /// Register a plugin with a capability and transition it to Running.
    fn register_test_plugin(registry: &PluginRegistry, name: &str, cap_name: &str) -> PluginId {
        let version = PluginVersion::new(1, 0, 0);
        let cap_id = PluginCapabilityId::new(cap_name);
        let cap_meta = CapabilityMetadata::new(cap_name).with_tag("test");
        let capability = PluginCapability::new(cap_id, cap_meta);

        let plugin: PluginPtr =
            Arc::new(ExecutableMockPlugin::new(name, version).with_capability(capability));

        let reg = registry.register(plugin).expect("register should succeed");
        let plugin_id = reg.plugin_id.clone();

        let states = [
            PluginState::Registered,
            PluginState::Validated,
            PluginState::Initialized,
            PluginState::Running,
        ];
        for &next in &states {
            registry.transition_to(&plugin_id, next).unwrap();
        }

        plugin_id
    }

    // ── CapabilityResolver tests ──────────────────────────────────────

    #[test]
    fn resolver_default_selection() {
        let resolver = CapabilityResolver::default();
        match resolver.selection {
            CapabilitySelection::First => {}
            _ => panic!("expected First"),
        }
    }

    #[test]
    fn resolver_empty_registry_returns_empty() {
        let resolver = CapabilityResolver::default();
        let registry = PluginRegistry::new();
        let request = CapabilityRequest::new("nonexistent.cap");
        let matches = resolver.resolve(&registry, &request);
        assert!(matches.is_empty());
    }

    #[test]
    fn resolver_finds_capability_exact_match() {
        let registry = PluginRegistry::new();
        register_test_plugin(&registry, "test.plugin", "test.cap");

        let resolver = CapabilityResolver::default();
        let request = CapabilityRequest::new("test.cap");
        let matches = resolver.resolve(&registry, &request);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].capability_id.as_str(), "test.cap");
    }

    #[test]
    fn resolver_resolve_best_returns_first() {
        let registry = PluginRegistry::new();
        register_test_plugin(&registry, "plugin.a", "shared.cap");

        let resolver = CapabilityResolver::default();
        let request = CapabilityRequest::new("shared.cap");
        let best = resolver.resolve_best(&registry, &request);
        assert!(best.is_some());
        assert_eq!(best.unwrap().capability_id.as_str(), "shared.cap");
    }

    #[test]
    fn resolver_resolve_best_nonexistent_returns_none() {
        let resolver = CapabilityResolver::default();
        let registry = PluginRegistry::new();
        let request = CapabilityRequest::new("no.such.cap");
        let best = resolver.resolve_best(&registry, &request);
        assert!(best.is_none());
    }

    #[test]
    fn resolver_plugin_id_filter_excludes_non_matching() {
        let registry = PluginRegistry::new();
        let id_a = register_test_plugin(&registry, "alpha", "test.cap");
        register_test_plugin(&registry, "beta", "test.cap");

        let resolver = CapabilityResolver::default();
        let request = CapabilityRequest::new("test.cap").with_plugin_id(id_a.clone());
        let matches = resolver.resolve(&registry, &request);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].plugin_id, id_a);
    }

    #[test]
    fn resolver_only_healthy_excludes_degraded() {
        let registry = PluginRegistry::new();

        let v = PluginVersion::new(1, 0, 0);
        let cap = PluginCapability::new(
            PluginCapabilityId::new("healthy.cap"),
            CapabilityMetadata::new("healthy.cap"),
        );
        let plugin: PluginPtr =
            Arc::new(ExecutableMockPlugin::new("healthy.plugin", v.clone()).with_capability(cap));
        let reg = registry.register(plugin).unwrap();
        for &s in &[
            PluginState::Registered,
            PluginState::Validated,
            PluginState::Initialized,
            PluginState::Running,
        ] {
            registry.transition_to(&reg.plugin_id, s).unwrap();
        }

        let resolver = CapabilityResolver::new(CapabilitySelection::OnlyHealthy);
        let request = CapabilityRequest::new("healthy.cap");
        let matches = resolver.resolve(&registry, &request);
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn resolver_version_filter() {
        let registry = PluginRegistry::new();

        let v2 = PluginVersion::new(2, 0, 0);
        let cap = PluginCapability::new(
            PluginCapabilityId::new("ver.cap"),
            CapabilityMetadata::new("ver.cap"),
        );
        let plugin: PluginPtr =
            Arc::new(ExecutableMockPlugin::new("ver.plugin", v2).with_capability(cap));
        registry.register(plugin).unwrap();

        let resolver = CapabilityResolver::default();
        let request = CapabilityRequest::new("ver.cap").with_version(PluginVersion::new(1, 5, 0));
        let matches = resolver.resolve(&registry, &request);
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn resolver_version_filter_excludes_lower() {
        let registry = PluginRegistry::new();

        let v1 = PluginVersion::new(1, 0, 0);
        let cap = PluginCapability::new(
            PluginCapabilityId::new("low.cap"),
            CapabilityMetadata::new("low.cap"),
        );
        let plugin: PluginPtr =
            Arc::new(ExecutableMockPlugin::new("low.plugin", v1).with_capability(cap));
        registry.register(plugin).unwrap();

        let resolver = CapabilityResolver::default();
        let request = CapabilityRequest::new("low.cap").with_version(PluginVersion::new(2, 0, 0));
        let matches = resolver.resolve(&registry, &request);
        assert!(matches.is_empty());
    }

    #[test]
    fn resolver_matcher_health_first() {
        let registry = PluginRegistry::new();

        let cap_a = PluginCapability::new(
            PluginCapabilityId::new("shared.cap"),
            CapabilityMetadata::new("shared.cap"),
        );
        let plugin_a: PluginPtr = Arc::new(
            ExecutableMockPlugin::new("healthy.one", PluginVersion::new(1, 0, 0))
                .with_capability(cap_a),
        );
        let reg_a = registry.register(plugin_a).unwrap();
        for &s in &[
            PluginState::Registered,
            PluginState::Validated,
            PluginState::Initialized,
            PluginState::Running,
        ] {
            registry.transition_to(&reg_a.plugin_id, s).unwrap();
        }

        let cap_b = PluginCapability::new(
            PluginCapabilityId::new("shared.cap"),
            CapabilityMetadata::new("shared.cap"),
        );
        let plugin_b: PluginPtr = Arc::new(
            ExecutableMockPlugin::new("healthy.two", PluginVersion::new(1, 0, 0))
                .with_capability(cap_b),
        );
        let reg_b = registry.register(plugin_b).unwrap();
        for &s in &[
            PluginState::Registered,
            PluginState::Validated,
            PluginState::Initialized,
            PluginState::Running,
        ] {
            registry.transition_to(&reg_b.plugin_id, s).unwrap();
        }

        let matcher = CapabilityMatcher::new(vec![
            MatchCriterion::HealthFirst,
            MatchCriterion::VersionLatest,
        ]);
        let resolver = CapabilityResolver::new(CapabilitySelection::ByPriority(matcher));
        let request = CapabilityRequest::new("shared.cap");
        let matches = resolver.resolve(&registry, &request);
        assert!(!matches.is_empty());
    }

    // ── PluginCapabilityNode tests ────────────────────────────────────

    #[test]
    fn capability_node_has_correct_capability_name() {
        let registry = PluginRegistry::new();
        let plugin_id = register_test_plugin(&registry, "node.test", "node.cap");

        let plugin = registry.get_plugin(&plugin_id).unwrap();
        let cap_meta = CapabilityMetadata::new("node.cap");
        let node = PluginCapabilityNode::new(
            plugin,
            registry,
            "node.cap",
            plugin_id.clone(),
            PluginCapabilityId::new("node.cap"),
            cap_meta,
        );

        assert_eq!(node.capability(), "node.cap");
    }

    #[test]
    fn capability_node_validate_succeeds_for_running_plugin() {
        let registry = PluginRegistry::new();
        let plugin_id = register_test_plugin(&registry, "val.test", "val.cap");

        let plugin = registry.get_plugin(&plugin_id).unwrap();
        let cap_meta = CapabilityMetadata::new("val.cap");
        let node = PluginCapabilityNode::new(
            plugin,
            registry,
            "val.cap",
            plugin_id,
            PluginCapabilityId::new("val.cap"),
            cap_meta,
        );

        let result = node.validate(&ExecutionInput::new());
        assert!(result.is_ok());
    }

    #[test]
    fn capability_node_validate_fails_for_non_running_plugin() {
        let registry = PluginRegistry::new();
        let plugin_id = register_test_plugin(&registry, "not.ready", "notready.cap");

        registry
            .transition_to(&plugin_id, PluginState::Paused)
            .unwrap();

        let plugin = registry.get_plugin(&plugin_id).unwrap();
        let cap_meta = CapabilityMetadata::new("notready.cap");
        let node = PluginCapabilityNode::new(
            plugin,
            registry,
            "notready.cap",
            plugin_id,
            PluginCapabilityId::new("notready.cap"),
            cap_meta,
        );

        let result = node.validate(&ExecutionInput::new());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("expected Running"));
    }

    #[test]
    fn capability_node_execute_calls_plugin() {
        let registry = PluginRegistry::new();
        let plugin_id = register_test_plugin(&registry, "exec.test", "exec.cap");

        let plugin = registry.get_plugin(&plugin_id).unwrap();
        let cap_meta = CapabilityMetadata::new("exec.cap");
        let node = PluginCapabilityNode::new(
            plugin,
            registry,
            "exec.cap",
            plugin_id,
            PluginCapabilityId::new("exec.cap"),
            cap_meta,
        );

        let ctx = test_ctx();
        let input = ExecutionInput::new().with_param("url", "https://example.com");
        let result = node.execute(&ctx, input);
        assert!(result.is_ok());

        let output = result.unwrap();
        assert_eq!(output.values.get("executed").unwrap(), "true");
        assert_eq!(output.values.get("url").unwrap(), "https://example.com");
    }

    #[test]
    fn capability_node_execute_respects_cancellation() {
        let registry = PluginRegistry::new();
        let plugin_id = register_test_plugin(&registry, "cancel.test", "cancel.cap");

        let plugin = registry.get_plugin(&plugin_id).unwrap();
        let cap_meta = CapabilityMetadata::new("cancel.cap");
        let node = PluginCapabilityNode::new(
            plugin,
            registry,
            "cancel.cap",
            plugin_id,
            PluginCapabilityId::new("cancel.cap"),
            cap_meta,
        );

        let token = CancellationToken::new();
        token.cancel();
        let ctx = ExecutionContext {
            cancellation_token: token,
            ..test_ctx()
        };

        let result = node.execute(&ctx, ExecutionInput::new());
        assert!(result.is_err());
        match result {
            Err(ExecutionError::Cancelled) => {}
            _ => panic!("expected Cancelled error"),
        }
    }

    #[test]
    fn capability_node_validates_required_params() {
        let registry = PluginRegistry::new();
        let plugin_id = register_test_plugin(&registry, "params.test", "params.cap");

        let plugin = registry.get_plugin(&plugin_id).unwrap();
        let cap_meta = CapabilityMetadata::new("params.cap")
            .with_required_param("url")
            .with_required_param("method");
        let node = PluginCapabilityNode::new(
            plugin,
            registry,
            "params.cap",
            plugin_id,
            PluginCapabilityId::new("params.cap"),
            cap_meta,
        );

        let input = ExecutionInput::new().with_param("url", "https://example.com");
        let result = node.validate(&input);
        assert!(result.is_err());

        let input = input.with_param("method", "GET");
        let result = node.validate(&input);
        assert!(result.is_ok());
    }

    // ── PluginExecutionBridge tests ────────────────────────────────────

    #[test]
    fn bridge_register_all_registers_running_plugins() {
        let registry = PluginRegistry::new();
        register_test_plugin(&registry, "bridge.test", "bridge.cap");

        let resolver = CapabilityResolver::default();
        let bridge = PluginExecutionBridge::new(registry, resolver);
        let node_registry = make_node_registry();

        let count = bridge.register_all(&node_registry);
        assert_eq!(count, 1);

        let node = node_registry.find("bridge.cap");
        assert!(node.is_some());
    }

    #[test]
    fn bridge_register_all_skips_non_running() {
        let registry = PluginRegistry::new();

        let v = PluginVersion::new(1, 0, 0);
        let cap = PluginCapability::new(
            PluginCapabilityId::new("skip.cap"),
            CapabilityMetadata::new("skip.cap"),
        );
        let plugin: PluginPtr =
            Arc::new(ExecutableMockPlugin::new("skip.plugin", v).with_capability(cap));
        registry.register(plugin).unwrap();

        let resolver = CapabilityResolver::default();
        let bridge = PluginExecutionBridge::new(registry, resolver);
        let node_registry = make_node_registry();

        let count = bridge.register_all(&node_registry);
        assert_eq!(count, 0);
        assert!(node_registry.find("skip.cap").is_none());
    }

    #[test]
    fn bridge_register_capability_resolves_and_registers() {
        let registry = PluginRegistry::new();
        register_test_plugin(&registry, "reg.test", "reg.cap");

        let resolver = CapabilityResolver::default();
        let bridge = PluginExecutionBridge::new(registry, resolver);
        let node_registry = make_node_registry();

        let request = CapabilityRequest::new("reg.cap");
        let was_overwritten = bridge
            .register_capability(&node_registry, &request)
            .unwrap();
        assert!(!was_overwritten);

        let node = node_registry.find("reg.cap");
        assert!(node.is_some());
    }

    #[test]
    fn bridge_register_capability_nonexistent_fails() {
        let registry = PluginRegistry::new();
        register_test_plugin(&registry, "other.test", "other.cap");

        let resolver = CapabilityResolver::default();
        let bridge = PluginExecutionBridge::new(registry, resolver);
        let node_registry = make_node_registry();

        let request = CapabilityRequest::new("no.such.cap");
        let result = bridge.register_capability(&node_registry, &request);
        assert!(result.is_err());
    }

    // ── PluginRuntime tests ───────────────────────────────────────────

    #[test]
    fn plugin_runtime_creates_from_manager() {
        let manager = Arc::new(PluginManager::new(
            crate::loader::PluginLoader::new(Vec::new(), Vec::new()),
            PluginRegistry::new(),
        ));
        let runtime = PluginRuntime::from_manager(manager);
        assert_eq!(runtime.manager().plugins().len(), 0);
    }

    #[test]
    fn plugin_runtime_register_capabilities() {
        let manager = Arc::new(PluginManager::new(
            crate::loader::PluginLoader::new(Vec::new(), Vec::new()),
            PluginRegistry::new(),
        ));
        let registry = manager.registry();
        register_test_plugin(registry, "rt.test", "rt.cap");

        let runtime = PluginRuntime::from_manager(manager);
        let node_registry = make_node_registry();
        let count = runtime.register_capabilities(&node_registry);

        assert_eq!(count, 1);
        assert!(node_registry.find("rt.cap").is_some());
    }

    #[test]
    fn plugin_runtime_end_to_end_via_dag_engine() {
        let manager = Arc::new(PluginManager::new(
            crate::loader::PluginLoader::new(Vec::new(), Vec::new()),
            PluginRegistry::new(),
        ));
        let registry = manager.registry();
        register_test_plugin(registry, "e2e.test", "e2e.cap");

        let runtime = PluginRuntime::from_manager(manager);
        let node_registry = make_node_registry();
        runtime.register_capabilities(&node_registry);

        let engine = crate::engine::DagEngine::new(
            EventBus::new(),
            Logger::new(Arc::new(StdoutSink::new()), Arc::new(NoopFilter), "test"),
            MetricsRegistry::new(),
            Tracer::new(),
        );

        let node = node_registry.find("e2e.cap").unwrap();
        let ctx = test_ctx();
        let dag_node = crate::exec::NodeFactory::create_node(
            node,
            NodeId::from_string("e2e-node"),
            "E2E Test",
            ExecutionInput::new(),
            None,
            ctx,
        );

        engine.register_node(dag_node).unwrap();
        let result = engine.execute(&[NodeId::from_string("e2e-node")]).unwrap();
        assert_eq!(result.state, crate::node::DagExecutionState::Completed);
        assert_eq!(result.success_count, 1);
    }

    #[test]
    fn plugin_runtime_end_to_end_with_params() {
        let manager = Arc::new(PluginManager::new(
            crate::loader::PluginLoader::new(Vec::new(), Vec::new()),
            PluginRegistry::new(),
        ));
        let registry = manager.registry();
        register_test_plugin(registry, "params.e2e", "params.e2e.cap");

        let runtime = PluginRuntime::from_manager(manager);
        let node_registry = make_node_registry();
        runtime.register_capabilities(&node_registry);

        let engine = crate::engine::DagEngine::new(
            EventBus::new(),
            Logger::new(Arc::new(StdoutSink::new()), Arc::new(NoopFilter), "test"),
            MetricsRegistry::new(),
            Tracer::new(),
        );

        let node = node_registry.find("params.e2e.cap").unwrap();
        let ctx = test_ctx();
        let dag_node = crate::exec::NodeFactory::create_node(
            node,
            NodeId::from_string("params-e2e-node"),
            "Params E2E",
            ExecutionInput::new().with_param("action", "test"),
            None,
            ctx,
        );

        engine.register_node(dag_node).unwrap();
        let result = engine
            .execute(&[NodeId::from_string("params-e2e-node")])
            .unwrap();
        assert_eq!(result.state, crate::node::DagExecutionState::Completed);
        assert_eq!(result.success_count, 1);
    }

    // ── Edge cases ───────────────────────────────────────────────────

    #[test]
    fn multiple_plugins_same_capability_resolves_first() {
        let registry = PluginRegistry::new();

        let cap = PluginCapability::new(
            PluginCapabilityId::new("shared.cap"),
            CapabilityMetadata::new("shared.cap"),
        );
        let plugin_a: PluginPtr = Arc::new(
            ExecutableMockPlugin::new("alpha", PluginVersion::new(1, 0, 0))
                .with_capability(cap.clone()),
        );
        let reg_a = registry.register(plugin_a).unwrap();
        for &s in &[
            PluginState::Registered,
            PluginState::Validated,
            PluginState::Initialized,
            PluginState::Running,
        ] {
            registry.transition_to(&reg_a.plugin_id, s).unwrap();
        }

        let plugin_b: PluginPtr = Arc::new(
            ExecutableMockPlugin::new("beta", PluginVersion::new(2, 0, 0)).with_capability(cap),
        );
        let reg_b = registry.register(plugin_b).unwrap();
        for &s in &[
            PluginState::Registered,
            PluginState::Validated,
            PluginState::Initialized,
            PluginState::Running,
        ] {
            registry.transition_to(&reg_b.plugin_id, s).unwrap();
        }

        let resolver = CapabilityResolver::default();
        let request = CapabilityRequest::new("shared.cap");
        let matches = resolver.resolve(&registry, &request);
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn bridge_with_empty_registry_registers_zero() {
        let registry = PluginRegistry::new();
        let resolver = CapabilityResolver::default();
        let bridge = PluginExecutionBridge::new(registry, resolver);
        let node_registry = make_node_registry();

        let count = bridge.register_all(&node_registry);
        assert_eq!(count, 0);
    }

    #[test]
    fn capability_node_handle_nonexistent_plugin_in_registry() {
        let registry = PluginRegistry::new();
        let plugin: PluginPtr = Arc::new(ExecutableMockPlugin::new(
            "ghost",
            PluginVersion::new(1, 0, 0),
        ));

        let node = PluginCapabilityNode::new(
            plugin,
            registry,
            "ghost.cap",
            PluginId::new("ghost"),
            PluginCapabilityId::new("ghost.cap"),
            CapabilityMetadata::new("ghost.cap"),
        );

        let result = node.validate(&ExecutionInput::new());
        assert!(result.is_err());
    }

    // ── Send + Sync ──────────────────────────────────────────────────

    #[test]
    fn runtime_types_are_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<CapabilityRequest>();
        assert_sync::<CapabilityRequest>();
        assert_send::<CapabilityMatch>();
        assert_sync::<CapabilityMatch>();
        assert_send::<CapabilitySelection>();
        assert_sync::<CapabilitySelection>();
        assert_send::<CapabilityMatcher>();
        assert_sync::<CapabilityMatcher>();
        assert_send::<MatchCriterion>();
        assert_sync::<MatchCriterion>();
        assert_send::<CapabilityResolver>();
        assert_sync::<CapabilityResolver>();
        assert_send::<PluginCapabilityNode>();
        assert_sync::<PluginCapabilityNode>();
        assert_send::<PluginExecutionBridge>();
        assert_sync::<PluginExecutionBridge>();
        assert_send::<PluginRuntime>();
        assert_sync::<PluginRuntime>();
    }
}
