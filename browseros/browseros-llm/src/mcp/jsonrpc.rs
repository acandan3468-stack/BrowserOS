use serde::{Deserialize, Serialize};

pub const JSON_RPC_VERSION: &str = "2.0";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcId {
    Number(serde_json::Number),
    String(String),
    Null,
}

impl JsonRpcId {
    pub fn number(n: u64) -> Self {
        JsonRpcId::Number(serde_json::Number::from(n))
    }

    pub fn as_u64(&self) -> Option<u64> {
        match self {
            JsonRpcId::Number(n) => n.as_u64(),
            JsonRpcId::String(s) => s.parse().ok(),
            JsonRpcId::Null => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: JsonRpcId,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: JsonRpcId,
    #[serde(default)]
    pub result: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl JsonRpcError {
    pub const PARSE_ERROR: i64 = -32700;
    pub const INVALID_REQUEST: i64 = -32600;
    pub const METHOD_NOT_FOUND: i64 = -32601;
    pub const INVALID_PARAMS: i64 = -32602;
    pub const INTERNAL_ERROR: i64 = -32603;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcErrorResponse {
    pub jsonrpc: String,
    pub id: JsonRpcId,
    pub error: JsonRpcError,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcMessage {
    Request(JsonRpcRequest),
    Response(JsonRpcResponse),
    ErrorResponse(JsonRpcErrorResponse),
    Notification(JsonRpcNotification),
}

// ────────────────────────────────────────────────────────────────────────────
// MCP-specific structures (defined here to avoid circular deps with client)
// ────────────────────────────────────────────────────────────────────────────

/// MCP tool definition from tools/list response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolDef {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

/// MCP tools/list result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpListToolsResult {
    pub tools: Vec<McpToolDef>,
}

/// MCP call_tool result content item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpContentItem {
    #[serde(rename = "type")]
    pub type_: String,
    #[serde(default)]
    pub text: String,
}

/// MCP tools/call result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpCallToolResult {
    pub content: Vec<McpContentItem>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "isError")]
    pub is_error: Option<bool>,
}

/// MCP initialize result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpInitializeResult {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    pub capabilities: serde_json::Value,
    #[serde(rename = "serverInfo")]
    pub server_info: McpServerInfo,
}

/// MCP server identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerInfo {
    pub name: String,
    pub version: String,
}

// ────────────────────────────────────────────────────────────────────────────

impl JsonRpcMessage {
    pub fn parse(json_str: &str) -> Result<Self, super::errors::McpError> {
        let msg: JsonRpcMessage = serde_json::from_str(json_str)
            .map_err(|e| super::errors::McpError::InvalidJson(format!("parse failed: {}", e)))?;
        msg.validate_version()?;
        Ok(msg)
    }

    pub fn serialize(&self) -> Result<String, super::errors::McpError> {
        serde_json::to_string(self)
            .map_err(|e| super::errors::McpError::InvalidJson(format!("serialize failed: {}", e)))
    }

    fn validate_version(&self) -> Result<(), super::errors::McpError> {
        let version = match self {
            JsonRpcMessage::Request(r) => &r.jsonrpc,
            JsonRpcMessage::Response(r) => &r.jsonrpc,
            JsonRpcMessage::ErrorResponse(e) => &e.jsonrpc,
            JsonRpcMessage::Notification(n) => &n.jsonrpc,
        };
        if version != JSON_RPC_VERSION {
            return Err(super::errors::McpError::InvalidJson(format!(
                "expected jsonrpc '{JSON_RPC_VERSION}', got '{version}'"
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jsonrpc_parse_request() {
        let json = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}"#;
        let msg = JsonRpcMessage::parse(json).unwrap();
        assert!(matches!(msg, JsonRpcMessage::Request(_)));
    }

    #[test]
    fn jsonrpc_parse_response() {
        let json = r#"{"jsonrpc":"2.0","id":1,"result":{"tools":[]}}"#;
        let msg = JsonRpcMessage::parse(json).unwrap();
        assert!(matches!(msg, JsonRpcMessage::Response(_)));
    }

    #[test]
    fn jsonrpc_parse_error_response() {
        let json =
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"Method not found"}}"#;
        let msg = JsonRpcMessage::parse(json).unwrap();
        assert!(matches!(msg, JsonRpcMessage::ErrorResponse(_)));
    }

    #[test]
    fn jsonrpc_parse_notification() {
        let json = r#"{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}"#;
        let msg = JsonRpcMessage::parse(json).unwrap();
        assert!(matches!(msg, JsonRpcMessage::Notification(_)));
    }

    #[test]
    fn jsonrpc_parse_invalid_version() {
        let json = r#"{"jsonrpc":"1.0","id":1,"method":"x"}"#;
        let err = JsonRpcMessage::parse(json).unwrap_err();
        assert!(matches!(err, crate::mcp::errors::McpError::InvalidJson(_)));
    }

    #[test]
    fn jsonrpc_parse_malformed_json() {
        let json = "not-json";
        let err = JsonRpcMessage::parse(json).unwrap_err();
        assert!(matches!(err, crate::mcp::errors::McpError::InvalidJson(_)));
    }

    #[test]
    fn jsonrpc_serialize_request() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: JsonRpcId::number(1),
            method: "test".into(),
            params: None,
        };
        let msg = JsonRpcMessage::Request(req);
        let json = msg.serialize().unwrap();
        let parsed = JsonRpcMessage::parse(&json).unwrap();
        assert!(matches!(parsed, JsonRpcMessage::Request(_)));
    }

    #[test]
    fn jsonrpc_serialize_response() {
        let resp = JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id: JsonRpcId::number(1),
            result: serde_json::json!({"ok": true}),
        };
        let msg = JsonRpcMessage::Response(resp);
        let json = msg.serialize().unwrap();
        let parsed = JsonRpcMessage::parse(&json).unwrap();
        assert!(matches!(parsed, JsonRpcMessage::Response(_)));
    }

    #[test]
    fn jsonrpc_id_number() {
        let id = JsonRpcId::number(42);
        assert_eq!(id.as_u64(), Some(42));
    }

    #[test]
    fn jsonrpc_id_string() {
        let id = JsonRpcId::String("abc".into());
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"abc\"");
    }

    #[test]
    fn jsonrpc_id_null() {
        let id = JsonRpcId::Null;
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "null");
    }

    #[test]
    fn jsonrpc_untagged_request() {
        let json = r#"{"jsonrpc":"2.0","id":1,"method":"x"}"#;
        let msg = JsonRpcMessage::parse(json).unwrap();
        match msg {
            JsonRpcMessage::Request(r) => assert_eq!(r.method, "x"),
            _ => panic!("expected Request variant"),
        }
    }
}
