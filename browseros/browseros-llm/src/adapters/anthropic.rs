use crate::error::LlmError;
use crate::provider::LlProvider;
use crate::types::{
    LlFinishReason, LlTool, LlToolCallDelta, ProviderCapability, ProviderEmbedRequest,
    ProviderEmbedResponse, ProviderHealthResult, ProviderMessage, ProviderRequest,
    ProviderResponse, ProviderStream, ProviderStreamEvent,
};
use std::time::SystemTime;

pub struct AnthropicAdapter {
    config: crate::types::ProviderConfig,
    agent: ureq::Agent,
}

impl AnthropicAdapter {
    pub fn new(config: crate::types::ProviderConfig) -> Self {
        let agent = super::http::build_agent(&config);
        Self { config, agent }
    }

    fn api_url(&self) -> String {
        if !self.config.api_url.is_empty() {
            self.config.api_url.trim_end_matches('/').to_string()
        } else {
            "https://api.anthropic.com".to_string()
        }
    }

    fn messages_url(&self) -> String {
        format!("{}/v1/messages", self.api_url())
    }

    fn build_headers(&self, request: ureq::Request) -> ureq::Request {
        let req = request.set("Content-Type", "application/json");
        let req = super::http::apply_headers(req, &self.config);
        // If apply_headers didn't set x-api-key (because key doesn't start with sk-),
        // we still need it for Anthropic
        if let Some(key) = &self.config.api_key {
            req.set("x-api-key", key)
                .set("anthropic-version", "2023-06-01")
        } else {
            req
        }
    }

    fn send_request(&self, body: &serde_json::Value) -> Result<String, LlmError> {
        let request = self
            .agent
            .post(&self.messages_url())
            .set("Content-Type", "application/json");
        let request = self.build_headers(request);
        let resp = request.send_json(body);
        super::http::handle_response(resp)
    }

    fn send_stream_request(
        &self,
        body: &serde_json::Value,
    ) -> Result<Box<dyn std::io::Read + Send>, LlmError> {
        let request = self
            .agent
            .post(&self.messages_url())
            .set("Content-Type", "application/json");
        let request = self.build_headers(request);
        match request.send_json(body) {
            Ok(response) => {
                let reader: Box<dyn std::io::Read + Send> = Box::new(response.into_reader());
                Ok(reader)
            }
            Err(err) => Err(super::http::map_ureq_error(err)),
        }
    }

    fn build_chat_body(&self, request: &ProviderRequest) -> serde_json::Value {
        let mut messages: Vec<serde_json::Value> = Vec::new();

        // Anthropic uses a separate system parameter
        let mut system = request.system_prompt.clone();

        for msg in &request.messages {
            match msg.role {
                crate::types::LlRole::System => {
                    // Accumulate system messages
                    if let crate::types::LlContent::Text(ref t) = msg.content {
                        let s = system.get_or_insert_with(String::new);
                        if !s.is_empty() {
                            s.push('\n');
                        }
                        s.push_str(t);
                    }
                }
                crate::types::LlRole::User => {
                    let content = match &msg.content {
                        crate::types::LlContent::Text(text) => {
                            serde_json::json!([{"type": "text", "text": text}])
                        }
                        crate::types::LlContent::Image { mime_type, data } => {
                            let b64 = crate::adapters::openai::base64_encode(data);
                            serde_json::json!([{
                                "type": "image",
                                "source": {
                                    "type": "base64",
                                    "media_type": mime_type,
                                    "data": b64
                                }
                            }])
                        }
                        crate::types::LlContent::ToolResult { call_id, output } => {
                            serde_json::json!([{
                                "type": "tool_result",
                                "tool_use_id": call_id,
                                "content": output
                            }])
                        }
                        crate::types::LlContent::ToolCall { .. } => continue,
                    };
                    messages.push(serde_json::json!({
                        "role": "user",
                        "content": content
                    }));
                }
                crate::types::LlRole::Assistant => {
                    let mut content_parts: Vec<serde_json::Value> = Vec::new();
                    match &msg.content {
                        crate::types::LlContent::Text(text) => {
                            content_parts.push(serde_json::json!({"type": "text", "text": text}));
                        }
                        crate::types::LlContent::ToolCall {
                            call_id,
                            name,
                            arguments,
                        } => {
                            content_parts.push(serde_json::json!({
                                "type": "tool_use",
                                "id": call_id,
                                "name": name,
                                "input": arguments
                            }));
                        }
                        _ => {}
                    }
                    messages.push(serde_json::json!({
                        "role": "assistant",
                        "content": content_parts
                    }));
                }
                crate::types::LlRole::Tool => {
                    // Already handled as ToolResult content above
                    continue;
                }
            }
        }

        let mut body = serde_json::json!({
            "model": request.model,
            "messages": messages,
            "max_tokens": request.max_tokens.unwrap_or(1024),
        });

        if let Some(ref s) = system {
            body["system"] = serde_json::json!(s);
        }
        if let Some(temp) = request.temperature {
            body["temperature"] = serde_json::json!(temp);
        }
        if !request.stop_sequences.is_empty() {
            body["stop_sequences"] = serde_json::json!(request.stop_sequences);
        }
        if !request.tools.is_empty() {
            body["tools"] = serde_json::json!(build_anthropic_tools(&request.tools));
        }
        if request.stream {
            body["stream"] = serde_json::json!(true);
        }

        body
    }

