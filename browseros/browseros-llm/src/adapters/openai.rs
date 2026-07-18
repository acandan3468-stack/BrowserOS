use crate::error::LlmError;
use crate::provider::LlProvider;
use crate::types::{
    LlFinishReason, LlTool, LlToolCallDelta, ProviderCapability, ProviderEmbedRequest,
    ProviderEmbedResponse, ProviderHealthResult, ProviderMessage, ProviderRequest,
    ProviderResponse, ProviderStream, ProviderStreamEvent,
};
use std::collections::HashMap;
use std::time::SystemTime;

pub struct OpenAIAdapter {
    config: crate::types::ProviderConfig,
    agent: ureq::Agent,
}

impl OpenAIAdapter {
    pub fn new(config: crate::types::ProviderConfig) -> Self {
        let agent = super::http::build_agent(&config);
        Self { config, agent }
    }

    fn api_url(&self) -> String {
        if !self.config.api_url.is_empty() {
            self.config.api_url.trim_end_matches('/').to_string()
        } else {
            "https://api.openai.com".to_string()
        }
    }

    fn chat_url(&self) -> String {
        format!("{}/v1/chat/completions", self.api_url())
    }

    fn embed_url(&self) -> String {
        format!("{}/v1/embeddings", self.api_url())
    }

    fn models_url(&self) -> String {
        format!("{}/v1/models", self.api_url())
    }

    fn build_chat_body(&self, request: &ProviderRequest) -> serde_json::Value {
        let messages = build_openai_messages(request);

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
            body["tools"] = serde_json::json!(build_openai_tools(&request.tools));
        }
        if request.stream {
            body["stream"] = serde_json::json!(true);
            body["stream_options"] = serde_json::json!({"include_usage": true});
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

        let _role = message
            .get("role")
            .and_then(|r| r.as_str())
            .unwrap_or("assistant");

        // Tool calls
        let tool_calls_content =
            if let Some(tcs) = message.get("tool_calls").and_then(|t| t.as_array()) {
                if !tcs.is_empty() {
                    let tc = &tcs[0];
                    let call_id = tc["id"].as_str().unwrap_or("").to_string();
                    let name = tc["function"]["name"].as_str().unwrap_or("").to_string();
                    let args_str = tc["function"]["arguments"].as_str().unwrap_or("{}");
                    let arguments: HashMap<String, serde_json::Value> =
                        serde_json::from_str(args_str).unwrap_or_default();
                    Some(crate::types::LlContent::ToolCall {
                        call_id,
                        name,
                        arguments,
                    })
                } else {
                    None
                }
            } else {
                None
            };

        let provider_content = if let Some(tc) = tool_calls_content {
            crate::types::LlContent::ToolCall {
                call_id: match &tc {
                    crate::types::LlContent::ToolCall { call_id, .. } => call_id.clone(),
                    _ => unreachable!(),
                },
                name: match &tc {
                    crate::types::LlContent::ToolCall { name, .. } => name.clone(),
                    _ => unreachable!(),
                },
                arguments: match &tc {
                    crate::types::LlContent::ToolCall { arguments, .. } => arguments.clone(),
                    _ => unreachable!(),
                },
            }
        } else {
            crate::types::LlContent::Text(content)
        };

        let finish_reason = choice
            .get("finish_reason")
            .and_then(|f| f.as_str())
            .map(map_openai_finish_reason)
            .unwrap_or(LlFinishReason::Stop);

        let input_tokens = val["usage"]["prompt_tokens"].as_u64().unwrap_or(0);
        let output_tokens = val["usage"]["completion_tokens"].as_u64().unwrap_or(0);
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

        let input_tokens = val["usage"]["prompt_tokens"].as_u64().unwrap_or(0);
        let model = val["model"].as_str().unwrap_or("").to_string();

        Ok(ProviderEmbedResponse {
            embeddings,
            input_tokens,
            model,
        })
    }
}

