use crate::error::LlmError;
use crate::provider::LlProvider;
use crate::types::{
    LlFinishReason, LlTool, LlToolCallDelta, ProviderCapability, ProviderEmbedRequest,
    ProviderEmbedResponse, ProviderHealthResult, ProviderMessage, ProviderRequest,
    ProviderResponse, ProviderStream, ProviderStreamEvent,
};
use std::time::SystemTime;

pub struct GeminiAdapter {
    config: crate::types::ProviderConfig,
    agent: ureq::Agent,
}

impl GeminiAdapter {
    pub fn new(config: crate::types::ProviderConfig) -> Self {
        let agent = super::http::build_agent(&config);
        Self { config, agent }
    }

    fn api_url(&self) -> String {
        if !self.config.api_url.is_empty() {
            self.config.api_url.trim_end_matches('/').to_string()
        } else {
            "https://generativelanguage.googleapis.com/v1beta".to_string()
        }
    }

    fn generate_content_url(&self, model: &str) -> String {
        let key = self.config.api_key.as_deref().unwrap_or("");
        format!(
            "{}/models/{}:generateContent?key={}",
            self.api_url(),
            model,
            key
        )
    }

    fn stream_generate_content_url(&self, model: &str) -> String {
        let key = self.config.api_key.as_deref().unwrap_or("");
        format!(
            "{}/models/{}:streamGenerateContent?key={}&alt=sse",
            self.api_url(),
            model,
            key
        )
    }

    fn build_chat_body(&self, request: &ProviderRequest) -> serde_json::Value {
        let contents = build_gemini_contents(request);

        let mut body = serde_json::json!({
            "contents": contents,
        });

        let mut generation_config = serde_json::Map::new();
        if let Some(max_tokens) = request.max_tokens {
            generation_config.insert("maxOutputTokens".into(), serde_json::json!(max_tokens));
        }
        if let Some(temp) = request.temperature {
            generation_config.insert("temperature".into(), serde_json::json!(temp));
        }
        if !request.stop_sequences.is_empty() {
            generation_config.insert(
                "stopSequences".into(),
                serde_json::json!(request.stop_sequences),
            );
        }
        if !generation_config.is_empty() {
            body["generationConfig"] = serde_json::Value::Object(generation_config);
        }

        if !request.tools.is_empty() {
            body["tools"] = serde_json::json!(build_gemini_tools(&request.tools));
        }

        if let Some(ref system) = request.system_prompt {
            body["systemInstruction"] = serde_json::json!({
                "parts": [{"text": system}]
            });
        }

        body
    }

