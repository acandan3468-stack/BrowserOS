use crate::error::LlmError;
use crate::provider::LlProvider;
use crate::types::{
    LlFinishReason, LlTool, LlToolCallDelta, ProviderCapability, ProviderEmbedRequest,
    ProviderEmbedResponse, ProviderHealthResult, ProviderMessage, ProviderRequest,
    ProviderResponse, ProviderStream, ProviderStreamEvent,
};
use std::collections::HashMap;
use std::time::SystemTime;

/// A fully configuration-driven HTTP provider adapter.
///
/// Everything is driven by `ProviderConfig`:
/// - URLs, paths, headers, authentication
/// - Request templates (JSON body construction)
/// - Response field mappings
///
/// This adapter uses convention-over-configuration with sensible defaults
/// for common API patterns while allowing full customization via config.
pub struct GenericHttpAdapter {
    config: crate::types::ProviderConfig,
    agent: ureq::Agent,
}

impl GenericHttpAdapter {
    pub fn new(config: crate::types::ProviderConfig) -> Self {
        let agent = super::http::build_agent(&config);
        Self { config, agent }
    }

    fn api_url(&self) -> String {
        if !self.config.api_url.is_empty() {
            self.config.api_url.trim_end_matches('/').to_string()
        } else {
            "http://localhost".to_string()
        }
    }

    fn chat_url(&self) -> String {
        // Convention: /v1/chat/completions
        format!("{}/v1/chat/completions", self.api_url())
    }

    fn embed_url(&self) -> String {
        // Convention: /v1/embeddings
        format!("{}/v1/embeddings", self.api_url())
    }

    fn health_url(&self) -> String {
        // Convention: /health or /
        format!("{}/health", self.api_url())
    }

    fn build_chat_body(&self, request: &ProviderRequest) -> serde_json::Value {
        // Generic OpenAI-compatible request format
        let messages: Vec<serde_json::Value> = request
            .messages
            .iter()
            .map(|msg| {
                let role = msg.role.as_str();
                let content = match &msg.content {
                    crate::types::LlContent::Text(text) => serde_json::json!(text),
                    crate::types::LlContent::ToolResult { call_id, output } => {
                        serde_json::json!({
                            "role": "tool",
                            "tool_call_id": call_id,
                            "content": output
                        })
                    }
                    crate::types::LlContent::ToolCall { call_id, name, arguments } => {
                        return serde_json::json!({
                            "role": "assistant",
                            "content": null,
                            "tool_calls": [{
                                "id": call_id,
                                "type": "function",
                                "function": {
                                    "name": name,
                                    "arguments": serde_json::to_string(arguments).unwrap_or_default()
                                }
                            }]
                        });
                    }
                    crate::types::LlContent::Image { .. } => serde_json::json!("[image]"),
                };
                serde_json::json!({
                    "role": role,
                    "content": content
                })
            })
            .collect();

        let mut body = serde_json::json!({
            "model": request.model,
            "messages": messages,
        });

        if let Some(max_tokens) = request.max_tokens {
            body["max_tokens"] = serde_json::json!(max_tokens);
        }
        if let Some(temp) = request.temperature {
            body["temperature"] = serde_json::json!(temp);
        }
        if !request.stop_sequences.is_empty() {
            body["stop"] = serde_json::json!(request.stop_sequences);
        }
        if let Some(ref system) = request.system_prompt {
            body["system"] = serde_json::json!(system);
        }
        if !request.tools.is_empty() {
            body["tools"] = serde_json::json!(build_generic_tools(&request.tools));
        }
        if request.stream {
            body["stream"] = serde_json::json!(true);
        }

        body
    }

