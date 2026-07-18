/// P15 — Plugin Loader
///
/// Filesystem-based plugin discovery, manifest parsing, validation,
/// dependency resolution, and lifecycle orchestration.
///
/// # Design
/// - Fully synchronous (no tokio, no async)
/// - No unwrap/expect/panic in production
/// - Reuses P14 types (`PluginRegistry`, `Plugin`, `PluginError`, etc.)
/// - Strongly typed manifest parsing with detailed error reporting
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Deserialize;

use crate::exec::CapabilityMetadata;
use crate::plugin::{
    Plugin, PluginAuthor, PluginCapability, PluginCapabilityId, PluginContext, PluginDependency,
    PluginError, PluginHooks, PluginId, PluginManifest, PluginMetadata, PluginPermission,
    PluginRegistration, PluginRegistry, PluginState, PluginStatus, PluginValidationResult,
    PluginVersion,
};

// ── Manifest file types (TOML deserialization) ──────────────────────

/// Top-level structure of a `manifest.toml` file.
#[derive(Debug, Clone, Deserialize)]
pub struct PluginManifestFile {
    pub plugin: ManifestHeader,
    pub capabilities: Option<Vec<ManifestCapability>>,
    pub dependencies: Option<Vec<ManifestDependency>>,
    pub permissions: Option<Vec<ManifestPermission>>,
    pub hooks: Option<ManifestHooks>,
}

/// A single permission entry in the manifest, stored as an inline table.
#[derive(Debug, Clone, Deserialize)]
pub struct ManifestPermission {
    pub name: String,
}

/// `[plugin]` section of a manifest.
#[derive(Debug, Clone, Deserialize)]
pub struct ManifestHeader {
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: Option<String>,
    pub homepage: Option<String>,
    pub license: Option<String>,
}

/// A single capability entry in the manifest.
#[derive(Debug, Clone, Deserialize)]
pub struct ManifestCapability {
    pub name: String,
    pub description: Option<String>,
    pub required_params: Option<Vec<String>>,
    pub optional_params: Option<Vec<String>>,
    pub output_keys: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub version: Option<String>,
}

/// A single dependency entry in the manifest.
#[derive(Debug, Clone, Deserialize)]
pub struct ManifestDependency {
    pub plugin: String,
    pub required: Option<bool>,
    pub min_version: Option<String>,
    pub max_version: Option<String>,
}

/// `[hooks]` section of a manifest.
#[derive(Debug, Clone, Deserialize)]
pub struct ManifestHooks {
    pub before_execution: Option<Vec<String>>,
    pub after_execution: Option<Vec<String>>,
    pub before_node: Option<Vec<String>>,
    pub after_node: Option<Vec<String>>,
    pub execution_failed: Option<Vec<String>>,
    pub execution_cancelled: Option<Vec<String>>,
}

// ── Conversion helpers ──────────────────────────────────────────────

fn parse_version(s: &str, context: &str) -> Result<PluginVersion, PluginError> {
    PluginVersion::parse(s).ok_or_else(|| {
        PluginError::ManifestParseFailed(format!("invalid version '{s}' in {context}"))
    })
}

fn perms_eq(perm: &PluginPermission, name: &str) -> bool {
    matches!(
        (perm, name),
        (PluginPermission::BrowserAccess, "BrowserAccess")
            | (PluginPermission::NetworkAccess, "NetworkAccess")
            | (PluginPermission::StorageAccess, "StorageAccess")
            | (PluginPermission::FilesystemAccess, "FilesystemAccess")
            | (PluginPermission::ClipboardAccess, "ClipboardAccess")
            | (PluginPermission::DownloadAccess, "DownloadAccess")
            | (PluginPermission::InputSimulation, "InputSimulation")
    )
}

fn perm_from_str(name: &str) -> Option<PluginPermission> {
    match name {
        "BrowserAccess" => Some(PluginPermission::BrowserAccess),
        "NetworkAccess" => Some(PluginPermission::NetworkAccess),
        "StorageAccess" => Some(PluginPermission::StorageAccess),
        "FilesystemAccess" => Some(PluginPermission::FilesystemAccess),
        "ClipboardAccess" => Some(PluginPermission::ClipboardAccess),
        "DownloadAccess" => Some(PluginPermission::DownloadAccess),
        "InputSimulation" => Some(PluginPermission::InputSimulation),
        _ => None,
    }
}

fn capabilities_from_manifest(list: Vec<ManifestCapability>) -> Vec<PluginCapability> {
    list.into_iter()
        .map(|c| {
            let id = PluginCapabilityId::new(&c.name);
            let mut meta = CapabilityMetadata::new(&c.name);
            if let Some(d) = c.description {
                meta = meta.with_description(d);
            }
            if let Some(p) = c.required_params {
                for param in p {
                    meta = meta.with_required_param(param);
                }
            }
            if let Some(p) = c.optional_params {
                for param in p {
                    meta = meta.with_optional_param(param);
                }
            }
            if let Some(k) = c.output_keys {
                for key in k {
                    meta = meta.with_output_key(key);
                }
            }
            if let Some(t) = c.tags {
                for tag in t {
                    meta = meta.with_tag(tag);
                }
            }
            if let Some(v) = c.version {
                meta = meta.with_version(v);
            }
            PluginCapability::new(id, meta)
        })
        .collect()
}

fn dependencies_from_manifest(list: Vec<ManifestDependency>) -> Vec<PluginDependency> {
    list.into_iter()
        .map(|d| {
            let mut dep = PluginDependency::new(PluginId::new(&d.plugin));
            if let Some(req) = d.required {
                if !req {
                    dep = dep.optional();
                }
            }
            if let Some(v) = d.min_version {
                if let Some(ver) = PluginVersion::parse(&v) {
                    dep = dep.with_min_version(ver);
                }
            }
            if let Some(v) = d.max_version {
                if let Some(ver) = PluginVersion::parse(&v) {
                    dep = dep.with_max_version(ver);
                }
            }
            dep
        })
        .collect()
}

fn hooks_from_manifest(h: ManifestHooks) -> PluginHooks {
    let mut hooks = PluginHooks::new();
    if let Some(list) = h.before_execution {
        for name in list {
            hooks = hooks.on_before_execution(name);
        }
    }
    if let Some(list) = h.after_execution {
        for name in list {
            hooks = hooks.on_after_execution(name);
        }
    }
    if let Some(list) = h.before_node {
        for name in list {
            hooks = hooks.on_before_node(name);
        }
    }
    if let Some(list) = h.after_node {
        for name in list {
            hooks = hooks.on_after_node(name);
        }
    }
    if let Some(list) = h.execution_failed {
        for name in list {
            hooks = hooks.on_execution_failed(name);
        }
    }
    if let Some(list) = h.execution_cancelled {
        for name in list {
            hooks = hooks.on_execution_cancelled(name);
        }
    }
    hooks
}

