use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use super::errors::McpError;
use super::jsonrpc::*;
use super::transport::Transport;
use crate::types::LlTool;

const DEFAULT_MCP_TIMEOUT_MS: u64 = 10_000;

pub struct McpClient {
    transport: Arc<Mutex<dyn Transport>>,
    next_id: AtomicU64,
    timeout_ms: u64,
}

impl McpClient {
    pub fn new(transport: Arc<Mutex<dyn Transport>>) -> Self {
        Self {
            transport,
            next_id: AtomicU64::new(1),
            timeout_ms: DEFAULT_MCP_TIMEOUT_MS,
        }
    }

    #[allow(dead_code)]
    pub fn set_timeout(&mut self, timeout_ms: u64) {
        self.timeout_ms = timeout_ms;
    }

    fn next_id(&self) -> JsonRpcId {
        JsonRpcId::number(self.next_id.fetch_add(1, Ordering::Relaxed))
    }

    fn send_request(&self, request: &JsonRpcRequest) -> Result<(), McpError> {
        let msg = JsonRpcMessage::Request(request.clone());
        let json = msg.serialize()?;
        if json.contains('\n') || json.contains('\r') {
            return Err(McpError::TransportError(
                "serialized request contains newline characters".into(),
            ));
        }
        let mut transport = self
            .transport
            .lock()
            .map_err(|e| McpError::InternalError(format!("transport lock poisoned: {e}")))?;
        transport.send(&json)
    }

    fn receive_response(
        &self,
        timeout_ms: u64,
        expected_id: &JsonRpcId,
    ) -> Result<JsonRpcResponse, McpError> {
        let mut transport = self
            .transport
            .lock()
            .map_err(|e| McpError::InternalError(format!("transport lock poisoned: {e}")))?;
        let line = transport.receive(timeout_ms)?;
        let msg = JsonRpcMessage::parse(&line)?;

        match msg {
            JsonRpcMessage::Response(resp) => {
                if resp.id != *expected_id {
                    return Err(McpError::TransportError(format!(
                        "response id mismatch: expected {:?}, got {:?}",
                        expected_id, resp.id
                    )));
                }
                Ok(resp)
            }
            JsonRpcMessage::ErrorResponse(err_resp) => {
                if err_resp.id != *expected_id {
                    return Err(McpError::TransportError(format!(
                        "error response id mismatch: expected {:?}, got {:?}",
                        expected_id, err_resp.id
                    )));
                }
                Err(match err_resp.error.code {
                    JsonRpcError::PARSE_ERROR => McpError::InvalidJson(err_resp.error.message),
                    JsonRpcError::INVALID_REQUEST => {
                        McpError::InvalidRequest(err_resp.error.message)
                    }
                    JsonRpcError::METHOD_NOT_FOUND => {
                        McpError::UnsupportedMethod(err_resp.error.message)
                    }
                    JsonRpcError::INVALID_PARAMS => {
                        McpError::InvalidRequest(err_resp.error.message)
                    }
                    JsonRpcError::INTERNAL_ERROR => McpError::InternalError(err_resp.error.message),
                    _ => McpError::JsonRpcError {
                        code: err_resp.error.code,
                        message: err_resp.error.message,
                    },
                })
            }
            other => Err(McpError::TransportError(format!(
                "unexpected message type: {other:?}"
            ))),
        }
    }

    fn build_request(&self, method: &str, params: Option<serde_json::Value>) -> JsonRpcRequest {
        JsonRpcRequest {
            jsonrpc: JSON_RPC_VERSION.into(),
            id: self.next_id(),
            method: method.into(),
            params,
        }
    }

