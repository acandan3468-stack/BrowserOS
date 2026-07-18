//! Soak test: memory stability under continuous load.
//!
//! Run with:
//!   cargo test --test soak_memory_stability -- --ignored --nocapture
//!   SOAK_SECS=180 cargo test ...     (override duration)

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use browseros_event_bus::EventBus;
use browseros_lifecycle::LifecycleManager;
use browseros_observability::MetricsRegistry;
use browseros_scheduler::Scheduler;
use browseros_types::event::{Event, EventCategory, EventMetadata};
use browseros_types::identifiers::{CorrelationId, ModuleId};
use browseros_types::value::{ContentType, SemVer};

use sysinfo::System;

fn soak_duration() -> Duration {
    std::env::var("SOAK_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .map(Duration::from_secs)
        .unwrap_or(Duration::from_secs(120))
}

fn make_event(kind: &str, correlation_id: CorrelationId) -> Box<dyn Event> {
    use std::any::Any;
    struct E {
        k: &'static str,
        m: EventMetadata,
    }
    impl std::fmt::Debug for E {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("E").finish()
        }
    }
    impl Event for E {
        fn kind(&self) -> &'static str {
            self.k
        }
        fn category(&self) -> EventCategory {
            EventCategory::Domain
        }
        fn metadata(&self) -> &EventMetadata {
            &self.m
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
    }
    Box::new(E {
        k: Box::leak(kind.to_owned().into_boxed_str()),
        m: EventMetadata::new(
            ModuleId::new("mem-soak", SemVer::new(1, 0, 0)),
            correlation_id,
            None,
            ContentType::new("app/json"),
            chrono::Utc::now(),
        ),
    })
}

fn sample_memory(sys: &System) -> u64 {
    if let Some(process) = sys.process(sysinfo::Pid::from_u32(std::process::id())) {
        return process.memory();
    }
    0
}

// ─── Test 1: Memory trend under continuous event load ──────────────────

#[test]
fn memory_stable_under_continuous_events() {
    let bus = EventBus::new();
    let mut sys = System::new();

    let counter = Arc::new(AtomicU64::new(0));
    let c = counter.clone();
    bus.subscribe(
        "mem_test",
        Arc::new(move |_: &dyn Event| {
            c.fetch_add(1, Ordering::Relaxed);
        }),
    );

    let duration = soak_duration();
    let sample_interval = Duration::from_secs(5);
    let _num_samples = (duration.as_secs_f64() / sample_interval.as_secs_f64()) as usize;
    let running = Arc::new(AtomicBool::new(true));

    // Publisher thread
    let r = running.clone();
    let b = bus.clone();
    let pub_thread = std::thread::spawn(move || {
        while r.load(Ordering::Relaxed) {
            b.publish(make_event("mem_test", CorrelationId::new()));
            std::thread::sleep(Duration::from_micros(10));
        }
    });

    // Memory sampling thread
    let mem_samples: Arc<std::sync::Mutex<Vec<(f64, u64)>>> =
        Arc::new(std::sync::Mutex::new(Vec::new()));
    let ms = mem_samples.clone();
    let r = running.clone();
    let sample_thread = std::thread::spawn(move || {
        sys.refresh_memory();
        let start = Instant::now();
        while r.load(Ordering::Relaxed) {
            std::thread::sleep(sample_interval);
            sys.refresh_memory();
            let mem = sample_memory(&sys);
            let t = start.elapsed().as_secs_f64();
            ms.lock().unwrap().push((t, mem));
            let latest = mem;
            let first = ms
                .lock()
                .unwrap()
                .first()
                .copied()
                .map(|(_, m)| m)
                .unwrap_or(latest);
            let delta = if latest > first { latest - first } else { 0 };
            eprintln!(
                "  t={t:.0}s mem={}KB delta={}KB",
                latest / 1024,
                delta / 1024
            );
        }
    });

    std::thread::sleep(duration);
    running.store(false, Ordering::Relaxed);
    std::thread::sleep(Duration::from_millis(100));

    let _ = pub_thread.join();
    let _ = sample_thread.join();

    let total_events = counter.load(Ordering::Relaxed);
    let samples = mem_samples.lock().unwrap();

    assert!(
        samples.len() >= 3,
        "Need at least 3 memory samples, got {}",
        samples.len()
    );

    // Check for linear growth: compare first vs last third
    let mid = samples.len() / 2;
    let first_avg = samples[..mid].iter().map(|(_, m)| *m).sum::<u64>() / mid as u64;
    let last_avg =
        samples[mid..].iter().map(|(_, m)| *m).sum::<u64>() / (samples.len() - mid) as u64;

    let growth = if last_avg > first_avg {
        last_avg - first_avg
    } else {
        0
    };

    // Allow 5MB growth for safe caches
    assert!(
        growth < 5 * 1024 * 1024,
        "Memory grew by {}KB from avg first-half to avg second-half — possible leak",
        growth / 1024,
    );

    eprintln!(
        "Memory stable: {} events, {} samples, first_avg={}KB last_avg={}KB growth={}KB",
        total_events,
        samples.len(),
        first_avg / 1024,
        last_avg / 1024,
        growth / 1024,
    );
}

