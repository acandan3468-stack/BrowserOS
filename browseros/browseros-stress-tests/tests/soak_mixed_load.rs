//! Soak test: all subsystems simultaneously under sustained load.
//!
//! Run with:
//!   cargo test --test soak_mixed_load -- --ignored --nocapture
//!   SOAK_SECS=300 cargo test ...      (override duration)

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use browseros_event_bus::EventBus;
use browseros_lifecycle::LifecycleManager;
use browseros_observability::{LevelFilter, LogLevel, Logger, MetricsRegistry, StdoutSink, Tracer};
use browseros_scheduler::Scheduler;
use browseros_types::event::{Event, EventCategory, EventMetadata};
use browseros_types::identifiers::{CorrelationId, ModuleId};
use browseros_types::value::{ContentType, SemVer};

fn soak_duration() -> Duration {
    std::env::var("SOAK_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .map(Duration::from_secs)
        .unwrap_or(Duration::from_secs(90))
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
            ModuleId::new("mixed-soak", SemVer::new(1, 0, 0)),
            correlation_id,
            None,
            ContentType::new("app/json"),
            chrono::Utc::now(),
        ),
    })
}

fn test_logger() -> Arc<Logger> {
    Arc::new(Logger::new(
        Arc::new(StdoutSink::new()),
        Arc::new(LevelFilter::new(LogLevel::Warn)),
        "mixed-soak",
    ))
}

// ─── Test: All subsystems simultaneously ───────────────────────────────

#[test]
fn all_subsystems_simultaneous_no_interference() {
    let bus = Arc::new(EventBus::new());
    let logger = test_logger();
    let metrics = Arc::new(MetricsRegistry::new());
    let tracer = Arc::new(Tracer::new());
    let scheduler = Arc::new(Scheduler::new(bus.clone()));
    let lifecycle = Arc::new(LifecycleManager::new(
        bus.clone(),
        logger.clone(),
        ModuleId::new("mixed-soak-lifecycle", SemVer::new(1, 0, 0)),
    ));

    let duration = soak_duration();
    let running = Arc::new(AtomicBool::new(true));
    let event_count = Arc::new(AtomicU64::new(0));
    let delayed_count = Arc::new(AtomicU64::new(0));
    let lifecycle_ops = Arc::new(AtomicU64::new(0));

    // ── Event Bus producer ──
    let ec = event_count.clone();
    let r = running.clone();
    let b = bus.clone();
    let bus_producer = std::thread::spawn(move || {
        while r.load(Ordering::Relaxed) {
            b.publish(make_event("mixed", CorrelationId::new()));
            ec.fetch_add(1, Ordering::Relaxed);
            std::thread::sleep(Duration::from_micros(50));
        }
    });

    // ── Event Bus consumer with logging & metrics ──
    let (b, l, m, t) = (
        bus.clone(),
        test_logger(),
        metrics.clone(),
        Arc::new(Tracer::new()),
    );
    b.subscribe(
        "mixed",
        Arc::new(move |ev: &dyn Event| {
            l.info(format!("mixed ev"));
            m.counter("mixed_events").increment();
            let mut span = t.start_span("handle_mixed");
            span.set_field("cid", format!("{:?}", ev.metadata().correlation_id));
            span.finish();
        }),
    );

    // ── Lifecycle transitions ──
    let lc = lifecycle.clone();
    let lo = lifecycle_ops.clone();
    let r = running.clone();
    let lifecycle_thread = std::thread::spawn(move || {
        let mut comp_idx = 0u64;
        while r.load(Ordering::Relaxed) {
            let name = format!("comp_{}", comp_idx % 10);
            lc.register_component(&name);
            let _ = lc.transition_to(&name, browseros_lifecycle::LifecycleState::Initializing);
            let _ = lc.transition_to(&name, browseros_lifecycle::LifecycleState::Running);
            let _ = lc.transition_to(&name, browseros_lifecycle::LifecycleState::Stopping);
            let _ = lc.transition_to(&name, browseros_lifecycle::LifecycleState::Stopped);
            lo.fetch_add(1, Ordering::Relaxed);
            comp_idx += 1;
            std::thread::sleep(Duration::from_millis(5));
        }
    });

    // ── Scheduler delayed tasks ──
    let sc = scheduler.clone();
    let dc = delayed_count.clone();
    let r = running.clone();
    let scheduler_thread = std::thread::spawn(move || {
        while r.load(Ordering::Relaxed) {
            let d = dc.clone();
            sc.schedule_after(Duration::from_millis(10), move || {
                d.fetch_add(1, Ordering::Relaxed);
            });
            std::thread::sleep(Duration::from_millis(20));
        }
    });

    // ── Metrics and tracer sampling ──
    let m = metrics.clone();
    let tr = tracer.clone();
    let r = running.clone();
    let observability_thread = std::thread::spawn(move || {
        while r.load(Ordering::Relaxed) {
            m.gauge("load_gauge").set(42.0);
            m.histogram("load_latency").observe(0.5);
            let mut span = tr.start_span("sampling_span");
            span.set_field("sample", "value".into());
            span.finish();
            std::thread::sleep(Duration::from_millis(30));
        }
    });

    // Let it run for the soak duration
    std::thread::sleep(duration);
    running.store(false, Ordering::Relaxed);

    // Let scheduler tasks drain
    std::thread::sleep(Duration::from_millis(500));

    // Join
    for h in [
        bus_producer,
        lifecycle_thread,
        scheduler_thread,
        observability_thread,
    ] {
        let _ = h.join();
    }

    let ev = event_count.load(Ordering::Relaxed);
    let dc = delayed_count.load(Ordering::Relaxed);
    let lo = lifecycle_ops.load(Ordering::Relaxed);
    let mc = metrics.counter("mixed_events").value();

    // Verify metrics counter was incremented by event handler
    assert!(
        mc >= ev / 2,
        "Metrics counter ({mc}) should track events (~{ev})"
    );
    assert!(lo > 0, "Lifecycle operations should have occurred ({lo})");
    assert!(dc > 0, "Delayed tasks should have executed ({dc})");

    // Snapshot consistency
    let snap = metrics.snapshot();
    assert!(
        snap.counters.contains_key("mixed_events"),
        "Snapshot should contain mixed_events"
    );
    assert!(
        snap.gauges.contains_key("load_gauge"),
        "Snapshot should contain load_gauge"
    );
    assert!(
        snap.histograms.contains_key("load_latency"),
        "Snapshot should contain load_latency"
    );

    eprintln!(
        "Mixed load: {} events, {} delayed tasks, {} lifecycle ops, {} metrics count",
        ev, dc, lo, mc,
    );
    eprintln!("Duration: {:.1}s", duration.as_secs_f64());
}

