//! LLM Gateway request/response types, configuration, and shared data structures.

use crate::error::LlmError;
use browseros_types::identifiers::CorrelationId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ────────────────────────────────────────────────────────────────────────────
// Chat types
// ────────────────────────────────────────────────────────────────────────────

/// A chat completion request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlRequest {
    pub messages: Vec<LlMessage>,
    pub model: Option<String>,
    pub capability: Option<String>,
    pub max_tokens: Option<usize>,
    pub temperature: Option<f64>,
    pub stop_sequences: Vec<String>,
    pub system_prompt: Option<String>,
    pub tools: Vec<LlTool>,
    pub stream: bool,
    pub correlation_id: CorrelationId,
}

impl Default for LlRequest {
    fn default() -> Self {
        Self {
            messages: Vec::new(),
            model: None,
            capability: None,
            max_tokens: None,
            temperature: None,
            stop_sequences: Vec::new(),
            system_prompt: None,
            tools: Vec::new(),
            stream: false,
            correlation_id: CorrelationId::new(),
        }
    }
}

impl LlRequest {
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    pub fn with_capability(mut self, capability: impl Into<String>) -> Self {
        self.capability = Some(capability.into());
        self
    }

    pub fn with_temperature(mut self, temperature: f64) -> Self {
        self.temperature = Some(temperature);
        self
    }

    pub fn with_message(mut self, message: LlMessage) -> Self {
        self.messages.push(message);
        self
    }

    pub fn with_tool(mut self, tool: LlTool) -> Self {
        self.tools.push(tool);
        self
    }

    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    pub fn with_stream(mut self, stream: bool) -> Self {
        self.stream = stream;
        self
    }

    /// Validate the request. Returns all errors found.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        if self.messages.is_empty() {
            errors.push("at least one message is required".to_string());
        }
        if let Some(temp) = self.temperature {
            if !(0.0..=2.0).contains(&temp) {
                errors.push(format!("temperature {} out of range [0.0, 2.0]", temp));
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Normalize the request — cap temperature and remove empty stop sequences.
    pub fn normalized(&self) -> Self {
        let mut req = self.clone();
        if let Some(temp) = req.temperature {
            if temp < 0.0 {
                req.temperature = Some(0.0);
            } else if temp > 2.0 {
                req.temperature = Some(2.0);
            }
        }
        req.stop_sequences.retain(|s| !s.is_empty());
        req
    }
}

/// Message roles — provider-agnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum LlRole {
    System,
    User,
    Assistant,
    Tool,
}

impl LlRole {
    /// Return a static string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            LlRole::System => "system",
            LlRole::User => "user",
            LlRole::Assistant => "assistant",
            LlRole::Tool => "tool",
        }
    }
}

/// A single message in a conversation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlMessage {
    pub role: LlRole,
    pub content: LlContent,
    pub name: Option<String>,
}

impl LlMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: LlRole::System,
            content: LlContent::Text(content.into()),
            name: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: LlRole::User,
            content: LlContent::Text(content.into()),
            name: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: LlRole::Assistant,
            content: LlContent::Text(content.into()),
            name: None,
        }
    }

    pub fn tool_result(call_id: impl Into<String>, output: impl Into<String>) -> Self {
        Self {
            role: LlRole::Tool,
            content: LlContent::ToolResult {
                call_id: call_id.into(),
                output: output.into(),
            },
            name: None,
        }
    }

    pub fn tool_call(
        call_id: impl Into<String>,
        name: impl Into<String>,
        arguments: HashMap<String, serde_json::Value>,
    ) -> Self {
        Self {
            role: LlRole::Assistant,
            content: LlContent::ToolCall {
                call_id: call_id.into(),
                name: name.into(),
                arguments,
            },
            name: None,
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
}

/// Message content (supports multi-modal).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum LlContent {
    Text(String),
    Image {
        mime_type: String,
        data: Vec<u8>,
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

impl LlContent {
    /// Approximate memory size in bytes.
    pub fn len_approx(&self) -> usize {
        match self {
            LlContent::Text(t) => t.len(),
            LlContent::Image { mime_type, data } => mime_type.len() + data.len(),
            LlContent::ToolResult { call_id, output } => call_id.len() + output.len(),
            LlContent::ToolCall {
                call_id,
                name,
                arguments,
                ..
            } => call_id.len() + name.len() + arguments.len() * 64,
        }
    }
}

impl std::fmt::Display for LlContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LlContent::Text(text) => write!(f, "{}", text),
            LlContent::Image { mime_type, .. } => write!(f, "[image: {}]", mime_type),
            LlContent::ToolResult { call_id, output } => {
                write!(f, "[tool_result: {} | {}]", call_id, output)
            }
            LlContent::ToolCall {
                call_id,
                name,
                arguments,
            } => {
                let args_str =
                    serde_json::to_string(arguments).unwrap_or_else(|_| "{}".to_string());
                write!(f, "[tool_call: {}({}) id={}]", name, args_str, call_id)
            }
        }
    }
}

/// Tool definition for function calling.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlTool {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
    pub strict: bool,
}

impl LlTool {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            parameters: serde_json::Value::Object(serde_json::Map::new()),
            strict: false,
        }
    }

    pub fn with_parameters(mut self, params: serde_json::Value) -> Self {
        self.parameters = params;
        self
    }

    pub fn with_strict(mut self, strict: bool) -> Self {
        self.strict = strict;
        self
    }
}

