//! # browseros-mcp
//!
//! MCP (Model Context Protocol) server for BrowserOS — the sole external
//! interface for the entire platform.
//!
//! ## Architecture
//!
//! ```text
//! MCP Client (stdio)
//!     │
//!     ├── McpServer (this crate)
//!     │     ├── ToolRegistry → McpTool implementations
//!     │     ├── SessionManager (Phase 6.2+)
//!     │     └── NotificationDispatcher (Phase 6.4+)
//!     │
//!     └── RuntimeContext (browseros-runtime)
//!           ├── LlGateway (browseros-llm)
//!           ├── BrowserPool (browseros-browser)
//!           ├── EventBus (browseros-event-bus)
//!           └── ...
//! ```
//!
//! ## Module tree
//!
//! - `server` — McpServer, McpServerBuilder, transport, dispatcher, metrics
//! - `tools` — McpTool trait, ToolRegistry, system tools
//! - `types` — McpToolContext, ToolOutput, ToolError, enums
//! - `error` — McpServerError, JSON-RPC error code mapping

pub mod error;
pub mod server;
pub mod tools;
pub mod types;

pub use server::{McpServer, McpServerBuilder};
pub use tools::system::{SystemHealthTool, SystemVersionTool};
pub use tools::{McpTool, ToolRegistry};
pub use types::{
    McpToolContext, ToolCapability, ToolError, ToolOutput, ToolPermission, ToolVisibility,
};

// Re-export JSON-RPC types from browseros-llm for protocol conformance
pub use browseros_llm::mcp::jsonrpc::{
    JsonRpcError, JsonRpcId, JsonRpcMessage, JsonRpcRequest, JsonRpcResponse, McpContentItem,
    McpInitializeResult, McpListToolsResult, McpServerInfo, McpToolDef,
};
