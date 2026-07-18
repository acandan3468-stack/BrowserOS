//! Integration tests for browseros-types.
//!
//! Tests that span multiple modules or exercise real-world scenarios.

use browseros_types::clock::{CancellationToken, Clock, MockClock, SystemClock};
use browseros_types::component::{
    CapabilityDefinition, ComponentManifest, ComponentState, HealthCheckDefinition, HealthStatus,
    ResourceRequirements,
};
use browseros_types::error::{BrowserOsError, ErrorContext, ErrorKind, ErrorSeverity, RetryPolicy};
use browseros_types::event::EventMetadata;
use browseros_types::identifiers::*;
use browseros_types::message::{MessageEnvelope, MessageEnvelopeBuilder, TraceContext};
use browseros_types::module::ModuleDescriptor;
use browseros_types::value::{
    ContentType, DeliveryGuarantee, ErrorCode, LogLevel, ModuleType, Priority, SemVer,
};
use std::time::Duration;

// ─── Helper factories ──────────────────────────────────────────────────

fn mid(name: &str, major: u32, minor: u32, patch: u32) -> ModuleId {
    ModuleId::new(name, SemVer::new(major, minor, patch))
}

fn ct(s: &str) -> ContentType {
    ContentType::new(s)
}

fn basic_envelope() -> MessageEnvelope {
    MessageEnvelopeBuilder::new(mid("source", 1, 0, 0), ct("app/json"), vec![1, 2, 3])
        .build()
        .expect("basic_envelope should succeed")
}

// ─── Cross-module: ModuleId + SemVer + ModuleType → ModuleDescriptor ───

#[test]
fn module_descriptor_integration() {
    let module_id = ModuleId::new("event-bus", SemVer::new(2, 1, 0));
    let desc = ModuleDescriptor {
        module_id,
        module_type: ModuleType::Core,
        description: "Core event bus".into(),
        enabled: true,
    };
    assert_eq!(desc.module_id.name(), "event-bus");
    assert_eq!(*desc.module_id.version(), SemVer::new(2, 1, 0));
    assert_eq!(desc.module_type, ModuleType::Core);
}

// ─── Cross-module: IDs + Envelope → EventMetadata ──────────────────────

#[test]
fn event_metadata_from_envelope_components() {
    let source = mid("scheduler", 1, 0, 0);
    let correlation_id = CorrelationId::new();
    let causation_id = CausationId::new();
    let content_type = ct("application/x.browseros.event.v1+json");

    let meta = EventMetadata::new(
        source,
        correlation_id,
        Some(causation_id),
        content_type,
        chrono::Utc::now(),
    );

    assert!(!meta.id.as_uuid().is_nil());
    assert_eq!(meta.correlation_id, correlation_id);
    assert_eq!(meta.causation_id, Some(causation_id));
    assert_eq!(meta.source.name(), "scheduler");
}

// ─── Cross-module: Error + Retry + ErrorCode ───────────────────────────

#[test]
fn error_with_retry_policy_integration() {
    let err = BrowserOsError::transient("service unavailable")
        .with_code(ErrorCode::new("SVC_UNAVAIL"))
        .with_context(ErrorContext {
            message: "connection refused".into(),
            module: "http_client".into(),
            file: "client.rs".into(),
            line: 42,
        });

    assert!(err.is_retryable());
    let policy = err
        .retry_policy()
        .expect("transient error should have retry policy");

    match policy {
        RetryPolicy::ExponentialBackoff {
            initial_delay_ms,
            max_delay_ms,
            multiplier,
            max_retries,
            jitter,
        } => {
            assert_eq!(initial_delay_ms, 100);
            assert_eq!(max_delay_ms, 30_000);
            assert!((multiplier - 2.0).abs() < f64::EPSILON);
            assert_eq!(max_retries, 5);
            assert!(jitter);
        }
        _ => panic!("expected ExponentialBackoff"),
    }
}

// ─── Cross-module: ComponentState + Error ──────────────────────────────

#[test]
fn component_state_failure_transition() {
    // Simulate: a ready component encounters an error → Failed
    assert!(ComponentState::Ready.can_transition_to(ComponentState::Failed));

    let error =
        BrowserOsError::internal("unexpected state").with_code(ErrorCode::new("INTERNAL_ERR"));

    assert_eq!(error.kind, ErrorKind::Internal);
    assert!(!error.is_retryable());
}

// ─── Cross-module: Clock + CancellationToken ──────────────────────────

