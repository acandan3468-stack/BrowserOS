# MCP Adapter Design

**Phase:** 2.1 — Design Freeze  
**Status:** Draft  

---

## 1. MCP's Role in BrowserOS

MCP (Model Context Protocol) is an **adapter** that communicates with the **LLM Gateway** — it is NOT the LLM runtime itself. MCP provides:

- A protocol for tool/function discovery
- A protocol for tool execution
- A protocol for resource access
- Structured context delivery for prompts

BrowserOS uses MCP as one of many ways to interact with a planner. The MCP adapter lives in `browseros-llm/mcp/` and maps between MCP protocol messages and BrowserOS internal types.

---

## 2. Architecture

```
                     browseros-runtime
                           │
┌──────────────────────────▼──────────────────────────┐
│                 browseros-llm                        │
│                                                      │
│  ┌──────────────────────────────────────────────┐   │
│  │              LlGateway                        │   │
│  │  ┌──────────┐  ┌──────────┐  ┌────────────┐  │   │
│  │  │ Router    │  │ Provider │  │ Cache      │  │   │
│  │  └──────────┘  └──────────┘  └────────────┘  │   │
│  └────────────────────┬──────────────────────────┘   │
│                       │                               │
│  ┌────────────────────▼──────────────────────────┐   │
│  │            MCP Adapter                         │   │
│  │  ┌─────────────┐  ┌──────────────────────┐    │   │
│  │  │ McpTransport │  │ McpToolRegistry      │    │   │
│  │  │ (stdio/REST) │  │ (tool→capability map) │    │   │
│  │  └─────────────┘  └──────────────────────┘    │   │
│  └────────────────────────────────────────────────┘   │
│                                                      │
│  ┌──────────────────────────────────────────────┐   │
│  │  External MCP Servers                        │   │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐    │   │
│  │  │ Server A │ │ Server B │ │ Server C │    │   │
│  │  └──────────┘ └──────────┘ └──────────┘    │   │
│  └──────────────────────────────────────────────┘   │
└──────────────────────────────────────────────────────┘
```

---

## 3. MCP Transport Layer

```rust
// browseros-llm/src/mcp/adapter.rs

/// MCP Transport — how messages are sent/received
pub trait McpTransport: Send + Sync {
    fn send(&self, message: McpMessage) -> Result<(), LlmError>;
    fn recv(&self) -> Result<McpMessage, LlmError>;
    fn is_connected(&self) -> bool;
}

/// MCP Message (JSON-RPC based)
pub struct McpMessage {
    pub jsonrpc: String,         // "2.0"
    pub id: Option<McpId>,
    pub method: Option<String>,
    pub params: Option<serde_json::Value>,
    pub result: Option<serde_json::Value>,
    pub error: Option<McpError>,
}

/// Transport implementations
pub struct StdioTransport {
    process: std::process::Child,
    stdin: BufWriter<std::process::ChildStdin>,
    stdout: BufReader<std::process::ChildStdout>,
}

pub struct RestTransport {
    client: reqwest::blocking::Client,
    base_url: String,
    headers: HeaderMap,
}

pub struct WebSocketTransport {
    // Future: for persistent connections
}
```

---

## 4. MCP Tool Registry

```rust
/// Maps MCP tool definitions to BrowserOS capabilities
pub struct McpToolRegistry {
    /// Registered tools from MCP servers
    tools: HashMap<String, McpToolDef>,
    /// Capability → tool mapping
    capability_map: HashMap<String, Vec<String>>,
}

pub struct McpToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub server_id: String,              // Which MCP server owns this tool
    pub browseros_capabilities: Vec<String>,  // Maps to LlGateway capabilities
}

impl McpToolRegistry {
    /// Register a tool from MCP server handshake
    pub fn register_tool(&mut self, server_id: &str, tool: ToolDefinition);

    /// List all tools as BrowserOS LlTools
    pub fn to_ll_tools(&self) -> Vec<LlTool>;

    /// Find tools that match a capability
    pub fn find_by_capability(&self, capability: &str) -> Vec<&McpToolDef>;

    /// Execute a tool via the MCP server
    pub fn execute_tool(
        &self,
        server_id: &str,
        tool_name: &str,
        arguments: HashMap<String, serde_json::Value>,
    ) -> Result<String, LlmError>;
}
```

---

## 5. MCP ↔ Gateway Data Flow

### Tool Discovery Flow

```
Gateway startup
  │
  ├─ MCP Adapter connects to registered servers (stdio)
  │     └─ For each server:
  │           ├─ Send initialize request
  │           ├─ Receive server capabilities + tools
  │           └─ Register tools in McpToolRegistry
  │
  ├─ McpToolRegistry → LlTool conversion
  │     └─ Each McpToolDef becomes an LlTool
  │
  └─ LlTools are available via LlGateway for planner use
```

### Chat + Tool Execution Flow

```
LLMPlanner.plan()
  │
  ├─ LlGateway::chat(LlRequest { tools: McpToolRegistry::to_ll_tools(), ... })
  │     └─ Provider returns response with tool_calls
  │
  ├─ LLMPlanner parses tool_calls
  │     └─ For each tool_call:
  │           ├─ McpToolRegistry::execute_tool(server, tool, args)
  │           │     └─ StdioTransport::send(tool_execution_request)
  │           │     └─ StdioTransport::recv() → tool result
  │           └─ Append ToolResult message to conversation
  │
  └─ Continue: LlGateway::chat(next_request with ToolResult messages)
```

---

## 6. MCP Adapter Configuration

```toml
[mcp]
servers = [
    { name = "filesystem", command = "npx", args = ["-y", "@modelcontextprotocol/server-filesystem", "./"] },
    { name = "github", command = "npx", args = ["-y", "@modelcontextprotocol/server-github"] },
    { name = "custom", transport = "rest", base_url = "http://localhost:8080/mcp" },
]

# Capability routing: Which MCP tools back which LLM capabilities
[mcp.capabilities]
planning = ["filesystem", "github"]     # Planner can use filesystem + github tools
code = ["filesystem"]                   # Code capability has filesystem access
```

---

## 7. Design Invariants

| # | Invariant | Enforcement |
|---|---|---|
| 1 | MCP adapter lives in browseros-llm | Directory structure |
| 2 | MCP adapter does not depend on browseros-dag | Cargo.toml |
| 3 | MCP tools are surfaced as LlTool | Conversion in adapter |
| 4 | Gateway doesn't know about MCP | Gateway types are provider-agnostic |
| 5 | MCP transport is pluggable | McpTransport trait |
| 6 | Tool execution is synchronous | StdioTransport blocks on read |
| 7 | MCP errors → LlmError | Adapter maps errors |
