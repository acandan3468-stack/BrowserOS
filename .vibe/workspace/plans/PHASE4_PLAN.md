# Phase 4 Plan — browseros-llm

## Phase 4 Objectives

1. Harden the public API (non-exhaustive enums, Display impls, naming cleanup)
2. Add streaming tool-call delta support across all adapters
3. Implement the MCP adapter (Model Context Protocol) — MVP scope
4. Add feature flags for optional dependencies and transports
5. Add budget enforcement and optional model discovery

**Binding constraint**: Do NOT change frozen Phase 3 public API signatures.
All additions must be backward-compatible. Breaking changes require a Phase 5
or a major version bump.

---

## Execution Order

### Phase 4A — API Hardening

**Objectives**:
- Add `#[non_exhaustive]` to 4 enums (`LlContent`, `LlRole`, `LlFinishReason`, `ProviderHealth`)
- Add `Display` impl for `LlContent`
- Fix `CostSnapshot` naming collision (`cost::CostSnapshot` → `CostTrackerSnapshot`)
- Document `LlStreamChunk.index` non-zero contract
- `Clone` on `LlStreamChunk` and `LlStreamEvent` (safe additive derives)

**Deliverables**:
- Updated enum declarations with `#[non_exhaustive]`
- `impl Display for LlContent`
- `CostTrackerSnapshot` replaces the internal `cost::CostSnapshot` name
- Doc comment on `LlStreamChunk.index`

**Acceptance Criteria**:
- All 4 enums accept new variants externally without match errors
- `LlContent` prints human-readable text
- `CostSnapshot` appears only in `types` module (no naming collision)
- Existing 458 tests pass

**Exit Criteria**: `cargo clippy -- -D warnings` clean, 458+ tests pass,
all public API consumers can migrate without breakage.

**Risk Level**: LOW

---

### Phase 4B — Streaming Revision

**Objectives**:
- Add `tool_calls: Vec<LlToolCallDelta>` to `ProviderStreamEvent::Chunk`
- Update Gateway bridge (`chat_stream`) to forward `tool_calls` directly
- Update each adapter's streaming parser to emit `tool_calls`
- Ensure `LlStreamChunk.tool_calls` is always populated (never `Vec::new()`)

**Data flow — streaming tool-call contract**:

```
Adapter (parse_chunk)
    ↓
ProviderStreamEvent::Chunk { content, finish_reason, tool_calls }
    ↓  (forwarded directly, no aggregation)
Gateway (chat_stream background thread)
    ↓  (birebir forwarding, never Vec::new())
LlStreamChunk { content, finish_reason, tool_calls, index }
    ↓
Consumer (LlStreamHandle::iter / recv / try_recv)
```

Key rules:
- `ProviderStreamEvent::Chunk.tool_calls` → `LlStreamChunk.tool_calls` is
  a direct 1:1 forward. The Gateway never replaces it with `Vec::new()`.
- Each adapter decides when a tool-call delta occurs and emits a chunk.
- Empty `tool_calls` means a text-only delta (most common case).
- Non-empty `tool_calls` means a tool-call is starting or its arguments
  are being streamed (OpenAI delta format) or a complete tool call arrived
  (Anthropic/Gemini content_block format).

**Adapter-specific notes**:

| Adapter | Current streaming behaviour | Phase 4B change |
|---------|---------------------------|-----------------|
| OpenAI | Parses `delta.tool_calls[].function.arguments` into `StreamAggregator`, discards in streaming, emits only on `Done` | Emit each tool-call delta as a chunk with partial `LlToolCallDelta` |
| Anthropic | `content_block_delta` for text only. `content_block_start` with `tool_use` returns `None` | Emit `content_block_start` as chunk with complete `LlToolCallDelta` |
| Gemini | Tool calls in non-streaming only. Streaming is text-only | After streaming finishes, if final message has tool calls, emit as final chunk |
| Ollama | No tool support. Streaming is text-only | No change needed |
| Generic HTTP | Mirrors OpenAI format | Same change as OpenAI |

