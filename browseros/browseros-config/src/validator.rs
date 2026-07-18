use std::fmt;
use std::path::PathBuf;

use serde::de::DeserializeOwned;

/// Error returned by the configuration system.
#[derive(Debug)]
pub enum ConfigError {
    /// The config file could not be read or parsed.
    FileError { path: PathBuf, detail: String },
    /// A configuration value failed to parse into the expected type.
    ParseError { key: String, detail: String },
    /// One or more validation rules failed.
    Validation { errors: Vec<ValidationError> },
    /// A required field has no value and no default.
    MissingField { key: String },
    /// An environment variable was set for a blocked config key.
    BlockedEnvVar { key: String, var: String },
    /// A nested component config block could not be deserialized.
    ComponentParse { namespace: String, detail: String },
    /// The config file path is not absolute (security requirement).
    NonAbsolutePath { path: PathBuf },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::FileError { path, detail } => {
                write!(f, "config file error at {}: {}", path.display(), detail)
            }
            ConfigError::ParseError { key, detail } => {
                write!(f, "config parse error for '{}': {}", key, detail)
            }
            ConfigError::Validation { errors } => {
                write!(f, "config validation failed with {} error(s)", errors.len())
            }
            ConfigError::MissingField { key } => {
                write!(f, "config missing required field '{}'", key)
            }
            ConfigError::BlockedEnvVar { key, var } => {
                write!(
                    f,
                    "config error: '{}' cannot be set via environment variable '{}'",
                    key, var
                )
            }
            ConfigError::ComponentParse { namespace, detail } => {
                write!(
                    f,
                    "config parse error for component '{}': {}",
                    namespace, detail
                )
            }
            ConfigError::NonAbsolutePath { path } => {
                write!(
                    f,
                    "config file path must be absolute, got '{}'",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for ConfigError {}

/// A single validation failure.
#[derive(Debug, Clone)]
pub struct ValidationError {
    /// The config key that failed validation.
    pub key: String,
    /// Human-readable description of the problem.
    pub message: String,
    /// The actual value, if representable as a string.
    pub value: Option<String>,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.value {
            Some(val) => write!(f, "'{}': {} (got '{}')", self.key, self.message, val),
            None => write!(f, "'{}': {}", self.key, self.message),
        }
    }
}

/// A validator function for a single config key.
pub type ValidatorFn = Box<dyn Fn(&str, &serde_json::Value) -> Vec<ValidationError> + Send + Sync>;

/// Deserializes a JSON value into the target type.
pub fn deserialize_value<T: DeserializeOwned>(
    key: &str,
    value: &serde_json::Value,
) -> Result<T, ConfigError> {
    T::deserialize(value).map_err(|e| ConfigError::ParseError {
        key: key.to_string(),
        detail: e.to_string(),
    })
}

/// Extract a value from a flat map by dot-notation key.
pub fn get_value<'a>(map: &'a serde_json::Value, key: &str) -> Option<&'a serde_json::Value> {
    let parts: Vec<&str> = key.split('.').collect();
    let mut current = map;
    for part in parts {
        match current {
            serde_json::Value::Object(obj) => {
                current = obj.get(part)?;
            }
            _ => return None,
        }
    }
    Some(current)
}

/// Collect all leaf keys in a JSON Value tree, returns dot-notation paths.
pub fn collect_keys(value: &serde_json::Value) -> Vec<String> {
    fn recurse(prefix: &str, value: &serde_json::Value, keys: &mut Vec<String>) {
        match value {
            serde_json::Value::Object(obj) => {
                for (k, v) in obj {
                    let new_prefix = if prefix.is_empty() {
                        k.clone()
                    } else {
                        format!("{}.{}", prefix, k)
                    };
                    recurse(&new_prefix, v, keys);
                }
            }
            _ => {
                if !prefix.is_empty() {
                    keys.push(prefix.to_string());
                }
            }
        }
    }
    let mut keys = Vec::new();
    recurse("", value, &mut keys);
    keys
}

/// Build a default JSON tree from a type that implements Default + Serialize.
pub fn to_default_value<T: Default + serde::Serialize>() -> serde_json::Value {
    serde_json::to_value(T::default()).unwrap_or(serde_json::Value::Null)
}

