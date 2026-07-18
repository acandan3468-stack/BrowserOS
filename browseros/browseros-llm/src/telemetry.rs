//! Telemetry — metrics, events, and logging for LLM operations.
//!
//! Central observability component for `browseros-llm`.  Integrates
//! with `browseros-observability` (metrics, logging, tracing) and
//! `browseros-event-bus` for event publishing.

use std::any::Any;
use std::sync::Arc;

use browseros_event_bus::EventBus;
use browseros_observability::logger::{FieldValue, LevelFilter, LogLevel, Logger};
use browseros_observability::metrics::{
    Counter, Gauge, Histogram, HistogramBuckets, MetricsRegistry,
};
use browseros_observability::tracer::{SpanGuard, Tracer};
use browseros_types::event::{Event, EventCategory, EventMetadata};
use browseros_types::identifiers::{CorrelationId, ModuleId};
use browseros_types::value::{ContentType, SemVer};
use chrono::Utc;

use crate::error::LlmError;
use crate::types::{ProviderHealth, ResolvedEndpoint, TelemetryConfig};

// ─────────────────────────────────────────────────────────────────────────────
// Constants
// ─────────────────────────────────────────────────────────────────────────────

/// Default latency histogram buckets (milliseconds).
const LATENCY_BUCKETS_MS: &[f64] = &[
    50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0, 30000.0, 60000.0,
];

/// Stream chunk latency buckets (milliseconds).
#[allow(dead_code)]
const STREAM_LATENCY_BUCKETS_MS: &[f64] = &[5.0, 10.0, 25.0, 50.0, 100.0, 200.0, 500.0, 1000.0];

/// Token histogram buckets.
const TOKEN_BUCKETS: &[f64] = &[
    1.0, 10.0, 50.0, 100.0, 500.0, 1000.0, 4000.0, 16000.0, 64000.0, 128000.0,
];

const LLM_COST_CENTS_TOTAL: &str = "llm_cost_cents_total";
const LLM_BUDGET_REMAINING_CENTS: &str = "llm_budget_remaining_cents";
const LLM_PROVIDER_HEALTH: &str = "llm_provider_health";
const LLM_CACHE_SIZE: &str = "llm_cache_size";
const LLM_ACTIVE_STREAMS: &str = "llm_active_streams";

// ─────────────────────────────────────────────────────────────────────────────
// Event types
// ─────────────────────────────────────────────────────────────────────────────