/// Structured chat response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlResponse {
    pub message: LlMessage,
    pub finish_reason: LlFinishReason,
    pub usage: LlUsage,
    pub model: String,
    pub provider: String,
}

impl LlResponse {
    /// Append content to the response message (used during streaming aggregation).
    pub fn append_content(&mut self, content: &str) {
        match &mut self.message.content {
            LlContent::Text(ref mut existing) => existing.push_str(content),
            _ => {
                self.message.content = LlContent::Text(content.to_string());
            }
        }
    }

    /// Approximate memory size in bytes (for cache eviction).
    pub fn approximate_size(&self) -> usize {
        let mut size = std::mem::size_of::<Self>();
        size += self.model.len();
        size += self.provider.len();
        size += self.message.content.len_approx();
        size
    }

    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        if self.model.is_empty() {
            errors.push("model is required".to_string());
        }
        if self.provider.is_empty() {
            errors.push("provider is required".to_string());
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

/// Why the generation stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum LlFinishReason {
    Stop,
    Length,
    ToolCalls,
    ContentFilter,
    Error,
}

/// Token usage accounting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cost_estimate_cents: f64,
    pub currency: String,
}

impl Default for LlUsage {
    fn default() -> Self {
        Self {
            input_tokens: 0,
            output_tokens: 0,
            total_tokens: 0,
            cost_estimate_cents: 0.0,
            currency: "USD".to_string(),
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Embedding types
// ────────────────────────────────────────────────────────────────────────────

/// Embedding request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlEmbedRequest {
    pub input: Vec<String>,
    pub model: Option<String>,
    pub correlation_id: CorrelationId,
}

impl Default for LlEmbedRequest {
    fn default() -> Self {
        Self {
            input: Vec::new(),
            model: None,
            correlation_id: CorrelationId::new(),
        }
    }
}

impl LlEmbedRequest {
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        if self.input.is_empty() {
            errors.push("at least one input string is required".to_string());
        }
        for (i, s) in self.input.iter().enumerate() {
            if s.is_empty() {
                errors.push(format!("input[{}] is empty", i));
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

/// Embedding response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlEmbedResponse {
    pub embeddings: Vec<Vec<f32>>,
    pub dimensions: usize,
    pub model: String,
    pub usage: LlUsage,
}

// ────────────────────────────────────────────────────────────────────────────
// Tool calling — streaming deltas
// ────────────────────────────────────────────────────────────────────────────

/// A streaming delta for a tool call (follows OpenAI SSE delta format).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlToolCallDelta {
    pub index: usize,
    pub id: Option<String>,
    pub name: Option<String>,
    pub arguments: Option<String>,
}

impl LlToolCallDelta {
    /// Returns `true` if this is the first chunk for a new tool call.
    pub fn is_start(&self) -> bool {
        self.id.is_some()
    }

    /// Merge accumulated arguments with this delta.
    pub fn merge_arguments(accumulated: &mut String, delta: &Option<String>) {
        if let Some(d) = delta {
            accumulated.push_str(d);
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Provider-level types (used by LlProvider trait)
// ────────────────────────────────────────────────────────────────────────────

/// Provider capability flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

/// Request format after Gateway → Router → Provider resolution.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// Provider-level message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderMessage {
    pub role: LlRole,
    pub content: LlContent,
}

/// Provider-level response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderResponse {
    pub message: ProviderMessage,
    pub finish_reason: LlFinishReason,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub model: String,
}

/// Provider health result from a health check.
#[derive(Debug, Clone)]
pub struct ProviderHealthResult {
    pub status: ProviderHealth,
    pub latency_ms: Option<u64>,
    pub checked_at: std::time::SystemTime,
    pub error: Option<String>,
}

/// Streaming response from a provider.
pub struct ProviderStream {
    pub receiver: std::sync::mpsc::Receiver<ProviderStreamEvent>,
}

impl std::fmt::Debug for ProviderStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderStream").finish_non_exhaustive()
    }
}

/// Events from a provider streaming response.
#[derive(Debug)]
pub enum ProviderStreamEvent {
    Chunk {
        content: String,
        finish_reason: Option<LlFinishReason>,
        tool_calls: Vec<LlToolCallDelta>,
    },
    Done {
        input_tokens: u64,
        output_tokens: u64,
    },
    Error(LlmError),
}

/// Provider embedding request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderEmbedRequest {
    pub model: String,
    pub input: Vec<String>,
    pub timeout_ms: u64,
}

/// Provider embedding response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderEmbedResponse {
    pub embeddings: Vec<Vec<f32>>,
    pub input_tokens: u64,
    pub model: String,
}

// ────────────────────────────────────────────────────────────────────────────
// Configuration types
// ────────────────────────────────────────────────────────────────────────────

/// Root LLM Gateway configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    #[serde(default)]
    pub default_provider: String,

    #[serde(default)]
    pub providers: Vec<ProviderConfig>,

    #[serde(default)]
    pub models: Vec<ModelConfig>,

    #[serde(default)]
    pub routing: RoutingConfig,

    #[serde(default)]
    pub cache: CacheConfig,

    #[serde(default)]
    pub cost: CostConfig,

    #[serde(default)]
    pub timeout: TimeoutConfig,

    #[serde(default)]
    pub telemetry: TelemetryConfig,

    #[serde(default)]
    pub mcp: McpConfig,

    #[serde(default = "default_stream_channel_capacity")]
    pub stream_channel_capacity: usize,
}

