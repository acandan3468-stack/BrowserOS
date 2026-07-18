//! # browseros-lifecycle
//!
//! Component lifecycle management for the BrowserOS runtime.
//!
//! The `LifecycleManager` tracks component state transitions, emits
//! lifecycle events on the event bus, and logs every transition via
//! the observability logger.  This enables other components (health
//! probes, metrics collectors, admin interfaces) to react to component
//! state changes.
//!
//! ## State machine
//!
//! Each component follows: `Created → Initializing → Running → Stopping → Stopped`
//! or `Running → Failed` on unrecoverable error.
//!
//! ## Integration
//!
//! Lifecycle events carry `correlation_id` from the lifecycle manager's
//! logger, enabling end-to-end tracing of start-up sequences.

use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use browseros_event_bus::EventBus;
use browseros_observability::{LogLevel, Logger};
use browseros_types::event::{Event, EventCategory, EventMetadata};
use browseros_types::identifiers::{CorrelationId, ModuleId};
use browseros_types::value::{ContentType, SemVer};

/// Component state as tracked by the lifecycle manager.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleState {
    Created,
    Initializing,
    Running,
    Degraded,
    Stopping,
    Stopped,
    Failed,
}

impl LifecycleState {
    fn label(&self) -> &'static str {
        match self {
            LifecycleState::Created => "created",
            LifecycleState::Initializing => "initializing",
            LifecycleState::Running => "running",
            LifecycleState::Degraded => "degraded",
            LifecycleState::Stopping => "stopping",
            LifecycleState::Stopped => "stopped",
            LifecycleState::Failed => "failed",
        }
    }
}

/// Event published when a component transitions between states.
#[derive(Debug)]
pub struct LifecycleTransitionEvent {
    metadata: EventMetadata,
    pub component_name: String,
    pub from: LifecycleState,
    pub to: LifecycleState,
}

impl Event for LifecycleTransitionEvent {
    fn kind(&self) -> &'static str {
        "lifecycle.transition"
    }

    fn category(&self) -> EventCategory {
        EventCategory::System
    }

    fn metadata(&self) -> &EventMetadata {
        &self.metadata
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Returns true if the transition `from → to` is valid.
fn is_valid_transition(from: LifecycleState, to: LifecycleState) -> bool {
    matches!(
        (from, to),
        (LifecycleState::Created, LifecycleState::Initializing)
            | (LifecycleState::Initializing, LifecycleState::Running)
            | (LifecycleState::Initializing, LifecycleState::Failed)
            | (LifecycleState::Running, LifecycleState::Degraded)
            | (LifecycleState::Running, LifecycleState::Stopping)
            | (LifecycleState::Running, LifecycleState::Failed)
            | (LifecycleState::Degraded, LifecycleState::Running)
            | (LifecycleState::Degraded, LifecycleState::Stopping)
            | (LifecycleState::Degraded, LifecycleState::Failed)
            | (LifecycleState::Stopping, LifecycleState::Stopped)
            | (LifecycleState::Stopping, LifecycleState::Failed)
    )
}

/// Manages component lifecycle state and emits lifecycle events.
///
/// Thread-safe.  All state mutations are protected by `RwLock`.
#[derive(Clone)]
pub struct LifecycleManager {
    inner: Arc<LifecycleInner>,
}

struct LifecycleInner {
    states: RwLock<HashMap<String, LifecycleState>>,
    bus: Arc<EventBus>,
    logger: Arc<Logger>,
    module_id: ModuleId,
}

impl LifecycleManager {
    /// Create a new lifecycle manager.
    ///
    /// The manager uses the given `module_id` as the source of lifecycle events.
    pub fn new(bus: Arc<EventBus>, logger: Arc<Logger>, module_id: ModuleId) -> Self {
        Self {
            inner: Arc::new(LifecycleInner {
                states: RwLock::new(HashMap::new()),
                bus,
                logger,
                module_id,
            }),
        }
    }

    /// Register a component and set its initial state to `Created`.
    pub fn register_component(&self, name: &str) {
        let mut states = self.inner.states.write().unwrap();
        states
            .entry(name.to_owned())
            .or_insert(LifecycleState::Created);
    }

    /// Transition a component to a new state.
    ///
    /// Emits a `LifecycleTransitionEvent` on the event bus and logs the
    /// transition.  If the transition is invalid, the method logs a warning
    /// and does NOT change state.
    pub fn transition_to(&self, component: &str, to: LifecycleState) {
        let mut states = self.inner.states.write().unwrap();
        let from = states
            .get(component)
            .copied()
            .unwrap_or(LifecycleState::Created);

        if !is_valid_transition(from, to) {
            self.inner.logger.log_with(
                LogLevel::Warn,
                format!(
                    "invalid lifecycle transition for '{}': {} -> {}",
                    component,
                    from.label(),
                    to.label(),
                ),
                vec![],
            );
            return;
        }

        states.insert(component.to_owned(), to);

        // Log the transition
        self.inner.logger.log_with(
            LogLevel::Info,
            format!(
                "component '{}' transitioned: {} -> {}",
                component,
                from.label(),
                to.label(),
            ),
            vec![],
        );

        // Emit lifecycle event
        let meta = EventMetadata::new(
            self.inner.module_id.clone(),
            CorrelationId::new(),
            None,
            ContentType::new("application/x-browseros-lifecycle-event"),
            chrono::Utc::now(),
        );

        self.inner.bus.publish(Box::new(LifecycleTransitionEvent {
            metadata: meta,
            component_name: component.to_owned(),
            from,
            to,
        }));
    }

    /// Returns the current state of a component, or `None` if unknown.
    pub fn current_state(&self, component: &str) -> Option<LifecycleState> {
        self.inner.states.read().unwrap().get(component).copied()
    }

    /// Returns all registered component names and their states.
    pub fn all_states(&self) -> HashMap<String, LifecycleState> {
        self.inner.states.read().unwrap().clone()
    }

    /// Start all registered components (transition them to `Running`).
    ///
    /// Components transition through `Created → Initializing → Running`.
    /// If a component was already `Created`, it moves through the chain.
    /// Otherwise, the current state is preserved.
    pub fn start_all(&self) {
        let names: Vec<String> = {
            let states = self.inner.states.read().unwrap();
            states.keys().cloned().collect()
        };

        for name in &names {
            let current = self.current_state(name);
            if current == Some(LifecycleState::Created) {
                self.transition_to(name, LifecycleState::Initializing);
                self.transition_to(name, LifecycleState::Running);
            }
        }
    }

    /// Initiate graceful shutdown of all components.
    ///
    /// Transitions `Running` and `Degraded` components to `Stopping`,
    /// then to `Stopped`.
    pub fn shutdown_all(&self) {
        let names: Vec<String> = {
            let states = self.inner.states.read().unwrap();
            states.keys().cloned().collect()
        };

        for name in &names {
            let current = self.current_state(name);
            if current == Some(LifecycleState::Running) || current == Some(LifecycleState::Degraded)
            {
                self.transition_to(name, LifecycleState::Stopping);
                self.transition_to(name, LifecycleState::Stopped);
            }
        }
    }
}

impl std::fmt::Debug for LifecycleManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LifecycleManager").finish()
    }
}

