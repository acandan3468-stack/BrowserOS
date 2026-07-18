# browseros-mcp Error Model

**Document:** ERROR_MODEL.md  
**Phase:** Design Freeze  
**Date:** 2026-07-16  

---

## 1. Error Classification

All BrowserOS errors fall into one of six categories:

| Category | Recoverable | Retry Strategy | Telemetry Severity |
|-----------|-------------|----------------|---------------------|
| Protocol (MCP) | No | N/A | Error |
| Request (Client) | No | N/A | Warn |
| Resource | No | N/A | Error |
| Transport | Depends | Session recreate | Error |
| Browser (External) | Yes | Tool retry | Error |
| Internal (Bug) | No | N/A | Critical |

---

## 2. MCP Standard Errors

| Code | Name | Meaning | Source |
|------|------|---------|--------|
| -32700 | Parse Error | Invalid JSON received | Transport |
| -32600 | Invalid Request | Invalid JSON-RPC structure | Dispatcher |
| -32601 | Method Not Found | Unknown method or tool name | Dispatcher |
| -32602 | Invalid Params | Missing required field or type mismatch | Tool |
| -32603 | Internal Error | Unexpected server error | Any |

These are defined by the JSON-RPC 2.0 specification. BrowserOS passes them through unchanged from the MCP transport layer.

---

## 3. BrowserOS Custom Errors

### 3.1 Application Errors (-32000 range)

| Code | Name | Meaning | Recoverable? | Retry? | Severity | User Message |
|------|------|---------|-------------|--------|----------|--------------|
| -32000 | Application Error | Generic tool execution failure | No | No | Error | "Tool execution failed: {reason}" |
| -32001 | Session Not Found | Invalid or expired session_id | No | Create new | Warn | "Session not found. Create a new session with session/create." |
| -32002 | Session Limit Reached | Max concurrent sessions | No | Close others | Warn | "Session limit reached ({max}). Close an existing session first." |
| -32003 | Browser Not Available | No browser attached to session | No | Create session | Error | "No browser available. Ensure session has browser access." |
| -32004 | Browser Crashed | Browser process terminated | Yes | Recreate session | Critical | "Browser process crashed: {reason}. Create a new session." |
| -32005 | Browser Connection Lost | CDP connection failed | Yes | Recreate session | Error | "Browser connection lost. Create a new session." |
| -32006 | Element Not Found | DOM element not found | No | Change selector | Warn | "Element not found: {selector}. The page may have changed." |
| -32007 | Navigation Failed | Page failed to load | Yes | Retry navigate | Error | "Navigation to {url} failed: {reason}." |
| -32008 | Navigation Timeout | Page load timeout | Yes | Retry with longer timeout | Warn | "Navigation timed out after {timeout}ms." |
| -32009 | Network Interception Error | Interception setup failed | No | Retry | Error | "Network interception failed: {reason}." |
| -32010 | Network Conditions Error | Failed to set network conditions | No | Retry | Error | "Failed to set network conditions: {reason}." |
| -32011 | Cookie Error | Cookie operation failed | No | Retry | Warn | "Cookie operation failed: {reason}." |
| -32012 | Storage Error | Storage operation failed | No | Retry | Warn | "Storage operation failed: {reason}." |
| -32013 | Workflow Execution Failed | DAG execution error | No | Fix workflow | Error | "Workflow execution failed at step {step}: {reason}." |
| -32014 | Workflow Plan Failed | LLM plan generation failed | No | Rephrase goal | Warn | "Could not generate a plan: {reason}. Try rephrasing your goal." |
| -32015 | LLM Error | LLM provider returned error | Yes | Retry | Error | "LLM error: {reason}. Check LLM configuration." |
| -32016 | LLM Provider Not Available | No LLM provider configured | No | Configure LLM | Warn | "No LLM provider available. Configure an LLM provider first." |
| -32017 | Tool Execution Timeout | Tool exceeded max duration | Yes | Retry | Warn | "Tool execution timed out after {timeout}ms." |
| -32018 | Tool Cancelled | Execution cancelled by client | No | N/A | Info | "Tool execution was cancelled." |
| -32019 | Permission Denied | Client lacks required permission | No | N/A | Warn | "Permission denied: {required_permission}." |
| -32020 | Tool Not Supported | Tool not available in this context | No | N/A | Warn | "Tool {name} is not available in the current context." |
| -32021 | Config Error | Configuration validation failed | No | Fix config | Error | "Configuration error: {reason}." |
| -32022 | Session Not Initialized | Session not yet ready | No | Wait | Warn | "Session is still initializing. Try again shortly." |

### 3.2 Internal Errors (not exposed to client)

| Code | Meaning | Logged As | Action |
|------|---------|-----------|--------|
| -32099 | Internal: Thread pool full | Critical | Log, reject request |
| -32098 | Internal: Mutex poisoned | Critical | Log, abort session |
| -32097 | Internal: Channel full | Error | Log, drop message |
| -32096 | Internal: Resource leak detected | Warning | Log, clean up |
| -32095 | Internal: Unexpected error variant | Error | Log stack trace |

Internal errors are NEVER returned to the MCP client as-is. They are converted to `-32603 Internal Error` with a sanitized message.

---

## 4. Mapping BrowserOS Errors to MCP JSON-RPC

```
BrowserOS Internal Error
       │
       ├── Is it a protocol error? → Map to -32700, -32600, -32601, -32602
       │
       ├── Is it an application error? → Map to -32000..-32022
       │
       ├── Is it an internal error? → Map to -32603 (sanitized)
       │
       └── Is it a timeout? → Map to -32003 or -32008 or -32017
```