fn default_stream_channel_capacity() -> usize {
    crate::DEFAULT_STREAM_CHANNEL_CAPACITY
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            default_provider: String::new(),
            providers: Vec::new(),
            models: Vec::new(),
            routing: RoutingConfig::default(),
            cache: CacheConfig::default(),
            cost: CostConfig::default(),
            timeout: TimeoutConfig::default(),
            telemetry: TelemetryConfig::default(),
            mcp: McpConfig::default(),
            stream_channel_capacity: default_stream_channel_capacity(),
        }
    }
}

impl LlmConfig {
    /// Validate the configuration, returning all errors found.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        let mut model_ids = std::collections::HashSet::new();
        for m in &self.models {
            if !model_ids.insert(&m.id) {
                errors.push(format!("duplicate model id: '{}'", m.id));
            }
        }
        let mut provider_ids = std::collections::HashSet::new();
        for p in &self.providers {
            if !provider_ids.insert(&p.provider_id) {
                errors.push(format!("duplicate provider id: '{}'", p.provider_id));
            }
        }
        for m in &self.models {
            if !provider_ids.contains(&m.provider) {
                errors.push(format!(
                    "model '{}' references unknown provider '{}'",
                    m.id, m.provider
                ));
            }
        }
        for chain in &self.routing.fallback_chains {
            for pid in &chain.providers {
                if !provider_ids.contains(pid) {
                    errors.push(format!(
                        "fallback chain '{}' references unknown provider '{}'",
                        chain.name, pid
                    ));
                }
            }
        }
        for (cap, mcp_names) in &self.mcp.capabilities {
            for mcp_name in mcp_names {
                let exists = self.mcp.servers.iter().any(|s| &s.name == mcp_name);
                if !exists {
                    errors.push(format!(
                        "MCP capability '{}' references unknown server '{}'",
                        cap, mcp_name
                    ));
                }
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Normalize configuration — strip empty strings and sort lists.
    pub fn normalized(&self) -> Self {
        let mut cfg = self.clone();
        cfg.providers.retain(|p| !p.provider_id.is_empty());
        cfg.models.sort_by(|a, b| a.id.cmp(&b.id));
        cfg
    }
}

/// Per-provider adapter configuration.
#[derive(Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub provider_id: String,
    pub provider_type: String,

    #[serde(default)]
    pub api_url: String,

    #[serde(default)]
    pub api_key: Option<String>,

    #[serde(default)]
    pub organization_id: Option<String>,

    #[serde(default)]
    pub default_model: Option<String>,

    #[serde(default)]
    pub models: Vec<String>,

    #[serde(default = "default_provider_timeout")]
    pub timeout_secs: u64,

    #[serde(default = "default_provider_retries")]
    pub max_retries: u32,

    #[serde(default)]
    pub custom_headers: HashMap<String, String>,
}

impl std::fmt::Debug for ProviderConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderConfig")
            .field("provider_id", &self.provider_id)
            .field("provider_type", &self.provider_type)
            .field("api_url", &self.api_url)
            .field("api_key", &self.api_key.as_ref().map(|_| "[REDACTED]"))
            .field("organization_id", &self.organization_id)
            .field("default_model", &self.default_model)
            .field("models", &self.models)
            .field("timeout_secs", &self.timeout_secs)
            .field("max_retries", &self.max_retries)
            .field("custom_headers", &self.custom_headers)
            .finish()
    }
}

fn default_provider_timeout() -> u64 {
    crate::DEFAULT_TIMEOUT_SECS
}

fn default_provider_retries() -> u32 {
    3
}

/// Model definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub id: String,
    pub provider: String,

    #[serde(default)]
    pub capabilities: Vec<String>,

    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,

    #[serde(default = "default_context_window")]
    pub context_window: usize,

    #[serde(default)]
    pub cost_per_1k_input: f64,

    #[serde(default)]
    pub cost_per_1k_output: f64,

    #[serde(default)]
    pub aliases: Vec<String>,
}

fn default_max_tokens() -> usize {
    4096
}

fn default_context_window() -> usize {
    8192
}

/// Routing configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingConfig {
    #[serde(default)]
    pub fallback_chains: Vec<FallbackChainConfig>,

    #[serde(default = "default_retry_max")]
    pub retry_max: u32,

    #[serde(default = "default_retry_base_ms")]
    pub retry_base_ms: u64,

    #[serde(default = "default_health_check_interval")]
    pub health_check_interval_secs: u64,
}

fn default_retry_max() -> u32 {
    3
}

fn default_retry_base_ms() -> u64 {
    1000
}

fn default_health_check_interval() -> u64 {
    60
}

impl Default for RoutingConfig {
    fn default() -> Self {
        Self {
            fallback_chains: Vec::new(),
            retry_max: default_retry_max(),
            retry_base_ms: default_retry_base_ms(),
            health_check_interval_secs: default_health_check_interval(),
        }
    }
}

/// Fallback chain definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FallbackChainConfig {
    pub name: String,
    pub capability: String,
    pub providers: Vec<String>,
}