    fn parse_chat_response(&self, body: &str) -> Result<ProviderResponse, LlmError> {
        let val: serde_json::Value =
            serde_json::from_str(body).map_err(|e| LlmError::MalformedResponse(e.to_string()))?;

        let candidate = val["candidates"][0]
            .as_object()
            .ok_or_else(|| LlmError::MalformedResponse("missing candidates[0]".into()))?;

        let content = candidate["content"]
            .as_object()
            .ok_or_else(|| LlmError::MalformedResponse("missing content".into()))?;

        let parts = content["parts"]
            .as_array()
            .ok_or_else(|| LlmError::MalformedResponse("missing parts".into()))?;

        let mut text_content = String::new();
        let mut tool_call_content: Option<(
            String,
            std::collections::HashMap<String, serde_json::Value>,
        )> = None;

        for part in parts {
            match part.get("text").and_then(|t| t.as_str()) {
                Some(text) => text_content.push_str(text),
                None => {
                    // Check for function call
                    if let Some(fc) = part.get("functionCall") {
                        let name = fc["name"].as_str().unwrap_or("").to_string();
                        let args = fc["args"].as_object().cloned().unwrap_or_default();
                        let arguments: std::collections::HashMap<String, serde_json::Value> =
                            args.into_iter().collect();
                        tool_call_content = Some((name, arguments));
                    }
                }
            }
        }

        let finish_reason = candidate
            .get("finishReason")
            .and_then(|f| f.as_str())
            .map(map_gemini_finish_reason)
            .unwrap_or(LlFinishReason::Stop);

        let provider_content = if let Some((name, arguments)) = tool_call_content {
            crate::types::LlContent::ToolCall {
                call_id: format!("fc-{}", name),
                name,
                arguments,
            }
        } else {
            crate::types::LlContent::Text(text_content)
        };

        let usage = val.get("usageMetadata");
        let input_tokens = usage
            .and_then(|u| u["promptTokenCount"].as_u64())
            .unwrap_or(0);
        let output_tokens = usage
            .and_then(|u| u["candidatesTokenCount"].as_u64())
            .unwrap_or(0);
        let model = val["modelVersion"].as_str().unwrap_or("").to_string();

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
        let val: serde_json::Value =
            serde_json::from_str(data).map_err(|e| LlmError::MalformedResponse(e.to_string()))?;

        // Check for error
        if let Some(error) = val.get("error") {
            let msg = error["message"].as_str().unwrap_or("unknown error");
            return Err(LlmError::ProviderError(msg.to_string()));
        }

        let candidate = match val["candidates"][0].as_object() {
            Some(c) => c,
            None => return Ok(None),
        };

        let finish_reason = candidate
            .get("finishReason")
            .and_then(|f| f.as_str())
            .filter(|r| !r.is_empty())
            .map(map_gemini_finish_reason);

        let parts = candidate["content"]["parts"].as_array();
        let content = parts
            .and_then(|p| p[0].get("text"))
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();

        // Check parts for functionCall
        let tool_calls = parts
            .and_then(|p| {
                p.iter().find_map(|part| {
                    let fc = part.get("functionCall")?;
                    let name = fc["name"].as_str()?.to_string();
                    let args_str =
                        serde_json::to_string(&fc["args"]).unwrap_or_else(|_| "{}".to_string());
                    Some(vec![LlToolCallDelta {
                        index: 0,
                        id: Some(format!("fc-{}", name)),
                        name: Some(name),
                        arguments: Some(args_str),
                    }])
                })
            })
            .unwrap_or_default();

        if let Some(fr) = finish_reason {
            return Ok(Some(ProviderStreamEvent::Chunk {
                content,
                finish_reason: Some(fr),
                tool_calls,
            }));
        }

        Ok(Some(ProviderStreamEvent::Chunk {
            content,
            finish_reason: None,
            tool_calls,
        }))
    }
}

fn build_gemini_contents(request: &ProviderRequest) -> Vec<serde_json::Value> {
    let mut contents: Vec<serde_json::Value> = Vec::new();

    for msg in &request.messages {
        let role = match msg.role {
            crate::types::LlRole::User => "user",
            crate::types::LlRole::Assistant => "model",
            crate::types::LlRole::System => "user", // system → user (gemini doesn't have system role in contents)
            crate::types::LlRole::Tool => "function", // tool → function
        };

        let parts = match &msg.content {
            crate::types::LlContent::Text(text) => {
                vec![serde_json::json!({"text": text})]
            }
            crate::types::LlContent::Image { mime_type, data } => {
                let b64 = crate::adapters::openai::base64_encode(data);
                vec![serde_json::json!({
                    "inlineData": {
                        "mimeType": mime_type,
                        "data": b64
                    }
                })]
            }
            crate::types::LlContent::ToolResult { call_id, output } => {
                vec![serde_json::json!({
                    "functionResponse": {
                        "name": call_id,
                        "response": {"response": output}
                    }
                })]
            }
            crate::types::LlContent::ToolCall {
                call_id: _,
                name,
                arguments,
            } => {
                vec![serde_json::json!({
                    "functionCall": {
                        "name": name,
                        "args": arguments
                    }
                })]
            }
        };

        contents.push(serde_json::json!({
            "role": role,
            "parts": parts
        }));
    }

    contents
}

fn build_gemini_tools(tools: &[LlTool]) -> Vec<serde_json::Value> {
    tools
        .iter()
        .map(|t| {
            serde_json::json!({
                "functionDeclarations": [{
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters,
                }]
            })
        })
        .collect()
}

fn map_gemini_finish_reason(reason: &str) -> LlFinishReason {
    match reason {
        "STOP" => LlFinishReason::Stop,
        "MAX_TOKENS" => LlFinishReason::Length,
        "SAFETY" => LlFinishReason::ContentFilter,
        "RECITATION" => LlFinishReason::ContentFilter,
        "TOOL_CALL" | "FUNCTION_CALL" => LlFinishReason::ToolCalls,
        _ => LlFinishReason::Error,
    }
}