### 4.1 Mapping Table

| BrowserOS Error Type | MCP Code | Notes |
|---------------------|----------|-------|
| `McpError::ParseError` | -32700 | Passthrough from transport layer |
| `McpError::InvalidRequest` | -32600 | Passthrough |
| `McpError::MethodNotFound` | -32601 | Passthrough |
| `McpError::InvalidParams` | -32602 | Passthrough |
| `ToolError::NotFound` | -32601 (MethodNotFound) | Unknown tool name |
| `ToolError::InvalidParams` | -32602 (Invalid Params) | Schema validation failure |
| `ToolError::ExecutionError` | -32000 (Application Error) | Generic tool failure |
| `SessionError::NotFound` | -32001 (Session Not Found) | |
| `SessionError::LimitReached` | -32002 (Session Limit Reached) | |
| `BrowserError::NotAvailable` | -32003 (Browser Not Available) | |
| `BrowserError::Crashed` | -32004 (Browser Crashed) | |
| `BrowserError::ConnectionLost` | -32005 (Browser Connection Lost) | |
| `DomError::ElementNotFound` | -32006 (Element Not Found) | |
| `NavigationError::Failed` | -32007 (Navigation Failed) | |
| `NavigationError::Timeout` | -32008 (Navigation Timeout) | |
| `NetworkError::Interception` | -32009 (Network Interception Error) | |
| `NetworkError::Conditions` | -32010 (Network Conditions Error) | |
| `StorageError::CookieError` | -32012 (Storage Error) | |
| `WorkflowError::ExecutionFailed` | -32013 (Workflow Execution Failed) | |
| `WorkflowError::PlanFailed` | -32014 (Workflow Plan Failed) | |
| `LlmError::ProviderError` | -32015 (LLM Error) | |
| `LlmError::ConfigurationError` | -32016 (LLM Provider Not Available) | |
| `ToolError::Timeout` | -32017 (Tool Execution Timeout) | |
| `ToolError::Cancelled` | -32018 (Tool Cancelled) | |
| `ToolError::PermissionDenied` | -32019 (Permission Denied) | |
| `ToolError::NotSupported` | -32020 (Tool Not Supported) | |
| `ConfigError::ValidationFailed` | -32021 (Config Error) | |
| `SessionError::NotInitialized` | -32022 (Session Not Initialized) | |
| Any unknown/unexpected | -32603 (Internal Error) | Sanitized message |

---

## 5. Error Response Format

```json
{
  "jsonrpc": "2.0",
  "id": 10,
  "error": {
    "code": -32006,
    "message": "Element not found: #submit-btn",
    "data": {
      "selector": "#submit-btn",
      "session_id": "sess_01J3YF...",
      "tool": "dom/click",
      "timestamp": "2026-07-16T12:00:00Z",
      "recoverable": false,
      "retryable": false,
      "details": {
        "expected_tag": "button",
        "page_url": "https://example.com/login",
        "dom_snapshot_available": true
      }
    }
  }
}
```

The `data` field is optional and MAY contain:
- `session_id` — if the error is session-scoped
- `tool` — the tool that produced the error
- `recoverable` — whether the error can be recovered from (client hint)
- `retryable` — whether retrying the same request may succeed
- `details` — structured error context (tool-specific)

### 5.1 Security: Data Sanitization

The `error.data` field MUST NOT contain:
- API keys or tokens
- File system paths (unless explicitly included)
- Stack traces
- Internal IP addresses
- Full parameter dumps

Sanitization is applied by the error formatting layer before serialization.

---

## 6. McpServerError Definition

```rust
#[non_exhaustive]
pub enum McpServerError {
    // MCP Protocol
    ParseError(String),
    InvalidRequest(String),
    MethodNotFound(String),
    InvalidParams(String),
    InternalError(String),

    // Application Errors
    ApplicationError { code: i32, message: String, data: Option<serde_json::Value> },

    // Session
    SessionNotFound(SessionId),

    // Transport
    TransportError(String),
    TransportTimeout { elapsed_ms: u64 },

    // Shutdown
    ServerShuttingDown,
    CancelRequested,

    // Internal
    PoisonedLock(String),
    ThreadPoolExhausted,
}
```

### 6.1 From Conversions

`McpServerError` implements `From` for all internal error types:

- `From<SessionError> for McpServerError`
- `From<ToolError> for McpServerError`
- `From<BrowserError> for McpServerError`
- `From<LlmError> for McpServerError`
- `From<WorkflowError> for McpServerError`
- `From<DomError> for McpServerError`
- `From<NetworkError> for McpServerError`
- `From<StorageError> for McpServerError`
- `From<ConfigError> for McpServerError`

Each implementation maps to the appropriate MCP error code per the table above.

---

## 7. Error Logging and Telemetry

Every error that reaches the dispatcher is:

1. Logged to stderr with the appropriate level
2. Recorded in metrics (counter by error code)
3. Associated with session_id telemetry context (if available)
4. Sanitized before sending to client

### 7.1 Severity Mapping

| Error Category | Log Level | Metric Tag |
|----------------|-----------|------------|
| Protocol (MCP) | error | protocol_error |
| Application | warn | application_error |
| Browser | error | browser_error |
| Transport | error | transport_error |
| Internal | critical | internal_error |
| Cancellation | info | cancelled |
| Timeout | warn | timeout |

---

*This document defines the complete Error Model. It contains no Rust code, no Cargo.toml, and no placeholders.*