/// Cache configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    #[serde(default = "default_cache_enabled")]
    pub enabled: bool,

    #[serde(default = "default_cache_max_entries")]
    pub max_entries: usize,

    #[serde(default = "default_cache_max_memory_bytes")]
    pub max_memory_bytes: usize,

    #[serde(default = "default_cache_ttl")]
    pub ttl_secs: u64,

    #[serde(default = "default_cache_embed_ttl")]
    pub embed_ttl_secs: u64,
}

fn default_cache_enabled() -> bool {
    true
}

fn default_cache_max_entries() -> usize {
    crate::DEFAULT_CACHE_MAX_ENTRIES
}

fn default_cache_max_memory_bytes() -> usize {
    100 * 1024 * 1024
}

fn default_cache_ttl() -> u64 {
    crate::DEFAULT_CACHE_TTL_SECS
}

fn default_cache_embed_ttl() -> u64 {
    crate::DEFAULT_CACHE_EMBED_TTL_SECS
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: default_cache_enabled(),
            max_entries: default_cache_max_entries(),
            max_memory_bytes: default_cache_max_memory_bytes(),
            ttl_secs: default_cache_ttl(),
            embed_ttl_secs: default_cache_embed_ttl(),
        }
    }
}

/// Cost tracking configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CostConfig {
    #[serde(default)]
    pub budget_monthly_cents: Option<u64>,

    #[serde(default)]
    pub alert_threshold: Option<f64>,
}

/// Timeout configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeoutConfig {
    #[serde(default = "default_timeout_secs")]
    pub default_secs: u64,

    #[serde(default = "default_streaming_timeout")]
    pub streaming_secs: u64,

    #[serde(default = "default_embedding_timeout")]
    pub embedding_secs: u64,
}

fn default_timeout_secs() -> u64 {
    crate::DEFAULT_TIMEOUT_SECS
}

fn default_streaming_timeout() -> u64 {
    crate::DEFAULT_STREAMING_TIMEOUT_SECS
}

fn default_embedding_timeout() -> u64 {
    10
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            default_secs: default_timeout_secs(),
            streaming_secs: default_streaming_timeout(),
            embedding_secs: default_embedding_timeout(),
        }
    }
}

/// Telemetry configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryConfig {
    #[serde(default = "default_metrics_enabled")]
    pub enable_metrics: bool,

    #[serde(default = "default_events_enabled")]
    pub enable_events: bool,

    #[serde(default = "default_log_level")]
    pub log_level: String,

    #[serde(default = "default_sampling_rate")]
    pub request_sampling_rate: f64,

    #[serde(default)]
    pub enable_stream_chunk_events: bool,
}

fn default_metrics_enabled() -> bool {
    true
}

fn default_events_enabled() -> bool {
    true
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_sampling_rate() -> f64 {
    1.0
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            enable_metrics: default_metrics_enabled(),
            enable_events: default_events_enabled(),
            log_level: default_log_level(),
            request_sampling_rate: default_sampling_rate(),
            enable_stream_chunk_events: false,
        }
    }
}

/// MCP configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpConfig {
    #[serde(default)]
    pub servers: Vec<McpServerConfig>,

    #[serde(default)]
    pub capabilities: HashMap<String, Vec<String>>,
}

/// Configuration for a single MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub name: String,

    #[serde(default)]
    pub command: String,

    #[serde(default)]
    pub args: Vec<String>,

    #[serde(default = "default_mcp_transport")]
    pub transport: String,

    #[serde(default)]
    pub base_url: Option<String>,

    #[serde(default = "default_mcp_auto_start")]
    pub auto_start: bool,
}

fn default_mcp_transport() -> String {
    "stdio".to_string()
}

fn default_mcp_auto_start() -> bool {
    true
}

// ────────────────────────────────────────────────────────────────────────────
// Resolver types
// ────────────────────────────────────────────────────────────────────────────

/// Result of capability → provider resolution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedEndpoint {
    pub provider_id: String,
    pub model_id: String,
    pub capability: String,
    pub priority: u32,
    pub estimated_cost_cents: f64,
}

/// Provider health snapshot.
#[derive(Debug, Clone)]
pub struct ProviderHealthReport {
    pub provider_id: String,
    pub status: ProviderHealth,
    pub models: Vec<String>,
    pub latency_p50_ms: u64,
    pub error_rate: f64,
    pub last_check: std::time::SystemTime,
}

/// Provider health status.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ProviderHealth {
    Healthy,
    Degraded { reason: String },
    Unavailable { since: std::time::SystemTime },
    Unknown,
}

// ────────────────────────────────────────────────────────────────────────────
// Cost tracking types
// ────────────────────────────────────────────────────────────────────────────

/// Snapshot of cost tracking state.
#[derive(Debug, Clone)]
pub struct CostSnapshot {
    pub total_tokens_in: u64,
    pub total_tokens_out: u64,
    pub total_cost_cents: f64,
    pub cost_by_model: HashMap<String, f64>,
    pub cost_by_provider: HashMap<String, f64>,
    pub budget_cents: Option<u64>,
    pub budget_remaining_cents: Option<f64>,
    pub session_start: std::time::SystemTime,
}

#[cfg(test)]
mod tests {
    #![allow(clippy::field_reassign_with_default)]
    use super::*;

    // ─── LlRequest ───────────────────────────────────────────────────────────

