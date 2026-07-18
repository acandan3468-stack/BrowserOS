//! Value objects for the BrowserOS domain.
//!
//! These types represent domain-level values that are not identifiers but carry
//! semantic meaning across the system (versioning, content negotiation, quality
//! of service, severity, etc.).

use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ===========================================================================
// SemVer
// ===========================================================================

/// A semantic version number (`major.minor.patch`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SemVer {
    /// Major version component.
    pub major: u32,
    /// Minor version component.
    pub minor: u32,
    /// Patch version component.
    pub patch: u32,
}

impl SemVer {
    /// Creates a new semantic version.
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }
}

impl fmt::Display for SemVer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Error returned when parsing a [`SemVer`] from a string fails.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SemVerParseError {
    /// The string did not match the `major.minor.patch` format.
    #[error("invalid semver format, expected `major.minor.patch`")]
    InvalidFormat,
    /// One of the numeric components could not be parsed.
    #[error("invalid semver component: {0}")]
    InvalidComponent(#[from] std::num::ParseIntError),
}

impl FromStr for SemVer {
    type Err = SemVerParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 3 {
            return Err(SemVerParseError::InvalidFormat);
        }
        Ok(Self {
            major: parts[0].parse()?,
            minor: parts[1].parse()?,
            patch: parts[2].parse()?,
        })
    }
}

impl PartialOrd for SemVer {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SemVer {
    fn cmp(&self, other: &Self) -> Ordering {
        self.major
            .cmp(&other.major)
            .then(self.minor.cmp(&other.minor))
            .then(self.patch.cmp(&other.patch))
    }
}

// ===========================================================================
// ContentType
// ===========================================================================

/// A media-type string used for content negotiation (e.g.
/// `"application/x.browseros.event.v1+json"`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ContentType(String);

impl ContentType {
    /// Creates a new content type from a string slice.
    pub fn new(s: &str) -> Self {
        Self(s.to_owned())
    }

    /// Returns a reference to the inner string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ContentType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for ContentType {
    type Err = std::convert::Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_owned()))
    }
}

// ===========================================================================
// Priority
// ===========================================================================

/// Message or task priority level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Priority {
    /// Critical — must be handled immediately.
    Critical,
    /// High — urgent but not critical.
    High,
    /// Normal — default priority.
    #[default]
    Normal,
    /// Low — background / best-effort.
    Low,
}

impl Priority {
    fn rank(self) -> u8 {
        match self {
            Priority::Critical => 4,
            Priority::High => 3,
            Priority::Normal => 2,
            Priority::Low => 1,
        }
    }
}

impl PartialOrd for Priority {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.rank().cmp(&other.rank()))
    }
}

// ===========================================================================
// DeliveryGuarantee
// ===========================================================================

/// Delivery guarantee level for messages or events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeliveryGuarantee {
    /// The message may be delivered zero or one times (fire-and-forget).
    AtMostOnce,
    /// The message will be retried until acknowledged.
    AtLeastOnce,
    /// The message is delivered exactly once (deduplicated).
    ExactlyOnce,
}

// ===========================================================================
// LogLevel
// ===========================================================================

/// Severity level for log messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogLevel {
    /// Finest-grained diagnostic information.
    Trace,
    /// Detailed diagnostic information.
    Debug,
    /// General operational information.
    Info,
    /// Potentially harmful situations.
    Warn,
    /// Error events that might still allow the system to continue.
    Error,
}

impl LogLevel {
    fn rank(self) -> u8 {
        match self {
            LogLevel::Error => 5,
            LogLevel::Warn => 4,
            LogLevel::Info => 3,
            LogLevel::Debug => 2,
            LogLevel::Trace => 1,
        }
    }
}

impl PartialOrd for LogLevel {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.rank().cmp(&other.rank()))
    }
}

// ===========================================================================
// ModuleType
// ===========================================================================