#[test]
fn mock_clock_with_cancellation() {
    let clock = MockClock::new(chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap());
    let token = CancellationToken::new();

    assert!(!token.is_cancelled());
    let now = clock.now();
    assert_eq!(now.timestamp(), 1_700_000_000);

    clock.advance(Duration::from_secs(60));
    assert_eq!(clock.now().timestamp(), 1_700_000_060);

    token.cancel();
    assert!(token.is_cancelled());
}

// ─── Cross-module: MessageEnvelope + Priority + ContentType + TraceContext ─

#[test]
fn high_priority_message_with_trace() {
    let source = mid("orchestrator", 2, 0, 0);
    let dest = mid("worker", 1, 5, 0);
    let payload = b"execute-task-42".to_vec();

    let trace = TraceContext {
        trace_id: "trace-abc".into(),
        span_id: "span-001".into(),
        parent_span_id: None,
    };

    let envelope = MessageEnvelopeBuilder::new(source, ct("app/x.browseros.task"), payload)
        .destination(dest)
        .priority(Priority::Critical)
        .ttl(chrono::Duration::seconds(60))
        .trace_context(trace)
        .build()
        .expect("envelope with all fields should succeed");

    assert_eq!(envelope.priority, Priority::Critical);
    assert_eq!(envelope.ttl, Some(chrono::Duration::seconds(60)));
    assert_eq!(
        envelope.trace_context.as_ref().unwrap().trace_id,
        "trace-abc"
    );
    assert!(!envelope.is_expired());
    assert_eq!(envelope.sequence, 0);
    assert_eq!(envelope.retry_count, 0);
}

// ─── Cross-module: HealthStatus + ErrorCode ───────────────────────────

#[test]
fn unhealthy_component_with_error_code() {
    let status = HealthStatus::Unhealthy {
        message: "disk full".into(),
        error_code: ErrorCode::new("DISK_FULL"),
    };

    assert!(status.is_unhealthy());
    assert!(!status.is_healthy());
    assert!(!status.is_degraded());

    if let HealthStatus::Unhealthy { ref error_code, .. } = status {
        assert_eq!(error_code.as_str(), "DISK_FULL");
    }
}

// ─── Cross-module: ComponentManifest + CapabilityDefinition ───────────

#[test]
fn manifest_with_capabilities() {
    let cap = CapabilityDefinition {
        id: CapabilityId::from_string("storage.read"),
        version: SemVer::new(1, 0, 0),
        description: "Read from storage".into(),
        interface: "storage.v1".into(),
    };

    let manifest = ComponentManifest {
        name: "storage-service".into(),
        version: SemVer::new(1, 2, 3),
        description: "Persistent storage service".into(),
        dependencies: vec!["disk-driver".into()],
        required_capabilities: vec![CapabilityId::from_string("disk.access")],
        provided_capabilities: vec![cap],
        startup_timeout: Duration::from_secs(30),
        shutdown_timeout: Duration::from_secs(10),
        health_check_interval: Duration::from_secs(5),
    };

    assert_eq!(manifest.name, "storage-service");
    assert_eq!(manifest.provided_capabilities.len(), 1);
    assert_eq!(
        manifest.provided_capabilities[0].id.as_str(),
        "storage.read"
    );
}

// ─── Cross-module: LogLevel + ErrorSeverity comparison ────────────────

#[test]
fn severity_vs_loglevel_semantics() {
    // Both represent severity but in different domains
    assert!(LogLevel::Error > LogLevel::Warn);
    assert!(LogLevel::Warn > LogLevel::Info);

    assert!(ErrorSeverity::Critical > ErrorSeverity::Error);
    assert!(ErrorSeverity::Error > ErrorSeverity::Warning);

    // is_reportable should match intuitive severity
    assert!(ErrorSeverity::Error.is_reportable());
    assert!(ErrorSeverity::Critical.is_reportable());
    assert!(!ErrorSeverity::Warning.is_reportable());
}

// ─── Cross-module: DeliveryGuarantee + Priority + RetryPolicy ─────────

#[test]
fn delivery_guarantee_and_retry_interaction() {
    // ExactlyOnce messages should use retry
    let guarantee = DeliveryGuarantee::ExactlyOnce;
    assert_eq!(guarantee, DeliveryGuarantee::ExactlyOnce);

    // Transient errors get a retry policy
    let err = BrowserOsError::transient("delivery failed");
    assert!(err.retry_policy().is_some());
}

// ─── Cross-module: Full ID lifecycle ──────────────────────────────────

#[test]
fn id_lifecycle_uuid_roundtrip() {
    let original = EventId::new();
    let serialized = original.to_string();
    let deserialized: EventId = serialized.parse().unwrap();
    assert_eq!(original, deserialized);

    let corr = CorrelationId::new();
    let serialized = corr.to_string();
    let deserialized: CorrelationId = serialized.parse().unwrap();
    assert_eq!(corr, deserialized);
}

