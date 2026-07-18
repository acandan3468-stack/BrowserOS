# LLM Routing, Fallback & Retry

**Phase:** 2.1 — Design Freeze  
**Status:** Draft  

---

## 1. `LlRouter` — Capability-Based Resolver

```rust
// browseros-llm/src/router.rs

pub struct LlRouter {
    capabilities: HashMap<String, Vec<ResolvedEndpoint>>,
    fallback_chains: Vec<FallbackChain>,
    config: RoutingConfig,
    registry: Arc<ModelRegistry>,
    health: Arc<ProviderHealthTracker>,
}
```

### Resolution Algorithm

```
resolve(capability, hints, context_length)
  │
  ├─ 1. Query ModelRegistry for capability
  │     └─ Returns all models with matching capability, sorted by priority
  │
  ├─ 2. Filter by hints (if any)
  │     └─ "fast" → sort by latency
  │     └─ "cheap" → sort by cost
  │     └─ "smart" → sort by capability score
  │     └─ "local" → filter local providers only
  │
  ├─ 3. Filter by context window if context_length known
  │     └─ Remove models where context_window < context_length
  │
  ├─ 4. Filter unhealthy providers
  │     └─ Remove providers with status != Healthy
  │
  ├─ 5. Select best match
  │     └─ If multiple: use priority, then round-robin
  │
  └─ 6. Cache resolved endpoint for this capability + hints
```

### Capability→Model Mapping

Configured in `LlmConfig.models[]`:

```toml
[llm.models]
# Each model declares its capabilities
models = [
    { id = "gpt-4o", provider = "openai", capabilities = ["planning", "reasoning", "code", "vision"],
      cost_per_1k_input = 2.5, cost_per_1k_output = 10.0, context_window = 128000 },
    { id = "claude-3-opus", provider = "anthropic", capabilities = ["planning", "reasoning", "code"],
      cost_per_1k_input = 15.0, cost_per_1k_output = 75.0, context_window = 200000 },
    { id = "claude-3-sonnet", provider = "anthropic", capabilities = ["planning", "code", "reasoning"],
      cost_per_1k_input = 3.0, cost_per_1k_output = 15.0, context_window = 200000 },
    { id = "llama3.2", provider = "ollama", capabilities = ["planning", "code"],
      cost_per_1k_input = 0.0, cost_per_1k_output = 0.0, context_window = 128000 },
    { id = "gemini-2.0-flash", provider = "gemini", capabilities = ["planning", "vision", "fast"],
      cost_per_1k_input = 0.1, cost_per_1k_output = 0.4, context_window = 1048576 },
]
```

---

## 2. Fallback Chain

When the primary provider fails, the Router tries fallbacks in order:

```rust
pub struct FallbackChain {
    name: String,                    // "planning-chain", "code-chain"
    capability: String,
    providers: Vec<String>,          // Ordered: ["openai", "anthropic", "ollama"]
    timeout_ms: u64,
    retry_config: RetryConfig,
}
```

### Fallback Execution Flow

```
LlGateway::chat(request)
  │
  ├─ Router::resolve("planning", [])
  │     └─ Returns (provider="openai", model="gpt-4o")
  │
  ├─ Provider::chat(request)
  │     ├─ SUCCESS → return response
  │     ├─ 429 Rate Limited → retry with backoff
  │     ├─ 503 Unavailable → FALLBACK
  │     │     └─ Router::resolve_next("planning", exclude=["openai"])
  │     │           └─ Returns (provider="anthropic", model="claude-3-sonnet")
  │     │           └─ Provider::chat(request)
  │     │                 ├─ SUCCESS → return response (with fallback metadata)
  │     │                 └─ FAIL → try next in chain
  │     └─ All failed → LlmError::AllProvidersFailed { attempts: [...] }
  └─ Return error
```

---

## 3. Retry Configuration

```rust
pub struct RetryConfig {
    max_attempts: u32,          // Default: 3
    base_delay_ms: u64,         // Default: 1000
    max_delay_ms: u64,         // Default: 30000
    strategy: RetryStrategy,    // Exponential backoff with jitter
    retryable_errors: Vec<RetryableError>,
}

pub enum RetryStrategy {
    ExponentialBackoff,
    Linear,
    Constant,
    NoRetry,
}

pub enum RetryableError {
    RateLimited,
    ServerError,
    Timeout,
    Unavailable,
    // Auth failures, invalid requests → NOT retryable
}
```

### Retry Behavior