    fn parse_chat_response(&self, body: &str) -> Result<ProviderResponse, LlmError> {
        let val: serde_json::Value =
            serde_json::from_str(body).map_err(|e| LlmError::MalformedResponse(e.to_string()))?;

        let choice = val["choices"][0]
            .as_object()
            .ok_or_else(|| LlmError::MalformedResponse("missing choices[0]".into()))?;

        let message = choice
            .get("message")
            .and_then(|m| m.as_object())
            .ok_or_else(|| LlmError::MalformedResponse("missing message".into()))?;

        let content = message
            .get("content")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();

        // Tool calls
        let tool_call_content = message
            .get("tool_calls")
            .and_then(|tcs| tcs.as_array())
            .and_then(|tcs| {
                tcs.first().map(|tc| {
                    let call_id = tc["id"].as_str().unwrap_or("").to_string();
                    let name = tc["function"]["name"].as_str().unwrap_or("").to_string();
                    let args_str = tc["function"]["arguments"].as_str().unwrap_or("{}");
                    let arguments: HashMap<String, serde_json::Value> =
                        serde_json::from_str(args_str).unwrap_or_default();
                    (call_id, name, arguments)
                })
            });

        let finish_reason = choice
            .get("finish_reason")
            .and_then(|f| f.as_str())
            .map(|r| match r {
                "stop" => LlFinishReason::Stop,
                "length" | "max_tokens" => LlFinishReason::Length,
                "tool_calls" | "function_call" => LlFinishReason::ToolCalls,
                "content_filter" => LlFinishReason::ContentFilter,
                _ => LlFinishReason::Error,
            })
            .unwrap_or(LlFinishReason::Stop);

        let input_tokens = val["usage"]["prompt_tokens"]
            .as_u64()
            .or_else(|| val["usage"]["input_tokens"].as_u64())
            .unwrap_or(0);
        let output_tokens = val["usage"]["completion_tokens"]
            .as_u64()
            .or_else(|| val["usage"]["output_tokens"].as_u64())
            .unwrap_or(0);
        let model = val["model"].as_str().unwrap_or("").to_string();

        let provider_content = if let Some((call_id, name, arguments)) = tool_call_content {
            crate::types::LlContent::ToolCall {
                call_id,
                name,
                arguments,
            }
        } else {
            crate::types::LlContent::Text(content)
        };

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

    fn parse_chunk(data: &str) -> Result<ProviderStreamEvent, LlmError> {
        let val: serde_json::Value =
            serde_json::from_str(data).map_err(|e| LlmError::MalformedResponse(e.to_string()))?;

        let choices = val["choices"].as_array();
        if choices.is_none() || choices.map(|c| c.is_empty()).unwrap_or(true) {
            // Check for usage in the final chunk
            if val.get("usage").is_some() {
                let input_tokens = val["usage"]["prompt_tokens"].as_u64().unwrap_or(0);
                let output_tokens = val["usage"]["completion_tokens"].as_u64().unwrap_or(0);
                return Ok(ProviderStreamEvent::Done {
                    input_tokens,
                    output_tokens,
                });
            }
            return Ok(ProviderStreamEvent::Chunk {
                content: String::new(),
                finish_reason: None,
                tool_calls: vec![],
            });
        }

        let choice = &choices.unwrap()[0];
        let delta = choice["delta"].as_object();
        let content = delta
            .and_then(|d| d.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();

        let tool_calls = delta
            .and_then(|d| d.get("tool_calls"))
            .and_then(|t| t.as_array())
            .map(|tc_array| {
                tc_array
                    .iter()
                    .map(|tc| {
                        let idx = tc["index"].as_u64().unwrap_or(0) as usize;
                        let id = tc["id"].as_str().map(|s| s.to_string());
                        let name = tc["function"]["name"].as_str().map(|s| s.to_string());
                        let args = tc["function"]["arguments"].as_str().map(|s| s.to_string());
                        LlToolCallDelta {
                            index: idx,
                            id,
                            name,
                            arguments: args,
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let finish_reason = choice
            .get("finish_reason")
            .and_then(|f| f.as_str())
            .filter(|r| !r.is_empty())
            .map(|r| match r {
                "stop" => LlFinishReason::Stop,
                "length" | "max_tokens" => LlFinishReason::Length,
                "tool_calls" | "function_call" => LlFinishReason::ToolCalls,
                "content_filter" => LlFinishReason::ContentFilter,
                _ => LlFinishReason::Error,
            });

        // If we have a terminal finish_reason with usage, emit Done
        if finish_reason.is_some() && val.get("usage").is_some() {
            let input_tokens = val["usage"]["prompt_tokens"]
                .as_u64()
                .or_else(|| val["usage"]["input_tokens"].as_u64())
                .unwrap_or(0);
            let output_tokens = val["usage"]["completion_tokens"]
                .as_u64()
                .or_else(|| val["usage"]["output_tokens"].as_u64())
                .unwrap_or(0);
            return Ok(ProviderStreamEvent::Done {
                input_tokens,
                output_tokens,
            });
        }

        Ok(ProviderStreamEvent::Chunk {
            content,
            finish_reason,
            tool_calls,
        })
    }

    fn parse_embed_response(&self, body: &str) -> Result<ProviderEmbedResponse, LlmError> {
        let val: serde_json::Value =
            serde_json::from_str(body).map_err(|e| LlmError::MalformedResponse(e.to_string()))?;

        let mut embeddings = Vec::new();
        if let Some(data) = val["data"].as_array() {
            for item in data {
                if let Some(embedding) = item["embedding"].as_array() {
                    let vec: Vec<f32> = embedding
                        .iter()
                        .filter_map(|v| v.as_f64().map(|f| f as f32))
                        .collect();
                    embeddings.push(vec);
                }
            }
        }

        let input_tokens = val["usage"]["prompt_tokens"]
            .as_u64()
            .or_else(|| val["usage"]["input_tokens"].as_u64())
            .unwrap_or(0);
        let model = val["model"].as_str().unwrap_or("").to_string();

        Ok(ProviderEmbedResponse {
            embeddings,
            input_tokens,
            model,
        })
    }
}

fn build_generic_tools(tools: &[LlTool]) -> Vec<serde_json::Value> {
    tools
        .iter()
        .map(|t| {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters,
                }
            })
        })
        .collect()
}

impl LlProvider for GenericHttpAdapter {
    fn id(&self) -> &str {
        &self.config.provider_id
    }

    fn name(&self) -> &str {
        &self.config.provider_id
    }

    fn capabilities(&self) -> Vec<ProviderCapability> {
        vec![
            ProviderCapability::Chat,
            ProviderCapability::Streaming,
            ProviderCapability::Embedding,
            ProviderCapability::ToolUse,
            ProviderCapability::SystemPrompt,
        ]
    }

    fn models(&self) -> Vec<String> {
        self.config.models.clone()
    }

    fn chat(&self, request: ProviderRequest) -> Result<ProviderResponse, LlmError> {
        let body = self.build_chat_body(&request);
        let body = super::http::send_json_post(&self.agent, &self.chat_url(), &self.config, &body)?;
        self.parse_chat_response(&body)
    }

    fn chat_stream(&self, request: ProviderRequest) -> Result<ProviderStream, LlmError> {
        let body = self.build_chat_body(&request);
        let reader =
            super::http::send_json_post_stream(&self.agent, &self.chat_url(), &self.config, &body)?;

        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut reader = std::io::BufReader::new(reader);
            loop {
                match super::http::read_sse_event(&mut reader) {
                    Some(data) => match Self::parse_chunk(&data) {
                        Ok(ProviderStreamEvent::Done {
                            input_tokens,
                            output_tokens,
                        }) => {
                            let _ = tx.send(ProviderStreamEvent::Done {
                                input_tokens,
                                output_tokens,
                            });
                            break;
                        }
                        Ok(event) => {
                            if tx.send(event).is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(ProviderStreamEvent::Error(e));
                            break;
                        }
                    },
                    None => {
                        let _ = tx.send(ProviderStreamEvent::Done {
                            input_tokens: 0,
                            output_tokens: 0,
                        });
                        break;
                    }
                }
            }
        });

        Ok(ProviderStream { receiver: rx })
    }

    fn embed(&self, request: ProviderEmbedRequest) -> Result<ProviderEmbedResponse, LlmError> {
        let body = serde_json::json!({
            "model": request.model,
            "input": request.input,
        });
        let body =
            super::http::send_json_post(&self.agent, &self.embed_url(), &self.config, &body)?;
        self.parse_embed_response(&body)
    }

    fn health(&self) -> ProviderHealthResult {
        let start = std::time::Instant::now();
        match super::http::send_get(&self.agent, &self.health_url(), &self.config) {
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
            provider_id: "custom".into(),
            provider_type: "generic".into(),
            api_url: "http://localhost:8080".into(),
            api_key: None,
            organization_id: None,
            default_model: Some("default".into()),
            models: vec!["default".into()],
            timeout_secs: 30,
            max_retries: 3,
            custom_headers: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn adapter_identity() {
        let adapter = GenericHttpAdapter::new(test_config());
        assert_eq!(adapter.id(), "custom");
        assert_eq!(adapter.name(), "custom");
    }

    #[test]
    fn capabilities_include_chat() {
        let adapter = GenericHttpAdapter::new(test_config());
        let caps = adapter.capabilities();
        assert!(caps.contains(&ProviderCapability::Chat));
    }

    #[test]
    fn parse_chat_response_simple() {
        let adapter = GenericHttpAdapter::new(test_config());
        let body = r#"{
            "id": "1",
            "object": "chat.completion",
            "created": 123,
            "model": "default",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "Hello!"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5}
        }"#;
        let resp = adapter.parse_chat_response(body).unwrap();
        assert_eq!(resp.input_tokens, 10);
        assert_eq!(resp.output_tokens, 5);
        if let LlContent::Text(ref t) = resp.message.content {
            assert_eq!(t, "Hello!");
        }
    }

    #[test]
    fn parse_chat_response_alternative_usage() {
        let adapter = GenericHttpAdapter::new(test_config());
        let body = r#"{
            "choices": [{"index": 0, "message": {"role": "assistant", "content": "Hi"}, "finish_reason": "stop"}],
            "usage": {"input_tokens": 7, "output_tokens": 3}
        }"#;
        let resp = adapter.parse_chat_response(body).unwrap();
        assert_eq!(resp.input_tokens, 7);
        assert_eq!(resp.output_tokens, 3);
    }

    #[test]
    fn parse_chunk_content() {
        let data = r#"{"choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}"#;
        let event = GenericHttpAdapter::parse_chunk(data).unwrap();
        match event {
            ProviderStreamEvent::Chunk { content, .. } => {
                assert_eq!(content, "Hello");
            }
            _ => panic!("expected Chunk"),
        }
    }

    #[test]
    fn parse_chunk_done() {
        let data = r#"{"choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":5,"completion_tokens":10}}"#;
        let event = GenericHttpAdapter::parse_chunk(data).unwrap();
        match event {
            ProviderStreamEvent::Done {
                input_tokens,
                output_tokens,
            } => {
                assert_eq!(input_tokens, 5);
                assert_eq!(output_tokens, 10);
            }
            _ => panic!("expected Done"),
        }
    }

    #[test]
    fn parse_chunk_empty_choices() {
        let data = r#"{"choices":[]}"#;
        let event = GenericHttpAdapter::parse_chunk(data).unwrap();
        match event {
            ProviderStreamEvent::Chunk { content, .. } => {
                assert!(content.is_empty());
            }
            _ => panic!("expected Chunk"),
        }
    }

    #[test]
    fn parse_embed_response_simple() {
        let adapter = GenericHttpAdapter::new(test_config());
        let body = r#"{
            "object": "list",
            "data": [{"object": "embedding", "index": 0, "embedding": [0.1, 0.2]}],
            "model": "default",
            "usage": {"prompt_tokens": 5}
        }"#;
        let resp = adapter.parse_embed_response(body).unwrap();
        assert_eq!(resp.embeddings.len(), 1);
        assert_eq!(resp.embeddings[0].len(), 2);
    }