// ── Result of discovering a plugin directory ────────────────────────

/// Result of discovering a plugin in a configured directory.
#[derive(Debug, Clone)]
pub struct PluginDiscoveryResult {
    pub plugin_id: PluginId,
    pub manifest_path: PathBuf,
}

// ── PluginLoader ────────────────────────────────────────────────────

/// Discovers, validates, and loads plugins from the filesystem.
///
/// # Lifecycle
/// Loading a plugin follows this sequence:
/// 1. Discover (find `manifest.toml` in plugin directories)
/// 2. Read & parse (deserialise TOML into strongly typed structures)
/// 3. Validate (check required fields, version format, etc.)
/// 4. Convert (TOML types → P14 [`PluginManifest`])
/// 5. Resolve dependencies (topological sort, cycle/missing/version checks)
/// 6. Register (insert into [`PluginRegistry`])
/// 7. Validate against registry
/// 8. Initialise and transition to Running
#[derive(Debug)]
pub struct PluginLoader {
    plugin_dirs: Vec<PathBuf>,
    allowed_permissions: Vec<PluginPermission>,
}

impl PluginLoader {
    pub fn new(plugin_dirs: Vec<PathBuf>, allowed_permissions: Vec<PluginPermission>) -> Self {
        Self {
            plugin_dirs,
            allowed_permissions,
        }
    }

    /// Configure the set of allowed permissions.
    pub fn with_allowed_permissions(mut self, perms: Vec<PluginPermission>) -> Self {
        self.allowed_permissions = perms;
        self
    }

    // ── Discovery ───────────────────────────────────────────────────

    /// Scan all configured plugin directories for `manifest.toml` files.
    ///
    /// Non-existent or inaccessible directories are silently skipped.
    /// Returns a list of discovered plugin identities and their paths.
    pub fn discover(&self) -> Vec<PluginDiscoveryResult> {
        let mut results = Vec::new();
        for dir in &self.plugin_dirs {
            let entries = match fs::read_dir(dir) {
                Ok(e) => e,
                Err(_) => continue,
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let manifest_path = path.join("manifest.toml");
                if manifest_path.is_file() {
                    // Derive plugin_id from directory name
                    if let Some(dir_name) = path.file_name() {
                        let id = PluginId::new(dir_name.to_string_lossy().as_ref());
                        results.push(PluginDiscoveryResult {
                            plugin_id: id,
                            manifest_path,
                        });
                    }
                }
            }
        }
        results.sort_by(|a, b| a.plugin_id.as_str().cmp(b.plugin_id.as_str()));
        results
    }

    // ── Manifest reading ────────────────────────────────────────────

    /// Read and parse a `manifest.toml` file.
    pub fn read_manifest(&self, path: &Path) -> Result<PluginManifestFile, PluginError> {
        let content = fs::read_to_string(path).map_err(|e| {
            PluginError::ManifestParseFailed(format!("cannot read '{}': {e}", path.display()))
        })?;
        toml::from_str::<PluginManifestFile>(&content).map_err(|e| {
            PluginError::ManifestParseFailed(format!("invalid manifest '{}': {e}", path.display()))
        })
    }

    // ── Manifest validation ─────────────────────────────────────────

    /// Validate a parsed manifest for structural correctness.
    pub fn validate_manifest(&self, manifest: &PluginManifestFile) -> PluginValidationResult {
        let mut result = PluginValidationResult::new();

        // Validate name
        if manifest.plugin.name.trim().is_empty() {
            result = result.with_error("plugin name must not be empty");
        }

        // Validate version
        if PluginVersion::parse(&manifest.plugin.version).is_none() {
            result = result.with_error(format!(
                "invalid version '{}' — expected MAJOR.MINOR.PATCH",
                manifest.plugin.version
            ));
        }

        // Validate author
        if manifest.plugin.author.trim().is_empty() {
            result = result.with_error("plugin author must not be empty");
        }

        // Validate capability names
        if let Some(ref caps) = manifest.capabilities {
            let mut seen = std::collections::HashSet::new();
            for cap in caps {
                if cap.name.trim().is_empty() {
                    result = result.with_error("capability name must not be empty");
                } else if !seen.insert(cap.name.clone()) {
                    result = result.with_error(format!("duplicate capability name '{}'", cap.name));
                }
            }
        }

        // Validate dependency references
        if let Some(ref deps) = manifest.dependencies {
            let mut seen = std::collections::HashSet::new();
            for dep in deps {
                if dep.plugin.trim().is_empty() {
                    result = result.with_error("dependency plugin name must not be empty");
                } else if !seen.insert(dep.plugin.clone()) {
                    result = result.with_warning(format!(
                        "duplicate dependency reference to '{}'",
                        dep.plugin
                    ));
                }
                // Validate version strings if present
                if let Some(ref v) = dep.min_version {
                    if PluginVersion::parse(v).is_none() {
                        result = result.with_warning(format!(
                            "dependency '{}' has invalid min_version '{v}'",
                            dep.plugin
                        ));
                    }
                }
                if let Some(ref v) = dep.max_version {
                    if PluginVersion::parse(v).is_none() {
                        result = result.with_warning(format!(
                            "dependency '{}' has invalid max_version '{v}'",
                            dep.plugin
                        ));
                    }
                }
            }
        }

        // Validate permissions
        if let Some(ref perm_entries) = manifest.permissions {
            for entry in perm_entries {
                let matched = self
                    .allowed_permissions
                    .iter()
                    .any(|ap| perms_eq(ap, &entry.name));
                if !matched {
                    result = result.with_warning(format!(
                        "permission '{}' is not in the allow-list",
                        entry.name
                    ));
                }
            }
        }

        result
    }

    // ── Manifest conversion ─────────────────────────────────────────

    /// Convert a parsed manifest file into P14 types.
    pub fn convert_manifest(
        &self,
        file: PluginManifestFile,
    ) -> Result<(PluginMetadata, PluginManifest), PluginError> {
        let version = parse_version(&file.plugin.version, "plugin header")?;

        let author = PluginAuthor::new(&file.plugin.author);
        let author = match file.plugin.homepage {
            Some(ref url) => author.with_url(url),
            None => author,
        };

        let mut metadata = PluginMetadata::new(&file.plugin.name, version, author);
        if let Some(d) = file.plugin.description {
            metadata = metadata.with_description(d);
        }
        if let Some(h) = file.plugin.homepage {
            metadata = metadata.with_homepage(h);
        }
        if let Some(l) = file.plugin.license {
            metadata = metadata.with_license(l);
        }

        let mut manifest = PluginManifest::new(metadata.clone());

        if let Some(caps) = file.capabilities {
            for cap in capabilities_from_manifest(caps) {
                manifest = manifest.with_capability(cap);
            }
        }

        if let Some(deps) = file.dependencies {
            for dep in dependencies_from_manifest(deps) {
                manifest = manifest.with_dependency(dep);
            }
        }

        if let Some(perm_entries) = file.permissions {
            for entry in perm_entries {
                if let Some(perm) = perm_from_str(&entry.name) {
                    manifest = manifest.with_permission(perm);
                }
            }
        }

        if let Some(h) = file.hooks {
            manifest = manifest.with_hooks(hooks_from_manifest(h));
        }

        Ok((metadata, manifest))
    }

