# Production Validation Checklist

## Provider configuration templates

### OpenAI

```toml
[llm]
default_provider = "openai"

[[llm.providers]]
provider_id = "openai"
provider_type = "openai"
api_url = "https://api.openai.com/v1"
api_key = "${OPENAI_API_KEY}"
models = ["gpt-4o-mini", "gpt-4o"]

[[llm.models]]
id = "gpt-4o-mini"
provider = "openai"
capabilities = ["chat", "streaming", "tool_use"]
max_tokens = 16384
context_window = 128000
cost_per_1k_input = 0.00015
cost_per_1k_output = 0.0006
```

### Anthropic

```toml
[[llm.providers]]
provider_id = "anthropic"
provider_type = "anthropic"
api_url = "https://api.anthropic.com/v1"
api_key = "${ANTHROPIC_API_KEY}"
models = ["claude-3-haiku-20240307"]

[[llm.models]]
id = "claude-3-haiku-20240307"
provider = "anthropic"
capabilities = ["chat", "streaming", "tool_use"]
max_tokens = 4096
context_window = 200000
```

### Gemini

```toml
[[llm.providers]]
provider_id = "gemini"
provider_type = "gemini"
api_url = "https://generativelanguage.googleapis.com/v1beta"
api_key = "${GEMINI_API_KEY}"
models = ["gemini-1.5-flash"]

[[llm.models]]
id = "gemini-1.5-flash"
provider = "gemini"
capabilities = ["chat", "streaming", "tool_use"]
max_tokens = 8192
context_window = 1048576
```

### Ollama

```toml
[[llm.providers]]
provider_id = "ollama"
provider_type = "ollama"
api_url = "http://localhost:11434"
models = ["llama3.2"]

[[llm.models]]
id = "llama3.2"
provider = "ollama"
capabilities = ["chat", "streaming"]
max_tokens = 4096
context_window = 8192
```

### Generic HTTP (OpenAI-compatible)

```toml
[[llm.providers]]
provider_id = "my-custom"
provider_type = "http_generic"
api_url = "https://my-llm.example.com/v1"
api_key = "${CUSTOM_API_KEY}"
models = ["my-model"]

[[llm.models]]
id = "my-model"
provider = "my-custom"
capabilities = ["chat", "streaming", "tool_use"]
```

### MCP stdio

```toml
[llm.mcp]
capabilities.planning = ["filesystem"]  # MCP tool capability prefix

[[llm.mcp.servers]]
name = "filesystem"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
auto_start = true
```

---

## Checklist by provider

### 1. OpenAI

- [ ] **Chat basic**: `LlGateway::chat()` with `model = "gpt-4o-mini"` returns `LlResponse`
- [ ] **Chat with system prompt**: System prompt appears in assistant behaviour
- [ ] **Chat with temperature**: Lower temperature reduces variability empirically
- [ ] **Streaming**: `chat_stream()` produces `LlStreamEvent::Chunk` sequence, terminates with `Done`
- [ ] **Streaming order**: Chunk indices are sequential, no gaps
- [ ] **Tool calling**: Request with `LlTool` triggers `ToolCall` in response
- [ ] **Tool calling multiple**: Multiple tool calls returned in one response
- [ ] **Retry**: Set `max_retries = 3`, inject transient network failure → retry succeeds
- [ ] **Timeout**: Set `timeout_secs = 1`, call slow endpoint → `LlmError::Timeout`
- [ ] **Rate limiting**: Exceed TPM/RPM → retry with backoff, eventual success or `AllProvidersFailed`
- [ ] **Auth failure**: Use `api_key = "invalid"` → `LlmError::ConfigurationError` or `ProviderError`
- [ ] **Cache**: Identical request twice → second returns cached response (same `cost_snapshot`)
- [ ] **Cache disabled**: `cache.enabled = false` → both requests hit wire
- [ ] **Telemetry**: Events emitted to event bus; metrics counters increment
- [ ] **Cost tracking**: Token counts match between response and `cost_snapshot()`
- [ ] **Embedding**: `embed()` with `text-embedding-3-small` returns `LlEmbedResponse`

### 2. Anthropic

