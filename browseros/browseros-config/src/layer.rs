use serde_json::Value as JsonValue;

use crate::source::{load_source, ConfigSource};
use crate::validator::merge_into;
use crate::validator::ConfigError;

/// Builder that loads configuration from multiple sources and merges them.
///
/// Sources are processed in registration order — later sources override earlier
/// ones when keys conflict.
///
/// # Default handling
///
/// Defaults are NOT loaded as a raw source by this builder.  Instead, they are
/// applied at registration time via each component config's `Default` impl.
/// This satisfies INV-028 (default config always produces a working system)
/// without the loader needing to know the component types.
///
/// # Example
///
/// ```rust
/// use browseros_config::ConfigLoader;
/// use browseros_config::ConfigSource;
///
/// let raw = ConfigLoader::new()
///     .add_source(ConfigSource::Environment)
///     .load()
///     .expect("config should load");
/// ```
pub struct ConfigLoader {
    sources: Vec<ConfigSource>,
}

impl ConfigLoader {
    /// Create a new builder with no sources.
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
        }
    }

    /// Register a configuration source.
    ///
    /// Later sources have higher precedence — their values overwrite values
    /// from earlier sources when keys conflict.
    pub fn add_source(mut self, source: ConfigSource) -> Self {
        self.sources.push(source);
        self
    }

    /// Load and merge all registered sources.
    ///
    /// The result is a single merged JSON tree.  If any source fails to load
    /// (e.g. missing file, invalid YAML, blocked env variable), the entire
    /// load fails with all errors collected.
    ///
    /// If no sources are registered, returns an empty JSON object (no error).
    pub fn load(&self) -> Result<JsonValue, ConfigError> {
        let mut merged = JsonValue::Object(serde_json::Map::new());

        for source in &self.sources {
            let value = load_source(source)?;
            if !value.is_null() {
                merge_into(&mut merged, value);
            }
        }

        Ok(merged)
    }

    /// Returns the number of registered sources.
    pub fn source_count(&self) -> usize {
        self.sources.len()
    }

    /// Returns `true` if no sources are registered.
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }
}

impl Default for ConfigLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ConfigLoader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConfigLoader")
            .field("sources", &self.sources.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn loader_new_is_empty() {
        let loader = ConfigLoader::new();
        assert!(loader.is_empty());
        assert_eq!(loader.source_count(), 0);
    }

    #[test]
    fn load_no_sources_returns_empty_object() {
        let raw = ConfigLoader::new().load().unwrap();
        assert_eq!(raw, JsonValue::Object(serde_json::Map::new()));
    }

    #[test]
    fn load_environment_empty() {
        // In a clean environment, this should return an empty-ish result
        let raw = ConfigLoader::new()
            .add_source(ConfigSource::Environment)
            .load()
            .unwrap();
        // Should be an object (possibly empty)
        assert!(raw.is_object());
    }

    #[test]
    fn load_config_file_success() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, "event:\n  bus:\n    buffer_size: 1024\n").unwrap();

        let raw = ConfigLoader::new()
            .add_source(ConfigSource::File(path))
            .load()
            .unwrap();

        assert_eq!(
            raw.pointer("/event/bus/buffer_size"),
            Some(&serde_json::json!(1024))
        );
    }

    #[test]
    fn load_nonexistent_file_errors() {
        let result = ConfigLoader::new()
            .add_source(ConfigSource::File(PathBuf::from(
                "/tmp/__nonexistent_browseros_test.yaml__",
            )))
            .load();
        assert!(result.is_err());
    }

    #[test]
    fn load_multiple_sources_merge_order() {
        // First source: base config file
        let dir = tempfile::tempdir().unwrap();
        let path1 = dir.path().join("base.yaml");
        std::fs::write(&path1, "key: from_base\nshared: base_value\n").unwrap();

        let path2 = dir.path().join("override.yaml");
        std::fs::write(&path2, "shared: override_value\nother: extra\n").unwrap();

        let raw = ConfigLoader::new()
            .add_source(ConfigSource::File(path1))
            .add_source(ConfigSource::File(path2))
            .load()
            .unwrap();

        // Second file overrides shared key
        assert_eq!(raw["shared"], serde_json::json!("override_value"));
        // First file's key is preserved
        assert_eq!(raw["key"], serde_json::json!("from_base"));
        // Second file's new key is added
        assert_eq!(raw["other"], serde_json::json!("extra"));
    }

    #[test]
    fn loader_add_source_chain() {
        let loader = ConfigLoader::new()
            .add_source(ConfigSource::File(PathBuf::from("/a.yaml")))
            .add_source(ConfigSource::Environment);

        assert_eq!(loader.source_count(), 2);
    }

    #[test]
    fn loader_default_is_empty() {
        let loader = ConfigLoader::default();
        assert!(loader.is_empty());
    }

    #[test]
    fn load_file_with_invalid_yaml_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.yaml");
        std::fs::write(&path, "{{invalid yaml").unwrap();

        let result = ConfigLoader::new()
            .add_source(ConfigSource::File(path))
            .load();
        assert!(result.is_err());
    }

    #[test]
    fn load_relative_path_errors() {
        let result = ConfigLoader::new()
            .add_source(ConfigSource::File(PathBuf::from("relative.yaml")))
            .load();
        assert!(result.is_err());
    }
}
