use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// A single recorded span event for debugging.
#[derive(Debug, Clone)]
pub struct SpanEvent {
    pub name: &'static str,
    pub timestamp: std::time::Duration,
}

/// Internal span state.
#[derive(Debug, Clone)]
struct SpanData {
    id: u64,
    parent_id: Option<u64>,
    name: &'static str,
    start: Instant,
    fields: Vec<(&'static str, String)>,
    events: Vec<SpanEvent>,
}

/// A guard that records span duration on drop.
///
/// Dropping the guard finalises the span and passes it to the tracer's
/// completion callback.  Callers may also call `finish()` early.
#[derive(Debug)]
pub struct SpanGuard {
    data: SpanData,
    tracer: Tracer,
    finished: bool,
}

impl SpanGuard {
    fn new(data: SpanData, tracer: Tracer) -> Self {
        Self {
            data,
            tracer,
            finished: false,
        }
    }

    /// Record an event within this span.
    pub fn add_event(&mut self, name: &'static str) {
        let elapsed = self.data.start.elapsed();
        self.data.events.push(SpanEvent {
            name,
            timestamp: elapsed,
        });
    }

    /// Set a field on the span.
    pub fn set_field(&mut self, key: &'static str, value: String) {
        self.data.fields.push((key, value));
    }

    /// Finish the span, recording its duration.
    pub fn finish(mut self) {
        if !self.finished {
            self.finished = true;
            self.tracer.record_span(&self.data);
        }
    }
}

impl Drop for SpanGuard {
    fn drop(&mut self) {
        if !self.finished {
            self.finished = true;
            self.tracer.record_span(&self.data);
        }
    }
}

/// A completed span report delivered to the completion callback.
#[derive(Debug, Clone)]
pub struct SpanReport {
    pub id: u64,
    pub parent_id: Option<u64>,
    pub name: &'static str,
    pub duration: std::time::Duration,
    pub fields: Vec<(&'static str, String)>,
    pub events: Vec<SpanEvent>,
}

struct TracerInner {
    next_id: AtomicU64,
    on_complete: Option<Arc<dyn Fn(SpanReport) + Send + Sync>>,
}

/// Tracer for creating and recording spans.
///
/// Spans measure operation duration and carry metadata for debugging
/// and performance analysis.
#[derive(Clone)]
pub struct Tracer {
    inner: Arc<TracerInner>,
}

impl Tracer {
    /// Create a tracer with no completion callback.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(TracerInner {
                next_id: AtomicU64::new(1),
                on_complete: None,
            }),
        }
    }

    /// Create a tracer that calls `on_complete` for every finished span.
    pub fn with_callback<F>(callback: F) -> Self
    where
        F: Fn(SpanReport) + Send + Sync + 'static,
    {
        Self {
            inner: Arc::new(TracerInner {
                next_id: AtomicU64::new(1),
                on_complete: Some(Arc::new(callback)),
            }),
        }
    }

    /// Start a new span.
    pub fn start_span(&self, name: &'static str) -> SpanGuard {
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        SpanGuard::new(
            SpanData {
                id,
                parent_id: None,
                name,
                start: Instant::now(),
                fields: Vec::with_capacity(4),
                events: Vec::with_capacity(4),
            },
            self.clone(),
        )
    }

    /// Start a child span of the given parent.
    pub fn start_child(&self, name: &'static str, parent_id: u64) -> SpanGuard {
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        SpanGuard::new(
            SpanData {
                id,
                parent_id: Some(parent_id),
                name,
                start: Instant::now(),
                fields: Vec::with_capacity(4),
                events: Vec::with_capacity(4),
            },
            self.clone(),
        )
    }

    fn record_span(&self, data: &SpanData) {
        let duration = data.start.elapsed();
        let report = SpanReport {
            id: data.id,
            parent_id: data.parent_id,
            name: data.name,
            duration,
            fields: data.fields.clone(),
            events: data.events.clone(),
        };
        if let Some(ref cb) = self.inner.on_complete {
            cb(report);
        }
    }

    /// Generate a new unique span ID (useful for manual parent tracking).
    pub fn next_id(&self) -> u64 {
        self.inner.next_id.fetch_add(1, Ordering::Relaxed)
    }
}

impl std::fmt::Debug for Tracer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tracer").finish()
    }
}