/// Classification of a module within the runtime.
///
/// This enum is `#[non_exhaustive]` — new variants may be added in future
/// versions without a breaking semver bump.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ModuleType {
    /// Core runtime component (EventBus, Scheduler, etc.)
    Core,
    /// Dynamically loaded plugin
    Plugin,
    /// Long-running service
    Service,
    /// Perception sensor (DOM, Vision, Network)
    Sensor,
    /// Executable skill
    Skill,
    /// Utility tool
    Tool,
    /// Hardware or OS driver
    Driver,
}

impl ModuleType {
    /// Returns a human-readable label for this module type.
    pub fn as_str(&self) -> &'static str {
        match self {
            ModuleType::Core => "core",
            ModuleType::Plugin => "plugin",
            ModuleType::Service => "service",
            ModuleType::Sensor => "sensor",
            ModuleType::Skill => "skill",
            ModuleType::Tool => "tool",
            ModuleType::Driver => "driver",
        }
    }
}

impl std::fmt::Display for ModuleType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ===========================================================================
// ErrorCode
// ===========================================================================

/// A domain-specific error code (e.g. `"EVENT_BUS_FULL"`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ErrorCode(String);

impl ErrorCode {
    /// Creates a new error code from a string slice.
    pub fn new(s: &str) -> Self {
        Self(s.to_owned())
    }

