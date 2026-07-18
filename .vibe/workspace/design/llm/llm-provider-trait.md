# LLM Provider Trait & Adapter Interface

**Phase:** 2.1 — Design Freeze  
**Status:** Draft  

---

## 1. `LlProvider` Trait (Definitive)

```rust
// browseros-llm/src/provider.rs

/// A provider-agnostic LLM backend adapter.
/// All provider-specific logic is encapsulated behind this trait.
/// The Gateway never knows which concrete provider it's calling.
pub trait LlProvider: Send + Sync {
    /// Unique provider identifier (e.g. "openai", "anthropic", "ollama")
    fn id(&self) -> &str;

    /// Human-readable provider name (e.g. "OpenAI", "Anthropic Claude")
    fn name(&self) -> &str;

    /// Capabilities this provider supports
    fn capabilities(&self) -> Vec<ProviderCapability>;

    /// List of model IDs this provider can serve
    fn models(&self) -> Vec<String>;

    /// Chat completion (blocking)
    fn chat(&self, request: ProviderRequest) -> Result<ProviderResponse, LlmError>;

    /// Streaming chat completion
    fn chat_stream(&self, request: ProviderRequest) -> Result<ProviderStream, LlmError>;

    /// Embedding generation
    fn embed(&self, request: ProviderEmbedRequest) -> Result<ProviderEmbedResponse, LlmError>;

    /// Health check
    fn health(&self) -> ProviderHealthResult;
}
```

---

## 2. Provider-Level Types

```rust
/// Request format AFTER Gateway→Router→Provider resolution
/// (Gateway has already resolved model, capability checks, routing)
pub struct ProviderRequest {
    pub model: String,
    pub messages: Vec<ProviderMessage>,
    pub max_tokens: Option<usize>,
    pub temperature: Option<f64>,
    pub stop_sequences: Vec<String>,
    pub system_prompt: Option<String>,
    pub tools: Vec<LlTool>,
    pub stream: bool,
    pub timeout_ms: u64,
}

/// Provider-level message (protocol-agnostic)
pub struct ProviderMessage {
    pub role: LlRole,
    pub content: LlContent,
}

/// Provider-level response (just the raw response — Gateway wraps it)
pub struct ProviderResponse {
    pub message: ProviderMessage,
    pub finish_reason: LlFinishReason,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub model: String,          // Model ID as reported by provider
}

/// Streaming response from provider
pub struct ProviderStream {
    pub receiver: crossbeam_channel::Receiver<ProviderStreamEvent>,
}

pub enum ProviderStreamEvent {
    Chunk { content: String, finish_reason: Option<LlFinishReason> },
    Done { input_tokens: u64, output_tokens: u64 },
    Error(LlmError),
}

/// Provider embedding request
pub struct ProviderEmbedRequest {
    pub model: String,
    pub input: Vec<String>,
    pub timeout_ms: u64,
}

/// Provider embedding response
pub struct ProviderEmbedResponse {
    pub embeddings: Vec<Vec<f32>>,
    pub input_tokens: u64,
    pub model: String,
}

/// Provider capability flags
pub enum ProviderCapability {
    Chat,
    Streaming,
    Embedding,
    FunctionCalling,
    Vision,
    JsonMode,
    ToolUse,
    SystemPrompt,
}

/// Provider health
pub struct ProviderHealthResult {
    pub status: ProviderHealth,
    pub latency_ms: Option<u64>,
    pub checked_at: SystemTime,
    pub error: Option<String>,
}
```

---

## 3. Adapter Pattern

Each provider adapter follows the same structure:

```
adapters/
├── mod.rs            — Re-exports
├── openai.rs         — OpenAI / Azure OpenAI
├── anthropic.rs      — Anthropic Claude
├── ollama.rs         — Ollama (local)
├── gemini.rs         — Google Gemini
└── http_generic.rs   — Generic HTTP (minimal interface)
```

Each adapter:
1. Implements `LlProvider`
2. Is configured via `LlmConfig` during `LlGateway::new()`
3. Translates between `ProviderRequest`/`ProviderResponse` and the provider's native API format
4. Uses `reqwest` for HTTP transport
5. Performs its own auth (API key in header, bearer token, etc.)
6. Reports its own token counting (or uses a shared tokenizer)

---

## 4. OpenAI Adapter (Reference Implementation)

```rust
// browseros-llm/src/adapters/openai.rs

pub struct OpenAiAdapter {
    config: ProviderConfig,
    client: reqwest::blocking::Client,
    http_client: reqwest::Client,       // For streaming
}

impl OpenAiAdapter {
    pub fn new(config: ProviderConfig) -> Result<Self, LlmError> {
        // Validate config
        // Build reqwest clients
        // Return adapter
    }
}

impl LlProvider for OpenAiAdapter {
    fn id(&self) -> &str { "openai" }
    fn name(&self) -> &str { "OpenAI" }

    fn capabilities(&self) -> Vec<ProviderCapability> {
        vec![
            ProviderCapability::Chat,
            ProviderCapability::Streaming,
            ProviderCapability::Embedding,
            ProviderCapability::FunctionCalling,
            ProviderCapability::Vision,
            ProviderCapability::JsonMode,
            ProviderCapability::SystemPrompt,
        ]
    }

    fn models(&self) -> Vec<String> {
        self.config.models.clone()
    }

    fn chat(&self, request: ProviderRequest) -> Result<ProviderResponse, LlmError> {
        // 1. Build OpenAI-native request body
        // 2. POST to https://api.openai.com/v1/chat/completions
        // 3. Parse response
        // 4. Map to ProviderResponse
        // 5. Handle errors (rate limit, auth, etc.)
    }

    fn chat_stream(&self, request: ProviderRequest) -> Result<ProviderStream, LlmError> {
        // 1. Build request body with stream: true
        // 2. POST with reqwest (non-blocking client)
        // 3. Parse SSE events
        // 4. Push to crossbeam channel
        // 5. Return ProviderStream with receiver
    }

    fn embed(&self, request: ProviderEmbedRequest) -> Result<ProviderEmbedResponse, LlmError> {
        // 1. Build OpenAI embedding request
        // 2. POST to https://api.openai.com/v1/embeddings
        // 3. Parse response
    }

    fn health(&self) -> ProviderHealthResult {
        // GET https://api.openai.com/v1/models
        // Return status based on response
    }
}
```

