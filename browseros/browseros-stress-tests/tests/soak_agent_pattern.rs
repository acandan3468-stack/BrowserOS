//! Soak test: real agent pattern simulation.
//!
//! Simulates browser-like event bursts, DOM-like hierarchical events,
//! and correlated event chains.
//!
//! Run with:
//!   cargo test --test soak_agent_pattern -- --ignored --nocapture
//!   SOAK_SECS=60 cargo test ...      (override duration)

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use browseros_event_bus::EventBus;
use browseros_lifecycle::LifecycleManager;
use browseros_observability::{LevelFilter, LogLevel, Logger, StdoutSink, Tracer};
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
            ModuleId::new("agent-soak", SemVer::new(1, 0, 0)),
            correlation_id,
            None,
            ContentType::new("app/json"),
            chrono::Utc::now(),
        ),
    })
}

// ─── Test 1: Browser-like event bursts ─────────────────────────────────

#[test]
fn browser_like_event_bursts() {
    let bus = EventBus::new();
    let total_bursts = 20;
    let events_per_burst = 5_000;

    let received = Arc::new(AtomicU64::new(0));
    let seen_kinds = Arc::new(Mutex::new(HashSet::new()));

    // Simulate browser event types
    let browser_kinds = [
        "dom.click",
        "dom.input",
        "dom.scroll",
        "network.fetch",
        "network.response",
        "storage.set",
        "storage.get",
        "timer.timeout",
        "timer.interval",
        "user.navigate",
    ];

    for kind in &browser_kinds {
        let r = received.clone();
        let sk = seen_kinds.clone();
        let k = kind.to_string();
        bus.subscribe(
            kind,
            Arc::new(move |_: &dyn Event| {
                r.fetch_add(1, Ordering::Relaxed);
                sk.lock().unwrap().insert(k.clone());
            }),
        );
    }

    let start = Instant::now();

    for burst in 0..total_bursts {
        // Each burst simulates a user interaction followed by system reactions
        let root_kind = browser_kinds[burst % browser_kinds.len()];
        bus.publish(make_event(root_kind, CorrelationId::new()));

        // Reactions triggered by the root event
        for i in 0..events_per_burst {
            let kind = browser_kinds[(burst + i + 1) % browser_kinds.len()];
            bus.publish(make_event(kind, CorrelationId::new()));
        }

        if burst > 0 && burst % 5 == 0 {
            eprintln!(
                "  Burst {burst}/{}: {} events",
                total_bursts,
                burst * events_per_burst
            );
        }
    }

    let elapsed = start.elapsed();
    let total = total_bursts as u64 * (1 + events_per_burst as u64);
    let rcvd = received.load(Ordering::Relaxed);

    assert_eq!(
        rcvd, total,
        "Event loss in browser bursts: got {rcvd}, expected {total}"
    );
    assert_eq!(
        seen_kinds.lock().unwrap().len(),
        browser_kinds.len(),
        "All browser event kinds should have been seen",
    );

    eprintln!(
        "Browser bursts: {total} events across {total_bursts} bursts in {:.2}s ({:.0}/s)",
        elapsed.as_secs_f64(),
        total as f64 / elapsed.as_secs_f64(),
    );
}

// ─── Test 2: DOM-like hierarchical events ──────────────────────────────

#[test]
fn dom_hierarchical_event_propagation() {
    let bus = EventBus::new();

    // DOM hierarchy: document → body → div → span
    let dom_depth = 4;
    let capture_count = Arc::new(AtomicU64::new(0));
    let bubble_count = Arc::new(AtomicU64::new(0));

    // Capture phase (document catches all)
    let cc = capture_count.clone();
    bus.subscribe(
        "dom.document",
        Arc::new(move |_: &dyn Event| {
            cc.fetch_add(1, Ordering::Relaxed);
        }),
    );

    // Bubble phase (each level)
    let bc = bubble_count.clone();
    bus.subscribe(
        "dom.span",
        Arc::new(move |_: &dyn Event| {
            bc.fetch_add(1, Ordering::Relaxed);
        }),
    );

    let depth_labels = ["dom.document", "dom.body", "dom.div", "dom.span"];
    let num_iterations = 10_000;
    let start = Instant::now();

    for i in 0..num_iterations {
        // Simulate DOM event propagation: capture → target → bubble
        for depth in 0..dom_depth {
            bus.publish(make_event(depth_labels[depth], CorrelationId::new()));
        }
        if i > 0 && i % 2_000 == 0 {
            eprintln!("  DOM iteration {i}/{}", num_iterations);
        }
    }

    let elapsed = start.elapsed();
    let total_events = (num_iterations * dom_depth) as u64;
    let captured = capture_count.load(Ordering::Relaxed);
    let bubbled = bubble_count.load(Ordering::Relaxed);

    // Each subscriber receives only events published to its exact kind
    assert_eq!(
        captured, num_iterations as u64,
        "Document receives only dom.document events"
    );
    assert_eq!(
        bubbled, num_iterations as u64,
        "Span receives only dom.span events"
    );

    eprintln!(
        "DOM hierarchy: {total_events} events across {num_iterations} iterations in {:.2}s",
        elapsed.as_secs_f64(),
    );
}

// ─── Test 3: Correlated event chains (parent → child → child) ──────────

