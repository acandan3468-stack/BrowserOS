pub mod dispatcher;
pub mod metrics;
pub mod transport;

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use browseros_runtime::RuntimeContext;

use crate::error::McpServerError;
use crate::server::dispatcher::dispatch;
use crate::server::metrics::{ServerMetrics, SharedMetrics};
use crate::server::transport::StdioReader;
use crate::tools::{McpTool, ToolRegistry};
use crate::types::McpToolContext;

/// The MCP server, constructed via [`McpServerBuilder`].
pub struct McpServer {
    runtime: Arc<RuntimeContext>,
    registry: Arc<ToolRegistry>,
    metrics: SharedMetrics,
    stdout: Arc<Mutex<std::io::Stdout>>,
    shutdown: Arc<std::sync::atomic::AtomicBool>,
}

impl McpServer {
    /// Start the server, blocking until shutdown is requested.
    ///
    /// Reads JSON-RPC messages from stdin and dispatches them.
    /// Returns when stdin closes or a shutdown request is received.
    pub fn start(self) -> Result<(), McpServerError> {
        let mut reader = StdioReader::new();
        let context = McpToolContext::new(Arc::clone(&self.runtime));

        loop {
            if self.shutdown.load(Ordering::SeqCst)
                || self.metrics.shutdown_requested.load(Ordering::SeqCst)
            {
                break;
            }

            let line = match reader.read_line() {
                Ok(l) => l,
                Err(McpServerError::TransportError(_)) => {
                    // stdin closed, shut down
                    break;
                }
                Err(e) => {
                    // Log error and continue
                    self.runtime.logger().error(format!("stdin error: {e}"));
                    break;
                }
            };

            if line.trim().is_empty() {
                continue;
            }

            let msg = match browseros_llm::mcp::jsonrpc::JsonRpcMessage::parse(&line) {
                Ok(m) => m,
                Err(e) => {
                    self.metrics.increment_errors();
                    let err_response = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": null,
                        "error": {
                            "code": -32700,
                            "message": format!("Parse error: {e}")
                        }
                    });
                    let json = serde_json::to_string(&err_response).unwrap_or_default();
                    if let Ok(mut stdout) = self.stdout.lock() {
                        use std::io::Write;
                        let _ = writeln!(stdout, "{json}");
                        let _ = stdout.flush();
                    }
                    continue;
                }
            };

            match msg {
                browseros_llm::mcp::jsonrpc::JsonRpcMessage::Notification(n) => {
                    dispatcher::handle_notification(&n.method, n.params, &self.metrics)?;
                }
                browseros_llm::mcp::jsonrpc::JsonRpcMessage::Request(req) => {
                    dispatch(
                        req,
                        &self.registry,
                        context.clone(),
                        &self.metrics,
                        &self.stdout,
                    )?;
                }
                // We never receive Response or ErrorResponse as a server
                _ => {}
            }

            if self.metrics.shutdown_requested.load(Ordering::SeqCst) {
                break;
            }
        }

        self.shutdown.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// Request a graceful shutdown.
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
    }

    /// Returns a snapshot of server metrics.
    pub fn metrics_snapshot(&self) -> metrics::MetricsSnapshot {
        self.metrics.snapshot()
    }
}

/// Builder for constructing a configured [`McpServer`].
pub struct McpServerBuilder {
    runtime: Option<Arc<RuntimeContext>>,
    tools: Vec<(String, Box<dyn McpTool>)>,
}

impl McpServerBuilder {
    pub fn new() -> Self {
        McpServerBuilder {
            runtime: None,
            tools: Vec::new(),
        }
    }

    /// Inject the runtime context for dependency injection.
    pub fn with_runtime(mut self, rt: Arc<RuntimeContext>) -> Self {
        self.runtime = Some(rt);
        self
    }

    /// Register a tool by name.
    pub fn with_tool(mut self, name: &str, tool: Box<dyn McpTool>) -> Self {
        self.tools.push((name.to_string(), tool));
        self
    }

    /// Consume the builder and construct the server.
    ///
    /// # Errors
    ///
    /// Returns `McpServerError::InternalError` if no runtime was provided.
    pub fn build(self) -> Result<McpServer, McpServerError> {
        let runtime = self
            .runtime
            .ok_or_else(|| McpServerError::InternalError("RuntimeContext is required".into()))?;

        let registry = Arc::new(ToolRegistry::new());
        for (name, tool) in self.tools {
            registry.register(&name, tool).map_err(|e| {
                McpServerError::InternalError(format!("tool registration failed: {e}"))
            })?;
        }

        runtime
            .logger()
            .info(format!("McpServer built with {} tools", registry.len()));

        Ok(McpServer {
            runtime,
            registry,
            metrics: Arc::new(ServerMetrics::new()),
            stdout: Arc::new(Mutex::new(std::io::stdout())),
            shutdown: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        })
    }
}

impl Default for McpServerBuilder {
    fn default() -> Self {
        Self::new()
    }
}
