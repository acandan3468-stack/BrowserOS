/// P14 — Plugin Runtime Foundation
///
/// Defines trait-based runtime contracts for every future Plugin system.
/// All types are strongly typed and serde-serialisable — no `Any`,
/// no `Box<dyn ...>`, no `serde_json::Value`.
///
/// # Design invariants
/// - `Plugin` trait exposes **only** contracts, no implementations
/// - `PluginRegistry` owns metadata only — never browser objects
/// - All types are `#[non_exhaustive]` where extensibility is required
/// - Reuses exec.rs abstractions (`ExecutionContext`, `NodeRegistry`, etc.)
/// - Zero runtime behaviour changes — no executor / scheduler / event-bus modifications
/// - Thread-safe: `PluginRegistry` wraps an `Arc<RwLock<...>>`
///
/// # Architecture flow
/// ```text
/// Planner → Capability Resolution → Plugin Registry → NodeFactory → DagEngine
/// ```
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::exec::{CapabilityMetadata, ExecutionContext, ExecutionInput, ExecutionOutput};

// ── Phantom type impls ──────────────────────────────────────────────

fn _phantom_send_sync<T: Send + Sync>() {}

#[allow(dead_code)]
fn _plugin_types_are_send_sync() {
    _phantom_send_sync::<PluginId>();
    _phantom_send_sync::<PluginVersion>();
    _phantom_send_sync::<PluginAuthor>();
    _phantom_send_sync::<PluginCapabilityId>();
    _phantom_send_sync::<PluginCapability>();
    _phantom_send_sync::<PluginState>();
    _phantom_send_sync::<PluginStatus>();
    _phantom_send_sync::<PluginPermission>();
    _phantom_send_sync::<PluginDependency>();
    _phantom_send_sync::<PluginMetadata>();
    _phantom_send_sync::<PluginManifest>();
    _phantom_send_sync::<PluginHooks>();
    _phantom_send_sync::<PluginContext>();
    _phantom_send_sync::<PluginExecutionMetadata>();
    _phantom_send_sync::<PluginRegistration>();
    _phantom_send_sync::<PluginValidationResult>();
    _phantom_send_sync::<PluginError>();
    _phantom_send_sync::<PluginLifecycle>();
}

// ── PluginId ────────────────────────────────────────────────────────

/// Unique identifier for a plugin.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PluginId(String);

impl PluginId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for PluginId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for PluginId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for PluginId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

// ── PluginVersion ───────────────────────────────────────────────────

/// Semantic version for a plugin.
///
/// Follows `MAJOR.MINOR.PATCH` semver convention with simple comparison.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl PluginVersion {
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// Parse a `"MAJOR.MINOR.PATCH"` string.
    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.splitn(3, '.').collect();
        if parts.len() != 3 {
            return None;
        }
        Some(Self {
            major: parts[0].parse().ok()?,
            minor: parts[1].parse().ok()?,
            patch: parts[2].parse().ok()?,
        })
    }

    /// Returns `true` when `other` is semver-compatible with `self`
    /// (same major version, minor ≥ self.minor).
    pub fn compatible_with(&self, other: &PluginVersion) -> bool {
        self.major == other.major && other.minor >= self.minor
    }
}

impl std::fmt::Display for PluginVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl PartialOrd for PluginVersion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PluginVersion {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.major
            .cmp(&other.major)
            .then(self.minor.cmp(&other.minor))
            .then(self.patch.cmp(&other.patch))
    }
}

// ── PluginAuthor ────────────────────────────────────────────────────

/// Author of a plugin.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginAuthor {
    pub name: String,
    pub email: Option<String>,
    pub url: Option<String>,
}

impl PluginAuthor {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            email: None,
            url: None,
        }
    }

    pub fn with_email(mut self, email: impl Into<String>) -> Self {
        self.email = Some(email.into());
        self
    }

    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }
}

// ── PluginCapabilityId ──────────────────────────────────────────────

/// Unique identifier for a plugin capability.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PluginCapabilityId(String);

impl PluginCapabilityId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for PluginCapabilityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for PluginCapabilityId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

// ── PluginCapability ────────────────────────────────────────────────

/// A capability declared by a plugin.
///
/// Wraps the existing [`CapabilityMetadata`] from exec.rs so the
/// Plugin Registry can feed capability descriptors directly into
/// [`NodeRegistry`] for execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCapability {
    pub id: PluginCapabilityId,
    pub metadata: CapabilityMetadata,
}

impl PluginCapability {
    pub fn new(id: PluginCapabilityId, metadata: CapabilityMetadata) -> Self {
        Self { id, metadata }
    }
}

// ── PluginState ─────────────────────────────────────────────────────

/// Lifecycle state of a plugin.
///
/// Valid transitions:
/// - `Discovered → Registered`
/// - `Registered → Validated`
/// - `Validated → Initialized`
/// - `Initialized → Running`
/// - `Running ↔ Paused`
/// - `Running → Disabled`, `Paused → Disabled`
/// - `* → Failed` (any state)
/// - `Failed → Unloaded`, `Disabled → Unloaded`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PluginState {
    Discovered,
    Registered,
    Validated,
    Initialized,
    Running,
    Paused,
    Disabled,
    Failed,
    Unloaded,
}

impl PluginState {
    /// Returns `true` if transitioning from `self` to `other` is valid.
    pub fn can_transition_to(self, other: PluginState) -> bool {
        matches!(
            (self, other),
            (PluginState::Discovered, PluginState::Registered)
                | (PluginState::Registered, PluginState::Validated)
                | (PluginState::Validated, PluginState::Initialized)
                | (PluginState::Initialized, PluginState::Running)
                | (PluginState::Running, PluginState::Paused)
                | (PluginState::Running, PluginState::Disabled)
                | (PluginState::Running, PluginState::Failed)
                | (PluginState::Paused, PluginState::Running)
                | (PluginState::Paused, PluginState::Disabled)
                | (PluginState::Paused, PluginState::Failed)
                | (PluginState::Disabled, PluginState::Unloaded)
                | (PluginState::Failed, PluginState::Unloaded)
                | (_, PluginState::Failed)
        )
    }
}

impl std::fmt::Display for PluginState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PluginState::Discovered => write!(f, "Discovered"),
            PluginState::Registered => write!(f, "Registered"),
            PluginState::Validated => write!(f, "Validated"),
            PluginState::Initialized => write!(f, "Initialized"),
            PluginState::Running => write!(f, "Running"),
            PluginState::Paused => write!(f, "Paused"),
            PluginState::Disabled => write!(f, "Disabled"),
            PluginState::Failed => write!(f, "Failed"),
            PluginState::Unloaded => write!(f, "Unloaded"),
        }
    }
}

// ── PluginLifecycle ─────────────────────────────────────────────────

/// Validated lifecycle transitions for plugins.
pub struct PluginLifecycle;

impl PluginLifecycle {
    /// Validate that a transition from `current` to `next` is legal.
    pub fn validate_transition(
        current: PluginState,
        next: PluginState,
        plugin_id: &PluginId,
    ) -> Result<(), PluginError> {
        if current.can_transition_to(next) {
            Ok(())
        } else {
            Err(PluginError::InvalidTransition {
                plugin_id: plugin_id.clone(),
                from: current,
                to: next,
            })
        }
    }
}

// ── PluginStatus ────────────────────────────────────────────────────

/// Operational health status of a plugin.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PluginStatus {
    Healthy,
    Degraded(String),
    Unhealthy(String),
    Unknown,
}

// ── PluginPermission ────────────────────────────────────────────────

/// Typed permission that a plugin may require.
///
/// Extensible via `#[non_exhaustive]` — new variants can be added in
/// future versions without breaking existing plugins.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum PluginPermission {
    BrowserAccess,
    NetworkAccess,
    StorageAccess,
    FilesystemAccess,
    ClipboardAccess,
    DownloadAccess,
    InputSimulation,
}

// ── PluginDependency ────────────────────────────────────────────────

/// A dependency on another plugin.
///
/// Dependencies can be required (registration fails without them) or
/// optional (capabilities degrade gracefully).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginDependency {
    pub plugin_id: PluginId,
    pub required: bool,
    pub min_version: Option<PluginVersion>,
    pub max_version: Option<PluginVersion>,
}

impl PluginDependency {
    pub fn new(plugin_id: PluginId) -> Self {
        Self {
            plugin_id,
            required: true,
            min_version: None,
            max_version: None,
        }
    }

    pub fn optional(mut self) -> Self {
        self.required = false;
        self
    }

    pub fn with_min_version(mut self, version: PluginVersion) -> Self {
        self.min_version = Some(version);
        self
    }

    pub fn with_max_version(mut self, version: PluginVersion) -> Self {
        self.max_version = Some(version);
        self
    }

    /// Check whether `found` satisfies this dependency's version constraints.
    pub fn matches(&self, found: &PluginVersion) -> bool {
        if let Some(ref min) = self.min_version {
            if found < min {
                return false;
            }
        }
        if let Some(ref max) = self.max_version {
            if found > max {
                return false;
            }
        }
        true
    }
}

// ── PluginMetadata ──────────────────────────────────────────────────

/// Human-readable metadata about a plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    pub name: String,
    pub version: PluginVersion,
    pub author: PluginAuthor,
    pub description: String,
    pub homepage: Option<String>,
    pub license: Option<String>,
}

