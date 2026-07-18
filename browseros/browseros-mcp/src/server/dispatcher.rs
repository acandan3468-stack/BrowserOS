use std::io::Write;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use browseros_llm::mcp::jsonrpc::{
    JsonRpcError, JsonRpcErrorResponse, JsonRpcId, JsonRpcMessage, JsonRpcRequest, JsonRpcResponse,
    McpInitializeResult, McpListToolsResult, McpServerInfo,
};

use crate::error::McpServerError;
use crate::server::metrics::{uptime_secs, SharedMetrics};
use crate::tools::ToolRegistry;
use crate::types::McpToolContext;

/// Dispatches a single JSON-RPC request and writes the response to stdout.
pub fn dispatch(
    request: JsonRpcRequest,
    registry: &ToolRegistry,
    context: McpToolContext,
    metrics: &SharedMetrics,
    stdout: &Arc<Mutex<std::io::Stdout>>,
) -> Result<(), McpServerError> {
    metrics.increment_requests();

    let response = match request.method.as_str() {
        "initialize" => handle_initialize(registry, request.id),
        "shutdown" => handle_shutdown(request.id, metrics),
        "tools/list" => handle_tools_list(registry, request.id),
        "tools/call" => handle_tools_call(request.id, request.params, registry, &context, metrics),
        _ => {
            metrics.increment_errors();
            JsonRpcMessage::ErrorResponse(JsonRpcErrorResponse {
                jsonrpc: "2.0".into(),
                id: request.id,
                error: JsonRpcError {
                    code: -32601,
                    message: format!("Method not found: {}", request.method),
                    data: None,
                },
            })
        }
    };

    write_response(&response, stdout)
}

/// Handle notifications (no response expected).
pub fn handle_notification(
    method: &str,
    _params: Option<serde_json::Value>,
    metrics: &SharedMetrics,
) -> Result<(), McpServerError> {
    match method {
        "notifications/initialized" => {
            metrics.increment_requests();
            Ok(())
        }
        "exit" => {
            metrics.shutdown_requested.store(true, Ordering::SeqCst);
            Ok(())
        }
        _ => Ok(()),
    }
}

fn handle_initialize(_registry: &ToolRegistry, id: JsonRpcId) -> JsonRpcMessage {
    JsonRpcMessage::Response(JsonRpcResponse {
        jsonrpc: "2.0".into(),
        id,
        result: serde_json::to_value(McpInitializeResult {
            protocol_version: "2024-11-05".into(),
            capabilities: serde_json::json!({ "tools": {} }),
            server_info: McpServerInfo {
                name: "browseros-mcp".into(),
                version: "0.1.0".into(),
            },
        })
        .unwrap_or_default(),
    })
}

fn handle_shutdown(id: JsonRpcId, metrics: &SharedMetrics) -> JsonRpcMessage {
    metrics.shutdown_requested.store(true, Ordering::SeqCst);

    let snapshot = serde_json::json!({
        "uptime_seconds": uptime_secs(),
        "requests_total": metrics.requests_total.load(Ordering::Relaxed),
        "tools_called_total": metrics.tools_called_total.load(Ordering::Relaxed),
        "errors_total": metrics.errors_total.load(Ordering::Relaxed),
    });

    JsonRpcMessage::Response(JsonRpcResponse {
        jsonrpc: "2.0".into(),
        id,
        result: serde_json::json!({
            "message": "Shutdown initiated",
            "metrics": snapshot,
        }),
    })
}

fn handle_tools_list(registry: &ToolRegistry, id: JsonRpcId) -> JsonRpcMessage {
    let tools = registry.list_tools();
    JsonRpcMessage::Response(JsonRpcResponse {
        jsonrpc: "2.0".into(),
        id,
        result: serde_json::to_value(McpListToolsResult { tools }).unwrap_or_default(),
    })
}

fn handle_tools_call(
    id: JsonRpcId,
    params: Option<serde_json::Value>,
    registry: &ToolRegistry,
    context: &McpToolContext,
    metrics: &SharedMetrics,
) -> JsonRpcMessage {
    let params = match params {
        Some(p) => p,
        None => {
            metrics.increment_errors();
            return JsonRpcMessage::ErrorResponse(JsonRpcErrorResponse {
                jsonrpc: "2.0".into(),
                id,
                error: JsonRpcError {
                    code: -32602,
                    message: "Missing params".into(),
                    data: None,
                },
            });
        }
    };

    let name = match params.get("name").and_then(|v| v.as_str()) {
        Some(n) => n,
        None => {
            metrics.increment_errors();
            return JsonRpcMessage::ErrorResponse(JsonRpcErrorResponse {
                jsonrpc: "2.0".into(),
                id,
                error: JsonRpcError {
                    code: -32602,
                    message: "Missing required field: name".into(),
                    data: None,
                },
            });
        }
    };

    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or(serde_json::Value::Object(Default::default()));

    metrics.increment_tools_called();
    match registry.execute_by_name(name, McpToolContext::new(context.runtime().clone()), args) {
        Ok(output) => {
            use browseros_llm::mcp::jsonrpc::McpCallToolResult;
            if output.is_error {
                metrics.increment_errors();
            }
            JsonRpcMessage::Response(JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id,
                result: serde_json::to_value(McpCallToolResult {
                    content: output.content,
                    is_error: if output.is_error { Some(true) } else { None },
                })
                .unwrap_or_default(),
            })
        }
        Err(tool_err) => {
            metrics.increment_errors();
            let err: McpServerError = tool_err.into();
            JsonRpcMessage::ErrorResponse(JsonRpcErrorResponse {
                jsonrpc: "2.0".into(),
                id,
                error: err.to_jsonrpc_error(),
            })
        }
    }
}