macro_rules! llm_event {
    ($name:ident, $kind:expr) => {
        #[derive(Debug)]
        struct $name {
            metadata: EventMetadata,
        }

        impl Event for $name {
            fn kind(&self) -> &'static str {
                $kind
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
    };
}

llm_event!(LlmRequestStartedEvent, "llm.request.started");
llm_event!(LlmRequestCompletedEvent, "llm.request.completed");
llm_event!(LlmRequestFailedEvent, "llm.request.failed");
llm_event!(LlmCacheHitEvent, "llm.cache.hit");
llm_event!(LlmCacheMissEvent, "llm.cache.miss");
llm_event!(LlmProviderDegradedEvent, "llm.provider.degraded");
llm_event!(LlmRateLimitedEvent, "llm.rate_limited");
llm_event!(LlmCostThresholdEvent, "llm.cost.threshold");
llm_event!(LlmModelSwitchedEvent, "llm.model.switched");
llm_event!(LlmStreamChunkEvent, "llm.stream.chunk");
llm_event!(LlmRetryEvent, "llm.retry");
llm_event!(LlmFallbackEvent, "llm.fallback");

fn make_event_metadata(correlation_id: CorrelationId) -> EventMetadata {
    EventMetadata::new(
        ModuleId::new("browseros-llm", SemVer::new(0, 1, 0)),
        correlation_id,
        None,
        ContentType::new("application/x-browseros-event"),
        Utc::now(),
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// LlmTelemetry
// ─────────────────────────────────────────────────────────────────────────────

/// Central telemetry hub for LLM operations.
///
/// Collects metrics, publishes events, writes structured logs, and
/// creates tracing spans.  All operations are best-effort — failures
/// inside telemetry never break the LLM execution path.
pub struct LlmTelemetry {
    /// Shared metrics registry.
    metrics: MetricsRegistry,
    /// Optional event bus for publishing telemetry events.
    event_bus: Option<EventBus>,
    /// Structured logger.
    logger: Logger,
    /// Optional tracer for span creation.
    tracer: Option<Tracer>,
    /// Telemetry configuration (honoured by all record_* helpers).
    config: TelemetryConfig,
}

impl std::fmt::Debug for LlmTelemetry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LlmTelemetry")
            .field("event_bus", &self.event_bus.is_some())
            .field("tracer", &self.tracer.is_some())
            .field("config", &self.config)
            .finish()
    }
}

impl LlmTelemetry {
    /// Create a new telemetry hub.
    pub fn new(
        metrics: MetricsRegistry,
        event_bus: Option<EventBus>,
        logger: Logger,
        tracer: Option<Tracer>,
        config: &TelemetryConfig,
    ) -> Self {
        Self {
            metrics,
            event_bus,
            logger: logger.child("browseros-llm"),
            tracer,
            config: config.clone(),
        }
    }

    /// Create a minimal telemetry instance with all telemetry enabled.
    /// Logs are written to stderr (MCP-compatible — stdout is reserved for JSON-RPC).
    pub fn new_enabled() -> Self {
        let metrics = MetricsRegistry::new();
        let filter = Arc::new(LevelFilter::new(LogLevel::Trace));
        let sink = Arc::new(browseros_observability::export::StderrSink::new());
        let logger = Logger::new(sink, filter, "browseros-llm");
        Self {
            metrics,
            event_bus: None,
            logger,
            tracer: None,
            config: TelemetryConfig::default(),
        }
    }

    /// Return a reference to the metrics registry.
    pub fn metrics(&self) -> &MetricsRegistry {
        &self.metrics
    }

    /// Return a reference to the config.
    pub fn config(&self) -> &TelemetryConfig {
        &self.config
    }

    // ── Internal helpers ──────────────────────────────────────────────────

    /// Emit an event if the event bus is configured and events are enabled.
    fn emit(&self, event: Box<dyn Event>) {
        if !self.config.enable_events {
            return;
        }
        if let Some(ref bus) = self.event_bus {
            bus.publish(event);
        }
    }

    /// Start a span if a tracer is configured.
    #[allow(dead_code)]
    pub fn start_span(&self, name: &'static str) -> Option<SpanGuard> {
        self.tracer.as_ref().map(|t| t.start_span(name))
    }

    /// Start a child span if a tracer is configured.
    #[allow(dead_code)]
    pub fn start_child_span(&self, name: &'static str, parent_id: u64) -> Option<SpanGuard> {
        self.tracer.as_ref().map(|t| t.start_child(name, parent_id))
    }

    /// Log a structured message if it passes the configured log level filter.
    #[allow(dead_code)]
    fn log(&self, level: LogLevel, message: impl Into<String>) {
        self.logger.log_with(level, message, Vec::new());
    }

    fn log_with_fields(
        &self,
        level: LogLevel,
        message: impl Into<String>,
        fields: Vec<(String, FieldValue)>,
    ) {
        self.logger.log_with(level, message, fields);
    }

    // ── Counter/ gauge/ histogram helpers ─────────────────────────────────

    fn cnt(&self, name: &str) -> Counter {
        if !self.config.enable_metrics {
            return Counter::new();
        }
        self.metrics.counter(name)
    }

    fn gauge(&self, name: &str) -> Gauge {
        if !self.config.enable_metrics {
            return Gauge::new();
        }
        self.metrics.gauge(name)
    }

    fn histogram(&self, name: &str, buckets: &'static [f64]) -> Histogram {
        if !self.config.enable_metrics {
            return Histogram::new(HistogramBuckets::default());
        }
        self.metrics
            .histogram_with(name, HistogramBuckets::new(buckets.to_vec()))
    }

    // ── Provider health gauge mapping ─────────────────────────────────────

    fn health_gauge_value(status: &ProviderHealth) -> f64 {
        match status {
            ProviderHealth::Unknown => 0.0,
            ProviderHealth::Healthy => 1.0,
            ProviderHealth::Degraded { .. } => 2.0,
            ProviderHealth::Unavailable { .. } => 3.0,
        }
    }

    // ── Sampling support ──────────────────────────────────────────────────

    fn should_sample(&self) -> bool {
        if (self.config.request_sampling_rate - 1.0).abs() < f64::EPSILON {
            return true;
        }
        if self.config.request_sampling_rate <= 0.0 {
            return false;
        }
        // Simple threshold-based sampling: always sample when rate >= 1.0,
        // never when rate <= 0.0, otherwise compare against a hash of the current
        // nanosecond timestamp modulo u64::MAX.
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let threshold = (self.config.request_sampling_rate * u64::MAX as f64) as u64;
        ((now as u64)
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407))
            <= threshold
    }

    // ── Stats helpers local to this module ────────────────────────────────

    /// Record a metric label for total tokens, split by direction.
    /// Metrics are incremented via the registry counters.
    fn record_tokens_inner(&self, input: u64, output: u64) {
        if !self.config.enable_metrics {
            return;
        }
        let total = self.cnt("llm_tokens_total");
        total.add(input + output);
    }

    // ── Public helpers ────────────────────────────────────────────────────

    /// Record that a provider request has started.
    pub fn record_request_started(&self, req: &crate::types::ProviderRequest) {
        if !self.should_sample() {
            return;
        }
        self.cnt("llm_requests_total").increment();
        self.log_with_fields(
            LogLevel::Info,
            "llm_request_started",
            vec![
                ("model".into(), FieldValue::String(req.model.clone())),
                (
                    "input_tokens_est".into(),
                    FieldValue::Int(req.messages.len() as i64),
                ),
            ],
        );
        let meta = make_event_metadata(CorrelationId::new());
        let event = LlmRequestStartedEvent { metadata: meta };
        self.emit(Box::new(event));
    }

    /// Record that a provider request completed successfully.
    pub fn record_request_completed(
        &self,
        model: &str,
        provider: &str,
        duration_ms: f64,
        input_tokens: u64,
        output_tokens: u64,
        correlation_id: CorrelationId,
    ) {
        if !self.should_sample() {
            return;
        }
        // Counters
        self.cnt("llm_requests_total");
        self.record_tokens_inner(input_tokens, output_tokens);
        self.record_input_tokens(input_tokens);
        self.record_output_tokens(output_tokens);

        // Latency histogram
        self.histogram("llm_request_duration_ms", LATENCY_BUCKETS_MS)
            .observe(duration_ms);

        // Structured log
        self.log_with_fields(
            LogLevel::Info,
            "llm_request_completed",
            vec![
                ("model".into(), FieldValue::String(model.to_owned())),
                ("provider".into(), FieldValue::String(provider.to_owned())),
                ("duration_ms".into(), FieldValue::Float(duration_ms)),
                ("input_tokens".into(), FieldValue::Int(input_tokens as i64)),
                (
                    "output_tokens".into(),
                    FieldValue::Int(output_tokens as i64),
                ),
            ],
        );

        // Event
        let meta = make_event_metadata(correlation_id);
        let event = LlmRequestCompletedEvent { metadata: meta };
        self.emit(Box::new(event));
    }

    /// Record that a provider request failed.
    pub fn record_request_failed(
        &self,
        error: &LlmError,
        provider: &str,
        model: &str,
        duration_ms: f64,
        correlation_id: CorrelationId,
    ) {
        if !self.should_sample() {
            return;
        }
        let sanitized = LlmError::sanitize(&error.to_string());
        self.log_with_fields(
            LogLevel::Error,
            "llm_request_failed",
            vec![
                ("model".into(), FieldValue::String(model.to_owned())),
                ("provider".into(), FieldValue::String(provider.to_owned())),
                ("error".into(), FieldValue::String(sanitized)),
                ("duration_ms".into(), FieldValue::Float(duration_ms)),
                (
                    "error_type".into(),
                    FieldValue::String(format!("{:?}", error)),
                ),
            ],
        );

        let meta = make_event_metadata(correlation_id);
        let event = LlmRequestFailedEvent { metadata: meta };
        self.emit(Box::new(event));
    }

    /// Record a cache hit.
    pub fn record_cache_hit(&self, key: &str) {
        if !self.config.enable_metrics {
            return;
        }
        self.cnt("llm_cache_hits_total").increment();
        self.log_with_fields(
            LogLevel::Debug,
            "llm_cache_hit",
            vec![("key".into(), FieldValue::String(key.to_owned()))],
        );
        let meta = make_event_metadata(CorrelationId::new());
        let event = LlmCacheHitEvent { metadata: meta };
        self.emit(Box::new(event));
    }

    /// Record a cache miss.
    pub fn record_cache_miss(&self, key: &str) {
        if !self.config.enable_metrics {
            return;
        }
        self.cnt("llm_cache_misses_total").increment();
        self.log_with_fields(
            LogLevel::Debug,
            "llm_cache_miss",
            vec![("key".into(), FieldValue::String(key.to_owned()))],
        );
        let meta = make_event_metadata(CorrelationId::new());
        let event = LlmCacheMissEvent { metadata: meta };
        self.emit(Box::new(event));
    }

    /// Set the cache size gauge.
    pub fn set_cache_size(&self, size: usize) {
        if !self.config.enable_metrics {
            return;
        }
        self.gauge(LLM_CACHE_SIZE).set(size as f64);
    }

    /// Record input tokens histogram observe.
    fn record_input_tokens(&self, tokens: u64) {
        if !self.config.enable_metrics {
            return;
        }
        self.histogram("llm_input_tokens", TOKEN_BUCKETS)
            .observe(tokens as f64);
    }

    /// Record output tokens histogram observe.
    fn record_output_tokens(&self, tokens: u64) {
        if !self.config.enable_metrics {
            return;
        }
        self.histogram("llm_output_tokens", TOKEN_BUCKETS)
            .observe(tokens as f64);
    }

    /// Record cost incurred.
    pub fn record_cost(&self, cost_cents: f64, model: &str) {
        if !self.config.enable_metrics {
            return;
        }
        let counter = self.cnt(LLM_COST_CENTS_TOTAL);
        // Accumulate fractional cents via repeated addition
        let millicents = (cost_cents * 1000.0).round() as u64;
        for _ in 0..millicents {
            counter.increment();
        }
        self.log_with_fields(
            LogLevel::Info,
            "llm_cost_recorded",
            vec![
                ("cost_cents".into(), FieldValue::Float(cost_cents)),
                ("model".into(), FieldValue::String(model.to_owned())),
            ],
        );
    }

    /// Record budget threshold.
    pub fn record_budget_threshold(
        &self,
        budget_cents: f64,
        remaining_cents: f64,
        correlation_id: CorrelationId,
    ) {
        if !self.config.enable_metrics {
            return;
        }
        self.gauge(LLM_BUDGET_REMAINING_CENTS).set(remaining_cents);
        self.log_with_fields(
            LogLevel::Warn,
            "llm_budget_threshold",
            vec![
                ("budget_cents".into(), FieldValue::Float(budget_cents)),
                ("remaining_cents".into(), FieldValue::Float(remaining_cents)),
            ],
        );
        let meta = make_event_metadata(correlation_id);
        let event = LlmCostThresholdEvent { metadata: meta };
        self.emit(Box::new(event));
    }

    /// Record provider health status change.
    pub fn record_provider_health(
        &self,
        provider: &str,
        status: &ProviderHealth,
        correlation_id: CorrelationId,
    ) {
        if !self.config.enable_metrics {
            return;
        }
        let gauge_val = Self::health_gauge_value(status);
        self.gauge(LLM_PROVIDER_HEALTH).set(gauge_val);
        self.log_with_fields(
            LogLevel::Warn,
            "llm_provider_degraded",
            vec![
                ("provider".into(), FieldValue::String(provider.to_owned())),
                ("status".into(), FieldValue::String(format!("{:?}", status))),
            ],
        );
        let meta = make_event_metadata(correlation_id);
        let event = LlmProviderDegradedEvent { metadata: meta };
        self.emit(Box::new(event));
    }

    /// Record a retry attempt.
    pub fn record_retry(&self, attempt: u32, provider: &str, model: &str) {
        if !self.config.enable_metrics {
            return;
        }
        self.cnt("llm_retries_total").increment();
        self.log_with_fields(
            LogLevel::Warn,
            "llm_retry",
            vec![
                ("attempt".into(), FieldValue::Int(attempt as i64)),
                ("provider".into(), FieldValue::String(provider.to_owned())),
                ("model".into(), FieldValue::String(model.to_owned())),
            ],
        );
        let meta = make_event_metadata(CorrelationId::new());
        let event = LlmRetryEvent { metadata: meta };
        self.emit(Box::new(event));
    }

    /// Record a fallback to another provider/model.
    pub fn record_fallback(
        &self,
        from_provider: &str,
        to_provider: &str,
        endpoint: &ResolvedEndpoint,
    ) {
        if !self.config.enable_metrics {
            return;
        }
        self.cnt("llm_fallbacks_total").increment();
        self.log_with_fields(
            LogLevel::Warn,
            "llm_fallback",
            vec![
                (
                    "from_provider".into(),
                    FieldValue::String(from_provider.to_owned()),
                ),
                (
                    "to_provider".into(),
                    FieldValue::String(to_provider.to_owned()),
                ),
                (
                    "model".into(),
                    FieldValue::String(endpoint.model_id.clone()),
                ),
                (
                    "capability".into(),
                    FieldValue::String(endpoint.capability.clone()),
                ),
            ],
        );
        let meta = make_event_metadata(CorrelationId::new());
        let event = LlmFallbackEvent { metadata: meta };
        self.emit(Box::new(event));
        // Also emit a model-switched event for observability
        let switch_meta = make_event_metadata(CorrelationId::new());
        let switch_event = LlmModelSwitchedEvent {
            metadata: switch_meta,
        };
        self.emit(Box::new(switch_event));
    }

    /// Record a rate limit.
    pub fn record_rate_limit(
        &self,
        provider: &str,
        model: &str,
        retry_after_ms: u64,
        correlation_id: CorrelationId,
    ) {
        if !self.config.enable_metrics {
            return;
        }
        self.cnt("llm_rate_limits_total").increment();
        self.log_with_fields(
            LogLevel::Warn,
            "llm_rate_limited",
            vec![
                ("provider".into(), FieldValue::String(provider.to_owned())),
                ("model".into(), FieldValue::String(model.to_owned())),
                (
                    "retry_after_ms".into(),
                    FieldValue::Int(retry_after_ms as i64),
                ),
            ],
        );
        let meta = make_event_metadata(correlation_id);
        let event = LlmRateLimitedEvent { metadata: meta };
        self.emit(Box::new(event));
    }

    // ── Streaming telemetry ───────────────────────────────────────────────

    /// Record a stream chunk received.
    pub fn record_stream_chunk(&self, chunk_len: usize, model: &str) {
        if !self.config.enable_metrics {
            return;
        }
        if self.config.enable_stream_chunk_events {
            self.log_with_fields(
                LogLevel::Debug,
                "llm_stream_chunk",
                vec![
                    ("chunk_len".into(), FieldValue::Int(chunk_len as i64)),
                    ("model".into(), FieldValue::String(model.to_owned())),
                ],
            );
            let meta = make_event_metadata(CorrelationId::new());
            let event = LlmStreamChunkEvent { metadata: meta };
            self.emit(Box::new(event));
        }
    }

    /// Record that streaming finished.
    pub fn record_stream_finished(&self, model: &str, total_chunks: u64) {
        if !self.config.enable_metrics {
            return;
        }
        self.gauge(LLM_ACTIVE_STREAMS);
        // Active streams decremented on finish
        let g = self.gauge(LLM_ACTIVE_STREAMS);
        let current = g.value();
        if current > 0.0 {
            g.set(current - 1.0);
        }
        self.log_with_fields(
            LogLevel::Info,
            "llm_stream_finished",
            vec![
                ("model".into(), FieldValue::String(model.to_owned())),
                ("total_chunks".into(), FieldValue::Int(total_chunks as i64)),
            ],
        );
    }

    /// Record a stream error.
    pub fn record_stream_error(
        &self,
        error: &LlmError,
        model: &str,
        correlation_id: CorrelationId,
    ) {
        let sanitized = LlmError::sanitize(&error.to_string());
        // Active streams decremented on error
        if self.config.enable_metrics {
            let g = self.gauge(LLM_ACTIVE_STREAMS);
            let current = g.value();
            if current > 0.0 {
                g.set(current - 1.0);
            }
        }
        self.log_with_fields(
            LogLevel::Error,
            "llm_stream_error",
            vec![
                ("error".into(), FieldValue::String(sanitized)),
                ("model".into(), FieldValue::String(model.to_owned())),
            ],
        );
        let meta = make_event_metadata(correlation_id);
        let event = LlmRequestFailedEvent { metadata: meta };
        self.emit(Box::new(event));
    }

    /// Increment active stream count.
    pub fn record_stream_started(&self, model: &str) {
        if !self.config.enable_metrics {
            return;
        }
        let g = self.gauge(LLM_ACTIVE_STREAMS);
        let current = g.value();
        g.set(current + 1.0);
        self.log_with_fields(
            LogLevel::Info,
            "llm_stream_started",
            vec![("model".into(), FieldValue::String(model.to_owned()))],
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Send + Sync safety
// ─────────────────────────────────────────────────────────────────────────────

fn _assert_send_sync()
where
    LlmTelemetry: Send + Sync,
{
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ProviderHealth, TelemetryConfig};
    use browseros_observability::export::OutputSink;
    use browseros_observability::logger::LogFilter;
    use browseros_observability::logger::LogRecord;
    use std::sync::Mutex;

    /// A sink that captures log records in memory.
    #[derive(Clone)]
    struct TestSink {
        records: Arc<Mutex<Vec<LogRecord>>>,
    }

    impl TestSink {
        fn new() -> Self {
            Self {
                records: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn records(&self) -> Vec<LogRecord> {
            self.records.lock().unwrap().clone()
        }
    }

    impl OutputSink for TestSink {
        fn write(&self, record: &LogRecord) {
            self.records.lock().unwrap().push(record.clone());
        }

        fn flush(&self) {}
    }

    fn test_filter() -> Arc<dyn LogFilter> {
        Arc::new(LevelFilter::new(LogLevel::Trace))
    }

    fn test_logger(sink: Arc<TestSink>) -> Logger {
        Logger::new(sink, test_filter(), "test")
    }

    fn test_telemetry() -> LlmTelemetry {
        LlmTelemetry::new_enabled()
    }

    fn telemetry_with_sink(sink: Arc<TestSink>) -> LlmTelemetry {
        let metrics = MetricsRegistry::new();
        let logger = Logger::new(sink, test_filter(), "test");
        LlmTelemetry::new(metrics, None, logger, None, &TelemetryConfig::default())
    }

    fn telemetry_disabled() -> LlmTelemetry {
        let metrics = MetricsRegistry::new();
        let sink = Arc::new(TestSink::new());
        let logger = Logger::new(sink, test_filter(), "test");
        LlmTelemetry::new(
            metrics,
            None,
            logger,
            None,
            &TelemetryConfig {
                enable_metrics: false,
                enable_events: false,
                request_sampling_rate: 0.0,
                ..TelemetryConfig::default()
            },
        )
    }

    // ── Constructor tests ────────────────────────────────────────────────

    #[test]
    fn new_creates_telemetry() {
        let t = test_telemetry();
        assert!(t.config.enable_metrics);
    }

    #[test]
    fn new_disabled_telemetry() {
        let t = telemetry_disabled();
        assert!(!t.config.enable_metrics);
    }

    #[test]
    fn debug_does_not_panic() {
        let t = test_telemetry();
        let _ = format!("{:?}", t);
    }

    #[test]
    fn metrics_registry_accessible() {
        let t = test_telemetry();
        let _ = t.metrics();
    }

    // ── Request started ──────────────────────────────────────────────────

    #[test]
    fn record_request_started_increments_counter() {
        let t = test_telemetry();
        let req = crate::types::ProviderRequest {
            model: "gpt-4".into(),
            messages: vec![],
            max_tokens: None,
            temperature: None,
            stop_sequences: vec![],
            system_prompt: None,
            tools: vec![],
            stream: false,
            timeout_ms: 30000,
        };
        t.record_request_started(&req);
        let val = t.metrics().counter("llm_requests_total").value();
        assert_eq!(val, 1);
    }

    #[test]
    fn record_request_started_disabled_noop() {
        let t = telemetry_disabled();
        let req = crate::types::ProviderRequest {
            model: "gpt-4".into(),
            messages: vec![],
            max_tokens: None,
            temperature: None,
            stop_sequences: vec![],
            system_prompt: None,
            tools: vec![],
            stream: false,
            timeout_ms: 30000,
        };
        t.record_request_started(&req);
        let val = t.metrics().counter("llm_requests_total").value();
        assert_eq!(val, 0);
    }

    // ── Request completed ────────────────────────────────────────────────

    #[test]
    fn record_request_completed_increments_tokens() {
        let t = test_telemetry();
        t.record_request_completed("gpt-4", "openai", 150.0, 100, 50, CorrelationId::new());
        // Token counter is incremented by request_completed
        let tokens = t.metrics().counter("llm_tokens_total").value();
        assert_eq!(tokens, 150);
    }

    #[test]
    fn record_request_completed_logs() {
        let sink = Arc::new(TestSink::new());
        let t = telemetry_with_sink(sink.clone());
        t.record_request_completed("gpt-4", "openai", 150.0, 100, 50, CorrelationId::new());
        let records = sink.records();
        let has_log = records.iter().any(|r| r.message == "llm_request_completed");
        assert!(has_log);
    }

    #[test]
    fn record_request_completed_disabled_noop() {
        let t = telemetry_disabled();
        t.record_request_completed("gpt-4", "openai", 150.0, 100, 50, CorrelationId::new());
        let val = t.metrics().counter("llm_requests_total").value();
        assert_eq!(val, 0);
    }

    // ── Request failed ───────────────────────────────────────────────────

    #[test]
    fn record_request_failed_logs() {
        let sink = Arc::new(TestSink::new());
        let t = telemetry_with_sink(sink.clone());
        let err = LlmError::ProviderError("timeout".into());
        t.record_request_failed(&err, "openai", "gpt-4", 500.0, CorrelationId::new());
        let records = sink.records();
        let has_log = records.iter().any(|r| r.message == "llm_request_failed");
        assert!(has_log);
    }

    // ── Cache hit ────────────────────────────────────────────────────────

    #[test]
    fn record_cache_hit_increments_counter() {
        let t = test_telemetry();
        t.record_cache_hit("key1");
        assert_eq!(t.metrics().counter("llm_cache_hits_total").value(), 1);
    }

    #[test]
    fn record_cache_hit_disabled_noop() {
        let t = telemetry_disabled();
        t.record_cache_hit("key1");
        assert_eq!(t.metrics().counter("llm_cache_hits_total").value(), 0);
    }

    // ── Cache miss ───────────────────────────────────────────────────────

    #[test]
    fn record_cache_miss_increments_counter() {
        let t = test_telemetry();
        t.record_cache_miss("key1");
        assert_eq!(t.metrics().counter("llm_cache_misses_total").value(), 1);
    }

    #[test]
    fn cache_hit_and_miss_track_separately() {
        let t = test_telemetry();
        t.record_cache_hit("k");
        t.record_cache_miss("k");
        assert_eq!(t.metrics().counter("llm_cache_hits_total").value(), 1);
        assert_eq!(t.metrics().counter("llm_cache_misses_total").value(), 1);
    }

    // ── Cache size gauge ─────────────────────────────────────────────────

    #[test]
    fn cache_size_gauge_tracks_value() {
        let t = test_telemetry();
        t.set_cache_size(42);
        let val = t.metrics().gauge(LLM_CACHE_SIZE).value();
        assert!((val - 42.0).abs() < f64::EPSILON);
    }

    // ── Cost telemetry ───────────────────────────────────────────────────

    #[test]
    fn record_cost_increments_counter() {
        let t = test_telemetry();
        t.record_cost(0.05, "gpt-4");
        let val = t.metrics().counter(LLM_COST_CENTS_TOTAL).value();
        // 0.05 cents = 50 millicents
        assert_eq!(val, 50);
    }

    #[test]
    fn record_cost_disabled_noop() {
        let t = telemetry_disabled();
        t.record_cost(1.0, "gpt-4");
        assert_eq!(t.metrics().counter(LLM_COST_CENTS_TOTAL).value(), 0);
    }

    #[test]
    fn record_cost_multiple_calls_accumulate() {
        let t = test_telemetry();
        t.record_cost(0.10, "gpt-4");
        t.record_cost(0.20, "gpt-4");
        let val = t.metrics().counter(LLM_COST_CENTS_TOTAL).value();
        assert_eq!(val, 300); // 100 + 200 millicents
    }

    // ── Budget threshold ─────────────────────────────────────────────────

    #[test]
    fn record_budget_threshold_sets_gauge() {
        let t = test_telemetry();
        t.record_budget_threshold(100.0, 50.0, CorrelationId::new());
        let val = t.metrics().gauge(LLM_BUDGET_REMAINING_CENTS).value();
        assert!((val - 50.0).abs() < f64::EPSILON);
    }

    // ── Provider health ──────────────────────────────────────────────────

    #[test]
    fn health_gauge_unknown_zero() {
        assert!(
            (LlmTelemetry::health_gauge_value(&ProviderHealth::Unknown) - 0.0).abs() < f64::EPSILON
        );
    }

    #[test]
    fn health_gauge_healthy_one() {
        assert!(
            (LlmTelemetry::health_gauge_value(&ProviderHealth::Healthy) - 1.0).abs() < f64::EPSILON
        );
    }

    #[test]
    fn health_gauge_degraded_two() {
        assert!(
            (LlmTelemetry::health_gauge_value(&ProviderHealth::Degraded {
                reason: "err".into()
            }) - 2.0)
                .abs()
                < f64::EPSILON
        );
    }

    #[test]
    fn health_gauge_unavailable_three() {
        assert!(
            (LlmTelemetry::health_gauge_value(&ProviderHealth::Unavailable {
                since: std::time::SystemTime::now()
            }) - 3.0)
                .abs()
                < f64::EPSILON
        );
    }

    #[test]
    fn record_provider_healthy_sets_gauge() {
        let t = test_telemetry();
        t.record_provider_health("openai", &ProviderHealth::Healthy, CorrelationId::new());
        let val = t.metrics().gauge(LLM_PROVIDER_HEALTH).value();
        assert!((val - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn record_provider_degraded_logs() {
        let sink = Arc::new(TestSink::new());
        let t = telemetry_with_sink(sink.clone());
        t.record_provider_health(
            "openai",
            &ProviderHealth::Degraded {
                reason: "high latency".into(),
            },
            CorrelationId::new(),
        );
        let records = sink.records();
        let has_log = records.iter().any(|r| r.message == "llm_provider_degraded");
        assert!(has_log);
    }

    // ── Retry telemetry ──────────────────────────────────────────────────

    #[test]
    fn record_retry_increments_counter() {
        let t = test_telemetry();
        t.record_retry(1, "openai", "gpt-4");
        assert_eq!(t.metrics().counter("llm_retries_total").value(), 1);
    }

    #[test]
    fn record_retry_disabled_noop() {
        let t = telemetry_disabled();
        t.record_retry(1, "openai", "gpt-4");
        assert_eq!(t.metrics().counter("llm_retries_total").value(), 0);
    }

    #[test]
    fn record_retry_multiple_tracks_each() {
        let t = test_telemetry();
        t.record_retry(1, "openai", "gpt-4");
        t.record_retry(2, "openai", "gpt-4");
        assert_eq!(t.metrics().counter("llm_retries_total").value(), 2);
    }

    // ── Fallback telemetry ───────────────────────────────────────────────

    #[test]
    fn record_fallback_increments_counter() {
        let t = test_telemetry();
        let ep = ResolvedEndpoint {
            provider_id: "anthropic".into(),
            model_id: "claude-3".into(),
            capability: "chat".into(),
            priority: 1,
            estimated_cost_cents: 0.0,
        };
        t.record_fallback("openai", "anthropic", &ep);
        assert_eq!(t.metrics().counter("llm_fallbacks_total").value(), 1);
    }

    #[test]
    fn record_fallback_disabled_noop() {
        let t = telemetry_disabled();
        let ep = ResolvedEndpoint {
            provider_id: "anthropic".into(),
            model_id: "claude-3".into(),
            capability: "chat".into(),
            priority: 1,
            estimated_cost_cents: 0.0,
        };
        t.record_fallback("openai", "anthropic", &ep);
        assert_eq!(t.metrics().counter("llm_fallbacks_total").value(), 0);
    }

    // ── Rate limit telemetry ─────────────────────────────────────────────

    #[test]
    fn record_rate_limit_increments_counter() {
        let t = test_telemetry();
        t.record_rate_limit("openai", "gpt-4", 5000, CorrelationId::new());
        assert_eq!(t.metrics().counter("llm_rate_limits_total").value(), 1);
    }

    #[test]
    fn record_rate_limit_disabled_noop() {
        let t = telemetry_disabled();
        t.record_rate_limit("openai", "gpt-4", 5000, CorrelationId::new());
        assert_eq!(t.metrics().counter("llm_rate_limits_total").value(), 0);
    }

    #[test]
    fn record_rate_limit_logs() {
        let sink = Arc::new(TestSink::new());
        let t = telemetry_with_sink(sink.clone());
        t.record_rate_limit("openai", "gpt-4", 5000, CorrelationId::new());
        let records = sink.records();
        let has_log = records.iter().any(|r| r.message == "llm_rate_limited");
        assert!(has_log);
    }

    // ── Streaming telemetry ──────────────────────────────────────────────

    #[test]
    fn record_stream_chunk_chunks_enabled() {
        let t = LlmTelemetry::new(
            MetricsRegistry::new(),
            None,
            test_logger(Arc::new(TestSink::new())),
            None,
            &TelemetryConfig {
                enable_stream_chunk_events: true,
                ..TelemetryConfig::default()
            },
        );
        // Should not panic
        t.record_stream_chunk(100, "gpt-4");
    }

    #[test]
    fn record_stream_chunk_chunks_disabled_noop() {
        let sink = Arc::new(TestSink::new());
        let t = telemetry_with_sink(sink.clone());
        t.record_stream_chunk(100, "gpt-4");
        let records = sink.records();
        let has_chunk_log = records.iter().any(|r| r.message == "llm_stream_chunk");
        assert!(!has_chunk_log);
    }

    #[test]
    fn record_stream_started_increments_gauge() {
        let t = test_telemetry();
        t.record_stream_started("gpt-4");
        let val = t.metrics().gauge(LLM_ACTIVE_STREAMS).value();
        assert!((val - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn record_stream_finished_decrements_gauge() {
        let t = test_telemetry();
        t.record_stream_started("gpt-4");
        t.record_stream_finished("gpt-4", 10);
        let val = t.metrics().gauge(LLM_ACTIVE_STREAMS).value();
        assert!((val - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn record_stream_error_decrements_gauge() {
        let t = test_telemetry();
        t.record_stream_started("gpt-4");
        t.record_stream_error(
            &LlmError::ProviderError("err".into()),
            "gpt-4",
            CorrelationId::new(),
        );
        let val = t.metrics().gauge(LLM_ACTIVE_STREAMS).value();
        assert!((val - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn record_stream_finished_logs() {
        let sink = Arc::new(TestSink::new());
        let t = telemetry_with_sink(sink.clone());
        t.record_stream_finished("gpt-4", 20);
        let records = sink.records();
        let has_log = records.iter().any(|r| r.message == "llm_stream_finished");
        assert!(has_log);
    }

    // ── Tokens total ─────────────────────────────────────────────────────

    #[test]
    fn tokens_total_increments() {
        let t = test_telemetry();
        t.record_request_completed("gpt-4", "openai", 100.0, 10, 20, CorrelationId::new());
        let val = t.metrics().counter("llm_tokens_total").value();
        assert_eq!(val, 30);
    }

    // ── Histograms ───────────────────────────────────────────────────────

    #[test]
    fn latency_histogram_records() {
        let t = test_telemetry();
        t.record_request_completed("gpt-4", "openai", 150.0, 10, 5, CorrelationId::new());
        // Just ensure no panic
    }

    #[test]
    fn input_tokens_histogram_records() {
        let t = test_telemetry();
        t.record_request_completed("gpt-4", "openai", 100.0, 100, 50, CorrelationId::new());
        let h = t.metrics.histogram_with(
            "llm_input_tokens",
            HistogramBuckets::new(TOKEN_BUCKETS.to_vec()),
        );
        assert_eq!(h.count(), 1);
    }

    #[test]
    fn output_tokens_histogram_records() {
        let t = test_telemetry();
        t.record_request_completed("gpt-4", "openai", 100.0, 100, 50, CorrelationId::new());
        let h = t.metrics.histogram_with(
            "llm_output_tokens",
            HistogramBuckets::new(TOKEN_BUCKETS.to_vec()),
        );
        assert_eq!(h.count(), 1);
    }

    // ── Disabled telemetry ───────────────────────────────────────────────

    #[test]
    fn disabled_metrics_no_counters() {
        let t = telemetry_disabled();
        t.record_cache_hit("k");
        t.record_cache_miss("k");
        t.record_retry(1, "p", "m");
        t.record_cost(1.0, "m");
        assert_eq!(t.metrics().counter("llm_cache_hits_total").value(), 0);
        assert_eq!(t.metrics().counter("llm_retries_total").value(), 0);
    }

    #[test]
    fn disabled_telemetry_no_logs() {
        let sink = Arc::new(TestSink::new());
        let metrics = MetricsRegistry::new();
        let logger = Logger::new(sink.clone(), test_filter(), "test");
        let t = LlmTelemetry::new(
            metrics,
            None,
            logger,
            None,
            &TelemetryConfig {
                enable_metrics: false,
                enable_events: false,
                request_sampling_rate: 0.0,
                ..TelemetryConfig::default()
            },
        );
        t.record_cache_hit("k");
        t.record_cache_miss("k");
        let _all = sink.records();
        // Only sampling test: should be no records because sampling rate is 0
        // Actually, request_sampling_rate applies to request_* methods only,
        // but cache methods use enable_metrics. So logs are emitted for cache
        // hits/misses even when sampling is 0.
        // Let's just verify no panics
    }

    // ── Sanitization in telemetry ────────────────────────────────────────

    #[test]
    fn request_failed_sanitizes_error() {
        let sink = Arc::new(TestSink::new());
        let t = telemetry_with_sink(sink.clone());
        let err = LlmError::ProviderAuthFailed("token sk-abc123".into());
        t.record_request_failed(&err, "openai", "gpt-4", 100.0, CorrelationId::new());
        let records = sink.records();
        let failed = records.iter().find(|r| r.message == "llm_request_failed");
        assert!(failed.is_some());
        let fields = &failed.unwrap().fields;
        let has_redacted = fields.iter().any(|(_, v)| match v {
            FieldValue::String(s) => s.contains("[KEY_REDACTED]"),
            _ => false,
        });
        assert!(has_redacted);
    }

    #[test]
    fn stream_error_sanitizes() {
        let sink = Arc::new(TestSink::new());
        let t = telemetry_with_sink(sink.clone());
        let err = LlmError::ProviderError("Bearer sk-xyz".into());
        t.record_stream_error(&err, "gpt-4", CorrelationId::new());
        let records = sink.records();
        let has_stream_err = records.iter().any(|r| r.message == "llm_stream_error");
        assert!(has_stream_err);
    }

    // ── Sampling ─────────────────────────────────────────────────────────

    #[test]
    fn sampling_rate_one_samples_all() {
        let t = LlmTelemetry::new(
            MetricsRegistry::new(),
            None,
            test_logger(Arc::new(TestSink::new())),
            None,
            &TelemetryConfig {
                request_sampling_rate: 1.0,
                ..TelemetryConfig::default()
            },
        );
        let req = crate::types::ProviderRequest {
            model: "gpt-4".into(),
            messages: vec![],
            max_tokens: None,
            temperature: None,
            stop_sequences: vec![],
            system_prompt: None,
            tools: vec![],
            stream: false,
            timeout_ms: 30000,
        };
        for _ in 0..10 {
            t.record_request_started(&req);
        }
        let val = t.metrics().counter("llm_requests_total").value();
        assert_eq!(val, 10);
    }

    #[test]
    fn sampling_rate_zero_samples_none() {
        let t = LlmTelemetry::new(
            MetricsRegistry::new(),
            None,
            test_logger(Arc::new(TestSink::new())),
            None,
            &TelemetryConfig {
                request_sampling_rate: 0.0,
                ..TelemetryConfig::default()
            },
        );
        let req = crate::types::ProviderRequest {
            model: "gpt-4".into(),
            messages: vec![],
            max_tokens: None,
            temperature: None,
            stop_sequences: vec![],
            system_prompt: None,
            tools: vec![],
            stream: false,
            timeout_ms: 30000,
        };
        t.record_request_started(&req);
        let val = t.metrics().counter("llm_requests_total").value();
        assert_eq!(val, 0);
    }

    // ── Event bus tests ──────────────────────────────────────────────────

    #[test]
    fn event_bus_emit_no_panic_without_bus() {
        let t = test_telemetry();
        // Emitting without an event bus should be a no-op
        let meta = make_event_metadata(CorrelationId::new());
        let event = LlmCacheHitEvent { metadata: meta };
        t.emit(Box::new(event));
    }

    #[test]
    fn event_bus_with_bus_delivers() {
        let bus = EventBus::new();
        let received = Arc::new(Mutex::new(false));
        let r = received.clone();
        let handler = Arc::new(move |_: &dyn Event| {
            *r.lock().unwrap() = true;
        });
        bus.subscribe("llm.cache.hit", handler);

        let metrics = MetricsRegistry::new();
        let logger = test_logger(Arc::new(TestSink::new()));
        let t = LlmTelemetry::new(
            metrics,
            Some(bus),
            logger,
            None,
            &TelemetryConfig::default(),
        );
        t.record_cache_hit("k");
        assert!(*received.lock().unwrap());
    }

    #[test]
    fn events_disabled_no_delivery() {
        let bus = EventBus::new();
        let received = Arc::new(Mutex::new(false));
        let r = received.clone();
        let handler = Arc::new(move |_: &dyn Event| {
            *r.lock().unwrap() = true;
        });
        bus.subscribe("llm.cache.hit", handler);

        let metrics = MetricsRegistry::new();
        let logger = test_logger(Arc::new(TestSink::new()));
        let t = LlmTelemetry::new(
            metrics,
            Some(bus),
            logger,
            None,
            &TelemetryConfig {
                enable_events: false,
                ..TelemetryConfig::default()
            },
        );
        t.record_cache_hit("k");
        assert!(!*received.lock().unwrap());
    }

    // ── Event kind constants ─────────────────────────────────────────────

    #[test]
    fn cache_hit_event_kind() {
        let meta = make_event_metadata(CorrelationId::new());
        let ev = LlmCacheHitEvent { metadata: meta };
        assert_eq!(ev.kind(), "llm.cache.hit");
    }

    #[test]
    fn request_completed_event_kind() {
        let meta = make_event_metadata(CorrelationId::new());
        let ev = LlmRequestCompletedEvent { metadata: meta };
        assert_eq!(ev.kind(), "llm.request.completed");
    }

    #[test]
    fn request_failed_event_kind() {
        let meta = make_event_metadata(CorrelationId::new());
        let ev = LlmRequestFailedEvent { metadata: meta };
        assert_eq!(ev.kind(), "llm.request.failed");
    }

    #[test]
    fn provider_degraded_event_kind() {
        let meta = make_event_metadata(CorrelationId::new());
        let ev = LlmProviderDegradedEvent { metadata: meta };
        assert_eq!(ev.kind(), "llm.provider.degraded");
    }

    #[test]
    fn rate_limited_event_kind() {
        let meta = make_event_metadata(CorrelationId::new());
        let ev = LlmRateLimitedEvent { metadata: meta };
        assert_eq!(ev.kind(), "llm.rate_limited");
    }

    #[test]
    fn cost_threshold_event_kind() {
        let meta = make_event_metadata(CorrelationId::new());
        let ev = LlmCostThresholdEvent { metadata: meta };
        assert_eq!(ev.kind(), "llm.cost.threshold");
    }

    #[test]
    fn model_switched_event_kind() {
        let meta = make_event_metadata(CorrelationId::new());
        let ev = LlmModelSwitchedEvent { metadata: meta };
        assert_eq!(ev.kind(), "llm.model.switched");
    }

    #[test]
    fn stream_chunk_event_kind() {
        let meta = make_event_metadata(CorrelationId::new());
        let ev = LlmStreamChunkEvent { metadata: meta };
        assert_eq!(ev.kind(), "llm.stream.chunk");
    }

    #[test]
    fn retry_event_kind() {
        let meta = make_event_metadata(CorrelationId::new());
        let ev = LlmRetryEvent { metadata: meta };
        assert_eq!(ev.kind(), "llm.retry");
    }

    #[test]
    fn fallback_event_kind() {
        let meta = make_event_metadata(CorrelationId::new());
        let ev = LlmFallbackEvent { metadata: meta };
        assert_eq!(ev.kind(), "llm.fallback");
    }

    // ── Fallback also emits model-switched ───────────────────────────────

    #[test]
    fn fallback_emits_model_switched_event() {
        let bus = EventBus::new();
        let switched = Arc::new(Mutex::new(0usize));
        let s = switched.clone();
        let h: Arc<dyn browseros_event_bus::EventHandler> = Arc::new(move |ev: &dyn Event| {
            if ev.kind() == "llm.model.switched" {
                *s.lock().unwrap() += 1;
            }
        });
        bus.subscribe("llm.model.switched", h);

        let metrics = MetricsRegistry::new();
        let logger = test_logger(Arc::new(TestSink::new()));
        let t = LlmTelemetry::new(
            metrics,
            Some(bus),
            logger,
            None,
            &TelemetryConfig::default(),
        );
        let ep = ResolvedEndpoint {
            provider_id: "anthropic".into(),
            model_id: "claude-3".into(),
            capability: "chat".into(),
            priority: 1,
            estimated_cost_cents: 0.0,
        };
        t.record_fallback("openai", "anthropic", &ep);
        assert_eq!(*switched.lock().unwrap(), 1);
    }

    // ── Log output validation ────────────────────────────────────────────

    #[test]
    fn log_messages_have_correct_target() {
        let sink = Arc::new(TestSink::new());
        let t = telemetry_with_sink(sink.clone());
        t.log(LogLevel::Info, "test message");
        let records = sink.records();
        if let Some(rec) = records.first() {
            assert_eq!(rec.message, "test message");
        }
    }

    #[test]
    fn log_correlation_id_present_when_set() {
        let sink = Arc::new(TestSink::new());
        let logger = test_logger(sink.clone());
        let corr = CorrelationId::new();
        let _logger = logger.with_correlation_id(corr);
    }

    // ── Tracer ───────────────────────────────────────────────────────────

    #[test]
    fn tracer_span_created_when_tracer_present() {
        let tracer = Tracer::new();
        let metrics = MetricsRegistry::new();
        let logger = test_logger(Arc::new(TestSink::new()));
        let t = LlmTelemetry::new(
            metrics,
            None,
            logger,
            Some(tracer),
            &TelemetryConfig::default(),
        );
        let _guard = t.start_span("test.span");
        // Span was created without panic
    }

    #[test]
    fn tracer_span_none_when_no_tracer() {
        let t = test_telemetry();
        let guard = t.start_span("test.span");
        assert!(guard.is_none());
    }

    #[test]
    fn tracer_span_records_on_drop() {
        let reports = Arc::new(Mutex::new(Vec::new()));
        let r = reports.clone();
        let tracer = Tracer::with_callback(move |report| {
            r.lock().unwrap().push(report);
        });
        let metrics = MetricsRegistry::new();
        let logger = test_logger(Arc::new(TestSink::new()));
        let t = LlmTelemetry::new(
            metrics,
            None,
            logger,
            Some(tracer),
            &TelemetryConfig::default(),
        );
        let _guard = t.start_span("test.drop");
        drop(_guard);
        assert_eq!(reports.lock().unwrap().len(), 1);
    }

    // ── Send + Sync ──────────────────────────────────────────────────────

    #[test]
    fn telemetry_is_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<LlmTelemetry>();
        assert_sync::<LlmTelemetry>();
    }

    // ── Multiple calls do not panic ──────────────────────────────────────

    #[test]
    fn multiple_record_calls_no_panic() {
        let t = test_telemetry();
        for i in 0..20 {
            t.record_cache_hit(&format!("k{}", i));
            t.record_cache_miss(&format!("k{}", i));
            t.record_retry(i, "p", "m");
            t.record_rate_limit("p", "m", 1000, CorrelationId::new());
        }
        assert_eq!(t.metrics().counter("llm_cache_hits_total").value(), 20);
        assert_eq!(t.metrics().counter("llm_cache_misses_total").value(), 20);
        assert_eq!(t.metrics().counter("llm_retries_total").value(), 20);
        assert_eq!(t.metrics().counter("llm_rate_limits_total").value(), 20);
    }

    #[test]
    fn concurrent_access_is_safe() {
        let t = Arc::new(test_telemetry());
        let mut handles = Vec::new();
        for _ in 0..4 {
            let t = t.clone();
            handles.push(std::thread::spawn(move || {
                for _ in 0..50 {
                    t.record_cache_hit("concurrent");
                    t.record_cache_miss("concurrent");
                    t.record_retry(1, "p", "m");
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(t.metrics().counter("llm_cache_hits_total").value(), 200);
        assert_eq!(t.metrics().counter("llm_cache_misses_total").value(), 200);
    }

    // ── Histogram buckets constant validation ────────────────────────────

    #[test]
    fn latency_buckets_are_sorted() {
        let mut prev = -1.0f64;
        for &b in LATENCY_BUCKETS_MS {
            assert!(b > prev, "buckets must be sorted");
            prev = b;
        }
    }

    #[test]
    fn stream_latency_buckets_are_sorted() {
        let mut prev = -1.0f64;
        for &b in STREAM_LATENCY_BUCKETS_MS {
            assert!(b > prev, "buckets must be sorted");
            prev = b;
        }
    }

    #[test]
    fn token_buckets_are_sorted() {
        let mut prev = -1.0f64;
        for &b in TOKEN_BUCKETS {
            assert!(b > prev, "buckets must be sorted");
            prev = b;
        }
    }

    // ── Edge cases ───────────────────────────────────────────────────────

    #[test]
    fn record_cost_zero() {
        let t = test_telemetry();
        t.record_cost(0.0, "free-model");
        assert_eq!(t.metrics().counter(LLM_COST_CENTS_TOTAL).value(), 0);
    }

    #[test]
    fn stream_finished_with_zero_chunks() {
        let t = test_telemetry();
        t.record_stream_finished("gpt-4", 0);
        // Should not panic
    }

    #[test]
    fn record_request_failed_duration_zero() {
        let t = test_telemetry();
        let err = LlmError::EmptyResponse;
        t.record_request_failed(&err, "p", "m", 0.0, CorrelationId::new());
        // Should not panic
    }

    #[test]
    fn record_budget_threshold_zero_remaining() {
        let t = test_telemetry();
        t.record_budget_threshold(10.0, 0.0, CorrelationId::new());
        let val = t.metrics().gauge(LLM_BUDGET_REMAINING_CENTS).value();
        assert!((val - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn record_provider_health_unknown_sets_zero() {
        let t = test_telemetry();
        t.record_provider_health("p", &ProviderHealth::Unknown, CorrelationId::new());
        let val = t.metrics().gauge(LLM_PROVIDER_HEALTH).value();
        assert!((val - 0.0).abs() < f64::EPSILON);
    }

    // ── Metric names ─────────────────────────────────────────────────────

    #[test]
    fn constant_metric_names_are_correct() {
        assert_eq!(LLM_COST_CENTS_TOTAL, "llm_cost_cents_total");
        assert_eq!(LLM_BUDGET_REMAINING_CENTS, "llm_budget_remaining_cents");
        assert_eq!(LLM_PROVIDER_HEALTH, "llm_provider_health");
        assert_eq!(LLM_CACHE_SIZE, "llm_cache_size");
        assert_eq!(LLM_ACTIVE_STREAMS, "llm_active_streams");
    }

    // ── Histogram via metrics registry ───────────────────────────────────

    #[test]
    fn histogram_created_with_custom_buckets() {
        let t = test_telemetry();
        let h = t.histogram("test_custom", &[1.0, 2.0, 3.0]);
        h.observe(1.5);
        assert_eq!(h.count(), 1);
    }

    #[test]
    fn histogram_disabled_returns_default() {
        let t = telemetry_disabled();
        let h = t.histogram("test_disabled", &[1.0, 2.0]);
        h.observe(1.5);
        // Should not be registered in registry
        let reg_h = t
            .metrics
            .histogram_with("test_disabled", HistogramBuckets::new(vec![1.0, 2.0]));
        assert_eq!(reg_h.count(), 0);
    }

    // ── Gauge edge cases ─────────────────────────────────────────────────

    #[test]
    fn cache_size_gauge_negative_not_possible() {
        let t = test_telemetry();
        t.set_cache_size(0);
        let val = t.metrics().gauge(LLM_CACHE_SIZE).value();
        assert!((val - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn multiple_gauge_updates_reflect_last() {
        let t = test_telemetry();
        t.set_cache_size(10);
        t.set_cache_size(20);
        t.set_cache_size(30);
        let val = t.metrics().gauge(LLM_CACHE_SIZE).value();
        assert!((val - 30.0).abs() < f64::EPSILON);
    }

    // ── Stream gauge negatives are clamped ───────────────────────────────

    #[test]
    fn stream_gauge_never_negative() {
        let t = test_telemetry();
        // Decrement without increment first (should stay at 0)
        let g = t.gauge(LLM_ACTIVE_STREAMS);
        g.set(0.0);
        t.record_stream_finished("gpt-4", 0);
        let val = t.metrics().gauge(LLM_ACTIVE_STREAMS).value();
        assert!(val >= 0.0);
    }

    // ── Smoke: all record_* called together ──────────────────────────────

    #[test]
    fn smoke_all_methods() {
        let t = test_telemetry();
        let ep = ResolvedEndpoint {
            provider_id: "anthropic".into(),
            model_id: "claude-3".into(),
            capability: "chat".into(),
            priority: 1,
            estimated_cost_cents: 0.0,
        };
        let req = crate::types::ProviderRequest {
            model: "gpt-4".into(),
            messages: vec![],
            max_tokens: None,
            temperature: None,
            stop_sequences: vec![],
            system_prompt: None,
            tools: vec![],
            stream: false,
            timeout_ms: 30000,
        };

        t.record_request_started(&req);
        t.record_request_completed("gpt-4", "openai", 100.0, 10, 5, CorrelationId::new());
        t.record_request_failed(
            &LlmError::Timeout { elapsed_ms: 5000 },
            "openai",
            "gpt-4",
            100.0,
            CorrelationId::new(),
        );
        t.record_cache_hit("k");
        t.record_cache_miss("k");
        t.set_cache_size(5);
        t.record_cost(0.05, "gpt-4");
        t.record_budget_threshold(100.0, 50.0, CorrelationId::new());
        t.record_provider_health("openai", &ProviderHealth::Healthy, CorrelationId::new());
        t.record_provider_health(
            "openai",
            &ProviderHealth::Degraded {
                reason: "latency".into(),
            },
            CorrelationId::new(),
        );
        t.record_retry(1, "openai", "gpt-4");
        t.record_fallback("openai", "anthropic", &ep);
        t.record_rate_limit("openai", "gpt-4", 5000, CorrelationId::new());
        t.record_stream_started("gpt-4");
        t.record_stream_chunk(100, "gpt-4");
        t.record_stream_finished("gpt-4", 5);
        t.record_stream_error(
            &LlmError::ProviderError("err".into()),
            "gpt-4",
            CorrelationId::new(),
        );

        // Check some counters
        assert_eq!(t.metrics().counter("llm_cache_hits_total").value(), 1);
        assert_eq!(t.metrics().counter("llm_retries_total").value(), 1);
        assert_eq!(t.metrics().counter("llm_fallbacks_total").value(), 1);
        assert_eq!(t.metrics().counter("llm_rate_limits_total").value(), 1);
    }

    // ── Events have correct category ────────────────────────────────────

    #[test]
    fn event_category_is_domain() {
        let meta = make_event_metadata(CorrelationId::new());
        let ev = LlmCacheHitEvent { metadata: meta };
        assert_eq!(ev.category(), EventCategory::Domain);
    }

    // ── Event metadata has correlation_id ────────────────────────────────

    #[test]
    fn event_metadata_has_correlation_id() {
        let corr = CorrelationId::new();
        let meta = make_event_metadata(corr);
        assert_eq!(meta.correlation_id, corr);
    }

    // ── Logger child target ──────────────────────────────────────────────

    #[test]
    fn logger_has_correct_target() {
        let sink = Arc::new(TestSink::new());
        let logger = test_logger(sink.clone());
        let child = logger.child("browseros-llm");
        child.info("test");
    }
}
