//! # browseros-llm
//!
//! Provider-agnostic LLM Gateway for the BrowserOS runtime.
//!
//! ## Architecture
//!
//! The LLM Gateway sits at layer L3 (alongside `browseros-dag`) and provides
//! a unified interface for communicating with LLM backends without coupling
//! the planner, runtime, or any other subsystem to a specific provider.
//!
//! ```text
//! Planner (browseros-dag)
//!   └── LlGateway::chat(request)        ← Provider-agnostic API
//!         ├── Router.resolve()           ← Capability → provider mapping
//!         ├── Cache.get()                ← Response cache (LRU + TTL)
//!         ├── Provider.chat()            ← LlProvider trait (OpenAI, Anthropic, …)
//!         ├── CostTracker.record()       ← Token + cost accounting
//!         └── LlmTelemetry.emit()        ← Events + metrics + logs
//! ```
//!
//! ## Key design invariants
//!
//! - **Provider-agnostic core**: The Gateway never knows which concrete provider
//!   is handling a request. All provider-specific code lives in adapters.
//! - **MCP is an adapter**: MCP is one protocol among many. The Gateway has zero
//!   MCP knowledge — MCP tools are surfaced as `LlTool`.
//! - **No async runtime**: All I/O uses blocking calls + background threads.
//!   The API surface is synchronous (`Send + Sync`).
//! - **No dependency on browseros-dag**: The LLM crate does not import planner
//!   types, plugin types, or any DAG internals.
//! - **No serde_json::Value leaks**: Gateway types use concrete Rust types.
//!   `serde_json::Value` is internal to adapters and MCP.
//!
//! ## Crate boundaries
//!
//! Dependencies:
//! - `browseros-types` — identifiers, event types
//! - `browseros-event-bus` — event publishing
//! - `browseros-observability` — Logger, MetricsRegistry, Tracer
//! - `serde`, `serde_json` — (de)serialization (internal only)
//!
//! Does NOT depend on:
//! - `browseros-dag` — no planner types
//! - `browseros-bridge` — no browser port traits
//! - `browseros-browser`, `browseros-cdp` — no browser internals

pub mod cache;
pub mod cost;
pub mod error;
pub mod gateway;
pub mod model_registry;
pub mod provider;
pub mod router;
pub mod streaming;
pub mod telemetry;
pub mod types;

/// Provider adapter implementations (private).
///
/// Only the `adapters::create_provider` factory is exposed to `LlGateway::new`.
mod adapters;

/// MCP adapter (Model Context Protocol).
///
/// MCP is one protocol for tool discovery and execution. The MCP adapter
/// lives inside this crate and translates between MCP protocol messages
/// and BrowserOS internal types. The Gateway has zero MCP knowledge.
pub mod mcp;

// ────────────────────────────────────────────────────────────────────────────
// Re-exports — convenience for external consumers
// ────────────────────────────────────────────────────────────────────────────

pub use cache::LlCache;
pub use cost::CostTracker;
pub use error::LlmError;
pub use gateway::LlGateway;
pub use model_registry::ModelRegistry;
pub use provider::LlProvider;
pub use router::LlRouter;
pub use streaming::LlStreamHandle;
// Re-export only user-facing types (not internal Provider* types)
pub use mcp::{McpAdapter, McpError, McpTransport};
pub use streaming::{LlStreamChunk, LlStreamEvent};
pub use types::{
    CacheConfig, CostConfig, CostSnapshot, FallbackChainConfig, LlContent, LlEmbedRequest,
    LlEmbedResponse, LlFinishReason, LlMessage, LlRequest, LlResponse, LlRole, LlTool,
    LlToolCallDelta, LlUsage, LlmConfig, McpConfig, McpServerConfig, ModelConfig, ProviderConfig,
    ProviderHealth, ProviderHealthReport, ResolvedEndpoint, RoutingConfig, TelemetryConfig,
    TimeoutConfig,
};

// ────────────────────────────────────────────────────────────────────────────
// Constants
// ────────────────────────────────────────────────────────────────────────────

/// Default capacity for streaming channels (background thread → consumer).
pub const DEFAULT_STREAM_CHANNEL_CAPACITY: usize = 256;

/// Default request timeout in seconds.
pub const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Default streaming timeout in seconds.
pub const DEFAULT_STREAMING_TIMEOUT_SECS: u64 = 120;

/// Default LRU cache entry limit.
pub const DEFAULT_CACHE_MAX_ENTRIES: usize = 1000;

/// Default cache TTL in seconds (5 minutes).
pub const DEFAULT_CACHE_TTL_SECS: u64 = 300;

/// Default embedding cache TTL in seconds (1 hour).
pub const DEFAULT_CACHE_EMBED_TTL_SECS: u64 = 3600;
