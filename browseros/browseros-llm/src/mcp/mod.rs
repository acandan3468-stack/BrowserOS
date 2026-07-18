pub mod adapter;
mod client;
pub mod errors;
pub mod jsonrpc;
pub mod transport;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime};

use crate::types::{LlTool, McpServerConfig, ProviderHealth, ProviderHealthResult};
use client::McpClient;
pub use errors::McpError;
use transport::{StdioTransport, Transport};

/// MCP transport protocol selector.
///
/// Currently only `Stdio` is supported. Future transports (WebSocket, SSE)
/// will be added as new variants.
#[non_exhaustive]
pub enum McpTransport {
    /// Subprocess stdin/stdout transport.
    Stdio,
}

/// An MCP provider adapter wrapping a single stdio MCP server.
///
/// Implements `LlProvider` to expose MCP tools through the BrowserOS LLM
/// Gateway protocol.
///
/// # Lifecycle
///
/// - `new()` configures the adapter. If `auto_start` is `true` (default),
///   the subprocess is spawned immediately. If `false`, spawn is deferred
///   until the first call to `initialize()` or any I/O operation.
/// - `initialize()` performs the MCP handshake (`initialize` request,
///   `notifications/initialized`), discovers tools via `tools/list`,
///   and caches them.
/// - Tools are called through `call_tool()`.
/// - On `Drop`, the subprocess is shut down.
///
/// # Thread safety
///
/// `McpAdapter` is `Send + Sync`. All mutable state is behind `Arc<Mutex<...>>`
/// or `AtomicBool`.
pub struct McpAdapter {
    pub(crate) id: String,
    pub(crate) config: McpServerConfig,
    transport: Mutex<Option<Arc<Mutex<dyn Transport>>>>,
    client: Mutex<Option<McpClient>>,
    pub(crate) tools: Arc<Mutex<HashMap<String, LlTool>>>,
    pub(crate) initialized: AtomicBool,
}

impl McpAdapter {
    /// Create a new MCP adapter.
    ///
    /// If `config.auto_start` is `true` (default), the MCP server subprocess
    /// is spawned immediately. If `false`, spawning is deferred to the first
    /// I/O operation (typically in `initialize()`).
    ///
    /// # Errors
    ///
    /// When `auto_start` is `true`, returns `McpError::SpawnFailed` if the
    /// subprocess cannot be started. When `auto_start` is `false`, `new()`
    /// never spawns and never fails.
    pub fn new(config: McpServerConfig) -> Result<Self, McpError> {
        let id = config.name.clone();
        let tools = Arc::new(Mutex::new(HashMap::new()));

        if config.auto_start {
            let stdio = StdioTransport::spawn(&config.command, &config.args)?;
            let transport: Arc<Mutex<dyn Transport>> = Arc::new(Mutex::new(stdio));
            let client = McpClient::new(Arc::clone(&transport));
            Ok(Self {
                id,
                config,
                transport: Mutex::new(Some(transport)),
                client: Mutex::new(Some(client)),
                tools,
                initialized: AtomicBool::new(false),
            })
        } else {
            Ok(Self {
                id,
                config,
                transport: Mutex::new(None),
                client: Mutex::new(None),
                tools,
                initialized: AtomicBool::new(false),
            })
        }
    }

    /// Ensure the subprocess is running, spawning it lazily if needed.
    fn ensure_spawned(&self) -> Result<(), McpError> {
        let mut guard = self
            .transport
            .lock()
            .map_err(|e| McpError::InternalError(format!("transport lock poisoned: {e}")))?;
        if guard.is_some() {
            return Ok(());
        }
        let stdio = StdioTransport::spawn(&self.config.command, &self.config.args)?;
        let transport: Arc<Mutex<dyn Transport>> = Arc::new(Mutex::new(stdio));
        let client = McpClient::new(Arc::clone(&transport));
        *guard = Some(transport);
        drop(guard);
        let mut client_guard = self
            .client
            .lock()
            .map_err(|e| McpError::InternalError(format!("client lock poisoned: {e}")))?;
        *client_guard = Some(client);
        Ok(())
    }