// ─── Test: Scheduler + Event Bus — no starvation ───────────────────────

#[test]
fn scheduler_and_event_bus_no_starvation() {
    let bus = Arc::new(EventBus::new());
    let sched = Scheduler::new(bus.clone());

    let bus_count = Arc::new(AtomicU64::new(0));
    let sched_count = Arc::new(AtomicU64::new(0));

    let bc = bus_count.clone();
    bus.subscribe(
        "starve",
        Arc::new(move |_: &dyn Event| {
            bc.fetch_add(1, Ordering::Relaxed);
        }),
    );

    let duration = soak_duration().min(Duration::from_secs(30));
    let running = Arc::new(AtomicBool::new(true));
    let total_expected = Arc::new(AtomicU64::new(0));
    let sched_expected = Arc::new(AtomicU64::new(0));

    // Busy event publisher
    let r = running.clone();
    let b = bus.clone();
    let te = total_expected.clone();
    let pub_thread = std::thread::spawn(move || {
        while r.load(Ordering::Relaxed) {
            b.publish(make_event("starve", CorrelationId::new()));
            te.fetch_add(1, Ordering::Relaxed);
            std::thread::sleep(Duration::from_micros(100));
        }
    });

    // Concurrent scheduler
    let r = running.clone();
    let sc = sched.clone();
    let se = sched_expected.clone();
    let sc2 = sched_count.clone();
    let sched_thread = std::thread::spawn(move || {
        while r.load(Ordering::Relaxed) {
            let c = sc2.clone();
            sc.schedule_after(Duration::from_millis(5), move || {
                c.fetch_add(1, Ordering::Relaxed);
            });
            se.fetch_add(1, Ordering::Relaxed);
            std::thread::sleep(Duration::from_millis(10));
        }
    });

    std::thread::sleep(duration);
    running.store(false, Ordering::Relaxed);
    std::thread::sleep(Duration::from_millis(500));

    let _ = pub_thread.join();
    let _ = sched_thread.join();

    let bc = bus_count.load(Ordering::Relaxed);
    let sc = sched_count.load(Ordering::Relaxed);
    let te = total_expected.load(Ordering::Relaxed);

    // Both should have made progress — neither starves the other
    assert!(bc > 0, "Bus should have received events");
    assert!(sc > 0, "Scheduler should have executed tasks");
    assert!(
        bc >= te / 2,
        "Bus events significant: {bc} vs {te} expected"
    );
    eprintln!("No starvation: bus={bc} sched={sc} expected={te}");
}