fn build_openai_messages(request: &ProviderRequest) -> Vec<serde_json::Value> {
    let mut messages: Vec<serde_json::Value> = Vec::new();

    // System prompt as a system message
    if let Some(ref system) = request.system_prompt {
        messages.push(serde_json::json!({
            "role": "system",
            "content": system
        }));
    }

    for msg in &request.messages {
        let role = msg.role.as_str();
        let content = match &msg.content {
            crate::types::LlContent::Text(text) => {
                serde_json::json!(text)
            }
            crate::types::LlContent::Image { mime_type, data } => {
                let b64 = base64_encode(data);
                serde_json::json!([
                    {"type": "text", "text": ""},
                    {"type": "image_url", "image_url": {"url": format!("data:{};base64,{}", mime_type, b64)}}
                ])
            }
            crate::types::LlContent::ToolResult { call_id, output } => {
                serde_json::json!({
                    "role": "tool",
                    "tool_call_id": call_id,
                    "content": output
                })
            }
            crate::types::LlContent::ToolCall {
                call_id,
                name,
                arguments,
            } => {
                return vec![serde_json::json!({
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
                })];
            }
        };

        messages.push(serde_json::json!({
            "role": role,
            "content": content
        }));
    }

    messages
}

fn build_openai_tools(tools: &[LlTool]) -> Vec<serde_json::Value> {
    tools
        .iter()
        .map(|t| {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters,
                    "strict": t.strict,
                }
            })
        })
        .collect()
}

fn map_openai_finish_reason(reason: &str) -> LlFinishReason {
    match reason {
        "stop" => LlFinishReason::Stop,
        "length" | "max_tokens" => LlFinishReason::Length,
        "tool_calls" | "function_call" => LlFinishReason::ToolCalls,
        "content_filter" => LlFinishReason::ContentFilter,
        _ => LlFinishReason::Error,
    }
}