**Deliverables**:
- `ProviderStreamEvent::Chunk` gains `tool_calls` field
- Gateway `chat_stream` updated to forward the field
- 5 adapter streaming parsers updated
- Tests: tool-call delta emission for OpenAI, Anthropic, Gemini

**Acceptance Criteria**:
- Streaming response with tool calls emits `LlStreamChunk` with
  `tool_calls` populated (not empty for tool-call segments)
- Non-streaming behaviour unchanged
- All existing tests pass

**Exit Criteria**: Integration tests verify tool-call deltas arrive at
consumer for all adapters that support tools.

**Risk Level**: MEDIUM (provider format divergence)

---

### Phase 4C — MCP Core (MVP)

**Objectives**:
- Implement MCP stdio transport (subprocess stdin/stdout)
- Implement JSON-RPC 2.0 message layer (request/response/notification)
- Implement tool discovery (`tools/list` → `LlTool`)
- Implement tool execution (`tools/call` → `LlToolCallDelta`)
- Wire MCP adapter as an `LlProvider` implementation
- Move `src/mcp/` back to `pub mod mcp` and add `McpAdapter` to crate exports

**MVP scope — IN**:
- ✓ stdio transport (subprocess)
- ✓ JSON-RPC 2.0 (request, response, error, notification)
- ✓ Tool discovery (`tools/list`)
- ✓ Tool execution (`tools/call`)
- ✓ Single server per adapter instance
- ✓ Subprocess lifecycle (start, health check, shutdown on Drop)
- ✓ Error handling (timeout, crash, malformed response)

**MVP scope — OUT** (deferred to Phase 4+):
- ✗ WebSocket transport
- ✗ SSE transport (HTTP)
- ✗ Multi-server routing (one adapter = one MCP server)
- ✗ Remote session management
- ✗ Advanced protocol extensions (logging, sampling, resources)
- ✗ MCP over network crate (browseros-network will handle this in Phase 2.6)

**Integration with existing types**:

| MCP concept | Existing type | Mapping |
|------------|---------------|---------|
| `tools/list` result → tool definition | `LlTool` | `name`, `description`, `inputSchema` → `parameters` (Value) |
| `tools/call` result → tool output | `LlToolCallDelta` | Full result stored as `LlToolCallDelta { index: 0, id, name, arguments: serde_json::Value }` |
| `tools/call` in streaming | `LlStreamChunk.tool_calls` | Non-streaming only in MVP. If server supports streaming, future work. |
| Server identity | `McpServerConfig.name` | Used as provider ID prefix (e.g. `mcp-filesystem`) |
| Server config | `McpServerConfig { command, args, transport, base_url, auto_start }` | `transport: "stdio"` for MVP. `base_url` reserved for WebSocket future. |

**Dependency strategy**:

| Transport | MVP? | Deps required | Notes |
|-----------|------|---------------|-------|
| stdio | ✓ MVP | `std::process` (stdlib), `serde_json` (already present) | Zero new dependencies |
| JSON-RPC | ✓ MVP | Manual impl over `serde_json::Value` | No crate needed — JSON-RPC 2.0 is 6 field types |
| SSE | ✗ Future | `ureq` (already present) | Easy to add later |
| WebSocket | ✗ Future | `tungstenite` (blocking, new dep) | Feature-gated behind `websocket` flag. Not in MVP. |

**Deliverables**:
- `src/mcp/mod.rs` — `McpTransport`, `JsonRpcMessage`, `McpAdapter`
- `src/mcp/stdio.rs` — subprocess management, stdin/stdout I/O
- `src/mcp/jsonrpc.rs` — request/response types, parser
- `src/mcp/tool_registry.rs` — tool list caching, execution dispatch
- `McpAdapter` implementing `LlProvider` (chat + health only for MVP)
- Integration test with a real MCP stdio server

**Acceptance Criteria**:
- `McpAdapter` connects to a stdio MCP server, discovers tools, executes `tools/call`
- Tools appear in `capabilities() -> [ToolUse]`
- Tool call responses arrive in `LlResponse.message.content`
- Server subprocess is cleaned up on `McpAdapter` Drop
- MCP errors propagate as `LlmError::ProviderError`

