//! Soak test: degraded environment simulation.
//!
//! Run with:
//!   cargo test --test soak_degraded_env -- --ignored --nocapture
//!   SOAK_SECS=60 cargo test ...      (override duration)

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use browseros_event_bus::EventBus;
use browseros_scheduler::Scheduler;
use browseros_types::event::{Event, EventCategory, EventMetadata};
use browseros_types::identifiers::{CorrelationId, ModuleId};
use browseros_types::value::{ContentType, SemVer};

fn soak_duration() -> Duration {
    std::env::var("SOAK_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .map(Duration::from_secs)
        .unwrap_or(Duration::from_secs(60))
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
            ModuleId::new("degraded", SemVer::new(1, 0, 0)),
            correlation_id,
            None,
            ContentType::new("app/json"),
            chrono::Utc::now(),
        ),
    })
}

// ─── Test 1: Slow event handlers — system does not collapse ────────────

#[test]
fn slow_handlers_graceful_degradation() {
    let bus = EventBus::new();
    let fast_count = Arc::new(AtomicU64::new(0));
    let slow_count = Arc::new(AtomicU64::new(0));

    let fc = fast_count.clone();
    bus.subscribe(
        "fast",
        Arc::new(move |_: &dyn Event| {
            fc.fetch_add(1, Ordering::Relaxed);
        }),
    );

    let sc = slow_count.clone();
    bus.subscribe(
        "slow",
        Arc::new(move |_: &dyn Event| {
            std::thread::sleep(Duration::from_millis(5));
            sc.fetch_add(1, Ordering::Relaxed);
        }),
    );

    let duration = soak_duration().min(Duration::from_secs(30));
    let running = Arc::new(AtomicBool::new(true));
    let fast_published = Arc::new(AtomicU64::new(0));
    let slow_published = Arc::new(AtomicU64::new(0));

    // Fast publisher
    let r = running.clone();
    let b = bus.clone();
    let fp = fast_published.clone();
    let fast_thread = std::thread::spawn(move || {
        while r.load(Ordering::Relaxed) {
            b.publish(make_event("fast", CorrelationId::new()));
            fp.fetch_add(1, Ordering::Relaxed);
            std::thread::yield_now();
        }
    });

    // Slow publisher
    let r = running.clone();
    let b = bus.clone();
    let sp = slow_published.clone();
    let slow_thread = std::thread::spawn(move || {
        while r.load(Ordering::Relaxed) {
            b.publish(make_event("slow", CorrelationId::new()));
            sp.fetch_add(1, Ordering::Relaxed);
            std::thread::sleep(Duration::from_micros(500));
        }
    });

    std::thread::sleep(duration);
    running.store(false, Ordering::Relaxed);
    std::thread::sleep(Duration::from_millis(200));

    let _ = fast_thread.join();
    let _ = slow_thread.join();

    let fp = fast_published.load(Ordering::Relaxed);
    let sp = slow_published.load(Ordering::Relaxed);
    let fr = fast_count.load(Ordering::Relaxed);
    let sr = slow_count.load(Ordering::Relaxed);

    // Fast events should not be blocked by slow handlers
    assert!(fr > 0, "Fast handlers should have processed events");
    assert_eq!(fr, fp, "Fast events should not be lost");
    assert_eq!(sr, sp, "Slow events should not be lost");
    // Fast throughput should be significantly higher than slow
    assert!(
        fr > sr * 10,
        "Fast events ({fr}) should dominate slow ({sr}) — graceful degradation"
    );

    eprintln!(
        "Degradation: fast={fr}/{fp} slow={sr}/{sp} — ratio {}:1",
        fr as f64 / sr.max(1) as f64,
    );
}

// ─── Test 2: Delayed scheduler execution under load ────────────────────

#[test]
fn delayed_scheduler_under_load() {
    let bus = Arc::new(EventBus::new());
    let sched = Scheduler::new(bus.clone());
    let exec_count = Arc::new(AtomicU64::new(0));

    let duration = soak_duration().min(Duration::from_secs(30));
    let running = Arc::new(AtomicBool::new(true));
    let scheduled = Arc::new(AtomicU64::new(0));

    // Saturate the event bus
    let r = running.clone();
    let b = bus.clone();
    let saturator = std::thread::spawn(move || {
        while r.load(Ordering::Relaxed) {
            b.publish(make_event("saturate", CorrelationId::new()));
            std::thread::yield_now();
        }
    });

    // Schedule delayed tasks under load
    let r = running.clone();
    let sc = sched.clone();
    let ec = exec_count.clone();
    let sd = scheduled.clone();
    let scheduler_thread = std::thread::spawn(move || {
        while r.load(Ordering::Relaxed) {
            let c = ec.clone();
            sc.schedule_after(Duration::from_millis(5), move || {
                c.fetch_add(1, Ordering::Relaxed);
            });
            sd.fetch_add(1, Ordering::Relaxed);
            std::thread::sleep(Duration::from_micros(200));
        }
    });

    std::thread::sleep(duration);
    running.store(false, Ordering::Relaxed);
    std::thread::sleep(Duration::from_millis(500));

    let _ = saturator.join();
    let _ = scheduler_thread.join();

    let exec = exec_count.load(Ordering::Relaxed);
    let sched_count = scheduled.load(Ordering::Relaxed);

    // Scheduler should still make progress under saturation
    assert!(
        exec > 0,
        "Delayed tasks should execute even under bus saturation"
    );
    assert!(
        exec >= sched_count / 2,
        "Most scheduled tasks should execute: {exec}/{sched_count}"
    );
    eprintln!("Scheduler under load: {exec}/{sched_count} tasks executed");
}

// ─── Test 3: High contention subscribers ───────────────────────────────

#[test]
fn high_contention_subscribers_graceful() {
    let bus = EventBus::new();
    let num_subscribers = 50;
    let per_subscriber_count: Vec<Arc<AtomicU64>> = (0..num_subscribers)
        .map(|_| Arc::new(AtomicU64::new(0)))
        .collect();

    for (i, c) in per_subscriber_count.iter().enumerate() {
        let c = c.clone();
        bus.subscribe(
            &format!("contend_{}", i % 5),
            Arc::new(move |_: &dyn Event| {
                std::thread::sleep(Duration::from_micros(10 + (i as u64 % 50)));
                c.fetch_add(1, Ordering::Relaxed);
            }),
        );
    }

    let events_per_kind = 5_000u64;
    let num_kinds = 5;
    let start = Instant::now();

    for k in 0..num_kinds {
        for _ in 0..events_per_kind {
            bus.publish(make_event(&format!("contend_{}", k), CorrelationId::new()));
        }
    }

    let elapsed = start.elapsed();

    // Each handler for kind N has ~num_subscribers/5 = 10 subscribers
    for (i, c) in per_subscriber_count.iter().enumerate() {
        let expected = events_per_kind;
        let actual = c.load(Ordering::Relaxed);
        if actual != expected {
            // Some handlers may not have been addressed if contention blocks,
            // but every handler of a published kind MUST receive all events of that kind
            let kind_idx = i % 5;
            let kind_events = events_per_kind;
            assert!(
                actual == kind_events,
                "Subscriber {i} (kind {kind_idx}): got {actual}, expected {kind_events}",
            );
        }
    }

    eprintln!(
        "High contention: {num_subscribers} subs, {} total events in {:.2}s",
        num_kinds as u64 * events_per_kind,
        elapsed.as_secs_f64(),
    );
}
