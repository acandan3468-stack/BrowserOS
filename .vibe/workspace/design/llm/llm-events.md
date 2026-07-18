# LLM Event Model

**Phase:** 2.1 — Design Freeze  
**Status:** Draft  

---

## 1. LLM-Specific Event Taxonomy

All LLM events follow the existing `Event` pattern from `browseros-event-bus`:

```rust
pub struct Event {
    pub event_type: EventType,
    pub metadata: EventMetadata,
    pub payload: EventPayload,
}
```

New `EventType` variant:
```rust
// In browseros-types
#[non_exhaustive]
pub enum EventType {
    // ... existing variants ...
    
    // LLM events (Phase 2.1)
    LlmRequestStarted,
    LlmRequestCompleted,
    LlmRequestFailed,
    LlmCacheHit,
    LlmProviderDegraded,
    LlmRateLimited,
    LlmCostThreshold,
    LlmModelSwitched,
    LlmStreamChunk,
}
```

---

## 2. Event Payloads

```rust
// All payloads are in browseros-llm/src/telemetry.rs (not in browseros-types)
// They implement EventPayload trait

/// Emitted when any LLM request starts
pub struct LlmRequestStartedPayload {
    pub correlation_id: CorrelationId,
    pub provider_id: String,
    pub model_id: String,
    pub capability: String,
    pub input_tokens: Option<u64>,
    pub request_type: LlmRequestType,        // Chat, Stream, Embed
    pub timestamp: SystemTime,
}

/// Emitted on successful LLM request completion
pub struct LlmRequestCompletedPayload {
    pub correlation_id: CorrelationId,
    pub provider_id: String,
    pub model_id: String,
    pub latency_ms: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cost_estimate_cents: f64,
    pub finish_reason: String,
    pub was_fallback: bool,
    pub retry_count: u32,
    pub timestamp: SystemTime,
}

/// Emitted when an LLM request fails
pub struct LlmRequestFailedPayload {
    pub correlation_id: CorrelationId,
    pub provider_id: String,
    pub model_id: String,
    pub error_code: String,
    pub error_message: String,
    pub retry_count: u32,
    pub was_fallback: bool,
    pub latency_ms: u64,
    pub timestamp: SystemTime,
}

/// Emitted on cache hit (response or embedding)
pub struct LlmCacheHitPayload {
    pub correlation_id: CorrelationId,
    pub cache_key: String,
    pub model_id: String,
    pub cache_type: LlmCacheType,      // Response, Embedding
    pub saved_latency_estimate_ms: u64,
    pub timestamp: SystemTime,
}

/// Emitted when a provider becomes degraded
pub struct LlmProviderDegradedPayload {
    pub provider_id: String,
    pub previous_status: String,
    pub new_status: String,
    pub consecutive_failures: u32,
    pub error_sample: Option<String>,
    pub timestamp: SystemTime,
}

/// Emitted when rate limited
pub struct LlmRateLimitedPayload {
    pub provider_id: String,
    pub model_id: Option<String>,
    pub retry_after_ms: u64,
    pub fallback_used: bool,
    pub timestamp: SystemTime,
}

/// Emitted when cost exceeds threshold
pub struct LlmCostThresholdPayload {
    pub current_cost_cents: f64,
    pub threshold_cents: f64,
    pub provider_id: String,
    pub model_id: String,
    pub period_start: SystemTime,
    pub timestamp: SystemTime,
}

/// Emitted when router switches model/provider mid-request
pub struct LlmModelSwitchedPayload {
    pub correlation_id: CorrelationId,
    pub from_provider: String,
    pub from_model: String,
    pub to_provider: String,
    pub to_model: String,
    pub reason: String,                // "fallback", "rate_limited", "timeout"
    pub timestamp: SystemTime,
}

/// Emitted per streaming chunk
pub struct LlmStreamChunkPayload {
    pub correlation_id: CorrelationId,
    pub provider_id: String,
    pub model_id: String,
    pub chunk_index: usize,
    pub content_length: usize,
    pub timestamp: SystemTime,
}
```

---

## 3. Event Enum

```rust
// browseros-llm/src/telemetry.rs

#[non_exhaustive]
pub enum LlmEvent {
    RequestStarted(Box<LlmRequestStartedPayload>),
    RequestCompleted(Box<LlmRequestCompletedPayload>),
    RequestFailed(Box<LlmRequestFailedPayload>),
    CacheHit(Box<LlmCacheHitPayload>),
    ProviderDegraded(Box<LlmProviderDegradedPayload>),
    RateLimited(Box<LlmRateLimitedPayload>),
    CostThreshold(Box<LlmCostThresholdPayload>),
    ModelSwitched(Box<LlmModelSwitchedPayload>),
    StreamChunk(Box<LlmStreamChunkPayload>),
}

impl EventPayload for LlmEvent {
    fn event_type(&self) -> EventType {
        match self {
            LlmEvent::RequestStarted(_) => EventType::LlmRequestStarted,
            LlmEvent::RequestCompleted(_) => EventType::LlmRequestCompleted,
            LlmEvent::RequestFailed(_) => EventType::LlmRequestFailed,
            LlmEvent::CacheHit(_) => EventType::LlmCacheHit,
            LlmEvent::ProviderDegraded(_) => EventType::LlmProviderDegraded,
            LlmEvent::RateLimited(_) => EventType::LlmRateLimited,
            LlmEvent::CostThreshold(_) => EventType::LlmCostThreshold,
            LlmEvent::ModelSwitched(_) => EventType::LlmModelSwitched,
            LlmEvent::StreamChunk(_) => EventType::LlmStreamChunk,
        }
    }
}
```

---

## 4. Event Emission Points

| Event | Emitted By | When |
|---|---|---|
| RequestStarted | Gateway::chat/embed | Before provider call |
| RequestCompleted | Gateway::chat/embed | After successful response |
| RequestFailed | Gateway::chat/embed | After final failure (all retries exhausted) |
| CacheHit | Gateway::chat/embed | Before provider call, on cache match |
| ProviderDegraded | HealthTracker | On health check failure |
| RateLimited | Router | On 429 response |
| CostThreshold | CostTracker | On monthly budget threshold breach |
| ModelSwitched | Router | On fallback activation |
| StreamChunk | Gateway::chat_stream | Per chunk received |

---

## 5. Event Bus Integration

```rust
impl LlGateway {
    fn emit(&self, event: LlmEvent) {
        if let Some(bus) = self.event_bus.as_ref() {
            let _ = bus.publish(Event::new(
                event.event_type(),
                EventMetadata::new(self.correlation_id),
                EventPayload::Llm(Box::new(event)),
            ));
        }
    }
}
```

Events can be subscribed to by:
- **Logger**: Log all LLM operations
- **Metrics**: Update counters, histograms
- **CostTracker**: Real-time cost monitoring
- **HealthTracker**: Detect provider degradation patterns
- **External monitoring**: Prometheus, Datadog, etc.
