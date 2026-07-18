/// P15 — Plugin Manager
///
/// High-level manager that owns [`PluginLoader`] + [`PluginRegistry`] and
/// provides a unified API for managing the complete plugin lifecycle.
///
/// # Capabilities
/// - `load_plugin` / `load_all` — load and initialise plugins
/// - `reload_plugin` — unload then reload a plugin by ID
/// - `unload_plugin` — shut down and deregister a plugin
/// - `shutdown_all` — gracefully shut down all plugins
/// - `plugin` / `plugins` — query plugin registrations
/// - `plugin_state` — get the current lifecycle state
/// - `health` — inspect health of all plugins
/// - `statistics` — aggregated plugin statistics
///
/// # Integration
/// The manager does NOT modify Planner, Executor, RuntimeContext, MCP,
/// or Bridge.  It prepares them for future integration by exposing
/// capabilities through the registry.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use crate::loader::PluginLoader;
use crate::plugin::{
    PluginError, PluginId, PluginRegistration, PluginRegistry, PluginState, PluginStatus,
};

/// High-level manager for plugin lifecycle.
#[derive(Debug)]
pub struct PluginManager {
    loader: PluginLoader,
    registry: PluginRegistry,
    plugin_paths: Arc<RwLock<HashMap<PluginId, PathBuf>>>,
}

impl PluginManager {
    pub fn new(loader: PluginLoader, registry: PluginRegistry) -> Self {
        Self {
            loader,
            registry,
            plugin_paths: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Return a reference to the underlying [`PluginLoader`].
    pub fn loader(&self) -> &PluginLoader {
        &self.loader
    }

    /// Return a reference to the underlying [`PluginRegistry`].
    pub fn registry(&self) -> &PluginRegistry {
        &self.registry
    }

    // ── Loading ─────────────────────────────────────────────────────

    /// Load a single plugin from a manifest path.
    ///
    /// The path is recorded so the plugin can be reloaded by ID later.
    pub fn load_plugin(&self, path: &Path) -> Result<PluginRegistration, PluginError> {
        let registration = self.loader.load(&self.registry, path)?;
        let plugin_id = registration.plugin_id.clone();

        let mut paths = self
            .plugin_paths
            .write()
            .map_err(|_| PluginError::Internal("PluginManager lock poisoned".into()))?;
        paths.insert(plugin_id, path.to_path_buf());

        Ok(registration)
    }

    /// Discover, resolve dependencies, and load all plugins.
    ///
    /// Returns the registrations of successfully loaded plugins.
    pub fn load_all(&self) -> Result<Vec<PluginRegistration>, PluginError> {
        let registrations = self.loader.load_all(&self.registry)?;

        let mut paths = self
            .plugin_paths
            .write()
            .map_err(|_| PluginError::Internal("PluginManager lock poisoned".into()))?;

        // Store paths from discovery
        for result in self.loader.discover() {
            paths.insert(result.plugin_id, result.manifest_path);
        }

        Ok(registrations)
    }

    // ── Reload ──────────────────────────────────────────────────────

    /// Reload a plugin by ID: unload then load from the stored path.
    pub fn reload_plugin(&self, plugin_id: &PluginId) -> Result<PluginRegistration, PluginError> {
        let path = {
            let paths = self
                .plugin_paths
                .read()
                .map_err(|_| PluginError::Internal("PluginManager lock poisoned".into()))?;
            paths
                .get(plugin_id)
                .cloned()
                .ok_or_else(|| PluginError::NotFound(plugin_id.clone()))?
        };

        // Unload
        self.unload_plugin(plugin_id)?;

        // Reload
        self.load_plugin(&path)
    }

    // ── Unload ──────────────────────────────────────────────────────

    /// Unload a plugin: shut down, transition to Unloaded, deregister.
    pub fn unload_plugin(&self, plugin_id: &PluginId) -> Result<(), PluginError> {
        // Check plugin exists
        self.registry
            .lookup(plugin_id)
            .ok_or_else(|| PluginError::NotFound(plugin_id.clone()))?;

        // Shutdown
        let _ = self.registry.shutdown_plugin(plugin_id);

        // Transition through lifecycle
        let current = self
            .registry
            .lookup(plugin_id)
            .map(|r| r.state)
            .unwrap_or(PluginState::Failed);

        // Transition to Disabled (only if in Running or Paused)
        if current == PluginState::Running || current == PluginState::Paused {
            let _ = self
                .registry
                .transition_to(plugin_id, PluginState::Disabled);
        }

        // Transition to Unloaded
        let _ = self
            .registry
            .transition_to(plugin_id, PluginState::Unloaded);

        // Deregister
        self.registry.unregister(plugin_id)?;

        // Remove path mapping
        let mut paths = self
            .plugin_paths
            .write()
            .map_err(|_| PluginError::Internal("PluginManager lock poisoned".into()))?;
        paths.remove(plugin_id);

        Ok(())
    }

    // ── Shutdown all ────────────────────────────────────────────────

    /// Gracefully shut down and unload all registered plugins.
    pub fn shutdown_all(&self) -> Result<(), PluginError> {
        let plugins = self.registry.list_plugins();
        for reg in plugins {
            let _ = self.unload_plugin(&reg.plugin_id);
        }
        Ok(())
    }

    // ── Queries ─────────────────────────────────────────────────────

    /// Get a plugin's registration by ID.
    pub fn plugin(&self, plugin_id: &PluginId) -> Option<PluginRegistration> {
        self.registry.lookup(plugin_id)
    }

    /// List all registered plugin registrations.
    pub fn plugins(&self) -> Vec<PluginRegistration> {
        self.registry.list_plugins()
    }

    /// Get the current lifecycle state of a plugin.
    pub fn plugin_state(&self, plugin_id: &PluginId) -> Option<PluginState> {
        self.registry.lookup(plugin_id).map(|r| r.state)
    }

    /// Inspect health of all registered plugins.
    pub fn health(&self) -> Vec<(PluginId, PluginStatus)> {
        self.registry.health_inspect()
    }

    // ── Statistics ──────────────────────────────────────────────────

    /// Aggregate statistics about all registered plugins.
    pub fn statistics(&self) -> PluginStatistics {
        let plugins = self.registry.list_plugins();
        let health_results = self.registry.health_inspect();
        let capabilities = self.registry.list_capabilities();

        let mut stats = PluginStatistics {
            total: plugins.len(),
            running: 0,
            failed: 0,
            disabled: 0,
            total_capabilities: capabilities.len(),
            healthy: 0,
            degraded: 0,
        };

        for reg in &plugins {
            match reg.state {
                PluginState::Running => stats.running += 1,
                PluginState::Failed => stats.failed += 1,
                PluginState::Disabled | PluginState::Unloaded => stats.disabled += 1,
                _ => {}
            }
        }

        for (_, status) in &health_results {
            match status {
                PluginStatus::Healthy => stats.healthy += 1,
                PluginStatus::Degraded(_) => stats.degraded += 1,
                _ => {}
            }
        }

        stats
    }
}

/// Aggregated statistics about all managed plugins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginStatistics {
    pub total: usize,
    pub running: usize,
    pub failed: usize,
    pub disabled: usize,
    pub total_capabilities: usize,
    pub healthy: usize,
    pub degraded: usize,
}

impl PluginStatistics {
    pub fn new() -> Self {
        Self {
            total: 0,
            running: 0,
            failed: 0,
            disabled: 0,
            total_capabilities: 0,
            healthy: 0,
            degraded: 0,
        }
    }
}

impl Default for PluginStatistics {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use tempfile::TempDir;

