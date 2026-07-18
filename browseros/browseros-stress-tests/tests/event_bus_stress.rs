use std::any::Any;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier, Mutex, OnceLock};
use std::time::Instant;

use browseros_event_bus::EventBus;
use browseros_types::event::{Event, EventCategory, EventMetadata};
use browseros_types::identifiers::{CorrelationId, ModuleId};
use browseros_types::value::{ContentType, SemVer};

fn intern_kind(kind: &str) -> &'static str {
    static CACHE: OnceLock<Mutex<HashMap<String, &'static str>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = cache.lock().unwrap();
    if let Some(k) = cache.get(kind) {
        return k;
    }
    let leaked: &'static str = Box::leak(kind.to_owned().into_boxed_str());
    cache.insert(kind.to_owned(), leaked);
    leaked
}

struct StressEvent {
    kind: &'static str,
    metadata: EventMetadata,
}

impl std::fmt::Debug for StressEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StressEvent").finish()
    }
}

impl Event for StressEvent {
    fn kind(&self) -> &'static str {
        self.kind
    }
    fn category(&self) -> EventCategory {
        EventCategory::Domain
    }
    fn metadata(&self) -> &EventMetadata {
        &self.metadata
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

fn test_module() -> ModuleId {
    ModuleId::new("stress", SemVer::new(1, 0, 0))
}

fn make_event(kind: &str, corr_id: CorrelationId) -> Box<dyn Event> {
    Box::new(StressEvent {
        kind: intern_kind(kind),
        metadata: EventMetadata::new(
            test_module(),
            corr_id,
            None,
            ContentType::new("application/x-stress"),
            chrono::Utc::now(),
        ),
    })
}

/// 1a. 10k events/sec burst simulation with synchronous handlers.
#[test]
fn event_bus_10k_burst_no_event_loss() {
    let bus = EventBus::new();
    let total_published = 10_000u64;
    let num_subscribers = 4;
    let counters: Vec<Arc<AtomicU64>> = (0..num_subscribers)
        .map(|_| Arc::new(AtomicU64::new(0)))
        .collect();

    // Register N subscribers for the same event kind
    for c in &counters {
        let c = c.clone();
        bus.subscribe(
            "burst",
            Arc::new(move |_: &dyn Event| {
                c.fetch_add(1, Ordering::Relaxed);
            }),
        );
    }

    let start = Instant::now();
    for _ in 0..total_published {
        bus.publish(make_event("burst", CorrelationId::new()));
    }
    let elapsed = start.elapsed();
    let rate = total_published as f64 / elapsed.as_secs_f64();

    // Verify no event loss — each subscriber should have received all events
    for (i, c) in counters.iter().enumerate() {
        assert_eq!(
            c.load(Ordering::Relaxed),
            total_published,
            "Subscriber {i} lost events"
        );
    }

    // Rate assertion: warn if below 10k/s but don't fail (depends on CI)
    if rate < 10_000.0 {
        eprintln!(
            "WARNING: throughput {:.0} events/sec is below 10k target",
            rate
        );
    }
    eprintln!("OK: burst throughput {:.0} events/sec", rate);
}

/// 1b. 8 concurrent publishers, 4 subscribers, verify total event count.
#[test]
fn concurrent_publishers_no_event_loss() {
    let bus = Arc::new(EventBus::new());
    let num_publishers = 8;
    let events_per_publisher = 1_250u64; // 10k total
    let total_expected = num_publishers as u64 * events_per_publisher;
    let subscriber_count = Arc::new(AtomicU64::new(0));

    let sc = subscriber_count.clone();
    bus.subscribe(
        "concurrent_burst",
        Arc::new(move |_: &dyn Event| {
            sc.fetch_add(1, Ordering::Relaxed);
        }),
    );

    let barrier = Arc::new(Barrier::new(num_publishers));
    let mut handles = Vec::new();

    for _ in 0..num_publishers {
        let bus = bus.clone();
        let bar = barrier.clone();
        handles.push(std::thread::spawn(move || {
            bar.wait(); // synchronize start
            for _ in 0..events_per_publisher {
                bus.publish(make_event("concurrent_burst", CorrelationId::new()));
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    assert_eq!(
        subscriber_count.load(Ordering::Relaxed),
        total_expected,
        "Event loss detected under concurrent publishing"
    );
}

/// 1c. Correlation ID integrity — no event with wrong correlation_id passes.
#[test]
fn correlation_id_integrity_preserved() {
    let bus = EventBus::new();
    let total_events = 5_000;
    let expected_ids = Arc::new(Mutex::new(Vec::new()));
    let received_ids = Arc::new(Mutex::new(Vec::new()));

    let ei = expected_ids.clone();
    let ri = received_ids.clone();
    bus.subscribe(
        "corr_test",
        Arc::new(move |ev: &dyn Event| {
            ri.lock().unwrap().push(ev.metadata().correlation_id);
        }),
    );

    let mut ids = Vec::new();
    for _ in 0..total_events {
        let cid = CorrelationId::new();
        ei.lock().unwrap().push(cid);
        ids.push(cid);
    }

    // Publish in order
    for cid in &ids {
        bus.publish(make_event("corr_test", *cid));
    }

    let received = received_ids.lock().unwrap();
    assert_eq!(received.len(), total_events, "Not all events were received");

    // Every received correlation_id must be in the expected set
    let expected_set: std::collections::HashSet<_> =
        expected_ids.lock().unwrap().iter().copied().collect();
    for cid in received.iter() {
        assert!(
            expected_set.contains(cid),
            "Received unexpected correlation_id"
        );
    }
}

/// 1d. Unsubscribe under concurrent publish does not cause deadlock.
#[test]
fn concurrent_publish_and_unsubscribe_no_deadlock() {
    let bus = Arc::new(EventBus::new());
    let handle_count = 20;
    let mut handles = Vec::new();

    for i in 0..handle_count {
        let bus = bus.clone();
        let h = bus.subscribe(
            &format!("kind_{}", i % 5),
            Arc::new(move |_: &dyn Event| {
                std::thread::sleep(std::time::Duration::from_micros(5));
            }),
        );
        handles.push((i, h));
    }

    let mut threads = Vec::new();
    let bus_pub = bus.clone();
    threads.push(std::thread::spawn(move || {
        for _ in 0..500 {
            bus_pub.publish(make_event("kind_0", CorrelationId::new()));
        }
    }));

    for (i, h) in &handles {
        let bus = bus.clone();
        let h = h.clone();
        threads.push(std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_micros(10));
            bus.unsubscribe(&h);
        }));
    }

    for t in threads {
        t.join().unwrap();
    }
}

/// 1e. Multiple event kinds — verify handlers only receive their kind.
#[test]
fn multiple_event_kinds_deliver_to_correct_handlers() {
    let bus = EventBus::new();
    let kinds = 5;
    let events_per_kind = 2_000;
    let counters: Vec<Arc<AtomicU64>> = (0..kinds).map(|_| Arc::new(AtomicU64::new(0))).collect();

    for (i, c) in counters.iter().enumerate() {
        let c = c.clone();
        let kind = format!("group_{}", i);
        bus.subscribe(
            &kind,
            Arc::new(move |_: &dyn Event| {
                c.fetch_add(1, Ordering::Relaxed);
            }),
        );
    }

    for i in 0..kinds {
        for _ in 0..events_per_kind {
            bus.publish(make_event(&format!("group_{}", i), CorrelationId::new()));
        }
    }

    for (i, c) in counters.iter().enumerate() {
        assert_eq!(
            c.load(Ordering::Relaxed),
            events_per_kind,
            "Handler for group_{i} received wrong count"
        );
    }
}

/// 1f. Subscribe while publishing — no deadlock (RwLock contention stress).
#[test]
fn concurrent_subscribe_and_publish_no_deadlock() {
    let bus = Arc::new(EventBus::new());
    let mut threads = Vec::new();

    for i in 0..4 {
        let bus = bus.clone();
        threads.push(std::thread::spawn(move || {
            for _ in 0..100 {
                bus.subscribe(
                    &format!("dyn_{}", i),
                    Arc::new(move |_: &dyn Event| {
                        std::thread::sleep(std::time::Duration::from_micros(1));
                    }),
                );
            }
        }));
    }

    for i in 0..4 {
        let bus = bus.clone();
        threads.push(std::thread::spawn(move || {
            for _ in 0..500 {
                bus.publish(make_event(&format!("dyn_{}", i % 4), CorrelationId::new()));
            }
        }));
    }

    for t in threads {
        t.join().unwrap();
    }
}