// ─── Test 2: No retained event references ──────────────────────────────

#[test]
fn no_retained_event_references() {
    let bus = EventBus::new();
    let mut sys = System::new();

    let runs = 5;
    let events_per_run = 50_000u64;
    let mut mem_snapshots = Vec::new();

    for run in 0..runs {
        let counter = Arc::new(AtomicU64::new(0));
        let c = counter.clone();
        bus.subscribe(
            &format!("run_{run}"),
            Arc::new(move |_: &dyn Event| {
                c.fetch_add(1, Ordering::Relaxed);
            }),
        );

        for _ in 0..events_per_run {
            bus.publish(make_event(&format!("run_{run}"), CorrelationId::new()));
        }

        assert_eq!(
            counter.load(Ordering::Relaxed),
            events_per_run,
            "Event loss in run {run}"
        );

        // Force cleanup of subscriber by replacing it
        let noop: Arc<dyn browseros_event_bus::EventHandler> = Arc::new(|_: &dyn Event| {});
        let _replace = bus.subscribe(&format!("run_{run}"), noop);

        sys.refresh_memory();
        mem_snapshots.push(sample_memory(&sys));
        eprintln!("  Run {run}: mem={}KB", mem_snapshots[run] / 1024);
    }

    // Memory should not grow linearly across runs
    if mem_snapshots.len() >= 3 {
        let growth_run0_to_last = if mem_snapshots.last() > mem_snapshots.first() {
            mem_snapshots.last().unwrap() - mem_snapshots.first().unwrap()
        } else {
            0
        };
        assert!(
            growth_run0_to_last < 3 * 1024 * 1024,
            "Memory grew {}KB across {runs} runs — event references retained",
            growth_run0_to_last / 1024,
        );
    }
    eprintln!("No retained references: memory stable across {runs} runs");
}

// ─── Test 3: No Arc cycle leaks in scheduler + bus + lifecycle ─────────

#[test]
fn no_arc_cycle_leaks() {
    let bus = Arc::new(EventBus::new());
    let metrics = Arc::new(MetricsRegistry::new());
    let mut sys = System::new();

    let cycles = 20;
    let ops_per_cycle = 10_000;
    let mut mem_readings = Vec::new();

    for cycle in 0..cycles {
        // Create fresh subsystems each cycle
        let scheduler = Arc::new(Scheduler::new(bus.clone()));
        let lifecycle = Arc::new(LifecycleManager::new(
            bus.clone(),
            Arc::new(browseros_observability::Logger::new(
                Arc::new(browseros_observability::StdoutSink::new()),
                Arc::new(browseros_observability::LevelFilter::new(
                    browseros_observability::LogLevel::Error,
                )),
                "cycle",
            )),
            ModuleId::new("cycle", SemVer::new(1, 0, 0)),
        ));

        // Register & transition components
        let comp_name = format!("cycle_{cycle}");
        lifecycle.register_component(&comp_name);
        let _ = lifecycle.transition_to(
            &comp_name,
            browseros_lifecycle::LifecycleState::Initializing,
        );
        let _ = lifecycle.transition_to(&comp_name, browseros_lifecycle::LifecycleState::Running);

        // Schedule tasks
        for _ in 0..ops_per_cycle {
            let m = metrics.clone();
            scheduler.schedule_after(Duration::from_millis(1), move || {
                m.counter("cycle_ops").increment();
            });
        }

        // Publish events
        for _ in 0..ops_per_cycle {
            bus.publish(make_event("cycle", CorrelationId::new()));
        }

        // Drop scheduler and lifecycle to test for Arc cycles
        drop(scheduler);
        drop(lifecycle);

        std::thread::sleep(Duration::from_millis(100));
        sys.refresh_memory();
        mem_readings.push(sample_memory(&sys));
        eprintln!("  Cycle {cycle}: mem={}KB", mem_readings[cycle] / 1024);
    }

    // Verify no linear growth
    if mem_readings.len() >= 3 {
        let first_avg = mem_readings[..mem_readings.len() / 2].iter().sum::<u64>()
            / (mem_readings.len() / 2) as u64;
        let last_avg = mem_readings[mem_readings.len() / 2..].iter().sum::<u64>()
            / (mem_readings.len() - mem_readings.len() / 2) as u64;
        let growth = if last_avg > first_avg {
            last_avg - first_avg
        } else {
            0
        };
        assert!(
            growth < 3 * 1024 * 1024,
            "Memory grew {}KB across {cycles} cycles — possible Arc cycle leak",
            growth / 1024,
        );
    }

    assert_eq!(
        metrics.counter("cycle_ops").value(),
        (cycles * ops_per_cycle) as u64,
        "All cycle_ops should have been counted"
    );
    eprintln!("No Arc cycle leaks: memory stable across {cycles} cycles");
}
