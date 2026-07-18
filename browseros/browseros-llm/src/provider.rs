//! LLM Provider trait — provider-agnostic backend adapter interface.

use crate::error::LlmError;
use crate::types::{
    ProviderCapability, ProviderEmbedRequest, ProviderEmbedResponse, ProviderHealthResult,
    ProviderRequest, ProviderResponse, ProviderStream,
};

/// A provider-agnostic LLM backend adapter.
///
/// All provider-specific logic is encapsulated behind this trait.
/// The Gateway never knows which concrete provider it's calling.
pub trait LlProvider: Send + Sync {
    /// Unique provider identifier (e.g. "openai", "anthropic", "ollama").
    fn id(&self) -> &str;

    /// Human-readable provider name (e.g. "OpenAI", "Anthropic Claude").
    fn name(&self) -> &str;

    /// Capabilities this provider supports.
    fn capabilities(&self) -> Vec<ProviderCapability>;

    /// List of model IDs this provider can serve.
    fn models(&self) -> Vec<String>;

    /// Chat completion (blocking).
    fn chat(&self, request: ProviderRequest) -> Result<ProviderResponse, LlmError>;

    /// Streaming chat completion.
    fn chat_stream(&self, request: ProviderRequest) -> Result<ProviderStream, LlmError>;

    /// Embedding generation.
    fn embed(&self, request: ProviderEmbedRequest) -> Result<ProviderEmbedResponse, LlmError>;

    /// Health check.
    fn health(&self) -> ProviderHealthResult;
}

/// Round to nearest 0.001 cent (millicent precision) to avoid floating point drift.
pub fn round_to_millicents(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}