- [ ] **Chat basic**: `chat()` with `claude-3-haiku` returns `LlResponse`
- [ ] **Chat long context**: 50k+ token message completes without truncation
- [ ] **Streaming**: `chat_stream()` produces content block sequence
- [ ] **Streaming tool calls**: `content_block_start(tool_use)` emits `LlToolCallDelta`
- [ ] **Tool calling**: Tools defined → Claude executes them
- [ ] **Tool calling with image**: Vision + tool call in single request
- [ ] **Retry**: Transient error → retry succeeds (Anthropic 529 handling)
- [ ] **Timeout**: Low timeout → `LlmError::Timeout`
- [ ] **Auth failure**: Invalid key → `LlmError::ConfigurationError` or `ProviderError`
- [ ] **Cache**: Response caching works
- [ ] **Cost tracking**: Token counts match Anthropic's reported usage

### 3. Gemini

- [ ] **Chat basic**: `chat()` with `gemini-1.5-flash` returns `LlResponse`
- [ ] **Chat multi-turn**: 5-message conversation maintains context
- [ ] **Streaming**: `chat_stream()` produces chunk sequence
- [ ] **Streaming final tool call**: Tool call emitted in final chunk after stream end
- [ ] **Tool calling**: Tools defined → Gemini returns `functionCall`
- [ ] **Embedding**: `embed()` returns `LlEmbedResponse`
- [ ] **Retry**: Transient error → retry succeeds
- [ ] **Timeout**: Low timeout → `LlmError::Timeout`
- [ ] **Auth failure**: Invalid key → `LlmError::ConfigurationError`
- [ ] **Cache**: Response caching works
- [ ] **Cost tracking**: Token counts match Gemini's reported usage

### 4. Ollama

- [ ] **Chat basic**: `chat()` with local model returns `LlResponse`
- [ ] **Chat streaming**: `chat_stream()` produces real-time tokens
- [ ] **Embedding**: `embed()` returns `LlEmbedResponse`
- [ ] **No tools**: Tool request returns error or is gracefully ignored (adapter behaviour)
- [ ] **Retry**: Kill ollama process, restart → retry succeeds
- [ ] **Timeout**: Set low timeout → `LlmError::Timeout` (works for unresponsive server)
- [ ] **Cache**: Response caching works
- [ ] **Health**: `health()` returns `Healthy` when server is up
- [ ] **Health offline**: `health()` returns `Unavailable` when server is down
- [ ] **Cost tracking**: Cost is zero (local model)

### 5. Generic HTTP (OpenAI-compatible)

- [ ] **Chat basic**: `chat()` returns `LlResponse` from custom endpoint
- [ ] **Streaming**: `chat_stream()` from SSE-compatible endpoint
- [ ] **Tool calling**: Tools passed through as OpenAI-format JSON
- [ ] **Custom headers**: Custom auth header in `custom_headers` transmitted
- [ ] **Non-standard URL**: URL path with version prefix works
- [ ] **Retry**: Transient error → retries with backoff
- [ ] **Timeout**: Low timeout → `LlmError::Timeout`
- [ ] **Auth failure**: Wrong key → `ProviderError`
- [ ] **Malformed response**: Server returns HTML → `LlmError::MalformedResponse`
- [ ] **Cache**: Response caching works

### 6. MCP stdio

- [ ] **Spawn + initialize**: Subprocess spawns, JSON-RPC handshake succeeds
- [ ] **Tool discovery**: `tools/list` returns > 0 tools, mapped to `LlTool`
- [ ] **Tool execution**: Call discovered tool, verify output in response
- [ ] **Tool execution with args**: Arguments serialised to JSON, tool uses them
- [ ] **Tool error response**: Tool returns `isError: true` → `LlmError::ProviderError`
- [ ] **Unknown tool**: Call nonexistent tool → `McpError::ToolNotFound`
- [ ] **Sequential calls**: 5 rapid tool calls all succeed
- [ ] **Multi-server**: 2 MCP servers configured, both discoverable
- [ ] **Subprocess crash**: Kill subprocess → next call returns error
- [ ] **Subprocess restart**: After crash, new `McpAdapter` spawns and works
- [ ] **Health before init**: `health()` → `Unavailable`
- [ ] **Health after init**: `health()` → `Healthy`
- [ ] **Drop cleanup**: Dropping adapter kills subprocess (no zombie)
- [ ] **Timeout on tool call**: Short timeout → `McpError::Timeout`
- [ ] **Large response**: Tool returns 5MB+ text → either succeeds or returns size error
- [ ] **Concurrent calls**: 5 parallel `call_tool()` → all complete (thread safety)
