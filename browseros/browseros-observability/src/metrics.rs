use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Instant;

/// A monotonically increasing counter.
#[derive(Debug, Clone)]
pub struct Counter {
    inner: Arc<AtomicU64>,
}

impl Counter {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn increment(&self) {
        self.inner.fetch_add(1, Ordering::Relaxed);
    }

    pub fn add(&self, value: u64) {
        self.inner.fetch_add(value, Ordering::Relaxed);
    }

    pub fn value(&self) -> u64 {
        self.inner.load(Ordering::Relaxed)
    }
}

impl Default for Counter {
    fn default() -> Self {
        Self::new()
    }
}

/// A gauge that holds a current floating-point value.
#[derive(Debug, Clone)]
pub struct Gauge {
    inner: Arc<RwLock<f64>>,
}

impl Gauge {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(0.0)),
        }
    }

    pub fn set(&self, value: f64) {
        if let Ok(mut g) = self.inner.write() {
            *g = value;
        }
    }

    pub fn value(&self) -> f64 {
        self.inner.read().map(|g| *g).unwrap_or(0.0)
    }
}

impl Default for Gauge {
    fn default() -> Self {
        Self::new()
    }
}

/// Configured bucket boundaries for a histogram.
#[derive(Debug, Clone)]
pub struct HistogramBuckets {
    boundaries: Vec<f64>,
}

impl HistogramBuckets {
    pub fn new(boundaries: Vec<f64>) -> Self {
        let mut b = boundaries;
        b.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        Self { boundaries: b }
    }
}

impl Default for HistogramBuckets {
    fn default() -> Self {
        Self::new(vec![
            0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
        ])
    }
}

/// Distribution of observed values across configured buckets.
#[derive(Clone)]
pub struct Histogram {
    inner: Arc<HistogramInner>,
}

#[derive(Debug)]
struct HistogramInner {
    buckets: HistogramBuckets,
    counts: RwLock<Vec<AtomicU64>>,
    total: AtomicU64,
    sum: RwLock<f64>,
}

impl Histogram {
    pub fn new(buckets: HistogramBuckets) -> Self {
        let n = buckets.boundaries.len() + 1;
        let counts = (0..n).map(|_| AtomicU64::new(0)).collect();
        Self {
            inner: Arc::new(HistogramInner {
                buckets,
                counts: RwLock::new(counts),
                total: AtomicU64::new(0),
                sum: RwLock::new(0.0),
            }),
        }
    }

    pub fn observe(&self, value: f64) {
        if let Ok(mut s) = self.inner.sum.write() {
            *s += value;
        }
        self.inner.total.fetch_add(1, Ordering::Relaxed);

        let counts = self.inner.counts.read().unwrap();
        let mut idx = counts.len() - 1;
        for (i, boundary) in self.inner.buckets.boundaries.iter().enumerate() {
            if value <= *boundary {
                idx = i;
                break;
            }
        }
        counts[idx].fetch_add(1, Ordering::Relaxed);
    }

    pub fn count(&self) -> u64 {
        self.inner.total.load(Ordering::Relaxed)
    }

    pub fn sum(&self) -> f64 {
        self.inner.sum.read().map(|s| *s).unwrap_or(0.0)
    }
}

impl std::fmt::Debug for Histogram {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Histogram")
            .field("count", &self.count())
            .finish()
    }
}

/// Guards a `Histogram::observe` call for RAII timing.
#[derive(Debug)]
pub struct HistogramTimer {
    histogram: Histogram,
    start: Instant,
    finished: bool,
}

impl HistogramTimer {
    pub fn new(histogram: Histogram) -> Self {
        Self {
            histogram,
            start: Instant::now(),
            finished: false,
        }
    }

    pub fn finish(mut self) -> f64 {
        self.finished = true;
        let ms = self.start.elapsed().as_secs_f64() * 1000.0;
        self.histogram.observe(ms);
        ms
    }
}

impl Drop for HistogramTimer {
    fn drop(&mut self) {
        if !self.finished {
            let ms = self.start.elapsed().as_secs_f64() * 1000.0;
            self.histogram.observe(ms);
        }
    }
}

/// Snapshot of all metrics at a point in time.
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub counters: HashMap<String, u64>,
    pub gauges: HashMap<String, f64>,
    pub histograms: HashMap<String, HistogramSnapshot>,
}

/// Snapshot of a single histogram.
#[derive(Debug, Clone)]
pub struct HistogramSnapshot {
    pub count: u64,
    pub sum: f64,
    pub buckets: Vec<Bucket>,
}

/// A bucket with its cumulative count.
#[derive(Debug, Clone)]
pub struct Bucket {
    pub le: f64,
    pub count: u64,
}

/// Central metrics registry.
///
/// Thread-safe and cheap to clone.  All metric operations are lock-free
/// or use short-lived RwLock acquisitions.
#[derive(Clone)]
pub struct MetricsRegistry {
    inner: Arc<MetricsInner>,
}

#[derive(Debug)]
struct MetricsInner {
    counters: RwLock<HashMap<String, Counter>>,
    gauges: RwLock<HashMap<String, Gauge>>,
    histograms: RwLock<HashMap<String, Histogram>>,
}

