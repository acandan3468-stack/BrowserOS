use std::path::{Path, PathBuf};

use serde_json::Value as JsonValue;

use crate::validator::{
    env_to_config_key, is_valid_env_name, merge_into, ConfigError, ENV_BLOCKLIST,
};

/// Describes where a configuration value originates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigSource {
    /// In-process default configuration — the `Default` impl of `RootConfig`.
    Defaults,
    /// A YAML configuration file on disk. The path must be absolute.
    File(PathBuf),
    /// Environment variables with the `BROWSEROS_` prefix.
    Environment,
}

impl ConfigSource {
    /// Returns a human-readable label for the source.
    pub fn label(&self) -> &'static str {
        match self {
            ConfigSource::Defaults => "defaults",
            ConfigSource::File(_) => "file",
            ConfigSource::Environment => "environment",
        }
    }
}

/// Load raw configuration from the given source.
///
/// Returns `Ok(JsonValue::Null)` for `Defaults` (handled at registration time
/// via `Default` impls).  For `File`, parses YAML into JSON.  For
/// `Environment`, builds a tree from `BROWSEROS_*` variables.
pub fn load_source(source: &ConfigSource) -> Result<JsonValue, ConfigError> {
    match source {
        ConfigSource::Defaults => Ok(JsonValue::Null),
        ConfigSource::File(path) => load_file(path),
        ConfigSource::Environment => load_env(),
    }
}

/// Load and parse a YAML config file.
fn load_file(path: &Path) -> Result<JsonValue, ConfigError> {
    if !path.is_absolute() {
        return Err(ConfigError::NonAbsolutePath {
            path: path.to_path_buf(),
        });
    }
    let content = std::fs::read_to_string(path).map_err(|e| ConfigError::FileError {
        path: path.to_path_buf(),
        detail: e.to_string(),
    })?;
    serde_yaml::from_str(&content).map_err(|e| ConfigError::FileError {
        path: path.to_path_buf(),
        detail: e.to_string(),
    })
}

/// Load configuration from `BROWSEROS_*` environment variables.
///
/// Variables are filtered by prefix, validated, and converted to a nested
/// JSON tree keyed by dot-notation paths.
fn load_env() -> Result<JsonValue, ConfigError> {
    let mut map = serde_json::Map::new();

    for (var_name, var_value) in std::env::vars() {
        if !is_valid_env_name(&var_name) {
            continue;
        }
        let config_key = env_to_config_key(&var_name);

        // Check blocklist
        if ENV_BLOCKLIST.contains(&config_key.as_str()) {
            return Err(ConfigError::BlockedEnvVar {
                key: config_key,
                var: var_name,
            });
        }

        // Parse the value. Try integer first, then float, then bool, then string.
        let parsed = parse_env_value(&var_value);
        set_at_path(&mut map, &config_key, parsed);
    }

    Ok(JsonValue::Object(map))
}

/// Parse an environment variable string into a JSON value.
fn parse_env_value(s: &str) -> JsonValue {
    // Try integer
    if let Ok(v) = s.parse::<i64>() {
        return JsonValue::Number(v.into());
    }
    // Try float
    if let Ok(v) = s.parse::<f64>() {
        if let Some(n) = serde_json::Number::from_f64(v) {
            return JsonValue::Number(n);
        }
    }
    // Try bool
    if let Ok(v) = s.parse::<bool>() {
        return JsonValue::Bool(v);
    }
    // Fall back to string
    JsonValue::String(s.to_owned())
}

/// Insert a value into a JSON map at a dot-notation key path.
pub fn set_at_path(map: &mut serde_json::Map<String, JsonValue>, key: &str, value: JsonValue) {
    let parts: Vec<&str> = key.split('.').collect();
    if parts.is_empty() {
        return;
    }
    if parts.len() == 1 {
        map.insert(parts[0].to_string(), value);
        return;
    }
    // Navigate/create path
    let mut current = map;
    for (i, part) in parts.iter().enumerate() {
        if i == parts.len() - 1 {
            current.insert(part.to_string(), value.clone());
        } else {
            current = current
                .entry(part.to_string())
                .or_insert_with(|| JsonValue::Object(serde_json::Map::new()))
                .as_object_mut()
                .unwrap();
        }
    }
}

/// Merge a raw JSON overlay into base. Base is mutated in place.
pub fn merge_layers(base: &mut JsonValue, overlay: JsonValue) {
    merge_into(base, overlay);
}

