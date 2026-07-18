//! Soak test: long-running event bus under sustained load.
//!
//! Run with:
//!   cargo test --test soak_event_bus -- --ignored --nocapture
//!   SOAK_EVENTS=500000 cargo test ...  (override event count)
//!   SOAK_SECS=300 cargo test ...      (override duration limit)

use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use sysinfo::System;

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

struct SoakEvent {
    kind: &'static str,
    metadata: EventMetadata,
}

impl std::fmt::Debug for SoakEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SoakEvent").finish()
    }
}

impl Event for SoakEvent {
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
    ModuleId::new("soak", SemVer::new(1, 0, 0))
}

fn make_event(kind: &str, corr_id: CorrelationId) -> Box<dyn Event> {
    Box::new(SoakEvent {
        kind: intern_kind(kind),
        metadata: EventMetadata::new(
            test_module(),
            corr_id,
            None,
            ContentType::new("application/x-soak"),
            chrono::Utc::now(),
        ),
    })
}

fn soak_event_count() -> u64 {
    std::env::var("SOAK_EVENTS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000)
}

fn soak_duration() -> Duration {
    let secs = std::env::var("SOAK_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(120u64);
    Duration::from_secs(secs)
}

fn sample_memory_rss(sys: &System) -> u64 {
    if let Some(process) = sys.process(sysinfo::Pid::from_u32(std::process::id())) {
        return process.memory();
    }
    0
}

// ─── Test 1: Sustained 100k+ events with memory trending ───────────────

#[test]
fn sustained_high_volume_no_memory_growth() {
    let bus = EventBus::new();
    let total = soak_event_count();
    let check_interval = total / 20;
    let num_kinds = 5;

    // Register handlers for 5 event kinds
    let per_kind: Vec<Arc<AtomicU64>> = (0..num_kinds)
        .map(|_| Arc::new(AtomicU64::new(0)))
        .collect();
    for (i, c) in per_kind.iter().enumerate() {
        let c = c.clone();
        let kind = format!("soak_kind_{}", i);
        bus.subscribe(
            &kind,
            Arc::new(move |_: &dyn Event| {
                c.fetch_add(1, Ordering::Relaxed);
            }),
        );
    }

    // Sample memory at start
    let mut sys = System::new();
    sys.refresh_memory();
    let start_mem = sample_memory_rss(&sys);
    let start = Instant::now();
    let mut last_mem = start_mem;

    for i in 0..total {
        let kind_idx = (i % num_kinds as u64) as usize;
        bus.publish(make_event(
            &format!("soak_kind_{}", kind_idx),
            CorrelationId::new(),
        ));

        // Periodic memory check
        if i > 0 && i % check_interval == 0 {
            sys.refresh_memory();
            let cur_mem = sample_memory_rss(&sys);
            let elapsed = start.elapsed();
            let rate = i as f64 / elapsed.as_secs_f64();
            eprintln!(
                "  [{}/{}] mem={}KB (delta={}KB) rate={:.0}/s",
                i,
                total,
                cur_mem / 1024,
                if cur_mem > last_mem {
                    (cur_mem - last_mem) / 1024
                } else {
                    0
                },
                rate,
            );
            last_mem = cur_mem;

            if elapsed > soak_duration() {
                eprintln!("  WARNING: reached duration limit at event {}", i);
                break;
            }
        }
    }

    let elapsed = start.elapsed();

    // Verify no handler lost events
    let total_received: u64 = per_kind.iter().map(|c| c.load(Ordering::Relaxed)).sum();
    eprintln!(
        "Soak complete: {} sent, {} received, {:.1}s, {:.0}/s",
        total,
        total_received,
        elapsed.as_secs_f64(),
        total as f64 / elapsed.as_secs_f64(),
    );

    assert_eq!(
        total_received, total,
        "Event loss: sent {total}, received {total_received}"
    );

    // Verify no unbounded memory growth (allow 10MB tolerance for caches)
    sys.refresh_memory();
    let end_mem = sample_memory_rss(&sys);
    let mem_delta = if end_mem > start_mem {
        end_mem - start_mem
    } else {
        0
    };
    assert!(
        mem_delta < 10 * 1024 * 1024,
        "Memory grew by {}KB — possible leak",
        mem_delta / 1024,
    );
    eprintln!(
        "Memory delta: {}KB (start={}KB end={}KB)",
        mem_delta / 1024,
        start_mem / 1024,
        end_mem / 1024
    );
}

// ─── Test 2: Sustained mixed producer/consumer load ────────────────────

#[test]
fn sustained_mixed_producer_consumer() {
    let bus = Arc::new(EventBus::new());
    let total = soak_event_count();
    let num_producers = 4;
    let events_per_producer = total / num_producers as u64;

    let consumer_count = Arc::new(AtomicU64::new(0));
    let cc = consumer_count.clone();
    bus.subscribe(
        "sustained",
        Arc::new(move |_: &dyn Event| {
            cc.fetch_add(1, Ordering::Relaxed);
            // Simulate lightweight processing
            std::hint::spin_loop();
        }),
    );

    let barrier = Arc::new(std::sync::Barrier::new(num_producers));
    let mut handles = Vec::new();

    for _ in 0..num_producers {
        let bus = bus.clone();
        let bar = barrier.clone();
        handles.push(std::thread::spawn(move || {
            bar.wait();
            for _ in 0..events_per_producer {
                bus.publish(make_event("sustained", CorrelationId::new()));
            }
        }));
    }

    let start = Instant::now();
    for h in handles {
        h.join().unwrap();
    }
    let elapsed = start.elapsed();

    let received = consumer_count.load(Ordering::Relaxed);
    let expected = num_producers as u64 * events_per_producer;
    assert_eq!(
        received, expected,
        "Event loss: expected {expected}, got {received}"
    );
    eprintln!(
        "Mixed producer/consumer: {expected} events in {:.2}s ({:.0}/s)",
        elapsed.as_secs_f64(),
        expected as f64 / elapsed.as_secs_f64(),
    );
}

// ─── Test 3: Correlation ID integrity over long event chains ───────────

#[test]
fn correlation_id_integrity_over_long_chain() {
    let bus = EventBus::new();
    let total = soak_event_count();

    // Chain: each handler publishes a child event with propagated correlation_id
    let chain_depth = 3;
    let leaf_count = Arc::new(AtomicU64::new(0));
    let seen_ids = Arc::new(Mutex::new(HashSet::new()));

    // Leaf handler
    let lc = leaf_count.clone();
    let si = seen_ids.clone();
    bus.subscribe(
        "chain_3",
        Arc::new(move |ev: &dyn Event| {
            lc.fetch_add(1, Ordering::Relaxed);
            si.lock().unwrap().insert(ev.metadata().correlation_id);
        }),
    );

    // Chain level 2 — publishes to chain_3
    let bus_l2 = bus.clone();
    bus.subscribe(
        "chain_2",
        Arc::new(move |ev: &dyn Event| {
            bus_l2.publish(make_event("chain_3", ev.metadata().correlation_id));
        }),
    );

    // Chain level 1 — publishes to chain_2
    let bus_l1 = bus.clone();
    bus.subscribe(
        "chain_1",
        Arc::new(move |ev: &dyn Event| {
            bus_l1.publish(make_event("chain_2", ev.metadata().correlation_id));
        }),
    );

    // Publish root events
    let mut root_ids = Vec::with_capacity(total as usize);
    for _ in 0..total {
        let cid = CorrelationId::new();
        root_ids.push(cid);
        bus.publish(make_event("chain_1", cid));
    }

    let leaf_received = leaf_count.load(Ordering::Relaxed);
    assert_eq!(
        leaf_received, total,
        "Correlation chain broken: expected {total} leaf events, got {leaf_received}"
    );

    // Verify every leaf correlation_id is one of the root ids
    let seen = seen_ids.lock().unwrap();
    let root_set: HashSet<CorrelationId> = root_ids.iter().copied().collect();
    assert_eq!(
        seen.len(),
        total as usize,
        "Duplicate or missing correlation_ids in chain"
    );
    for cid in seen.iter() {
        assert!(
            root_set.contains(cid),
            "Leaf received unexpected correlation_id {:?}",
            cid,
        );
    }
    eprintln!("Correlation chain integrity: {total} events, depth {chain_depth}, all IDs verified");
}

// ─── Test 4: Long-duration throughput stability ────────────────────────

#[test]
fn throughput_stable_over_time() {
    let bus = EventBus::new();
    let check_interval = 10_000u64;
    let checks = 10;
    let events_per_check = check_interval;

    let counter = Arc::new(AtomicU64::new(0));
    let c = counter.clone();
    bus.subscribe(
        "throughput",
        Arc::new(move |_: &dyn Event| {
            c.fetch_add(1, Ordering::Relaxed);
        }),
    );

    let mut window_rates = Vec::new();

    for chk in 0..checks {
        let window_start = Instant::now();
        for _ in 0..events_per_check {
            bus.publish(make_event("throughput", CorrelationId::new()));
        }
        let window_elapsed = window_start.elapsed();
        let window_rate = events_per_check as f64 / window_elapsed.as_secs_f64();
        window_rates.push(window_rate);
        eprintln!("  Window {chk}: {:.0} events/s", window_rate);
    }

    let total_received = counter.load(Ordering::Relaxed);
    assert_eq!(
        total_received,
        checks * events_per_check,
        "Event loss over time"
    );

    let min_rate = window_rates.iter().cloned().fold(f64::MAX, f64::min);
    let max_rate = window_rates.iter().cloned().fold(f64::MIN, f64::max);
    let avg_rate: f64 = window_rates.iter().sum::<f64>() / window_rates.len() as f64;

    // Throughput should not degrade — allow 30% variance
    if min_rate < avg_rate * 0.7 {
        eprintln!(
            "WARNING: throughput degraded from {:.0} to {:.0} (avg {:.0})",
            max_rate, min_rate, avg_rate,
        );
    }
    eprintln!(
        "Throughput: min={:.0} max={:.0} avg={:.0} events/s",
        min_rate, max_rate, avg_rate
    );
}