/// Merge `overlay` into `base`. Overlay keys overwrite base keys.
/// Works on flat serde_json::Value trees (no deep merge).
pub fn merge_into(base: &mut serde_json::Value, overlay: serde_json::Value) {
    match (base, overlay) {
        (base @ serde_json::Value::Object(_), serde_json::Value::Object(overlay_map)) => {
            let base_map = base.as_object_mut().unwrap();
            for (k, v) in overlay_map {
                if base_map.contains_key(&k) && base_map[&k].is_object() && v.is_object() {
                    merge_into(&mut base_map[&k], v);
                } else {
                    base_map.insert(k, v);
                }
            }
        }
        (base, overlay) => *base = overlay,
    }
}

/// Validate that environment variables follow the BROWSEROS_ naming convention.
pub fn is_valid_env_name(name: &str) -> bool {
    name.starts_with("BROWSEROS_")
        && name.len() > 10
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Convert an env var name to a config key.
///
/// Uses `__` (double underscore) as the hierarchy separator and `_`
/// (single underscore) as part of a key name.
///
/// E.g., `BROWSEROS_EVENT__BUS__BUFFER_SIZE` → `event.bus.buffer_size`
pub fn env_to_config_key(env_name: &str) -> String {
    let body = &env_name["BROWSEROS_".len()..];
    body.split("__")
        .map(|part| part.to_lowercase())
        .collect::<Vec<_>>()
        .join(".")
}

/// Config keys that must NOT be set via environment variables.
pub const ENV_BLOCKLIST: &[&str] = &["plugin.scan_path", "store.event_store.path"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_keys_flat() {
        let v: serde_json::Value = serde_json::json!({
            "event": {
                "bus": {
                    "buffer_size": 1024,
                    "timeout_ms": 5000
                }
            }
        });
        let keys = collect_keys(&v);
        assert!(keys.contains(&"event.bus.buffer_size".to_string()));
        assert!(keys.contains(&"event.bus.timeout_ms".to_string()));
        assert_eq!(keys.len(), 2);
    }

    #[test]
    fn collect_keys_empty_object() {
        let v = serde_json::json!({});
        let keys = collect_keys(&v);
        assert!(keys.is_empty());
    }

    #[test]
    fn collect_keys_flat_no_nesting() {
        let v = serde_json::json!({"port": 8080});
        let keys = collect_keys(&v);
        assert_eq!(keys, vec!["port"]);
    }

    #[test]
    fn get_value_nested() {
        let v = serde_json::json!({"event": {"bus": {"buffer_size": 1024}}});
        let val = get_value(&v, "event.bus.buffer_size");
        assert_eq!(val, Some(&serde_json::json!(1024)));
    }

    #[test]
    fn get_value_missing() {
        let v = serde_json::json!({"event": {}});
        let val = get_value(&v, "event.bus.buffer_size");
        assert!(val.is_none());
    }

    #[test]
    fn get_value_partial_missing() {
        let v = serde_json::json!({"event": {"bus": {}}});
        let val = get_value(&v, "event.bus.buffer_size");
        assert!(val.is_none());
    }

    #[test]
    fn merge_overwrites_scalar() {
        let mut base = serde_json::json!({"key": "old"});
        let overlay = serde_json::json!({"key": "new"});
        merge_into(&mut base, overlay);
        assert_eq!(base, serde_json::json!({"key": "new"}));
    }

    #[test]
    fn merge_keeps_unrelated() {
        let mut base = serde_json::json!({"a": 1, "b": 2});
        let overlay = serde_json::json!({"b": 3});
        merge_into(&mut base, overlay);
        assert_eq!(base, serde_json::json!({"a": 1, "b": 3}));
    }

    #[test]
    fn merge_into_empty() {
        let mut base = serde_json::json!({});
        let overlay = serde_json::json!({"key": "val"});
        merge_into(&mut base, overlay);
        assert_eq!(base, serde_json::json!({"key": "val"}));
    }

    #[test]
    fn merge_into_nested_object() {
        let mut base = serde_json::json!({"outer": {"inner": 1, "keep": 2}});
        let overlay = serde_json::json!({"outer": {"inner": 99}});
        merge_into(&mut base, overlay);
        assert_eq!(base, serde_json::json!({"outer": {"inner": 99, "keep": 2}}));
    }

    #[test]
    fn merge_into_nested_overwrite_with_scalar() {
        let mut base = serde_json::json!({"key": {"nested": 1}});
        let overlay = serde_json::json!({"key": "scalar"});
        merge_into(&mut base, overlay);
        assert_eq!(base, serde_json::json!({"key": "scalar"}));
    }

    #[test]
    fn deserialize_value_integer() {
        let v = serde_json::json!(42);
        let result: Result<u32, _> = deserialize_value("test", &v);
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn deserialize_value_type_mismatch() {
        let v = serde_json::json!("not-an-int");
        let result: Result<u32, ConfigError> = deserialize_value("test.key", &v);
        match result {
            Err(ConfigError::ParseError { key, detail }) => {
                assert_eq!(key, "test.key");
                assert!(detail.contains("not-an-int"));
            }
            other => panic!("expected ParseError, got {:?}", other),
        }
    }

    #[test]
    fn is_valid_env_name_correct() {
        assert!(is_valid_env_name("BROWSEROS_EVENT_BUS_SIZE"));
    }

    #[test]
    fn is_valid_env_name_no_prefix() {
        assert!(!is_valid_env_name("EVENT_BUS_SIZE"));
    }

    #[test]
    fn is_valid_env_name_too_short() {
        assert!(!is_valid_env_name("BROWSEROS_"));
    }

    #[test]
    fn is_valid_env_name_special_chars() {
        assert!(!is_valid_env_name("BROWSEROS_EVENT-BUS"));
    }

    #[test]
    fn env_to_config_key_basic() {
        assert_eq!(
            env_to_config_key("BROWSEROS_EVENT__BUS__BUFFER_SIZE"),
            "event.bus.buffer_size"
        );
    }

    #[test]
    fn env_to_config_key_single() {
        assert_eq!(env_to_config_key("BROWSEROS_PORT"), "port");
    }

    #[test]
    fn env_to_config_key_with_inner_underscore() {
        assert_eq!(
            env_to_config_key("BROWSEROS_SCHEDULER__MAX_CONCURRENT_TASKS"),
            "scheduler.max_concurrent_tasks"
        );
    }

    #[test]
    fn validation_error_display_with_value() {
        let err = ValidationError {
            key: "test.key".into(),
            message: "must be positive".into(),
            value: Some("-1".into()),
        };
        let s = err.to_string();
        assert!(s.contains("test.key"));
        assert!(s.contains("must be positive"));
        assert!(s.contains("-1"));
    }

    #[test]
    fn validation_error_display_without_value() {
        let err = ValidationError {
            key: "test.key".into(),
            message: "must be present".into(),
            value: None,
        };
        assert_eq!(err.to_string(), "'test.key': must be present");
    }

    #[test]
    fn config_error_file_error_display() {
        let err = ConfigError::FileError {
            path: PathBuf::from("/etc/browseros.yaml"),
            detail: "No such file".into(),
        };
        assert!(err.to_string().contains("No such file"));
    }

    #[test]
    fn config_error_blocked_env_var_display() {
        let err = ConfigError::BlockedEnvVar {
            key: "plugin.scan_path".into(),
            var: "BROWSEROS_PLUGIN_SCAN_PATH".into(),
        };
        let s = err.to_string();
        assert!(s.contains("plugin.scan_path"));
        assert!(s.contains("BROWSEROS_PLUGIN_SCAN_PATH"));
    }

    #[test]
    fn config_error_missing_field_display() {
        let err = ConfigError::MissingField {
            key: "required.key".into(),
        };
        assert!(err.to_string().contains("required.key"));
    }

    #[test]
    fn config_error_non_absolute_path() {
        let err = ConfigError::NonAbsolutePath {
            path: PathBuf::from("relative.yaml"),
        };
        assert!(err.to_string().contains("relative.yaml"));
    }

    #[test]
    fn env_blocklist_has_expected_keys() {
        assert!(ENV_BLOCKLIST.contains(&"plugin.scan_path"));
        assert!(ENV_BLOCKLIST.contains(&"store.event_store.path"));
        assert_eq!(ENV_BLOCKLIST.len(), 2);
    }

    #[test]
    fn collect_keys_deeply_nested() {
        let v = serde_json::json!({
            "a": {"b": {"c": 1, "d": 2}, "e": 3},
            "f": 4
        });
        let mut keys = collect_keys(&v);
        keys.sort();
        assert_eq!(keys, vec!["a.b.c", "a.b.d", "a.e", "f"]);
    }

    #[test]
    fn merge_preserves_non_overlapping() {
        let mut base = serde_json::json!({"a": 1});
        let overlay = serde_json::json!({"b": 2});
        merge_into(&mut base, overlay);
        assert_eq!(base, serde_json::json!({"a": 1, "b": 2}));
    }
}
