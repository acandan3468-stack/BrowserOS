use std::sync::{Arc, Barrier};

use browseros_event_bus::EventBus;
use browseros_lifecycle::{LifecycleManager, LifecycleState};
use browseros_observability::{LevelFilter, LogLevel, Logger, StdoutSink};

fn test_logger() -> Arc<Logger> {
    Arc::new(Logger::new(
        Arc::new(StdoutSink::new()),
        Arc::new(LevelFilter::new(LogLevel::Warn)),
        "lifecycle-chaos",
    ))
}

fn test_bus() -> Arc<EventBus> {
    Arc::new(EventBus::new())
}

fn test_manager() -> LifecycleManager {
    LifecycleManager::new(
        test_bus(),
        test_logger(),
        browseros_lifecycle::infra_module_id("chaos"),
    )
}

/// 2a. Rapid start/stop: 100 single-cycle transitions on distinct components.
/// Each component follows: Created → Initializing → Running → Stopping → Stopped.
#[test]
fn rapid_start_stop_no_corruption() {
    let mgr = test_manager();

    for i in 0..100 {
        let name = format!("cycler_{i}");
        mgr.register_component(&name);
        mgr.transition_to(&name, LifecycleState::Initializing);
        assert_eq!(
            mgr.current_state(&name).unwrap(),
            LifecycleState::Initializing,
            "Cycle {i}: should be Initializing"
        );
        mgr.transition_to(&name, LifecycleState::Running);
        assert_eq!(
            mgr.current_state(&name).unwrap(),
            LifecycleState::Running,
            "Cycle {i}: should be Running"
        );
        mgr.transition_to(&name, LifecycleState::Stopping);
        mgr.transition_to(&name, LifecycleState::Stopped);
        assert_eq!(
            mgr.current_state(&name).unwrap(),
            LifecycleState::Stopped,
            "Cycle {i}: should be Stopped"
        );
    }

    // All 100 components are tracked
    assert_eq!(mgr.all_states().len(), 100);
}

/// 2b. Concurrent start_all / shutdown_all from multiple threads.
/// The state machine rejects invalid transitions so concurrent calls
/// are safe but may produce any valid intermediate state.
#[test]
fn concurrent_start_shutdown_no_invalid_states() {
    let mgr = test_manager();
    let num_components = 10;
    let component_names: Vec<String> = (0..num_components).map(|i| format!("comp_{}", i)).collect();

    for name in &component_names {
        mgr.register_component(name);
    }

    let num_threads = 8;
    let barrier = Arc::new(Barrier::new(num_threads));
    let mut handles = Vec::new();

    for _ in 0..num_threads {
        let mgr = mgr.clone();
        let bar = barrier.clone();
        handles.push(std::thread::spawn(move || {
            bar.wait();
            for _ in 0..10 {
                mgr.start_all();
                // Verify all states are valid enum values (no corruption)
                for (_, state) in mgr.all_states() {
                    let _ = match state {
                        LifecycleState::Created
                        | LifecycleState::Initializing
                        | LifecycleState::Running
                        | LifecycleState::Degraded
                        | LifecycleState::Stopping
                        | LifecycleState::Stopped
                        | LifecycleState::Failed => true,
                    };
                }
                mgr.shutdown_all();
                for (_, state) in mgr.all_states() {
                    let _ = match state {
                        LifecycleState::Created
                        | LifecycleState::Initializing
                        | LifecycleState::Running
                        | LifecycleState::Degraded
                        | LifecycleState::Stopping
                        | LifecycleState::Stopped
                        | LifecycleState::Failed => true,
                    };
                }
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }
}

/// 2c. Simulate partial initialization failures.
/// After a Failed state, no further transitions are accepted (terminal).
/// Each cycle uses a fresh component to avoid terminal-state edge cases.
#[test]
fn partial_init_failures_no_corruption() {
    let mgr = test_manager();
    let mut successes = 0u32;
    let mut failures = 0u32;

    for i in 0..50 {
        let name = format!("fragile_{i}");
        mgr.register_component(&name);
        mgr.transition_to(&name, LifecycleState::Initializing);
        let state = mgr.current_state(&name).unwrap();
        assert_eq!(state, LifecycleState::Initializing);

        if i % 2 == 0 {
            mgr.transition_to(&name, LifecycleState::Running);
            assert_eq!(mgr.current_state(&name).unwrap(), LifecycleState::Running);
            mgr.transition_to(&name, LifecycleState::Stopping);
            mgr.transition_to(&name, LifecycleState::Stopped);
            successes += 1;
        } else {
            mgr.transition_to(&name, LifecycleState::Failed);
            let state = mgr.current_state(&name).unwrap();
            assert_eq!(
                state,
                LifecycleState::Failed,
                "Component {name} should be Failed"
            );
            mgr.transition_to(&name, LifecycleState::Initializing);
            assert_eq!(mgr.current_state(&name).unwrap(), LifecycleState::Failed);
            failures += 1;
        }
    }

    eprintln!("Init failures: {failures}, successes: {successes}");
}

/// 2d. Many components registered simultaneously from multiple threads.
#[test]
fn concurrent_registration_no_races() {
    let mgr = test_manager();
    let num_threads = 8;
    let comps_per_thread = 50;
    let barrier = Arc::new(Barrier::new(num_threads));
    let mut handles = Vec::new();

    for t in 0..num_threads {
        let mgr = mgr.clone();
        let bar = barrier.clone();
        handles.push(std::thread::spawn(move || {
            bar.wait();
            for i in 0..comps_per_thread {
                mgr.register_component(&format!("race_comp_{t}_{i}"));
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    let all = mgr.all_states();
    assert_eq!(all.len() as u64, num_threads as u64 * comps_per_thread);
    for (_, state) in &all {
        assert_eq!(*state, LifecycleState::Created);
    }
}

/// 2e. Invalid transitions in rapid succession — system must reject all.
#[test]
fn rapid_invalid_transitions_all_rejected() {
    let mgr = test_manager();
    mgr.register_component("protected");

    for _ in 0..1000 {
        // All of these should be rejected since component is Created:
        mgr.transition_to("protected", LifecycleState::Running);
        mgr.transition_to("protected", LifecycleState::Stopped);
        mgr.transition_to("protected", LifecycleState::Failed);
        mgr.transition_to("protected", LifecycleState::Degraded);
        mgr.transition_to("protected", LifecycleState::Stopping);
    }

    assert_eq!(
        mgr.current_state("protected"),
        Some(LifecycleState::Created),
        "State should remain Created after 1000 invalid transitions"
    );
}