**Exit Criteria**: Integration test passes with a known stdio MCP server
(e.g. `@modelcontextprotocol/server-filesystem`).

**Risk Level**: HIGH

**Risk rationale**:
1. MCP spec is evolving — tool call format may change (mitigation: pin version)
2. Subprocess lifecycle — zombie processes if not handled (mitigation: guarded Drop)
3. JSON-RPC edge cases — batch, notifications, parse errors (mitigation: strict parsing first)

---

### Phase 4D — Feature Flags

**Objectives**:
- Add `mcp` feature (enabled by default) for MCP adapter
- Add `har` feature for HAR data types (scoped as `llm-har` to avoid
  workspace collision with `browseros-network`)
- Gate `ureq` behind a default feature (`http-client`)
- Prepare `websocket` feature for future WebSocket transport (added now,
  unused until Phase 4+)

**Deliverables**:
- Updated `Cargo.toml` with feature declarations
- `#[cfg(feature = "mcp")]` on MCP adapter module
- `#[cfg(feature = "har")]` on HAR types (if any)
- `#[cfg(feature = "http-client")]` on `ureq` imports in adapters
- Feature validation: `cargo build --no-default-features` succeeds

**Acceptance Criteria**:
- `cargo build --no-default-features` passes with gateway, cache, router,
  cost, telemetry, error types (no HTTP, no MCP, no HAR)
- `cargo build` (default) includes all features
- Existing 458 tests pass under default features

**Exit Criteria**: Feature matrix tested: `--no-default-features`,
default, `--features mcp`, `--all-features`.

**Risk Level**: LOW

---

### Phase 4E — Budget Enforcement & Model Discovery

**Objectives**:
- Add `CostTracker::set_budget(cents)` runtime budget adjustment
- Add `CostTracker::is_budget_exceeded() -> bool`
- Wire budget check in `LlGateway::chat()` before request dispatch
- Add `LlProvider::discover_models() -> Vec<String>` with default impl

**Design decisions**:
- Budget check happens after cache lookup (cached responses are free)
- Budget exceedance returns `LlmError::ConfigurationError("budget exceeded")`
- Budget is soft by default — can be made hard via config flag in Phase 5
- `discover_models()` default impl returns `self.models()` (configured list)

**Deliverables**:
- `CostTracker::set_budget(cents: u64)` — dynamic budget adjustment
- `CostTracker::is_budget_exceeded() -> bool` — budget check
- `LlGateway::chat()` budget check gate
- `LlProvider::discover_models()` — default method on trait
- OpenAI adapter override: calls `GET /v1/models` to discover

**Acceptance Criteria**:
- Request rejected when budget exceeded with clear error
- Budget can be adjusted at runtime after `CostTracker` construction
- `discover_models()` returns configured models by default
- OpenAI adapter returns live model list from API
- All existing tests pass

**Exit Criteria**: Integration test verifies budget exceeded → request rejected.

**Risk Level**: MEDIUM

**Risk rationale**: Budget enforcement changes Gateway semantics from
"best-effort" to "enforced". Cached responses bypass budget — this is
intentional (already paid for), but must be clearly documented.

---

## Dependency Strategy

### MVP (Phase 4C)

| Dependency | Status | Purpose |
|------------|--------|---------|
| `serde_json` | Already present | JSON-RPC message parsing |
| `std::process` | stdlib | Subprocess management for stdio transport |
| `std::io::BufRead` | stdlib | Line-based reading from subprocess stdout |

**Zero new runtime dependencies for MVP.**

### Future (Phase 4+)

| Dependency | Status | Gating | Purpose |
|------------|--------|--------|---------|
| `tungstenite` (blocking) | New | `#[cfg(feature = "websocket")]` | WebSocket transport for remote MCP servers |
| `ureq` | Already present | `#[cfg(feature = "http-client")]` | SSE transport via HTTP (future) |