    fn parse_chat_response(&self, body: &str) -> Result<ProviderResponse, LlmError> {
        let val: serde_json::Value =
            serde_json::from_str(body).map_err(|e| LlmError::MalformedResponse(e.to_string()))?;

        let content_blocks = val["content"]
            .as_array()
            .ok_or_else(|| LlmError::MalformedResponse("missing content".into()))?;

        let mut text_content = String::new();
        let mut tool_call_content: Option<(
            String,
            String,
            std::collections::HashMap<String, serde_json::Value>,
        )> = None;

        for block in content_blocks {
            match block["type"].as_str() {
                Some("text") => {
                    if let Some(t) = block["text"].as_str() {
                        text_content.push_str(t);
                    }
                }
                Some("tool_use") => {
                    let call_id = block["id"].as_str().unwrap_or("").to_string();
                    let name = block["name"].as_str().unwrap_or("").to_string();
                    let input = block["input"].as_object().cloned().unwrap_or_default();
                    let arguments: std::collections::HashMap<String, serde_json::Value> =
                        input.into_iter().collect();
                    tool_call_content = Some((call_id, name, arguments));
                }
                _ => {}
            }
        }

        let provider_content = if let Some((call_id, name, arguments)) = tool_call_content {
            crate::types::LlContent::ToolCall {
                call_id,
                name,
                arguments,
            }
        } else {
            crate::types::LlContent::Text(text_content)
        };

        let stop_reason = val["stop_reason"].as_str().unwrap_or("end_turn");
        let finish_reason = match stop_reason {
            "end_turn" | "stop_sequence" => LlFinishReason::Stop,
            "max_tokens" => LlFinishReason::Length,
            "tool_use" => LlFinishReason::ToolCalls,
            _ => LlFinishReason::Error,
        };

        let input_tokens = val["usage"]["input_tokens"].as_u64().unwrap_or(0);
        let output_tokens = val["usage"]["output_tokens"].as_u64().unwrap_or(0);
        let model = val["model"].as_str().unwrap_or("").to_string();

        Ok(ProviderResponse {
            message: ProviderMessage {
                role: crate::types::LlRole::Assistant,
                content: provider_content,
            },
            finish_reason,
            input_tokens,
            output_tokens,
            model,
        })
    }