impl MetricsRegistry {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(MetricsInner {
                counters: RwLock::new(HashMap::new()),
                gauges: RwLock::new(HashMap::new()),
                histograms: RwLock::new(HashMap::new()),
            }),
        }
    }

    pub fn counter(&self, name: &str) -> Counter {
        let mut map = self.inner.counters.write().unwrap();
        map.entry(name.to_owned()).or_default().clone()
    }

    pub fn gauge(&self, name: &str) -> Gauge {
        let mut map = self.inner.gauges.write().unwrap();
        map.entry(name.to_owned()).or_default().clone()
    }

    pub fn histogram(&self, name: &str) -> Histogram {
        let mut map = self.inner.histograms.write().unwrap();
        map.entry(name.to_owned())
            .or_insert_with(|| Histogram::new(HistogramBuckets::default()))
            .clone()
    }

    pub fn histogram_with(&self, name: &str, buckets: HistogramBuckets) -> Histogram {
        let mut map = self.inner.histograms.write().unwrap();
        map.entry(name.to_owned())
            .or_insert_with(|| Histogram::new(buckets))
            .clone()
    }

    pub fn snapshot(&self) -> MetricsSnapshot {
        let counters = self
            .inner
            .counters
            .read()
            .unwrap()
            .iter()
            .map(|(k, c)| (k.clone(), c.value()))
            .collect();
        let gauges = self
            .inner
            .gauges
            .read()
            .unwrap()
            .iter()
            .map(|(k, g)| (k.clone(), g.value()))
            .collect();
        let histograms = self
            .inner
            .histograms
            .read()
            .unwrap()
            .iter()
            .map(|(k, h)| {
                let snapshot = HistogramSnapshot {
                    count: h.count(),
                    sum: h.sum(),
                    buckets: Vec::new(),
                };
                (k.clone(), snapshot)
            })
            .collect();
        MetricsSnapshot {
            counters,
            gauges,
            histograms,
        }
    }
}

impl Default for MetricsRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for MetricsRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MetricsRegistry").finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counter_increment() {
        let c = Counter::new();
        assert_eq!(c.value(), 0);
        c.increment();
        assert_eq!(c.value(), 1);
        c.increment();
        assert_eq!(c.value(), 2);
    }

    #[test]
    fn counter_add() {
        let c = Counter::new();
        c.add(10);
        assert_eq!(c.value(), 10);
        c.add(5);
        assert_eq!(c.value(), 15);
    }

    #[test]
    fn gauge_set_and_get() {
        let g = Gauge::new();
        assert!((g.value() - 0.0).abs() < f64::EPSILON);
        g.set(42.5);
        assert!((g.value() - 42.5).abs() < f64::EPSILON);
    }

    #[test]
    fn histogram_observe_records() {
        let h = Histogram::new(HistogramBuckets::default());
        assert_eq!(h.count(), 0);
        h.observe(0.05);
        assert_eq!(h.count(), 1);
        h.observe(2.0);
        assert_eq!(h.count(), 2);
    }

    #[test]
    fn histogram_sum_tracks_total() {
        let h = Histogram::new(HistogramBuckets::default());
        h.observe(1.0);
        h.observe(2.0);
        assert!((h.sum() - 3.0).abs() < 0.001);
    }

    #[test]
    fn registry_counter_creates_on_demand() {
        let reg = MetricsRegistry::new();
        let c = reg.counter("ops");
        c.increment();
        assert_eq!(reg.counter("ops").value(), 1);
    }

    #[test]
    fn registry_gauge_creates_on_demand() {
        let reg = MetricsRegistry::new();
        let g = reg.gauge("temperature");
        g.set(36.6);
        assert!((reg.gauge("temperature").value() - 36.6).abs() < f64::EPSILON);
    }

    #[test]
    fn registry_histogram_creates_on_demand() {
        let reg = MetricsRegistry::new();
        let h = reg.histogram("latency");
        h.observe(0.5);
        assert_eq!(reg.histogram("latency").count(), 1);
    }

    #[test]
    fn snapshot_contains_all_metrics() {
        let reg = MetricsRegistry::new();
        reg.counter("c1").increment();
        reg.gauge("g1").set(1.0);
        reg.histogram("h1").observe(0.1);
        let snap = reg.snapshot();
        assert!(snap.counters.contains_key("c1"));
        assert!(snap.gauges.contains_key("g1"));
        assert!(snap.histograms.contains_key("h1"));
    }

    #[test]
    fn histogram_timer_records_on_drop() {
        let h = Histogram::new(HistogramBuckets::default());
        {
            let _t = HistogramTimer::new(h.clone());
        }
        assert_eq!(h.count(), 1);
    }

    #[test]
    fn histogram_timer_finish_returns_ms() {
        let h = Histogram::new(HistogramBuckets::default());
        let t = HistogramTimer::new(h.clone());
        let ms = t.finish();
        assert!(ms >= 0.0);
        assert_eq!(h.count(), 1);
    }

    #[test]
    fn concurrent_counter_access() {
        let c = Counter::new();
        let mut threads = Vec::new();
        for _ in 0..10 {
            let c = c.clone();
            threads.push(std::thread::spawn(move || {
                for _ in 0..100 {
                    c.increment();
                }
            }));
        }
        for t in threads {
            t.join().unwrap();
        }
        assert_eq!(c.value(), 1000);
    }

    #[test]
    fn registry_is_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<MetricsRegistry>();
        assert_sync::<MetricsRegistry>();
    }

    #[test]
    fn histogram_default_buckets() {
        let b = HistogramBuckets::default();
        assert!(!b.boundaries.is_empty());
        assert_eq!(b.boundaries[0], 0.005);
    }
}
