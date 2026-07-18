//! Component lifecycle, health, and deployment types for BrowserOS.
//!
//! Defines the state machine for component lifecycle management, health check
//! reporting, and the manifest schema used to describe and deploy components.

use crate::identifiers::CapabilityId;
use crate::value::{ErrorCode, SemVer};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Valid states in the component lifecycle state machine.
///
/// Transitions are validated at runtime by [`ComponentState::can_transition_to`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComponentState {
    /// Component has been created but not yet initialised.
    Created,
    /// Component is performing its startup initialisation.
    Initializing,
    /// Component is fully operational.
    Ready,
    /// Component is operational but in a degraded / reduced-capacity state.
    Degraded,
    /// Component is in the process of shutting down.
    Stopping,
    /// Component has shut down cleanly.
    Stopped,
    /// Component has encountered a non-recoverable error.
    Failed,
}

impl ComponentState {
    /// Returns `true` if transitioning from `self` to `next` is a valid
    /// lifecycle transition according to the BrowserOS component state machine.
    ///
    /// Every state may transition to itself (idempotent/no-op).
    pub fn can_transition_to(&self, next: ComponentState) -> bool {
        if *self == next {
            return true;
        }
        matches!(
            (*self, next),
            (Self::Created, Self::Initializing)
                | (Self::Initializing, Self::Ready)
                | (Self::Initializing, Self::Failed)
                | (Self::Ready, Self::Degraded)
                | (Self::Ready, Self::Stopping)
                | (Self::Ready, Self::Failed)
                | (Self::Degraded, Self::Ready)
                | (Self::Degraded, Self::Stopping)
                | (Self::Degraded, Self::Failed)
                | (Self::Stopping, Self::Stopped)
                | (Self::Stopping, Self::Failed)
        )
    }
}

/// Reports the current health of a component together with optional diagnostics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HealthStatus {
    /// Component is operating normally.
    Healthy {
        /// Measured latency of the most recent health check.
        latency: Duration,
    },
    /// Component is functional but operating below nominal capacity.
    Degraded {
        /// Human-readable explanation of the degraded condition.
        message: String,
        /// Measured latency of the most recent health check.
        latency: Duration,
    },
    /// Component has failed and is not serving requests.
    Unhealthy {
        /// Human-readable explanation of the failure.
        message: String,
        /// Machine-readable error code for programmatic handling.
        error_code: ErrorCode,
    },
}

impl HealthStatus {
    /// Returns `true` when the component is fully healthy.
    pub fn is_healthy(&self) -> bool {
        matches!(self, Self::Healthy { .. })
    }

    /// Returns `true` when the component is degraded but still operational.
    pub fn is_degraded(&self) -> bool {
        matches!(self, Self::Degraded { .. })
    }

    /// Returns `true` when the component has failed.
    pub fn is_unhealthy(&self) -> bool {
        matches!(self, Self::Unhealthy { .. })
    }
}

/// Describes a deployable component and its runtime contract.
///
/// The manifest is used by the lifecycle manager to validate, schedule, and
/// supervise components within the BrowserOS runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentManifest {
    /// Human-readable name of the component.
    pub name: String,
    /// Semantic version of the component.
    pub version: SemVer,
    /// Short description of the component's purpose.
    pub description: String,
    /// Names of other components this component depends on.
    pub dependencies: Vec<String>,
    /// Capabilities the runtime must grant to this component.
    pub required_capabilities: Vec<CapabilityId>,
    /// Capabilities this component exposes to the runtime.
    pub provided_capabilities: Vec<CapabilityDefinition>,
    /// Maximum time to wait for the component to start.
    pub startup_timeout: Duration,
    /// Maximum time to wait for the component to shut down gracefully.
    pub shutdown_timeout: Duration,
    /// Interval at which the runtime should poll the component's health.
    pub health_check_interval: Duration,
}

/// Defines a capability that a component provides.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityDefinition {
    /// Unique identifier for this capability.
    pub id: CapabilityId,
    /// Semantic version of the capability interface.
    pub version: SemVer,
    /// Human-readable description of what the capability offers.
    pub description: String,
    /// Schema or interface identifier that consumers use to bind.
    pub interface: String,
}

/// Configuration for a single health check on a component.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckDefinition {
    /// Name identifying this check (e.g. "liveness", "readiness").
    pub name: String,
    /// How often the check should be performed.
    pub interval: Duration,
    /// Maximum time a single check may take before it is considered failed.
    pub timeout: Duration,
    /// Number of consecutive failures required to mark the component as down.
    pub failure_threshold: u32,
}