    fn parse_chunk(data: &str) -> Result<Option<ProviderStreamEvent>, LlmError> {
        // Anthropic SSE events: event: message_start, content_block_start, content_block_delta,
        // content_block_stop, message_delta, message_stop, ping
        // Each event has a data: {...} line with a "type" field
        let val: serde_json::Value =
            serde_json::from_str(data).map_err(|e| LlmError::MalformedResponse(e.to_string()))?;

        let event_type = val["type"].as_str().unwrap_or("");

        match event_type {
            "content_block_delta" => {
                let delta = val["delta"].as_object();
                let text = delta
                    .and_then(|d| d.get("text"))
                    .and_then(|t| t.as_str())
                    .unwrap_or("");
                Ok(Some(ProviderStreamEvent::Chunk {
                    content: text.to_string(),
                    finish_reason: None,
                    tool_calls: vec![],
                }))
            }
            "message_delta" => {
                let input_tokens = val["usage"]["input_tokens"].as_u64().unwrap_or(0);
                let output_tokens = val["usage"]["output_tokens"].as_u64().unwrap_or(0);
                Ok(Some(ProviderStreamEvent::Done {
                    input_tokens,
                    output_tokens,
                }))
            }
            "message_start" => {
                // Just acknowledge, no content yet
                Ok(None)
            }
            "content_block_start" => {
                // Check for tool_use content block
                let block = val["content_block"].as_object();
                let block_type = block.and_then(|b| b.get("type")).and_then(|t| t.as_str());
                if block_type == Some("tool_use") {
                    let block = block.unwrap();
                    let id = block["id"].as_str().unwrap_or("").to_string();
                    let name = block["name"].as_str().unwrap_or("").to_string();
                    let input_str =
                        serde_json::to_string(&block["input"]).unwrap_or_else(|_| "{}".to_string());
                    let tool_calls = vec![LlToolCallDelta {
                        index: val["index"].as_u64().unwrap_or(0) as usize,
                        id: Some(id),
                        name: Some(name),
                        arguments: Some(input_str),
                    }];
                    Ok(Some(ProviderStreamEvent::Chunk {
                        content: String::new(),
                        finish_reason: None,
                        tool_calls,
                    }))
                } else {
                    // Text block start — content arrives via deltas
                    Ok(None)
                }
            }
            "content_block_stop" => {
                // Block finished, no additional content
                Ok(None)
            }
            "message_stop" => {
                // Message complete — usage already sent in message_delta
                Ok(None)
            }
            "ping" => Ok(None),
            _ => {
                // Unknown event type — ignore
                Ok(None)
            }
        }
    }
}

fn build_anthropic_tools(tools: &[LlTool]) -> Vec<serde_json::Value> {
    tools
        .iter()
        .map(|t| {
            serde_json::json!({
                "name": t.name,
                "description": t.description,
                "input_schema": t.parameters,
            })
        })
        .collect()
}

impl LlProvider for AnthropicAdapter {
    fn id(&self) -> &str {
        &self.config.provider_id
    }

    fn name(&self) -> &str {
        "Anthropic"
    }

    fn capabilities(&self) -> Vec<ProviderCapability> {
        vec![
            ProviderCapability::Chat,
            ProviderCapability::Streaming,
            ProviderCapability::ToolUse,
            ProviderCapability::SystemPrompt,
        ]
    }

    fn models(&self) -> Vec<String> {
        self.config.models.clone()
    }

    fn chat(&self, request: ProviderRequest) -> Result<ProviderResponse, LlmError> {
        let body = self.build_chat_body(&request);
        let response_body = self.send_request(&body)?;
        self.parse_chat_response(&response_body)
    }