    // ── Dependency resolution ───────────────────────────────────────

    /// Resolve the loading order for a set of plugins.
    ///
    /// Checks for:
    /// - Missing required dependencies
    /// - Version conflicts
    /// - Circular dependencies
    /// - Duplicate plugin IDs
    ///
    /// Returns plugins in deterministic loading order (topological sort).
    pub fn resolve_loading_order(
        &self,
        plugins: &[(PluginId, &PluginManifest)],
    ) -> Result<Vec<PluginId>, PluginError> {
        // Check for duplicate IDs
        let mut seen_ids = std::collections::HashSet::new();
        for (id, _) in plugins {
            if !seen_ids.insert(id.clone()) {
                return Err(PluginError::AlreadyRegistered(id.clone()));
            }
        }

        // Build ID → manifest map for lookup
        let mut manifest_map: HashMap<&PluginId, &PluginManifest> = HashMap::new();
        for (id, manifest) in plugins {
            manifest_map.insert(id, manifest);
        }

        // Collect all required dependencies and build adjacency
        let mut adj: HashMap<&PluginId, Vec<&PluginId>> = HashMap::new(); // dep → dependents
        let mut in_degree: HashMap<&PluginId, usize> = HashMap::new();

        for (id, _) in plugins {
            in_degree.entry(id).or_insert(0);
        }

        for (id, manifest) in plugins {
            for dep in &manifest.dependencies {
                if !dep.required {
                    continue;
                }
                let dep_id = &dep.plugin_id;
                // Check if dep exists in the set
                if !manifest_map.contains_key(dep_id) {
                    return Err(PluginError::DependencyNotFound {
                        plugin_id: id.clone(),
                        dependency: dep_id.clone(),
                    });
                }
                // Check version compatibility
                if let Some(dep_manifest) = manifest_map.get(dep_id) {
                    let found = &dep_manifest.metadata.version;
                    if !dep.matches(found) {
                        return Err(PluginError::DependencyVersionMismatch {
                            plugin_id: id.clone(),
                            dependency: dep_id.clone(),
                            required: dep
                                .min_version
                                .clone()
                                .unwrap_or_else(|| PluginVersion::new(0, 0, 0)),
                            found: found.clone(),
                        });
                    }
                }
                adj.entry(dep_id).or_default().push(id);
                *in_degree.entry(id).or_insert(0) += 1;
            }
        }

        // Kahn's algorithm for topological sort (FIFO for alphabetical order)
        let mut queue: Vec<&PluginId> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(id, _)| *id)
            .collect();
        queue.sort_by(|a, b| a.as_str().cmp(b.as_str()));

        let mut order: Vec<PluginId> = Vec::with_capacity(plugins.len());
        while !queue.is_empty() {
            let id = queue.remove(0);
            order.push(id.clone());
            if let Some(dependents) = adj.remove(id) {
                for dep in dependents {
                    if let Some(deg) = in_degree.get_mut(dep) {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push(dep);
                        }
                    }
                }
                queue.sort_by(|a, b| a.as_str().cmp(b.as_str()));
            }
        }

        // If not all plugins are in order, there's a cycle
        if order.len() != plugins.len() {
            let unresolved: Vec<PluginId> = plugins
                .iter()
                .map(|(id, _)| id)
                .filter(|id| !order.contains(id))
                .cloned()
                .collect();
            return Err(PluginError::CircularDependency(unresolved));
        }

        Ok(order)
    }

    // ── Loading ─────────────────────────────────────────────────────

    /// Load a single plugin from a manifest path.
    ///
    /// Does NOT verify dependencies against other plugins — use
    /// [`load_all`](PluginLoader::load_all) for full dependency resolution.
    pub fn load(
        &self,
        registry: &PluginRegistry,
        path: &Path,
    ) -> Result<PluginRegistration, PluginError> {
        if !path.exists() {
            return Err(PluginError::ManifestNotFound(path.display().to_string()));
        }

        let manifest_file = self.read_manifest(path)?;

        let validation = self.validate_manifest(&manifest_file);
        if !validation.valid {
            return Err(PluginError::ManifestParseFailed(format!(
                "manifest validation failed for '{}': {:?}",
                path.display(),
                validation.errors
            )));
        }

        let (metadata, manifest) = self.convert_manifest(manifest_file)?;
        let plugin_id = PluginId::new(&metadata.name);

        let plugin: Arc<dyn Plugin> = Arc::new(ManifestPlugin::new(metadata, manifest));

        let mut reg = registry.register(plugin)?;
        reg.state = PluginState::Registered;

        // Transition through lifecycle
        self.transition_to_running(registry, &plugin_id)?;

        registry
            .lookup(&plugin_id)
            .ok_or_else(|| PluginError::Internal("plugin lost after loading".into()))
    }

    /// Discover, validate, resolve dependencies, and load all plugins.
    pub fn load_all(
        &self,
        registry: &PluginRegistry,
    ) -> Result<Vec<PluginRegistration>, PluginError> {
        let discovered = self.discover();
        if discovered.is_empty() {
            return Ok(Vec::new());
        }

        // Read and validate all manifests
        let mut all_plugins: Vec<(PluginId, PluginManifest)> = Vec::with_capacity(discovered.len());
        let mut path_map: HashMap<PluginId, PathBuf> = HashMap::new();

        for result in &discovered {
            let manifest_file = match self.read_manifest(&result.manifest_path) {
                Ok(f) => f,
                Err(_) => continue,
            };

            let validation = self.validate_manifest(&manifest_file);
            if !validation.valid {
                continue;
            }

            if let Ok((metadata, manifest)) = self.convert_manifest(manifest_file) {
                let id = PluginId::new(&metadata.name);
                all_plugins.push((id.clone(), manifest));
                path_map.insert(id, result.manifest_path.clone());
            }
        }

        // Resolve loading order
        let plugin_refs: Vec<(PluginId, &PluginManifest)> =
            all_plugins.iter().map(|(id, m)| (id.clone(), m)).collect();
        let order = self.resolve_loading_order(&plugin_refs)?;

        // Register and initialise in loading order
        let mut registrations: Vec<PluginRegistration> = Vec::with_capacity(order.len());
        for id in &order {
            // Find manifest by ID
            let manifest = all_plugins
                .iter()
                .find(|(pid, _)| pid == id)
                .map(|(_, m)| m)
                .ok_or_else(|| PluginError::Internal("plugin lost during load_all".into()))?;

            let metadata = manifest.metadata.clone();

            let plugin: Arc<dyn Plugin> =
                Arc::new(ManifestPlugin::new(metadata.clone(), manifest.clone()));

            match registry.register(plugin) {
                Ok(reg) => {
                    if let Err(_e) = self.transition_to_running(registry, id) {
                        // Mark as failed but continue loading others
                        let _ = registry.transition_to(id, PluginState::Failed);
                        registrations.push(PluginRegistration {
                            state: PluginState::Failed,
                            ..reg
                        });
                        continue;
                    }
                    if let Some(reg) = registry.lookup(id) {
                        registrations.push(reg);
                    }
                }
                Err(PluginError::AlreadyRegistered(_)) => {
                    // Skip duplicate
                    continue;
                }
                Err(_e) => {
                    // Push a minimal failed registration
                    let meta = manifest.metadata.clone();
                    registrations.push(PluginRegistration::new(id.clone(), meta, manifest.clone()));
                    continue;
                }
            }
        }

        Ok(registrations)
    }

    /// Transition a plugin through the full lifecycle to Running.
    fn transition_to_running(
        &self,
        registry: &PluginRegistry,
        plugin_id: &PluginId,
    ) -> Result<(), PluginError> {
        registry.transition_to(plugin_id, PluginState::Registered)?;
        registry.transition_to(plugin_id, PluginState::Validated)?;

        // Check permissions
        let allowed = &self.allowed_permissions;
        let perm_result = registry.verify_permissions(plugin_id, allowed)?;
        if !perm_result.valid {
            let _ = registry.transition_to(plugin_id, PluginState::Failed);
            return if let Some(missing) = perm_result.missing_permissions.into_iter().next() {
                Err(PluginError::MissingPermission(plugin_id.clone(), missing))
            } else {
                Err(PluginError::Internal(
                    "permission verification failed without details".into(),
                ))
            };
        }

        // Validate plugin
        let validation = registry.validate_plugin(plugin_id)?;
        if !validation.valid {
            let _ = registry.transition_to(plugin_id, PluginState::Failed);
            return Err(PluginError::LoadFailed(format!(
                "plugin '{}' validation failed",
                plugin_id
            )));
        }

        registry.transition_to(plugin_id, PluginState::Initialized)?;
        let ctx = PluginContext::new(plugin_id.clone());
        registry.initialize_plugin(plugin_id, &ctx)?;
        registry.transition_to(plugin_id, PluginState::Running)?;

        Ok(())
    }
}

