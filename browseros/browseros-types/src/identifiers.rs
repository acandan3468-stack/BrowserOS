//! Strongly-typed identifiers for the BrowserOS domain.
//!
//! Each identifier is a newtype wrapper around [`Uuid`], [`String`], or [`u64`],
//! providing type safety and domain semantics while keeping the representation
//! lightweight and zero-cost at runtime.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::value::SemVer;

/// Error returned when a string cannot be parsed into an identifier.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum IdParseError {
    /// The input was not a valid UUID.
    #[error("invalid UUID: {0}")]
    InvalidUuid(#[from] uuid::Error),
    /// The input was not a valid integer.
    #[error("invalid integer: {0}")]
    InvalidInteger(#[from] std::num::ParseIntError),
}

macro_rules! uuid_id {
    ($(#[$attr:meta])* $name:ident) => {
        $(#[$attr])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            /// Creates a new unique identifier using UUID v7.
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }

            /// Wraps an existing [`Uuid`].
            pub fn from_uuid(uuid: Uuid) -> Self {
                Self(uuid)
            }

            /// Returns a reference to the inner [`Uuid`].
            pub fn as_uuid(&self) -> &Uuid {
                &self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }

        impl FromStr for $name {
            type Err = IdParseError;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Uuid::from_str(s).map(Self).map_err(IdParseError::from)
            }
        }
    };
}

macro_rules! string_id {
    ($(#[$attr:meta])* $name:ident) => {
        $(#[$attr])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Creates a new identifier from any string-like value.
            pub fn from_string(s: impl Into<String>) -> Self {
                Self(s.into())
            }

            /// Returns a reference to the inner string.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }

        impl FromStr for $name {
            type Err = std::convert::Infallible;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ok(Self(s.to_owned()))
            }
        }
    };
}

macro_rules! u64_id {
    ($(#[$attr:meta])* $name:ident) => {
        $(#[$attr])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(u64);

        impl $name {
            /// Creates a new identifier with the given value.
            pub fn new(value: u64) -> Self {
                Self(value)
            }

            /// Returns the inner `u64` value.
            pub fn get(&self) -> u64 {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }

        impl FromStr for $name {
            type Err = IdParseError;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                s.parse::<u64>().map(Self).map_err(IdParseError::from)
            }
        }
    };
}

// ---------------------------------------------------------------------------
// UUID-based identifiers
// ---------------------------------------------------------------------------

uuid_id! {
    /// Unique event identifier.
    EventId
}

uuid_id! {
    /// Stable identity for a DOM element handle (UUID v7).
    HandleId
}

uuid_id! {
    /// Unique message identifier.
    MessageId
}

uuid_id! {
    /// Traces an operation or saga across components.
    CorrelationId
}

uuid_id! {
    /// Records what caused this event.
    ///
    /// This is a [`Uuid`] wrapper; the absence of a causation link is
    /// represented separately as `Option::None` at the usage site.
    CausationId
}

uuid_id! {
    /// Identifier for a scheduled task.
    TaskId
}

uuid_id! {
    /// Identifier for a DAG execution.
    ExecutionId
}

uuid_id! {
    /// Handle for an active subscription.
    SubscriptionHandle
}

// ---------------------------------------------------------------------------
// String-based identifiers
// ---------------------------------------------------------------------------

string_id! {
    /// Human-readable DAG node identifier.
    NodeId
}

string_id! {
    /// Plugin identifier.
    PluginId
}

string_id! {
    /// Capability identifier (e.g. `"dom.sensor"`).
    CapabilityId
}

string_id! {
    /// Service identifier.
    ServiceId
}

string_id! {
    /// State entity identifier.
    EntityId
}

// ---------------------------------------------------------------------------
// u64-based identifiers
// ---------------------------------------------------------------------------

u64_id! {
    /// State version number.
    VersionId
}

u64_id! {
    /// Position in an event stream.
    StreamPosition
}

// ---------------------------------------------------------------------------
// Composite identifiers (not generated by macros)
// ---------------------------------------------------------------------------

/// Unique module identifier within the runtime.
///
/// Combines a human-readable module name with a semantic version to
/// disambiguate multiple versions of the same module.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModuleId {
    name: String,
    version: SemVer,
}

impl ModuleId {
    /// Create a new `ModuleId`.
    pub fn new(name: impl Into<String>, version: SemVer) -> Self {
        Self {
            name: name.into(),
            version,
        }
    }

    /// The human-readable module name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The semantic version of this module.
    pub fn version(&self) -> &SemVer {
        &self.version
    }
}

impl CausationId {
    /// Creates a `CausationId` from an existing [`EventId`].
    ///
    /// This makes the semantic relationship explicit: the event with this
    /// causation ID was caused by the event with the given `EventId`.
    pub fn from_event_id(id: EventId) -> Self {
        Self::from_uuid(*id.as_uuid())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    /// Helper: assert that a UUID-based ID round-trips through Display/FromStr.
    fn uuid_id_roundtrip<T>(id: T)
    where
        T: FromStr + fmt::Display + PartialEq + std::fmt::Debug,
        T::Err: std::fmt::Debug,
    {
        let s = id.to_string();
        let parsed: T = s.parse().unwrap();
        assert_eq!(id, parsed, "Display/FromStr roundtrip failed for {:?}", id);
    }

    /// Helper: assert UUID new() produces a non-nil ID and Default matches new().
    fn uuid_id_new_and_default<T>()
    where
        T: NewUuid + Default + PartialEq + std::fmt::Debug,
    {
        let id = T::new();
        assert!(!id.is_nil(), "new() produced nil UUID");
        let def = T::default();
        assert!(!def.is_nil(), "Default produced nil UUID");
    }

    /// Trait used only in tests to expose is_nil through a uniform interface.
    trait NewUuid: Sized {
        fn new() -> Self;
        fn is_nil(&self) -> bool;
    }
    macro_rules! impl_new_uuid {
        ($t:ty) => {
            impl NewUuid for $t {
                fn new() -> Self {
                    <$t>::new()
                }
                fn is_nil(&self) -> bool {
                    // With uuid_id!, inner is at index 0 (zero-cost newtype)
                    let inner: &Uuid = self.as_uuid();
                    inner.is_nil()
                }
            }
        };
    }
    impl_new_uuid!(EventId);
    impl_new_uuid!(MessageId);
    impl_new_uuid!(CorrelationId);
    impl_new_uuid!(CausationId);
    impl_new_uuid!(TaskId);
    impl_new_uuid!(ExecutionId);
    impl_new_uuid!(SubscriptionHandle);

    // ── UUID-based identifier tests ──────────────────────────────────────

    #[test]
    fn event_id_new_and_default() {
        uuid_id_new_and_default::<EventId>();
    }
    #[test]
    fn event_id_from_uuid_roundtrip() {
        let u = Uuid::now_v7();
        let id = EventId::from_uuid(u);
        assert_eq!(id.as_uuid(), &u);
    }
    #[test]
    fn event_id_display_fromstr_roundtrip() {
        uuid_id_roundtrip(EventId::new());
    }
    #[test]
    fn event_id_clone_copy_eq_hash() {
        let a = EventId::new();
        let b = a;
        assert_eq!(a, b);
        let c = EventId::from_uuid(*a.as_uuid());
        assert_eq!(a, c);
        let mut ha = DefaultHasher::new();
        a.hash(&mut ha);
        let mut hc = DefaultHasher::new();
        c.hash(&mut hc);
        assert_eq!(ha.finish(), hc.finish());
    }
    #[test]
    fn event_id_invalid_uuid() {
        let err = "not-a-uuid".parse::<EventId>().unwrap_err();
        match err {
            IdParseError::InvalidUuid(_) => {}
            _ => panic!("expected InvalidUuid, got {:?}", err),
        }
    }

    #[test]
    fn message_id_new_and_default() {
        uuid_id_new_and_default::<MessageId>();
    }
    #[test]
    fn message_id_from_uuid_roundtrip() {
        let u = Uuid::now_v7();
        let id = MessageId::from_uuid(u);
        assert_eq!(id.as_uuid(), &u);
    }
    #[test]
    fn message_id_display_fromstr_roundtrip() {
        uuid_id_roundtrip(MessageId::new());
    }
    #[test]
    fn message_id_clone_copy_eq_hash() {
        let a = MessageId::new();
        let b = a;
        assert_eq!(a, b);
    }
    #[test]
    fn message_id_invalid_uuid() {
        assert!("".parse::<MessageId>().is_err());
        assert!("garbage".parse::<MessageId>().is_err());
    }

    #[test]
    fn correlation_id_new_and_default() {
        uuid_id_new_and_default::<CorrelationId>();
    }
    #[test]
    fn correlation_id_display_fromstr_roundtrip() {
        uuid_id_roundtrip(CorrelationId::new());
    }
    #[test]
    fn correlation_id_from_uuid_roundtrip() {
        let u = Uuid::now_v7();
        let id = CorrelationId::from_uuid(u);
        assert_eq!(id.as_uuid(), &u);
    }

    #[test]
    fn causation_id_new_and_default() {
        uuid_id_new_and_default::<CausationId>();
    }
    #[test]
    fn causation_id_display_fromstr_roundtrip() {
        uuid_id_roundtrip(CausationId::new());
    }
    #[test]
    fn causation_id_from_uuid_roundtrip() {
        let u = Uuid::now_v7();
        let id = CausationId::from_uuid(u);
        assert_eq!(id.as_uuid(), &u);
    }

    #[test]
    fn task_id_new_and_default() {
        uuid_id_new_and_default::<TaskId>();
    }
    #[test]
    fn task_id_display_fromstr_roundtrip() {
        uuid_id_roundtrip(TaskId::new());
    }

    #[test]
    fn execution_id_new_and_default() {
        uuid_id_new_and_default::<ExecutionId>();
    }
    #[test]
    fn execution_id_display_fromstr_roundtrip() {
        uuid_id_roundtrip(ExecutionId::new());
    }

    #[test]
    fn subscription_handle_new_and_default() {
        uuid_id_new_and_default::<SubscriptionHandle>();
    }
    #[test]
    fn subscription_handle_display_fromstr_roundtrip() {
        uuid_id_roundtrip(SubscriptionHandle::new());
    }

    // ── String-based identifier tests ────────────────────────────────────

    macro_rules! test_string_id {
        ($test_name:ident, $ty:ident, $val:expr) => {
            #[test]
            fn $test_name() {
                let id = $ty::from_string($val);
                assert_eq!(id.as_str(), $val);
                assert_eq!(id.to_string(), $val);
                // FromStr roundtrip
                let parsed: $ty = $val.parse().unwrap();
                assert_eq!(id, parsed);
                // Clone
                let cloned = id.clone();
                assert_eq!(id, cloned);
                // Eq
                let id2 = $ty::from_string($val);
                assert_eq!(id, id2);
                // Hash
                let mut h1 = DefaultHasher::new();
                id.hash(&mut h1);
                let mut h2 = DefaultHasher::new();
                id2.hash(&mut h2);
                assert_eq!(h1.finish(), h2.finish());
            }
        };
    }

    test_string_id!(node_id_basic, NodeId, "node-42");
    test_string_id!(plugin_id_basic, PluginId, "my-plugin");
    test_string_id!(capability_id_basic, CapabilityId, "dom.sensor");
    test_string_id!(service_id_basic, ServiceId, "auth-service");
    test_string_id!(entity_id_basic, EntityId, "entity-123");

    #[test]
    fn string_id_empty() {
        let id = NodeId::from_string("");
        assert_eq!(id.as_str(), "");
        assert_eq!(id.to_string(), "");
        let parsed: NodeId = "".parse().unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn string_id_special_chars() {
        let id = PluginId::from_string("plugin.with.dots_and-dashes/123");
        assert_eq!(id.as_str(), "plugin.with.dots_and-dashes/123");
        let parsed: PluginId = "plugin.with.dots_and-dashes/123".parse().unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn string_id_display_eq_as_str() {
        let id = CapabilityId::from_string("test.capability");
        assert_eq!(id.to_string(), id.as_str());
    }

    // ── u64-based identifier tests ───────────────────────────────────────

    macro_rules! test_u64_id {
        ($test_name:ident, $val:expr, $ty:ident) => {
            #[test]
            fn $test_name() {
                let id = $ty::new($val);
                assert_eq!(id.get(), $val);
                assert_eq!(id.to_string(), $val.to_string());
                // FromStr roundtrip
                let parsed: $ty = $val.to_string().parse().unwrap();
                assert_eq!(id, parsed);
                // Clone + Copy
                let copied = id;
                assert_eq!(id, copied);
                // Eq
                let id2 = $ty::new($val);
                assert_eq!(id, id2);
                // Hash
                let mut h1 = DefaultHasher::new();
                id.hash(&mut h1);
                let mut h2 = DefaultHasher::new();
                id2.hash(&mut h2);
                assert_eq!(h1.finish(), h2.finish());
            }
        };
    }

    test_u64_id!(version_id_basic, 42, VersionId);
    test_u64_id!(stream_position_basic, 0, StreamPosition);
    test_u64_id!(version_id_large, u64::MAX, VersionId);
    test_u64_id!(stream_position_large, u64::MAX, StreamPosition);

    #[test]
    fn u64_id_invalid() {
        match "not-a-number".parse::<VersionId>().unwrap_err() {
            IdParseError::InvalidInteger(_) => {}
            other => panic!("expected InvalidInteger, got {:?}", other),
        }
        match "".parse::<StreamPosition>().unwrap_err() {
            IdParseError::InvalidInteger(_) => {}
            other => panic!("expected InvalidInteger, got {:?}", other),
        }
    }

    #[test]
    fn u64_id_negative_string() {
        match "-1".parse::<VersionId>().unwrap_err() {
            IdParseError::InvalidInteger(_) => {}
            other => panic!("expected InvalidInteger, got {:?}", other),
        }
    }

    // ── ModuleId ─────────────────────────────────────────────────────────────

    #[test]
    fn module_id_new() {
        let mid = ModuleId::new("event-bus", SemVer::new(1, 2, 3));
        assert_eq!(mid.name(), "event-bus");
        assert_eq!(mid.version(), &SemVer::new(1, 2, 3));
    }

    #[test]
    fn module_id_name_accessor() {
        let mid = ModuleId::new("scheduler", SemVer::new(0, 1, 0));
        assert_eq!(mid.name(), "scheduler");
    }

    #[test]
    fn module_id_version_accessor() {
        let mid = ModuleId::new("storage", SemVer::new(2, 0, 0));
        assert_eq!(mid.version(), &SemVer::new(2, 0, 0));
    }

    #[test]
    fn module_id_clone_eq_hash() {
        let a = ModuleId::new("bus", SemVer::new(1, 0, 0));
        let b = a.clone();
        assert_eq!(a, b);
        let c = ModuleId::new("bus", SemVer::new(1, 0, 0));
        assert_eq!(a, c);
        let d = ModuleId::new("bus", SemVer::new(1, 0, 1));
        assert_ne!(a, d);

        let mut h1 = DefaultHasher::new();
        a.hash(&mut h1);
        let mut h2 = DefaultHasher::new();
        c.hash(&mut h2);
        assert_eq!(h1.finish(), h2.finish());
    }

    #[test]
    fn module_id_serde_roundtrip() {
        let a = ModuleId::new("my-mod", SemVer::new(3, 2, 1));
        let json = serde_json::to_string(&a).unwrap();
        let b: ModuleId = serde_json::from_str(&json).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn causation_id_from_event_id() {
        let event_id = EventId::new();
        let cause = CausationId::from_event_id(event_id);
        assert_eq!(cause.to_string(), event_id.to_string());
    }
}
