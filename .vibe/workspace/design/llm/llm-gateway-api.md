# LLM Gateway Public API

**Phase:** 2.1 — Design Freeze  
**Status:** Draft  

---

## 1. `LlGateway` — Main Entry Point

```rust
// browseros-llm/src/gateway.rs

pub struct LlGateway {
    router: Arc<LlRouter>,
    registry: Arc<ModelRegistry>,
    cache: Arc<LlCache>,
    cost: Arc<CostTracker>,
    telemetry: Arc<LlmTelemetry>,
    config: LlmConfig,
}

impl LlGateway {
    /// Create a new Gateway with registered providers and configuration
    pub fn new(
        providers: Vec<Box<dyn LlProvider>>,
        config: LlmConfig,
        event_bus: Arc<EventBus>,
        metrics: Arc<MetricsRegistry>,
        logger: Arc<dyn Logger>,
    ) -> Result<Self, LlmError>;

    /// Core chat completion — synchronous
    pub fn chat(&self, request: LlRequest) -> Result<LlResponse, LlmError>;

    /// Streaming chat completion — returns a stream handle
    pub fn chat_stream(&self, request: LlRequest) -> Result<LlStreamHandle, LlmError>;

    /// Embedding generation
    pub fn embed(&self, request: LlEmbedRequest) -> Result<LlEmbedResponse, LlmError>;

    /// Resolve which model/provider will handle a given capability + content
    pub fn resolve(
        &self,
        capability: &str,
        hints: &[String],
    ) -> Result<ResolvedEndpoint, LlmError>;

    /// Health check for all registered providers
    pub fn health(&self) -> Vec<ProviderHealthReport>;

    /// Clear response cache
    pub fn clear_cache(&self);

    /// Get current cost snapshot
    pub fn cost_snapshot(&self) -> CostSnapshot;
}
```

---

## 2. Core Types

```rust
// browseros-llm/src/types.rs

/// A chat completion request
pub struct LlRequest {
    pub messages: Vec<LlMessage>,
    pub model: Option<String>,          // Specific model override
    pub capability: Option<String>,     // "planning", "code", "reasoning"
    pub max_tokens: Option<usize>,
    pub temperature: Option<f64>,
    pub stop_sequences: Vec<String>,
    pub system_prompt: Option<String>,
    pub tools: Vec<LlTool>,
    pub stream: bool,
    pub correlation_id: CorrelationId,
}

/// A single message in the conversation
pub struct LlMessage {
    pub role: LlRole,
    pub content: LlContent,
    pub name: Option<String>,
}

/// Message roles — provider-agnostic
pub enum LlRole {
    System,
    User,
    Assistant,
    Tool,
}

/// Message content (supports multi-modal)
pub enum LlContent {
    Text(String),
    Image {
        mime_type: String,
        data: Vec<u8>,                   // raw bytes or base64
    },
    ToolResult {
        call_id: String,
        output: String,
    },
    ToolCall {
        call_id: String,
        name: String,
        arguments: HashMap<String, serde_json::Value>,
    },
}

/// Tool definition for function calling
pub struct LlTool {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,  // JSON Schema object
    pub strict: bool,
}

/// Structured chat response
pub struct LlResponse {
    pub message: LlMessage,
    pub finish_reason: LlFinishReason,
    pub usage: LlUsage,
    pub model: String,                  // Actual model used
    pub provider: String,               // Actual provider used
}

/// Token usage accounting
pub struct LlUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cost_estimate_cents: f64,
    pub currency: String,               // "USD", "EUR", etc.
}

/// Why the generation stopped
pub enum LlFinishReason {
    Stop,
    Length,
    ToolCalls,
    ContentFilter,
    Error(String),
}

/// Embedding request
pub struct LlEmbedRequest {
    pub input: Vec<String>,
    pub model: Option<String>,
    pub correlation_id: CorrelationId,
}

/// Embedding response
pub struct LlEmbedResponse {
    pub embeddings: Vec<Vec<f32>>,
    pub dimensions: usize,
    pub model: String,
    pub usage: LlUsage,
}
```

---

## 3. Streaming Types

