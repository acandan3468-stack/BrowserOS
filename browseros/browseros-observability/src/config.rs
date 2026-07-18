use serde::{Deserialize, Serialize};

use crate::logger::LogLevel;

/// Configuration for the logging subsystem.
///
/// Registered under the `"logging"` namespace in `RootConfig`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogConfig {
    /// Minimum log level to emit (default: `Info`).
    pub level: LogLevel,
    /// Whether to use JSON output (default: `false`).
    pub json: bool,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: LogLevel::Info,
            json: false,
        }
    }
}

impl browseros_config::Config for LogConfig {
    fn namespace() -> &'static str {
        "logging"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use browseros_config::Config;

    #[test]
    fn log_config_default_level_is_info() {
        let cfg = LogConfig::default();
        assert_eq!(cfg.level, LogLevel::Info);
    }

    #[test]
    fn log_config_default_json_is_false() {
        let cfg = LogConfig::default();
        assert!(!cfg.json);
    }

    #[test]
    fn log_config_serde_roundtrip() {
        let cfg = LogConfig::default();
        let json = serde_json::to_string(&cfg).unwrap();
        let deserialized: LogConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg.level, deserialized.level);
    }

    #[test]
    fn log_config_namespace() {
        assert_eq!(LogConfig::namespace(), "logging");
    }
}