    fn chat_stream(&self, request: ProviderRequest) -> Result<ProviderStream, LlmError> {
        let body = self.build_chat_body(&request);
        let mut reader = self.send_stream_request(&body)?;

        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut buf = String::new();
            let mut reader_ref: &mut dyn std::io::Read = &mut *reader;
            loop {
                buf.clear();
                match std::io::Read::read_to_string(&mut reader_ref, &mut buf) {
                    Ok(0) => break,
                    Ok(_) => {
                        for line in buf.lines() {
                            if let Some(data) = super::http::parse_sse_line(line) {
                                match Self::parse_chunk(&data) {
                                    Ok(Some(event)) => {
                                        if tx.send(event).is_err() {
                                            return;
                                        }
                                    }
                                    Ok(None) => {}
                                    Err(e) => {
                                        let _ = tx.send(ProviderStreamEvent::Error(e));
                                        return;
                                    }
                                }
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
            let _ = tx.send(ProviderStreamEvent::Done {
                input_tokens: 0,
                output_tokens: 0,
            });
        });

        Ok(ProviderStream { receiver: rx })
    }

    fn embed(&self, _request: ProviderEmbedRequest) -> Result<ProviderEmbedResponse, LlmError> {
        Err(LlmError::ConfigurationError(
            "Anthropic does not support embeddings".into(),
        ))
    }

    fn health(&self) -> ProviderHealthResult {
        let start = std::time::Instant::now();
        match self.send_request(&serde_json::json!({
            "model": self.config.default_model.as_deref().unwrap_or("claude-3-opus-20240229"),
            "messages": [{"role": "user", "content": [{"type": "text", "text": "hi"}]}],
            "max_tokens": 1,
        })) {
            Ok(_) => ProviderHealthResult {
                status: crate::types::ProviderHealth::Healthy,
                latency_ms: Some(start.elapsed().as_millis() as u64),
                checked_at: SystemTime::now(),
                error: None,
            },
            Err(e) => ProviderHealthResult {
                status: crate::types::ProviderHealth::Unavailable {
                    since: SystemTime::now(),
                },
                latency_ms: None,
                checked_at: SystemTime::now(),
                error: Some(e.to_string()),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{LlContent, LlRole};

    fn test_config() -> crate::types::ProviderConfig {
        crate::types::ProviderConfig {
            provider_id: "anthropic".into(),
            provider_type: "anthropic".into(),
            api_url: String::new(),
            api_key: Some("sk-ant-test".into()),
            organization_id: None,
            default_model: Some("claude-3-opus-20240229".into()),
            models: vec![
                "claude-3-opus-20240229".into(),
                "claude-3-sonnet-20240229".into(),
            ],
            timeout_secs: 30,
            max_retries: 3,
            custom_headers: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn adapter_identity() {
        let adapter = AnthropicAdapter::new(test_config());
        assert_eq!(adapter.id(), "anthropic");
        assert_eq!(adapter.name(), "Anthropic");
    }

    #[test]
    fn capabilities_include_chat() {
        let adapter = AnthropicAdapter::new(test_config());
        let caps = adapter.capabilities();
        assert!(caps.contains(&ProviderCapability::Chat));
    }

    #[test]
    fn embed_not_supported() {
        let adapter = AnthropicAdapter::new(test_config());
        let req = ProviderEmbedRequest {
            model: "claude-3".into(),
            input: vec!["hello".into()],
            timeout_ms: 30000,
        };
        let err = adapter.embed(req).unwrap_err();
        assert!(err.to_string().contains("not support"));
    }

    #[test]
    fn parse_chat_response_simple() {
        let adapter = AnthropicAdapter::new(test_config());
        let body = r#"{
            "id": "msg-1",
            "type": "message",
            "role": "assistant",
            "content": [{"type": "text", "text": "Hello!"}],
            "model": "claude-3-opus-20240229",
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 10, "output_tokens": 5}
        }"#;
        let resp = adapter.parse_chat_response(body).unwrap();
        assert_eq!(resp.input_tokens, 10);
        assert_eq!(resp.output_tokens, 5);
        assert_eq!(resp.finish_reason, LlFinishReason::Stop);
        if let LlContent::Text(ref t) = resp.message.content {
            assert_eq!(t, "Hello!");
        }
    }

    #[test]
    fn parse_chat_response_tool_use() {
        let adapter = AnthropicAdapter::new(test_config());
        let body = r#"{
            "id": "msg-1",
            "type": "message",
            "role": "assistant",
            "content": [
                {"type": "text", "text": "Let me check..."},
                {"type": "tool_use", "id": "tu-1", "name": "get_weather", "input": {"location": "NYC"}}
            ],
            "model": "claude-3-opus-20240229",
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 10, "output_tokens": 5}
        }"#;
        let resp = adapter.parse_chat_response(body).unwrap();
        assert_eq!(resp.finish_reason, LlFinishReason::ToolCalls);
        assert!(matches!(resp.message.content, LlContent::ToolCall { .. }));
    }

    #[test]
    fn parse_chunk_content_block_delta() {
        let data = r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}"#;
        let event = AnthropicAdapter::parse_chunk(data).unwrap();
        if let Some(ProviderStreamEvent::Chunk { content, .. }) = event {
            assert_eq!(content, "Hello");
        } else {
            panic!("expected Chunk");
        }
    }

    #[test]
    fn parse_chunk_message_delta() {
        let data = r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"input_tokens":10,"output_tokens":5}}"#;
        let event = AnthropicAdapter::parse_chunk(data).unwrap();
        if let Some(ProviderStreamEvent::Done {
            input_tokens,
            output_tokens,
        }) = event
        {
            assert_eq!(input_tokens, 10);
            assert_eq!(output_tokens, 5);
        } else {
            panic!("expected Done");
        }
    }

    #[test]
    fn parse_chunk_ping() {
        let data = r#"{"type":"ping"}"#;
        let event = AnthropicAdapter::parse_chunk(data).unwrap();
        assert!(event.is_none());
    }

    #[test]
    fn build_chat_body_with_system() {
        let adapter = AnthropicAdapter::new(test_config());
        let request = ProviderRequest {
            model: "claude-3".into(),
            messages: vec![ProviderMessage {
                role: LlRole::User,
                content: LlContent::Text("hi".into()),
            }],
            max_tokens: Some(100),
            temperature: Some(0.7),
            stop_sequences: vec![],
            system_prompt: Some("be helpful".into()),
            tools: vec![],
            stream: false,
            timeout_ms: 30000,
        };
        let body = adapter.build_chat_body(&request);
        assert_eq!(body["model"], "claude-3");
        assert_eq!(body["system"], "be helpful");
        assert_eq!(body["max_tokens"], 100);
    }

    #[test]
    fn build_chat_body_tools() {
        let adapter = AnthropicAdapter::new(test_config());
        let request = ProviderRequest {
            model: "claude-3".into(),
            messages: vec![],
            max_tokens: Some(1024),
            temperature: None,
            stop_sequences: vec![],
            system_prompt: None,
            tools: vec![LlTool::new("get_weather", "Get weather")],
            stream: false,
            timeout_ms: 30000,
        };
        let body = adapter.build_chat_body(&request);
        assert!(body["tools"].is_array());
        assert_eq!(body["tools"][0]["name"], "get_weather");
    }

    #[test]
    fn adapter_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<AnthropicAdapter>();
        assert_sync::<AnthropicAdapter>();
    }