/// Measure elapsed time for an operation using RAII.
///
/// Records the duration (in milliseconds as f64) when dropped.
/// Optionally records to a callback.
pub struct Timer {
    label: &'static str,
    start: Instant,
    callback: Option<Arc<dyn Fn(&'static str, f64) + Send + Sync>>,
}

impl Timer {
    pub fn start(label: &'static str) -> Self {
        Self {
            label,
            start: Instant::now(),
            callback: None,
        }
    }

    pub fn start_with<F>(label: &'static str, callback: F) -> Self
    where
        F: Fn(&'static str, f64) + Send + Sync + 'static,
    {
        Self {
            label,
            start: Instant::now(),
            callback: Some(Arc::new(callback)),
        }
    }

    pub fn elapsed_ms(&self) -> f64 {
        self.start.elapsed().as_secs_f64() * 1000.0
    }

    pub fn finish(self) -> f64 {
        let ms = self.elapsed_ms();
        if let Some(ref cb) = self.callback {
            cb(self.label, ms);
        }
        ms
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        let ms = self.start.elapsed().as_secs_f64() * 1000.0;
        if let Some(ref cb) = self.callback {
            cb(self.label, ms);
        }
    }
}

impl std::fmt::Debug for Timer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Timer").field("label", &self.label).finish()
    }
}

impl Default for Tracer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn tracer_creates_span() {
        let tracer = Tracer::new();
        let span = tracer.start_span("test_span");
        assert_eq!(span.data.name, "test_span");
    }

    #[test]
    fn span_ids_are_unique() {
        let tracer = Tracer::new();
        let s1 = tracer.start_span("a");
        let s2 = tracer.start_span("b");
        assert_ne!(s1.data.id, s2.data.id);
    }

    #[test]
    fn child_span_has_parent_id() {
        let tracer = Tracer::new();
        let parent = tracer.start_span("parent");
        let parent_id = parent.data.id;
        let child = tracer.start_child("child", parent_id);
        assert_eq!(child.data.parent_id, Some(parent_id));
        drop(parent);
        drop(child);
    }

    #[test]
    fn span_add_event_records_timestamp() {
        let tracer = Tracer::new();
        let mut span = tracer.start_span("test");
        span.add_event("phase1");
        assert_eq!(span.data.events.len(), 1);
    }

    #[test]
    fn span_set_field_attaches_field() {
        let tracer = Tracer::new();
        let mut span = tracer.start_span("test");
        span.set_field("key", "value".into());
        assert_eq!(span.data.fields.len(), 1);
    }

    #[test]
    fn span_finish_triggers_callback() {
        let reports = Arc::new(Mutex::new(Vec::new()));
        let reports_clone = reports.clone();
        let tracer = Tracer::with_callback(move |r| {
            reports_clone.lock().unwrap().push(r);
        });
        let span = tracer.start_span("cb_test");
        span.finish();
        assert_eq!(reports.lock().unwrap().len(), 1);
    }

    #[test]
    fn drop_triggers_callback() {
        let reports = Arc::new(Mutex::new(Vec::new()));
        let reports_clone = reports.clone();
        let tracer = Tracer::with_callback(move |r| {
            reports_clone.lock().unwrap().push(r);
        });
        let _span = tracer.start_span("drop_test");
        drop(_span);
        assert_eq!(reports.lock().unwrap().len(), 1);
    }

    #[test]
    fn timer_finish_returns_ms() {
        let timer = Timer::start("op");
        let ms = timer.finish();
        assert!(ms >= 0.0);
    }

    #[test]
    fn timer_callback_invoked_on_drop() {
        let results = Arc::new(Mutex::new(Vec::new()));
        let r = results.clone();
        let timer = Timer::start_with("timed_op", move |label, ms| {
            r.lock().unwrap().push((label, ms));
        });
        drop(timer);
        assert_eq!(results.lock().unwrap().len(), 1);
    }

    #[test]
    fn report_contains_name_and_duration() {
        let reports = Arc::new(Mutex::new(Vec::new()));
        let r = reports.clone();
        let tracer = Tracer::with_callback(move |report| {
            r.lock().unwrap().push(report);
        });
        let span = tracer.start_span("my_operation");
        span.finish();
        let report = reports.lock().unwrap().remove(0);
        assert_eq!(report.name, "my_operation");
        assert!(report.duration.as_nanos() > 0);
    }
}