WebSocket transport is explicitly **not** part of the Phase 4 MVP. It is
deferred to a future phase. The feature flag scaffolding (`websocket`)
may be added in Phase 4D to prepare, but the implementation is deferred.

---

## Streaming Contract

### Data flow

```
ADAPTER LAYER
    │
    ▼
ProviderStreamEvent::Chunk {
    content: String,
    finish_reason: Option<LlFinishReason>,
    tool_calls: Vec<LlToolCallDelta>,      ← NEW in Phase 4B
}
    │
    │  (birebir forward — Gateway does NOT transform or filter)
    ▼
GATEWAY (chat_stream background thread)
    │
    │  forwarding rule:
    │    LlStreamChunk.content          = ProviderStreamEvent::Chunk.content
    │    LlStreamChunk.finish_reason    = ProviderStreamEvent::Chunk.finish_reason
    │    LlStreamChunk.tool_calls       = ProviderStreamEvent::Chunk.tool_calls  (NEVER Vec::new())
    │    LlStreamChunk.index            = incrementing counter
    │
    ▼
LlStreamChunk {
    content: String,
    finish_reason: Option<LlFinishReason>,
    tool_calls: Vec<LlToolCallDelta>,
    index: usize,
}
    │
    ▼
CONSUMER (LlStreamHandle::iter / recv / try_recv)
```

### Contract rules

1. `tool_calls` is `Vec::new()` for text-only chunks (most common).
2. `tool_calls` is non-empty when a tool-call delta or complete tool call
   is being delivered.
3. The Gateway never produces `tool_calls: Vec::new()` to replace a value
   that came from the adapter.
4. Each adapter's `parse_chunk` is responsible for deciding when a chunk
   contains tool-call data and emitting it.
5. The consumer merges `LlToolCallDelta` values across chunks using
   `LlToolCallDelta::merge_arguments()` if needed (OpenAI delta format).
   For Anthropic/Gemini, each tool call is complete in a single chunk.

---

## MCP Scope — Phase 4 MVP

### Supported
- ✓ stdio transport (subprocess stdin/stdout)
- ✓ JSON-RPC 2.0 request/response/error/notification
- ✓ Tool discovery (`tools/list`)
- ✓ Tool execution (`tools/call`)
- ✓ Single MCP server per adapter instance
- ✓ Subprocess lifecycle (spawn, health, shutdown on Drop)
- ✓ Error handling (timeout, crash, malformed JSON-RPC)
- ✓ Mapping to `LlProvider` trait (chat delegates to tool calls)

### Not supported (deferred)
- ✗ WebSocket transport — future, feature-gated
- ✗ SSE transport — future
- ✗ Multi-server routing within one adapter — future
- ✗ Remote session management — future
- ✗ Protocol extensions (resources, logging, sampling) — Phase 4+
- ✗ Streaming MCP responses — Phase 4+

---

## Phase Boundary Summary

| Phase | Name | Risk | Est. time | Depends on |
|-------|------|------|-----------|------------|
| 4A | API Hardening | LOW | 0.5-1 day | None |
| 4B | Streaming Revision | MEDIUM | 1-2 days | 4A |
| 4C | MCP Core (MVP) | HIGH | 3-5 days | 4A, 4B |
| 4D | Feature Flags | LOW | 0.5 day | 4C (can overlap) |
| 4E | Budget + Discovery | MEDIUM | 1-2 days | 4A |

Total estimated time: 6-11 days.

---

## Risk Analysis

