# LLM Gateway Architecture

**Phase:** 2.1 — Design Freeze  
**Status:** Draft  

---

## 1. High-Level Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                     browseros-runtime                           │
│  RuntimeContext { llm_gateway: Arc<LlGateway> }                 │
└───────────────────────────┬─────────────────────────────────────┘
                            │
┌───────────────────────────▼─────────────────────────────────────┐
│                     browseros-llm (GATEWAY)                      │
│                                                                  │
│  ┌─────────────────────┐    ┌───────────────────────────────┐   │
│  │    LlGateway        │    │      ModelRegistry            │   │
│  │  (public API)       │───▶│  — provider lookup            │   │
│  │  — chat             │    │  — model metadata             │   │
│  │  — embed            │    │  — capability query           │   │
│  │  — stream           │    └───────────────────────────────┘   │
│  │  — health           │                                        │
│  │  — resolve          │    ┌───────────────────────────────┐   │
│  └─────────┬───────────┘    │      CacheManager             │   │
│            │                │  — LRU response cache         │   │
│            ▼                │  — embedding cache            │   │
│  ┌─────────────────────┐    │  — TTL per model              │   │
│  │   Router            │    └───────────────────────────────┘   │
│  │  — capability→provider                                        │
│  │  — fallback chain   │    ┌───────────────────────────────┐   │
│  │  — timeout          │    │      CostTracker              │   │
│  │  — retry            │    │  — token count                │   │
│  └─────────┬───────────┘    │  — rate estimate              │   │
│            │                │  — budget enforce             │   │
│            ▼                └───────────────────────────────┘   │
│  ┌─────────────────────┐                                         │
│  │  LlProvider trait   │    ┌───────────────────────────────┐   │
│  │  (abstract)         │    │      Telemetry                │   │
│  │  ▲        ▲         │    │  — latency histograms         │   │
│  │  │        │         │    │  — error counters             │   │
│  └──┼────────┼─────────┘    │  — token metrics              │   │
│     │        │              └───────────────────────────────┘   │
│  ┌──┴──┐ ┌───┴────┐                                              │
│  │Open │ │Anthropic│  ...  (1 trait, N adapters)                 │
│  │ AI  │ │        │                                              │
│  └─────┘ └────────┘                                              │
│                                                                  │
│  ┌───────────────────────────────┐                               │
│  │     MCP Adapter              │                               │
│  │  (maps MCP ↔ LlGateway types)│                               │
│  └───────────────────────────────┘                               │
└─────────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────────┐
│                     browseros-dag                                │
│  PlannerBridge trait (unchanged)                                 │
│  LLMPlanner (NEW — implements PlannerBridge via LlGateway)       │
└─────────────────────────────────────────────────────────────────┘
```

---

## 2. Crate Structure

```
browseros-llm/
├── Cargo.toml
├── src/
│   ├── lib.rs            — re-exports, constants
│   ├── gateway.rs        — LlGateway struct (public API)
│   ├── router.rs         — LlRouter: capability→provider resolution
│   ├── model_registry.rs — ModelRegistry: registered models & metadata
│   ├── provider.rs       — LlProvider trait
│   ├── error.rs          — LlmError enum
│   ├── types.rs          — Request/Response types
│   ├── cache.rs          — LlCache: response + embedding cache
│   ├── cost.rs           — CostTracker: token tracking + estimate
│   ├── telemetry.rs      — LlmTelemetry: metrics + events
│   ├── streaming.rs      — LlStream: streaming response types
│   ├── adapters/
│   │   ├── mod.rs
│   │   ├── openai.rs     — OpenAI adapter
│   │   ├── anthropic.rs  — Anthropic adapter
│   │   ├── ollama.rs     — Ollama adapter
│   │   ├── gemini.rs     — Gemini adapter
│   │   └── http_generic.rs — Generic HTTP adapter
│   └── mcp/
│       ├── mod.rs
│       └── adapter.rs    — MCP↔Gateway bridge
```

---

## 3. Dependency Graph

```
browseros-llm
├── browseros-types       — Identifiers, event types, enums
├── browseros-event-bus   — EventHandler, Bus for LLM events
├── browseros-observability — Metrics, Tracer, Logger
├── serde, serde_json     — Provider-agnostic I/O (internal only)
├── reqwest               — HTTP client (adapter layer only)
└── tokenizers (optional) — Abstract token counting
```

---

## 4. Lifecycle Flow

```
PlannerBridge::plan()
  └── LLMPlanner::plan()
        └── LlGateway::chat(request)
              ├── Router::resolve(capability)
              │     └── ModelRegistry::lookup()
              │           └── Returns (model_id, provider_id)
              ├── CacheManager::get(key)
              │     └── Hit → return cached response
              ├── Provider::chat(request)
              │     ├── Retry with backoff on 429/503
              │     ├── Timeout enforcement
              │     └── Return LlResponse
              ├── CostTracker::record(model, tokens)
              ├── LlmTelemetry::emit()
              └── Return LlResponse
