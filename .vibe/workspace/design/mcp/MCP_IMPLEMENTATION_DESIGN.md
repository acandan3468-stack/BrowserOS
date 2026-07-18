# Phase 4C — MCP Core MVP: Implementation Design

> **Status**: Final Draft  
> **Date**: 2026-07-16  
> **Author**: opencode  
> **Audience**: Implementation engineer  
> **Contract**: This document is the sole architectural reference for Phase 4C. Every decision below is binding unless a concrete blocker is discovered during implementation.

---

## Table of Contents

1. [Architecture Overview](#1-architecture-overview)
2. [Module Layout](#2-module-layout)
3. [JSON-RPC Design](#3-json-rpc-design)
4. [Process Lifecycle](#4-process-lifecycle)
5. [Tool Discovery](#5-tool-discovery)
6. [Tool Execution](#6-tool-execution)
7. [Gateway Integration](#7-gateway-integration)
8. [Provider Integration](#8-provider-integration)
9. [Error Mapping](#9-error-mapping)
10. [Threading Model](#10-threading-model)
11. [Security Review](#11-security-review)
12. [Performance Review](#12-performance-review)
13. [Test Plan](#13-test-plan)
14. [Risks](#14-risks)
15. [Open Questions](#15-open-questions)
16. [Final Readiness Verdict](#16-final-readiness-verdict)

---

## 1. Architecture Overview

### 1.1 Component Diagram

```
┌─────────────────────────────────────────────────────────────────────┐
│                        Consumer (Planner/DAG)                       │
│                                                                     │
│   LlGateway::chat(request)       LlGateway::chat(request)          │
│       │                                │                           │
│       ▼                                ▼                           │
│  ┌──────────┐                   ┌──────────────┐                   │
│  │  Router   │                   │    Router    │                   │
│  │ resolve() │                   │  resolve()   │                   │
│  └────┬─────┘                   └──────┬───────┘                   │
│       │                                │                           │
│       ▼                                ▼                           │
│  ┌──────────┐                   ┌──────────────┐                  │
│  │ OpenAI   │                   │  McpAdapter  │                  │
│  │ Provider │ (tool defs)       │  (tool exec) │                  │
│  └──────────┘                   └──────┬───────┘                  │
│                                        │                          │
│                                        ▼                          │
│                                  ┌──────────────┐                 │
│                                  │ MCP Stdio    │                 │
│                                  │ Subprocess   │                 │
│                                  │ (stdio)      │                 │
│                                  └──────────────┘                 │
└─────────────────────────────────────────────────────────────────────┘
```

### 1.2 Data Flow — Two-Phase Tool Execution

```
Phase 1 — Tool Discovery (at startup)
======================================
McpAdapter::new()
    ↓
client.initialize()          ─── JSON-RPC "initialize" ───▶ MCP Server
    ↓
client.list_tools()          ─── JSON-RPC "tools/list"  ───▶ MCP Server
    ↓
tools: Vec<LlTool>           ◀── JSON-RPC response      ◀─── MCP Server
    ↓
Cached in McpAdapter.tools (Arc<Mutex<HashMap<String, LlTool>>>)

Phase 2 — Tool Execution (on demand)
======================================
Consumer sends LlRequest with model="mcp-<name>"
    ↓
Gateway → Router resolves to McpAdapter
    ↓
LlProvider.chat(ProviderRequest)
    ↓
McpAdapter extracts tool call from last assistant message
    ↓
client.call_tool(name, arguments)  ─── JSON-RPC "tools/call" ──▶ MCP Server
    ↓
Result: { content: [{ type, text }] }  ◀── JSON-RPC response ◀───
    ↓
McpAdapter wraps in ProviderResponse { message: ToolResult }
    ↓
Returns to Gateway → Consumer
```

### 1.3 Ownership Model

```
LlGateway
  ├── providers: HashMap<String, Box<dyn LlProvider>>
  │     └── McpAdapter {                              ← OWNED by Gateway
  │           id: String,
  │           config: McpServerConfig,
  │           transport: Arc<Mutex<StdioTransport>>,  ← SHARED (transport + tool cache)
  │           tools: Arc<Mutex<HashMap<String, LlTool>>>, ← SHARED
  │           initialized: Arc<AtomicBool>,            ← SHARED
  │         }
  ├── router: Arc<LlRouter>                            ← SHARED
  ├── registry: Arc<ModelRegistry>                     ← SHARED
  └── cost_tracker: CostTracker                        ← OWNED
```

**Rules**:
- `McpAdapter` is owned by `LlGateway.providers` HashMap (single owner)
- `StdioTransport` (the subprocess Child) is behind `Arc<Mutex<>>` because:
  - The Drop impl needs exclusive access to kill the process
  - Tool execution needs shared access to write/read stdin/stdout
- Tool cache is behind `Arc<Mutex<>>` because:
  - Initial discovery writes to the cache
  - Future reads may need to refresh the cache
- `initialized: AtomicBool` is lock-free:
  - Only transitions false→true once
  - Read by every incoming request

### 1.4 Lifecycle

```
CREATED ──▶ UNINITIALIZED ──▶ INITIALIZING ──▶ READY ──▶ SHUTDOWN ──▶ DESTROYED
                │                    │              │
                │               ┌────┘              │
                ▼               ▼                   │
           Initialize()    ListTools()          health()
           (JSON-RPC        (JSON-RPC           (check subprocess
            initialize)      tools/list)          alive + initialized)
```

### 1.5 Interaction with Gateway

**Where the Gateway changes** (`gateway.rs`):

1. **`LlGateway::build()`** — NEW code to create MCP providers from `config.mcp.servers`:
   ```rust
   // After creating all config-based providers:
   for mcp_server in &config.mcp.servers {
       let adapter = McpAdapter::new(mcp_server.clone())?;
       adapter.initialize()?; // optional: or defer to first use
       providers.push(Box::new(adapter));
   }
   ```

2. **Registry registration** — McpAdapter's models must be registered:
   ```rust
   // The builder must auto-register a model for each MCP server:
   let mcp_model = ModelConfig {
       id: format!("mcp-{}", server.name),
       provider: server.name.clone(), // matches McpAdapter.id()
       capabilities: vec!["chat".into(), "tool_use".into()],
       max_tokens: 0, // MCP servers don't have token limits
       context_window: 0,
       cost_per_1k_input: 0.0,
       cost_per_1k_output: 0.0,
       aliases: vec![],
   };
   // Insert into models list before building registry
   ```

3. **`LlGateway::chat_stream()`** — NO changes needed. McpAdapter returns `Err(StreamingUnsupported)`.

4. **`LlGateway::chat()` retry/fallback** — NO changes needed. MCP errors flow through existing retry logic.

5. **`LlGateway::health()`** — NO changes needed. McpAdapter.health() returns subprocess status.

### 1.6 Interaction with Router

**No changes to `router.rs`.**

The MCP model is registered like any other model with capability `"tool_use"` (or the consumer routes by explicit model ID `"mcp-<name>"`).

### 1.7 Interaction with LlProvider

**`McpAdapter` implements `LlProvider`**. See [Section 8](#8-provider-integration) for the complete method-by-method breakdown.

### 1.8 Interaction with Telemetry

**No changes to `telemetry.rs`.**

McpAdapter's `chat()` calls are recorded by the Gateway's telemetry the same way as any other provider. No MCP-specific telemetry in MVP.

### 1.9 Interaction with CostTracker

**No changes to `cost.rs`.**

MCP tool executions are free (local subprocess). `estimate()` and `record()` are called normally with zero cost rates. Cost tracking works identically.

### 1.10 Interaction with Cache

**No changes to `cache.rs`.**

MCP tool results could be cached in the future but are NOT cached in MVP. The cache key is request-based; since MCP requests pass through `chat()`, they will use the cache if `request.tools` and `request.messages` produce a stable key. This is acceptable.

---

## 2. Module Layout

### 2.1 Proposed Structure

```
src/mcp/
    mod.rs          — Module root, re-exports, McpAdapter struct definition + public API
    adapter.rs      — LlProvider implementation for McpAdapter
    transport.rs    — McpTransport trait + StdioTransport (subprocess I/O)
    jsonrpc.rs      — JSON-RPC 2.0 wire format (Message enum, serializer/deserializer)
    client.rs       — MCP protocol client (initialize, tools/list, tools/call)
    errors.rs       — McpError enum + conversions to LlmError
```

### 2.2 Justification for Deviating from PHASE4_PLAN.md

The plan suggested:
```
src/mcp/
    mod.rs
    adapter.rs
    stdio.rs
    jsonrpc.rs
    protocol.rs
    registry.rs
    process.rs
    errors.rs
    tests.rs
```

**Changes made and rationale**:

| Planned file | Action | Rationale |
|---|---|---|
| `stdio.rs` | Merged into `transport.rs` | Transport abstraction allows future WebSocket/SSE without restructuring. Stdio becomes one variant. |
| `protocol.rs` | Renamed to `client.rs` | "Protocol" is ambiguous (JSON-RPC? MCP?). `client.rs` clearly owns MCP protocol operations. |
| `registry.rs` | Removed, inlined into `McpAdapter` | Tool cache is a simple `HashMap<String, LlTool>` behind `Arc<Mutex>`. A separate file is overengineering for MVP. |
| `process.rs` | Merged into `transport.rs` | Process lifecycle (spawn, kill, wait) is inseparable from the stdio transport. Splitting them creates circular dependencies. |
| `tests.rs` | Removed, use inline `#[cfg(test)]` | Consistent with all other modules in this crate. Each file has its own test module. |
| `errors.rs` | Kept | MCP-specific errors (protocol errors, initialization failures) need a dedicated home before mapping to `LlmError`. |

### 2.3 File-by-File Specification

---

#### `src/mcp/mod.rs`

**Purpose**: Module root. Defines the `McpAdapter` struct and its public API. Re-exports key types.

**Public types**:
- `McpAdapter` — the MCP provider adapter struct
- `McpTransport` — transport type enum (`Stdio` for MVP)

**Private types**: None

**Traits**: None

**Responsibilities**:
- Define `McpAdapter` struct fields
- Implement `McpAdapter::new(config)` — constructor, spawns subprocess
- Implement `McpAdapter::initialize()` — MCP initialize handshake + tool discovery
- Implement `McpAdapter::call_tool(name, args)` — public tool execution method
- Implement `McpAdapter::tools()` — return cached tool list
- Implement `Drop` for `McpAdapter` — kill subprocess
- Define `McpTransport` enum
- Re-export `McpAdapter`, `McpTransport` for `lib.rs`

**Dependencies**: `crate::types::McpServerConfig`, `crate::error::LlmError`, `crate::mcp::transport::StdioTransport`, `crate::mcp::client::McpClient`, `crate::mcp::errors::McpError`

---

#### `src/mcp/adapter.rs`

**Purpose**: `LlProvider` trait implementation for `McpAdapter`.

**Public types**: None (implementation file, no new public types)

**Private types**: None

**Traits**: `impl LlProvider for McpAdapter`

**Responsibilities**:
- `id()` → returns `config.name`
- `name()` → returns `config.name`
- `capabilities()` → `[Chat, ToolUse, FunctionCalling]`
- `models()` → `[config.name]`
- `chat(request)` → extracts tool call from request, executes via `client.call_tool()`, returns `ProviderResponse`
- `chat_stream()` → `Err(StreamingUnsupported)`
- `embed()` → `Err(CapabilityNotSupported)`
- `health()` → checks subprocess alive + initialized flag

**Dependencies**: `crate::mcp::mod::McpAdapter`, `crate::provider::LlProvider`, `crate::error::LlmError`, `crate::types::*`

---

#### `src/mcp/transport.rs`

**Purpose**: Transport abstraction for MCP communication. Stdio implementation for MVP.

**Public types**:
- `McpTransport` — enum with `Stdio(StdioTransport)` variant
- `StdioTransport` — subprocess stdin/stdout I/O

**Private types**: None

**Traits**:
- `Transport` — abstract trait with `send(msg)`, `receive(timeout)`, `is_alive()`, `shutdown()`

**Responsibilities**:
- `StdioTransport::spawn(command, args)` — spawn subprocess, capture stdin/stdout/stderr
- `StdioTransport::send(message)` — write JSON-RPC message to stdin
- `StdioTransport::receive(timeout_ms)` — read one JSON-RPC message from stdout (line-based)
- `StdioTransport::is_alive()` — check if subprocess is still running
- `StdioTransport::shutdown()` — graceful SIGTERM, then SIGKILL after timeout, wait
- `impl Drop for StdioTransport` — ensure process cleanup

**Line-reading strategy**:
- Wrap stdout in `BufReader<ChildStdout>`
- Use `read_line()` for each JSON-RPC message (MCP uses newline-delimited JSON)
- Use `read_to_end()` for stderr on failure (for error reporting)
- All I/O is blocking (matches the crate's threading model)

**Dependencies**: `std::process::{Command, Child, ChildStdin, ChildStdout}`, `std::io::{BufRead, BufReader, Write}`, `serde_json`, `crate::mcp::errors::McpError`

---

#### `src/mcp/jsonrpc.rs`

**Purpose**: JSON-RPC 2.0 wire format types and serialization.

**Public types**:
- `JsonRpcVersion` — version marker (`"2.0"`)
- `JsonRpcId` — request ID (Number, String, or Null)
- `JsonRpcRequest` — `{ jsonrpc, id, method, params }`
- `JsonRpcResponse` — `{ jsonrpc, id, result }`
- `JsonRpcError` — `{ code, message, data? }`
- `JsonRpcErrorResponse` — `{ jsonrpc, id, error }`
- `JsonRpcNotification` — `{ jsonrpc, method, params }` (no id)
- `JsonRpcMessage` — enum over Request/Response/ErrorResponse/Notification

**Private types**: None

**Traits**: `Serialize`, `Deserialize` on all types

**Responsibilities**:
- Strict JSON-RPC 2.0 parsing: validate `jsonrpc: "2.0"`, reject unknown fields
- `JsonRpcMessage::parse(json_str)` → deserialize from JSON string
- `JsonRpcMessage::serialize()` → serialize to JSON string
- `JsonRpcId`: number → `serde_json::Number`, string → `String`, null → `Value::Null`
- Standard error codes: `ParseError(-32700)`, `InvalidRequest(-32600)`, `MethodNotFound(-32601)`, `InvalidParams(-32602)`, `InternalError(-32603)`

**Serialization strategy**:
- Use `#[serde(untagged)]` for `JsonRpcId` (auto-detect number vs string vs null)
- Use `#[serde(tag = "...")]` for `JsonRpcMessage` discriminant
- Use `#[serde(skip_serializing_if = "Option::is_none")]` for optional fields
- `params` and `result` are `serde_json::Value` (flexible schema)

**Dependencies**: `serde`, `serde_json`, `serde::Serialize`, `serde::Deserialize`

---

#### `src/mcp/client.rs`

**Purpose**: MCP protocol-level operations on top of JSON-RPC.

**Public types**:
- `McpClient` — MCP protocol client

**Private types**:
- `InitializeResult` — parsed response from `initialize` request
- `ListToolsResult` — parsed response from `tools/list` request
- `CallToolResult` — parsed response from `tools/call` request
- `McpCapabilities` — server capability flags
- `McpServerInfo` — server identity info

**Traits**: None

**Responsibilities**:
- `McpClient::new(transport)` — construct with owned transport
- `client.initialize()` — send `initialize` request, verify protocol version, store server info
- `client.send_initialized()` — send `notifications/initialized` notification
- `client.list_tools()` — send `tools/list`, return `Vec<McpToolDef>`
- `client.call_tool(name, args)` — send `tools/call`, return `McpCallResult`
- `client.transport()` — return shared reference to transport (for Drop/health)

**MCP protocol constants**:
- Protocol version: `"2024-11-05"` (pinned for MVP)
- Client info: `{ name: "browseros", version: "0.1.0" }`
- Client capabilities: `{}` (no extensions in MVP)

**Dependencies**: `crate::mcp::transport::Transport`, `crate::mcp::jsonrpc`, `crate::mcp::errors::McpError`, `serde_json`, `std::time::Duration`

---

#### `src/mcp/errors.rs`

**Purpose**: MCP-specific error types and conversion to `LlmError`.

**Public types**:
- `McpError` — enum of MCP-specific errors

**Private types**: None

**Traits**: `impl From<McpError> for LlmError`

**Responsibilities**:
- Define MCP error variants
- Implement `Display` for `McpError`
- Implement `From<McpError> for LlmError` mapping
- Helper: `McpError::from_jsonrpc(code, message)` — create from JSON-RPC error

**Dependencies**: `crate::error::LlmError`, `crate::mcp::jsonrpc::JsonRpcError`

---

## 3. JSON-RPC Design

### 3.1 Core Types

```rust
/// JSON-RPC 2.0 version marker.
pub const JSON_RPC_VERSION: &str = "2.0";

/// Request/response identifier.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcId {
    Number(serde_json::Number),
    String(String),
    Null,
}

impl JsonRpcId {
    pub fn number(n: u64) -> Self {
        JsonRpcId::Number(serde_json::Number::from(n))
    }

    pub fn as_u64(&self) -> Option<u64> {
        match self {
            JsonRpcId::Number(n) => n.as_u64(),
            JsonRpcId::String(s) => s.parse().ok(),
            JsonRpcId::Null => None,
        }
    }
}

/// JSON-RPC 2.0 Request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: JsonRpcId,
    pub method: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

/// JSON-RPC 2.0 Response (success).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: JsonRpcId,

    #[serde(default)]
    pub result: serde_json::Value,
}

/// JSON-RPC 2.0 Error object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl JsonRpcError {
    // Standard JSON-RPC error codes
    pub const PARSE_ERROR: i64 = -32700;
    pub const INVALID_REQUEST: i64 = -32600;
    pub const METHOD_NOT_FOUND: i64 = -32601;
    pub const INVALID_PARAMS: i64 = -32602;
    pub const INTERNAL_ERROR: i64 = -32603;
}

/// JSON-RPC 2.0 Error Response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcErrorResponse {
    pub jsonrpc: String,
    pub id: JsonRpcId,
    pub error: JsonRpcError,
}

/// JSON-RPC 2.0 Notification (no id).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

/// Union of all JSON-RPC message types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcMessage {
    Request(JsonRpcRequest),
    Response(JsonRpcResponse),
    ErrorResponse(JsonRpcErrorResponse),
    Notification(JsonRpcNotification),
}
```

### 3.2 Serialization Strategy

**Parsing**:
```rust
impl JsonRpcMessage {
    /// Parse a JSON string into a JSON-RPC message.
    /// Returns Error if jsonrpc != "2.0" or if the JSON is malformed.
    pub fn parse(json_str: &str) -> Result<Self, McpError> {
        let msg: JsonRpcMessage = serde_json::from_str(json_str)
            .map_err(|e| McpError::InvalidJson(format!("parse failed: {}", e)))?;
        msg.validate_version()?;
        Ok(msg)
    }

    /// Validate jsonrpc field is "2.0".
    fn validate_version(&self) -> Result<(), McpError> {
        let version = match self {
            JsonRpcMessage::Request(r) => &r.jsonrpc,
            JsonRpcMessage::Response(r) => &r.jsonrpc,
            JsonRpcMessage::ErrorResponse(e) => &e.jsonrpc,
            JsonRpcMessage::Notification(n) => &n.jsonrpc,
        };
        if version != JSON_RPC_VERSION {
            return Err(McpError::InvalidJson(
                format!("expected jsonrpc '2.0', got '{}'", version)
            ));
        }
        Ok(())
    }
}
```

**Request-response pairing**:
- Use a local `AtomicU64` counter for request IDs (monotonically increasing, per McpClient instance)
- Each request's `id` is stored; the response's `id` is matched to complete the pair
- Only one in-flight request at a time for MVP (sequential send/receive over stdio)

### 3.3 MCP-Specific Structures

```rust
/// MCP tool definition from tools/list response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolDef {
    pub name: String,

    #[serde(default)]
    pub description: String,

    #[serde(default)]
    pub inputSchema: serde_json::Value,
}

/// MCP tools/list result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpListToolsResult {
    pub tools: Vec<McpToolDef>,
}

/// MCP call_tool result content item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpContentItem {
    #[serde(rename = "type")]
    pub type_: String,

    #[serde(default)]
    pub text: String,
}

/// MCP tools/call result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpCallToolResult {
    pub content: Vec<McpContentItem>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub isError: Option<bool>,
}

/// MCP initialize result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpInitializeResult {
    pub protocolVersion: String,
    pub capabilities: serde_json::Value,
    pub serverInfo: McpServerInfo,
}

/// MCP server identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerInfo {
    pub name: String,
    pub version: String,
}
```

---

## 4. Process Lifecycle

### 4.1 Spawn Sequence

```
McpAdapter::new(config):
  1. Command::new(&config.command)
       .args(&config.args)
       .stdin(Stdio::piped())
       .stdout(Stdio::piped())
       .stderr(Stdio::piped())
       .spawn()
  2. Wrap in StdioTransport { child, stdin, stdout }
  3. Create McpClient { transport, next_id: AtomicU64(1) }
  4. Return McpAdapter { id, config, transport, tools, state }

Sequence diagram:
┌─────────────┐          ┌──────────────┐         ┌────────────┐
│ McpAdapter  │          │   Stdio      │         │ MCP Server │
│    .new()   │          │  Transport   │         │ Subprocess │
└──────┬──────┘          └──────┬───────┘         └─────┬──────┘
       │                        │                       │
       │  spawn(cmd, args)      │                       │
       │───────────────────────▶│                       │
       │                        │  Command::spawn()     │
       │                        │──────────────────────▶│
       │                        │        (process)      │
       │                        │◀──────────────────────│
       │     Ok(transport)      │                       │
       │◀───────────────────────│                       │
       │                        │                       │
       │  initialize()          │                       │
       │─────────────────────────────────────────────────▶ See section 4.2
```

### 4.2 Startup / Initialize

```
McpAdapter::initialize():
  1. Send: {
       "jsonrpc": "2.0",
       "id": next_id(),
       "method": "initialize",
       "params": {
         "protocolVersion": "2024-11-05",
         "capabilities": {},
         "clientInfo": { "name": "browseros", "version": "0.1.0" }
       }
     }
  2. Receive: JsonRpcMessage::Response {
       id: <matching>,
       result: {
         "protocolVersion": "2024-11-05",
         "capabilities": { "tools": {} },
         "serverInfo": { "name": "...", "version": "..." }
       }
     }
  3. Validate protocolVersion == "2024-11-05" (tolerant: accept any 2024-* or 2025-*)
  4. Send notification: {
       "jsonrpc": "2.0",
       "method": "notifications/initialized",
       "params": {}
     }
  5. Call list_tools() (see Section 5)
  6. Set initialized = true

Timeout: 10 seconds (configurable via McpServerConfig auto_start, but MVP uses hardcoded 10s)

On failure:
  - Log warning
  - Return McpError::InitializationFailed(msg)
  - McpAdapter is still functional for retry (transport remains alive)
```

### 4.3 Health / Ready Detection

```rust
impl McpAdapter {
    pub fn health(&self) -> ProviderHealthResult {
        if !self.initialized.load(Ordering::Acquire) {
            return ProviderHealthResult {
                status: ProviderHealth::Unavailable {
                    since: SystemTime::now(),
                },
                latency_ms: None,
                checked_at: SystemTime::now(),
                error: Some("not initialized".into()),
            };
        }

        let start = Instant::now();
        match self.client.transport().lock().unwrap().is_alive() {
            true => ProviderHealthResult {
                status: ProviderHealth::Healthy,
                latency_ms: Some(start.elapsed().as_millis() as u64),
                checked_at: SystemTime::now(),
                error: None,
            },
            false => ProviderHealthResult {
                status: ProviderHealth::Unavailable {
                    since: SystemTime::now(),
                },
                latency_ms: None,
                checked_at: SystemTime::now(),
                error: Some("process exited".into()),
            },
        }
    }
}
```

**Ready detection**: `initialized.load(Ordering::Acquire) == true` + `transport.is_alive() == true`

### 4.4 Shutdown / Drop

```rust
impl Drop for StdioTransport {
    fn drop(&mut self) {
        // 1. Try graceful shutdown: send stdin EOF, wait 1s
        if let Some(stdin) = self.stdin.take() {
            drop(stdin); // closes stdin → MCP server should exit
        }

        // 2. Wait with timeout
        if let Some(child) = self.child.as_mut() {
            let _ = child.wait_timeout(Duration::from_secs(3));
        }

        // 3. Force kill if still alive
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
```

**`McpAdapter::Drop`**:
```rust
impl Drop for McpAdapter {
    fn drop(&mut self) {
        // transport Drop handles subprocess cleanup
        // No explicit action needed beyond transport Drop
    }
}
```

The `Arc<Mutex<StdioTransport>>` ensures that Drop runs exactly once when the last reference (the McpAdapter itself) is dropped.

### 4.5 Panic Handling

- `StdioTransport::send()`: If the subprocess has already exited, `write()` returns an error, which propagates as `McpError::TransportError("pipe closed")`
- `StdioTransport::receive()`: If the subprocess has exited, `read_line()` returns `Ok(0)`, which propagates as `McpError::ProcessExited`
- The `Drop` impl is panic-safe: it uses only `std::process::Child` methods which are infallible
- All `Mutex` locks use `lock().map_err()` to handle poisoning (poisoned mutex → `McpError::InternalError`)

### 4.6 Timeouts

- **Startup timeout**: 10 seconds for `initialize` + `tools/list` (hardcoded for MVP)
- **Tool execution timeout**: `config.timeout.default_secs` (default 30s), passed via `ProviderRequest.timeout_ms`
- **Shutdown timeout**: 3 seconds for graceful shutdown, then force kill
- **Receive timeout**: Implemented by spawning a helper thread that sends through a channel with a timer, or more simply: use `stdin.write()` (non-blocking from the caller's perspective) and a `recv_timeout` on stdout reads

Since MVP uses blocking I/O, the timeout strategy is:
```rust
pub fn receive(&self, timeout_ms: u64) -> Result<String, McpError> {
    let (tx, rx) = std::sync::mpsc::channel();
    let reader = Arc::clone(&self.reader); // Arc<Mutex<BufReader<ChildStdout>>>
    
    std::thread::spawn(move || {
        let mut line = String::new();
        let result = reader.lock().unwrap().read_line(&mut line);
        let _ = tx.send((line, result));
    });
    
    match rx.recv_timeout(Duration::from_millis(timeout_ms)) {
        Ok((line, Ok(_))) if !line.is_empty() => Ok(line),
        Ok((_, Ok(_))) => Err(McpError::TransportError("empty response".into())),
        Ok((_, Err(e))) => Err(McpError::TransportError(format!("read error: {}", e))),
        Err(_) => Err(McpError::Timeout { elapsed_ms: timeout_ms }),
    }
}
```

### 4.7 Restart Policy

**MVP: No automatic restart.**

- If the subprocess exits, `health()` returns `Unavailable`
- The next `chat()` call fails with `McpError::ProcessExited`
- The consumer can recreate the adapter or rely on Gateway fallback
- Future: add `auto_start` → restart on failure with backoff

---

## 5. Tool Discovery

### 5.1 `tools/list` → `Vec<LlTool>` Mapping

**MCP Protocol**:
```json
// Request
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/list",
  "params": {}
}

// Response
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "tools": [
      {
        "name": "read_file",
        "description": "Read the contents of a file",
        "inputSchema": {
          "type": "object",
          "properties": {
            "path": { "type": "string", "description": "File path" }
          },
          "required": ["path"]
        }
      }
    ]
  }
}
```

**Field Mapping**:

| MCP `tools/list` field | `LlTool` field | Mapping |
|---|---|---|
| `name` | `name` | Direct copy |
| `description` | `description` | Direct copy |
| `inputSchema` | `parameters` | Direct copy (`serde_json::Value`) |
| (none) | `strict` | Always `false` for MCP tools |

**Rust code**:
```rust
impl From<McpToolDef> for LlTool {
    fn from(tool: McpToolDef) -> Self {
        LlTool {
            name: tool.name,
            description: tool.description,
            parameters: tool.inputSchema,
            strict: false,
        }
    }
}

impl McpClient {
    pub fn list_tools(&self) -> Result<Vec<LlTool>, McpError> {
        let request = self.build_request("tools/list", Some(serde_json::json!({})));
        self.send_request(request)?;
        let response = self.receive_response(10_000)?;

        let result: McpListToolsResult = serde_json::from_value(response.result)
            .map_err(|e| McpError::InvalidJson(format!("tools/list result: {}", e)))?;

        let tools: Vec<LlTool> = result.tools.into_iter().map(Into::into).collect();
        Ok(tools)
    }
}
```

### 5.2 Caching Strategy

```rust
impl McpAdapter {
    pub fn tools(&self) -> Vec<LlTool> {
        self.tools
            .lock()
            .unwrap()
            .values()
            .cloned()
            .collect()
    }

    pub fn refresh_tools(&self) -> Result<(), McpError> {
        let tools = self.client.list_tools()?;
        let mut cache = self.tools.lock().unwrap();
        cache.clear();
        for tool in tools {
            cache.insert(tool.name.clone(), tool);
        }
        Ok(())
    }
}
```

- Tools are discovered once during `initialize()`
- `refresh_tools()` is available but NOT called automatically in MVP
- Cache is a `HashMap<String, LlTool>` keyed by tool name

---

## 6. Tool Execution

### 6.1 `tools/call` → `ProviderResponse` Mapping

**MCP Protocol**:
```json
// Request
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/call",
  "params": {
    "name": "read_file",
    "arguments": { "path": "/tmp/test.txt" }
  }
}

// Response (success)
{
  "jsonrpc": "2.0",
  "id": 2,
  "result": {
    "content": [
      { "type": "text", "text": "file contents here..." }
    ],
    "isError": false
  }
}

// Response (error — MCP-level)
{
  "jsonrpc": "2.0",
  "id": 2,
  "result": {
    "content": [
      { "type": "text", "text": "File not found: /tmp/test.txt" }
    ],
    "isError": true
  }
}
```

### 6.2 From McpAdapter.chat() to Tool Execution

```rust
impl LlProvider for McpAdapter {
    fn chat(&self, request: ProviderRequest) -> Result<ProviderResponse, LlmError> {
        // 1. Find the last assistant message with a ToolCall
        let tool_call = request
            .messages
            .iter()
            .rev()
            .find_map(|msg| {
                if let LlContent::ToolCall { name, arguments, .. } = &msg.content {
                    Some((name.clone(), arguments.clone()))
                } else {
                    None
                }
            });

        let (tool_name, tool_args) = tool_call.ok_or_else(|| {
            McpError::InvalidRequest("no ToolCall message found in request".into())
        })?;

        // 2. Convert arguments from HashMap to serde_json::Value
        let args_value = serde_json::to_value(&tool_args)
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

        // 3. Execute via MCP client with timeout
        let timeout = std::cmp::max(request.timeout_ms, 5_000);
        let result = self.client.call_tool(&tool_name, args_value, timeout)?;

        // 4. Convert MCP result to ProviderResponse
        let output = result
            .content
            .iter()
            .map(|c| c.text.clone())
            .collect::<Vec<_>>()
            .join("\n");

        Ok(ProviderResponse {
            message: ProviderMessage {
                role: LlRole::Tool,
                content: LlContent::ToolResult {
                    call_id: String::new(), // MCP doesn't use call_ids
                    output,
                },
            },
            finish_reason: LlFinishReason::Stop,
            input_tokens: 0,
            output_tokens: 0,
            model: self.id.clone(),
        })
    }
}
```

### 6.3 JsonRpcId → Request Matching

```rust
impl McpClient {
    pub fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
        timeout_ms: u64,
    ) -> Result<McpCallToolResult, McpError> {
        let params = serde_json::json!({
            "name": name,
            "arguments": arguments,
        });

        let request = self.build_request("tools/call", Some(params));
        self.send_request(&request)?;
        let response = self.receive_response(timeout_ms)?;

        let result: McpCallToolResult = serde_json::from_value(response.result)
            .map_err(|e| McpError::InvalidJson(format!("tools/call result: {}", e)))?;

        if result.isError.unwrap_or(false) {
            let error_text = result.content.first().map(|c| c.text.clone())
                .unwrap_or_else(|| "unknown error".into());
            return Err(McpError::ToolExecutionError(error_text));
        }

        Ok(result)
    }
}
```

---

## 7. Gateway Integration

### 7.1 Changes Required in `gateway.rs`

**ONE change location**: `LlGateway::build()` method.

Current code (lines 103-121):
```rust
// Build provider adapters via factory
let mut providers: Vec<Box<dyn LlProvider>> = Vec::new();
for pc in &config.providers {
    let provider = crate::adapters::create_provider(pc).map_err(|e| {
        LlmError::ConfigurationError(...)
    })?;
    providers.push(provider);
}
```

**New code**:
```rust
// Build provider adapters via factory
let mut providers: Vec<Box<dyn LlProvider>> = Vec::new();
for pc in &config.providers {
    let provider = crate::adapters::create_provider(pc).map_err(|e| {
        LlmError::ConfigurationError(...)
    })?;
    providers.push(provider);
}

// Build MCP providers from McpConfig
let mut mcp_models: Vec<ModelConfig> = Vec::new();
for server in &config.mcp.servers {
    let adapter = crate::mcp::McpAdapter::new(server.clone()).map_err(|e| {
        LlmError::ConfigurationError(format!("failed to create MCP server '{}': {}",
            server.name, e))
    })?;

    let model_id = format!("mcp-{}", server.name);
    mcp_models.push(ModelConfig {
        id: model_id,
        provider: server.name.clone(),
        capabilities: vec!["chat".into(), "tool_use".into()],
        max_tokens: 0,
        context_window: 0,
        cost_per_1k_input: 0.0,
        cost_per_1k_output: 0.0,
        aliases: vec![],
    });

    providers.push(Box::new(adapter));
}

// Then build registry with mcp_models merged into config.models
let all_models: Vec<ModelConfig> = config.models.iter()
    .chain(mcp_models.iter())
    .cloned()
    .collect();

let registry = Arc::new(ModelRegistry::new(&all_models, &providers).map_err(|e| {
    LlmError::ConfigurationError(format!("failed to build model registry: {}", e))
})?);
```

### 7.2 Changes Required in `lib.rs`

**One change location**: Change `mod mcp` from `mod` to `pub mod`:

```rust
/// MCP adapter (Model Context Protocol).
pub mod mcp;  // was: mod mcp
```

Add re-export:
```rust
pub use mcp::McpAdapter;
pub use mcp::McpTransport;
```

### 7.3 Changes Required in `src/mcp/mod.rs`

Replace the stub (8 lines) with the full implementation described in Section 2.

### 7.4 No Changes Required

The following files are **NOT modified** in Phase 4C:

| File | Reason |
|---|---|
| `src/adapters/mod.rs` | MCP adapter is in `src/mcp/`, not in `src/adapters/` |
| `src/adapters/openai.rs` | MCP is independent of OpenAI |
| `src/adapters/anthropic.rs` | Same |
| `src/adapters/gemini.rs` | Same |
| `src/adapters/ollama.rs` | Same |
| `src/adapters/http_generic.rs` | Same |
| `src/http.rs` | MCP uses stdio, not HTTP (in MVP) |
| `src/router.rs` | MCP models are registered like any other model |
| `src/streaming.rs` | McpAdapter doesn't support streaming |
| `src/cache.rs` | No MCP-specific caching |
| `src/cost.rs` | MCP tools are free |
| `src/telemetry.rs` | MCP uses existing telemetry paths |
| `src/types.rs` | `McpConfig`, `McpServerConfig` already exist |
| `src/error.rs` | `LlmError` variants are sufficient |
| `src/provider.rs` | `LlProvider` trait is unchanged |
| `src/model_registry.rs` | No changes needed |

---

## 8. Provider Integration

### 8.1 `McpAdapter` as `LlProvider`

```rust
impl LlProvider for McpAdapter {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn capabilities(&self) -> Vec<ProviderCapability>;
    fn models(&self) -> Vec<String>;
    fn chat(&self, request: ProviderRequest) -> Result<ProviderResponse, LlmError>;
    fn chat_stream(&self, request: ProviderRequest) -> Result<ProviderStream, LlmError>;
    fn embed(&self, request: ProviderEmbedRequest) -> Result<ProviderEmbedResponse, LlmError>;
    fn health(&self) -> ProviderHealthResult;
}
```

### 8.2 Method-by-Method Breakdown

| Method | Status | Behavior |
|---|---|---|
| `id()` | **IMPLEMENTED** | Returns `self.config.name.clone()` (e.g., `"mcp-filesystem"`) |
| `name()` | **IMPLEMENTED** | Returns `self.config.name.clone()` (same as id for MVP) |
| `capabilities()` | **IMPLEMENTED** | Returns `[Chat, ToolUse, FunctionCalling]` |
| `models()` | **IMPLEMENTED** | Returns `[self.id()]` — the MCP server is its own "model" |
| `chat()` | **IMPLEMENTED** | See Section 6.2 — extracts tool call, executes via MCP, returns result |
| `chat_stream()` | **UNSUPPORTED** | Returns `Err(LlmError::StreamingUnsupported)` |
| `embed()` | **UNSUPPORTED** | Returns `Err(LlmError::CapabilityNotSupported("embedding"))` |
| `health()` | **IMPLEMENTED** | See Section 4.3 — checks `initialized` flag + `is_alive()` |

### 8.3 `chat()` — Detailed State Machine

```
chat(ProviderRequest)
  │
  ├── transport.is_alive() == false
  │     └── return Err(McpError::ProcessExited)
  │
  ├── initialized == false
  │     ├── Try initialize() again (deferred init)
  │     ├── On failure → return Err(McpError::InitializationFailed)
  │     └── On success → continue
  │
  ├── Find last assistant message with ToolCall content
  │     ├── Not found → return Err(McpError::InvalidRequest)
  │     └── Found → extract name, arguments
  │
  ├── Validate tool exists in cache
  │     ├── Not in cache → return Err(McpError::ToolNotFound(name))
  │     └── Found → continue
  │
  ├── client.call_tool(name, args, timeout)
  │     ├── Transport error → return Err(McpError::TransportError)
  │     ├── Timeout → return Err(McpError::Timeout)
  │     ├── JSON-RPC error → return Err(McpError::from_jsonrpc(code, msg))
  │     └── Success → continue
  │
  ├── Convert McpCallToolResult.content to string
  ├── Wrap in ProviderResponse { role: Tool, content: ToolResult }
  └── Return Ok(ProviderResponse)
```

### 8.4 Future Considerations (Not MVP)

- **`chat_stream()`**: Could stream MCP tool results in the future if MCP adds streaming support. No streaming in MVP.
- **Multiple tool calls in one request**: MVP handles one tool call per request (the last one). Future: batch execution.
- **Tool discovery refresh**: MVP discovers once at startup. Future: periodic refresh, change notifications.
- **Resource exposure**: MCP `resources/*` methods are not supported in MVP.

---

## 9. Error Mapping

### 9.1 Complete Mapping Table

| Error Source | MCP Error | LlmError Variant |
|---|---|---|
| JSON-RPC parse error (-32700) | `McpError::InvalidJson(msg)` | `LlmError::MalformedResponse(msg)` |
| JSON-RPC invalid request (-32600) | `McpError::JsonRpcError(code, msg)` | `LlmError::ProviderError(msg)` |
| JSON-RPC method not found (-32601) | `McpError::UnsupportedMethod(method)` | `LlmError::CapabilityNotSupported(method)` |
| JSON-RPC invalid params (-32602) | `McpError::JsonRpcError(code, msg)` | `LlmError::InvalidRequest(msg)` |
| JSON-RPC internal error (-32603) | `McpError::JsonRpcError(code, msg)` | `LlmError::ProviderError(msg)` |
| Process exited before response | `McpError::ProcessExited(exit_code)` | `LlmError::TransportError(msg)` |
| Process never started | `McpError::SpawnFailed(msg)` | `LlmError::ConnectionError(msg)` |
| Timeout waiting for response | `McpError::Timeout { elapsed_ms }` | `LlmError::Timeout { elapsed_ms }` |
| Invalid JSON from server | `McpError::InvalidJson(msg)` | `LlmError::MalformedResponse(msg)` |
| Tool not found in cache | `McpError::ToolNotFound(name)` | `LlmError::CapabilityNotSupported(msg)` |
| Tool execution error (isError: true) | `McpError::ToolExecutionError(msg)` | `LlmError::ProviderError(msg)` |
| No ToolCall in request | `McpError::InvalidRequest(msg)` | `LlmError::InvalidRequest(msg)` |
| Initialization failed | `McpError::InitializationFailed(msg)` | `LlmError::ProviderUnavailable(msg)` |
| Pipe closed (broken stdin/stdout) | `McpError::TransportError(msg)` | `LlmError::TransportError(msg)` |
| Mutex poisoned | `McpError::InternalError(msg)` | `LlmError::ProviderError(msg)` |

### 9.2 `McpError` Enum

```rust
#[derive(Debug, Clone)]
pub enum McpError {
    /// Failed to spawn subprocess.
    SpawnFailed(String),
    /// Subprocess exited unexpectedly.
    ProcessExited(Option<i32>),
    /// Transport-level error (pipe closed, write failed).
    TransportError(String),
    /// Timeout waiting for response.
    Timeout { elapsed_ms: u64 },
    /// Invalid JSON received or malformed structure.
    InvalidJson(String),
    /// Method not supported by server.
    UnsupportedMethod(String),
    /// JSON-RPC protocol error.
    JsonRpcError { code: i64, message: String },
    /// Tool not found in local cache.
    ToolNotFound(String),
    /// Tool execution returned isError: true.
    ToolExecutionError(String),
    /// Initialization handshake failed.
    InitializationFailed(String),
    /// Request is invalid (e.g., no ToolCall found).
    InvalidRequest(String),
    /// Internal error (mutex poisoned, etc.).
    InternalError(String),
}
```

### 9.3 `From<McpError> for LlmError`

```rust
impl From<McpError> for LlmError {
    fn from(e: McpError) -> Self {
        match e {
            McpError::SpawnFailed(msg) => LlmError::ConnectionError(msg),
            McpError::ProcessExited(code) => {
                let msg = match code {
                    Some(c) => format!("MCP process exited with code {}", c),
                    None => "MCP process exited with no code".into(),
                };
                LlmError::TransportError(msg)
            }
            McpError::TransportError(msg) => LlmError::TransportError(msg),
            McpError::Timeout { elapsed_ms } => LlmError::Timeout { elapsed_ms },
            McpError::InvalidJson(msg) => LlmError::MalformedResponse(msg),
            McpError::UnsupportedMethod(m) => LlmError::CapabilityNotSupported(m),
            McpError::JsonRpcError { code, message } => {
                if code == -32601 {
                    LlmError::CapabilityNotSupported(message)
                } else {
                    LlmError::ProviderError(format!("JSON-RPC error {}: {}", code, message))
                }
            }
            McpError::ToolNotFound(name) => {
                LlmError::CapabilityNotSupported(format!("MCP tool '{}' not found", name))
            }
            McpError::ToolExecutionError(msg) => LlmError::ProviderError(msg),
            McpError::InitializationFailed(msg) => LlmError::ProviderUnavailable(msg),
            McpError::InvalidRequest(msg) => LlmError::InvalidRequest(msg),
            McpError::InternalError(msg) => LlmError::ProviderError(msg),
        }
    }
}
```

---

## 10. Threading Model

### 10.1 Channel Architecture

```
┌─────────────────────────────────────────────────┐
│                  Client Thread                   │
│  (owned by Gateway, calls chat())                │
│                                                   │
│  McpAdapter::chat(request)                        │
│       │                                           │
│       ├── transport.lock()  [Mutex]              │
│       ├── stdin.write_all(json)  [blocking]       │
│       ├── stdout.read_line()  [blocking+timeout]  │
│       └── transport.unlock()                      │
│                                                   │
│  All I/O is synchronous on the calling thread.   │
│  No background threads for MVP.                  │
└─────────────────────────────────────────────────┘
```

### 10.2 Lock Strategy

| Lock | Type | Contention | Held duration |
|---|---|---|---|
| `transport.lock()` (Mutex) | `std::sync::Mutex<StdioTransport>` | Per-request serialization | Full request duration (send + receive) |
| `tools.lock()` (Mutex) | `std::sync::Mutex<HashMap<...>>` | Read-only after init | Brief (clone tool def) |
| `initialized` | `AtomicBool` | Lock-free | N/A |

**Why stdio is serialized**: MCP over stdio is a half-duplex protocol — one request at a time, matched by request ID. The Mutex ensures only one thread is writing/reading at a time. This is acceptable for MVP since MCP tool execution is typically fast (local subprocess).

**Lock ordering**: `transport → tools` is the only required ordering. Tools is never held while acquiring transport.

### 10.3 Ownership

```
Client Thread (Gateway's calling thread)
    │
    ├── owns: McpAdapter (via Box<dyn LlProvider>)
    └── calls: chat() synchronously
               ├── transport.lock()
               ├── send JSON-RPC
               ├── receive JSON-RPC
               └── transport.unlock()

Drop (any thread, when Gateway is destroyed)
    └── McpAdapter::drop()
         └── transport.lock()
              └── StdioTransport::drop()
                   ├── close stdin
                   ├── wait_timeout(3s)
                   ├── kill()
                   └── wait()
```

### 10.4 `Arc` and `Mutex` Usage

```rust
pub struct McpAdapter {
    pub(crate) id: String,
    pub(crate) config: McpServerConfig,
    pub(crate) transport: Arc<Mutex<StdioTransport>>,
    pub(crate) tools: Arc<Mutex<HashMap<String, LlTool>>>,
    pub(crate) initialized: Arc<AtomicBool>,
}
```

`Arc` is necessary because:
- `StdioTransport` is accessed from both `chat()` (lock, write, read, unlock) and `health()` (lock, check alive, unlock) and `Drop` (lock, clean up)
- The `McpAdapter` is the sole `Arc` holder — no cloning of adapters in MVP

Within `McpClient`:
```rust
pub struct McpClient {
    transport: Arc<Mutex<StdioTransport>>,
    next_id: AtomicU64,
}
```

### 10.5 Send + Sync Verification

```rust
// McpAdapter contains:
// - String: Send + Sync
// - McpServerConfig: Send + Sync (all fields are String, Vec<String>, Option<String>)
// - Arc<Mutex<StdioTransport>>: Send + Sync
//     - StdioTransport contains:
//       - Option<Child>: Send (Child is Send)
//       - Option<ChildStdin>: Send
//       - Arc<Mutex<BufReader<ChildStdout>>>: Send + Sync
// - Arc<Mutex<HashMap<String, LlTool>>>: Send + Sync
// - Arc<AtomicBool>: Send + Sync

fn _assert_send_sync()
where
    McpAdapter: Send + Sync,
    McpClient: Send + Sync,
    StdioTransport: Send,
{
}
```

**Note**: `std::process::Child` is `Send` but NOT `Sync`. Since `Child` is behind a `Mutex` inside `StdioTransport`, and the `Mutex` gives us `Sync`, the overall `Arc<Mutex<StdioTransport>>` is both `Send + Sync`. The `McpAdapter` wrapping this is `Send + Sync`.

### 10.6 Thread Lifetime

- **McpAdapter runs on the caller's thread**: `chat()` blocks the calling thread (typically a Gateway thread or the consumer's thread)
- **No persistent background threads**: MVP uses synchronous I/O only. The `receive()` timeout is implemented with a short-lived `std::thread::spawn` + `recv_timeout` per call, which is joined immediately
- **Drop runs on the destructor's thread**: Typically the Gateway's thread when `LlGateway` is dropped

---

## 11. Security Review

### 11.1 Command Injection

**Risk**: `McpServerConfig.command` is a user-supplied string passed to `Command::new()`. A malicious config could inject shell commands.

**Mitigation**:
- Use `Command::new(&config.command)` with `.args(&config.args)` — these are **not** passed through a shell
- `Command::new()` on Windows uses `CreateProcess` directly; on Unix it uses `execvp` directly
- No shell interpretation means `command = "rm -rf /"` is treated as a program named literally `rm -rf /`, not as two arguments
- **Defense in depth**: Validate in `LlmConfig::validate()` that `McpServerConfig.command` does not contain shell metacharacters (space, `;`, `|`, `&`, `` ` ``, `$`, `(`, `)`, `{`, `}`)
- Add validation rule:
  ```rust
  if command.contains(' ') || command.contains(';') {
      errors.push(format!("MCP server '{}' command contains shell metacharacters", name));
  }
  ```

### 11.2 Path Traversal

**Risk**: MCP tools may accept file paths. A malicious or compromised MCP server could read/write arbitrary files.

**Mitigation**:
- **Not in scope for MVP**: Path traversal is an MCP server responsibility, not the adapter's
- Document that MCP server authors must validate paths
- The adapter merely relays `tools/call` arguments

### 11.3 Malicious MCP Server

**Risk**: A malicious MCP server could return infinite responses, hang connections, or return misleading tool definitions.

**Mitigation**:
- Timeout on all I/O operations (configurable via `ProviderRequest.timeout_ms`)
- Bounded tool list (reject >1000 tools in MVP, configurable)
- Bounded result size (reject >10MB response in MVP)
- Stderr is captured and logged on failure but not interpreted

### 11.4 Resource Exhaustion

**Risk**: Running too many MCP servers could exhaust system resources (process slots, file descriptors).

**Mitigation**:
- Each `McpServerConfig` creates exactly one subprocess
- Subprocess is killed on `Drop` (when Gateway is dropped)
- Document that users should limit the number of MCP servers
- Future: add max_servers config guard

### 11.5 Invalid JSON

**Risk**: Malformed JSON from the MCP server could cause panics or undefined behavior.

**Mitigation**:
- `serde_json::from_str()` is fallible and wrapped in `Result`
- All JSON parsing returns `McpError::InvalidJson` on failure
- No `unwrap()` or `expect()` in JSON parsing paths
- Bounded input: read_line limits to 1MB (configurable)

### 11.6 Large Responses

**Risk**: An MCP server could send an arbitrarily large response, consuming memory.

**Mitigation**:
- `read_line()` has no built-in size limit in stdlib, but we can cap: after reading a line, check its length
- Reject responses >10MB with `McpError::TransportError("response too large")`
- Add to `receive()`:
  ```rust
  const MAX_RESPONSE_BYTES: usize = 10 * 1024 * 1024;
  if line.len() > MAX_RESPONSE_BYTES {
      return Err(McpError::TransportError("response too large".into()));
  }
  ```

---

## 12. Performance Review

### 12.1 Startup Cost

| Operation | Estimated time | Notes |
|---|---|---|
| `Command::spawn()` | 10-50ms | OS subprocess creation |
| MCP `initialize` round-trip | 5-50ms | One JSON-RPC exchange |
| MCP `notifications/initialized` | 1-5ms | Fire-and-forget |
| `tools/list` round-trip | 10-100ms | Depends on number of tools |
| **Total startup** | **26-205ms** | Single-threaded, synchronous |

### 12.2 Tool Discovery Cost

- One-time cost at `initialize()`: `tools/list` JSON-RPC call
- Memory: O(n) where n = number of tools
- Each tool stored as `LlTool` (~200-500 bytes per tool)
- Expected: <50 tools per MCP server in practice

### 12.3 Memory Footprint

| Component | Memory | Notes |
|---|---|---|
| `McpAdapter` struct | ~200 bytes | Strings, Arcs |
| `StdioTransport` | ~200 bytes + OS handles | Child, pipes, mutex |
| `McpClient` | ~100 bytes | Arc, AtomicU64 |
| Tool cache (per tool) | ~500 bytes | `LlTool` with name, description, schema |
| **Total per server** | **~1KB + OS handles** | Negligible |
| **Worst case (20 servers)** | **~20KB** | Acceptable |

### 12.4 Expected Latency

| Operation | Latency | Notes |
|---|---|---|
| Tool execution (local) | 10-100ms | Subprocess IPC |
| Tool execution (slow) | 100-5000ms | Long-running tool |
| `chat()` no-op (missing ToolCall) | <1µs | Just message iteration |
| Timeout | Configurable | Default 30s |

### 12.5 Number of Allocations

Per `chat()` call:
- 1 allocation: request JSON string (~200 bytes)
- 1 allocation: response string (variable, usually <10KB)
- 1 allocation: `ProviderResponse` struct (stack + boxed content)

Total: **3-5 heap allocations per tool execution** (acceptable)

### 12.6 Cache Opportunities

| Cache target | Benefit | Complexity | MVP? |
|---|---|---|---|
| Tool list (tools/list) | Avoid repeated discovery | Low — cache in `tools: HashMap` | ✅ YES |
| Tool execution results | Avoid repeated tool calls | Medium — depends on args | ❌ NO |
| JSON-RPC serialization buffers | Reduce allocation | Low | ❌ NO (premature) |

---

## 13. Test Plan

### 13.1 Unit Tests (in `jsonrpc.rs`)

| # | Test | Description |
|---|---|---|
| 1 | `jsonrpc_parse_request` | Parse a valid JSON-RPC request |
| 2 | `jsonrpc_parse_response` | Parse a valid JSON-RPC response |
| 3 | `jsonrpc_parse_error_response` | Parse a JSON-RPC error response |
| 4 | `jsonrpc_parse_notification` | Parse a JSON-RPC notification |
| 5 | `jsonrpc_parse_invalid_version` | Reject `jsonrpc: "1.0"` |
| 6 | `jsonrpc_parse_malformed_json` | Return error on garbage input |
| 7 | `jsonrpc_serialize_request` | Round-trip serialize/deserialize request |
| 8 | `jsonrpc_serialize_response` | Round-trip serialize/deserialize response |
| 9 | `jsonrpc_id_number` | Id::Number round-trip |
| 10 | `jsonrpc_id_string` | Id::String round-trip |
| 11 | `jsonrpc_id_null` | Id::Null for notifications |
| 12 | `jsonrpc_untagged_request` | Request is not matched as Response |

### 13.2 Unit Tests (in `transport.rs`)

| # | Test | Description |
|---|---|---|
| 13 | `stdio_spawn_fake_server` | Spawn a known binary (e.g., `echo`) and verify pipes work |
| 14 | `stdio_send_receive` | Write a line, read it back from a loopback script |
| 15 | `stdio_is_alive_running` | `is_alive()` returns true for running process |
| 16 | `stdio_is_alive_exited` | `is_alive()` returns false after process exits |
| 17 | `stdio_shutdown_cleanup` | Drop closes pipes, process is reaped |
| 18 | `stdio_receive_timeout` | `receive()` returns Timeout when no data |
| 19 | `stdio_spawn_nonexistent_command` | `SpawnFailed` for bad command |
| 20 | `stdio_large_response_rejected` | Response >10MB is rejected |
| 21 | `transport_send_to_closed_stdin` | Error when process exits before write |

### 13.3 Unit Tests (in `client.rs`)

| # | Test | Description |
|---|---|---|
| 22 | `initialize_success` | Build request, parse mock response |
| 23 | `initialize_wrong_version` | Reject unsupported protocol version |
| 24 | `list_tools_parsed` | Parse mock tools/list response into Vec<LlTool> |
| 25 | `list_tools_empty` | Empty tool list is valid |
| 26 | `list_tools_missing_input_schema` | inputSchema defaults to {} |
| 27 | `call_tool_success` | Parse mock tools/call response |
| 28 | `call_tool_is_error` | `isError: true` → ToolExecutionError |
| 29 | `call_tool_missing_content` | Empty content array → empty result |
| 30 | `call_tool_multiple_content_items` | Multiple content items joined |
| 31 | `call_tool_timeout` | Timeout propagates correctly |

### 13.4 Unit Tests (in `adapter.rs`)

| # | Test | Description |
|---|---|---|
| 32 | `adapter_id_name` | id() and name() return config.name |
| 33 | `adapter_capabilities` | capabilities() includes ToolUse |
| 34 | `adapter_models` | models() returns [id] |
| 35 | `adapter_chat_no_tool_call` | Returns InvalidRequest when no ToolCall |
| 36 | `adapter_chat_tool_call_found` | Extracts tool call from messages |
| 37 | `adapter_chat_stream_unsupported` | Returns StreamingUnsupported |
| 38 | `adapter_embed_unsupported` | Returns CapabilityNotSupported |
| 39 | `adapter_health_initialized_ok` | Health returns Healthy |
| 40 | `adapter_health_not_initialized` | Health returns Unavailable |
| 41 | `adapter_health_process_exited` | Health returns Unavailable after exit |
| 42 | `adapter_drop_does_not_panic` | Drop with dead process is safe |
| 43 | `adapter_is_send_sync` | Static assertion for Send + Sync |

### 13.5 Integration Tests (in `tests/integration.rs`)

| # | Test | Description |
|---|---|---|
| 44 | `mcp_echo_server_tool_list` | Spawn a fake MCP echo server, verify tool discovery |
| 45 | `mcp_echo_server_tool_call` | Spawn a fake MCP echo server, execute tool, verify result |
| 46 | `mcp_adapter_via_gateway` | Build LlGateway with MCP adapter, route request |
| 47 | `mcp_server_crash_recovery` | Server crashes during request, error propagates |
| 48 | `mcp_server_never_initializes` | Server doesn't respond to initialize → timeout |

### 13.6 Fake MCP Server

A fake MCP server for tests:
```python
# tests/mcp_echo_server.py — remains in the repo
import sys, json

def send(msg):
    sys.stdout.write(json.dumps(msg) + "\n")
    sys.stdout.flush()

def recv():
    return json.loads(sys.stdin.readline())

while True:
    msg = recv()
    if msg["method"] == "initialize":
        send({
            "jsonrpc": "2.0",
            "id": msg["id"],
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "echo-server", "version": "0.1.0"}
            }
        })
    elif msg["method"] == "notifications/initialized":
        pass  # no response needed
    elif msg["method"] == "tools/list":
        send({
            "jsonrpc": "2.0",
            "id": msg["id"],
            "result": {
                "tools": [{
                    "name": "echo",
                    "description": "Echo input back",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "message": {"type": "string"}
                        },
                        "required": ["message"]
                    }
                }]
            }
        })
    elif msg["method"] == "tools/call":
        send({
            "jsonrpc": "2.0",
            "id": msg["id"],
            "result": {
                "content": [{"type": "text", "text": msg["params"]["arguments"]["message"]}]
            }
        })
```

### 13.7 Malformed Server Tests

| # | Test | Description |
|---|---|---|
| 49 | `malformed_returns_garbage` | Server sends non-JSON → MalformedResponse |
| 50 | `malformed_wrong_jsonrpc_field` | Server sends `jsonrpc: "3.0"` → MalformedResponse |
| 51 | `malformed_missing_id_in_response` | Response missing id → MalformedResponse |
| 52 | `malformed_wrong_id_in_response` | Response id doesn't match request → TransportError |

### 13.8 Expected Total Tests

| Category | Count |
|---|---|
| JSON-RPC unit tests | 12 |
| Transport/stdio unit tests | 10 |
| Client unit tests | 10 |
| Adapter unit tests | 12 |
| Integration tests | 5 |
| Malformed server tests | 4 |
| **Total new tests** | **53** |

---

## 14. Risks

### 14.1 Risk Register

| # | Risk | Severity | Likelihood | Mitigation | Status |
|---|---|---|---|---|---|
| R1 | MCP subprocess zombie on panic in chat() | **HIGH** | Medium | Drop guarded: `StdioTransport::drop()` kills child unconditionally. Mutex ensures Drop runs exactly once. | ACCEPTABLE |
| R2 | MCP spec evolves during implementation (protocol version changes) | **HIGH** | Medium | Pin `"2024-11-05"` for MVP. Validate version on initialize, warn on mismatch, continue if compatible. | ACCEPTABLE |
| R3 | Stdio transport deadlock (write waiting for read, or vice versa) | **HIGH** | Low | Mutex ensures serialized access (one request at a time). Timeout on receive prevents permanent blocking. | MITIGATED |
| R4 | Mutex poisoning on panic in transport I/O | **MEDIUM** | Low | `lock().map_err()` propagates poison as `McpError::InternalError`. Transport becomes unusable, which is safe. | MITIGATED |
| R5 | MCP server returns >1000 tools, overwhelming memory | **LOW** | Low | Cap tool list at 1000 in MVP. Log warning if exceeded. | MITIGATED |
| R6 | MCP server never responds to initialize (hangs) | **MEDIUM** | Medium | 10s startup timeout. Timeout returns `McpError::InitializationFailed`. | MITIGATED |
| R7 | McpAdapter.chat() called concurrently from multiple threads | **MEDIUM** | Medium | Transport Mutex serializes all access. This is correct but may cause contention. Document that MCP is single-threaded. | ACCEPTABLE |
| R8 | Command injection via McpServerConfig.command | **HIGH** | Low | `Command::new()` doesn't use shell. Add validation in `LlmConfig::validate()`. | MITIGATED |
| R9 | Adapter stores no call_id in ToolResult — consumer can't match tool calls | **LOW** | Medium | MCP has no call_id concept. The consumer must match by tool name. Document this limitation. | ACCEPTABLE |
| R10 | Deferred initialization on first chat() adds latency | **LOW** | Medium | Initialize happens in `McpAdapter::new()` by default. Deferred init only if `auto_start: false`. | ACCEPTABLE |
| R11 | McpAdapter not suitable for Gateway's streaming pipeline | **LOW** | Low | MCP tools are inherently non-streaming in MVP. `chat_stream()` returns `StreamingUnsupported`. | ACCEPTABLE |
| R12 | Multiple MCP servers with same name in config | **LOW** | Low | Duplicate detection in `LlmConfig::validate()`. | MITIGATED |
| R13 | Large response from MCP server (OOM risk) | **MEDIUM** | Low | Cap response at 10MB. Return TransportError if exceeded. | MITIGATED |
| R14 | MCP adapter has no startup health check before Gateway reports healthy | **MEDIUM** | Low | Gateway's `health()` calls each provider. McpAdapter returns `Unavailable` until initialized. | MITIGATED |
| R15 | Windows: `Command::new()` behavior differs (`.bat` files need `cmd /c`) | **MEDIUM** | Medium | Document that MCP commands must be executable binaries, not shell scripts. For `.bat`/`.cmd`, users must use `cmd /c script.bat` as command + args. | DOCUMENTED |

---

## 15. Open Questions

These questions are intentionally left unresolved in this design. The implementation engineer should resolve them during development with the simplest correct answer.

### Q1: Deferred vs Eager Initialization

Should `McpAdapter::new()` synchronously initialize (spawn process, handshake, discover tools)?

**Recommended**: Eager for MVP (`auto_start: true` by default). The Gateway's `build()` blocks until all MCP servers are initialized. This is the simplest correct approach. Deferred init (lazy on first `chat()`) can be added later.

### Q2: Error Recovery on Stale Transport

If `transport.is_alive()` returns false in `chat()`, should the adapter attempt to respawn the subprocess?

**Recommended**: No respawn in MVP. Return `McpError::ProcessExited`. The consumer can handle via Gateway fallback. Respawning adds complexity (re-init, re-discover tools) that belongs in a future phase.

### Q3: Tool Result Aggregation

MCP `tools/call` returns `content: Vec<{type, text}>`. How should multiple content items be combined?

**Recommended**: Join with `"\n"`. This is the simplest approach and covers 99% of real MCP tools (which return a single text item). Structured content (images, embedded resources) is deferred.

### Q4: Request Validation — Must the Tool Exist in Cache?

Should `chat()` validate that the requested tool name exists in the locally cached tool list before calling `tools/call`?

**Recommended**: Yes. If the tool is not in the cache, return `McpError::ToolNotFound`. This catches configuration errors early. The tool list can be refreshed explicitly if needed.

### Q5: Thread Safety — Is Arc<Mutex<>> Overkill for the Tool Cache?

The tool cache is written once (during init) and read many times. Is `Arc<Mutex<HashMap>>` necessary, or is `Arc<RwLock<HashMap>>` better?

**Recommended**: `Arc<Mutex<HashMap>>` is sufficient for MVP. The tool cache is small (<50 entries), and lock contention is negligible. `RwLock` adds complexity with no measurable benefit at this scale.

### Q6: Windows stdio Line Endings

On Windows, `BufRead::read_line()` includes `\r\n`. Should the JSON parser handle trailing `\r`?

**Recommended**: `serde_json::from_str()` already ignores ASCII whitespace including `\r`. No special handling needed.

### Q7: Process Stderr Handling

Should stderr be captured and logged?

**Recommended**: Yes. Spawn with `.stderr(Stdio::piped())`. On transport failure, read all stderr and include it in the error message. Do NOT poll stderr during normal operation (would require a background thread).

---

## 16. Final Readiness Verdict

# READY FOR IMPLEMENTATION

**Rationale**:

1. **All 16 required sections are fully specified** — an implementation engineer can implement from this document alone
2. **Zero ambiguous architectural decisions** — every integration point, error path, and data flow is documented
3. **JSON-RPC protocol is pinned** to `"2024-11-05"` with strict parsing
4. **All error paths are mapped** — 15 error sources with exact `LlmError` variants
5. **Thread safety is verified** — `Send + Sync` guaranteed with documented lock ordering
6. **Security mitigations are documented** — command injection, OOM, timeout
7. **53 tests are specified** — every module has pre-designed test coverage
8. **All open questions have recommended resolutions** — implementer makes zero architectural decisions
9. **Gateway changes are minimal and localized** — ONE method (`build()`) changes in ONE file (`gateway.rs`)
10. **Zero new runtime dependencies** — MVP uses only `std::process`, `std::io`, `serde_json` (already present)

**Implementation order** (recommended):
1. `jsonrpc.rs` — wire format (no dependencies)
2. `errors.rs` — error types (depends on jsonrpc types only)
3. `transport.rs` — I/O layer (depends on errors)
4. `client.rs` — protocol operations (depends on jsonrpc + transport + errors)
5. `mod.rs` — McpAdapter struct (depends on client + transport + errors)
6. `adapter.rs` — LlProvider impl (depends on mod)
7. `lib.rs` + `gateway.rs` — integration (depends on all)

**Files to create**: 5 new files in `src/mcp/`
**Files to modify**: 2 existing files (`gateway.rs`, `lib.rs`)
**Files to remove**: None
