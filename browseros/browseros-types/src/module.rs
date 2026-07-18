//! Module lifecycle descriptors for BrowserOS.
//!
//! Defines the [`ModuleDescriptor`] type used by the lifecycle manager to
//! track registered modules and their runtime state.

use crate::identifiers::ModuleId;
use crate::value::ModuleType;
use serde::{Deserialize, Serialize};

/// Descriptor for a module managed by the BrowserOS lifecycle manager.
///
/// Each running or registered module has a corresponding descriptor that
/// captures its identity, type classification, and enabled/disabled state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleDescriptor {
    /// Unique identifier for this module instance.
    pub module_id: ModuleId,
    /// Classification of this module (e.g. built-in, user, plugin).
    pub module_type: ModuleType,
    /// Human-readable description of the module's purpose.
    pub description: String,
    /// Whether the module is currently enabled and accepting work.
    pub enabled: bool,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::SemVer;

    #[test]
    fn module_descriptor_creation() {
        let desc = ModuleDescriptor {
            module_id: ModuleId::new("test-module", SemVer::new(1, 0, 0)),
            module_type: ModuleType::Core,
            description: "A test module".into(),
            enabled: true,
        };
        assert_eq!(desc.module_id.name(), "test-module");
        assert_eq!(desc.module_id.version(), &SemVer::new(1, 0, 0));
        assert_eq!(desc.module_type, ModuleType::Core);
        assert_eq!(desc.description, "A test module");
        assert!(desc.enabled);
    }

    #[test]
    fn module_descriptor_disabled() {
        let desc = ModuleDescriptor {
            module_id: ModuleId::new("plugin-x", SemVer::new(0, 5, 0)),
            module_type: ModuleType::Plugin,
            description: "A plugin".into(),
            enabled: false,
        };
        assert!(!desc.enabled);
    }

    #[test]
    fn module_descriptor_clone() {
        let a = ModuleDescriptor {
            module_id: ModuleId::new("m", SemVer::new(1, 0, 0)),
            module_type: ModuleType::Service,
            description: "svc".into(),
            enabled: true,
        };
        let b = a.clone();
        assert_eq!(a.module_id, b.module_id);
        assert_eq!(a.module_type, b.module_type);
        assert_eq!(a.enabled, b.enabled);
    }

    #[test]
    fn module_descriptor_serde_roundtrip() {
        let a = ModuleDescriptor {
            module_id: ModuleId::new("serde-module", SemVer::new(2, 3, 4)),
            module_type: ModuleType::Sensor,
            description: "sensor module".into(),
            enabled: true,
        };
        let json = serde_json::to_string(&a).unwrap();
        let b: ModuleDescriptor = serde_json::from_str(&json).unwrap();
        assert_eq!(a.module_id, b.module_id);
        assert_eq!(a.module_type, b.module_type);
        assert_eq!(a.description, b.description);
        assert_eq!(a.enabled, b.enabled);
    }
}