```rust
// browseros-llm/src/streaming.rs

/// Handle to a streaming response (Sync-friendly)
pub struct LlStreamHandle {
    pub model: String,
    pub provider: String,
    receiver: crossbeam_channel::Receiver<LlStreamEvent>,
}

/// Events from a streaming response
pub enum LlStreamEvent {
    Chunk(LlStreamChunk),
    Done(LlUsage),
    Error(LlmError),
}

/// A single streaming chunk
pub struct LlStreamChunk {
    pub content: String,
    pub finish_reason: Option<LlFinishReason>,
    pub tool_calls: Vec<LlToolCallDelta>,
    pub index: usize,
}

impl LlStreamHandle {
    /// Blocking receive — returns next event or disconnected
    pub fn recv(&self) -> Result<LlStreamEvent, LlmError>;

    /// Non-blocking try_receive
    pub fn try_recv(&self) -> Result<Option<LlStreamEvent>, LlmError>;

    /// Iterator interface
    pub fn iter(&self) -> LlStreamIterator<'_>;
}
```

---

## 4. Error Types

```rust
// browseros-llm/src/error.rs

#[non_exhaustive]
pub enum LlmError {
    // Provider errors
    ProviderError(String),
    ProviderUnavailable(String),
    ProviderRateLimited { retry_after_ms: u64 },
    ProviderAuthFailed(String),

    // Request errors
    InvalidRequest(String),
    ContextTooLong { current: usize, max: usize },
    ModelNotFound(String),
    CapabilityNotSupported(String),

    // Response errors
    EmptyResponse,
    ContentFiltered,
    MalformedResponse(String),

    // System errors
    Timeout { elapsed_ms: u64 },
    AllProvidersFailed { attempts: Vec<String> },
    CacheError(String),
    ConfigurationError(String),
    TransportError(String),
    StreamingUnsupported,
}
```

---

## 5. Resolver Types

```rust
/// Result of capability→provider resolution
pub struct ResolvedEndpoint {
    pub provider_id: String,
    pub model_id: String,
    pub capability: String,
    pub priority: u32,
    pub estimated_cost_cents: f64,
}

/// Provider health snapshot
pub struct ProviderHealthReport {
    pub provider_id: String,
    pub status: ProviderHealth,
    pub models: Vec<String>,
    pub latency_p50_ms: u64,
    pub error_rate: f64,
    pub last_check: SystemTime,
}

pub enum ProviderHealth {
    Healthy,
    Degraded { reason: String },
    Unavailable { since: SystemTime },
    Unknown,
}
```

---

## 6. Cost Tracking Types

```rust
/// Snapshot of cost tracking state
pub struct CostSnapshot {
    pub total_tokens_in: u64,
    pub total_tokens_out: u64,
    pub total_cost_cents: f64,
    pub cost_by_model: HashMap<String, f64>,
    pub cost_by_provider: HashMap<String, f64>,
    pub budget_cents: Option<u64>,
    pub budget_remaining_cents: Option<f64>,
    pub session_start: SystemTime,
}
```

---

## 7. Gateway Construction Example

```rust
// In browseros-runtime (during context initialization)

let llm_config: LlmConfig = config_store.get("llm")?;

let providers: Vec<Box<dyn LlProvider>> = vec![
    Box::new(OpenAiAdapter::new(llm_config.provider("openai"))?),
    Box::new(AnthropicAdapter::new(llm_config.provider("anthropic"))?),
    Box::new(OllamaAdapter::new(llm_config.provider("ollama"))?),
];

let gateway = LlGateway::new(
    providers,
    llm_config,
    event_bus.clone(),
    metrics_registry.clone(),
    logger.clone(),
)?;

context_builder.llm_gateway(gateway);
```

---

## 8. Consumer Interface Example (Planner)

```rust
// In browseros-dag (LLMPlanner — NEW, in dag crate's planner module)

impl PlannerBridge for LLMPlanner {
    fn plan(&self, request: PlannerRequest) -> PlanningResult {
        let llm_request = LlRequest {
            messages: vec![
                LlMessage::system(self.system_prompt.clone()),
                LlMessage::user(self.format_request(&request)),
            ],
            capability: Some("planning".into()),
            tools: self.get_planning_tools(),
            correlation_id: request.context.correlation_id,
            ..Default::default()
        };

        let response = self.gateway.chat(llm_request)?;

        ExecutionPlan::from_llm_response(response)
    }
}
```