    #[test]
    fn build_chat_body_simple() {
        let adapter = GenericHttpAdapter::new(test_config());
        let request = ProviderRequest {
            model: "default".into(),
            messages: vec![ProviderMessage {
                role: LlRole::User,
                content: LlContent::Text("hi".into()),
            }],
            max_tokens: None,
            temperature: None,
            stop_sequences: vec![],
            system_prompt: None,
            tools: vec![],
            stream: false,
            timeout_ms: 30000,
        };
        let body = adapter.build_chat_body(&request);
        assert_eq!(body["model"], "default");
        assert_eq!(body["messages"][0]["role"], "user");
    }

    #[test]
    fn health_unavailable_when_offline() {
        let mut cfg = test_config();
        cfg.api_url = "http://127.0.0.1:1".into();
        cfg.timeout_secs = 1;
        let adapter = GenericHttpAdapter::new(cfg);
        let result = adapter.health();
        match result.status {
            crate::types::ProviderHealth::Unavailable { .. } => {}
            _ => panic!("expected Unavailable"),
        }
    }

    #[test]
    fn adapter_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<GenericHttpAdapter>();
        assert_sync::<GenericHttpAdapter>();
    }

    // ─── tool_calls streaming ───────────────────────────────────────────────

    #[test]
    fn parse_chunk_tool_call_delta() {
        let data = r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"get_weather","arguments":"{\"loc\":\"NYC\"}"}}]},"finish_reason":null}]}"#;
        let event = GenericHttpAdapter::parse_chunk(data).unwrap();
        match event {
            ProviderStreamEvent::Chunk { tool_calls, .. } => {
                assert_eq!(tool_calls.len(), 1);
                assert_eq!(tool_calls[0].index, 0);
                assert_eq!(tool_calls[0].id.as_deref(), Some("call_1"));
                assert_eq!(tool_calls[0].name.as_deref(), Some("get_weather"));
                assert!(tool_calls[0].arguments.is_some());
            }
            _ => panic!("expected Chunk"),
        }
    }

    #[test]
    fn parse_chunk_text_only_has_empty_tool_calls() {
        let data = r#"{"choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}"#;
        let event = GenericHttpAdapter::parse_chunk(data).unwrap();
        match event {
            ProviderStreamEvent::Chunk {
                tool_calls,
                content,
                ..
            } => {
                assert_eq!(content, "Hello");
                assert!(tool_calls.is_empty());
            }
            _ => panic!("expected Chunk"),
        }
    }

    #[test]
    fn parse_chunk_mixed_text_and_tool_calls() {
        let data = r#"{"choices":[{"index":0,"delta":{"content":"Let me check","tool_calls":[{"index":0,"id":"call_1","function":{"name":"get_weather","arguments":"{}"}}]},"finish_reason":null}]}"#;
        let event = GenericHttpAdapter::parse_chunk(data).unwrap();
        match event {
            ProviderStreamEvent::Chunk {
                tool_calls,
                content,
                ..
            } => {
                assert_eq!(content, "Let me check");
                assert_eq!(tool_calls.len(), 1);
                assert_eq!(tool_calls[0].name.as_deref(), Some("get_weather"));
            }
            _ => panic!("expected Chunk"),
        }
    }

    #[test]
    fn parse_chunk_malformed_json() {
        let err = GenericHttpAdapter::parse_chunk("not json");
        assert!(err.is_err());
    }

    #[test]
    fn parse_chunk_empty_tool_calls_field() {
        let data = r#"{"choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}"#;
        let event = GenericHttpAdapter::parse_chunk(data).unwrap();
        match event {
            ProviderStreamEvent::Chunk { tool_calls, .. } => {
                assert!(tool_calls.is_empty());
            }
            _ => panic!("expected Chunk"),
        }
    }

    #[test]
    fn parse_chunk_multiple_simultaneous_tool_calls() {
        let data = r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"get_weather","arguments":"{}"}},{"index":1,"id":"call_2","function":{"name":"get_time","arguments":"{}"}}]},"finish_reason":null}]}"#;
        let event = GenericHttpAdapter::parse_chunk(data).unwrap();
        match event {
            ProviderStreamEvent::Chunk { tool_calls, .. } => {
                assert_eq!(tool_calls.len(), 2);
                assert_eq!(tool_calls[0].name.as_deref(), Some("get_weather"));
                assert_eq!(tool_calls[1].name.as_deref(), Some("get_time"));
            }
            _ => panic!("expected Chunk"),
        }
    }
}
