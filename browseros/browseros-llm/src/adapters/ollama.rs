use crate::error::LlmError;
use crate::provider::LlProvider;
use crate::types::{
    LlFinishReason, ProviderCapability, ProviderEmbedRequest, ProviderEmbedResponse,
    ProviderHealthResult, ProviderMessage, ProviderRequest, ProviderResponse, ProviderStream,
    ProviderStreamEvent,
};
use std::io::BufRead;
use std::time::SystemTime;

pub struct OllamaAdapter {
    config: crate::types::ProviderConfig,
    agent: ureq::Agent,
}

impl OllamaAdapter {
    pub fn new(config: crate::types::ProviderConfig) -> Self {
        let agent = super::http::build_agent(&config);
        Self { config, agent }
    }

    fn api_url(&self) -> String {
        if !self.config.api_url.is_empty() {
            self.config.api_url.trim_end_matches('/').to_string()
        } else {
            "http://localhost:11434".to_string()
        }
    }

    fn chat_url(&self) -> String {
        format!("{}/api/chat", self.api_url())
    }

    fn embed_url(&self) -> String {
        format!("{}/api/embed", self.api_url())
    }

    fn tags_url(&self) -> String {
        format!("{}/api/tags", self.api_url())
    }

    fn build_chat_body(&self, request: &ProviderRequest) -> serde_json::Value {
        let messages: Vec<serde_json::Value> = request
            .messages
            .iter()
            .map(|msg| {
                let role = msg.role.as_str();
                let content = match &msg.content {
                    crate::types::LlContent::Text(text) => text.clone(),
                    crate::types::LlContent::ToolResult { call_id, output } => {
                        format!("[tool_result {}] {}", call_id, output)
                    }
                    crate::types::LlContent::ToolCall {
                        call_id,
                        name,
                        arguments,
                    } => {
                        format!(
                            "[tool_call {}: {} with {}]",
                            call_id,
                            name,
                            serde_json::to_string(arguments).unwrap_or_default()
                        )
                    }
                    crate::types::LlContent::Image { .. } => "[image]".to_string(),
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
            "stream": request.stream,
        });

        if let Some(temp) = request.temperature {
            body["options"] = serde_json::json!({"temperature": temp});
        }

        body
    }

    fn parse_chat_response(&self, body: &str) -> Result<ProviderResponse, LlmError> {
        let val: serde_json::Value =
            serde_json::from_str(body).map_err(|e| LlmError::MalformedResponse(e.to_string()))?;

        let message = val["message"]
            .as_object()
            .ok_or_else(|| LlmError::MalformedResponse("missing message".into()))?;

        let content = message
            .get("content")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();

        let finish_reason = match val["done_reason"].as_str() {
            Some("stop") => LlFinishReason::Stop,
            Some("length") => LlFinishReason::Length,
            Some("tool_calls") => LlFinishReason::ToolCalls,
            _ => LlFinishReason::Stop,
        };

        let input_tokens = val
            .get("prompt_eval_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let output_tokens = val.get("eval_count").and_then(|v| v.as_u64()).unwrap_or(0);
        let model = val["model"].as_str().unwrap_or("").to_string();

        Ok(ProviderResponse {
            message: ProviderMessage {
                role: crate::types::LlRole::Assistant,
                content: crate::types::LlContent::Text(content),
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

        if val.get("done").and_then(|d| d.as_bool()).unwrap_or(false) {
            let input_tokens = val
                .get("prompt_eval_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let output_tokens = val.get("eval_count").and_then(|v| v.as_u64()).unwrap_or(0);
            return Ok(ProviderStreamEvent::Done {
                input_tokens,
                output_tokens,
            });
        }

        let content = val["message"]["content"].as_str().unwrap_or("").to_string();

        Ok(ProviderStreamEvent::Chunk {
            content,
            finish_reason: None,
            tool_calls: vec![],
        })
    }

    fn parse_embed_response(&self, body: &str) -> Result<ProviderEmbedResponse, LlmError> {
        let val: serde_json::Value =
            serde_json::from_str(body).map_err(|e| LlmError::MalformedResponse(e.to_string()))?;

        let mut embeddings = Vec::new();
        if let Some(embeds) = val["embeddings"].as_array() {
            for embed in embeds {
                if let Some(vec) = embed.as_array() {
                    let v: Vec<f32> = vec
                        .iter()
                        .filter_map(|x| x.as_f64().map(|f| f as f32))
                        .collect();
                    embeddings.push(v);
                }
            }
        }

        let input_tokens = val
            .get("prompt_eval_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let model = val["model"].as_str().unwrap_or("").to_string();

        Ok(ProviderEmbedResponse {
            embeddings,
            input_tokens,
            model,
        })
    }
}

impl LlProvider for OllamaAdapter {
    fn id(&self) -> &str {
        &self.config.provider_id
    }

    fn name(&self) -> &str {
        "Ollama"
    }

    fn capabilities(&self) -> Vec<ProviderCapability> {
        vec![
            ProviderCapability::Chat,
            ProviderCapability::Streaming,
            ProviderCapability::Embedding,
        ]
    }

    fn models(&self) -> Vec<String> {
        let url = self.tags_url();
        match super::http::send_get(&self.agent, &url, &self.config) {
            Ok(body) => {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&body) {
                    if let Some(models) = val["models"].as_array() {
                        return models
                            .iter()
                            .filter_map(|m| m["name"].as_str().map(|s| s.to_string()))
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
            let mut buf = String::new();
            loop {
                buf.clear();
                match reader.read_line(&mut buf) {
                    Ok(_) => {
                        let line = buf.trim().to_string();
                        if line.is_empty() {
                            continue;
                        }
                        match Self::parse_chunk(&line) {
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
                        }
                    }
                    Err(_) => break,
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
        match super::http::send_get(&self.agent, &self.tags_url(), &self.config) {
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
    use crate::types::LlContent;

    fn test_config() -> crate::types::ProviderConfig {
        crate::types::ProviderConfig {
            provider_id: "ollama".into(),
            provider_type: "ollama".into(),
            api_url: "http://localhost:11434".into(),
            api_key: None,
            organization_id: None,
            default_model: Some("llama3".into()),
            models: vec!["llama3".into(), "mistral".into()],
            timeout_secs: 30,
            max_retries: 3,
            custom_headers: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn adapter_identity() {
        let adapter = OllamaAdapter::new(test_config());
        assert_eq!(adapter.id(), "ollama");
        assert_eq!(adapter.name(), "Ollama");
    }

    #[test]
    fn capabilities_include_chat_and_embed() {
        let adapter = OllamaAdapter::new(test_config());
        let caps = adapter.capabilities();
        assert!(caps.contains(&ProviderCapability::Chat));
        assert!(caps.contains(&ProviderCapability::Embedding));
    }

    #[test]
    fn parse_chat_response_simple() {
        let adapter = OllamaAdapter::new(test_config());
        let body = r#"{
            "model": "llama3",
            "created_at": "2024-01-01T00:00:00Z",
            "message": {"role": "assistant", "content": "Hello!"},
            "done": true,
            "done_reason": "stop",
            "prompt_eval_count": 10,
            "eval_count": 5
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
    fn parse_chunk_done() {
        let data = r#"{"model":"llama3","message":{"role":"assistant","content":""},"done":true,"done_reason":"stop","prompt_eval_count":10,"eval_count":5}"#;
        let event = OllamaAdapter::parse_chunk(data).unwrap();
        match event {
            ProviderStreamEvent::Done {
                input_tokens,
                output_tokens,
            } => {
                assert_eq!(input_tokens, 10);
                assert_eq!(output_tokens, 5);
            }
            _ => panic!("expected Done"),
        }
    }

    #[test]
    fn parse_chunk_content() {
        let data =
            r#"{"model":"llama3","message":{"role":"assistant","content":"Hello"},"done":false}"#;
        let event = OllamaAdapter::parse_chunk(data).unwrap();
        match event {
            ProviderStreamEvent::Chunk { content, .. } => {
                assert_eq!(content, "Hello");
            }
            _ => panic!("expected Chunk"),
        }
    }

    #[test]
    fn parse_chunk_invalid_json() {
        let err = OllamaAdapter::parse_chunk("not json");
        assert!(err.is_err());
    }

    #[test]
    fn parse_embed_response_simple() {
        let adapter = OllamaAdapter::new(test_config());
        let body = r#"{"model":"llama3","embeddings":[[0.1,0.2,0.3]],"prompt_eval_count":5}"#;
        let resp = adapter.parse_embed_response(body).unwrap();
        assert_eq!(resp.embeddings.len(), 1);
        assert_eq!(resp.embeddings[0].len(), 3);
        assert_eq!(resp.input_tokens, 5);
    }

    #[test]
    fn health_returns_unavailable_when_offline() {
        let mut cfg = test_config();
        cfg.api_url = "http://127.0.0.1:1".into();
        cfg.timeout_secs = 1;
        let adapter = OllamaAdapter::new(cfg);
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
        assert_send::<OllamaAdapter>();
        assert_sync::<OllamaAdapter>();
    }

    // ─── tool_calls streaming ───────────────────────────────────────────────

    #[test]
    fn parse_chunk_content_with_empty_tool_calls() {
        let data =
            r#"{"model":"llama3","message":{"role":"assistant","content":"Hello"},"done":false}"#;
        let event = OllamaAdapter::parse_chunk(data).unwrap();
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
    fn parse_chunk_done_emits_no_tool_calls() {
        let data = r#"{"model":"llama3","message":{"role":"assistant","content":""},"done":true,"done_reason":"stop","prompt_eval_count":10,"eval_count":5}"#;
        let event = OllamaAdapter::parse_chunk(data).unwrap();
        match event {
            ProviderStreamEvent::Done { .. } => {}
            _ => panic!("expected Done"),
        }
    }

    #[test]
    fn parse_chunk_all_chunks_have_empty_tool_calls() {
        let chunks = vec![
            r#"{"model":"llama3","message":{"role":"assistant","content":"Hello"},"done":false}"#,
            r#"{"model":"llama3","message":{"role":"assistant","content":" world"},"done":false}"#,
            r#"{"model":"llama3","message":{"role":"assistant","content":"!"},"done":true,"done_reason":"stop","prompt_eval_count":10,"eval_count":5}"#,
        ];
        for chunk in &chunks {
            let event = OllamaAdapter::parse_chunk(chunk).unwrap();
            match event {
                ProviderStreamEvent::Chunk { tool_calls, .. } => {
                    assert!(
                        tool_calls.is_empty(),
                        "every Ollama Chunk must have empty tool_calls"
                    );
                }
                ProviderStreamEvent::Done { .. } => {}
                _ => panic!("unexpected event"),
            }
        }
    }
}