    /// Returns a reference to the inner string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for ErrorCode {
    type Err = std::convert::Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_owned()))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Ordering;

    // ─── SemVer ───────────────────────────────────────────────────────────

    #[test]
    fn semver_new() {
        let v = SemVer::new(1, 2, 3);
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
    }

    #[test]
    fn semver_display() {
        assert_eq!(SemVer::new(0, 0, 0).to_string(), "0.0.0");
        assert_eq!(SemVer::new(1, 20, 300).to_string(), "1.20.300");
        assert_eq!(
            SemVer::new(u32::MAX, u32::MAX, u32::MAX).to_string(),
            "4294967295.4294967295.4294967295"
        );
    }

    #[test]
    fn semver_from_str_valid() {
        let v: SemVer = "1.2.3".parse().unwrap();
        assert_eq!(v, SemVer::new(1, 2, 3));

        let v: SemVer = "0.0.0".parse().unwrap();
        assert_eq!(v, SemVer::new(0, 0, 0));
    }

    #[test]
    fn semver_from_str_invalid_format() {
        assert_eq!(
            "1.2".parse::<SemVer>().unwrap_err(),
            SemVerParseError::InvalidFormat
        );
        assert_eq!(
            "1.2.3.4".parse::<SemVer>().unwrap_err(),
            SemVerParseError::InvalidFormat
        );
        assert_eq!(
            "".parse::<SemVer>().unwrap_err(),
            SemVerParseError::InvalidFormat
        );
        assert_eq!(
            "abc".parse::<SemVer>().unwrap_err(),
            SemVerParseError::InvalidFormat
        );
    }

    #[test]
    fn semver_from_str_invalid_component() {
        let err = "a.b.c".parse::<SemVer>().unwrap_err();
        assert!(matches!(err, SemVerParseError::InvalidComponent(_)));
        let err = "1.2.x".parse::<SemVer>().unwrap_err();
        assert!(matches!(err, SemVerParseError::InvalidComponent(_)));
    }

    #[test]
    fn semver_ordering() {
        let v100 = SemVer::new(1, 0, 0);
        let v101 = SemVer::new(1, 0, 1);
        let v110 = SemVer::new(1, 1, 0);
        let v200 = SemVer::new(2, 0, 0);

        assert!(v100 < v101);
        assert!(v101 < v110);
        assert!(v110 < v200);
        assert!(v200 > v100);
        assert_eq!(v100.cmp(&v100), Ordering::Equal);
        assert_eq!(v100.cmp(&v101), Ordering::Less);
        assert_eq!(v101.cmp(&v100), Ordering::Greater);
    }

    #[test]
    fn semver_partial_ord() {
        let v1 = SemVer::new(1, 0, 0);
        let v2 = SemVer::new(2, 0, 0);
        assert_eq!(v1.partial_cmp(&v2), Some(Ordering::Less));
        assert_eq!(v2.partial_cmp(&v1), Some(Ordering::Greater));
        assert_eq!(v1.partial_cmp(&v1), Some(Ordering::Equal));
    }

    #[test]
    fn semver_clone_copy_eq_hash() {
        let a = SemVer::new(1, 2, 3);
        let b = a;
        assert_eq!(a, b);
        let c = SemVer::new(1, 2, 3);
        assert_eq!(a, c);
        // Hash
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h1 = DefaultHasher::new();
        a.hash(&mut h1);
        let mut h2 = DefaultHasher::new();
        c.hash(&mut h2);
        assert_eq!(h1.finish(), h2.finish());
    }

    #[test]
    fn semver_serde_roundtrip() {
        let v = SemVer::new(3, 2, 1);
        let json = serde_json::to_string(&v).unwrap();
        let deserialized: SemVer = serde_json::from_str(&json).unwrap();
        assert_eq!(v, deserialized);
    }

    // ─── ContentType ──────────────────────────────────────────────────────

    #[test]
    fn content_type_new() {
        let ct = ContentType::new("application/json");
        assert_eq!(ct.as_str(), "application/json");
    }

    #[test]
    fn content_type_display() {
        let ct = ContentType::new("text/plain");
        assert_eq!(ct.to_string(), "text/plain");
    }

    #[test]
    fn content_type_from_str() {
        let ct: ContentType = "application/x.browseros.event.v1+json".parse().unwrap();
        assert_eq!(ct.as_str(), "application/x.browseros.event.v1+json");
    }

    #[test]
    fn content_type_empty() {
        let ct = ContentType::new("");
        assert_eq!(ct.as_str(), "");
        assert_eq!(ct.to_string(), "");
    }

    #[test]
    fn content_type_clone_eq_hash() {
        let a = ContentType::new("app/json");
        let b = a.clone();
        assert_eq!(a, b);
        let c = ContentType::new("app/json");
        assert_eq!(a, c);

        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h1 = DefaultHasher::new();
        a.hash(&mut h1);
        let mut h2 = DefaultHasher::new();
        c.hash(&mut h2);
        assert_eq!(h1.finish(), h2.finish());
    }

    // ─── Priority ─────────────────────────────────────────────────────────

    #[test]
    fn priority_default() {
        assert_eq!(Priority::default(), Priority::Normal);
    }

    #[test]
    fn priority_ordering() {
        assert!(Priority::Critical > Priority::High);
        assert!(Priority::High > Priority::Normal);
        assert!(Priority::Normal > Priority::Low);
        assert!(Priority::Low < Priority::Critical);
        assert_eq!(Priority::Normal, Priority::Normal);
        assert_eq!(
            Priority::Normal.partial_cmp(&Priority::High),
            Some(Ordering::Less)
        );
        assert_eq!(
            Priority::Critical.partial_cmp(&Priority::Low),
            Some(Ordering::Greater)
        );
    }

    #[test]
    fn priority_clone_copy() {
        let a = Priority::High;
        let b = a;
        assert_eq!(a, b);
    }

    // ─── DeliveryGuarantee ────────────────────────────────────────────────

    #[test]
    fn delivery_guarantee_clone_copy_eq() {
        let a = DeliveryGuarantee::ExactlyOnce;
        let b = a;
        assert_eq!(a, b);
        assert_eq!(DeliveryGuarantee::AtMostOnce, DeliveryGuarantee::AtMostOnce);
        assert_eq!(
            DeliveryGuarantee::AtLeastOnce,
            DeliveryGuarantee::AtLeastOnce
        );
        assert_ne!(
            DeliveryGuarantee::AtMostOnce,
            DeliveryGuarantee::AtLeastOnce
        );
    }

    // ─── LogLevel ─────────────────────────────────────────────────────────

    #[test]
    fn log_level_ordering() {
        assert!(LogLevel::Error > LogLevel::Warn);
        assert!(LogLevel::Warn > LogLevel::Info);
        assert!(LogLevel::Info > LogLevel::Debug);
        assert!(LogLevel::Debug > LogLevel::Trace);
        assert!(LogLevel::Trace < LogLevel::Error);
        assert_eq!(LogLevel::Info, LogLevel::Info);
        assert_eq!(
            LogLevel::Error.partial_cmp(&LogLevel::Trace),
            Some(Ordering::Greater)
        );
        assert_eq!(
            LogLevel::Trace.partial_cmp(&LogLevel::Info),
            Some(Ordering::Less)
        );
    }

    #[test]
    fn log_level_clone_copy() {
        let a = LogLevel::Warn;
        let b = a;
        assert_eq!(a, b);
    }

    // ─── ModuleType ───────────────────────────────────────────────────────

    #[test]
    fn module_type_as_str() {
        assert_eq!(ModuleType::Core.as_str(), "core");
        assert_eq!(ModuleType::Plugin.as_str(), "plugin");
        assert_eq!(ModuleType::Service.as_str(), "service");
        assert_eq!(ModuleType::Sensor.as_str(), "sensor");
        assert_eq!(ModuleType::Skill.as_str(), "skill");
        assert_eq!(ModuleType::Tool.as_str(), "tool");
        assert_eq!(ModuleType::Driver.as_str(), "driver");
    }

    #[test]
    fn module_type_display() {
        assert_eq!(ModuleType::Core.to_string(), "core");
        assert_eq!(ModuleType::Driver.to_string(), "driver");
    }

    #[test]
    fn module_type_clone_copy_eq_hash() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let a = ModuleType::Sensor;
        let b = a;
        assert_eq!(a, b);
        assert_eq!(ModuleType::Core, ModuleType::Core);
        assert_ne!(ModuleType::Core, ModuleType::Plugin);

        let mut h1 = DefaultHasher::new();
        ModuleType::Core.hash(&mut h1);
        let mut h2 = DefaultHasher::new();
        ModuleType::Core.hash(&mut h2);
        assert_eq!(h1.finish(), h2.finish());
    }

    // ─── ErrorCode ────────────────────────────────────────────────────────

    #[test]
    fn error_code_new() {
        let ec = ErrorCode::new("EVENT_BUS_FULL");
        assert_eq!(ec.as_str(), "EVENT_BUS_FULL");
    }

    #[test]
    fn error_code_display() {
        let ec = ErrorCode::new("TIMEOUT");
        assert_eq!(ec.to_string(), "TIMEOUT");
    }

    #[test]
    fn error_code_from_str() {
        let ec: ErrorCode = "UNKNOWN".parse().unwrap();
        assert_eq!(ec.as_str(), "UNKNOWN");
    }

    #[test]
    fn error_code_empty() {
        let ec = ErrorCode::new("");
        assert_eq!(ec.as_str(), "");
        assert_eq!(ec.to_string(), "");
    }

    #[test]
    fn error_code_clone_eq_hash() {
        let a = ErrorCode::new("ERR_1");
        let b = a.clone();
        assert_eq!(a, b);
        let c = ErrorCode::new("ERR_1");
        assert_eq!(a, c);

        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h1 = DefaultHasher::new();
        a.hash(&mut h1);
        let mut h2 = DefaultHasher::new();
        c.hash(&mut h2);
        assert_eq!(h1.finish(), h2.finish());
    }

    #[test]
    fn error_code_serde_roundtrip() {
        let ec = ErrorCode::new("DB_CONNECTION_LOST");
        let json = serde_json::to_string(&ec).unwrap();
        let deserialized: ErrorCode = serde_json::from_str(&json).unwrap();
        assert_eq!(ec, deserialized);
    }
}
