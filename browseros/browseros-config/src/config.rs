use std::any::Any;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;

use serde::de::DeserializeOwned;

use crate::validator::{get_value, merge_into, ConfigError};

/// Trait implemented by every component's config block.
///
/// The only required method is `namespace()` which returns the dot-notation
/// key prefix for this component's configuration (e.g. `"event.bus"`).
///
/// Components define their own config struct in their crate, implement this
/// trait, and access their config through `RootConfig::for_component::<T>()`.
///
/// # Example
///
/// ```rust
/// use browseros_config::Config;
/// use serde::Deserialize;
///
/// #[derive(Debug, Deserialize)]
/// struct MyConfig {
///     timeout_ms: u64,
/// }
///
/// impl Config for MyConfig {
///     fn namespace() -> &'static str { "my.component" }
/// }
/// ```
pub trait Config: Debug + Send + Sync + 'static {
    /// The dot-notation namespace for this config block (e.g. `"event.bus"`).
    fn namespace() -> &'static str;
}

/// The merged, validated configuration tree for the entire runtime.
///
/// `RootConfig` stores pre-deserialized component config blocks keyed by
/// namespace.  Registration follows the three-layer precedence:
/// `defaults ← file ← env`.
///
/// # Access
///
/// Components retrieve their config block by type:
///
/// ```rust
/// # use browseros_config::Config;
/// # use serde::Deserialize;
/// # #[derive(Debug, Deserialize, serde::Serialize, Default)]
/// # struct MyConfig { timeout_ms: u64 }
/// # impl Config for MyConfig { fn namespace() -> &'static str { "my" } }
/// # use browseros_config::RootConfig;
/// # let mut root_config = RootConfig::new();
/// # let raw = serde_json::json!({"my": {"timeout_ms": 100}});
/// # root_config.register::<MyConfig>(&raw).unwrap();
/// let my_config: &MyConfig = root_config
///     .for_component::<MyConfig>()
///     .expect("MyConfig must be registered");
/// ```
pub struct RootConfig {
    blocks: HashMap<String, Arc<dyn Any + Send + Sync>>,
}

impl RootConfig {
    /// Creates an empty `RootConfig`.
    pub fn new() -> Self {
        Self {
            blocks: HashMap::new(),
        }
    }

    /// Register and validate a component config block.
    ///
    /// Precedence (three-layer merge):
    /// 1. Start with `T::default()` (baked-in defaults)
    /// 2. Overlay any values present in `raw` at `T::namespace()`
    /// 3. Deserialize the merged result
    ///
    /// If `raw` has no values for this namespace, `T::default()` is used
    /// directly (satisfying INV-028).
    pub fn register<T>(&mut self, raw: &serde_json::Value) -> Result<(), ConfigError>
    where
        T: Config + DeserializeOwned + Default + serde::Serialize,
    {
        let ns = T::namespace();

        let mut merged =
            serde_json::to_value(T::default()).map_err(|e| ConfigError::ComponentParse {
                namespace: ns.to_string(),
                detail: format!("failed to serialize default: {}", e),
            })?;

        if let Some(raw_block) = get_value(raw, ns) {
            if !raw_block.is_null() {
                merge_into(&mut merged, raw_block.clone());
            }
        }

        let config: T =
            serde_json::from_value(merged).map_err(|e| ConfigError::ComponentParse {
                namespace: ns.to_string(),
                detail: e.to_string(),
            })?;

        self.blocks.insert(ns.to_string(), Arc::new(config));
        Ok(())
    }

    /// Register a plugin config block by name.
    ///
    /// Looks up `plugin.<name>` in the raw tree, deserializes into `T`.
    /// Returns an error if the plugin section does not exist.
    pub fn register_plugin<T>(
        &mut self,
        name: &str,
        raw: &serde_json::Value,
    ) -> Result<(), ConfigError>
    where
        T: Debug + Send + Sync + 'static + DeserializeOwned,
    {
        let key = format!("plugin.{}", name);
        let value =
            get_value(raw, &key).ok_or_else(|| ConfigError::MissingField { key: key.clone() })?;

        let config: T =
            serde_json::from_value(value.clone()).map_err(|e| ConfigError::ComponentParse {
                namespace: key.clone(),
                detail: e.to_string(),
            })?;

        self.blocks.insert(key, Arc::new(config));
        Ok(())
    }

    /// Retrieve a previously registered component config block.
    pub fn for_component<T: Config>(&self) -> Option<&T> {
        self.blocks
            .get(T::namespace())
            .and_then(|b| b.downcast_ref::<T>())
    }

    /// Retrieve a previously registered plugin config block by name.
    pub fn for_plugin<T: 'static>(&self, name: &str) -> Option<&T> {
        let key = format!("plugin.{}", name);
        self.blocks.get(&key).and_then(|b| b.downcast_ref::<T>())
    }

    /// Returns `true` if a config block is registered for the given namespace.
    pub fn has_namespace(&self, namespace: &str) -> bool {
        self.blocks.contains_key(namespace)
    }

    /// Number of registered config blocks.
    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    /// Returns `true` if no config blocks are registered.
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }
}