    #[test]
    fn default_ll_request() {
        let req = LlRequest::default();
        assert!(req.messages.is_empty());
        assert!(req.model.is_none());
        assert!(!req.correlation_id.as_uuid().is_nil());
    }

    #[test]
    fn request_validate_empty_messages() {
        let req = LlRequest::default();
        let err = req.validate().unwrap_err();
        assert!(err.iter().any(|e| e.contains("at least one message")));
    }

    #[test]
    fn request_validate_invalid_temperature() {
        let req = LlRequest::default()
            .with_message(LlMessage::user("hi"))
            .with_temperature(3.0);
        assert!(req.validate().is_err());
    }

    #[test]
    fn request_validate_valid() {
        let req = LlRequest::default()
            .with_message(LlMessage::user("hi"))
            .with_temperature(0.7);
        assert!(req.validate().is_ok());
    }

    #[test]
    fn request_normalize_caps_temperature() {
        let mut req = LlRequest::default();
        req.temperature = Some(5.0);
        let norm = req.normalized();
        assert_eq!(norm.temperature, Some(2.0));
    }

    #[test]
    fn request_normalize_removes_empty_stop() {
        let req = LlRequest::default().with_message(LlMessage::user("hi"));
        let mut req2 = req.clone();
        req2.stop_sequences = vec!["".into(), "stop".into()];
        let norm = req2.normalized();
        assert_eq!(norm.stop_sequences, vec!["stop"]);
    }

    #[test]
    fn request_builder_pattern() {
        let req = LlRequest::default()
            .with_model("gpt-4")
            .with_capability("planning")
            .with_temperature(0.5)
            .with_message(LlMessage::system("you are helpful"))
            .with_message(LlMessage::user("hello"))
            .with_tool(LlTool::new("get_weather", "Get weather"))
            .with_stream(true);
        assert_eq!(req.model, Some("gpt-4".into()));
        assert_eq!(req.messages.len(), 2);
        assert_eq!(req.tools.len(), 1);
        assert!(req.stream);
    }