    /// Returns a locked reference to the client for I/O operations.
    fn with_client<F, T>(&self, f: F) -> Result<T, McpError>
    where
        F: FnOnce(&McpClient) -> Result<T, McpError>,
    {
        self.ensure_spawned()?;
        let guard = self
            .client
            .lock()
            .map_err(|e| McpError::InternalError(format!("client lock poisoned: {e}")))?;
        match guard.as_ref() {
            Some(client) => f(client),
            None => Err(McpError::InternalError("client not available".into())),
        }
    }

    /// Perform the MCP initialization handshake and discover tools.
    ///
    /// The subprocess is spawned here if `auto_start` was `false` and this
    /// is the first I/O operation.
    ///
    /// # Protocol
    ///
    /// 1. Sends `initialize` with protocol version `2024-11-05`.
    /// 2. Receives server capabilities and identity.
    /// 3. Sends `notifications/initialized`.
    /// 4. Calls `tools/list` and caches the tool definitions.
    pub fn initialize(&self) -> Result<(), McpError> {
        self.with_client(|client| {
            client.initialize()?;
            let tool_list = client.list_tools()?;

            let mut cache = self
                .tools
                .lock()
                .map_err(|e| McpError::InternalError(format!("tools lock poisoned: {e}")))?;
            cache.clear();
            for tool in tool_list {
                cache.insert(tool.name.clone(), tool);
            }

            self.initialized.store(true, Ordering::Release);
            Ok(())
        })
    }

    /// Returns the list of discovered MCP tools.
    pub fn tools(&self) -> Vec<LlTool> {
        self.tools
            .lock()
            .map(|cache| cache.values().cloned().collect())
            .unwrap_or_default()
    }

    /// Re-fetch the tool list from the MCP server, updating the cache.
    pub fn refresh_tools(&self) -> Result<(), McpError> {
        self.with_client(|client| {
            let tool_list = client.list_tools()?;
            let mut cache = self
                .tools
                .lock()
                .map_err(|e| McpError::InternalError(format!("tools lock poisoned: {e}")))?;
            cache.clear();
            for tool in tool_list {
                cache.insert(tool.name.clone(), tool);
            }
            Ok(())
        })
    }

    /// Call an MCP tool by name with the given JSON arguments.
    ///
    /// Returns the tool's text output (all content items joined by newlines).
    ///
    /// # Errors
    ///
    /// - `McpError::ToolNotFound` if the tool name is unknown.
    /// - `McpError::ToolExecutionError` if the server returned `isError: true`.
    /// - `McpError::Timeout` if the server does not respond within the timeout.
    pub fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
        timeout_ms: u64,
    ) -> Result<String, McpError> {
        {
            let cache = self
                .tools
                .lock()
                .map_err(|e| McpError::InternalError(format!("tools lock poisoned: {e}")))?;
            if !cache.contains_key(name) {
                return Err(McpError::ToolNotFound(name.into()));
            }
        }

        let result = self.with_client(|client| client.call_tool(name, arguments, timeout_ms))?;

        let output = result
            .content
            .iter()
            .map(|c| c.text.clone())
            .collect::<Vec<_>>()
            .join("\n");
        Ok(output)
    }

    /// Check the MCP server's health.
    ///
    /// Returns `Unavailable` if not yet initialized, or if the subprocess
    /// has exited.
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

        let transport = match self.transport.lock() {
            Ok(guard) => match guard.as_ref() {
                Some(t) => Arc::clone(t),
                None => {
                    return ProviderHealthResult {
                        status: ProviderHealth::Unavailable {
                            since: SystemTime::now(),
                        },
                        latency_ms: None,
                        checked_at: SystemTime::now(),
                        error: Some("not spawned".into()),
                    };
                }
            },
            Err(_) => {
                return ProviderHealthResult {
                    status: ProviderHealth::Unavailable {
                        since: SystemTime::now(),
                    },
                    latency_ms: None,
                    checked_at: SystemTime::now(),
                    error: Some("transport lock poisoned".into()),
                };
            }
        };

        let start = Instant::now();
        match transport.lock().map(|mut t| t.is_alive()).unwrap_or(false) {
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

impl Drop for McpAdapter {
    fn drop(&mut self) {
        if let Ok(guard) = self.transport.lock() {
            if let Some(transport) = guard.as_ref() {
                if let Ok(mut t) = transport.lock() {
                    t.shutdown();
                }
            }
        }
    }
}

fn _assert_send_sync()
where
    McpAdapter: Send + Sync,
    McpClient: Send + Sync,
{
}