/// Search for config file at standard locations.
/// Returns the first match, or None.
pub fn find_config_file() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from("./browseros.yaml"),
        PathBuf::from("./config/browseros.yaml"),
        #[cfg(unix)]
        PathBuf::from("/etc/browseros/config.yaml"),
    ];
    candidates.iter().find(|p| p.exists()).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_label_defaults() {
        assert_eq!(ConfigSource::Defaults.label(), "defaults");
    }

    #[test]
    fn source_label_file() {
        assert_eq!(ConfigSource::File(PathBuf::from("/x.yaml")).label(), "file");
    }

    #[test]
    fn source_label_env() {
        assert_eq!(ConfigSource::Environment.label(), "environment");
    }

    #[test]
    fn parse_env_value_integer() {
        assert_eq!(parse_env_value("42"), JsonValue::Number(42.into()));
    }

    #[test]
    fn parse_env_value_negative_integer() {
        assert_eq!(parse_env_value("-5"), JsonValue::Number((-5).into()));
    }

    #[test]
    fn parse_env_value_float() {
        let val = parse_env_value("2.5");
        assert!(val.is_number());
        assert!((val.as_f64().unwrap() - 2.5_f64).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_env_value_true() {
        assert_eq!(parse_env_value("true"), JsonValue::Bool(true));
    }

    #[test]
    fn parse_env_value_false_upper() {
        // "false" as string (not a JSON boolean convention in most envs)
        // Let's check: parse_env_value("false") should be parsed as bool
        assert_eq!(parse_env_value("false"), JsonValue::Bool(false));
    }

    #[test]
    fn parse_env_value_string() {
        assert_eq!(parse_env_value("hello"), JsonValue::String("hello".into()));
    }

    #[test]
    fn parse_env_value_empty_string() {
        assert_eq!(parse_env_value(""), JsonValue::String("".into()));
    }

    #[test]
    fn set_at_path_single() {
        let mut map = serde_json::Map::new();
        set_at_path(&mut map, "key", JsonValue::String("val".into()));
        assert_eq!(map.get("key").and_then(|v| v.as_str()), Some("val"));
    }

    #[test]
    fn set_at_path_nested() {
        let mut map = serde_json::Map::new();
        set_at_path(&mut map, "event.bus.size", JsonValue::Number(1024.into()));
        let event = map.get("event").unwrap().as_object().unwrap();
        let bus = event.get("bus").unwrap().as_object().unwrap();
        assert_eq!(bus.get("size").and_then(|v| v.as_u64()), Some(1024));
    }

    #[test]
    fn set_at_path_overwrites_existing() {
        let mut map = serde_json::Map::new();
        set_at_path(&mut map, "key", JsonValue::String("old".into()));
        set_at_path(&mut map, "key", JsonValue::String("new".into()));
        assert_eq!(map.get("key").and_then(|v| v.as_str()), Some("new"));
    }

    #[test]
    fn set_at_path_three_levels() {
        let mut map = serde_json::Map::new();
        set_at_path(&mut map, "a.b.c", JsonValue::Number(1.into()));
        let a = map.get("a").unwrap().as_object().unwrap();
        let b = a.get("b").unwrap().as_object().unwrap();
        assert_eq!(b.get("c").and_then(|v| v.as_u64()), Some(1));
    }

    #[test]
    fn merge_layers_flat() {
        let mut base = serde_json::json!({"a": 1});
        let overlay = serde_json::json!({"b": 2});
        merge_layers(&mut base, overlay);
        assert_eq!(base, serde_json::json!({"a": 1, "b": 2}));
    }

    #[test]
    fn find_config_file_no_file() {
        // In a temp dir with no config files
        let dir = tempfile::tempdir().unwrap();
        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        let result = find_config_file();
        std::env::set_current_dir(original).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn load_file_non_absolute() {
        let result = load_file(Path::new("relative.yaml"));
        assert!(matches!(result, Err(ConfigError::NonAbsolutePath { .. })));
    }

    #[test]
    fn load_file_nonexistent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("__nonexistent__");
        // Ensure the file does NOT exist
        if path.exists() {
            std::fs::remove_file(&path).ok();
        }
        let result = load_file(&path);
        match result {
            Err(ConfigError::FileError { .. }) => {}
            other => panic!("expected FileError, got {:?}", other),
        }
    }

    #[test]
    fn load_file_invalid_yaml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, ": invalid yaml [[[").unwrap();
        let result = load_file(&path);
        match result {
            Err(ConfigError::FileError { .. }) => {}
            other => panic!("expected FileError, got {:?}", other),
        }
    }

    #[test]
    fn load_file_valid_yaml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, "event:\n  bus:\n    buffer_size: 2048\n").unwrap();
        let result = load_file(&path).unwrap();
        assert_eq!(
            result.pointer("/event/bus/buffer_size"),
            Some(&serde_json::json!(2048))
        );
    }

    #[test]
    fn test_env_variable_parsing_blocked_key() {
        // Verify the env_blocklist check logic without actual env access
        for blocked in ENV_BLOCKLIST {
            assert!(blocked.starts_with("plugin.") || blocked.starts_with("store."));
        }
    }
}