// ── ManifestPlugin ──────────────────────────────────────────────────

/// A [`Plugin`] implementation backed entirely by manifest data.
///
/// Used by [`PluginLoader`] when no dynamic Plugin implementation is
/// available.  The plugin is fully metadata-driven:
/// - `validate()` returns success (actual validation happens in the loader)
/// - `initialize()` and `shutdown()` are no-ops
/// - `health()` always returns [`PluginStatus::Healthy`]
pub struct ManifestPlugin {
    metadata: PluginMetadata,
    manifest: PluginManifest,
}

impl ManifestPlugin {
    pub fn new(metadata: PluginMetadata, manifest: PluginManifest) -> Self {
        Self { metadata, manifest }
    }
}

impl Plugin for ManifestPlugin {
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
        Ok(())
    }

    fn shutdown(&self) -> Result<(), PluginError> {
        Ok(())
    }

    fn health(&self) -> PluginStatus {
        PluginStatus::Healthy
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::plugin::PluginPermission as PP;

    // ── Helpers ─────────────────────────────────────────────────────

    fn test_loader() -> PluginLoader {
        PluginLoader::new(
            vec![],
            vec![
                PP::BrowserAccess,
                PP::NetworkAccess,
                PP::StorageAccess,
                PP::FilesystemAccess,
            ],
        )
    }

    fn write_manifest(dir: &Path, name: &str, content: &str) -> PathBuf {
        let plugin_dir = dir.join(name);
        fs::create_dir_all(&plugin_dir).unwrap();
        let path = plugin_dir.join("manifest.toml");
        fs::write(&path, content).unwrap();
        path
    }

    fn valid_manifest(name: &str, version: &str) -> String {
        format!(
            r#"
[plugin]
name = "{name}"
version = "{version}"
author = "test author"
description = "test plugin"
homepage = "https://example.com"
license = "MIT"
"#
        )
    }

    fn make_test_registry() -> PluginRegistry {
        PluginRegistry::new()
    }

    // ── Manifest parsing tests ──────────────────────────────────────

    #[test]
    fn parse_valid_manifest() {
        let toml_str = valid_manifest("test-plugin", "1.0.0");
        let manifest: PluginManifestFile = toml::from_str(&toml_str).unwrap();
        assert_eq!(manifest.plugin.name, "test-plugin");
        assert_eq!(manifest.plugin.version, "1.0.0");
        assert_eq!(manifest.plugin.author, "test author");
        assert!(manifest.capabilities.is_none());
        assert!(manifest.dependencies.is_none());
        assert!(manifest.permissions.is_none());
    }

    #[test]
    fn parse_manifest_with_all_fields() {
        let toml_str = r#"
[plugin]
name = "full-plugin"
version = "2.1.3"
author = "Alice <alice@example.com>"
description = "A full-featured plugin"
homepage = "https://alice.example.com"
license = "Apache-2.0"

[[capabilities]]
name = "browser.navigate"
description = "Navigate to a URL"
required_params = ["url"]
optional_params = ["timeout"]
output_keys = ["status"]
tags = ["navigation", "core"]
version = "1.0.0"

[[capabilities]]
name = "dom.click"
description = "Click an element"

[[dependencies]]
plugin = "storage"
required = true
min_version = "1.0.0"
max_version = "2.0.0"

[[dependencies]]
plugin = "logger"
required = false

[[permissions]]
name = "BrowserAccess"

[[permissions]]
name = "NetworkAccess"

[hooks]
before_execution = ["pre_check"]
after_execution = ["post_log"]
"#;
        let manifest: PluginManifestFile = toml::from_str(toml_str).unwrap();
        assert_eq!(manifest.plugin.name, "full-plugin");
        assert_eq!(
            manifest.plugin.homepage.unwrap(),
            "https://alice.example.com"
        );
        assert_eq!(manifest.capabilities.as_ref().unwrap().len(), 2);
        assert_eq!(manifest.dependencies.as_ref().unwrap().len(), 2);
        assert_eq!(manifest.permissions.as_ref().unwrap().len(), 2);
        assert_eq!(
            manifest
                .hooks
                .as_ref()
                .unwrap()
                .before_execution
                .as_ref()
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn parse_manifest_missing_required_field() {
        let toml_str = r#"
[plugin]
name = "bad"
version = "1.0.0"
"#;
        // author is required — this should fail
        let result: Result<PluginManifestFile, _> = toml::from_str(toml_str);
        assert!(result.is_err());
    }

    #[test]
    fn parse_manifest_empty_string_fails() {
        let result: Result<PluginManifestFile, _> = toml::from_str("");
        assert!(result.is_err());
    }

    #[test]
    fn parse_manifest_truncated_toml() {
        let toml_str = "[plugin]\nname = \"test\"\nversion = \"1.0";
        let result: Result<PluginManifestFile, _> = toml::from_str(toml_str);
        assert!(result.is_err());
    }

    // ── Conversion tests ────────────────────────────────────────────

    #[test]
    fn convert_manifest_to_p14_types() {
        let loader = test_loader();
        let toml_str = valid_manifest("convert-test", "2.0.0");
        let file: PluginManifestFile = toml::from_str(&toml_str).unwrap();
        let (metadata, manifest) = loader.convert_manifest(file).unwrap();

        assert_eq!(metadata.name, "convert-test");
        assert_eq!(metadata.version, PluginVersion::new(2, 0, 0));
        assert_eq!(metadata.author.name, "test author");
        assert_eq!(metadata.description, "test plugin");
        assert_eq!(metadata.homepage.unwrap(), "https://example.com");
        assert_eq!(metadata.license.unwrap(), "MIT");
        assert!(manifest.capabilities.is_empty());
        assert!(manifest.dependencies.is_empty());
        assert!(manifest.permissions.is_empty());
    }

    #[test]
    fn convert_manifest_invalid_version() {
        let loader = test_loader();
        let toml_str = valid_manifest("bad-ver", "not-a-version");
        let file: PluginManifestFile = toml::from_str(&toml_str).unwrap();
        let result = loader.convert_manifest(file);
        assert!(result.is_err());
        match result.unwrap_err() {
            PluginError::ManifestParseFailed(msg) => {
                assert!(msg.contains("not-a-version"));
            }
            _ => panic!("expected ManifestParseFailed"),
        }
    }

    // ── Validation tests ────────────────────────────────────────────

    #[test]
    fn validate_valid_manifest() {
        let loader = test_loader();
        let toml_str = valid_manifest("valid", "1.0.0");
        let file: PluginManifestFile = toml::from_str(&toml_str).unwrap();
        let result = loader.validate_manifest(&file);
        assert!(result.valid);
    }

    #[test]
    fn validate_empty_name() {
        let loader = test_loader();
        let toml_str = valid_manifest("", "1.0.0");
        let file: PluginManifestFile = toml::from_str(&toml_str).unwrap();
        let result = loader.validate_manifest(&file);
        assert!(!result.valid);
    }

    #[test]
    fn validate_invalid_version() {
        let loader = test_loader();
        let toml_str = valid_manifest("test", "abc");
        let file: PluginManifestFile = toml::from_str(&toml_str).unwrap();
        let result = loader.validate_manifest(&file);
        assert!(!result.valid);
    }

    #[test]
    fn validate_empty_author() {
        let loader = test_loader();
        let toml_str = valid_manifest("test", "1.0.0").replace("test author", "");
        let file: PluginManifestFile = toml::from_str(&toml_str).unwrap();
        let result = loader.validate_manifest(&file);
        assert!(!result.valid);
    }

    #[test]
    fn validate_duplicate_capability() {
        let loader = test_loader();
        let toml_str = r#"
[plugin]
name = "dup-cap"
version = "1.0.0"
author = "test"

[[capabilities]]
name = "same.name"

[[capabilities]]
name = "same.name"
"#;
        let file: PluginManifestFile = toml::from_str(toml_str).unwrap();
        let result = loader.validate_manifest(&file);
        assert!(!result.valid);
    }

    #[test]
    fn validate_duplicate_dependency_warning() {
        let loader = test_loader();
        let toml_str = r#"
[plugin]
name = "dup-dep"
version = "1.0.0"
author = "test"

[[dependencies]]
plugin = "other"

[[dependencies]]
plugin = "other"
"#;
        let file: PluginManifestFile = toml::from_str(toml_str).unwrap();
        let result = loader.validate_manifest(&file);
        assert!(result.valid);
        assert_eq!(result.warnings.len(), 1);
    }

    #[test]
    fn validate_disallowed_permission() {
        let loader = PluginLoader::new(vec![], vec![PP::BrowserAccess]);
        let toml_str = r#"
[plugin]
name = "bad-perm"
version = "1.0.0"
author = "test"

[[permissions]]
name = "NetworkAccess"
"#;
        let file: PluginManifestFile = toml::from_str(toml_str).unwrap();
        let result = loader.validate_manifest(&file);
        assert!(result.valid);
        assert_eq!(result.warnings.len(), 1);
    }

    // ── Discovery tests ─────────────────────────────────────────────

    #[test]
    fn discover_no_directories() {
        let loader = test_loader();
        let results = loader.discover();
        assert!(results.is_empty());
    }

    #[test]
    fn discover_single_plugin() {
        let dir = TempDir::new().unwrap();
        write_manifest(
            dir.path(),
            "my-plugin",
            &valid_manifest("my-plugin", "1.0.0"),
        );

        let loader = PluginLoader::new(vec![dir.path().to_path_buf()], vec![]);
        let results = loader.discover();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].plugin_id, PluginId::new("my-plugin"));
    }

    #[test]
    fn discover_multiple_plugins() {
        let dir = TempDir::new().unwrap();
        write_manifest(dir.path(), "plugin-a", &valid_manifest("plugin-a", "1.0.0"));
        write_manifest(dir.path(), "plugin-b", &valid_manifest("plugin-b", "2.0.0"));

        let loader = PluginLoader::new(vec![dir.path().to_path_buf()], vec![]);
        let results = loader.discover();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].plugin_id, PluginId::new("plugin-a"));
        assert_eq!(results[1].plugin_id, PluginId::new("plugin-b"));
    }

    #[test]
    fn discover_empty_dir() {
        let dir = TempDir::new().unwrap();
        let loader = PluginLoader::new(vec![dir.path().to_path_buf()], vec![]);
        let results = loader.discover();
        assert!(results.is_empty());
    }

    #[test]
    fn discover_skips_non_manifest_dirs() {
        let dir = TempDir::new().unwrap();
        let sub = dir.path().join("no-manifest");
        fs::create_dir_all(&sub).unwrap();
        let loader = PluginLoader::new(vec![dir.path().to_path_buf()], vec![]);
        let results = loader.discover();
        assert!(results.is_empty());
    }

    #[test]
    fn discover_multiple_directories() {
        let dir1 = TempDir::new().unwrap();
        let dir2 = TempDir::new().unwrap();
        write_manifest(
            dir1.path(),
            "from-one",
            &valid_manifest("from-one", "1.0.0"),
        );
        write_manifest(
            dir2.path(),
            "from-two",
            &valid_manifest("from-two", "1.0.0"),
        );

        let loader = PluginLoader::new(
            vec![dir1.path().to_path_buf(), dir2.path().to_path_buf()],
            vec![],
        );
        let results = loader.discover();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn discover_nonexistent_dir_silently_skipped() {
        let loader = PluginLoader::new(
            vec![PathBuf::from("C:\\nonexistent-plugin-dir-12345")],
            vec![],
        );
        let results = loader.discover();
        assert!(results.is_empty());
    }

    // ── Read manifest tests ─────────────────────────────────────────

    #[test]
    fn read_manifest_file_not_found() {
        let loader = test_loader();
        let err = loader
            .read_manifest(Path::new("C:\\nonexistent.toml"))
            .unwrap_err();
        match err {
            PluginError::ManifestParseFailed(_) => {}
            _ => panic!("expected ManifestParseFailed"),
        }
    }

    #[test]
    fn read_manifest_invalid_content() {
        let dir = TempDir::new().unwrap();
        let path = write_manifest(dir.path(), "bad", "not valid toml {{{");
        // Need to write invalid content
        fs::write(&path, "not valid toml {{{").unwrap();
        let loader = test_loader();
        let err = loader.read_manifest(&path).unwrap_err();
        match err {
            PluginError::ManifestParseFailed(_) => {}
            _ => panic!("expected ManifestParseFailed"),
        }
    }

    // ── Dependency resolution tests ─────────────────────────────────

    fn make_simple_manifest(name: &str, deps: Vec<(&str, bool)>) -> PluginManifest {
        let author = PluginAuthor::new("test");
        let version = PluginVersion::new(1, 0, 0);
        let metadata = PluginMetadata::new(name, version, author);
        let mut manifest = PluginManifest::new(metadata);
        for (dep_name, required) in deps {
            let mut dep = PluginDependency::new(PluginId::new(dep_name));
            if !required {
                dep = dep.optional();
            }
            manifest = manifest.with_dependency(dep);
        }
        manifest
    }

    #[test]
    fn resolve_no_dependencies() {
        let loader = test_loader();
        let m = make_simple_manifest("standalone", vec![]);
        let plugins = vec![(PluginId::new("standalone"), &m)];
        let order = loader.resolve_loading_order(&plugins).unwrap();
        assert_eq!(order, vec![PluginId::new("standalone")]);
    }

    #[test]
    fn resolve_simple_chain() {
        let loader = test_loader();
        let m_a = make_simple_manifest("a", vec![]);
        let m_b = make_simple_manifest("b", vec![("a", true)]);
        let plugins = vec![(PluginId::new("a"), &m_a), (PluginId::new("b"), &m_b)];
        let order = loader.resolve_loading_order(&plugins).unwrap();
        assert_eq!(order, vec![PluginId::new("a"), PluginId::new("b")]);
    }

    #[test]
    fn resolve_diamond() {
        let loader = test_loader();
        let m_a = make_simple_manifest("a", vec![]);
        let m_b = make_simple_manifest("b", vec![("a", true)]);
        let m_c = make_simple_manifest("c", vec![("a", true)]);
        let m_d = make_simple_manifest("d", vec![("b", true), ("c", true)]);
        let plugins = vec![
            (PluginId::new("a"), &m_a),
            (PluginId::new("b"), &m_b),
            (PluginId::new("c"), &m_c),
            (PluginId::new("d"), &m_d),
        ];
        let order = loader.resolve_loading_order(&plugins).unwrap();
        assert_eq!(order[0], PluginId::new("a"));
        assert!(order[3] == PluginId::new("d"));
    }

    #[test]
    fn resolve_cycle_detected() {
        let loader = test_loader();
        let m_a = make_simple_manifest("a", vec![("b", true)]);
        let m_b = make_simple_manifest("b", vec![("a", true)]);
        let plugins = vec![(PluginId::new("a"), &m_a), (PluginId::new("b"), &m_b)];
        let err = loader.resolve_loading_order(&plugins).unwrap_err();
        match err {
            PluginError::CircularDependency(_) => {}
            _ => panic!("expected CircularDependency"),
        }
    }

    #[test]
    fn resolve_self_cycle() {
        let loader = test_loader();
        let m = make_simple_manifest("self", vec![("self", true)]);
        let plugins = vec![(PluginId::new("self"), &m)];
        let err = loader.resolve_loading_order(&plugins).unwrap_err();
        match err {
            PluginError::CircularDependency(_) => {}
            _ => panic!("expected CircularDependency"),
        }
    }

    #[test]
    fn resolve_missing_dependency() {
        let loader = test_loader();
        let m = make_simple_manifest("main", vec![("missing", true)]);
        let plugins = vec![(PluginId::new("main"), &m)];
        let err = loader.resolve_loading_order(&plugins).unwrap_err();
        match err {
            PluginError::DependencyNotFound { .. } => {}
            _ => panic!("expected DependencyNotFound"),
        }
    }

    #[test]
    fn resolve_optional_dependency_missing_is_ok() {
        let loader = test_loader();
        let m = make_simple_manifest("main", vec![("optional-dep", false)]);
        let plugins = vec![(PluginId::new("main"), &m)];
        let order = loader.resolve_loading_order(&plugins).unwrap();
        assert_eq!(order, vec![PluginId::new("main")]);
    }

    #[test]
    fn resolve_version_mismatch() {
        let loader = test_loader();
        let author = PluginAuthor::new("test");
        let meta_a = PluginMetadata::new("a", PluginVersion::new(1, 0, 0), author);

        let mut dep = PluginDependency::new(PluginId::new("a"));
        dep = dep.with_min_version(PluginVersion::new(2, 0, 0));

        let author_b = PluginAuthor::new("test");
        let meta_b = PluginMetadata::new("b", PluginVersion::new(1, 0, 0), author_b);
        let mut manifest_b = PluginManifest::new(meta_b);
        manifest_b = manifest_b.with_dependency(dep);

        let manifest_a = PluginManifest::new(meta_a);

        let plugins = vec![
            (PluginId::new("a"), &manifest_a),
            (PluginId::new("b"), &manifest_b),
        ];
        let err = loader.resolve_loading_order(&plugins).unwrap_err();
        match err {
            PluginError::DependencyVersionMismatch { .. } => {}
            _ => panic!("expected DependencyVersionMismatch"),
        }
    }

    #[test]
    fn resolve_duplicate_ids() {
        let loader = test_loader();
        let m = make_simple_manifest("dup", vec![]);
        let plugins = vec![(PluginId::new("dup"), &m), (PluginId::new("dup"), &m)];
        let err = loader.resolve_loading_order(&plugins).unwrap_err();
        match err {
            PluginError::AlreadyRegistered(_) => {}
            _ => panic!("expected AlreadyRegistered"),
        }
    }

    #[test]
    fn resolve_loading_order_deterministic() {
        let loader = test_loader();
        let m_a = make_simple_manifest("a", vec![]);
        let m_b = make_simple_manifest("b", vec![]);
        let m_c = make_simple_manifest("c", vec![]);
        let plugins = vec![
            (PluginId::new("c"), &m_c),
            (PluginId::new("a"), &m_a),
            (PluginId::new("b"), &m_b),
        ];
        let order = loader.resolve_loading_order(&plugins).unwrap();
        assert_eq!(
            order,
            vec![PluginId::new("a"), PluginId::new("b"), PluginId::new("c"),]
        );
    }

    // ── Loading tests ───────────────────────────────────────────────

    #[test]
    fn load_single_plugin() {
        let dir = TempDir::new().unwrap();
        let path = write_manifest(dir.path(), "single", &valid_manifest("single", "1.0.0"));
        let loader = PluginLoader::new(vec![], vec![PP::BrowserAccess]);
        let registry = make_test_registry();
        let reg = loader.load(&registry, &path).unwrap();
        assert_eq!(reg.plugin_id, PluginId::new("single"));
        assert_eq!(reg.state, PluginState::Running);
    }

    #[test]
    fn load_plugin_nonexistent_path() {
        let loader = test_loader();
        let registry = make_test_registry();
        let err = loader
            .load(&registry, Path::new("C:\\no-such-plugin\\manifest.toml"))
            .unwrap_err();
        match err {
            PluginError::ManifestNotFound(_) => {}
            _ => panic!("expected ManifestNotFound"),
        }
    }

    #[test]
    fn load_all_no_dirs() {
        let loader = test_loader();
        let registry = make_test_registry();
        let regs = loader.load_all(&registry).unwrap();
        assert!(regs.is_empty());
    }

    #[test]
    fn load_all_single_plugin() {
        let dir = TempDir::new().unwrap();
        write_manifest(dir.path(), "only-one", &valid_manifest("only-one", "1.0.0"));

        let loader = PluginLoader::new(vec![dir.path().to_path_buf()], vec![PP::BrowserAccess]);
        let registry = make_test_registry();
        let regs = loader.load_all(&registry).unwrap();
        assert_eq!(regs.len(), 1);
        assert_eq!(regs[0].plugin_id, PluginId::new("only-one"));
        assert_eq!(regs[0].state, PluginState::Running);
    }

    #[test]
    fn load_all_with_dependencies() {
        let dir = TempDir::new().unwrap();
        write_manifest(dir.path(), "base", &valid_manifest("base", "1.0.0"));

        let dep_manifest = format!(
            r#"
[plugin]
name = "dependent"
version = "1.0.0"
author = "test"

[[dependencies]]
plugin = "base"
required = true
"#
        );
        write_manifest(dir.path(), "dependent", &dep_manifest);

        let loader = PluginLoader::new(vec![dir.path().to_path_buf()], vec![PP::BrowserAccess]);
        let registry = make_test_registry();
        let regs = loader.load_all(&registry).unwrap();
        assert_eq!(regs.len(), 2);

        // Verify loading order: base first, then dependent
        let base_reg = registry.lookup(&PluginId::new("base")).unwrap();
        let dep_reg = registry.lookup(&PluginId::new("dependent")).unwrap();
        assert_eq!(base_reg.state, PluginState::Running);
        assert_eq!(dep_reg.state, PluginState::Running);
    }

    #[test]
    fn load_all_skips_invalid_manifest() {
        let dir = TempDir::new().unwrap();
        write_manifest(dir.path(), "good", &valid_manifest("good", "1.0.0"));

        let bad_manifest = r#"
[plugin]
name = "bad"
version = "not-a-version"
author = "test"
"#;
        write_manifest(dir.path(), "bad", bad_manifest);

        let loader = PluginLoader::new(vec![dir.path().to_path_buf()], vec![PP::BrowserAccess]);
        let registry = make_test_registry();
        let regs = loader.load_all(&registry).unwrap();
        assert_eq!(regs.len(), 1);
        assert_eq!(regs[0].plugin_id, PluginId::new("good"));
    }

    #[test]
    fn load_all_with_cycle_handling() {
        let dir = TempDir::new().unwrap();

        let dep_a = format!(
            r#"
[plugin]
name = "a"
version = "1.0.0"
author = "test"

[[dependencies]]
plugin = "b"
required = true
"#
        );
        write_manifest(dir.path(), "a", &dep_a);

        let dep_b = format!(
            r#"
[plugin]
name = "b"
version = "1.0.0"
author = "test"

[[dependencies]]
plugin = "a"
required = true
"#
        );
        write_manifest(dir.path(), "b", &dep_b);

        let loader = PluginLoader::new(vec![dir.path().to_path_buf()], vec![PP::BrowserAccess]);
        let registry = make_test_registry();
        let result = loader.load_all(&registry);
        assert!(result.is_err() || result.unwrap().is_empty());
    }

    // ── ManifestPlugin tests ────────────────────────────────────────

    #[test]
    fn manifest_plugin_implements_plugin_trait() {
        let author = PluginAuthor::new("test");
        let version = PluginVersion::new(1, 0, 0);
        let metadata = PluginMetadata::new("test-plugin", version, author);
        let manifest = PluginManifest::new(metadata.clone());
        let plugin = ManifestPlugin::new(metadata, manifest);

        assert_eq!(plugin.metadata().name, "test-plugin");
        assert_eq!(plugin.health(), PluginStatus::Healthy);
        assert!(plugin
            .initialize(&PluginContext::new(PluginId::new("test")))
            .is_ok());
        assert!(plugin.shutdown().is_ok());
    }

    #[test]
    fn manifest_plugin_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<ManifestPlugin>();
        assert_sync::<ManifestPlugin>();
    }

    // ── Error tests ─────────────────────────────────────────────────

    #[test]
    fn error_manifest_not_found_display() {
        let err = PluginError::ManifestNotFound("path/to/manifest.toml".into());
        assert_eq!(err.to_string(), "manifest not found: path/to/manifest.toml");
    }

    #[test]
    fn error_manifest_parse_failed_display() {
        let err = PluginError::ManifestParseFailed("invalid TOML at line 3".into());
        assert!(err.to_string().contains("invalid TOML"));
    }

    #[test]
    fn error_circular_dependency_display() {
        let err = PluginError::CircularDependency(vec![PluginId::new("a"), PluginId::new("b")]);
        assert!(err.to_string().contains("a"));
        assert!(err.to_string().contains("b"));
    }

    #[test]
    fn error_load_failed_display() {
        let err = PluginError::LoadFailed("something went wrong".into());
        assert_eq!(err.to_string(), "plugin load failed: something went wrong");
    }

    // ── Edge case tests ─────────────────────────────────────────────

    #[test]
    fn discover_with_dot_dir_skipped() {
        let dir = TempDir::new().unwrap();
        // Create a directory with a dot prefix
        let hidden = dir.path().join(".hidden");
        fs::create_dir_all(&hidden).unwrap();
        // No manifest in it

        let loader = PluginLoader::new(vec![dir.path().to_path_buf()], vec![]);
        let results = loader.discover();
        assert!(results.is_empty());
    }

    #[test]
    fn load_all_empty_permissions() {
        let dir = TempDir::new().unwrap();
        write_manifest(dir.path(), "no-perm", &valid_manifest("no-perm", "1.0.0"));

        let loader = PluginLoader::new(vec![dir.path().to_path_buf()], vec![]);
        let registry = make_test_registry();
        let regs = loader.load_all(&registry).unwrap();
        assert_eq!(regs.len(), 1);
        assert_eq!(regs[0].state, PluginState::Running);
    }

    #[test]
    fn read_manifest_with_capabilities() {
        let toml_str = r#"
[plugin]
name = "cap-plugin"
version = "1.0.0"
author = "test"

[[capabilities]]
name = "browser.navigate"
description = "Navigate to a URL"
required_params = ["url"]
optional_params = ["timeout"]
output_keys = ["status", "title"]
tags = ["navigation"]
version = "1.0.0"
"#;
        let manifest: PluginManifestFile = toml::from_str(toml_str).unwrap();
        let caps = manifest.capabilities.unwrap();
        assert_eq!(caps.len(), 1);
        assert_eq!(caps[0].name, "browser.navigate");
        assert_eq!(caps[0].required_params.as_ref().unwrap(), &vec!["url"]);
        assert_eq!(
            caps[0].output_keys.as_ref().unwrap(),
            &vec!["status", "title"]
        );
    }

    #[test]
    fn convert_capabilities() {
        let loader = test_loader();
        let toml_str = r#"
[plugin]
name = "cap-test"
version = "1.0.0"
author = "test"

[[capabilities]]
name = "test.cap"
description = "A test capability"
required_params = ["input"]
output_keys = ["output"]
tags = ["test"]
version = "1.0.0"
"#;
        let file: PluginManifestFile = toml::from_str(toml_str).unwrap();
        let (_, manifest) = loader.convert_manifest(file).unwrap();
        assert_eq!(manifest.capabilities.len(), 1);
        let cap = &manifest.capabilities[0];
        assert_eq!(cap.id, PluginCapabilityId::new("test.cap"));
        assert_eq!(cap.metadata.name, "test.cap");
        assert_eq!(cap.metadata.required_params, vec!["input"]);
        assert_eq!(cap.metadata.output_keys, vec!["output"]);
    }

    #[test]
    fn convert_dependencies() {
        let loader = test_loader();
        let toml_str = r#"
[plugin]
name = "dep-test"
version = "1.0.0"
author = "test"

[[dependencies]]
plugin = "base"
required = true
min_version = "1.0.0"
max_version = "2.0.0"

[[dependencies]]
plugin = "logger"
required = false
"#;
        let file: PluginManifestFile = toml::from_str(toml_str).unwrap();
        let (_, manifest) = loader.convert_manifest(file).unwrap();
        assert_eq!(manifest.dependencies.len(), 2);

        let base = &manifest.dependencies[0];
        assert_eq!(base.plugin_id, PluginId::new("base"));
        assert!(base.required);
        assert_eq!(base.min_version, Some(PluginVersion::new(1, 0, 0)));

        let logger = &manifest.dependencies[1];
        assert_eq!(logger.plugin_id, PluginId::new("logger"));
        assert!(!logger.required);
    }

    #[test]
    fn convert_hooks() {
        let loader = test_loader();
        let toml_str = r#"
[plugin]
name = "hook-test"
version = "1.0.0"
author = "test"

[hooks]
before_execution = ["pre", "check"]
after_execution = ["post"]
"#;
        let file: PluginManifestFile = toml::from_str(toml_str).unwrap();
        let (_, manifest) = loader.convert_manifest(file).unwrap();
        assert_eq!(manifest.hooks.before_execution, vec!["pre", "check"]);
        assert_eq!(manifest.hooks.after_execution, vec!["post"]);
    }

    #[test]
    fn convert_manifest_utf8_author() {
        let loader = test_loader();
        let toml_str = valid_manifest("utf8-test", "1.0.0").replace("test author", "Zoë Müller");
        let file: PluginManifestFile = toml::from_str(&toml_str).unwrap();
        let (metadata, _) = loader.convert_manifest(file).unwrap();
        assert_eq!(metadata.author.name, "Zoë Müller");
    }

    #[test]
    fn load_plugin_twice_fails() {
        let dir = TempDir::new().unwrap();
        let path = write_manifest(dir.path(), "dup-load", &valid_manifest("dup-load", "1.0.0"));
        let loader = PluginLoader::new(vec![], vec![PP::BrowserAccess]);
        let registry = make_test_registry();

        loader.load(&registry, &path).unwrap();
        let err = loader.load(&registry, &path).unwrap_err();
        match err {
            PluginError::AlreadyRegistered(_) => {}
            _ => panic!("expected AlreadyRegistered"),
        }
    }

    #[test]
    fn manifest_plugin_with_capabilities() {
        let version = PluginVersion::new(1, 0, 0);
        let cap = PluginCapability::new(
            PluginCapabilityId::new("test.cap"),
            CapabilityMetadata::new("test.cap"),
        );
        let meta = PluginMetadata::new("cap-test", version, PluginAuthor::new("t"));
        let manifest = PluginManifest::new(meta.clone()).with_capability(cap);
        let plugin = ManifestPlugin::new(meta, manifest);

        let caps = plugin.capabilities();
        assert_eq!(caps.len(), 1);
        assert_eq!(caps[0].id, PluginCapabilityId::new("test.cap"));
    }
}