| Error | Retryable? | Strategy | Notes |
|---|---|---|---|
| 429 Rate Limited | ✅ | Exponential + jitter | Uses `retry-after` header if available |
| 503 Service Unavailable | ✅ | Exponential | Max 3 attempts |
| 502 Bad Gateway | ✅ | Constant (2s) | Proxy issues |
| Timeout | ✅ | Linear (increasing) | Timeout grows by 50% each attempt |
| 401 Unauthorized | ❌ | — | Configuration error |
| 400 Bad Request | ❌ | — | Programming error |
| 422 Unprocessable | ❌ | — | Context too long, invalid params |

---

## 4. Timeout Enforcement

```rust
pub struct TimeoutConfig {
    default_secs: u64,        // Default: 30
    streaming_secs: u64,      // Default: 120 (per stream)
    embedding_secs: u64,      // Default: 10
    health_check_secs: u64,   // Default: 5
}
```

Timeouts are enforced at the HTTP client level (reqwest) and at the Gateway level (watchdog thread). If the adapter hangs, the Gateway timeout kills the request and triggers fallback.

---

## 5. Provider Health Tracking

```rust
pub struct ProviderHealthTracker {
    providers: HashMap<String, ProviderHealthResult>,
    check_interval: Duration,
}

impl ProviderHealthTracker {
    /// Run health checks on a schedule (called by Gateway startup)
    pub fn start_periodic_checks(&self, gateway: Weak<LlGateway>);

    /// Mark a provider as degraded after repeated failures
    pub fn record_failure(&self, provider_id: &str);

    /// Mark a provider as healthy after successful call
    pub fn record_success(&self, provider_id: &str);

    /// Get current health report
    pub fn report(&self) -> Vec<ProviderHealthReport>;
}
```

Health states transition:

```
Unknown ──[first check]──▶ Healthy
Healthy ──[3 failures]──▶ Degraded
Degraded ──[5 failures]──▶ Unavailable
Unavailable ──[time passes]──▶ Degraded (re-check)
Degraded ──[success]──▶ Healthy
```

---

## 6. Rate Limiting

The Gateway tracks rate limits per provider. If a provider returns 429, the Gateway:

1. Records the `retry-after` header value
2. Backs off all requests to this provider
3. Routes to fallback providers in the meantime
4. Returns to primary provider after the backoff window

```rust
pub struct RateLimitState {
    provider_id: String,
    retry_after: Option<Instant>,
    consecutive_429s: u32,
    backoff_multiplier: f64,     // Grows with each consecutive 429
}
```

---

## 7. Routing Examples

### Example 1: Default Planning Request

```
LlGateway::chat(LlRequest { capability: Some("planning"), ... })
  │
  ├─ Router::resolve("planning", [])
  │     ├─ Found: gpt-4o (priority 1), claude-3-sonnet (priority 2), llama3.2 (priority 3)
  │     ├─ All healthy? Yes
  │     ├─ Selected: gpt-4o (highest priority)
  │     └─ Returns ResolvedEndpoint { provider: "openai", model: "gpt-4o" }
  │
  └─ Successful response from gpt-4o
```

### Example 2: Cheap Request with Fallback

```
LlGateway::chat(LlRequest { capability: Some("code"), hints: ["cheap"], ... })
  │
  ├─ Router::resolve("code", ["cheap"])
  │     ├─ Filtered by cost: llama3.2 ($0), claude-3-sonnet ($3/1k)
  │     ├─ Selected: llama3.2 (free)
  │     └─ Returns ResolvedEndpoint { provider: "ollama", model: "llama3.2" }
  │
  ├─ Provider::chat() → 503 (Ollama not running)
  │
  ├─ Fallback: Router::resolve_next("code", exclude=["ollama"])
  │     ├─ Filtered by cost (next cheapest): claude-3-sonnet
  │     └─ Returns ResolvedEndpoint { provider: "anthropic", model: "claude-3-sonnet" }
  │
  └─ Successful response from claude-3-sonnet
```

### Example 3: Vision Request (Capability Match)

```
LlGateway::chat(LlRequest { messages: [user: { image: ... }], capability: Some("vision"), ... })
  │
  ├─ Router::resolve("vision", [])
  │     ├─ Models with vision capability: gpt-4o, gemini-2.0-flash
  │     ├─ Selected: gpt-4o (priority higher than gemini)
  │     └─ Returns ResolvedEndpoint { provider: "openai", model: "gpt-4o" }
  │
  └─ Successful response from gpt-4o with image understanding
```