/// Helper to create a minimal module ID for infrastructure components.
pub fn infra_module_id(name: &str) -> ModuleId {
    ModuleId::new(name, SemVer::new(0, 1, 0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use browseros_observability::{LevelFilter, LogLevel, StdoutSink};

    fn test_logger() -> Arc<Logger> {
        Arc::new(Logger::new(
            Arc::new(StdoutSink::new()),
            Arc::new(LevelFilter::new(LogLevel::Trace)),
            "lifecycle-test",
        ))
    }

    fn test_bus() -> Arc<EventBus> {
        Arc::new(EventBus::new())
    }

    fn test_manager() -> LifecycleManager {
        LifecycleManager::new(test_bus(), test_logger(), infra_module_id("lifecycle-test"))
    }

    #[test]
    fn register_component_creates_state() {
        let mgr = test_manager();
        mgr.register_component("my-component");
        assert_eq!(
            mgr.current_state("my-component"),
            Some(LifecycleState::Created)
        );
    }

    #[test]
    fn unknown_component_returns_none() {
        let mgr = test_manager();
        assert_eq!(mgr.current_state("unknown"), None);
    }

    #[test]
    fn valid_transition_succeeds() {
        let mgr = test_manager();
        mgr.register_component("c1");
        mgr.transition_to("c1", LifecycleState::Initializing);
        assert_eq!(mgr.current_state("c1"), Some(LifecycleState::Initializing));
        mgr.transition_to("c1", LifecycleState::Running);
        assert_eq!(mgr.current_state("c1"), Some(LifecycleState::Running));
    }

    #[test]
    fn invalid_transition_is_rejected() {
        let mgr = test_manager();
        mgr.register_component("c1");
        // Cannot go directly from Created to Running
        mgr.transition_to("c1", LifecycleState::Running);
        assert_eq!(mgr.current_state("c1"), Some(LifecycleState::Created));
    }

    #[test]
    fn lifecycle_event_is_emitted() {
        let bus = test_bus();
        let transitions = Arc::new(Mutex::new(Vec::new()));
        let t = transitions.clone();

        bus.subscribe(
            "lifecycle.transition",
            Arc::new(move |ev: &dyn Event| {
                if let Some(lce) = ev.as_any().downcast_ref::<LifecycleTransitionEvent>() {
                    t.lock()
                        .unwrap()
                        .push((lce.component_name.clone(), lce.from, lce.to));
                }
            }),
        );

        let mgr = LifecycleManager::new(bus, test_logger(), infra_module_id("test"));
        mgr.register_component("c1");
        mgr.transition_to("c1", LifecycleState::Initializing);

        let events = transitions.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, "c1");
        assert_eq!(events[0].1, LifecycleState::Created);
        assert_eq!(events[0].2, LifecycleState::Initializing);
    }

    #[test]
    fn start_all_transitions_through_chain() {
        let mgr = test_manager();
        mgr.register_component("a");
        mgr.register_component("b");
        mgr.start_all();

        assert_eq!(mgr.current_state("a"), Some(LifecycleState::Running));
        assert_eq!(mgr.current_state("b"), Some(LifecycleState::Running));
    }

    #[test]
    fn shutdown_all_transitions_to_stopped() {
        let mgr = test_manager();
        mgr.register_component("a");
        mgr.start_all();
        mgr.shutdown_all();

        assert_eq!(mgr.current_state("a"), Some(LifecycleState::Stopped));
    }

    #[test]
    fn all_states_returns_all_components() {
        let mgr = test_manager();
        mgr.register_component("x");
        mgr.register_component("y");
        let states = mgr.all_states();
        assert_eq!(states.len(), 2);
    }

    #[test]
    fn double_transition_to_same_state_is_noop() {
        let mgr = test_manager();
        mgr.register_component("c1");
        mgr.transition_to("c1", LifecycleState::Initializing);
        // Already initializing, trying to set initializing again is invalid
        // (Created→Initializing is valid but Initializing→Initializing is not)
        mgr.transition_to("c1", LifecycleState::Initializing);
        assert_eq!(mgr.current_state("c1"), Some(LifecycleState::Initializing));
    }
}