    use super::*;
    use crate::loader::PluginLoader;
    use crate::plugin::PluginPermission as PP;

    // ── Helpers ─────────────────────────────────────────────────────

    fn write_manifest(dir: &Path, name: &str, content: &str) {
        let plugin_dir = dir.join(name);
        fs::create_dir_all(&plugin_dir).unwrap();
        fs::write(plugin_dir.join("manifest.toml"), content).unwrap();
    }

    fn valid_manifest(name: &str) -> String {
        format!(
            r#"
[plugin]
name = "{name}"
version = "1.0.0"
author = "test author"
"#
        )
    }

    fn make_manager(dir: &Path) -> PluginManager {
        let loader = PluginLoader::new(
            vec![dir.to_path_buf()],
            vec![PP::BrowserAccess, PP::NetworkAccess],
        );
        let registry = PluginRegistry::new();
        PluginManager::new(loader, registry)
    }

    // ── Loading tests ───────────────────────────────────────────────

    #[test]
    fn manager_load_plugin() {
        let dir = TempDir::new().unwrap();
        write_manifest(dir.path(), "test-plugin", &valid_manifest("test-plugin"));
        let path = dir.path().join("test-plugin").join("manifest.toml");

        let manager = make_manager(dir.path());
        let reg = manager.load_plugin(&path).unwrap();
        assert_eq!(reg.plugin_id, PluginId::new("test-plugin"));
        assert_eq!(reg.state, PluginState::Running);
    }