pub(crate) fn base64_encode(data: &[u8]) -> String {
    // Simple base64 implementation without external dependency
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

struct StreamAggregator {
    content: String,
    tool_calls: Vec<ToolCallAccumulator>,
}

struct ToolCallAccumulator {
    index: usize,
    arguments: String,
}

impl StreamAggregator {
    fn new() -> Self {
        Self {
            content: String::new(),
            tool_calls: Vec::new(),
        }
    }

    fn start_tool_call(&mut self, index: usize) {
        self.tool_calls.push(ToolCallAccumulator {
            index,
            arguments: String::new(),
        });
    }

    fn append_tool_args(&mut self, index: usize, args: &str) {
        if let Some(tc) = self.tool_calls.iter_mut().find(|t| t.index == index) {
            tc.arguments.push_str(args);
        }
    }

    fn has_tool_calls(&self) -> bool {
        !self.tool_calls.is_empty()
    }
}

impl LlProvider for OpenAIAdapter {
    fn id(&self) -> &str {
        &self.config.provider_id
    }

    fn name(&self) -> &str {
        "OpenAI"
    }

    fn capabilities(&self) -> Vec<ProviderCapability> {
        vec![
            ProviderCapability::Chat,
            ProviderCapability::Streaming,
            ProviderCapability::Embedding,
            ProviderCapability::FunctionCalling,
            ProviderCapability::ToolUse,
            ProviderCapability::SystemPrompt,
            ProviderCapability::JsonMode,
            ProviderCapability::Vision,
        ]
    }

    fn models(&self) -> Vec<String> {
        let url = self.models_url();
        match super::http::send_get(&self.agent, &url, &self.config) {
            Ok(body) => {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&body) {
                    if let Some(data) = val["data"].as_array() {
                        return data
                            .iter()
                            .filter_map(|m| m["id"].as_str().map(|s| s.to_string()))
                            .collect();
                    }
                }
                self.config.models.clone()
            }
            Err(_) => self.config.models.clone(),
        }
    }

    fn chat(&self, request: ProviderRequest) -> Result<ProviderResponse, LlmError> {
        let body = self.build_chat_body(&request);
        let response_body =
            super::http::send_json_post(&self.agent, &self.chat_url(), &self.config, &body)?;
        self.parse_chat_response(&response_body)
    }

    fn chat_stream(&self, request: ProviderRequest) -> Result<ProviderStream, LlmError> {
        let body = self.build_chat_body(&request);
        let reader =
            super::http::send_json_post_stream(&self.agent, &self.chat_url(), &self.config, &body)?;

        let (tx, rx) = std::sync::mpsc::channel();
        let mut aggregator = StreamAggregator::new();

        std::thread::spawn(move || {
            let mut reader = std::io::BufReader::new(reader);
            loop {
                match super::http::read_sse_event(&mut reader) {
                    Some(data) => match Self::parse_chunk_static(&data, &mut aggregator) {
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
        let response_body =
            super::http::send_json_post(&self.agent, &self.embed_url(), &self.config, &body)?;
        self.parse_embed_response(&response_body)
    }

    fn health(&self) -> ProviderHealthResult {
        let start = std::time::Instant::now();
        match super::http::send_get(&self.agent, &self.models_url(), &self.config) {
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

impl OpenAIAdapter {
    fn parse_chunk_static(
        data: &str,
        aggregator: &mut StreamAggregator,
    ) -> Result<ProviderStreamEvent, LlmError> {
        let val: serde_json::Value =
            serde_json::from_str(data).map_err(|e| LlmError::MalformedResponse(e.to_string()))?;

        if let Some(usage) = val.get("usage") {
            if usage.is_object() && !usage.as_object().map(|o| o.is_empty()).unwrap_or(true) {
                let input_tokens = usage["prompt_tokens"].as_u64().unwrap_or(0);
                let output_tokens = usage["completion_tokens"].as_u64().unwrap_or(0);
                return Ok(ProviderStreamEvent::Done {
                    input_tokens,
                    output_tokens,
                });
            }
        }

        let choices = val["choices"].as_array();
        if choices.is_none() || choices.map(|c| c.is_empty()).unwrap_or(true) {
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
            .unwrap_or("");

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

                        // Track in aggregator for finish_reason correction
                        if id.is_some() {
                            aggregator.start_tool_call(idx);
                        }
                        if let Some(ref a) = args {
                            aggregator.append_tool_args(idx, a);
                        }

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
            .map(map_openai_finish_reason);

        let finish_reason =
            if finish_reason == Some(LlFinishReason::Stop) && aggregator.has_tool_calls() {
                Some(LlFinishReason::ToolCalls)
            } else {
                finish_reason
            };

        if !content.is_empty() {
            if !aggregator.content.is_empty() {
                aggregator.content.push_str(content);
            } else {
                aggregator.content = content.to_string();
            }
        }

        Ok(ProviderStreamEvent::Chunk {
            content: content.to_string(),
            finish_reason,
            tool_calls,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{LlContent, LlRole};

    fn test_config() -> crate::types::ProviderConfig {
        crate::types::ProviderConfig {
            provider_id: "openai".into(),
            provider_type: "openai".into(),
            api_url: String::new(),
            api_key: Some("sk-test".into()),
            organization_id: None,
            default_model: Some("gpt-4".into()),
            models: vec!["gpt-4".into(), "gpt-3.5-turbo".into()],
            timeout_secs: 30,
            max_retries: 3,
            custom_headers: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn adapter_identity() {
        let adapter = OpenAIAdapter::new(test_config());
        assert_eq!(adapter.id(), "openai");
        assert_eq!(adapter.name(), "OpenAI");
    }

    #[test]
    fn capabilities_include_chat() {
        let adapter = OpenAIAdapter::new(test_config());
        let caps = adapter.capabilities();
        assert!(caps.contains(&ProviderCapability::Chat));
        assert!(caps.contains(&ProviderCapability::Streaming));
        assert!(caps.contains(&ProviderCapability::Embedding));
        assert!(caps.contains(&ProviderCapability::ToolUse));
    }

    #[test]
    fn models_from_config() {
        let adapter = OpenAIAdapter::new(test_config());
        let models = adapter.models();
        assert!(models.contains(&"gpt-4".to_string()));
    }

    #[test]
    fn parse_chat_response_simple() {
        let adapter = OpenAIAdapter::new(test_config());
        let body = r#"{
            "id": "chat-1",
            "object": "chat.completion",
            "created": 123,
            "model": "gpt-4",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "Hello!"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
        }"#;
        let resp = adapter.parse_chat_response(body).unwrap();
        assert_eq!(resp.input_tokens, 10);
        assert_eq!(resp.output_tokens, 5);
        assert_eq!(resp.finish_reason, LlFinishReason::Stop);
        assert_eq!(resp.model, "gpt-4");
        if let LlContent::Text(ref t) = resp.message.content {
            assert_eq!(t, "Hello!");
        } else {
            panic!("expected Text content");
        }
    }

    #[test]
    fn parse_chat_response_tool_calls() {
        let adapter = OpenAIAdapter::new(test_config());
        let body = r#"{
            "id": "chat-1",
            "object": "chat.completion",
            "created": 123,
            "model": "gpt-4",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call-1",
                        "type": "function",
                        "function": {"name": "get_weather", "arguments": "{\"location\":\"NYC\"}"}
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
        }"#;
        let resp = adapter.parse_chat_response(body).unwrap();
        assert_eq!(resp.finish_reason, LlFinishReason::ToolCalls);
        assert!(matches!(resp.message.content, LlContent::ToolCall { .. }));
        if let LlContent::ToolCall {
            name, arguments, ..
        } = &resp.message.content
        {
            assert_eq!(name, "get_weather");
            assert_eq!(arguments.get("location"), Some(&serde_json::json!("NYC")));
        }
    }

    #[test]
    fn parse_chat_response_missing_choices() {
        let adapter = OpenAIAdapter::new(test_config());
        let body = r#"{"error": "not found"}"#;
        let err = adapter.parse_chat_response(body);
        assert!(err.is_err());
    }

    #[test]
    fn parse_embed_response_simple() {
        let adapter = OpenAIAdapter::new(test_config());
        let body = r#"{
            "object": "list",
            "data": [{"object": "embedding", "index": 0, "embedding": [0.1, 0.2, 0.3]}],
            "model": "text-embedding-3-small",
            "usage": {"prompt_tokens": 5, "total_tokens": 5}
        }"#;
        let resp = adapter.parse_embed_response(body).unwrap();
        assert_eq!(resp.embeddings.len(), 1);
        assert_eq!(resp.embeddings[0].len(), 3);
        assert_eq!(resp.input_tokens, 5);
    }

    #[test]
    fn parse_embed_response_empty() {
        let adapter = OpenAIAdapter::new(test_config());
        let body = r#"{"object": "list", "data": [], "model": "text-embedding-3-small", "usage": {"prompt_tokens": 0}}"#;
        let resp = adapter.parse_embed_response(body).unwrap();
        assert!(resp.embeddings.is_empty());
    }

    #[test]
    fn map_finish_reason_stop() {
        assert_eq!(map_openai_finish_reason("stop"), LlFinishReason::Stop);
    }

    #[test]
    fn map_finish_reason_length() {
        assert_eq!(map_openai_finish_reason("length"), LlFinishReason::Length);
    }

    #[test]
    fn map_finish_reason_tool_calls() {
        assert_eq!(
            map_openai_finish_reason("tool_calls"),
            LlFinishReason::ToolCalls
        );
    }

    #[test]
    fn map_finish_reason_content_filter() {
        assert_eq!(
            map_openai_finish_reason("content_filter"),
            LlFinishReason::ContentFilter
        );
    }

    #[test]
    fn map_finish_reason_unknown() {
        assert_eq!(map_openai_finish_reason("???"), LlFinishReason::Error);
    }

    #[test]
    fn base64_encode_works() {
        let encoded = base64_encode(b"hello");
        assert_eq!(encoded, "aGVsbG8=");
    }

    #[test]
    fn base64_encode_empty() {
        let encoded = base64_encode(b"");
        assert_eq!(encoded, "");
    }

    #[test]
    fn base64_encode_binary() {
        let data = vec![0u8, 1u8, 2u8, 3u8, 255u8, 254u8];
        let encoded = base64_encode(&data);
        assert!(!encoded.is_empty());
    }

    #[test]
    fn build_chat_body_simple() {
        let adapter = OpenAIAdapter::new(test_config());
        let request = ProviderRequest {
            model: "gpt-4".into(),
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
        assert_eq!(body["model"], "gpt-4");
        assert_eq!(body["temperature"], 0.7);
        assert_eq!(body["max_tokens"], 100);
        assert_eq!(body["system"], "be helpful");
        assert!(body.get("stream").is_none());
    }

    #[test]
    fn build_chat_body_stream() {
        let adapter = OpenAIAdapter::new(test_config());
        let request = ProviderRequest {
            model: "gpt-4".into(),
            messages: vec![ProviderMessage {
                role: LlRole::User,
                content: LlContent::Text("hi".into()),
            }],
            max_tokens: None,
            temperature: None,
            stop_sequences: vec![],
            system_prompt: None,
            tools: vec![],
            stream: true,
            timeout_ms: 30000,
        };
        let body = adapter.build_chat_body(&request);
        assert_eq!(body["stream"], true);
    }

    #[test]
    fn build_chat_body_tools() {
        let adapter = OpenAIAdapter::new(test_config());
        let request = ProviderRequest {
            model: "gpt-4".into(),
            messages: vec![],
            max_tokens: None,
            temperature: None,
            stop_sequences: vec![],
            system_prompt: None,
            tools: vec![LlTool::new("get_weather", "Get weather")],
            stream: false,
            timeout_ms: 30000,
        };
        let body = adapter.build_chat_body(&request);
        assert!(body["tools"].is_array());
        assert_eq!(body["tools"][0]["function"]["name"], "get_weather");
    }

    #[test]
    fn parse_chunk_content() {
        let mut agg = StreamAggregator::new();
        let data = r#"{"choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}"#;
        let event = OpenAIAdapter::parse_chunk_static(data, &mut agg).unwrap();
        match event {
            ProviderStreamEvent::Chunk { content, .. } => {
                assert_eq!(content, "Hello");
            }
            _ => panic!("expected Chunk"),
        }
    }

    #[test]
    fn parse_chunk_done() {
        let mut agg = StreamAggregator::new();
        let data = r#"{"choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":5,"completion_tokens":10}}"#;
        let event = OpenAIAdapter::parse_chunk_static(data, &mut agg).unwrap();
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
        let mut agg = StreamAggregator::new();
        let data = r#"{"choices":[]}"#;
        let event = OpenAIAdapter::parse_chunk_static(data, &mut agg).unwrap();
        match event {
            ProviderStreamEvent::Chunk { content, .. } => {
                assert_eq!(content, "");
            }
            _ => panic!("expected Chunk"),
        }
    }

    #[test]
    fn parse_chunk_invalid_json() {
        let mut agg = StreamAggregator::new();
        let data = "not json";
        let err = OpenAIAdapter::parse_chunk_static(data, &mut agg);
        assert!(err.is_err());
    }

    #[test]
    fn build_openai_tools_returns_correct_shape() {
        let tools = vec![LlTool::new("get_weather", "Get the weather")
            .with_parameters(serde_json::json!({"type": "object"}))];
        let result = build_openai_tools(&tools);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["type"], "function");
        assert_eq!(result[0]["function"]["name"], "get_weather");
    }

    #[test]
    fn health_returns_unknown_on_timeout() {
        let mut cfg = test_config();
        // Point to a non-routable address so it fails fast
        cfg.api_url = "http://127.0.0.1:1".into();
        cfg.timeout_secs = 1;
        let adapter = OpenAIAdapter::new(cfg);
        let result = adapter.health();
        match result.status {
            crate::types::ProviderHealth::Unavailable { .. } => {}
            _ => panic!("expected Unavailable"),
        }
        assert!(result.error.is_some());
    }

    #[test]
    fn adapter_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<OpenAIAdapter>();
        assert_sync::<OpenAIAdapter>();
    }

    // ─── tool_calls streaming ───────────────────────────────────────────────

    #[test]
    fn parse_chunk_partial_tool_call_delta() {
        let mut agg = StreamAggregator::new();
        let data = r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"get_weather","arguments":""}}]},"finish_reason":null}]}"#;
        let event = OpenAIAdapter::parse_chunk_static(data, &mut agg).unwrap();
        match event {
            ProviderStreamEvent::Chunk {
                tool_calls,
                content,
                ..
            } => {
                assert!(content.is_empty());
                assert_eq!(tool_calls.len(), 1);
                assert_eq!(tool_calls[0].index, 0);
                assert_eq!(tool_calls[0].id.as_deref(), Some("call_1"));
                assert_eq!(tool_calls[0].name.as_deref(), Some("get_weather"));
                assert_eq!(tool_calls[0].arguments.as_deref(), Some(""));
            }
            _ => panic!("expected Chunk"),
        }
    }

    #[test]
    fn parse_chunk_multiple_delta_accumulation() {
        let mut agg = StreamAggregator::new();
        // First delta: id+name (start of tool call)
        let data1 = r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"get_weather","arguments":""}}]},"finish_reason":null}]}"#;
        let event1 = OpenAIAdapter::parse_chunk_static(data1, &mut agg).unwrap();
        match event1 {
            ProviderStreamEvent::Chunk { tool_calls, .. } => {
                assert_eq!(tool_calls.len(), 1);
                assert!(tool_calls[0].is_start());
            }
            _ => panic!("expected Chunk"),
        }
        // Second delta: arguments (continuation)
        let data2 = r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"location\":\"NYC\"}"}}]},"finish_reason":null}]}"#;
        let event2 = OpenAIAdapter::parse_chunk_static(data2, &mut agg).unwrap();
        match event2 {
            ProviderStreamEvent::Chunk { tool_calls, .. } => {
                assert_eq!(tool_calls.len(), 1);
                assert!(!tool_calls[0].is_start());
                assert_eq!(
                    tool_calls[0].arguments.as_deref(),
                    Some("{\"location\":\"NYC\"}")
                );
            }
            _ => panic!("expected Chunk"),
        }
    }

    #[test]
    fn parse_chunk_text_only_has_empty_tool_calls() {
        let mut agg = StreamAggregator::new();
        let data = r#"{"choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}"#;
        let event = OpenAIAdapter::parse_chunk_static(data, &mut agg).unwrap();
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
        let mut agg = StreamAggregator::new();
        let data = r#"{"choices":[{"index":0,"delta":{"content":"Let me check","tool_calls":[{"index":0,"id":"call_1","function":{"name":"get_weather","arguments":"{\"loc\":\"NYC\"}"}}]},"finish_reason":null}]}"#;
        let event = OpenAIAdapter::parse_chunk_static(data, &mut agg).unwrap();
        match event {
            ProviderStreamEvent::Chunk {
                tool_calls,
                content,
                ..
            } => {
                assert_eq!(content, "Let me check");
                assert_eq!(tool_calls.len(), 1);
                assert_eq!(tool_calls[0].index, 0);
                assert_eq!(tool_calls[0].id.as_deref(), Some("call_1"));
            }
            _ => panic!("expected Chunk"),
        }
    }

    #[test]
    fn parse_chunk_multiple_simultaneous_tool_calls() {
        let mut agg = StreamAggregator::new();
        let data = r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"get_weather","arguments":"{\"loc\":\"NYC\"}"}},{"index":1,"id":"call_2","function":{"name":"get_time","arguments":"{\"tz\":\"EST\"}"}}]},"finish_reason":null}]}"#;
        let event = OpenAIAdapter::parse_chunk_static(data, &mut agg).unwrap();
        match event {
            ProviderStreamEvent::Chunk { tool_calls, .. } => {
                assert_eq!(tool_calls.len(), 2);
                assert_eq!(tool_calls[0].index, 0);
                assert_eq!(tool_calls[0].name.as_deref(), Some("get_weather"));
                assert_eq!(tool_calls[1].index, 1);
                assert_eq!(tool_calls[1].name.as_deref(), Some("get_time"));
            }
            _ => panic!("expected Chunk"),
        }
    }
}