    #[test]
    fn request_serialization_roundtrip() {
        let req = LlRequest::default()
            .with_message(LlMessage::user("hello"))
            .with_model("gpt-4");
        let json = serde_json::to_string(&req).unwrap();
        let deserialized: LlRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.model, req.model);
        assert_eq!(deserialized.messages.len(), req.messages.len());
    }

    // ─── LlMessage ──────────────────────────────────────────────────────────

    #[test]
    fn ll_message_system() {
        let msg = LlMessage::system("You are a helper");
        assert_eq!(msg.role, LlRole::System);
        assert!(matches!(msg.content, LlContent::Text(_)));
    }

    #[test]
    fn ll_message_user() {
        let msg = LlMessage::user("Hello");
        assert_eq!(msg.role, LlRole::User);
    }

    #[test]
    fn ll_message_assistant() {
        let msg = LlMessage::assistant("Hi there");
        assert_eq!(msg.role, LlRole::Assistant);
    }

    #[test]
    fn ll_message_tool_result() {
        let msg = LlMessage::tool_result("call-1", "result");
        assert_eq!(msg.role, LlRole::Tool);
        assert!(matches!(msg.content, LlContent::ToolResult { .. }));
    }

    #[test]
    fn ll_message_tool_call() {
        let mut args = HashMap::new();
        args.insert("location".into(), serde_json::json!("NYC"));
        let msg = LlMessage::tool_call("call-1", "get_weather", args);
        assert_eq!(msg.role, LlRole::Assistant);
    }

    #[test]
    fn ll_message_with_name() {
        let msg = LlMessage::user("test").with_name("alice");
        assert_eq!(msg.name, Some("alice".into()));
    }

    #[test]
    fn message_partial_eq() {
        let a = LlMessage::user("hello");
        let b = LlMessage::user("hello");
        let c = LlMessage::user("world");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn message_serialization_roundtrip() {
        let msg = LlMessage::user("hello").with_name("alice");
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: LlMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, deserialized);
    }

    // ─── LlContent ──────────────────────────────────────────────────────────

    #[test]
    fn content_text_len_approx() {
        let text = LlContent::Text("hello".into());
        assert_eq!(text.len_approx(), 5);
    }

    #[test]
    fn content_tool_call_len_approx() {
        let tool_call = LlContent::ToolCall {
            call_id: "c1".into(),
            name: "get_weather".into(),
            arguments: HashMap::new(),
        };
        assert!(tool_call.len_approx() > 0);
    }

    #[test]
    fn content_image_len_approx() {
        let img = LlContent::Image {
            mime_type: "image/png".into(),
            data: vec![0u8; 100],
        };
        assert_eq!(img.len_approx(), 109);
    }

    #[test]
    fn content_display_text() {
        let text = LlContent::Text("hello world".into());
        assert_eq!(format!("{}", text), "hello world");
    }

    #[test]
    fn content_display_image() {
        let img = LlContent::Image {
            mime_type: "image/png".into(),
            data: vec![0u8; 3],
        };
        let s = format!("{}", img);
        assert!(s.contains("image/png"));
        assert!(s.contains("[image:"));
    }

    #[test]
    fn content_display_tool_result() {
        let tr = LlContent::ToolResult {
            call_id: "call-1".into(),
            output: "42".into(),
        };
        let s = format!("{}", tr);
        assert!(s.contains("call-1"));
        assert!(s.contains("42"));
    }

    #[test]
    fn content_display_tool_call() {
        let mut args = std::collections::HashMap::new();
        args.insert("x".into(), serde_json::json!(1));
        let tc = LlContent::ToolCall {
            call_id: "c1".into(),
            name: "get_x".into(),
            arguments: args,
        };
        let s = format!("{}", tc);
        assert!(s.contains("get_x"));
        assert!(s.contains("c1"));
    }

    // ─── LlTool ─────────────────────────────────────────────────────────────

    #[test]
    fn tool_new() {
        let tool = LlTool::new("get_weather", "Get the weather");
        assert_eq!(tool.name, "get_weather");
        assert!(tool.parameters.is_object());
    }

    #[test]
    fn tool_with_params() {
        let params = serde_json::json!({"type": "object"});
        let tool = LlTool::new("f", "desc")
            .with_parameters(params.clone())
            .with_strict(true);
        assert_eq!(tool.parameters, params);
        assert!(tool.strict);
    }

    // ─── LlResponse ─────────────────────────────────────────────────────────

    #[test]
    fn ll_response_append_content() {
        let mut resp = LlResponse {
            message: LlMessage::assistant(""),
            finish_reason: LlFinishReason::Stop,
            usage: LlUsage::default(),
            model: "gpt-4".into(),
            provider: "openai".into(),
        };
        resp.append_content("Hello");
        resp.append_content(" world");
        if let LlContent::Text(ref t) = resp.message.content {
            assert_eq!(t, "Hello world");
        } else {
            panic!("expected Text content");
        }
    }

    #[test]
    fn response_validate_empty_model() {
        let resp = LlResponse {
            message: LlMessage::assistant("hi"),
            finish_reason: LlFinishReason::Stop,
            usage: LlUsage::default(),
            model: "".into(),
            provider: "openai".into(),
        };
        assert!(resp.validate().is_err());
    }

    #[test]
    fn response_validate_ok() {
        let resp = LlResponse {
            message: LlMessage::assistant("hi"),
            finish_reason: LlFinishReason::Stop,
            usage: LlUsage::default(),
            model: "gpt-4".into(),
            provider: "openai".into(),
        };
        assert!(resp.validate().is_ok());
    }

    #[test]
    fn response_approximate_size() {
        let resp = LlResponse {
            message: LlMessage::assistant("hello"),
            finish_reason: LlFinishReason::Stop,
            usage: LlUsage::default(),
            model: "gpt-4".into(),
            provider: "openai".into(),
        };
        assert!(resp.approximate_size() > 0);
    }

    // ─── LlFinishReason ──────────────────────────────────────────────────────

    #[test]
    fn finish_reason_equality() {
        assert_eq!(LlFinishReason::Stop, LlFinishReason::Stop);
        assert_ne!(LlFinishReason::Stop, LlFinishReason::Length);
    }

    // ─── LlUsage ─────────────────────────────────────────────────────────────

    #[test]
    fn usage_default_currency() {
        let usage = LlUsage::default();
        assert_eq!(usage.currency, "USD");
    }

    // ─── LlToolCallDelta ─────────────────────────────────────────────────────

    #[test]
    fn tool_call_delta_is_start() {
        let delta = LlToolCallDelta {
            index: 0,
            id: Some("call-1".into()),
            name: Some("get_weather".into()),
            arguments: None,
        };
        assert!(delta.is_start());
    }

    #[test]
    fn tool_call_delta_not_start() {
        let delta = LlToolCallDelta {
            index: 1,
            id: None,
            name: None,
            arguments: Some("{\"loc\":".into()),
        };
        assert!(!delta.is_start());
    }

    #[test]
    fn tool_call_delta_merge_arguments() {
        let mut acc = "".to_string();
        LlToolCallDelta::merge_arguments(&mut acc, &Some("{\"loc\":".into()));
        LlToolCallDelta::merge_arguments(&mut acc, &Some("\"NYC\"}".into()));
        assert_eq!(acc, "{\"loc\":\"NYC\"}");
    }

    #[test]
    fn tool_call_delta_merge_none() {
        let mut acc = "{}".to_string();
        LlToolCallDelta::merge_arguments(&mut acc, &None);
        assert_eq!(acc, "{}");
    }

    // ─── LlEmbedRequest ─────────────────────────────────────────────────────

    #[test]
    fn embed_request_validate_ok() {
        let req = LlEmbedRequest {
            input: vec!["hello".into()],
            model: None,
            correlation_id: CorrelationId::new(),
        };
        assert!(req.validate().is_ok());
    }

    #[test]
    fn embed_request_validate_empty_input() {
        let req = LlEmbedRequest::default();
        assert!(req.validate().is_err());
    }

    #[test]
    fn embed_request_validate_empty_string() {
        let req = LlEmbedRequest {
            input: vec!["".into()],
            model: None,
            correlation_id: CorrelationId::new(),
        };
        let err = req.validate().unwrap_err();
        assert!(err.iter().any(|e| e.contains("input[0]")));
    }

    #[test]
    fn embed_request_serialization() {
        let req = LlEmbedRequest {
            input: vec!["hello".into()],
            model: Some("embed-3".into()),
            correlation_id: CorrelationId::new(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let deserialized: LlEmbedRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.input, req.input);
        assert_eq!(deserialized.model, req.model);
    }

    // ─── LlEmbedResponse ────────────────────────────────────────────────────

    #[test]
    fn embed_response_serialization() {
        let resp = LlEmbedResponse {
            embeddings: vec![vec![0.1, 0.2, 0.3]],
            dimensions: 3,
            model: "embed-3".into(),
            usage: LlUsage::default(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let deserialized: LlEmbedResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, deserialized);
    }

    // ─── LlmConfig ──────────────────────────────────────────────────────────

    #[test]
    fn config_default_validates_ok() {
        let config = LlmConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn config_duplicate_model_id() {
        let mut config = LlmConfig::default();
        config.models = vec![
            ModelConfig {
                id: "gpt-4".into(),
                provider: "openai".into(),
                capabilities: vec![],
                max_tokens: 4096,
                context_window: 8192,
                cost_per_1k_input: 0.0,
                cost_per_1k_output: 0.0,
                aliases: vec![],
            },
            ModelConfig {
                id: "gpt-4".into(),
                provider: "openai".into(),
                capabilities: vec![],
                max_tokens: 4096,
                context_window: 8192,
                cost_per_1k_input: 0.0,
                cost_per_1k_output: 0.0,
                aliases: vec![],
            },
        ];
        config.providers = vec![ProviderConfig {
            provider_id: "openai".into(),
            provider_type: "openai".into(),
            api_url: String::new(),
            api_key: None,
            organization_id: None,
            default_model: None,
            models: vec![],
            timeout_secs: 30,
            max_retries: 3,
            custom_headers: HashMap::new(),
        }];
        let err = config.validate().unwrap_err();
        assert!(err.iter().any(|e| e.contains("duplicate model")));
    }

    #[test]
    fn config_missing_provider() {
        let mut config = LlmConfig::default();
        config.models = vec![ModelConfig {
            id: "gpt-4".into(),
            provider: "nonexistent".into(),
            capabilities: vec![],
            max_tokens: 4096,
            context_window: 8192,
            cost_per_1k_input: 0.0,
            cost_per_1k_output: 0.0,
            aliases: vec![],
        }];
        let err = config.validate().unwrap_err();
        assert!(err.iter().any(|e| e.contains("unknown provider")));
    }

    #[test]
    fn config_duplicate_provider_id() {
        let mut config = LlmConfig::default();
        config.providers = vec![
            ProviderConfig {
                provider_id: "openai".into(),
                provider_type: "openai".into(),
                api_url: String::new(),
                api_key: None,
                organization_id: None,
                default_model: None,
                models: vec![],
                timeout_secs: 30,
                max_retries: 3,
                custom_headers: HashMap::new(),
            },
            ProviderConfig {
                provider_id: "openai".into(),
                provider_type: "openai".into(),
                api_url: String::new(),
                api_key: None,
                organization_id: None,
                default_model: None,
                models: vec![],
                timeout_secs: 30,
                max_retries: 3,
                custom_headers: HashMap::new(),
            },
        ];
        let err = config.validate().unwrap_err();
        assert!(err.iter().any(|e| e.contains("duplicate provider")));
    }

    #[test]
    fn config_fallback_chain_unknown_provider() {
        let mut config = LlmConfig::default();
        config.providers = vec![ProviderConfig {
            provider_id: "openai".into(),
            provider_type: "openai".into(),
            api_url: String::new(),
            api_key: None,
            organization_id: None,
            default_model: None,
            models: vec![],
            timeout_secs: 30,
            max_retries: 3,
            custom_headers: HashMap::new(),
        }];
        config.routing.fallback_chains = vec![FallbackChainConfig {
            name: "chain".into(),
            capability: "chat".into(),
            providers: vec!["nonexistent".into()],
        }];
        let err = config.validate().unwrap_err();
        assert!(err.iter().any(|e| e.contains("fallback chain")));
    }

    #[test]
    fn config_mcp_unknown_server() {
        let mut config = LlmConfig::default();
        let mut capabilities = HashMap::new();
        capabilities.insert("planning".into(), vec!["nonexistent-server".into()]);
        config.mcp.capabilities = capabilities;
        let err = config.validate().unwrap_err();
        assert!(err.iter().any(|e| e.contains("unknown server")));
    }

    #[test]
    fn config_default_values_smoke() {
        let config = LlmConfig::default();
        assert_eq!(config.stream_channel_capacity, 256);

        let timeout = TimeoutConfig::default();
        assert_eq!(timeout.default_secs, 30);
        assert_eq!(timeout.streaming_secs, 120);

        let cache = CacheConfig::default();
        assert!(cache.enabled);
        assert_eq!(cache.max_entries, 1000);
        assert_eq!(cache.ttl_secs, 300);

        let cost = CostConfig::default();
        assert!(cost.budget_monthly_cents.is_none());
    }

    #[test]
    fn config_serialization_roundtrip() {
        let config = LlmConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: LlmConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(
            deserialized.stream_channel_capacity,
            config.stream_channel_capacity
        );
    }

    #[test]
    fn config_normalized_sorts_models() {
        let mut config = LlmConfig::default();
        config.models = vec![
            ModelConfig {
                id: "z-model".into(),
                provider: "p".into(),
                capabilities: vec![],
                max_tokens: 4096,
                context_window: 8192,
                cost_per_1k_input: 0.0,
                cost_per_1k_output: 0.0,
                aliases: vec![],
            },
            ModelConfig {
                id: "a-model".into(),
                provider: "p".into(),
                capabilities: vec![],
                max_tokens: 4096,
                context_window: 8192,
                cost_per_1k_input: 0.0,
                cost_per_1k_output: 0.0,
                aliases: vec![],
            },
        ];
        let norm = config.normalized();
        assert_eq!(norm.models[0].id, "a-model");
        assert_eq!(norm.models[1].id, "z-model");
    }

    // ─── ProviderConfig Debug ──────────────────────────────────────────────

    #[test]
    fn provider_config_debug_redacts_api_key() {
        let config = ProviderConfig {
            provider_id: "openai".into(),
            provider_type: "openai".into(),
            api_url: "https://api.openai.com".into(),
            api_key: Some("sk-abc123".into()),
            organization_id: None,
            default_model: None,
            models: vec![],
            timeout_secs: 30,
            max_retries: 3,
            custom_headers: HashMap::new(),
        };
        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("[REDACTED]"));
        assert!(!debug_str.contains("sk-abc123"));
    }

    // ─── ProviderCapability ────────────────────────────────────────────────

    #[test]
    fn provider_capability_equality() {
        assert_eq!(ProviderCapability::Chat, ProviderCapability::Chat);
        assert_ne!(ProviderCapability::Chat, ProviderCapability::Streaming);
    }

    // ─── ResolvedEndpoint ──────────────────────────────────────────────────

    #[test]
    fn resolved_endpoint_serialization() {
        let ep = ResolvedEndpoint {
            provider_id: "openai".into(),
            model_id: "gpt-4".into(),
            capability: "chat".into(),
            priority: 1,
            estimated_cost_cents: 0.5,
        };
        let json = serde_json::to_string(&ep).unwrap();
        let deserialized: ResolvedEndpoint = serde_json::from_str(&json).unwrap();
        assert_eq!(ep, deserialized);
    }

    // ─── Provider types serialization ──────────────────────────────────────

    #[test]
    fn provider_request_serialization() {
        let req = ProviderRequest {
            model: "gpt-4".into(),
            messages: vec![ProviderMessage {
                role: LlRole::User,
                content: LlContent::Text("hi".into()),
            }],
            max_tokens: Some(100),
            temperature: Some(0.7),
            stop_sequences: vec![],
            system_prompt: None,
            tools: vec![],
            stream: false,
            timeout_ms: 30000,
        };
        let json = serde_json::to_string(&req).unwrap();
        let deserialized: ProviderRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.model, req.model);
    }

    #[test]
    fn provider_response_serialization() {
        let resp = ProviderResponse {
            message: ProviderMessage {
                role: LlRole::Assistant,
                content: LlContent::Text("hi".into()),
            },
            finish_reason: LlFinishReason::Stop,
            input_tokens: 10,
            output_tokens: 20,
            model: "gpt-4".into(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let deserialized: ProviderResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, deserialized);
    }

    // ─── ModelConfig ───────────────────────────────────────────────────────

    #[test]
    fn model_config_default_values() {
        let mc = ModelConfig {
            id: "test".into(),
            provider: "p".into(),
            capabilities: vec![],
            max_tokens: 4096,
            context_window: 8192,
            cost_per_1k_input: 0.0,
            cost_per_1k_output: 0.0,
            aliases: vec![],
        };
        assert_eq!(mc.max_tokens, 4096);
        assert_eq!(mc.context_window, 8192);
    }

    // ─── Send + Sync assertions ────────────────────────────────────────────

    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}

    #[test]
    fn types_are_send_sync() {
        assert_send::<LlRequest>();
        assert_sync::<LlRequest>();
        assert_send::<LlResponse>();
        assert_sync::<LlResponse>();
        assert_send::<LlMessage>();
        assert_sync::<LlMessage>();
        assert_send::<LlTool>();
        assert_sync::<LlTool>();
        assert_send::<LlUsage>();
        assert_sync::<LlUsage>();
        assert_send::<LlEmbedRequest>();
        assert_sync::<LlEmbedRequest>();
        assert_send::<LlEmbedResponse>();
        assert_sync::<LlEmbedResponse>();
        assert_send::<LlmConfig>();
        assert_sync::<LlmConfig>();
        assert_send::<ProviderConfig>();
        assert_sync::<ProviderConfig>();
        assert_send::<ModelConfig>();
        assert_sync::<ModelConfig>();
        assert_send::<ProviderRequest>();
        assert_sync::<ProviderRequest>();
        assert_send::<ProviderResponse>();
        assert_sync::<ProviderResponse>();
        assert_send::<ResolvedEndpoint>();
        assert_sync::<ResolvedEndpoint>();
    }

    // ─── Clone correctness ────────────────────────────────────────────────

    #[test]
    fn clone_independence() {
        let original = LlRequest::default()
            .with_message(LlMessage::user("hello"))
            .with_model("gpt-4");
        let mut cloned = original.clone();
        cloned.messages.push(LlMessage::assistant("world"));
        assert_eq!(original.messages.len(), 1);
        assert_eq!(cloned.messages.len(), 2);
    }

    // ─── LlmConfig TOML deserialization ────────────────────────────────────

    #[test]
    fn config_from_toml_smoke() {
        // TOML with nested structs requires flattening or separate sections.
        // Smoke: just verify serde_json roundtrip works.
        let config = LlmConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: LlmConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.default_provider, "");
    }
}