    #[test]
    fn manager_load_all() {
        let dir = TempDir::new().unwrap();
        write_manifest(dir.path(), "p1", &valid_manifest("p1"));
        write_manifest(dir.path(), "p2", &valid_manifest("p2"));

        let manager = make_manager(dir.path());
        let regs = manager.load_all().unwrap();
        assert_eq!(regs.len(), 2);
    }

    #[test]
    fn manager_load_all_empty() {
        let dir = TempDir::new().unwrap();
        let manager = make_manager(dir.path());
        let regs = manager.load_all().unwrap();
        assert!(regs.is_empty());
    }

    // ── Query tests ─────────────────────────────────────────────────

    #[test]
    fn manager_plugin_query() {
        let dir = TempDir::new().unwrap();
        write_manifest(dir.path(), "query", &valid_manifest("query"));
        let manager = make_manager(dir.path());
        manager.load_all().unwrap();

        let reg = manager.plugin(&PluginId::new("query"));
        assert!(reg.is_some());
        assert_eq!(reg.unwrap().plugin_id, PluginId::new("query"));
    }

    #[test]
    fn manager_plugin_not_found() {
        let manager = PluginManager::new(PluginLoader::new(vec![], vec![]), PluginRegistry::new());
        let reg = manager.plugin(&PluginId::new("nonexistent"));
        assert!(reg.is_none());
    }

    #[test]
    fn manager_plugins_list() {
        let dir = TempDir::new().unwrap();
        write_manifest(dir.path(), "a", &valid_manifest("a"));
        write_manifest(dir.path(), "b", &valid_manifest("b"));
        let manager = make_manager(dir.path());
        manager.load_all().unwrap();

        let plugins = manager.plugins();
        assert_eq!(plugins.len(), 2);
    }

    #[test]
    fn manager_plugin_state() {
        let dir = TempDir::new().unwrap();
        write_manifest(dir.path(), "state-test", &valid_manifest("state-test"));
        let manager = make_manager(dir.path());
        manager.load_all().unwrap();

        let state = manager.plugin_state(&PluginId::new("state-test"));
        assert_eq!(state, Some(PluginState::Running));
    }

    #[test]
    fn manager_plugin_state_nonexistent() {
        let manager = PluginManager::new(PluginLoader::new(vec![], vec![]), PluginRegistry::new());
        assert!(manager.plugin_state(&PluginId::new("ghost")).is_none());
    }

    // ── Health tests ────────────────────────────────────────────────

    #[test]
    fn manager_health_empty() {
        let manager = PluginManager::new(PluginLoader::new(vec![], vec![]), PluginRegistry::new());
        let health = manager.health();
        assert!(health.is_empty());
    }

    #[test]
    fn manager_health_with_plugins() {
        let dir = TempDir::new().unwrap();
        write_manifest(dir.path(), "healthy-p", &valid_manifest("healthy-p"));
        let manager = make_manager(dir.path());
        manager.load_all().unwrap();

        let health = manager.health();
        assert_eq!(health.len(), 1);
        assert_eq!(health[0].1, PluginStatus::Healthy);
    }

    // ── Unload tests ────────────────────────────────────────────────

    #[test]
    fn manager_unload_plugin() {
        let dir = TempDir::new().unwrap();
        write_manifest(dir.path(), "to-unload", &valid_manifest("to-unload"));
        let manager = make_manager(dir.path());

        let path = dir.path().join("to-unload").join("manifest.toml");
        manager.load_plugin(&path).unwrap();
        manager.unload_plugin(&PluginId::new("to-unload")).unwrap();

        assert!(manager.plugin(&PluginId::new("to-unload")).is_none());
    }

    #[test]
    fn manager_unload_nonexistent() {
        let manager = PluginManager::new(PluginLoader::new(vec![], vec![]), PluginRegistry::new());
        let err = manager.unload_plugin(&PluginId::new("ghost")).unwrap_err();
        match err {
            PluginError::NotFound(_) => {}
            _ => panic!("expected NotFound"),
        }
    }

    // ── Reload tests ────────────────────────────────────────────────

    #[test]
    fn manager_reload_plugin() {
        let dir = TempDir::new().unwrap();
        write_manifest(dir.path(), "reloadable", &valid_manifest("reloadable"));
        let manager = make_manager(dir.path());

        let path = dir.path().join("reloadable").join("manifest.toml");
        manager.load_plugin(&path).unwrap();
        let reloaded = manager.reload_plugin(&PluginId::new("reloadable")).unwrap();
        assert_eq!(reloaded.plugin_id, PluginId::new("reloadable"));
        assert_eq!(reloaded.state, PluginState::Running);
    }