/// Resource limits and quotas for a component.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceRequirements {
    /// Maximum memory allocation in bytes.
    pub memory_bytes: u64,
    /// Maximum number of open file descriptors.
    pub file_descriptors: u32,
    /// Maximum number of concurrent tasks / threads.
    pub max_tasks: u32,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    // ─── ComponentState: valid transitions ───────────────────────────────

    #[test]
    fn component_state_self_transition() {
        for state in &[
            ComponentState::Created,
            ComponentState::Initializing,
            ComponentState::Ready,
            ComponentState::Degraded,
            ComponentState::Stopping,
            ComponentState::Stopped,
            ComponentState::Failed,
        ] {
            assert!(
                state.can_transition_to(*state),
                "self-transition should be valid for {:?}",
                state
            );
        }
    }

    /// All valid transitions (excluding self-transitions).
    const VALID_TRANSITIONS: &[(ComponentState, ComponentState)] = &[
        (ComponentState::Created, ComponentState::Initializing),
        (ComponentState::Initializing, ComponentState::Ready),
        (ComponentState::Initializing, ComponentState::Failed),
        (ComponentState::Ready, ComponentState::Degraded),
        (ComponentState::Ready, ComponentState::Stopping),
        (ComponentState::Ready, ComponentState::Failed),
        (ComponentState::Degraded, ComponentState::Ready),
        (ComponentState::Degraded, ComponentState::Stopping),
        (ComponentState::Degraded, ComponentState::Failed),
        (ComponentState::Stopping, ComponentState::Stopped),
        (ComponentState::Stopping, ComponentState::Failed),
    ];

    #[test]
    fn component_state_all_valid_transitions() {
        for &(from, to) in VALID_TRANSITIONS {
            assert!(
                from.can_transition_to(to),
                "valid transition {:?} -> {:?} was rejected",
                from,
                to
            );
        }
    }

    #[test]
    fn component_state_all_invalid_transitions() {
        /// All possible state pairs.
        const ALL_STATES: &[ComponentState] = &[
            ComponentState::Created,
            ComponentState::Initializing,
            ComponentState::Ready,
            ComponentState::Degraded,
            ComponentState::Stopping,
            ComponentState::Stopped,
            ComponentState::Failed,
        ];

        for from in ALL_STATES {
            for to in ALL_STATES {
                if *from == *to {
                    continue; // self-transitions are always valid
                }
                let is_valid = VALID_TRANSITIONS.contains(&(*from, *to));
                assert_eq!(
                    from.can_transition_to(*to),
                    is_valid,
                    "transition {:?} -> {:?} expected={}, got={}",
                    from,
                    to,
                    is_valid,
                    from.can_transition_to(*to)
                );
            }
        }
    }

    /// Specific invalid transition tests
    #[test]
    fn component_state_created_cannot_skip_initializing() {
        assert!(!ComponentState::Created.can_transition_to(ComponentState::Ready));
        assert!(!ComponentState::Created.can_transition_to(ComponentState::Failed));
        assert!(!ComponentState::Created.can_transition_to(ComponentState::Stopped));
    }

    #[test]
    fn component_state_initializing_cannot_skip_ready() {
        assert!(!ComponentState::Initializing.can_transition_to(ComponentState::Degraded));
        assert!(!ComponentState::Initializing.can_transition_to(ComponentState::Stopping));
        assert!(!ComponentState::Initializing.can_transition_to(ComponentState::Stopped));
    }

    #[test]
    fn component_state_ready_cannot_go_back() {
        assert!(!ComponentState::Ready.can_transition_to(ComponentState::Created));
        assert!(!ComponentState::Ready.can_transition_to(ComponentState::Initializing));
    }

    #[test]
    fn component_state_stopped_is_terminal() {
        assert!(!ComponentState::Stopped.can_transition_to(ComponentState::Created));
        assert!(!ComponentState::Stopped.can_transition_to(ComponentState::Ready));
        assert!(!ComponentState::Stopped.can_transition_to(ComponentState::Degraded));
        assert!(!ComponentState::Stopped.can_transition_to(ComponentState::Failed));
    }

    #[test]
    fn component_state_failed_is_terminal() {
        assert!(!ComponentState::Failed.can_transition_to(ComponentState::Created));
        assert!(!ComponentState::Failed.can_transition_to(ComponentState::Ready));
        assert!(!ComponentState::Failed.can_transition_to(ComponentState::Degraded));
        assert!(!ComponentState::Failed.can_transition_to(ComponentState::Stopping));
        assert!(!ComponentState::Failed.can_transition_to(ComponentState::Stopped));
    }

    #[test]
    fn component_state_degraded_cannot_skip() {
        assert!(!ComponentState::Degraded.can_transition_to(ComponentState::Created));
        assert!(!ComponentState::Degraded.can_transition_to(ComponentState::Initializing));
        assert!(!ComponentState::Degraded.can_transition_to(ComponentState::Stopped));
    }

    #[test]
    fn component_state_clone_copy_eq() {
        let a = ComponentState::Ready;
        let b = a;
        assert_eq!(a, b);
        assert_ne!(ComponentState::Ready, ComponentState::Degraded);
    }

    // ─── HealthStatus ─────────────────────────────────────────────────────

    #[test]
    fn health_status_is_healthy() {
        let h = HealthStatus::Healthy {
            latency: Duration::from_millis(10),
        };
        assert!(h.is_healthy());
        assert!(!h.is_degraded());
        assert!(!h.is_unhealthy());
    }

    #[test]
    fn health_status_is_degraded() {
        let h = HealthStatus::Degraded {
            message: "high latency".into(),
            latency: Duration::from_millis(500),
        };
        assert!(!h.is_healthy());
        assert!(h.is_degraded());
        assert!(!h.is_unhealthy());
    }

    #[test]
    fn health_status_is_unhealthy() {
        let h = HealthStatus::Unhealthy {
            message: "crash loop".into(),
            error_code: ErrorCode::new("CRASH_LOOP"),
        };
        assert!(!h.is_healthy());
        assert!(!h.is_degraded());
        assert!(h.is_unhealthy());
    }

    #[test]
    fn health_status_clone() {
        let h = HealthStatus::Healthy {
            latency: Duration::from_secs(1),
        };
        let cloned = h.clone();
        assert!(cloned.is_healthy());
    }

    // ─── ComponentManifest ────────────────────────────────────────────────

    #[test]
    fn component_manifest_creation() {
        let manifest = ComponentManifest {
            name: "test-component".into(),
            version: SemVer::new(1, 0, 0),
            description: "A test component".into(),
            dependencies: vec!["core".into()],
            required_capabilities: vec![CapabilityId::from_string("storage")],
            provided_capabilities: vec![],
            startup_timeout: Duration::from_secs(30),
            shutdown_timeout: Duration::from_secs(10),
            health_check_interval: Duration::from_secs(5),
        };
        assert_eq!(manifest.name, "test-component");
        assert_eq!(manifest.version, SemVer::new(1, 0, 0));
        assert_eq!(manifest.dependencies.len(), 1);
        assert!(manifest.provided_capabilities.is_empty());
    }

    #[test]
    fn component_manifest_clone() {
        let m = ComponentManifest {
            name: "c".into(),
            version: SemVer::new(0, 0, 1),
            description: "d".into(),
            dependencies: vec![],
            required_capabilities: vec![],
            provided_capabilities: vec![],
            startup_timeout: Duration::from_secs(10),
            shutdown_timeout: Duration::from_secs(5),
            health_check_interval: Duration::from_secs(1),
        };
        let cloned = m.clone();
        assert_eq!(m.name, cloned.name);
    }

    // ─── CapabilityDefinition ─────────────────────────────────────────────

    #[test]
    fn capability_definition_creation() {
        let cap = CapabilityDefinition {
            id: CapabilityId::from_string("dom.sensor"),
            version: SemVer::new(2, 1, 0),
            description: "DOM sensor capability".into(),
            interface: "dom.sensor.v2".into(),
        };
        assert_eq!(cap.id.as_str(), "dom.sensor");
        assert_eq!(cap.version, SemVer::new(2, 1, 0));
        assert_eq!(cap.interface, "dom.sensor.v2");
    }

    #[test]
    fn capability_definition_clone() {
        let a = CapabilityDefinition {
            id: CapabilityId::from_string("test"),
            version: SemVer::new(1, 0, 0),
            description: "test cap".into(),
            interface: "test.v1".into(),
        };
        let b = a.clone();
        assert_eq!(a.id, b.id);
        assert_eq!(a.version, b.version);
    }

    // ─── HealthCheckDefinition ────────────────────────────────────────────

    #[test]
    fn health_check_definition_creation() {
        let hc = HealthCheckDefinition {
            name: "liveness".into(),
            interval: Duration::from_secs(10),
            timeout: Duration::from_secs(1),
            failure_threshold: 3,
        };
        assert_eq!(hc.name, "liveness");
        assert_eq!(hc.failure_threshold, 3);
    }

    // ─── ResourceRequirements ─────────────────────────────────────────────

    #[test]
    fn resource_requirements_creation() {
        let req = ResourceRequirements {
            memory_bytes: 1024 * 1024 * 100,
            file_descriptors: 128,
            max_tasks: 10,
        };
        assert_eq!(req.memory_bytes, 104_857_600);
        assert_eq!(req.max_tasks, 10);
    }
}
