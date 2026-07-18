use std::sync::Arc;

use browseros_llm::mcp::jsonrpc::McpContentItem;
use browseros_runtime::RuntimeContext;

/// Context passed to every tool execution.
#[derive(Clone)]
pub struct McpToolContext {
    runtime: Arc<RuntimeContext>,
}

impl McpToolContext {
    pub fn new(runtime: Arc<RuntimeContext>) -> Self {
        McpToolContext { runtime }
    }

    pub fn runtime(&self) -> &Arc<RuntimeContext> {
        &self.runtime
    }
}

/// Successful tool execution result.
#[derive(Debug, Clone)]
pub struct ToolOutput {
    pub content: Vec<McpContentItem>,
    pub is_error: bool,
}

impl ToolOutput {
    pub fn text(text: impl Into<String>) -> Self {
        ToolOutput {
            content: vec![McpContentItem {
                type_: "text".into(),
                text: text.into(),
            }],
            is_error: false,
        }
    }

    pub fn empty() -> Self {
        ToolOutput {
            content: vec![],
            is_error: false,
        }
    }
}

/// Errors that can occur during tool execution.
#[derive(Debug, Clone)]
pub enum ToolError {
    NotFound(String),
    InvalidParams(String),
    ExecutionError(String),
    Timeout { timeout_ms: u64 },
    Cancelled,
    PermissionDenied(String),
    NotSupported(String),
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolError::NotFound(msg) => write!(f, "not found: {msg}"),
            ToolError::InvalidParams(msg) => write!(f, "invalid params: {msg}"),
            ToolError::ExecutionError(msg) => write!(f, "execution error: {msg}"),
            ToolError::Timeout { timeout_ms } => write!(f, "timeout after {timeout_ms}ms"),
            ToolError::Cancelled => write!(f, "cancelled"),
            ToolError::PermissionDenied(msg) => write!(f, "permission denied: {msg}"),
            ToolError::NotSupported(msg) => write!(f, "not supported: {msg}"),
        }
    }
}

impl std::error::Error for ToolError {}

/// Tool visibility level.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ToolVisibility {
    Public,
    Internal,
    Hidden,
    Diagnostic,
}

impl ToolVisibility {
    pub fn is_visible(&self) -> bool {
        matches!(self, ToolVisibility::Public)
    }
}

/// Tool capability flags.
#[derive(Debug, Clone, PartialEq)]
pub enum ToolCapability {
    RequiresSession,
    ReadOnly,
    MutatesState,
    RequiresBrowser,
    RequiresLLM,
    RequiresNetwork,
    LongRunning,
    CanProduceProgress,
    SupportsCancellation,
    Idempotent,
}

/// Tool permission requirements.
#[derive(Debug, Clone, PartialEq)]
pub enum ToolPermission {
    None,
    SessionRequired,
    BrowserAccess,
    NetworkAccess,
    WorkflowExecution,
    CredentialAccess,
    Admin,
}
