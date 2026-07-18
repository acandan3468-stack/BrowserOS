# LLM Architecture Analysis

**Phase:** 2.1 — Design Freeze  
**Status:** Analysis Complete  

---

## 1. Current State Assessment

### What Exists

| Component | Status | Description |
|---|---|---|
| `PlannerBridge` trait | ✅ | `fn plan(&self, request) -> PlanningResult` — provider-agnostic |
| `PlannerRequest` | ✅ | `goal: String`, `context: PlanningContext`, `parameters` |
| `PlannerResponse` | ✅ | `plan: ExecutionPlan`, `validation`, `metadata` |
| `PlanningContext` | ✅ | `available_capabilities`, `constraints`, `execution_mode`, `correlation_id` |
| `ExecutionPlan` | ✅ | `intents`, `dependencies`, `variables`, `constraints` |
| `ExecutionIntent` | ✅ | `capability`, `input`, `target`, `constraints`, `hints` |
| `CapabilityMetadata` | ✅ | `name`, `description`, `required_params`, `timeout`, `retryable` |
| `NodeRegistry` | ✅ | `list_capabilities()`, `list_capability_metadata()` — MCP discovery surface |
| `EventBus` | ✅ | Publish/subscribe for all internal communication |
| `RuntimeContext` | ✅ | DI container: logger, metrics, tracer, event-bus, lifecycle, scheduler, dag |

### What Does NOT Exist

- No LLM provider abstractions (no `Provider` trait, no adapter)
- No LLM Gateway (no routing, fallback, retry, cost tracking)
- No prompt layer (no system prompt management, no template engine)
- No conversation state management
- No memory integration
- No MCP adapter (only `PlannerType::MCP` enum variant exists)
- No token counting or cost accounting
- No model registry
- No streaming response support
- No tool-calling abstraction
- No embedding or vector search
- No LLM-specific observability events

### Gap Analysis

The current architecture recognizes that LLM integration is needed (via `PlannerType::LLM`, `PlannerType::MCP` enum variants and the `PlannerBridge` trait), but provides zero infrastructure for actually communicating with LLM providers. The gap is not in the planner interface — that is well-designed. The gap is the entire layer between `PlannerBridge` and the actual HTTP/WebSocket transport to LLM backends.

---

## 2. Key Architectural Decisions

### Decision 1: `browseros-llm` is a NEW crate

**Rationale:**
- The LLM Gateway is a cross-cutting concern that multiple consumers will use (Planner, MCP adapter, CLI, future agents)
- It must be at the correct layer to avoid cyclic dependencies
- It must remain provider-agnostic — a separate crate enforces this boundary
- It has its own external dependencies (reqwest, tokenizers, etc.) that should not leak into `browseros-dag`

### Decision 2: The LLM Gateway sits BETWEEN Runtime and Planner

```
RuntimeContext
  └── browseros-llm (Gateway)
        └── Planner (LLM implementation of PlannerBridge)
              └── DagEngine
```

The Gateway is protocol-agnostic. The Planner is a consumer of the Gateway.

### Decision 3: MCP is an ADAPTER, not the runtime

MCP is one protocol among many. The MCP adapter translates between BrowserOS internal types and MCP protocol messages. It does NOT own the LLM Gateway.

### Decision 4: No direct HTTP in planner code

Planner code never sees `reqwest`, `hyper`, or any transport library. It communicates exclusively through the Gateway trait interface.

---

## 3. Layer Placement

```
L4: browseros-runtime   ─── holds Arc<LlGateway>
L3: browseros-dag       ─── PlannerBridge trait (unchanged, frozen)
                         browseros-llm (NEW) ─── Gateway, Provider trait, adapters
L2: browseros-scheduler  browseros-observability
L1: browseros-event-bus  browseros-config  browseros-bridge
L0: browseros-types
```

`browseros-llm` lives at L3 alongside `browseros-dag`. It depends on:
- `browseros-types` (identifiers, events)
- `browseros-event-bus` (publish LLM events)
- `browseros-observability` (metrics, tracing)

It explicitly does NOT depend on:
- `browseros-dag` (no planner types in the Gateway)
- `browseros-bridge` (no browser port traits)
- `browseros-browser`, `browseros-cdp`, etc.

---

## 4. Provider Landscape

| Provider | API Style | Key Challenge |
|---|---|---|
| OpenAI | REST + SSE | Standard, well-documented |
| Anthropic | REST + SSE | Different message format (roles, content blocks) |
| Google Gemini | REST + SSE | Different content structure, no tool-use in streaming |
| Ollama | REST (local) | No auth, no SSE streaming |
| LM Studio | REST (local) | OpenAI-compatible, no auth |
| vLLM | REST (local) | OpenAI-compatible, supports streaming |
| OpenRouter | REST + SSE | Proxy — single API for many models |
| Azure OpenAI | REST + SSE | OpenAI format + headers |
| Generic HTTP | REST | Minimal contract — `/chat`, `/completions` |

The design MUST NOT hardcode any of these. Each is an adapter behind a common trait.

---

## 5. Risks

| Risk | Severity | Mitigation |
|---|---|---|
| Provider API drift | High | Interface is trait-based; providers map to trait, not to protocol |
| Token counting inconsistency | Medium | Abstract `Tokenizer` trait; models register their tokenizer |
| Streaming complexity | Medium | `StreamingProvider` subtrait; sync fallback if stream fails |
| Cost tracking accuracy | Medium | Approximate via model × token count × known rates |
| MCP protocol versioning | Medium | Adapter pattern isolates MCP version changes |
| Prompt injection | Medium | System prompt isolation; input sanitization in Gateway |
| Rate limiting across providers | Low | Gateway handles retry/backoff; provider reports rate state |

---

## 6. Design Principles

1. **Zero provider strings in planner code** — Planner never knows about OpenAI, Anthropic, etc.
2. **MCP is one adapter** — The Gateway has zero MCP knowledge
3. **All LLM communication flows through the Gateway** — No direct HTTP calls outside `browseros-llm`
4. **Provider = trait implementation** — Adding a provider means implementing 1-2 traits
5. **Events are the observability fabric** — Every LLM call publishes through EventBus
6. **Token counting is abstract** — Provider reports tokens; Gateway does not parse model internals
7. **Cost tracking is advisory** — Uses estimated rates, not real-time billing
8. **Memory belongs outside LLM** — Conversation history is managed by the consumer (Planner, agent), not by browseros-llm