    pub fn initialize(&self) -> Result<(), McpError> {
        let init_params = serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "browseros",
                "version": "0.1.0"
            }
        });

        let req = self.build_request("initialize", Some(init_params));
        let expected_id = req.id.clone();
        self.send_request(&req)?;
        let _response = self.receive_response(self.timeout_ms, &expected_id)?;

        let notif = JsonRpcNotification {
            jsonrpc: JSON_RPC_VERSION.into(),
            method: "notifications/initialized".into(),
            params: Some(serde_json::json!({})),
        };
        let notif_msg = JsonRpcMessage::Notification(notif);
        let notif_json = notif_msg.serialize()?;
        let mut transport = self
            .transport
            .lock()
            .map_err(|e| McpError::InternalError(format!("transport lock poisoned: {e}")))?;
        transport.send(&notif_json)?;

        Ok(())
    }

    pub fn list_tools(&self) -> Result<Vec<LlTool>, McpError> {
        let req = self.build_request("tools/list", Some(serde_json::json!({})));
        let expected_id = req.id.clone();
        self.send_request(&req)?;
        let response = self.receive_response(self.timeout_ms, &expected_id)?;

        let result: McpListToolsResult = serde_json::from_value(response.result)
            .map_err(|e| McpError::InvalidJson(format!("tools/list result: {e}")))?;

        Ok(result
            .tools
            .into_iter()
            .map(|t| LlTool {
                name: t.name,
                description: t.description,
                parameters: t.input_schema,
                strict: false,
            })
            .collect())
    }

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

        let req = self.build_request("tools/call", Some(params));
        let expected_id = req.id.clone();
        self.send_request(&req)?;
        let response = self.receive_response(timeout_ms, &expected_id)?;

        let result: McpCallToolResult = serde_json::from_value(response.result)
            .map_err(|e| McpError::InvalidJson(format!("tools/call result: {e}")))?;

        if result.is_error.unwrap_or(false) {
            let error_text = result
                .content
                .first()
                .map(|c| c.text.clone())
                .unwrap_or_else(|| "unknown error".into());
            return Err(McpError::ToolExecutionError(error_text));
        }

        Ok(result)
    }

    #[allow(dead_code)]
    pub fn transport(&self) -> &Arc<Mutex<dyn Transport>> {
        &self.transport
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::errors::McpError;
    use crate::mcp::transport::Transport;

    struct MockTransport {
        responses: Vec<String>,
        response_index: usize,
        send_error: Option<String>,
    }

    impl MockTransport {
        fn new(responses: Vec<&str>) -> Self {
            Self {
                responses: responses.into_iter().map(|s| s.to_string()).collect(),
                response_index: 0,
                send_error: None,
            }
        }

        fn with_send_error(responses: Vec<&str>, send_error: &str) -> Self {
            Self {
                responses: responses.into_iter().map(|s| s.to_string()).collect(),
                response_index: 0,
                send_error: Some(send_error.into()),
            }
        }
    }

    impl Transport for MockTransport {
        fn send(&mut self, _message: &str) -> Result<(), McpError> {
            if let Some(ref msg) = self.send_error {
                return Err(McpError::TransportError(msg.clone()));
            }
            Ok(())
        }

        fn receive(&mut self, _timeout_ms: u64) -> Result<String, McpError> {
            if self.response_index < self.responses.len() {
                let resp = self.responses[self.response_index].clone();
                self.response_index += 1;
                Ok(resp)
            } else {
                Err(McpError::Timeout { elapsed_ms: 100 })
            }
        }

        fn is_alive(&mut self) -> bool {
            true
        }
        fn shutdown(&mut self) {}
    }

    fn make_client(responses: Vec<&str>) -> McpClient {
        let transport: Arc<Mutex<dyn Transport>> =
            Arc::new(Mutex::new(MockTransport::new(responses)));
        McpClient::new(transport)
    }

    fn make_client_with_send_error(responses: Vec<&str>, send_error: &str) -> McpClient {
        let transport: Arc<Mutex<dyn Transport>> = Arc::new(Mutex::new(
            MockTransport::with_send_error(responses, send_error),
        ));
        let mut client = McpClient::new(transport);
        client.set_timeout(5000);
        client
    }

    fn mock_result_n(result: serde_json::Value, id: u64) -> String {
        serde_json::json!({"jsonrpc":"2.0","id":id,"result":result}).to_string()
    }

    fn mock_error_n(code: i64, message: &str, id: u64) -> String {
        serde_json::json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
            .to_string()
    }

    #[test]
    fn initialize_success() {
        let resp = mock_result_n(
            serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "test", "version": "1.0"}
            }),
            1,
        );
        let client = make_client(vec![&resp]);
        let result = client.initialize();
        assert!(result.is_ok());
    }

    #[test]
    fn initialize_accepts_any_version() {
        let resp = mock_result_n(
            serde_json::json!({
                "protocolVersion": "1.0",
                "capabilities": {},
                "serverInfo": {"name": "test", "version": "1.0"}
            }),
            1,
        );
        let client = make_client(vec![&resp]);
        let result = client.initialize();
        assert!(result.is_ok());
    }

    #[test]
    fn initialize_error_response() {
        let resp = mock_error_n(JsonRpcError::INTERNAL_ERROR, "server error", 1);
        let client = make_client(vec![&resp]);
        let result = client.initialize();
        assert!(matches!(result, Err(McpError::InternalError(_))));
    }

    #[test]
    fn initialize_invalid_response_json() {
        let client = {
            let transport: Arc<Mutex<dyn Transport>> = Arc::new(Mutex::new(MockTransport {
                responses: vec!["not-json".into()],
                response_index: 0,
                send_error: None,
            }));
            let mut c = McpClient::new(transport);
            c.set_timeout(5000);
            c
        };
        let result = client.initialize();
        assert!(matches!(result, Err(McpError::InvalidJson(_))));
    }

    #[test]
    fn initialize_timeout() {
        let client = make_client(vec![]);
        let result = client.initialize();
        assert!(matches!(result, Err(McpError::Timeout { .. })));
    }

    #[test]
    fn list_tools_parsed() {
        let resp = mock_result_n(
            serde_json::json!({
                "tools": [{"name": "echo", "description": "Echo", "inputSchema": {"type":"object","properties":{"msg":{"type":"string"}}}}]
            }),
            1,
        );
        let client = make_client(vec![&resp]);
        let result = client.list_tools();
        assert!(result.is_ok());
        let tools = result.unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "echo");
    }

    #[test]
    fn list_tools_empty() {
        let resp = mock_result_n(serde_json::json!({"tools": []}), 1);
        let client = make_client(vec![&resp]);
        let result = client.list_tools();
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn list_tools_missing_input_schema() {
        let resp = mock_result_n(
            serde_json::json!({
                "tools": [{"name": "echo", "description": "Echo"}]
            }),
            1,
        );
        let client = make_client(vec![&resp]);
        let result = client.list_tools();
        assert!(result.is_ok());
        let tools = result.unwrap();
        assert_eq!(tools.len(), 1);
        assert!(tools[0].parameters.is_object() || tools[0].parameters.is_null());
    }

    #[test]
    fn list_tools_method_not_found() {
        let resp = mock_error_n(
            JsonRpcError::METHOD_NOT_FOUND,
            "Method not found: tools/list",
            1,
        );
        let client = make_client(vec![&resp]);
        let result = client.list_tools();
        assert!(matches!(result, Err(McpError::UnsupportedMethod(_))));
    }

    #[test]
    fn list_tools_invalid_params() {
        let resp = mock_error_n(JsonRpcError::INVALID_PARAMS, "bad params", 1);
        let client = make_client(vec![&resp]);
        let result = client.list_tools();
        assert!(matches!(result, Err(McpError::InvalidRequest(_))));
    }

    #[test]
    fn call_tool_success() {
        let resp = mock_result_n(
            serde_json::json!({
                "content": [{"type": "text", "text": "hello"}]
            }),
            1,
        );
        let client = make_client(vec![&resp]);
        let result = client.call_tool("echo", serde_json::json!({"msg": "hi"}), 5000);
        assert!(result.is_ok());
        let call_result = result.unwrap();
        assert_eq!(call_result.content.len(), 1);
    }

    #[test]
    fn call_tool_is_error() {
        let resp = mock_result_n(
            serde_json::json!({
                "content": [{"type":"text","text":"error occurred"}],
                "isError": true
            }),
            1,
        );
        let client = make_client(vec![&resp]);
        let result = client.call_tool("fail", serde_json::json!({}), 5000);
        assert!(matches!(result, Err(McpError::ToolExecutionError(_))));
    }

    #[test]
    fn call_tool_missing_content() {
        let resp = mock_result_n(serde_json::json!({"content": []}), 1);
        let client = make_client(vec![&resp]);
        let result = client.call_tool("empty", serde_json::json!({}), 5000);
        assert!(result.is_ok());
        assert!(result.unwrap().content.is_empty());
    }

    #[test]
    fn call_tool_timeout() {
        let client = make_client(vec![]);
        let result = client.call_tool("timeout", serde_json::json!({}), 50);
        assert!(matches!(result, Err(McpError::Timeout { .. })));
    }

    #[test]
    fn call_tool_method_not_found() {
        let resp = mock_error_n(
            JsonRpcError::METHOD_NOT_FOUND,
            "Method not found: tools/call",
            1,
        );
        let client = make_client(vec![&resp]);
        let result = client.call_tool("nonexistent", serde_json::json!({}), 5000);
        assert!(matches!(result, Err(McpError::UnsupportedMethod(_))));
    }

    #[test]
    fn call_tool_parse_error_response() {
        let resp = mock_error_n(JsonRpcError::PARSE_ERROR, "Parse error", 1);
        let client = make_client(vec![&resp]);
        let result = client.call_tool("x", serde_json::json!({}), 5000);
        assert!(matches!(result, Err(McpError::InvalidJson(_))));
    }

    #[test]
    fn call_tool_custom_error_code() {
        let resp = mock_error_n(-32000, "Custom server error", 1);
        let client = make_client(vec![&resp]);
        let result = client.call_tool("x", serde_json::json!({}), 5000);
        match result {
            Err(McpError::JsonRpcError { code, .. }) => assert_eq!(code, -32000),
            _ => panic!("expected JsonRpcError with code -32000, got: {result:?}"),
        }
    }

    #[test]
    fn response_id_mismatch() {
        // Returns id=2 but client expects id=1
        let resp = mock_result_n(serde_json::json!({"tools": []}), 2);
        let client = make_client(vec![&resp]);
        let result = client.list_tools();
        assert!(
            matches!(result, Err(McpError::TransportError(msg)) if msg.contains("id mismatch"))
        );
    }

    #[test]
    fn error_response_id_mismatch() {
        let resp = mock_error_n(JsonRpcError::METHOD_NOT_FOUND, "nope", 2);
        let client = make_client(vec![&resp]);
        let result = client.list_tools();
        assert!(
            matches!(result, Err(McpError::TransportError(msg)) if msg.contains("id mismatch"))
        );
    }

    #[test]
    fn transport_send_error() {
        let client = make_client_with_send_error(vec![], "broken pipe");
        let result = client.list_tools();
        assert!(
            matches!(result, Err(McpError::TransportError(msg)) if msg.contains("broken pipe"))
        );
    }

    #[test]
    fn unexpected_message_type() {
        // Send a notification instead of a response — not parseable as Response or ErrorResponse
        let resp = r#"{"jsonrpc":"2.0","method":"some_event","params":{}}"#;
        let client = make_client(vec![resp]);
        let result = client.list_tools();
        assert!(
            matches!(result, Err(McpError::TransportError(msg)) if msg.contains("unexpected message type"))
        );
    }

    #[test]
    fn initialize_transport_send_error() {
        let client = make_client_with_send_error(vec![], "stdin closed");
        let result = client.initialize();
        assert!(
            matches!(result, Err(McpError::TransportError(msg)) if msg.contains("stdin closed"))
        );
    }

    #[test]
    fn sequential_initialize_list_call() {
        let init_resp = mock_result_n(
            serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "test", "version": "1.0"}
            }),
            1,
        );
        let list_resp = mock_result_n(
            serde_json::json!({
                "tools": [{"name": "echo", "description": "Echo", "inputSchema": {"type":"object"}}]
            }),
            2,
        );
        let call_resp = mock_result_n(
            serde_json::json!({
                "content": [{"type":"text","text":"done"}]
            }),
            3,
        );
        let client = make_client(vec![&init_resp, &list_resp, &call_resp]);
        client.initialize().expect("init failed");
        let tools = client.list_tools().expect("list_tools failed");
        assert_eq!(tools.len(), 1);
        let result = client
            .call_tool("echo", serde_json::json!({"x":1}), 5000)
            .expect("call_tool failed");
        assert_eq!(result.content[0].text, "done");
    }

    #[test]
    fn set_timeout_configurable() {
        let transport: Arc<Mutex<dyn Transport>> = Arc::new(Mutex::new(MockTransport::new(vec![])));
        let mut client = McpClient::new(transport);
        assert_eq!(client.timeout_ms, DEFAULT_MCP_TIMEOUT_MS);
        client.set_timeout(42);
        assert_eq!(client.timeout_ms, 42);
    }
}