impl LlProvider for GeminiAdapter {
    fn id(&self) -> &str {
        &self.config.provider_id
    }

    fn name(&self) -> &str {
        "Gemini"
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
        let url = self.generate_content_url(&request.model);
        let response_body = super::http::send_json_post(&self.agent, &url, &self.config, &body)?;
        self.parse_chat_response(&response_body)
    }

    fn chat_stream(&self, request: ProviderRequest) -> Result<ProviderStream, LlmError> {
        let body = self.build_chat_body(&request);
        let url = self.stream_generate_content_url(&request.model);
        let reader = super::http::send_json_post_stream(&self.agent, &url, &self.config, &body)?;

        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut reader = std::io::BufReader::new(reader);
            loop {
                match super::http::read_sse_event(&mut reader) {
                    Some(data) => match Self::parse_chunk(&data) {
                        Ok(Some(event)) => {
                            if tx.send(event).is_err() {
                                break;
                            }
                        }
                        Ok(None) => {}
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

    fn embed(&self, _request: ProviderEmbedRequest) -> Result<ProviderEmbedResponse, LlmError> {
        Err(LlmError::ConfigurationError(
            "Gemini embedding not yet supported via adapter".into(),
        ))
    }

    fn health(&self) -> ProviderHealthResult {
        let start = std::time::Instant::now();
        // Use a minimal model call to check health
        let model = self
            .config
            .default_model
            .as_deref()
            .unwrap_or("gemini-1.5-pro");
        let body = serde_json::json!({
            "contents": [{"parts": [{"text": "hi"}]}]
        });
        let url = self.generate_content_url(model);
        match super::http::send_json_post(&self.agent, &url, &self.config, &body) {
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
            provider_id: "gemini".into(),
            provider_type: "gemini".into(),
            api_url: String::new(),
            api_key: Some("AIza-test".into()),
            organization_id: None,
            default_model: Some("gemini-1.5-pro".into()),
            models: vec!["gemini-1.5-pro".into(), "gemini-1.5-flash".into()],
            timeout_secs: 30,
            max_retries: 3,
            custom_headers: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn adapter_identity() {
        let adapter = GeminiAdapter::new(test_config());
        assert_eq!(adapter.id(), "gemini");
        assert_eq!(adapter.name(), "Gemini");
    }

    #[test]
    fn capabilities_include_chat() {
        let adapter = GeminiAdapter::new(test_config());
        let caps = adapter.capabilities();
        assert!(caps.contains(&ProviderCapability::Chat));
    }

    #[test]
    fn parse_chat_response_simple() {
        let adapter = GeminiAdapter::new(test_config());
        let body = r#"{
            "candidates": [{
                "content": {
                    "parts": [{"text": "Hello!"}],
                    "role": "model"
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 10,
                "candidatesTokenCount": 5
            }
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
    fn parse_chat_response_tool_call() {
        let adapter = GeminiAdapter::new(test_config());
        let body = r#"{
            "candidates": [{
                "content": {
                    "parts": [
                        {"text": "Let me check..."},
                        {"functionCall": {"name": "get_weather", "args": {"location": "NYC"}}}
                    ],
                    "role": "model"
                },
                "finishReason": "FUNCTION_CALL"
            }],
            "usageMetadata": {
                "promptTokenCount": 10,
                "candidatesTokenCount": 5
            }
        }"#;
        let resp = adapter.parse_chat_response(body).unwrap();
        assert_eq!(resp.finish_reason, LlFinishReason::ToolCalls);
        assert!(matches!(resp.message.content, LlContent::ToolCall { .. }));
    }

    #[test]
    fn parse_chunk_content() {
        let data = r#"{"candidates":[{"content":{"parts":[{"text":"Hello"}],"role":"model"},"finishReason":""}]}"#;
        let event = GeminiAdapter::parse_chunk(data).unwrap();
        if let Some(ProviderStreamEvent::Chunk { content, .. }) = event {
            assert_eq!(content, "Hello");
        } else {
            panic!("expected Chunk");
        }
    }

    #[test]
    fn parse_chunk_finish() {
        let data = r#"{"candidates":[{"content":{"parts":[{"text":""}],"role":"model"},"finishReason":"STOP"}]}"#;
        let event = GeminiAdapter::parse_chunk(data).unwrap();
        if let Some(ProviderStreamEvent::Chunk { finish_reason, .. }) = event {
            assert_eq!(finish_reason, Some(LlFinishReason::Stop));
        } else {
            panic!("expected Chunk with finish_reason");
        }
    }

    #[test]
    fn map_finish_reason_stop() {
        assert_eq!(map_gemini_finish_reason("STOP"), LlFinishReason::Stop);
    }

    #[test]
    fn map_finish_reason_max_tokens() {
        assert_eq!(
            map_gemini_finish_reason("MAX_TOKENS"),
            LlFinishReason::Length
        );
    }

    #[test]
    fn map_finish_reason_tool_call() {
        assert_eq!(
            map_gemini_finish_reason("TOOL_CALL"),
            LlFinishReason::ToolCalls
        );
    }

    #[test]
    fn health_returns_unavailable_on_bad_key() {
        let mut cfg = test_config();
        cfg.api_key = Some("bad-key".into());
        cfg.timeout_secs = 1;
        let adapter = GeminiAdapter::new(cfg);
        let result = adapter.health();
        // With a bad API key we expect an error but not a panic
        match result.status {
            crate::types::ProviderHealth::Unavailable { .. } => {}
            _ => panic!("expected Unavailable"),
        }
    }

    #[test]
    fn build_chat_body_with_system() {
        let adapter = GeminiAdapter::new(test_config());
        let request = ProviderRequest {
            model: "gemini-1.5-pro".into(),
            messages: vec![ProviderMessage {
                role: LlRole::User,
                content: LlContent::Text("hi".into()),
            }],
            max_tokens: Some(100),
            temperature: Some(0.7),
            stop_sequences: vec!["stop".into()],
            system_prompt: Some("be helpful".into()),
            tools: vec![],
            stream: false,
            timeout_ms: 30000,
        };
        let body = adapter.build_chat_body(&request);
        assert_eq!(body["generationConfig"]["maxOutputTokens"], 100);
        assert_eq!(body["generationConfig"]["temperature"], 0.7);
        assert_eq!(body["systemInstruction"]["parts"][0]["text"], "be helpful");
    }

    #[test]
    fn adapter_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<GeminiAdapter>();
        assert_sync::<GeminiAdapter>();
    }

    // ─── tool_calls streaming ───────────────────────────────────────────────

    #[test]
    fn parse_chunk_function_call() {
        let data = r#"{"candidates":[{"content":{"parts":[{"functionCall":{"name":"get_weather","args":{"location":"NYC"}}}],"role":"model"},"finishReason":"FUNCTION_CALL"}]}"#;
        let event = GeminiAdapter::parse_chunk(data).unwrap();
        if let Some(ProviderStreamEvent::Chunk {
            tool_calls,
            finish_reason,
            ..
        }) = event
        {
            assert_eq!(tool_calls.len(), 1);
            assert_eq!(tool_calls[0].index, 0);
            assert_eq!(tool_calls[0].name.as_deref(), Some("get_weather"));
            assert_eq!(finish_reason, Some(LlFinishReason::ToolCalls));
        } else {
            panic!("expected Chunk with tool_calls");
        }
    }

    #[test]
    fn parse_chunk_normal_text_has_empty_tool_calls() {
        let data = r#"{"candidates":[{"content":{"parts":[{"text":"Hello"}],"role":"model"},"finishReason":""}]}"#;
        let event = GeminiAdapter::parse_chunk(data).unwrap();
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
    fn parse_chunk_no_tool_calls_returns_empty() {
        let data = r#"{"candidates":[{"content":{"parts":[{"text":"Just text"}],"role":"model"},"finishReason":"STOP"}]}"#;
        let event = GeminiAdapter::parse_chunk(data).unwrap();
        if let Some(ProviderStreamEvent::Chunk {
            tool_calls,
            content,
            ..
        }) = event
        {
            assert_eq!(content, "Just text");
            assert!(tool_calls.is_empty());
        } else {
            panic!("expected Chunk");
        }
    }
}
