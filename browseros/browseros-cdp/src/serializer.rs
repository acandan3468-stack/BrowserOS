use crate::error::{CdpError, CdpResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdpRequest {
    pub id: u64,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "sessionId")]
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdpResponse {
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<CdpErrorResponse>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "sessionId")]
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdpErrorResponse {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdpEvent {
    pub method: String,
    pub params: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "sessionId")]
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CdpMessage {
    Response(CdpResponse),
    Event(CdpEvent),
}

#[derive(Debug)]
pub struct RequestIdGenerator {
    next: AtomicU64,
}

impl RequestIdGenerator {
    pub fn new() -> Self {
        Self {
            next: AtomicU64::new(1),
        }
    }

    pub fn next(&self) -> u64 {
        self.next.fetch_add(1, Ordering::SeqCst)
    }

    pub fn reset(&self) {
        self.next.store(1, Ordering::SeqCst);
    }
}

impl Default for RequestIdGenerator {
    fn default() -> Self {
        Self::new()
    }
}

pub fn serialize_request(request: &CdpRequest) -> CdpResult<String> {
    serde_json::to_string(request).map_err(CdpError::from)
}

pub fn deserialize_message(json: &str) -> CdpResult<CdpMessage> {
    let value: Value = serde_json::from_str(json)?;

    if value.get("id").is_some() {
        let response: CdpResponse = serde_json::from_value(value)?;
        Ok(CdpMessage::Response(response))
    } else {
        let event: CdpEvent = serde_json::from_value(value)?;
        Ok(CdpMessage::Event(event))
    }
}

pub fn deserialize_result<T: serde::de::DeserializeOwned>(response: &CdpResponse) -> CdpResult<T> {
    if let Some(ref error) = response.error {
        return Err(CdpError::Protocol {
            code: error.code,
            message: error.message.clone(),
            data: error.data.as_ref().map(|d| d.to_string()),
        });
    }

    let result = response
        .result
        .as_ref()
        .ok_or(CdpError::NoResponse(response.id))?;

    serde_json::from_value(result.clone())
        .map_err(|e| CdpError::Serialization(format!("Failed to deserialize CDP result: {e}")))
}

pub fn parse_command_result<T: serde::de::DeserializeOwned>(json: &str) -> CdpResult<T> {
    let message = deserialize_message(json)?;
    match message {
        CdpMessage::Response(response) => deserialize_result(&response),
        CdpMessage::Event(_) => Err(CdpError::Serialization(
            "Expected response, got event".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_serialization() {
        let req = CdpRequest {
            id: 1,
            method: "Page.navigate".into(),
            params: Some(serde_json::json!({"url": "https://example.com"})),
            session_id: None,
        };
        let json = serialize_request(&req).unwrap();
        assert!(json.contains("\"id\":1"));
        assert!(json.contains("\"method\":\"Page.navigate\""));
        assert!(json.contains("\"url\":\"https://example.com\""));
    }

    #[test]
    fn test_request_no_params() {
        let req = CdpRequest {
            id: 1,
            method: "Page.enable".into(),
            params: None,
            session_id: None,
        };
        let json = serialize_request(&req).unwrap();
        assert!(!json.contains("params"));
    }

    #[test]
    fn test_response_deserialization() {
        let json = r#"{"id":1,"result":{"success":true}}"#;
        let msg = deserialize_message(json).unwrap();
        match msg {
            CdpMessage::Response(resp) => {
                assert_eq!(resp.id, 1);
                assert!(resp.error.is_none());
                let result = resp.result.unwrap();
                assert_eq!(result["success"], true);
            }
            CdpMessage::Event(_) => panic!("Expected response"),
        }
    }

    #[test]
    fn test_event_deserialization() {
        let json = r#"{"method":"Page.frameStartedLoading","params":{"frameId":"123"}}"#;
        let msg = deserialize_message(json).unwrap();
        match msg {
            CdpMessage::Event(evt) => {
                assert_eq!(evt.method, "Page.frameStartedLoading");
                let params = evt.params.unwrap();
                assert_eq!(params["frameId"], "123");
            }
            CdpMessage::Response(_) => panic!("Expected event"),
        }
    }

    #[test]
    fn test_error_response() {
        let json = r#"{"id":1,"error":{"code":-32000,"message":"Not implemented"}}"#;
        let msg = deserialize_message(json).unwrap();
        match msg {
            CdpMessage::Response(resp) => {
                let err = resp.error.unwrap();
                assert_eq!(err.code, -32000);
                assert_eq!(err.message, "Not implemented");
            }
            CdpMessage::Event(_) => panic!("Expected response"),
        }
    }

    #[test]
    fn test_deserialize_result_success() {
        let json = r#"{"id":1,"result":{"value":"ok"}}"#;
        let msg = deserialize_message(json).unwrap();
        match msg {
            CdpMessage::Response(resp) => {
                let result: Value = deserialize_result(&resp).unwrap();
                assert_eq!(result["value"], "ok");
            }
            CdpMessage::Event(_) => panic!("Expected response"),
        }
    }

    #[test]
    fn test_deserialize_result_error() {
        let json = r#"{"id":1,"error":{"code":-32601,"message":"Method not found"}}"#;
        let msg = deserialize_message(json).unwrap();
        match msg {
            CdpMessage::Response(resp) => {
                let result: Result<Value, _> = deserialize_result(&resp);
                assert!(result.is_err());
                match result.unwrap_err() {
                    CdpError::Protocol { code, .. } => assert_eq!(code, -32601),
                    _ => panic!("Expected protocol error"),
                }
            }
            CdpMessage::Event(_) => panic!("Expected response"),
        }
    }

    #[test]
    fn test_request_id_generator() {
        let gen = RequestIdGenerator::new();
        assert_eq!(gen.next(), 1);
        assert_eq!(gen.next(), 2);
        assert_eq!(gen.next(), 3);
    }

    #[test]
    fn test_request_id_reset() {
        let gen = RequestIdGenerator::new();
        gen.next();
        gen.next();
        gen.reset();
        assert_eq!(gen.next(), 1);
    }

    #[test]
    fn test_parse_command_result() {
        let json = r#"{"id":1,"result":{"value":42}}"#;
        let result: serde_json::Value = parse_command_result(json).unwrap();
        assert_eq!(result["value"], 42);
    }

    #[test]
    fn test_event_with_session_id() {
        let json =
            r#"{"method":"Runtime.consoleAPICalled","params":{"type":"log"},"sessionId":"S1"}"#;
        let msg = deserialize_message(json).unwrap();
        match msg {
            CdpMessage::Event(evt) => {
                assert_eq!(evt.method, "Runtime.consoleAPICalled");
                assert_eq!(evt.session_id.as_deref(), Some("S1"));
            }
            CdpMessage::Response(_) => panic!("Expected event"),
        }
    }

    #[test]
    fn test_invalid_json_returns_error() {
        let result = deserialize_message("not json");
        assert!(result.is_err());
    }

    #[test]
    fn test_request_id_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<RequestIdGenerator>();
    }
}