impl PluginMetadata {
    pub fn new(name: impl Into<String>, version: PluginVersion, author: PluginAuthor) -> Self {
        Self {
            name: name.into(),
            version,
            author,
            description: String::new(),
            homepage: None,
            license: None,
        }
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    pub fn with_homepage(mut self, url: impl Into<String>) -> Self {
        self.homepage = Some(url.into());
        self
    }

    pub fn with_license(mut self, license: impl Into<String>) -> Self {
        self.license = Some(license.into());
        self
    }
}

// ── PluginHooks ─────────────────────────────────────────────────────

/// Typed hook contracts that a plugin advertises.
///
/// Hooks are **contracts only** — they describe which execution extension
/// points the plugin will participate in, without any runtime behaviour.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginHooks {
    /// Hook names fired before a DAG node executes.
    pub before_execution: Vec<String>,
    /// Hook names fired after a DAG node executes.
    pub after_execution: Vec<String>,
    /// Hook names fired before a specific named DAG node.
    pub before_node: Vec<String>,
    /// Hook names fired after a specific named DAG node.
    pub after_node: Vec<String>,
    /// Hook names fired when a DAG node execution fails.
    pub execution_failed: Vec<String>,
    /// Hook names fired when a DAG node execution is cancelled.
    pub execution_cancelled: Vec<String>,
}

impl PluginHooks {
    pub fn new() -> Self {
        Self {
            before_execution: Vec::new(),
            after_execution: Vec::new(),
            before_node: Vec::new(),
            after_node: Vec::new(),
            execution_failed: Vec::new(),
            execution_cancelled: Vec::new(),
        }
    }

    pub fn on_before_execution(mut self, name: impl Into<String>) -> Self {
        self.before_execution.push(name.into());
        self
    }

    pub fn on_after_execution(mut self, name: impl Into<String>) -> Self {
        self.after_execution.push(name.into());
        self
    }

    pub fn on_before_node(mut self, name: impl Into<String>) -> Self {
        self.before_node.push(name.into());
        self
    }

    pub fn on_after_node(mut self, name: impl Into<String>) -> Self {
        self.after_node.push(name.into());
        self
    }

    pub fn on_execution_failed(mut self, name: impl Into<String>) -> Self {
        self.execution_failed.push(name.into());
        self
    }

    pub fn on_execution_cancelled(mut self, name: impl Into<String>) -> Self {
        self.execution_cancelled.push(name.into());
        self
    }
}

impl Default for PluginHooks {
    fn default() -> Self {
        Self::new()
    }
}

// ── PluginManifest ──────────────────────────────────────────────────

/// Complete manifest describing a plugin's identity, capabilities,
/// dependencies, permissions, and hook contracts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub metadata: PluginMetadata,
    pub capabilities: Vec<PluginCapability>,
    pub dependencies: Vec<PluginDependency>,
    pub permissions: Vec<PluginPermission>,
    pub hooks: PluginHooks,
}

impl PluginManifest {
    pub fn new(metadata: PluginMetadata) -> Self {
        Self {
            metadata,
            capabilities: Vec::new(),
            dependencies: Vec::new(),
            permissions: Vec::new(),
            hooks: PluginHooks::new(),
        }
    }

    pub fn with_capability(mut self, cap: PluginCapability) -> Self {
        self.capabilities.push(cap);
        self
    }

    pub fn with_dependency(mut self, dep: PluginDependency) -> Self {
        self.dependencies.push(dep);
        self
    }

    pub fn with_permission(mut self, perm: PluginPermission) -> Self {
        self.permissions.push(perm);
        self
    }

    pub fn with_hooks(mut self, hooks: PluginHooks) -> Self {
        self.hooks = hooks;
        self
    }
}

// ── PluginExecutionMetadata ─────────────────────────────────────────

/// Per-execution metadata for a plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginExecutionMetadata {
    pub execution_count: u64,
    pub last_started_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub state: PluginState,
}

impl PluginExecutionMetadata {
    pub fn new(state: PluginState) -> Self {
        Self {
            execution_count: 0,
            last_started_at: None,
            last_error: None,
            state,
        }
    }
}

// ── PluginContext ───────────────────────────────────────────────────

/// Lightweight context provided to a plugin during initialisation and
/// execution.
///
/// Reuses [`ExecutionContext`] from exec.rs for DAG execution identity
/// and observability.
#[derive(Debug, Clone)]
pub struct PluginContext {
    pub plugin_id: PluginId,
    pub execution_context: Option<ExecutionContext>,
    pub metadata: PluginExecutionMetadata,
}

impl PluginContext {
    pub fn new(plugin_id: PluginId) -> Self {
        Self {
            plugin_id,
            execution_context: None,
            metadata: PluginExecutionMetadata::new(PluginState::Discovered),
        }
    }

    pub fn with_execution_context(mut self, ctx: ExecutionContext) -> Self {
        self.execution_context = Some(ctx);
        self
    }
}

// ── PluginRegistration ──────────────────────────────────────────────

/// A snapshot of a plugin's registration state in the registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginRegistration {
    pub plugin_id: PluginId,
    pub metadata: PluginMetadata,
    pub manifest: PluginManifest,
    pub state: PluginState,
    pub status: PluginStatus,
    pub registered_at: DateTime<Utc>,
    pub last_health_check: Option<DateTime<Utc>>,
}

impl PluginRegistration {
    pub fn new(plugin_id: PluginId, metadata: PluginMetadata, manifest: PluginManifest) -> Self {
        Self {
            plugin_id,
            metadata,
            manifest,
            state: PluginState::Discovered,
            status: PluginStatus::Unknown,
            registered_at: Utc::now(),
            last_health_check: None,
        }
    }
}

// ── PluginValidationResult ──────────────────────────────────────────

