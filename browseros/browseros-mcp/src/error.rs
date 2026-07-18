use std::sync::{Mutex, RwLock};

#[non_exhaustive]
#[derive(Debug)]
pub enum McpServerError {
    ParseError(String),
    InvalidRequest(String),
    MethodNotFound(String),
    InvalidParams(String),
    InternalError(String),
    ApplicationError {
        code: i32,
        message: String,
        data: Option<serde_json::Value>,
    },
    TransportError(String),
    TransportTimeout {
        elapsed_ms: u64,
    },
    ServerShuttingDown,
}

impl std::fmt::Display for McpServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McpServerError::ParseError(msg) => write!(f, "parse error: {msg}"),
            McpServerError::InvalidRequest(msg) => write!(f, "invalid request: {msg}"),
            McpServerError::MethodNotFound(msg) => write!(f, "method not found: {msg}"),
            McpServerError::InvalidParams(msg) => write!(f, "invalid params: {msg}"),
            McpServerError::InternalError(msg) => write!(f, "internal error: {msg}"),
            McpServerError::ApplicationError { code, message, .. } => {
                write!(f, "error {code}: {message}")
            }
            McpServerError::TransportError(msg) => write!(f, "transport error: {msg}"),
            McpServerError::TransportTimeout { elapsed_ms } => {
                write!(f, "transport timeout after {elapsed_ms}ms")
            }
            McpServerError::ServerShuttingDown => write!(f, "server is shutting down"),
        }
    }
}

impl std::error::Error for McpServerError {}

impl McpServerError {
    pub fn jsonrpc_code(&self) -> i64 {
        match self {
            McpServerError::ParseError(_) => -32700,
            McpServerError::InvalidRequest(_) => -32600,
            McpServerError::MethodNotFound(_) => -32601,
            McpServerError::InvalidParams(_) => -32602,
            McpServerError::InternalError(_) => -32603,
            McpServerError::ApplicationError { code, .. } => *code as i64,
            McpServerError::TransportError(_) => -32603,
            McpServerError::TransportTimeout { .. } => -32017,
            McpServerError::ServerShuttingDown => -32000,
        }
    }

    pub fn to_jsonrpc_error(&self) -> browseros_llm::mcp::jsonrpc::JsonRpcError {
        use browseros_llm::mcp::jsonrpc::JsonRpcError;
        let data = match self {
            McpServerError::ApplicationError { data, .. } => data.clone(),
            _ => None,
        };
        JsonRpcError {
            code: self.jsonrpc_code(),
            message: self.to_string(),
            data,
        }
    }
}

impl From<std::io::Error> for McpServerError {
    fn from(e: std::io::Error) -> Self {
        McpServerError::TransportError(e.to_string())
    }
}

impl From<serde_json::Error> for McpServerError {
    fn from(e: serde_json::Error) -> Self {
        McpServerError::ParseError(e.to_string())
    }
}

impl<T> From<std::sync::PoisonError<Mutex<T>>> for McpServerError {
    fn from(e: std::sync::PoisonError<Mutex<T>>) -> Self {
        McpServerError::InternalError(format!("mutex poisoned: {e}"))
    }
}

impl<T> From<std::sync::PoisonError<RwLock<T>>> for McpServerError {
    fn from(e: std::sync::PoisonError<RwLock<T>>) -> Self {
        McpServerError::InternalError(format!("rwlock poisoned: {e}"))
    }
}

impl From<super::types::ToolError> for McpServerError {
    fn from(e: super::types::ToolError) -> Self {
        match e {
            super::types::ToolError::NotFound(msg) => McpServerError::MethodNotFound(msg),
            super::types::ToolError::InvalidParams(msg) => McpServerError::InvalidParams(msg),
            super::types::ToolError::ExecutionError(msg) => McpServerError::ApplicationError {
                code: -32000,
                message: msg,
                data: None,
            },
            super::types::ToolError::Timeout { timeout_ms } => McpServerError::TransportTimeout {
                elapsed_ms: timeout_ms,
            },
            super::types::ToolError::Cancelled => McpServerError::ApplicationError {
                code: -32018,
                message: "Tool execution was cancelled.".into(),
                data: None,
            },
            super::types::ToolError::PermissionDenied(msg) => McpServerError::ApplicationError {
                code: -32019,
                message: msg,
                data: None,
            },
            super::types::ToolError::NotSupported(msg) => McpServerError::ApplicationError {
                code: -32020,
                message: msg,
                data: None,
            },
        }
    }
}