impl Default for RootConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl Debug for RootConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let namespaces: Vec<&str> = self.blocks.keys().map(|s| s.as_str()).collect();
        f.debug_struct("RootConfig")
            .field("namespaces", &namespaces)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Deserialize, Serialize, Default, PartialEq)]
    struct TestConfig {
        value: u64,
        label: String,
    }

    impl Config for TestConfig {
        fn namespace() -> &'static str {
            "test.component"
        }
    }

    #[derive(Debug, Deserialize, Serialize, Default, PartialEq)]
    struct SimpleConfig {
        enabled: bool,
        count: u32,
    }

    impl Config for SimpleConfig {
        fn namespace() -> &'static str {
            "simple"
        }
    }

    #[test]
    fn root_config_new_is_empty() {
        let rc = RootConfig::new();
        assert!(rc.is_empty());
        assert_eq!(rc.len(), 0);
    }

    #[test]
    fn register_and_retrieve() {
        let raw = serde_json::json!({
            "test": { "component": { "value": 42, "label": "hello" } }
        });
        let mut rc = RootConfig::new();
        rc.register::<TestConfig>(&raw).unwrap();
        assert_eq!(rc.len(), 1);

        let config = rc.for_component::<TestConfig>().unwrap();
        assert_eq!(config.value, 42);
        assert_eq!(config.label, "hello");
    }

    #[test]
    fn register_uses_defaults_when_raw_missing() {
        let raw = serde_json::json!({});
        let mut rc = RootConfig::new();
        rc.register::<TestConfig>(&raw).unwrap();

        let config = rc.for_component::<TestConfig>().unwrap();
        assert_eq!(*config, TestConfig::default());
    }

    #[test]
    fn register_uses_defaults_when_raw_is_null() {
        let raw = serde_json::json!({"test": {"component": null}});
        let mut rc = RootConfig::new();
        rc.register::<TestConfig>(&raw).unwrap();
        assert_eq!(
            *rc.for_component::<TestConfig>().unwrap(),
            TestConfig::default()
        );
    }

    #[test]
    fn register_partial_merge_from_env() {
        let raw = serde_json::json!({
            "test": { "component": { "value": 99 } }
        });
        let mut rc = RootConfig::new();
        rc.register::<TestConfig>(&raw).unwrap();

        let config = rc.for_component::<TestConfig>().unwrap();
        assert_eq!(config.value, 99);
        assert_eq!(config.label, ""); // default preserved
    }

    #[test]
    fn register_type_mismatch_returns_error() {
        let raw = serde_json::json!({
            "test": { "component": { "value": "not-a-number", "label": "oops" } }
        });
        let mut rc = RootConfig::new();
        let result = rc.register::<TestConfig>(&raw);
        assert!(matches!(result, Err(ConfigError::ComponentParse { .. })));
    }

    #[test]
    fn for_component_none_when_not_registered() {
        let rc = RootConfig::new();
        assert!(rc.for_component::<TestConfig>().is_none());
    }

    #[test]
    fn has_namespace_true_after_register() {
        let raw = serde_json::json!({"test": {"component": {"value": 1, "label": "x"}}});
        let mut rc = RootConfig::new();
        rc.register::<TestConfig>(&raw).unwrap();
        assert!(rc.has_namespace("test.component"));
        assert!(!rc.has_namespace("other.namespace"));
    }

    #[test]
    fn multiple_components_independent() {
        let raw = serde_json::json!({
            "test": { "component": { "value": 10, "label": "a" } },
            "simple": { "enabled": true, "count": 5 }
        });
        let mut rc = RootConfig::new();
        rc.register::<TestConfig>(&raw).unwrap();
        rc.register::<SimpleConfig>(&raw).unwrap();

        assert_eq!(rc.len(), 2);
        assert_eq!(rc.for_component::<TestConfig>().unwrap().value, 10);
        assert!(rc.for_component::<SimpleConfig>().unwrap().enabled);
    }

    #[test]
    fn register_plugin_success() {
        #[derive(Debug, Deserialize)]
        struct PluginCfg {
            enabled: bool,
            interval_ms: u64,
        }

        let raw = serde_json::json!({
            "plugin": { "my_sensor": { "enabled": true, "interval_ms": 100 } }
        });
        let mut rc = RootConfig::new();
        rc.register_plugin::<PluginCfg>("my_sensor", &raw).unwrap();
        assert!(rc.for_plugin::<PluginCfg>("my_sensor").unwrap().enabled);
    }

    #[test]
    fn register_plugin_missing_errors() {
        #[derive(Debug, Deserialize)]
        struct PluginCfg {
            enabled: bool,
        }
        let raw = serde_json::json!({});
        let mut rc = RootConfig::new();
        assert!(rc
            .register_plugin::<PluginCfg>("nonexistent", &raw)
            .is_err());
    }

    #[test]
    fn register_plugin_isolation() {
        #[derive(Debug, Deserialize)]
        struct SensorA {
            val: u32,
        }
        #[derive(Debug, Deserialize)]
        struct SensorB {
            name: String,
        }

        let raw = serde_json::json!({
            "plugin": {
                "sensor_a": { "val": 1 },
                "sensor_b": { "name": "test" }
            }
        });
        let mut rc = RootConfig::new();
        rc.register_plugin::<SensorA>("sensor_a", &raw).unwrap();
        rc.register_plugin::<SensorB>("sensor_b", &raw).unwrap();

        assert_eq!(rc.for_plugin::<SensorA>("sensor_a").unwrap().val, 1);
        assert_eq!(rc.for_plugin::<SensorB>("sensor_b").unwrap().name, "test");
    }

    #[test]
    fn default_config_is_empty() {
        let rc = RootConfig::default();
        assert!(rc.is_empty());
    }

    #[test]
    fn debug_lists_namespaces() {
        let raw = serde_json::json!({"test": {"component": {"value": 1, "label": "x"}}});
        let mut rc = RootConfig::new();
        rc.register::<TestConfig>(&raw).unwrap();
        let debug = format!("{:?}", rc);
        assert!(debug.contains("test.component"));
    }
}