    // ─── tool_calls streaming ───────────────────────────────────────────────

    #[test]
    fn parse_chunk_tool_use_start() {
        let data = r#"{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_1","name":"get_weather","input":{"location":"NYC"}}}"#;
        let event = AnthropicAdapter::parse_chunk(data).unwrap();
        if let Some(ProviderStreamEvent::Chunk {
            tool_calls,
            content,
            ..
        }) = event
        {
            assert!(content.is_empty());
            assert_eq!(tool_calls.len(), 1);
            assert_eq!(tool_calls[0].index, 0);
            assert_eq!(tool_calls[0].id.as_deref(), Some("toolu_1"));
            assert_eq!(tool_calls[0].name.as_deref(), Some("get_weather"));
            assert!(tool_calls[0].arguments.is_some());
        } else {
            panic!("expected Chunk with tool_calls");
        }
    }

    #[test]
    fn parse_chunk_normal_text_has_empty_tool_calls() {
        let data = r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}"#;
        let event = AnthropicAdapter::parse_chunk(data).unwrap();
        if let Some(ProviderStreamEvent::Chunk {
            tool_calls,
            content,
            ..
        }) = event
        {
            assert_eq!(content, "Hello");
            assert!(tool_calls.is_empty());
        } else {
            panic!("expected Chunk");
        }
    }

    #[test]
    fn parse_chunk_mixed_tool_text_sequence() {
        // Text delta first
        let data1 = r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Checking weather"}}"#;
        let event1 = AnthropicAdapter::parse_chunk(data1).unwrap();
        if let Some(ProviderStreamEvent::Chunk {
            content,
            tool_calls,
            ..
        }) = event1
        {
            assert_eq!(content, "Checking weather");
            assert!(tool_calls.is_empty());
        } else {
            panic!("expected Chunk");
        }

        // Then a tool_use block
        let data2 = r#"{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"toolu_1","name":"get_weather","input":{"location":"NYC"}}}"#;
        let event2 = AnthropicAdapter::parse_chunk(data2).unwrap();
        if let Some(ProviderStreamEvent::Chunk {
            tool_calls,
            content,
            ..
        }) = event2
        {
            assert!(content.is_empty());
            assert_eq!(tool_calls.len(), 1);
            assert_eq!(tool_calls[0].index, 1);
            assert_eq!(tool_calls[0].name.as_deref(), Some("get_weather"));
        } else {
            panic!("expected Chunk with tool_calls");
        }
    }
}
