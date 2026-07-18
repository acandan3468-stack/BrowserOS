use std::sync::{Arc, Mutex};

use browseros_observability::{
    HistogramTimer, LevelFilter, LogLevel, Logger, MetricsRegistry, StdoutSink, Tracer,
};

fn test_logger() -> Arc<Logger> {
    Arc::new(Logger::new(
        Arc::new(StdoutSink::new()),
        Arc::new(LevelFilter::new(LogLevel::Trace)),
        "integrity",
    ))
}

/// 5a. Logger correlation_id preservation under concurrency.
#[test]
fn logger_concurrent_no_panic() {
    let logger = test_logger();
    let num_threads = 10;
    let msgs_per_thread = 500;

    let mut handles = Vec::new();
    for t in 0..num_threads {
        let logger = logger.clone();
        handles.push(std::thread::spawn(move || {
            for i in 0..msgs_per_thread {
                let msg = format!("thread_{t}_msg_{i}");
                logger.info(msg);
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }
}

/// 5b. Metrics registry: concurrent counter increments must be exact.
#[test]
fn metrics_counter_concurrent_exact() {
    let reg = MetricsRegistry::new();
    let counter = reg.counter("concurrent_counter");
    let num_threads = 10;
    let increments_per_thread = 1_000;
    let expected = (num_threads * increments_per_thread) as u64;

    let mut handles = Vec::new();
    for _ in 0..num_threads {
        let c = counter.clone();
        handles.push(std::thread::spawn(move || {
            for _ in 0..increments_per_thread {
                c.increment();
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    assert_eq!(
        counter.value(),
        expected,
        "Counter value {}. Expected {}",
        counter.value(),
        expected
    );
}

/// 5c. Metrics registry: concurrent gauge set/get — no panic, no corruption.
#[test]
fn metrics_gauge_concurrent() {
    let reg = MetricsRegistry::new();
    let gauge = reg.gauge("concurrent_gauge");
    let num_threads = 10;

    let mut handles = Vec::new();
    for t in 0..num_threads {
        let g = gauge.clone();
        handles.push(std::thread::spawn(move || {
            g.set(t as f64 * 10.0);
            let _ = g.value();
        }));
    }

    for h in handles {
        h.join().unwrap();
    }
}

/// 5d. Metrics registry: concurrent histogram observations.
#[test]
fn metrics_histogram_concurrent() {
    let reg = MetricsRegistry::new();
    let hist = reg.histogram("concurrent_hist");
    let num_threads = 10;
    let obs_per_thread = 100;
    let expected = (num_threads * obs_per_thread) as u64;

    let mut handles = Vec::new();
    for _ in 0..num_threads {
        let h = hist.clone();
        handles.push(std::thread::spawn(move || {
            for j in 0..obs_per_thread {
                h.observe(j as f64 * 0.5);
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    assert_eq!(hist.count(), expected);
}

/// 5e. Tracer: concurrent span creation does not corrupt.
#[test]
fn tracer_concurrent_spans() {
    let tracer = Arc::new(Tracer::new());
    let num_threads = 10;
    let spans_per_thread = 50;

    let mut handles = Vec::new();
    for _t in 0..num_threads {
        let tr = tracer.clone();
        handles.push(std::thread::spawn(move || {
            for _i in 0..spans_per_thread {
                let mut span = tr.start_span("stress_span");
                span.set_field("key", "value".into());
                span.add_event("checkpoint");
                span.finish();
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }
}

/// 5f. Metrics snapshot consistency under concurrent mutations.
#[test]
fn metrics_snapshot_consistent_under_load() {
    let reg = MetricsRegistry::new();
    let num_threads = 8;
    let ops_per_thread = 500;
    let mut handles = Vec::new();

    for _ in 0..num_threads {
        let r = reg.clone();
        handles.push(std::thread::spawn(move || {
            for i in 0..ops_per_thread {
                r.counter(&format!("c_{}", i % 10)).increment();
                r.gauge(&format!("g_{}", i % 5)).set(i as f64);
                r.histogram("h_main").observe(i as f64);
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    let snap = reg.snapshot();
    assert!(
        snap.counters.contains_key("c_0"),
        "Snapshot should contain c_0"
    );
    assert!(
        snap.gauges.contains_key("g_0"),
        "Snapshot should contain g_0"
    );
    assert!(
        snap.histograms.contains_key("h_main"),
        "Snapshot should contain h_main"
    );

    let total_c0 = snap.counters.get("c_0").copied().unwrap_or(0);
    let expected_per_counter = (num_threads * ops_per_thread / 10) as u64;
    assert_eq!(
        total_c0, expected_per_counter,
        "Counter c_0 should have exactly {expected_per_counter} increments"
    );
}

/// 5g. Multiple registries concurrently — no cross-talk.
#[test]
fn multiple_registries_independent() {
    let num_registries = 10;
    let registries: Vec<MetricsRegistry> = (0..num_registries)
        .map(|_| MetricsRegistry::new())
        .collect();
    let mut handles = Vec::new();

    for r in registries.iter() {
        let r = r.clone();
        handles.push(std::thread::spawn(move || {
            for _ in 0..100 {
                r.counter("indep").increment();
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    for (i, r) in registries.iter().enumerate() {
        assert_eq!(
            r.counter("indep").value(),
            100,
            "Registry {i} should have exactly 100 increments"
        );
    }
}

/// 5h. Histogram timer under concurrent use — verify RAII safety.
#[test]
fn histogram_timer_concurrent_raii() {
    let reg = MetricsRegistry::new();
    let hist = reg.histogram("raii_hist");
    let num_threads = 20;

    let mut handles = Vec::new();
    for _ in 0..num_threads {
        let h = hist.clone();
        handles.push(std::thread::spawn(move || {
            let _timer = HistogramTimer::new(h.clone());
            std::thread::yield_now();
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    assert_eq!(
        hist.count(),
        num_threads as u64,
        "All RAII timers should have recorded"
    );
}

/// 5i. Concurrent logger, metrics, and tracer — combined stress.
#[test]
fn full_observability_stress() {
    let logger = test_logger();
    let metrics = MetricsRegistry::new();
    let tracer = Arc::new(Tracer::new());
    let num_threads = 8;
    let ops_per_thread = 200;

    let mut handles = Vec::new();
    for t in 0..num_threads {
        let logger = logger.clone();
        let metrics = metrics.clone();
        let tracer = tracer.clone();
        handles.push(std::thread::spawn(move || {
            for i in 0..ops_per_thread {
                let mut span = tracer.start_span("integ_stress");
                span.set_field("thread", format!("{}", t));
                logger.info(format!("t{t} i{i}"));
                metrics.counter("stress_ops").increment();
                metrics.histogram("latency").observe(i as f64 * 0.1);
                span.add_event("done");
                span.finish();
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    assert_eq!(
        metrics.counter("stress_ops").value(),
        (num_threads * ops_per_thread) as u64,
        "All stress operations must be counted exactly"
    );
    assert_eq!(
        metrics.histogram("latency").count(),
        (num_threads * ops_per_thread) as u64,
        "All latency observations must be recorded"
    );
}
