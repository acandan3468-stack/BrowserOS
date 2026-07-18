use crate::error::LlmError;

#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum McpError {
    SpawnFailed(String),
    ProcessExited(Option<i32>),
    TransportError(String),
    Timeout { elapsed_ms: u64 },
    InvalidJson(String),
    UnsupportedMethod(String),
    JsonRpcError { code: i64, message: String },
    ToolNotFound(String),
    ToolExecutionError(String),
    InitializationFailed(String),
    InvalidRequest(String),
    InternalError(String),
}

impl std::fmt::Display for McpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McpError::SpawnFailed(msg) => write!(f, "MCP spawn failed: {msg}"),
            McpError::ProcessExited(code) => match code {
                Some(c) => write!(f, "MCP process exited with code {c}"),
                None => write!(f, "MCP process exited with no code"),
            },
            McpError::TransportError(msg) => write!(f, "MCP transport error: {msg}"),
            McpError::Timeout { elapsed_ms } => write!(f, "MCP timeout after {elapsed_ms}ms"),
            McpError::InvalidJson(msg) => write!(f, "MCP invalid JSON: {msg}"),
            McpError::UnsupportedMethod(m) => write!(f, "MCP unsupported method: {m}"),
            McpError::JsonRpcError { code, message } => {
                write!(f, "JSON-RPC error {code}: {message}")
            }
            McpError::ToolNotFound(name) => write!(f, "MCP tool not found: {name}"),
            McpError::ToolExecutionError(msg) => write!(f, "MCP tool execution error: {msg}"),
            McpError::InitializationFailed(msg) => write!(f, "MCP initialization failed: {msg}"),
            McpError::InvalidRequest(msg) => write!(f, "MCP invalid request: {msg}"),
            McpError::InternalError(msg) => write!(f, "MCP internal error: {msg}"),
        }
    }
}

impl From<McpError> for LlmError {
    fn from(e: McpError) -> Self {
        match e {
            McpError::SpawnFailed(msg) => LlmError::ConnectionError(msg),
            McpError::ProcessExited(code) => {
                let msg = match code {
                    Some(c) => format!("MCP process exited with code {c}"),
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
                    LlmError::ProviderError(format!("JSON-RPC error {code}: {message}"))
                }
            }
            McpError::ToolNotFound(name) => {
                LlmError::CapabilityNotSupported(format!("MCP tool '{name}' not found"))
            }
            McpError::ToolExecutionError(msg) => LlmError::ProviderError(msg),
            McpError::InitializationFailed(msg) => LlmError::ProviderUnavailable(msg),
            McpError::InvalidRequest(msg) => LlmError::InvalidRequest(msg),
            McpError::InternalError(msg) => LlmError::ProviderError(msg),
        }
    }
}

impl McpError {
    pub fn from_jsonrpc(code: i64, message: impl Into<String>) -> Self {
        McpError::JsonRpcError {
            code,
            message: message.into(),
        }
    }
}