    #[test]
    fn manager_reload_nonexistent() {
        let manager = PluginManager::new(PluginLoader::new(vec![], vec![]), PluginRegistry::new());
        let err = manager.reload_plugin(&PluginId::new("ghost")).unwrap_err();
        match err {
            PluginError::NotFound(_) => {}
            _ => panic!("expected NotFound"),
        }
    }

    // ── Shutdown tests ──────────────────────────────────────────────

    #[test]
    fn manager_shutdown_all() {
        let dir = TempDir::new().unwrap();
        write_manifest(dir.path(), "s1", &valid_manifest("s1"));
        write_manifest(dir.path(), "s2", &valid_manifest("s2"));
        let manager = make_manager(dir.path());
        manager.load_all().unwrap();
        manager.shutdown_all().unwrap();

        assert!(manager.plugins().is_empty());
    }

    #[test]
    fn manager_shutdown_all_empty() {
        let manager = PluginManager::new(PluginLoader::new(vec![], vec![]), PluginRegistry::new());
        manager.shutdown_all().unwrap();
        assert!(manager.plugins().is_empty());
    }

    // ── Statistics tests ────────────────────────────────────────────

    #[test]
    fn manager_statistics_empty() {
        let manager = PluginManager::new(PluginLoader::new(vec![], vec![]), PluginRegistry::new());
        let stats = manager.statistics();
        assert_eq!(stats.total, 0);
    }

    #[test]
    fn manager_statistics_with_plugins() {
        let dir = TempDir::new().unwrap();
        write_manifest(dir.path(), "stat1", &valid_manifest("stat1"));
        write_manifest(dir.path(), "stat2", &valid_manifest("stat2"));
        let manager = make_manager(dir.path());
        manager.load_all().unwrap();

        let stats = manager.statistics();
        assert_eq!(stats.total, 2);
        assert_eq!(stats.running, 2);
        assert_eq!(stats.healthy, 2);
    }

    #[test]
    fn manager_statistics_after_unload() {
        let dir = TempDir::new().unwrap();
        write_manifest(dir.path(), "stay", &valid_manifest("stay"));
        write_manifest(dir.path(), "go", &valid_manifest("go"));
        let manager = make_manager(dir.path());

        let path = dir.path().join("go").join("manifest.toml");
        manager.load_plugin(&path).unwrap();
        manager.unload_plugin(&PluginId::new("go")).unwrap();

        let path2 = dir.path().join("stay").join("manifest.toml");
        manager.load_plugin(&path2).unwrap();

        let stats = manager.statistics();
        assert_eq!(stats.total, 1);
        assert_eq!(stats.running, 1);
    }

    // ── Edge case tests ─────────────────────────────────────────────

    #[test]
    fn manager_load_plugin_nonexistent_path() {
        let manager = PluginManager::new(PluginLoader::new(vec![], vec![]), PluginRegistry::new());
        let err = manager.load_plugin(Path::new("C:\\no-such-path\\manifest.toml"));
        assert!(err.is_err());
    }

    #[test]
    fn manager_double_load_same_path() {
        let dir = TempDir::new().unwrap();
        write_manifest(dir.path(), "dup-manager", &valid_manifest("dup-manager"));
        let path = dir.path().join("dup-manager").join("manifest.toml");
        let manager = make_manager(dir.path());

        manager.load_plugin(&path).unwrap();
        let err = manager.load_plugin(&path).unwrap_err();
        match err {
            PluginError::AlreadyRegistered(_) => {}
            _ => panic!("expected AlreadyRegistered"),
        }
    }

    #[test]
    fn manager_accessors() {
        let loader = PluginLoader::new(vec![], vec![]);
        let registry = PluginRegistry::new();
        let manager = PluginManager::new(loader, registry);

        // Use explicit type annotations
        let _loader_ref = manager.loader();
        let _registry_ref = manager.registry();
    }

    #[test]
    fn statistics_default() {
        let stats = PluginStatistics::new();
        assert_eq!(stats.total, 0);
        assert_eq!(stats.running, 0);
        assert_eq!(stats.failed, 0);
        assert_eq!(stats.disabled, 0);
        assert_eq!(stats.total_capabilities, 0);
        assert_eq!(stats.healthy, 0);
        assert_eq!(stats.degraded, 0);
    }
}