#[test]
fn correlated_event_chains_perfect_propagation() {
    let bus = EventBus::new();
    let chain_length = 5;

    // Build a chain: ev_0 → ev_1 → ... → ev_{N-1}
    // Each handler publishes the next event with the same correlation_id
    let leaf_count = Arc::new(AtomicU64::new(0));
    let seen_ids = Arc::new(Mutex::new(HashSet::new()));
    // Leaf (last in chain)
    let lc = leaf_count.clone();
    let si = seen_ids.clone();
    let last_kind = format!("chain_ev_{}", chain_length - 1);
    bus.subscribe(
        &last_kind,
        Arc::new(move |ev: &dyn Event| {
            lc.fetch_add(1, Ordering::Relaxed);
            si.lock().unwrap().insert(ev.metadata().correlation_id);
        }),
    );

    // Intermediate links: each publishes the next link in the chain
    for depth in (0..chain_length - 1).rev() {
        let kind = format!("chain_ev_{}", depth);
        let next_kind = format!("chain_ev_{}", depth + 1);
        let b = bus.clone();
        bus.subscribe(
            &kind,
            Arc::new(move |ev: &dyn Event| {
                // Propagate the same correlation_id
                b.publish(make_event(&next_kind, ev.metadata().correlation_id));
            }),
        );
    }

    let total_chains = 2000;
    let mut root_ids = Vec::with_capacity(total_chains);

    let start = Instant::now();
    for _ in 0..total_chains {
        let cid = CorrelationId::new();
        root_ids.push(cid);
        bus.publish(make_event("chain_ev_0", cid));
    }
    let elapsed = start.elapsed();

    let leaf_received = leaf_count.load(Ordering::Relaxed);
    assert_eq!(
        leaf_received, total_chains as u64,
        "Chain broken: expected {total_chains} leaf events, got {leaf_received}",
    );

    // Every leaf correlation_id must be a root correlation_id
    let seen = seen_ids.lock().unwrap();
    let root_set: HashSet<CorrelationId> = root_ids.iter().copied().collect();
    assert_eq!(
        seen.len(),
        total_chains,
        "Duplicate or missing correlation_ids"
    );
    for cid in seen.iter() {
        assert!(root_set.contains(cid), "Unknown correlation_id in leaf");
    }

    eprintln!(
        "Event chains: {total_chains} chains, depth {chain_length}, all correlation_ids perfect, {:.2}s",
        elapsed.as_secs_f64(),
    );
}

// ─── Test 4: Full agent pattern (browser + lifecycle + scheduler) ──────

#[test]
fn full_agent_pattern_simulation() {
    let bus = Arc::new(EventBus::new());
    let logger = Arc::new(Logger::new(
        Arc::new(StdoutSink::new()),
        Arc::new(LevelFilter::new(LogLevel::Warn)),
        "agent",
    ));
    let tracer = Arc::new(Tracer::new());
    let scheduler = Arc::new(Scheduler::new(bus.clone()));
    let lifecycle = Arc::new(LifecycleManager::new(
        bus.clone(),
        logger.clone(),
        ModuleId::new("agent", SemVer::new(1, 0, 0)),
    ));

    let duration = soak_duration().min(Duration::from_secs(45));
    let running = Arc::new(AtomicBool::new(true));

    // Publisher: emits browser-like event bursts
    let browser_kinds = [
        "agent.click",
        "agent.input",
        "agent.scroll",
        "agent.fetch",
        "agent.parse",
        "agent.render",
    ];
    let total_published = Arc::new(AtomicU64::new(0));
    let r = running.clone();
    let b = bus.clone();
    let tp = total_published.clone();
    let pub_thread = std::thread::spawn(move || {
        while r.load(Ordering::Relaxed) {
            let kind =
                browser_kinds[Instant::now().elapsed().as_secs() as usize % browser_kinds.len()];
            b.publish(make_event(kind, CorrelationId::new()));
            tp.fetch_add(1, Ordering::Relaxed);
            std::thread::sleep(Duration::from_micros(100));
        }
    });

    // Lifecycle transitions
    let lc = lifecycle.clone();
    let r = running.clone();
    let lifecycle_thread = std::thread::spawn(move || {
        let mut idx = 0u64;
        while r.load(Ordering::Relaxed) {
            let name = format!("agent_comp_{}", idx % 5);
            lc.register_component(&name);
            let _ = lc.transition_to(&name, browseros_lifecycle::LifecycleState::Initializing);
            let _ = lc.transition_to(&name, browseros_lifecycle::LifecycleState::Running);
            let _ = lc.transition_to(&name, browseros_lifecycle::LifecycleState::Stopping);
            let _ = lc.transition_to(&name, browseros_lifecycle::LifecycleState::Stopped);
            idx += 1;
            std::thread::sleep(Duration::from_millis(10));
        }
    });

    // Scheduler: simulates agent timers and delayed reactions
    let sc = scheduler.clone();
    let r = running.clone();
    let sched_thread = std::thread::spawn(move || {
        while r.load(Ordering::Relaxed) {
            sc.schedule_after(Duration::from_millis(5), move || {});
            sc.schedule_after(Duration::from_millis(20), move || {});
            std::thread::sleep(Duration::from_millis(30));
        }
    });

    // Tracer: instrument everything
    let tr = tracer.clone();
    let r = running.clone();
    let trace_thread = std::thread::spawn(move || {
        while r.load(Ordering::Relaxed) {
            let mut span = tr.start_span("agent_cycle");
            span.set_field("phase", "soak".into());
            span.finish();
            std::thread::sleep(Duration::from_millis(50));
        }
    });

    std::thread::sleep(duration);
    running.store(false, Ordering::Relaxed);
    std::thread::sleep(Duration::from_millis(500));

    let _ = pub_thread.join();
    let _ = lifecycle_thread.join();
    let _ = sched_thread.join();
    let _ = trace_thread.join();

    let published = total_published.load(Ordering::Relaxed);
    assert!(published > 0, "Agent pattern should have published events");
    eprintln!(
        "Full agent pattern: {} events in {:.1}s",
        published,
        duration.as_secs_f64(),
    );
}