---

## 5. Anthropic Adapter (Variant Pattern)

Anthropic differs from OpenAI in:
- Message format: `content` is an array of content blocks (not a single string)
- Roles: `user`/`assistant` only (no `system` — system prompt is a separate field)
- Function calling uses `tool_use`/`tool_result` content blocks
- No native embedding endpoint
- Different error response format

```rust
// browseros-llm/src/adapters/anthropic.rs

impl LlProvider for AnthropicAdapter {
    fn capabilities(&self) -> Vec<ProviderCapability> {
        vec![
            ProviderCapability::Chat,
            ProviderCapability::Streaming,
            ProviderCapability::ToolUse,
            ProviderCapability::Vision,
            // NO Embedding, NO FunctionCalling (uses ToolUse instead)
        ]
    }

    fn chat(&self, request: ProviderRequest) -> Result<ProviderResponse, LlmError> {
        // Map ProviderRequest → Anthropic Messages API format
        // - LlContent::ToolCall → content type "tool_use"
        // - LlContent::ToolResult → content type "tool_result"
        // - system_prompt goes to top-level "system" field, not messages
        // POST https://api.anthropic.com/v1/messages
        // Map response → ProviderResponse
    }
}
```

---

## 6. Ollama Adapter (Local Pattern)

Ollama runs locally with no auth. Uses OpenAI-compatible API at `http://localhost:11434`.

```rust
// browseros-llm/src/adapters/ollama.rs

impl LlProvider for OllamaAdapter {
    fn capabilities(&self) -> Vec<ProviderCapability> {
        vec![
            ProviderCapability::Chat,
            ProviderCapability::Streaming,
            // NO Embedding, NO FunctionCalling (Ollama has limited FC)
        ]
    }

    fn chat(&self, request: ProviderRequest) -> Result<ProviderResponse, LlmError> {
        // POST http://localhost:11434/api/chat
        // Model names are different (e.g., "llama3.2", "mistral")
        // Map to ProviderResponse
    }
}
```

---

## 7. Generic HTTP Adapter (Minimal Contract)

For providers that don't have a dedicated adapter yet. Expects the user to configure:

```toml
[llm.providers.custom]
type = "http_generic"
base_url = "https://my-llm-server.example.com"
chat_path = "/v1/chat/completions"
embed_path = "/v1/embeddings"
auth_header = "Bearer sk-..."
format = "openai"  # or "custom"
```

```rust
// browseros-llm/src/adapters/http_generic.rs

pub struct HttpGenericAdapter {
    config: HttpGenericConfig,
    client: reqwest::blocking::Client,
}

impl LlProvider for HttpGenericAdapter {
    fn capabilities(&self) -> Vec<ProviderCapability> {
        let mut caps = vec![ProviderCapability::Chat];
        if self.config.supports_stream { caps.push(ProviderCapability::Streaming); }
        if self.config.supports_embed { caps.push(ProviderCapability::Embedding); }
        caps
    }

    fn chat(&self, request: ProviderRequest) -> Result<ProviderResponse, LlmError> {
        // Build request based on configured format (OpenAI-compatible or custom)
        // POST to base_url + chat_path
        // Parse response based on format
    }
}
```

---

## 8. Provider Registration

Providers are registered at Gateway construction time:

```rust
impl LlGateway {
    pub fn new(
        providers: Vec<Box<dyn LlProvider>>,
        config: LlmConfig,
        ...
    ) -> Result<Self, LlmError> {
        let registry = ModelRegistry::new(&config.models, &providers)?;
        let router = LlRouter::new(&config, &registry)?;

        Ok(Self {
            router: Arc::new(router),
            registry: Arc::new(registry),
            // ...
        })
    }
}

impl ModelRegistry {
    pub fn new(
        models: &[ModelConfig],
        providers: &[Box<dyn LlProvider>],
    ) -> Result<Self, LlmError> {
        // For each model in config:
        // 1. Find matching provider by ID
        // 2. Verify model is in provider.models()
        // 3. Register model with provider handle + metadata
    }
}
```

---

## 9. Adding a New Provider (Checklist)

To add a new provider (e.g., Cohere, Mistral AI, DeepSeek):

1. Create `adapters/cohere.rs`
2. Implement `LlProvider` trait
3. Map `ProviderRequest` → Cohere API format
4. Map Cohere response → `ProviderResponse`
5. Add provider config to `LlmConfig`
6. Register in `LlGateway::new()`
7. Done — no changes to Gateway, Router, or Planner