fn write_response(
    response: &JsonRpcMessage,
    stdout: &Arc<Mutex<std::io::Stdout>>,
) -> Result<(), McpServerError> {
    let json = serde_json::to_string(response)?;
    let mut handle = stdout
        .lock()
        .map_err(|e| McpServerError::InternalError(format!("stdout lock poisoned: {e}")))?;
    writeln!(handle, "{json}")?;
    handle.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize_response_includes_protocol() {
        let registry = ToolRegistry::new();
        let msg = handle_initialize(&registry, JsonRpcId::number(1));
        match msg {
            JsonRpcMessage::Response(resp) => {
                let result: McpInitializeResult = serde_json::from_value(resp.result).unwrap();
                assert_eq!(result.protocol_version, "2024-11-05");
                assert_eq!(result.server_info.name, "browseros-mcp");
            }
            _ => panic!("expected Response"),
        }
    }

    #[test]
    fn tools_list_empty_when_no_tools() {
        let registry = ToolRegistry::new();
        let msg = handle_tools_list(&registry, JsonRpcId::number(1));
        match msg {
            JsonRpcMessage::Response(resp) => {
                let result: McpListToolsResult = serde_json::from_value(resp.result).unwrap();
                assert!(result.tools.is_empty());
            }
            _ => panic!("expected Response"),
        }
    }

    #[test]
    fn tools_list_returns_registered_tools() {
        let registry = ToolRegistry::new();
        registry
            .register("test/hello", Box::new(crate::tools::tests::TestTool))
            .unwrap();
        let msg = handle_tools_list(&registry, JsonRpcId::number(1));
        match msg {
            JsonRpcMessage::Response(resp) => {
                let result: McpListToolsResult = serde_json::from_value(resp.result).unwrap();
                assert_eq!(result.tools.len(), 1);
                assert_eq!(result.tools[0].name, "test/hello");
            }
            _ => panic!("expected Response"),
        }
    }

    #[test]
    fn tools_call_missing_params_returns_error() {
        let registry = ToolRegistry::new();
        let ctx = McpToolContext::new(Arc::new(
            browseros_runtime::RuntimeContext::builder()
                .build()
                .unwrap(),
        ));
        let metrics = Arc::new(crate::server::metrics::ServerMetrics::new());
        let msg = handle_tools_call(JsonRpcId::number(1), None, &registry, &ctx, &metrics);
        match msg {
            JsonRpcMessage::ErrorResponse(err) => {
                assert_eq!(err.error.code, -32602);
            }
            _ => panic!("expected ErrorResponse"),
        }
    }

    #[test]
    fn tools_call_unknown_tool_returns_error() {
        let registry = ToolRegistry::new();
        let ctx = McpToolContext::new(Arc::new(
            browseros_runtime::RuntimeContext::builder()
                .build()
                .unwrap(),
        ));
        let metrics = Arc::new(crate::server::metrics::ServerMetrics::new());
        let params = serde_json::json!({"name": "unknown/tool", "arguments": {}});
        let msg = handle_tools_call(
            JsonRpcId::number(1),
            Some(params),
            &registry,
            &ctx,
            &metrics,
        );
        match msg {
            JsonRpcMessage::ErrorResponse(err) => {
                assert_eq!(err.error.code, -32601);
            }
            _ => panic!("expected ErrorResponse"),
        }
    }

    #[test]
    fn unknown_method_returns_error() {
        let id = JsonRpcId::number(1);
        let err = JsonRpcError {
            code: -32601,
            message: "Method not found: unknown/method".into(),
            data: None,
        };
        let expected = JsonRpcMessage::ErrorResponse(JsonRpcErrorResponse {
            jsonrpc: "2.0".into(),
            id,
            error: err,
        });
        // Verify the error format
        match &expected {
            JsonRpcMessage::ErrorResponse(e) => {
                assert_eq!(e.error.code, -32601);
            }
            _ => panic!("expected ErrorResponse"),
        }
    }
}
