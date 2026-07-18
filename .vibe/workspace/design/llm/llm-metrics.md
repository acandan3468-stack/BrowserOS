# LLM Observability

**Phase:** 2.1 — Design Freeze  
**Status:** Draft  

---

## 1. Metrics (Prometheus via browseros-observability)

All metrics follow the existing `MetricsRegistry` pattern from `browseros-observability`.

### Counters

| Metric Name | Type | Labels | Description |
|---|---|---|---|
| `llm_requests_total` | Counter | provider, model, capability, status | Total LLM requests |
| `llm_tokens_total` | Counter | provider, model, direction (input/output) | Total token count |
| `llm_cost_cents_total` | Counter | provider, model | Total estimated cost |
| `llm_cache_hits_total` | Counter | provider, cache_type | Cache hits |
| `llm_cache_misses_total` | Counter | provider, cache_type | Cache misses |
| `llm_failures_total` | Counter | provider, error_code | Request failures |
| `llm_retries_total` | Counter | provider | Retry attempts |
| `llm_fallbacks_total` | Counter | from_provider, to_provider | Fallback activations |
| `llm_rate_limits_total` | Counter | provider | Rate limit hits |

### Histograms

| Metric Name | Labels | Buckets | Description |
|---|---|---|---|
| `llm_request_duration_ms` | provider, model, capability | [100, 500, 1000, 3000, 10000, 30000] | Request latency |
| `llm_input_tokens` | provider, model | [100, 500, 1000, 5000, 10000, 50000] | Input token distribution |
| `llm_output_tokens` | provider, model | [100, 500, 1000, 5000, 10000, 50000] | Output token distribution |
| `llm_stream_chunk_latency_ms` | provider, model | [10, 50, 100, 500, 1000] | Time between stream chunks |

### Gauges

| Metric Name | Labels | Description |
|---|---|---|
| `llm_provider_health` | provider | 0=unknown, 1=healthy, 2=degraded, 3=unavailable |
| `llm_active_streams` | provider, model | Currently active streaming connections |
| `llm_cache_size` | provider | Current cache entry count |
| `llm_budget_remaining_cents` | — | Remaining monthly budget |
| `llm_concurrent_requests` | provider | Current in-flight requests |

---

## 2. Tracing (via browseros-observability::Tracer)

Each LLM request creates a span:

```
LlmGateway.chat
  ├── Router.resolve          [capability resolution time]
  ├── Cache.lookup            [cache check time]
  ├── Provider.chat           [provider call time]
  │     ├── HTTP.retry_1      [if retry]
  │     ├── HTTP.retry_2      [if retry]
  │     └── HTTP.response     [actual response]
  ├── CostTracker.record      [cost calculation time]
  └── Telemetry.emit          [event publication time]
```

Spans include:
- `correlation_id`
- `provider_id`, `model_id`
- `capability`
- `input_tokens`, `output_tokens`
- `latency_ms`
- `error_code` (if failed)

---

## 3. Logging

```rust
// Structured log events
log.info("llm_request_started", [
    "provider" => "openai",
    "model" => "gpt-4o",
    "capability" => "planning",
    "tokens_in" => 1523,
]);

log.info("llm_request_completed", [
    "provider" => "openai",
    "model" => "gpt-4o",
    "latency_ms" => 2340,
    "tokens_in" => 1523,
    "tokens_out" => 456,
    "cost_cents" => 0.84,
]);

log.warn("llm_fallback", [
    "from" => "openai/gpt-4o",
    "to" => "anthropic/claude-3-sonnet",
    "reason" => "rate_limited",
]);

log.error("llm_request_failed", [
    "provider" => "ollama",
    "model" => "llama3.2",
    "error" => "connection_refused",
    "retries" => 3,
]);
```

---

## 4. LlmTelemetry — Internal Aggregator

```rust
// browseros-llm/src/telemetry.rs

pub struct LlmTelemetry {
    metrics: Arc<MetricsRegistry>,
    event_bus: Option<Arc<EventBus>>,
    logger: Arc<dyn Logger>,
}

impl LlmTelemetry {
    pub fn record_chat(
        &self,
        provider: &str,
        model: &str,
        tokens_in: u64,
        tokens_out: u64,
        latency_ms: u64,
        success: bool,
    ) {
        self.metrics.counter("llm_requests_total")
            .with_label("provider", provider)
            .with_label("model", model)
            .with_label("status", if success { "success" } else { "failure" })
            .inc();

        self.metrics.histogram("llm_request_duration_ms")
            .with_label("provider", provider)
            .with_label("model", model)
            .observe(latency_ms as f64);

        self.metrics.counter("llm_tokens_total")
            .with_label("direction", "input")
            .with_label("model", model)
            .add(tokens_in);

        self.metrics.counter("llm_tokens_total")
            .with_label("direction", "output")
            .with_label("model", model)
            .add(tokens_out);
    }

    pub fn record_cache_hit(&self, cache_type: &str, saved_ms: u64) {
        self.metrics.counter("llm_cache_hits_total")
            .with_label("cache_type", cache_type)
            .inc();
    }

    pub fn record_provider_health(&self, provider: &str, status: ProviderHealth) {
        let value = match status {
            ProviderHealth::Healthy => 1,
            ProviderHealth::Degraded { .. } => 2,
            ProviderHealth::Unavailable { .. } => 3,
            ProviderHealth::Unknown => 0,
        };
        self.metrics.gauge("llm_provider_health")
            .with_label("provider", provider)
            .set(value);
    }

    pub fn record_cost(&self, model: &str, cost_cents: f64) {
        self.metrics.counter("llm_cost_cents_total")
            .with_label("model", model)
            .add(cost_cents);
    }
}
```

---

## 5. Telemetry Configuration

```toml
[llm.telemetry]
enable_metrics = true
enable_events = true
log_level = "info"
# Sampling: log 10% of requests for debugging
request_sampling_rate = 0.1
# Emit per-chunk events (can be noisy)
enable_stream_chunk_events = false
```