/// Result of validating a plugin's dependencies and permissions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginValidationResult {
    pub valid: bool,
    pub missing_dependencies: Vec<PluginDependency>,
    pub version_mismatches: Vec<PluginDependency>,
    pub missing_permissions: Vec<PluginPermission>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl PluginValidationResult {
    pub fn new() -> Self {
        Self {
            valid: true,
            missing_dependencies: Vec::new(),
            version_mismatches: Vec::new(),
            missing_permissions: Vec::new(),
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn with_missing_dependency(mut self, dep: PluginDependency) -> Self {
        self.missing_dependencies.push(dep);
        self.valid = false;
        self
    }

    pub fn with_version_mismatch(mut self, dep: PluginDependency) -> Self {
        self.version_mismatches.push(dep);
        self.valid = false;
        self
    }

    pub fn with_missing_permission(mut self, perm: PluginPermission) -> Self {
        self.missing_permissions.push(perm);
        self.valid = false;
        self
    }

    pub fn with_error(mut self, err: impl Into<String>) -> Self {
        self.errors.push(err.into());
        self.valid = false;
        self
    }

    pub fn with_warning(mut self, warn: impl Into<String>) -> Self {
        self.warnings.push(warn.into());
        self
    }
}

impl Default for PluginValidationResult {
    fn default() -> Self {
        Self::new()
    }
}

// ── PluginError ─────────────────────────────────────────────────────

/// Typed error that can occur during plugin operations.
#[derive(Debug, Clone, thiserror::Error)]
pub enum PluginError {
    #[error("plugin {0} not found")]
    NotFound(PluginId),

    #[error("plugin {0} already registered")]
    AlreadyRegistered(PluginId),

    #[error("plugin {0} not registered")]
    NotRegistered(PluginId),

    #[error("plugin {plugin_id} invalid state: current={current}, expected one of {expected:?}")]
    InvalidState {
        plugin_id: PluginId,
        current: PluginState,
        expected: Vec<PluginState>,
    },

    #[error("invalid transition for plugin {plugin_id}: {from} → {to}")]
    InvalidTransition {
        plugin_id: PluginId,
        from: PluginState,
        to: PluginState,
    },

    #[error("plugin {plugin_id} dependency {dependency} not found")]
    DependencyNotFound {
        plugin_id: PluginId,
        dependency: PluginId,
    },

    #[error("plugin {plugin_id} dependency {dependency} version mismatch: required {required}, found {found}")]
    DependencyVersionMismatch {
        plugin_id: PluginId,
        dependency: PluginId,
        required: PluginVersion,
        found: PluginVersion,
    },

    #[error("plugin {0} missing permission: {1:?}")]
    MissingPermission(PluginId, PluginPermission),

    #[error("plugin {0} initialisation failed: {1}")]
    InitializationFailed(PluginId, String),

    #[error("plugin {0} shutdown failed: {1}")]
    ShutdownFailed(PluginId, String),

    #[error("plugin {0} health check failed: {1}")]
    HealthCheckFailed(PluginId, String),

    #[error("manifest not found: {0}")]
    ManifestNotFound(String),

    #[error("manifest parse failed: {0}")]
    ManifestParseFailed(String),

    #[error("circular dependency detected: {0:?}")]
    CircularDependency(Vec<PluginId>),

    #[error("plugin load failed: {0}")]
    LoadFailed(String),

    #[error("plugin error: {0}")]
    Internal(String),
}

// ── StoredPlugin ────────────────────────────────────────────────────

/// Internal entry stored in the registry for each plugin.
struct StoredPlugin {
    plugin: Arc<dyn Plugin>,
    registration: PluginRegistration,
}

// ── Plugin trait ────────────────────────────────────────────────────

/// Contract-only trait that every plugin must implement.
///
/// # Thread safety
/// Implementations must be [`Send`] + [`Sync`] + `'static` so they can
/// be shared across DAG worker threads and stored in [`PluginRegistry`].
///
/// # Contracts (no implementation)
/// - [`metadata`](Plugin::metadata) — returns the plugin's identity
/// - [`manifest`](Plugin::manifest) — returns the full manifest
/// - [`capabilities`](Plugin::capabilities) — what this plugin provides
/// - [`permissions`](Plugin::permissions) — what this plugin requires
/// - [`dependencies`](Plugin::dependencies) — what this plugin needs
/// - [`validate`](Plugin::validate) — validate against the registry
/// - [`initialize`](Plugin::initialize) — prepare for execution
/// - [`shutdown`](Plugin::shutdown) — clean up resources
/// - [`health`](Plugin::health) — current operational status
/// - [`execute_capability`](Plugin::execute_capability) — run a capability
///
/// # Default implementations
/// - [`execute_capability`](Plugin::execute_capability) returns
///   [`PluginError::Internal`] — override to provide execution.
pub trait Plugin: Send + Sync + 'static {
    fn metadata(&self) -> PluginMetadata;
    fn manifest(&self) -> PluginManifest;
    fn capabilities(&self) -> Vec<PluginCapability>;
    fn permissions(&self) -> Vec<PluginPermission>;
    fn dependencies(&self) -> Vec<PluginDependency>;
    fn validate(&self, registry: &PluginRegistry) -> PluginValidationResult;
    fn initialize(&self, ctx: &PluginContext) -> Result<(), PluginError>;
    fn shutdown(&self) -> Result<(), PluginError>;
    fn health(&self) -> PluginStatus;
    fn execute_capability(
        &self,
        capability_id: &PluginCapabilityId,
        _ctx: &PluginContext,
        _input: ExecutionInput,
    ) -> Result<ExecutionOutput, PluginError> {
        Err(PluginError::Internal(format!(
            "Plugin {} does not support execute_capability",
            capability_id,
        )))
    }
}

// ── PluginRegistry ──────────────────────────────────────────────────

/// Thread-safe registry of plugins.
///
/// The registry owns metadata only — it never owns browser objects.
///
/// # Thread safety
/// Wraps an [`Arc<RwLock<...>>`] so multiple threads can read and write
/// concurrently.
///
/// # Capability resolution
/// The registry can feed plugin capabilities into a [`NodeRegistry`]
/// so that the Planner → Capability Resolution → Plugin Registry →
/// NodeFactory → DagEngine flow works end-to-end.
#[derive(Clone)]
pub struct PluginRegistry {
    inner: Arc<RwLock<HashMap<PluginId, StoredPlugin>>>,
}

impl std::fmt::Debug for PluginRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let count = self.inner.read().map(|map| map.len()).unwrap_or(0);
        f.debug_struct("PluginRegistry")
            .field("plugin_count", &count)
            .finish_non_exhaustive()
    }
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a new plugin.
    ///
    /// The plugin starts in [`PluginState::Discovered`].  Call
    /// [`transition_to`](PluginRegistry::transition_to) to advance its
    /// lifecycle.
    pub fn register(&self, plugin: Arc<dyn Plugin>) -> Result<PluginRegistration, PluginError> {
        let metadata = plugin.metadata();
        let manifest = plugin.manifest();
        let plugin_id = manifest.metadata.name.clone().into();

        {
            let mut map = self
                .inner
                .write()
                .map_err(|_| PluginError::Internal("PluginRegistry lock poisoned".into()))?;

            if map.contains_key(&plugin_id) {
                return Err(PluginError::AlreadyRegistered(plugin_id.clone()));
            }

            let registration = PluginRegistration::new(plugin_id.clone(), metadata, manifest);

            map.insert(
                plugin_id.clone(),
                StoredPlugin {
                    plugin,
                    registration: registration.clone(),
                },
            );

            Ok(registration)
        }
    }

    /// Unregister a plugin.  The plugin must be in [`PluginState::Unloaded`]
    /// or [`PluginState::Failed`].
    pub fn unregister(&self, plugin_id: &PluginId) -> Result<(), PluginError> {
        let mut map = self
            .inner
            .write()
            .map_err(|_| PluginError::Internal("PluginRegistry lock poisoned".into()))?;

        let stored = map
            .get(plugin_id)
            .ok_or_else(|| PluginError::NotFound(plugin_id.clone()))?;

        let current = stored.registration.state;
        if current != PluginState::Unloaded && current != PluginState::Failed {
            return Err(PluginError::InvalidState {
                plugin_id: plugin_id.clone(),
                current,
                expected: vec![PluginState::Unloaded, PluginState::Failed],
            });
        }

        map.remove(plugin_id);
        Ok(())
    }

    /// Transition a plugin to a new lifecycle state.
    pub fn transition_to(
        &self,
        plugin_id: &PluginId,
        next: PluginState,
    ) -> Result<PluginState, PluginError> {
        let mut map = self
            .inner
            .write()
            .map_err(|_| PluginError::Internal("PluginRegistry lock poisoned".into()))?;

        let stored = map
            .get_mut(plugin_id)
            .ok_or_else(|| PluginError::NotFound(plugin_id.clone()))?;

        let current = stored.registration.state;
        PluginLifecycle::validate_transition(current, next, plugin_id)?;

        stored.registration.state = next;

        // Refresh cached health status when entering Running state
        if next == PluginState::Running {
            stored.registration.status = stored.plugin.health();
        }

        Ok(next)
    }

    /// Look up a plugin by ID.
    pub fn lookup(&self, plugin_id: &PluginId) -> Option<PluginRegistration> {
        let map = self.inner.read().ok()?;
        map.get(plugin_id).map(|s| s.registration.clone())
    }

    /// Look up all plugins that provide a specific capability.
    pub fn lookup_by_capability(&self, cap_id: &PluginCapabilityId) -> Vec<PluginRegistration> {
        let map = match self.inner.read() {
            Ok(map) => map,
            Err(_) => return Vec::new(),
        };
        map.values()
            .filter(|s| {
                s.registration
                    .manifest
                    .capabilities
                    .iter()
                    .any(|c| &c.id == cap_id)
            })
            .map(|s| s.registration.clone())
            .collect()
    }

    /// Look up all plugins whose version satisfies `predicate`.
    pub fn lookup_by_version(&self, predicate: &PluginVersion) -> Vec<PluginRegistration> {
        let map = match self.inner.read() {
            Ok(map) => map,
            Err(_) => return Vec::new(),
        };
        map.values()
            .filter(|s| s.registration.metadata.version >= *predicate)
            .map(|s| s.registration.clone())
            .collect()
    }

    /// Look up all plugins in a given state.
    pub fn lookup_by_state(&self, state: PluginState) -> Vec<PluginRegistration> {
        let map = match self.inner.read() {
            Ok(map) => map,
            Err(_) => return Vec::new(),
        };
        map.values()
            .filter(|s| s.registration.state == state)
            .map(|s| s.registration.clone())
            .collect()
    }

    /// List all registered plugins.
    pub fn list_plugins(&self) -> Vec<PluginRegistration> {
        let map = match self.inner.read() {
            Ok(map) => map,
            Err(_) => return Vec::new(),
        };
        let mut regs: Vec<PluginRegistration> =
            map.values().map(|s| s.registration.clone()).collect();
        regs.sort_by_key(|a| a.plugin_id.to_string());
        regs
    }

    /// Aggregate all capability IDs from all registered plugins.
    pub fn list_capabilities(&self) -> Vec<PluginCapabilityId> {
        let map = match self.inner.read() {
            Ok(map) => map,
            Err(_) => return Vec::new(),
        };
        let mut caps: Vec<PluginCapabilityId> = map
            .values()
            .flat_map(|s| {
                s.registration
                    .manifest
                    .capabilities
                    .iter()
                    .map(|c| c.id.clone())
            })
            .collect();
        caps.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        caps.dedup();
        caps
    }

    /// Verify that all required dependencies of a plugin are present
    /// and satisfy version constraints.
    pub fn verify_dependencies(
        &self,
        plugin_id: &PluginId,
    ) -> Result<PluginValidationResult, PluginError> {
        let map = self
            .inner
            .read()
            .map_err(|_| PluginError::Internal("PluginRegistry lock poisoned".into()))?;

        let stored = map
            .get(plugin_id)
            .ok_or_else(|| PluginError::NotFound(plugin_id.clone()))?;

        let mut result = PluginValidationResult::new();

        for dep in &stored.registration.manifest.dependencies {
            let dep_id = &dep.plugin_id;

            match map.get(dep_id) {
                None => {
                    if dep.required {
                        result = result.with_missing_dependency(dep.clone());
                    } else {
                        result = result
                            .with_warning(format!("optional dependency {} not found", dep_id));
                    }
                }
                Some(dep_stored) => {
                    let found_version = &dep_stored.registration.metadata.version;
                    if !dep.matches(found_version) {
                        let mismatch = dep.clone();
                        if dep.required {
                            result = result.with_version_mismatch(mismatch);
                        } else {
                            result = result.with_warning(format!(
                                "optional dependency {} version mismatch: found {}",
                                dep_id, found_version
                            ));
                        }
                    }
                }
            }
        }

        Ok(result)
    }

    /// Verify that all required permissions of a plugin are recognised
    /// (allow-listed) by the registry.
    pub fn verify_permissions(
        &self,
        plugin_id: &PluginId,
        allowed_permissions: &[PluginPermission],
    ) -> Result<PluginValidationResult, PluginError> {
        let map = self
            .inner
            .read()
            .map_err(|_| PluginError::Internal("PluginRegistry lock poisoned".into()))?;

        let stored = map
            .get(plugin_id)
            .ok_or_else(|| PluginError::NotFound(plugin_id.clone()))?;

        let mut result = PluginValidationResult::new();

        for perm in &stored.registration.manifest.permissions {
            if !allowed_permissions.contains(perm) {
                result = result.with_missing_permission(perm.clone());
            }
        }

        Ok(result)
    }

    /// Run health checks on all registered plugins.
    pub fn health_inspect(&self) -> Vec<(PluginId, PluginStatus)> {
        let map = match self.inner.read() {
            Ok(map) => map,
            Err(_) => return Vec::new(),
        };
        let mut results: Vec<(PluginId, PluginStatus)> = map
            .values()
            .map(|s| (s.registration.plugin_id.clone(), s.plugin.health()))
            .collect();
        results.sort_by_key(|a| a.0.to_string());
        results
    }

    /// Get a plugin trait reference for advanced operations.
    pub(crate) fn get_plugin(&self, plugin_id: &PluginId) -> Option<Arc<dyn Plugin>> {
        let map = self.inner.read().ok()?;
        map.get(plugin_id).map(|s| Arc::clone(&s.plugin))
    }

    /// Validate a plugin by calling its [`Plugin::validate`] method.
    pub fn validate_plugin(
        &self,
        plugin_id: &PluginId,
    ) -> Result<PluginValidationResult, PluginError> {
        let plugin = self
            .get_plugin(plugin_id)
            .ok_or_else(|| PluginError::NotFound(plugin_id.clone()))?;
        Ok(plugin.validate(self))
    }

    /// Initialize a plugin by calling its [`Plugin::initialize`] method.
    pub fn initialize_plugin(
        &self,
        plugin_id: &PluginId,
        ctx: &PluginContext,
    ) -> Result<(), PluginError> {
        let plugin = self
            .get_plugin(plugin_id)
            .ok_or_else(|| PluginError::NotFound(plugin_id.clone()))?;
        plugin.initialize(ctx)
    }

    /// Shut down a plugin by calling its [`Plugin::shutdown`] method.
    pub fn shutdown_plugin(&self, plugin_id: &PluginId) -> Result<(), PluginError> {
        let plugin = self
            .get_plugin(plugin_id)
            .ok_or_else(|| PluginError::NotFound(plugin_id.clone()))?;
        plugin.shutdown()
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    // ── MockPlugin ───────────────────────────────────────────────────

    struct MockPlugin {
        metadata: PluginMetadata,
        manifest: PluginManifest,
        health_status: std::sync::RwLock<PluginStatus>,
        init_called: AtomicU64,
        shutdown_called: AtomicU64,
    }

    impl MockPlugin {
        fn new(name: &str, version: PluginVersion) -> Self {
            let author = PluginAuthor::new("test author").with_email("test@example.com");
            let metadata = PluginMetadata::new(name, version, author);
            let manifest = PluginManifest::new(metadata.clone());
            Self {
                metadata,
                manifest,
                health_status: std::sync::RwLock::new(PluginStatus::Healthy),
                init_called: AtomicU64::new(0),
                shutdown_called: AtomicU64::new(0),
            }
        }

        fn with_capability(mut self, cap: PluginCapability) -> Self {
            self.manifest = self.manifest.with_capability(cap);
            self
        }

        fn with_dependency(mut self, dep: PluginDependency) -> Self {
            self.manifest = self.manifest.with_dependency(dep);
            self
        }

        fn with_permission(mut self, perm: PluginPermission) -> Self {
            self.manifest = self.manifest.with_permission(perm);
            self
        }

        fn with_hooks(mut self, hooks: PluginHooks) -> Self {
            self.manifest = self.manifest.with_hooks(hooks);
            self
        }

        fn with_degraded(self) -> Self {
            *self.health_status.write().unwrap() =
                PluginStatus::Degraded("test degradation".into());
            self
        }

        fn init_count(&self) -> u64 {
            self.init_called.load(Ordering::SeqCst)
        }

        fn shutdown_count(&self) -> u64 {
            self.shutdown_called.load(Ordering::SeqCst)
        }
    }

    impl Plugin for MockPlugin {
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
            self.manifest.permissions.clone()
        }

        fn dependencies(&self) -> Vec<PluginDependency> {
            self.manifest.dependencies.clone()
        }

        fn validate(&self, _registry: &PluginRegistry) -> PluginValidationResult {
            PluginValidationResult::new()
        }

        fn initialize(&self, _ctx: &PluginContext) -> Result<(), PluginError> {
            self.init_called.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn shutdown(&self) -> Result<(), PluginError> {
            self.shutdown_called.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn health(&self) -> PluginStatus {
            self.health_status.read().unwrap().clone()
        }
    }

    struct FailingMockPlugin {
        metadata: PluginMetadata,
        manifest: PluginManifest,
    }

    impl FailingMockPlugin {
        fn new(name: &str) -> Self {
            let author = PluginAuthor::new("fail author");
            let metadata = PluginMetadata::new(name, PluginVersion::new(1, 0, 0), author);
            let manifest = PluginManifest::new(metadata.clone());
            Self { metadata, manifest }
        }
    }

    impl Plugin for FailingMockPlugin {
        fn metadata(&self) -> PluginMetadata {
            self.metadata.clone()
        }

        fn manifest(&self) -> PluginManifest {
            self.manifest.clone()
        }

        fn capabilities(&self) -> Vec<PluginCapability> {
            Vec::new()
        }

        fn permissions(&self) -> Vec<PluginPermission> {
            Vec::new()
        }

        fn dependencies(&self) -> Vec<PluginDependency> {
            Vec::new()
        }

        fn validate(&self, _registry: &PluginRegistry) -> PluginValidationResult {
            PluginValidationResult::new().with_error("validation failed intentionally")
        }

        fn initialize(&self, _ctx: &PluginContext) -> Result<(), PluginError> {
            Err(PluginError::InitializationFailed(
                PluginId::new("fail"),
                "init failed intentionally".into(),
            ))
        }

        fn shutdown(&self) -> Result<(), PluginError> {
            Err(PluginError::ShutdownFailed(
                PluginId::new("fail"),
                "shutdown failed intentionally".into(),
            ))
        }

        fn health(&self) -> PluginStatus {
            PluginStatus::Unhealthy("always unhealthy".into())
        }
    }

    // ── Helper ───────────────────────────────────────────────────────

    fn make_plugin_registry() -> PluginRegistry {
        PluginRegistry::new()
    }

    fn make_test_plugin(name: &str) -> Arc<dyn Plugin> {
        let author = PluginAuthor::new("test author");
        let version = PluginVersion::new(1, 0, 0);
        let metadata = PluginMetadata::new(name, version, author);
        let cap = PluginCapability::new(
            PluginCapabilityId::new(format!("{}.test_cap", name)),
            CapabilityMetadata::new(format!("{}.test_cap", name)),
        );
        let manifest = PluginManifest::new(metadata).with_capability(cap);
        let plugin = MockPlugin {
            metadata: manifest.metadata.clone(),
            manifest,
            health_status: std::sync::RwLock::new(PluginStatus::Healthy),
            init_called: AtomicU64::new(0),
            shutdown_called: AtomicU64::new(0),
        };
        Arc::new(plugin)
    }

    // ── PluginId tests ───────────────────────────────────────────────

    #[test]
    fn plugin_id_creation() {
        let id = PluginId::new("test.plugin");
        assert_eq!(id.as_str(), "test.plugin");
        assert_eq!(id.to_string(), "test.plugin");
    }

    #[test]
    fn plugin_id_from_string() {
        let id: PluginId = "test.plugin".into();
        assert_eq!(id.as_str(), "test.plugin");
    }

    #[test]
    fn plugin_id_equality() {
        let a = PluginId::new("alpha");
        let b = PluginId::new("alpha");
        let c = PluginId::new("beta");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn plugin_id_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(PluginId::new("a"));
        set.insert(PluginId::new("a"));
        set.insert(PluginId::new("b"));
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn plugin_id_serde_roundtrip() {
        let id = PluginId::new("serde.test");
        let json = serde_json::to_string(&id).unwrap();
        let deserialized: PluginId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, deserialized);
    }

    // ── PluginVersion tests ──────────────────────────────────────────

    #[test]
    fn plugin_version_creation() {
        let v = PluginVersion::new(1, 2, 3);
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
    }

    #[test]
    fn plugin_version_parse_valid() {
        let v = PluginVersion::parse("1.2.3").unwrap();
        assert_eq!(v, PluginVersion::new(1, 2, 3));
    }

    #[test]
    fn plugin_version_parse_invalid_format() {
        assert!(PluginVersion::parse("1.2").is_none());
        assert!(PluginVersion::parse("invalid").is_none());
        assert!(PluginVersion::parse("").is_none());
    }

    #[test]
    fn plugin_version_parse_non_numeric() {
        assert!(PluginVersion::parse("a.b.c").is_none());
    }

    #[test]
    fn plugin_version_display() {
        let v = PluginVersion::new(10, 20, 30);
        assert_eq!(v.to_string(), "10.20.30");
    }

    #[test]
    fn plugin_version_ordering() {
        let v1 = PluginVersion::new(1, 0, 0);
        let v2 = PluginVersion::new(2, 0, 0);
        let v3 = PluginVersion::new(1, 1, 0);
        let v4 = PluginVersion::new(1, 0, 1);
        assert!(v1 < v2);
        assert!(v1 < v3);
        assert!(v1 < v4);
        assert!(v3 < v2);
        assert_eq!(v1, PluginVersion::new(1, 0, 0));
    }

    #[test]
    fn plugin_version_compatible_with() {
        let base = PluginVersion::new(1, 0, 0);
        let patch = PluginVersion::new(1, 0, 5);
        let minor = PluginVersion::new(1, 2, 0);
        let major = PluginVersion::new(2, 0, 0);
        assert!(base.compatible_with(&patch));
        assert!(base.compatible_with(&minor));
        assert!(!base.compatible_with(&major));
    }

    #[test]
    fn plugin_version_compatible_same_major_any_minor() {
        let v120 = PluginVersion::new(1, 2, 0);
        let v110 = PluginVersion::new(1, 1, 0);
        assert!(!v120.compatible_with(&v110));
        assert!(v110.compatible_with(&v120));
    }

    #[test]
    fn plugin_version_serde_roundtrip() {
        let v = PluginVersion::new(3, 2, 1);
        let json = serde_json::to_string(&v).unwrap();
        let deserialized: PluginVersion = serde_json::from_str(&json).unwrap();
        assert_eq!(v, deserialized);
    }

    // ── PluginAuthor tests ───────────────────────────────────────────

    #[test]
    fn plugin_author_creation() {
        let author = PluginAuthor::new("Alice");
        assert_eq!(author.name, "Alice");
        assert!(author.email.is_none());
        assert!(author.url.is_none());
    }

    #[test]
    fn plugin_author_with_email_and_url() {
        let author = PluginAuthor::new("Bob")
            .with_email("bob@example.com")
            .with_url("https://bob.example.com");
        assert_eq!(author.email.unwrap(), "bob@example.com");
        assert_eq!(author.url.unwrap(), "https://bob.example.com");
    }

    #[test]
    fn plugin_author_serde_roundtrip() {
        let author = PluginAuthor::new("Carol").with_email("carol@example.com");
        let json = serde_json::to_string(&author).unwrap();
        let deserialized: PluginAuthor = serde_json::from_str(&json).unwrap();
        assert_eq!(author, deserialized);
    }

    // ── PluginCapabilityId tests ─────────────────────────────────────

    #[test]
    fn capability_id_creation() {
        let id = PluginCapabilityId::new("browser.navigate");
        assert_eq!(id.as_str(), "browser.navigate");
    }

    #[test]
    fn capability_id_serde_roundtrip() {
        let id = PluginCapabilityId::new("dom.click");
        let json = serde_json::to_string(&id).unwrap();
        let deserialized: PluginCapabilityId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, deserialized);
    }

    // ── PluginCapability tests ───────────────────────────────────────

    #[test]
    fn plugin_capability_creation() {
        let id = PluginCapabilityId::new("browser.navigate");
        let meta =
            CapabilityMetadata::new("browser.navigate").with_description("Navigate to a URL");
        let cap = PluginCapability::new(id.clone(), meta.clone());
        assert_eq!(cap.id, id);
        assert_eq!(cap.metadata.name, meta.name);
    }

    #[test]
    fn plugin_capability_serde_roundtrip() {
        let cap = PluginCapability::new(
            PluginCapabilityId::new("test.cap"),
            CapabilityMetadata::new("test.cap")
                .with_description("a test capability")
                .with_version("1.0.0"),
        );
        let json = serde_json::to_string(&cap).unwrap();
        let deserialized: PluginCapability = serde_json::from_str(&json).unwrap();
        assert_eq!(cap.id, deserialized.id);
        assert_eq!(cap.metadata.name, deserialized.metadata.name);
    }

    // ── PluginState tests ────────────────────────────────────────────

    #[test]
    fn plugin_state_valid_transitions() {
        assert!(PluginState::Discovered.can_transition_to(PluginState::Registered));
        assert!(PluginState::Registered.can_transition_to(PluginState::Validated));
        assert!(PluginState::Validated.can_transition_to(PluginState::Initialized));
        assert!(PluginState::Initialized.can_transition_to(PluginState::Running));
        assert!(PluginState::Running.can_transition_to(PluginState::Paused));
        assert!(PluginState::Paused.can_transition_to(PluginState::Running));
        assert!(PluginState::Running.can_transition_to(PluginState::Disabled));
        assert!(PluginState::Paused.can_transition_to(PluginState::Disabled));
        assert!(PluginState::Disabled.can_transition_to(PluginState::Unloaded));
        assert!(PluginState::Failed.can_transition_to(PluginState::Unloaded));
    }

    #[test]
    fn plugin_state_any_to_failed() {
        for state in &[
            PluginState::Discovered,
            PluginState::Registered,
            PluginState::Validated,
            PluginState::Initialized,
            PluginState::Running,
            PluginState::Paused,
            PluginState::Disabled,
            PluginState::Unloaded,
        ] {
            assert!(state.can_transition_to(PluginState::Failed));
        }
    }

    #[test]
    fn plugin_state_invalid_transitions() {
        assert!(!PluginState::Discovered.can_transition_to(PluginState::Running));
        assert!(!PluginState::Registered.can_transition_to(PluginState::Running));
        assert!(!PluginState::Validated.can_transition_to(PluginState::Running));
        assert!(!PluginState::Unloaded.can_transition_to(PluginState::Discovered));
        assert!(!PluginState::Disabled.can_transition_to(PluginState::Running));
        assert!(!PluginState::Failed.can_transition_to(PluginState::Running));
    }

    #[test]
    fn plugin_state_display() {
        assert_eq!(PluginState::Discovered.to_string(), "Discovered");
        assert_eq!(PluginState::Registered.to_string(), "Registered");
        assert_eq!(PluginState::Running.to_string(), "Running");
        assert_eq!(PluginState::Failed.to_string(), "Failed");
        assert_eq!(PluginState::Unloaded.to_string(), "Unloaded");
    }

    #[test]
    fn plugin_state_serde_roundtrip() {
        for state in &[
            PluginState::Discovered,
            PluginState::Registered,
            PluginState::Validated,
            PluginState::Initialized,
            PluginState::Running,
            PluginState::Paused,
            PluginState::Disabled,
            PluginState::Failed,
            PluginState::Unloaded,
        ] {
            let json = serde_json::to_string(state).unwrap();
            let deserialized: PluginState = serde_json::from_str(&json).unwrap();
            assert_eq!(*state, deserialized);
        }
    }

    // ── PluginLifecycle tests ────────────────────────────────────────

    #[test]
    fn lifecycle_valid_transition() {
        let id = PluginId::new("test");
        let result = PluginLifecycle::validate_transition(
            PluginState::Discovered,
            PluginState::Registered,
            &id,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn lifecycle_invalid_transition_returns_error() {
        let id = PluginId::new("test");
        let err = PluginLifecycle::validate_transition(
            PluginState::Discovered,
            PluginState::Running,
            &id,
        )
        .unwrap_err();
        match err {
            PluginError::InvalidTransition {
                ref plugin_id,
                from,
                to,
            } => {
                assert_eq!(*plugin_id, id);
                assert_eq!(from, PluginState::Discovered);
                assert_eq!(to, PluginState::Running);
            }
            _ => panic!("expected InvalidTransition error"),
        }
    }

    // ── PluginStatus tests ───────────────────────────────────────────

    #[test]
    fn plugin_status_ordering() {
        assert!(PluginStatus::Healthy < PluginStatus::Degraded("".into()));
        assert!(PluginStatus::Degraded("".into()) < PluginStatus::Unhealthy("".into()));
        assert!(PluginStatus::Unhealthy("".into()) < PluginStatus::Unknown);
    }

    // ── PluginPermission tests ───────────────────────────────────────

    #[test]
    fn plugin_permission_equality() {
        assert_eq!(
            PluginPermission::BrowserAccess,
            PluginPermission::BrowserAccess
        );
        assert_ne!(
            PluginPermission::BrowserAccess,
            PluginPermission::NetworkAccess
        );
    }

    #[test]
    fn plugin_permission_serde_roundtrip() {
        let perm = PluginPermission::StorageAccess;
        let json = serde_json::to_string(&perm).unwrap();
        let deserialized: PluginPermission = serde_json::from_str(&json).unwrap();
        assert_eq!(perm, deserialized);
    }

    // ── PluginDependency tests ───────────────────────────────────────

    #[test]
    fn dependency_creation() {
        let dep = PluginDependency::new(PluginId::new("other.plugin"));
        assert_eq!(dep.plugin_id, PluginId::new("other.plugin"));
        assert!(dep.required);
        assert!(dep.min_version.is_none());
        assert!(dep.max_version.is_none());
    }

    #[test]
    fn dependency_optional() {
        let dep = PluginDependency::new(PluginId::new("opt.plugin")).optional();
        assert!(!dep.required);
    }

    #[test]
    fn dependency_with_version_bounds() {
        let min_v = PluginVersion::new(1, 0, 0);
        let max_v = PluginVersion::new(2, 0, 0);
        let dep = PluginDependency::new(PluginId::new("ver.plugin"))
            .with_min_version(min_v.clone())
            .with_max_version(max_v.clone());
        assert_eq!(dep.min_version, Some(min_v));
        assert_eq!(dep.max_version, Some(max_v));
    }

    #[test]
    fn dependency_matches_version_in_range() {
        let dep = PluginDependency::new(PluginId::new("dep"))
            .with_min_version(PluginVersion::new(1, 0, 0))
            .with_max_version(PluginVersion::new(3, 0, 0));
        assert!(dep.matches(&PluginVersion::new(1, 5, 0)));
        assert!(dep.matches(&PluginVersion::new(1, 0, 0)));
        assert!(dep.matches(&PluginVersion::new(3, 0, 0)));
    }

    #[test]
    fn dependency_matches_version_out_of_range() {
        let dep = PluginDependency::new(PluginId::new("dep"))
            .with_min_version(PluginVersion::new(2, 0, 0));
        assert!(!dep.matches(&PluginVersion::new(1, 9, 9)));
        let dep2 = PluginDependency::new(PluginId::new("dep"))
            .with_max_version(PluginVersion::new(2, 0, 0));
        assert!(!dep2.matches(&PluginVersion::new(2, 0, 1)));
    }

    #[test]
    fn dependency_matches_no_constraints() {
        let dep = PluginDependency::new(PluginId::new("dep"));
        assert!(dep.matches(&PluginVersion::new(99, 0, 0)));
    }

    #[test]
    fn dependency_serde_roundtrip() {
        let dep = PluginDependency::new(PluginId::new("serde.dep"))
            .optional()
            .with_min_version(PluginVersion::new(1, 0, 0));
        let json = serde_json::to_string(&dep).unwrap();
        let deserialized: PluginDependency = serde_json::from_str(&json).unwrap();
        assert_eq!(dep.plugin_id, deserialized.plugin_id);
        assert_eq!(dep.required, deserialized.required);
    }

    // ── PluginMetadata tests ─────────────────────────────────────────

    #[test]
    fn metadata_creation() {
        let author = PluginAuthor::new("author");
        let version = PluginVersion::new(1, 0, 0);
        let meta = PluginMetadata::new("my-plugin", version.clone(), author.clone());
        assert_eq!(meta.name, "my-plugin");
        assert_eq!(meta.version, version);
        assert_eq!(meta.author.name, "author");
    }

    #[test]
    fn metadata_builder() {
        let meta = PluginMetadata::new("p", PluginVersion::new(1, 0, 0), PluginAuthor::new("a"))
            .with_description("desc")
            .with_homepage("https://example.com")
            .with_license("MIT");
        assert_eq!(meta.description, "desc");
        assert_eq!(meta.homepage.unwrap(), "https://example.com");
        assert_eq!(meta.license.unwrap(), "MIT");
    }

    #[test]
    fn metadata_serde_roundtrip() {
        let meta = PluginMetadata::new(
            "serde-plugin",
            PluginVersion::new(0, 1, 0),
            PluginAuthor::new("serde author"),
        );
        let json = serde_json::to_string(&meta).unwrap();
        let deserialized: PluginMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(meta.name, deserialized.name);
    }

    // ── PluginHooks tests ────────────────────────────────────────────

    #[test]
    fn hooks_creation() {
        let hooks = PluginHooks::new();
        assert!(hooks.before_execution.is_empty());
        assert!(hooks.after_execution.is_empty());
    }

    #[test]
    fn hooks_builder() {
        let hooks = PluginHooks::new()
            .on_before_execution("pre_validate")
            .on_after_execution("post_log")
            .on_before_node("node_setup")
            .on_after_node("node_teardown")
            .on_execution_failed("error_handler")
            .on_execution_cancelled("cancel_cleanup");
        assert_eq!(hooks.before_execution, vec!["pre_validate"]);
        assert_eq!(hooks.after_execution, vec!["post_log"]);
        assert_eq!(hooks.before_node, vec!["node_setup"]);
        assert_eq!(hooks.after_node, vec!["node_teardown"]);
        assert_eq!(hooks.execution_failed, vec!["error_handler"]);
        assert_eq!(hooks.execution_cancelled, vec!["cancel_cleanup"]);
    }

    #[test]
    fn hooks_default_is_empty() {
        let hooks = PluginHooks::default();
        assert!(hooks.before_execution.is_empty());
    }

    #[test]
    fn hooks_serde_roundtrip() {
        let hooks = PluginHooks::new()
            .on_before_execution("hook1")
            .on_after_execution("hook2");
        let json = serde_json::to_string(&hooks).unwrap();
        let deserialized: PluginHooks = serde_json::from_str(&json).unwrap();
        assert_eq!(hooks.before_execution, deserialized.before_execution);
        assert_eq!(hooks.after_execution, deserialized.after_execution);
    }

    // ── PluginManifest tests ─────────────────────────────────────────

    #[test]
    fn manifest_creation() {
        let meta = PluginMetadata::new("p", PluginVersion::new(1, 0, 0), PluginAuthor::new("a"));
        let manifest = PluginManifest::new(meta.clone());
        assert_eq!(manifest.metadata.name, meta.name);
        assert!(manifest.capabilities.is_empty());
        assert!(manifest.dependencies.is_empty());
        assert!(manifest.permissions.is_empty());
    }

    #[test]
    fn manifest_builder() {
        let meta = PluginMetadata::new("p", PluginVersion::new(1, 0, 0), PluginAuthor::new("a"));
        let cap = PluginCapability::new(
            PluginCapabilityId::new("test.cap"),
            CapabilityMetadata::new("test.cap"),
        );
        let dep = PluginDependency::new(PluginId::new("other")).optional();
        let manifest = PluginManifest::new(meta)
            .with_capability(cap.clone())
            .with_dependency(dep.clone())
            .with_permission(PluginPermission::BrowserAccess)
            .with_hooks(PluginHooks::new().on_before_execution("h"));
        assert_eq!(manifest.capabilities.len(), 1);
        assert_eq!(manifest.dependencies.len(), 1);
        assert_eq!(manifest.permissions.len(), 1);
        assert_eq!(manifest.hooks.before_execution, vec!["h"]);
    }

    #[test]
    fn manifest_serde_roundtrip() {
        let meta = PluginMetadata::new("p", PluginVersion::new(1, 0, 0), PluginAuthor::new("a"));
        let manifest = PluginManifest::new(meta).with_permission(PluginPermission::NetworkAccess);
        let json = serde_json::to_string(&manifest).unwrap();
        let deserialized: PluginManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(manifest.permissions, deserialized.permissions);
    }

    // ── PluginExecutionMetadata tests ────────────────────────────────

    #[test]
    fn execution_metadata_creation() {
        let meta = PluginExecutionMetadata::new(PluginState::Discovered);
        assert_eq!(meta.execution_count, 0);
        assert_eq!(meta.state, PluginState::Discovered);
        assert!(meta.last_started_at.is_none());
        assert!(meta.last_error.is_none());
    }

    #[test]
    fn execution_metadata_serde_roundtrip() {
        let meta = PluginExecutionMetadata::new(PluginState::Running);
        let json = serde_json::to_string(&meta).unwrap();
        let deserialized: PluginExecutionMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(meta.state, deserialized.state);
    }

    // ── PluginContext tests ──────────────────────────────────────────

    #[test]
    fn plugin_context_creation() {
        let ctx = PluginContext::new(PluginId::new("test.plugin"));
        assert_eq!(ctx.plugin_id, PluginId::new("test.plugin"));
        assert!(ctx.execution_context.is_none());
        assert_eq!(ctx.metadata.state, PluginState::Discovered);
    }

    // ── PluginRegistration tests ─────────────────────────────────────

    #[test]
    fn registration_creation() {
        let id = PluginId::new("reg.plugin");
        let meta = PluginMetadata::new(
            "reg.plugin",
            PluginVersion::new(1, 0, 0),
            PluginAuthor::new("a"),
        );
        let manifest = PluginManifest::new(meta.clone());
        let reg = PluginRegistration::new(id.clone(), meta, manifest);
        assert_eq!(reg.plugin_id, id);
        assert_eq!(reg.state, PluginState::Discovered);
        assert_eq!(reg.status, PluginStatus::Unknown);
    }

    #[test]
    fn registration_serde_roundtrip() {
        let id = PluginId::new("serde.reg");
        let meta = PluginMetadata::new(
            "serde.reg",
            PluginVersion::new(1, 0, 0),
            PluginAuthor::new("a"),
        );
        let manifest = PluginManifest::new(meta.clone());
        let reg = PluginRegistration::new(id, meta, manifest);
        let json = serde_json::to_string(&reg).unwrap();
        let deserialized: PluginRegistration = serde_json::from_str(&json).unwrap();
        assert_eq!(reg.plugin_id, deserialized.plugin_id);
        assert_eq!(reg.state, deserialized.state);
    }

    // ── PluginValidationResult tests ─────────────────────────────────

    #[test]
    fn validation_result_valid_by_default() {
        let result = PluginValidationResult::new();
        assert!(result.valid);
    }

    #[test]
    fn validation_result_with_missing_dependency() {
        let dep = PluginDependency::new(PluginId::new("missing"));
        let result = PluginValidationResult::new().with_missing_dependency(dep.clone());
        assert!(!result.valid);
        assert_eq!(result.missing_dependencies, vec![dep]);
    }

    #[test]
    fn validation_result_with_error() {
        let result = PluginValidationResult::new().with_error("something went wrong");
        assert!(!result.valid);
        assert_eq!(result.errors, vec!["something went wrong"]);
    }

    #[test]
    fn validation_result_with_warning() {
        let result = PluginValidationResult::new().with_warning("this is a warning");
        assert!(result.valid);
        assert_eq!(result.warnings, vec!["this is a warning"]);
    }

    #[test]
    fn validation_result_serde_roundtrip() {
        let result = PluginValidationResult::new()
            .with_error("err1")
            .with_warning("warn1");
        let json = serde_json::to_string(&result).unwrap();
        let deserialized: PluginValidationResult = serde_json::from_str(&json).unwrap();
        assert_eq!(result.valid, deserialized.valid);
        assert_eq!(result.errors, deserialized.errors);
    }

    // ── PluginError tests ────────────────────────────────────────────

    #[test]
    fn plugin_error_not_found_display() {
        let err = PluginError::NotFound(PluginId::new("missing"));
        assert_eq!(err.to_string(), "plugin missing not found");
    }

    #[test]
    fn plugin_error_invalid_transition_display() {
        let err = PluginError::InvalidTransition {
            plugin_id: PluginId::new("p"),
            from: PluginState::Discovered,
            to: PluginState::Running,
        };
        assert!(err.to_string().contains("Discovered"));
        assert!(err.to_string().contains("Running"));
    }

    #[test]
    fn plugin_error_already_registered() {
        let err = PluginError::AlreadyRegistered(PluginId::new("dup"));
        assert_eq!(err.to_string(), "plugin dup already registered");
    }

    // ── PluginRegistry tests ─────────────────────────────────────────

    #[test]
    fn registry_register_plugin() {
        let registry = make_plugin_registry();
        let plugin = make_test_plugin("test.register");
        let reg = registry.register(plugin).unwrap();
        assert_eq!(reg.state, PluginState::Discovered);
        assert_eq!(reg.plugin_id, PluginId::new("test.register"));
    }

    #[test]
    fn registry_register_duplicate_fails() {
        let registry = make_plugin_registry();
        let plugin = make_test_plugin("test.dup");
        registry.register(plugin).unwrap();
        let duplicate = make_test_plugin("test.dup");
        let err = registry.register(duplicate).unwrap_err();
        match err {
            PluginError::AlreadyRegistered(ref id) => {
                assert_eq!(*id, PluginId::new("test.dup"));
            }
            _ => panic!("expected AlreadyRegistered error"),
        }
    }

    #[test]
    fn registry_lookup_found() {
        let registry = make_plugin_registry();
        let plugin = make_test_plugin("test.lookup");
        registry.register(plugin).unwrap();
        let found = registry.lookup(&PluginId::new("test.lookup"));
        assert!(found.is_some());
        assert_eq!(found.unwrap().plugin_id, PluginId::new("test.lookup"));
    }

    #[test]
    fn registry_lookup_not_found() {
        let registry = make_plugin_registry();
        let found = registry.lookup(&PluginId::new("nonexistent"));
        assert!(found.is_none());
    }

    #[test]
    fn registry_lookup_by_capability() {
        let registry = make_plugin_registry();
        let plugin = make_test_plugin("cap.plugin");
        registry.register(plugin).unwrap();
        let results =
            registry.lookup_by_capability(&PluginCapabilityId::new("cap.plugin.test_cap"));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].plugin_id, PluginId::new("cap.plugin"));
    }

    #[test]
    fn registry_lookup_by_capability_empty() {
        let registry = make_plugin_registry();
        let results = registry.lookup_by_capability(&PluginCapabilityId::new("no.such.cap"));
        assert!(results.is_empty());
    }

    #[test]
    fn registry_lookup_by_version() {
        let registry = make_plugin_registry();
        let author = PluginAuthor::new("v");
        let v1 = PluginVersion::new(1, 0, 0);
        let meta1 = PluginMetadata::new("v1", v1, author.clone());
        let plugin1 = MockPlugin {
            metadata: meta1,
            manifest: PluginManifest::new(PluginMetadata::new(
                "v1",
                PluginVersion::new(1, 0, 0),
                author.clone(),
            )),
            health_status: std::sync::RwLock::new(PluginStatus::Healthy),
            init_called: AtomicU64::new(0),
            shutdown_called: AtomicU64::new(0),
        };
        registry.register(Arc::new(plugin1)).unwrap();

        let meta2 = PluginMetadata::new("v2", PluginVersion::new(2, 0, 0), author);
        let plugin2 = MockPlugin {
            metadata: meta2,
            manifest: PluginManifest::new(PluginMetadata::new(
                "v2",
                PluginVersion::new(2, 0, 0),
                PluginAuthor::new("v"),
            )),
            health_status: std::sync::RwLock::new(PluginStatus::Healthy),
            init_called: AtomicU64::new(0),
            shutdown_called: AtomicU64::new(0),
        };
        registry.register(Arc::new(plugin2)).unwrap();

        let results = registry.lookup_by_version(&PluginVersion::new(1, 5, 0));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].plugin_id, PluginId::new("v2"));
    }

    #[test]
    fn registry_lookup_by_state() {
        let registry = make_plugin_registry();
        let plugin = make_test_plugin("state.plugin");
        registry.register(plugin).unwrap();

        let discovered = registry.lookup_by_state(PluginState::Discovered);
        assert_eq!(discovered.len(), 1);

        let running = registry.lookup_by_state(PluginState::Running);
        assert!(running.is_empty());
    }

    #[test]
    fn registry_list_plugins() {
        let registry = make_plugin_registry();
        registry.register(make_test_plugin("b")).unwrap();
        registry.register(make_test_plugin("a")).unwrap();
        let plugins = registry.list_plugins();
        assert_eq!(plugins.len(), 2);
        assert_eq!(plugins[0].plugin_id, PluginId::new("a"));
        assert_eq!(plugins[1].plugin_id, PluginId::new("b"));
    }

    #[test]
    fn registry_list_capabilities() {
        let registry = make_plugin_registry();
        registry.register(make_test_plugin("cap.a")).unwrap();
        registry.register(make_test_plugin("cap.b")).unwrap();
        let caps = registry.list_capabilities();
        assert_eq!(caps.len(), 2);
    }

    #[test]
    fn registry_list_capabilities_dedup() {
        let registry = make_plugin_registry();
        let author = PluginAuthor::new("a");
        let version = PluginVersion::new(1, 0, 0);
        let shared_cap = PluginCapability::new(
            PluginCapabilityId::new("shared.cap"),
            CapabilityMetadata::new("shared.cap"),
        );
        let meta1 = PluginMetadata::new("p1", version.clone(), author.clone());
        let manifest1 = PluginManifest::new(meta1.clone()).with_capability(shared_cap.clone());
        let plugin1 = MockPlugin {
            metadata: meta1,
            manifest: manifest1,
            health_status: std::sync::RwLock::new(PluginStatus::Healthy),
            init_called: AtomicU64::new(0),
            shutdown_called: AtomicU64::new(0),
        };
        registry.register(Arc::new(plugin1)).unwrap();

        let meta2 = PluginMetadata::new("p2", version, author);
        let manifest2 = PluginManifest::new(meta2.clone()).with_capability(shared_cap);
        let plugin2 = MockPlugin {
            metadata: meta2,
            manifest: manifest2,
            health_status: std::sync::RwLock::new(PluginStatus::Healthy),
            init_called: AtomicU64::new(0),
            shutdown_called: AtomicU64::new(0),
        };
        registry.register(Arc::new(plugin2)).unwrap();

        let caps = registry.list_capabilities();
        assert_eq!(caps.len(), 1);
    }

    #[test]
    fn registry_transition_valid() {
        let registry = make_plugin_registry();
        let plugin = make_test_plugin("transit");
        registry.register(plugin).unwrap();
        let id = PluginId::new("transit");

        let next = registry
            .transition_to(&id, PluginState::Registered)
            .unwrap();
        assert_eq!(next, PluginState::Registered);
    }

    #[test]
    fn registry_transition_invalid() {
        let registry = make_plugin_registry();
        let plugin = make_test_plugin("bad.transit");
        registry.register(plugin).unwrap();
        let id = PluginId::new("bad.transit");

        let err = registry
            .transition_to(&id, PluginState::Running)
            .unwrap_err();
        match err {
            PluginError::InvalidTransition { .. } => {}
            _ => panic!("expected InvalidTransition error"),
        }
    }

    #[test]
    fn registry_transition_nonexistent() {
        let registry = make_plugin_registry();
        let err = registry
            .transition_to(&PluginId::new("ghost"), PluginState::Registered)
            .unwrap_err();
        match err {
            PluginError::NotFound(ref id) => assert_eq!(*id, PluginId::new("ghost")),
            _ => panic!("expected NotFound error"),
        }
    }

    #[test]
    fn registry_unload_full_lifecycle() {
        let registry = make_plugin_registry();
        let plugin = make_test_plugin("full.life");
        registry.register(plugin).unwrap();
        let id = PluginId::new("full.life");

        registry
            .transition_to(&id, PluginState::Registered)
            .unwrap();
        registry.transition_to(&id, PluginState::Validated).unwrap();
        registry
            .transition_to(&id, PluginState::Initialized)
            .unwrap();
        registry.transition_to(&id, PluginState::Running).unwrap();
        registry.transition_to(&id, PluginState::Disabled).unwrap();

        let next = registry.transition_to(&id, PluginState::Unloaded).unwrap();
        assert_eq!(next, PluginState::Unloaded);

        registry.unregister(&id).unwrap();
        assert!(registry.lookup(&id).is_none());
    }

    #[test]
    fn registry_unregister_fails_before_unloaded() {
        let registry = make_plugin_registry();
        let plugin = make_test_plugin("no.unload");
        registry.register(plugin).unwrap();
        let id = PluginId::new("no.unload");

        let err = registry.unregister(&id).unwrap_err();
        match err {
            PluginError::InvalidState {
                ref plugin_id,
                current,
                ..
            } => {
                assert_eq!(*plugin_id, id);
                assert_eq!(current, PluginState::Discovered);
            }
            _ => panic!("expected InvalidState error"),
        }
    }

    #[test]
    fn registry_verify_dependencies_success() {
        let registry = make_plugin_registry();
        let dep_plugin = make_test_plugin("dependency");
        registry.register(dep_plugin).unwrap();

        let dep = PluginDependency::new(PluginId::new("dependency"));
        let author = PluginAuthor::new("main");
        let meta = PluginMetadata::new("main", PluginVersion::new(1, 0, 0), author);
        let manifest = PluginManifest::new(meta).with_dependency(dep);
        let plugin = MockPlugin {
            metadata: manifest.metadata.clone(),
            manifest,
            health_status: std::sync::RwLock::new(PluginStatus::Healthy),
            init_called: AtomicU64::new(0),
            shutdown_called: AtomicU64::new(0),
        };
        registry.register(Arc::new(plugin)).unwrap();

        let result = registry
            .verify_dependencies(&PluginId::new("main"))
            .unwrap();
        assert!(result.valid);
    }

    #[test]
    fn registry_verify_dependencies_missing() {
        let registry = make_plugin_registry();
        let dep = PluginDependency::new(PluginId::new("missing.dep"));
        let author = PluginAuthor::new("main");
        let meta = PluginMetadata::new("main", PluginVersion::new(1, 0, 0), author);
        let manifest = PluginManifest::new(meta).with_dependency(dep.clone());
        let plugin = MockPlugin {
            metadata: manifest.metadata.clone(),
            manifest,
            health_status: std::sync::RwLock::new(PluginStatus::Healthy),
            init_called: AtomicU64::new(0),
            shutdown_called: AtomicU64::new(0),
        };
        registry.register(Arc::new(plugin)).unwrap();

        let result = registry
            .verify_dependencies(&PluginId::new("main"))
            .unwrap();
        assert!(!result.valid);
        assert_eq!(result.missing_dependencies.len(), 1);
    }

    #[test]
    fn registry_verify_dependencies_version_mismatch() {
        let registry = make_plugin_registry();
        let author = PluginAuthor::new("dep");
        let dep_meta = PluginMetadata::new("dep", PluginVersion::new(1, 0, 0), author);
        let dep_manifest = PluginManifest::new(dep_meta);
        let dep_plugin = MockPlugin {
            metadata: dep_manifest.metadata.clone(),
            manifest: dep_manifest,
            health_status: std::sync::RwLock::new(PluginStatus::Healthy),
            init_called: AtomicU64::new(0),
            shutdown_called: AtomicU64::new(0),
        };
        registry.register(Arc::new(dep_plugin)).unwrap();

        let dep = PluginDependency::new(PluginId::new("dep"))
            .with_min_version(PluginVersion::new(2, 0, 0));
        let main_author = PluginAuthor::new("main");
        let meta = PluginMetadata::new("main", PluginVersion::new(1, 0, 0), main_author);
        let manifest = PluginManifest::new(meta).with_dependency(dep);
        let main_plugin = MockPlugin {
            metadata: manifest.metadata.clone(),
            manifest,
            health_status: std::sync::RwLock::new(PluginStatus::Healthy),
            init_called: AtomicU64::new(0),
            shutdown_called: AtomicU64::new(0),
        };
        registry.register(Arc::new(main_plugin)).unwrap();

        let result = registry
            .verify_dependencies(&PluginId::new("main"))
            .unwrap();
        assert!(!result.valid);
        assert_eq!(result.version_mismatches.len(), 1);
    }

    #[test]
    fn registry_verify_dependencies_optional_missing_produces_warning() {
        let registry = make_plugin_registry();
        let dep = PluginDependency::new(PluginId::new("optional.dep")).optional();
        let author = PluginAuthor::new("main");
        let meta = PluginMetadata::new("main", PluginVersion::new(1, 0, 0), author);
        let manifest = PluginManifest::new(meta).with_dependency(dep);
        let plugin = MockPlugin {
            metadata: manifest.metadata.clone(),
            manifest,
            health_status: std::sync::RwLock::new(PluginStatus::Healthy),
            init_called: AtomicU64::new(0),
            shutdown_called: AtomicU64::new(0),
        };
        registry.register(Arc::new(plugin)).unwrap();

        let result = registry
            .verify_dependencies(&PluginId::new("main"))
            .unwrap();
        assert!(result.valid);
        assert_eq!(result.warnings.len(), 1);
    }

    #[test]
    fn registry_verify_permissions_allowed() {
        let registry = make_plugin_registry();
        let author = PluginAuthor::new("perm");
        let meta = PluginMetadata::new("perm.plugin", PluginVersion::new(1, 0, 0), author);
        let manifest = PluginManifest::new(meta).with_permission(PluginPermission::BrowserAccess);
        let plugin = MockPlugin {
            metadata: manifest.metadata.clone(),
            manifest,
            health_status: std::sync::RwLock::new(PluginStatus::Healthy),
            init_called: AtomicU64::new(0),
            shutdown_called: AtomicU64::new(0),
        };
        registry.register(Arc::new(plugin)).unwrap();

        let allowed = vec![PluginPermission::BrowserAccess];
        let result = registry
            .verify_permissions(&PluginId::new("perm.plugin"), &allowed)
            .unwrap();
        assert!(result.valid);
    }

    #[test]
    fn registry_verify_permissions_denied() {
        let registry = make_plugin_registry();
        let author = PluginAuthor::new("perm");
        let meta = PluginMetadata::new("perm.plugin", PluginVersion::new(1, 0, 0), author);
        let manifest = PluginManifest::new(meta).with_permission(PluginPermission::NetworkAccess);
        let plugin = MockPlugin {
            metadata: manifest.metadata.clone(),
            manifest,
            health_status: std::sync::RwLock::new(PluginStatus::Healthy),
            init_called: AtomicU64::new(0),
            shutdown_called: AtomicU64::new(0),
        };
        registry.register(Arc::new(plugin)).unwrap();

        let allowed = vec![PluginPermission::BrowserAccess];
        let result = registry
            .verify_permissions(&PluginId::new("perm.plugin"), &allowed)
            .unwrap();
        assert!(!result.valid);
        assert_eq!(result.missing_permissions.len(), 1);
    }

    #[test]
    fn registry_health_inspect() {
        let registry = make_plugin_registry();
        let author = PluginAuthor::new("h");
        let meta = PluginMetadata::new("healthy", PluginVersion::new(1, 0, 0), author);
        let manifest = PluginManifest::new(meta);
        let healthy = MockPlugin {
            metadata: manifest.metadata.clone(),
            manifest,
            health_status: std::sync::RwLock::new(PluginStatus::Healthy),
            init_called: AtomicU64::new(0),
            shutdown_called: AtomicU64::new(0),
        };
        registry.register(Arc::new(healthy)).unwrap();

        let degraded_plugin = make_test_plugin("degraded");
        // Can't easily make this degraded without changing MockPlugin... let's register a separate one
        registry.register(degraded_plugin).unwrap();

        let results = registry.health_inspect();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn registry_validate_plugin() {
        let registry = make_plugin_registry();
        let plugin: Arc<dyn Plugin> = Arc::new(FailingMockPlugin::new("validate.test"));
        registry.register(Arc::clone(&plugin)).unwrap();

        let result = registry
            .validate_plugin(&PluginId::new("validate.test"))
            .unwrap();
        assert!(!result.valid);
    }

    #[test]
    fn registry_initialize_plugin() {
        let registry = make_plugin_registry();
        let id = PluginId::new("init.test");
        let plugin = make_test_plugin("init.test");
        registry.register(Arc::clone(&plugin)).unwrap();

        let ctx = PluginContext::new(id.clone());
        let result = registry.initialize_plugin(&id, &ctx);
        assert!(result.is_ok());
    }

    #[test]
    fn registry_shutdown_plugin() {
        let registry = make_plugin_registry();
        let id = PluginId::new("shutdown.test");
        let plugin = make_test_plugin("shutdown.test");
        registry.register(Arc::clone(&plugin)).unwrap();

        let result = registry.shutdown_plugin(&id);
        assert!(result.is_ok());
    }

    #[test]
    fn registry_nonexistent_operations() {
        let registry = make_plugin_registry();
        let ghost = PluginId::new("ghost");

        assert!(registry.validate_plugin(&ghost).is_err());
        assert!(registry
            .initialize_plugin(&ghost, &PluginContext::new(ghost.clone()))
            .is_err());
        assert!(registry.shutdown_plugin(&ghost).is_err());
    }

    // ── Send + Sync tests ────────────────────────────────────────────

    #[test]
    fn plugin_registry_is_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<PluginRegistry>();
        assert_sync::<PluginRegistry>();
    }

    #[test]
    fn plugin_trait_is_object_safe() {
        fn assert_object_safe(_: Arc<dyn Plugin>) {}
        let plugin = make_test_plugin("object.safe");
        assert_object_safe(plugin);
    }

    // ── Empty registry tests ─────────────────────────────────────────

    #[test]
    fn registry_empty_list() {
        let registry = make_plugin_registry();
        assert!(registry.list_plugins().is_empty());
        assert!(registry.list_capabilities().is_empty());
        assert!(registry.health_inspect().is_empty());
    }

    // ── Cycle tests ──────────────────────────────────────────────────

    #[test]
    fn registry_running_paused_running() {
        let registry = make_plugin_registry();
        let plugin = make_test_plugin("cycle.test");
        registry.register(plugin).unwrap();
        let id = PluginId::new("cycle.test");

        registry
            .transition_to(&id, PluginState::Registered)
            .unwrap();
        registry.transition_to(&id, PluginState::Validated).unwrap();
        registry
            .transition_to(&id, PluginState::Initialized)
            .unwrap();
        registry.transition_to(&id, PluginState::Running).unwrap();
        registry.transition_to(&id, PluginState::Paused).unwrap();
        registry.transition_to(&id, PluginState::Running).unwrap();

        let state = registry.lookup(&id).unwrap().state;
        assert_eq!(state, PluginState::Running);
    }

    // ── Full lifecycle via helper ────────────────────────────────────

    #[test]
    fn registry_full_lifecycle_to_failed() {
        let registry = make_plugin_registry();
        let plugin: Arc<dyn Plugin> = Arc::new(FailingMockPlugin::new("fails.test"));
        registry.register(Arc::clone(&plugin)).unwrap();
        let id = PluginId::new("fails.test");

        registry.transition_to(&id, PluginState::Failed).unwrap();
        let state = registry.lookup(&id).unwrap().state;
        assert_eq!(state, PluginState::Failed);

        registry.transition_to(&id, PluginState::Unloaded).unwrap();
        registry.unregister(&id).unwrap();
        assert!(registry.lookup(&id).is_none());
    }

    // ── PluginValidationResult defaults ──────────────────────────────

    #[test]
    fn plugin_validation_result_default_is_valid() {
        let result = PluginValidationResult::default();
        assert!(result.valid);
        assert!(result.errors.is_empty());
    }
}