```

---

## 5. Gateway Configuration

The Gateway is configured during `RuntimeContext` construction. Configuration flows through the existing config system:

```rust
// Deserialized from browseros-config
struct LlmConfig {
    default_provider: String,        // "openai", "anthropic", etc.
    models: Vec<ModelConfig>,
    routing: RoutingConfig,
    cache: CacheConfig,
    cost: CostConfig,
    timeout: TimeoutConfig,
}

struct ModelConfig {
    id: String,                      // "gpt-4", "claude-3-opus", etc.
    provider: String,                // references a registered provider
    capabilities: Vec<String>,       // "planning", "code", "reasoning"
    max_tokens: usize,
    context_window: usize,
    cost_per_1k_input: f64,
    cost_per_1k_output: f64,
    aliases: Vec<String>,            // "fast", "cheap", "smart"
}

struct RoutingConfig {
    fallback_chain: Vec<String>,     // ordered provider IDs
    retry_max: u32,
    retry_base_ms: u64,
    health_check_interval_secs: u64,
}

struct CacheConfig {
    enabled: bool,
    max_entries: usize,
    ttl_secs: u64,
    embed_ttl_secs: u64,
}

struct CostConfig {
    budget_monthly_cents: Option<u64>,
    alert_threshold: Option<f64>,
}

struct TimeoutConfig {
    default_secs: u64,
    streaming_secs: u64,
    embedding_secs: u64,
}
```

---

## 6. Provider Abstraction

The provider layer is defined by a single trait with two variants (blocking + streaming):

```rust
trait LlProvider: Send + Sync {
    fn id(&self) -> &str;
    fn capabilities(&self) -> Vec<ProviderCapability>;
    fn models(&self) -> Vec<String>;
    fn chat(&self, request: LlRequest) -> Result<LlResponse, LlmError>;
    fn chat_stream(&self, request: LlRequest) -> Result<LlStream, LlmError>;
    fn embed(&self, input: Vec<String>) -> Result<Vec<Vec<f32>>, LlmError>;
    fn health(&self) -> ProviderHealth;
}
```

Each adapter implements `LlProvider` for a specific backend. Adapters live in `adapters/` and are registered at startup.

---

## 7. Event Integration

Every LLM operation publishes via EventBus:

| Event | Payload | When |
|---|---|---|
| `LlmRequestStarted` | model_id, provider, tokens_in, correlation_id | On chat/embed start |
| `LlmRequestCompleted` | model_id, latency_ms, tokens_out, cost_estimate | On success |
| `LlmRequestFailed` | model_id, error, retry_count | On failure |
| `LlmCacheHit` | cache_key, model_id | On cache hit |
| `LlmProviderDegraded` | provider_id, health_status | Health check failure |
| `LlmRateLimited` | provider_id, retry_after_ms | On rate limit |

---

## 8. Invariant Summary

| # | Invariant | Enforcement |
|---|---|---|
| 1 | Gateway never sees planner types | Separate crate, no `browseros-dag` dependency |
| 2 | Provider adapters never know about MCP | Adapters implement `LlProvider` only |
| 3 | Planner never constructs raw provider calls | Goes through `LlGateway` → `Router` → `Provider` |
| 4 | All LLM observability flows through EventBus | Gateway publishes events for every operation |
| 5 | Provider configuration is in `LlmConfig`, not in code | Adapters take config at registration |
| 6 | No `serde_json::Value` leaks to consumers | Gateway types use concrete Rust types; `Value` internal only |
| 7 | Gateway is Sync + Send | All internal types are thread-safe, no async runtime coupling |
| 8 | Streaming is optional per provider | `chat_stream` returns `Unsupported` error for non-streaming adapters |