| # | Risk | Severity | Likelihood | Mitigation | Phase |
|---|------|----------|------------|------------|-------|
| 1 | Anthropic streaming tool-call format differs from OpenAI (content_block_start vs delta.tool_calls) | High | High | Adapter-specific parsing; integration tests per provider | 4B |
| 2 | MCP subprocess lifecycle — zombie processes on crash | High | Medium | `Child::wait()` on Drop; timeout+kill on spawn failure; panic-guarded Drop | 4C |
| 3 | MCP spec changes during implementation | High | Medium | Pin MCP spec version; abstract transport behind trait | 4C |
| 4 | `LlStreamHandle::Clone` with `mpsc::Receiver` infeasible | Medium | High | Defer to Phase 4+ if needed; Phase 4A only adds Clone to LlStreamChunk/LlStreamEvent | 4A |
| 5 | Provider streaming format changes (OpenAI/Anthropic API updates) | Medium | Low | Per-adapter parsing; integration tests catch regressions | 4B |
| 6 | `#[non_exhaustive]` changes compiler error for external match arms | Low | Low | Documented in changelog; users add `_ =>` arm | 4A |
| 7 | Budget enforcement changes Gateway semantics (cached responses skip budget) | Medium | Medium | Check budget after cache hit; cached responses are already paid for | 4E |
| 8 | `discover_models()` default impl returns empty — useless | Low | High | OpenAI adapter overrides with live API call; default returns configured list | 4E |
| 9 | Feature flag `har` collides with `browseros-network` | Low | Medium | Use scoped name (`llm-har`) in workspace coordination | 4D |
| 10 | MCP adapter doesn't fit `LlProvider` streaming model (MCP tools are non-streaming) | Low | High | MCP adapter returns complete response in `chat()`; no streaming tool calls in MVP | 4C |
| 11 | OpenAI `tool_calls` delta accumulates across chunks — gateway must not aggregate | Medium | Medium | Gateway forwards raw deltas; consumer merges via `LlToolCallDelta::merge_arguments()` | 4B |
| 12 | MCP JSON-RPC parse errors on malformed server responses | Medium | Medium | Strict deserialisation; return `LlmError::MalformedResponse` on failure | 4C |
| 13 | MCP subprocess timeout on startup or tool execution | Medium | Medium | Configurable timeout in `McpServerConfig`; default 10s | 4C |
| 14 | Gemini streaming tool calls missing (tools never appear in streaming chunks) | Low | Low | Acceptable — Gemini tools are non-streaming; emit final chunk with tool data after stream ends | 4B |
| 15 | Cumulative risk: all phases in parallel destabilise the crate | High | Low | Sequential execution with overlapping permitted only for 4C ↔ 4D | All |

---

## Expected New Public APIs

After Phase 4:

- `browseros_llm::mcp::McpAdapter` — MCP provider adapter (implements `LlProvider`)
- `browseros_llm::mcp::McpTransport` — MCP transport enum (`Stdio` for MVP)
- `browseros_llm::mcp::ToolRegistry` — registry of MCP-discovered tools
- `ProviderStreamEvent::Chunk.tool_calls: Vec<LlToolCallDelta>` — streaming tool calls
- `browseros_llm::CostTracker::set_budget(cents: u64)`
- `browseros_llm::CostTracker::is_budget_exceeded() -> bool`
- Feature-gated modules: `#[cfg(feature = "mcp")]`, `#[cfg(feature = "har")]`,
  `#[cfg(feature = "http-client")]`

**No changes** to existing public method signatures on `LlGateway`, `LlProvider`,
or any config type.

---

## Integration Points

- **browseros-dag**: Phase 4 MCP tools consumed by the DAG planner.
  Planner calls `LlGateway::chat()` with `LlTool` definitions — MCP tools
  are opaque to the planner.
- **browseros-network** (Phase 2.6): No direct integration in MVP. If MCP
  runs over WebSocket in the future, network crate may provide the transport
  layer.
- **browseros-types**: No changes expected. Existing `LlTool`, `LlToolCallDelta`,
  and `CorrelationId` types are sufficient.

---

## Success Criteria

1. All 4 enums have `#[non_exhaustive]`
2. `LlContent` implements `Display`
3. `CostSnapshot` naming collision resolved
4. Streaming tool-call deltas work across OpenAI and Anthropic adapters
5. MCP adapter passes integration test with a real stdio MCP server
6. `cargo build --no-default-features` succeeds
7. Budget enforcement prevents requests when budget exceeded
8. All existing 458 tests continue to pass
9. `cargo clippy -- -D warnings` and `cargo fmt --check` pass
10. No breaking changes to the Phase 3 frozen API