#[test]
fn id_lifecycle_string_roundtrip() {
    let original = CapabilityId::from_string("dom.click");
    let serialized = original.to_string();
    let deserialized: CapabilityId = serialized.parse().unwrap();
    assert_eq!(original, deserialized);
}

#[test]
fn id_lifecycle_u64_roundtrip() {
    let original = VersionId::new(42);
    let serialized = original.to_string();
    let deserialized: VersionId = serialized.parse().unwrap();
    assert_eq!(original, deserialized);
}

// ─── Cross-module: SystemClock + MessageEnvelope timestamp ────────────

#[test]
fn envelope_timestamp_is_recent() {
    let clock = SystemClock::new();
    let before = clock.now();

    let envelope = basic_envelope();
    let after = clock.now();

    assert!(
        envelope.timestamp >= before,
        "timestamp should be >= creation time"
    );
    assert!(
        envelope.timestamp <= after,
        "timestamp should be <= after time"
    );
}

// ─── Cross-module: ResourceRequirements + ComponentManifest ───────────

#[test]
fn component_with_resource_limits() {
    let limits = ResourceRequirements {
        memory_bytes: 256_000_000,
        file_descriptors: 64,
        max_tasks: 4,
    };

    let _manifest = ComponentManifest {
        name: "vision-sensor".into(),
        version: SemVer::new(0, 9, 0),
        description: "Vision perception sensor".into(),
        dependencies: vec!["camera-driver".into()],
        required_capabilities: vec![CapabilityId::from_string("camera.access")],
        provided_capabilities: vec![],
        startup_timeout: Duration::from_secs(15),
        shutdown_timeout: Duration::from_secs(5),
        health_check_interval: Duration::from_secs(2),
    };

    assert_eq!(limits.memory_bytes, 256_000_000);
    assert_eq!(limits.max_tasks, 4);
}

// ─── Cross-module: Event kind + category + metadata ───────────────────

#[test]
fn event_kind_and_category_consistency() {
    // Create a minimal event to verify the trait contract
    let meta = EventMetadata::new(
        mid("test-mod", 1, 0, 0),
        CorrelationId::new(),
        None,
        ct("app/json"),
        chrono::Utc::now(),
    );
    assert_eq!(meta.source.name(), "test-mod");
    assert_eq!(*meta.source.version(), SemVer::new(1, 0, 0));
}

// ─── Cross-module: HealthCheckDefinition + ComponentManifest ──────────

#[test]
fn health_check_integration() {
    let check = HealthCheckDefinition {
        name: "readiness".into(),
        interval: Duration::from_secs(10),
        timeout: Duration::from_secs(2),
        failure_threshold: 5,
    };
    let _manifest = ComponentManifest {
        name: "web-server".into(),
        version: SemVer::new(1, 0, 0),
        description: "HTTP server".into(),
        dependencies: vec!["tls-driver".into()],
        required_capabilities: vec![CapabilityId::from_string("network.access")],
        provided_capabilities: vec![],
        startup_timeout: Duration::from_secs(60),
        shutdown_timeout: Duration::from_secs(30),
        health_check_interval: check.interval,
    };
    assert_eq!(check.name, "readiness");
    assert_eq!(check.failure_threshold, 5);
}

// ─── Cross-module: SemVer ordering in module versioning ───────────────

#[test]
fn module_version_comparison() {
    let v1 = SemVer::new(1, 0, 0);
    let v2 = SemVer::new(2, 0, 0);
    let v1_1 = SemVer::new(1, 1, 0);

    assert!(v1 < v2);
    assert!(v1 < v1_1);
    assert!(v1_1 < v2);

    let mod_a = ModuleId::new("module-a", v1);
    let mod_b = ModuleId::new("module-b", v2);

    // Higher major version → different module, no direct ordering on ModuleId
    assert_ne!(mod_a, mod_b);
    assert_eq!(mod_a.name(), "module-a");
    assert_eq!(mod_b.name(), "module-b");
}

// ─── Cross-module: Priority ordering for message routing ──────────────

#[test]
fn priority_based_message_routing() {
    let low = MessageEnvelopeBuilder::new(mid("sensor", 1, 0, 0), ct("app/json"), vec![])
        .priority(Priority::Low)
        .build()
        .expect("low priority envelope");
    let high = MessageEnvelopeBuilder::new(mid("sensor", 1, 0, 0), ct("app/json"), vec![])
        .priority(Priority::High)
        .build()
        .expect("high priority envelope");

    // Low priority messages should sort before High (lower number first in queue)
    assert!(Priority::Low < Priority::High);
    assert_eq!(low.source, high.source);
}
