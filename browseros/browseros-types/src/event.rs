use std::any::Any;
use std::fmt::Debug;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::identifiers::*;
use crate::value::ContentType;

/// Category of an event in the canonical hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventCategory {
    /// System-level events: service start/stop, config change, error.
    System,
    /// Domain events: task submitted, dag completed, plugin loaded.
    Domain,
    /// Metric events: counter increment, gauge set.
    Metric,
    /// Internal events: command pattern for routing.
    Internal,
}

/// Metadata attached to every event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventMetadata {
    /// Unique event identifier.
    pub id: EventId,
    /// ID of the event that caused this one, if any.
    pub causation_id: Option<CausationId>,
    /// Correlation ID grouping related events.
    pub correlation_id: CorrelationId,
    /// Module that emitted the event.
    pub source: ModuleId,
    /// Timestamp of event creation.
    pub timestamp: DateTime<Utc>,
    /// Content type of the event payload.
    pub content_type: ContentType,
}

impl EventMetadata {
    /// Create a new `EventMetadata` with a fresh `EventId` and the given timestamp.
    pub fn new(
        source: ModuleId,
        correlation_id: CorrelationId,
        causation_id: Option<CausationId>,
        content_type: ContentType,
        timestamp: DateTime<Utc>,
    ) -> Self {
        Self {
            id: EventId::new(),
            causation_id,
            correlation_id,
            source,
            timestamp,
            content_type,
        }
    }
}

/// Core trait for all events in the system.
///
/// Every event must provide a unique string `kind`, a `category`, and its
/// associated `metadata`. Implementations are expected to be lightweight
/// data carriers with no runtime behaviour.
pub trait Event: Debug + Send + Sync + 'static {
    /// A unique string identifying the event type (e.g. `"task.submitted"`).
    fn kind(&self) -> &'static str;
    /// The canonical category of this event.
    fn category(&self) -> EventCategory;
    /// Metadata attached to this event.
    fn metadata(&self) -> &EventMetadata;
    /// Downcast to `Any` for type-safe downcasting in event handlers.
    fn as_any(&self) -> &dyn Any;
}

/// Marker trait for system events.
///
/// System events relate to the runtime itself: service start/stop,
/// configuration changes, and errors.
pub trait SystemEvent: Event {}

/// Marker trait for domain events.
///
/// Domain events carry business-logic signals such as task submission,
/// DAG completion, or plugin lifecycle changes.
pub trait DomainEvent: Event {}

/// Marker trait for internal events.
///
/// Internal events are used for command-pattern routing and other
/// runtime-internal signalling.
pub trait InternalEvent: Event {}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::SemVer;
    use chrono::Utc;

    // ─── EventCategory ────────────────────────────────────────────────────

    #[test]
    fn event_category_clone_copy_eq() {
        let a = EventCategory::System;
        let b = a;
        assert_eq!(a, b);
        assert_eq!(EventCategory::System, EventCategory::System);
        assert_eq!(EventCategory::Domain, EventCategory::Domain);
        assert_eq!(EventCategory::Metric, EventCategory::Metric);
        assert_eq!(EventCategory::Internal, EventCategory::Internal);
        assert_ne!(EventCategory::System, EventCategory::Domain);
    }

    // ─── EventMetadata ────────────────────────────────────────────────────

    #[test]
    fn event_metadata_new() {
        let source = ModuleId::new("test-module", SemVer::new(1, 0, 0));
        let corr_id = CorrelationId::new();
        let cause_id = CausationId::new();
        let ct = ContentType::new("application/json");

        let meta = EventMetadata::new(
            source.clone(),
            corr_id,
            Some(cause_id),
            ct.clone(),
            Utc::now(),
        );

        // id is auto-generated, just check it's non-nil
        assert!(!meta.id.as_uuid().is_nil());
        assert_eq!(meta.causation_id, Some(cause_id));
        assert_eq!(meta.correlation_id, corr_id);
        assert_eq!(meta.source, source);
        assert_eq!(meta.content_type, ct);
        // timestamp should be very recent
        let now = Utc::now();
        let diff = now - meta.timestamp;
        assert!(diff.num_seconds() < 5, "timestamp too old");
    }

    #[test]
    fn event_metadata_no_causation() {
        let source = ModuleId::new("no-cause", SemVer::new(0, 0, 1));
        let corr_id = CorrelationId::new();
        let ct = ContentType::new("text/plain");

        let meta = EventMetadata::new(source, corr_id, None, ct, Utc::now());
        assert!(meta.causation_id.is_none());
    }

    #[test]
    fn event_metadata_unique_ids() {
        let source = ModuleId::new("mod", SemVer::new(1, 0, 0));
        let ct = ContentType::new("app/json");
        let corr = CorrelationId::new();

        let m1 = EventMetadata::new(source.clone(), corr, None, ct.clone(), Utc::now());
        let m2 = EventMetadata::new(source, CorrelationId::new(), None, ct, Utc::now());
        assert_ne!(m1.id, m2.id, "each metadata must have a unique id");
    }

    // ─── Concrete event implementations ──────────────────────────────────

    /// A minimal test event for asserting trait bounds.
    #[derive(Debug)]
    struct TestEvent {
        kind: &'static str,
        category: EventCategory,
        metadata: EventMetadata,
    }

    impl Event for TestEvent {
        fn kind(&self) -> &'static str {
            self.kind
        }
        fn category(&self) -> EventCategory {
            self.category
        }
        fn metadata(&self) -> &EventMetadata {
            &self.metadata
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    impl SystemEvent for TestEvent {}
    impl DomainEvent for TestEvent {}
    impl InternalEvent for TestEvent {}

    #[test]
    fn event_trait_kind_category() {
        let meta = EventMetadata::new(
            ModuleId::new("test", SemVer::new(1, 0, 0)),
            CorrelationId::new(),
            None,
            ContentType::new("app/json"),
            Utc::now(),
        );
        let ev = TestEvent {
            kind: "test.event",
            category: EventCategory::Domain,
            metadata: meta,
        };
        assert_eq!(ev.kind(), "test.event");
        assert_eq!(ev.category(), EventCategory::Domain);
    }

    #[test]
    fn system_event_marker() {
        fn assert_system<E: SystemEvent>() {}
        assert_system::<TestEvent>();
    }

    #[test]
    fn domain_event_marker() {
        fn assert_domain<E: DomainEvent>() {}
        assert_domain::<TestEvent>();
    }

    #[test]
    fn internal_event_marker() {
        fn assert_internal<E: InternalEvent>() {}
        assert_internal::<TestEvent>();
    }

    #[test]
    fn event_send_sync_static() {
        fn assert_bounds<E: Event>() {}
        assert_bounds::<TestEvent>();
    }
}
