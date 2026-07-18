/// Deterministic mock HTTP servers for all 6 providers.
///
/// Each mock simulates the provider's HTTP API with controlled,
/// deterministic responses. No real API calls are made.
use std::io::Read;
///
/// All servers listen on localhost on a randomly assigned port.
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// Simulated behaviours a mock server can exhibit.
#[derive(Clone, Debug, PartialEq)]
pub enum MockBehaviour {
    /// Normal operation — returns valid responses.
    Normal,
    /// Returns HTTP 401 on all requests.
    AuthFailure,
    /// Returns HTTP 429 on all requests.
    RateLimited,
    /// Returns HTTP 500 on all requests.
    ServerError,
    /// Delays response beyond the client timeout.
    Timeout(Duration),
    /// Returns malformed (non-JSON) body.
    MalformedResponse,
    /// First N requests fail, then succeed.
    TransientFailure { fail_count: usize },
    /// Returns a very large response body.
    LargeResponse(usize),
}

/// Controls a mock server's behaviour dynamically.
pub struct MockServer {
    pub port: u16,
    pub behaviour: Arc<std::sync::Mutex<MockBehaviour>>,
    pub request_count: Arc<AtomicUsize>,
    shutdown_tx: std::sync::mpsc::Sender<()>,
    thread_handle: Option<thread::JoinHandle<()>>,
}

impl MockServer {
    /// Start a new mock HTTP server on a random port.
    ///
    /// `provider_type` determines the response format: "openai", "anthropic",
    /// "gemini", "ollama", "http_generic".
    pub fn start(provider_type: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("failed to bind mock server");
        let port = listener.local_addr().unwrap().port();

        let behaviour: Arc<std::sync::Mutex<MockBehaviour>> =
            Arc::new(std::sync::Mutex::new(MockBehaviour::Normal));
        let request_count = Arc::new(AtomicUsize::new(0));
        let (shutdown_tx, shutdown_rx) = std::sync::mpsc::channel::<()>();

        let beh = Arc::clone(&behaviour);
        let rc = Arc::clone(&request_count);
        let provider = provider_type.to_string();

        let handle = thread::Builder::new()
            .name(format!("mock-{provider_type}-{port}"))
            .spawn(move || {
                listener.set_nonblocking(true).ok();
                let mut connections: Vec<TcpStream> = Vec::new();

                loop {
                    // Check shutdown
                    if shutdown_rx.try_recv().is_ok() {
                        break;
                    }

                    // Accept new connections (non-blocking)
                    match listener.accept() {
                        Ok((stream, _)) => {
                            stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
                            stream.set_write_timeout(Some(Duration::from_secs(5))).ok();
                            connections.push(stream);
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                        Err(_) => break,
                    }

                    // Process each connection
                    let mut i = 0;
                    while i < connections.len() {
                        let stream = &connections[i];
                        let mut buf_reader = BufReader::new(stream.try_clone().unwrap());
                        let mut request_line = String::new();

                        match buf_reader.read_line(&mut request_line) {
                            Ok(0) | Err(_) => {
                                connections.swap_remove(i);
                                continue;
                            }
                            Ok(_) => {
                                // Read headers until blank line
                                let mut headers = String::new();
                                loop {
                                    let mut line = String::new();
                                    match buf_reader.read_line(&mut line) {
                                        Ok(0) | Err(_) => break,
                                        Ok(_) => {
                                            if line.trim().is_empty() {
                                                break;
                                            }
                                            headers.push_str(&line);
                                        }
                                    }
                                }

                                // Read body if Content-Length is present
                                let mut body = String::new();
                                if let Some(clen_str) = headers
                                    .lines()
                                    .find(|l| l.to_lowercase().starts_with("content-length:"))
                                {
                                    if let Some(len_str) = clen_str.split(':').nth(1) {
                                        if let Ok(len) = len_str.trim().parse::<usize>() {
                                            if len > 0 && len < 1_000_000 {
                                                let mut buf = vec![0u8; len];
                                                buf_reader.read_exact(&mut buf).ok();
                                                body = String::from_utf8_lossy(&buf).to_string();
                                            }
                                        }
                                    }
                                }

                                rc.fetch_add(1, Ordering::SeqCst);
                                let current_behaviour = beh.lock().unwrap().clone();

                                // Build response
                                let (status_line, response_body) = match &current_behaviour {
                                    MockBehaviour::Normal => {
                                        let resp =
                                            build_normal_response(&provider, &request_line, &body);
                                        ("HTTP/1.1 200 OK\r\n".to_string(), resp)
                                    }
                                    MockBehaviour::AuthFailure => {
                                        let resp = build_error_response(&provider, 401);
                                        ("HTTP/1.1 401 Unauthorized\r\n".to_string(), resp)
                                    }
                                    MockBehaviour::RateLimited => {
                                        let resp = build_error_response(&provider, 429);
                                        ("HTTP/1.1 429 Too Many Requests\r\n".to_string(), resp)
                                    }
                                    MockBehaviour::ServerError => {
                                        let resp = build_error_response(&provider, 500);
                                        ("HTTP/1.1 500 Internal Server Error\r\n".to_string(), resp)
                                    }
                                    MockBehaviour::Timeout(_) => {
                                        // Don't respond — let the client timeout
                                        thread::sleep(Duration::from_secs(10));
                                        continue;
                                    }
                                    MockBehaviour::MalformedResponse => {
                                        (
                                            "HTTP/1.1 200 OK\r\n".to_string(),
                                            "this is not valid json".to_string(),
                                        )
                                    }
                                    MockBehaviour::TransientFailure { fail_count } => {
                                        let count = rc.load(Ordering::SeqCst);
                                        if count <= *fail_count {
                                            let resp = build_error_response(&provider, 500);
                                            (
                                                "HTTP/1.1 500 Internal Server Error\r\n"
                                                    .to_string(),
                                                resp,
                                            )
                                        } else {
                                            let resp = build_normal_response(
                                                &provider, &request_line, &body,
                                            );
                                            ("HTTP/1.1 200 OK\r\n".to_string(), resp)
                                        }
                                    }
                                    MockBehaviour::LargeResponse(size) => {
                                        let resp = build_large_response(&provider, *size);
                                        ("HTTP/1.1 200 OK\r\n".to_string(), resp)
                                    }
                                };

                                let response = format!(
                                    "{status_line}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                                    response_body.len(),
                                    response_body
                                );

                                // Write response to the stream
                                if let Ok(mut stream) = stream.try_clone() {
                                    stream.write_all(response.as_bytes()).ok();
                                    stream.flush().ok();
                                }
                            }
                        }
                        i += 1;
                    }

                    thread::sleep(Duration::from_millis(10));
                }
            })
            .expect("failed to spawn mock server thread");

        Self {
            port,
            behaviour,
            request_count,
            shutdown_tx,
            thread_handle: Some(handle),
        }
    }

    /// Change the behaviour for subsequent requests.
    pub fn set_behaviour(&self, b: MockBehaviour) {
        *self.behaviour.lock().unwrap() = b;
    }

    /// Reset request counter.
    pub fn reset_count(&self) {
        self.request_count.store(0, Ordering::SeqCst);
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        self.shutdown_tx.send(()).ok();
        if let Some(handle) = self.thread_handle.take() {
            handle.thread().unpark();
            handle.join().ok();
        }
    }
}

/// Build a normal response for the given provider type.
fn build_normal_response(provider_type: &str, request_line: &str, body: &str) -> String {
    match provider_type {
        "openai" => build_openai_response(request_line, body),
        "anthropic" => build_anthropic_response(request_line, body),
        "gemini" => build_gemini_response(request_line, body),
        "ollama" => build_ollama_response(request_line, body),
        "generic" | "http_generic" => build_openai_response(request_line, body),
        _ => serde_json::json!({"error": "unknown provider"}).to_string(),
    }
}

fn build_openai_response(request_line: &str, body: &str) -> String {
    let is_stream = body.contains("\"stream\": true") || body.contains("\"stream\":true");

    // Handle streaming differently (SSE)
    if is_stream {
        // For streaming, return the first SSE event
        return format!(
            "data: {{\"choices\":[{{\"index\":0,\"delta\":{{\"content\":\"Hello\"}},\"finish_reason\":null}}]}}\n\ndata: {{\"choices\":[{{\"index\":0,\"delta\":{{\"content\":\" world\"}},\"finish_reason\":null}}]}}\n\ndata: {{\"choices\":[{{\"index\":0,\"delta\":{{}},\"finish_reason\":\"stop\"}}],\"usage\":{{\"prompt_tokens\":10,\"completion_tokens\":5}}}}\n\ndata: [DONE]\n\n"
        );
    }

    // Check if body has tool definitions
    let has_tools = body.contains("\"tools\"");

    if has_tools {
        return serde_json::json!({
            "id": "chatcmpl-mock",
            "object": "chat.completion",
            "created": 1234567890,
            "model": "mock-model",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_mock_1",
                        "type": "function",
                        "function": {
                            "name": "get_weather",
                            "arguments": "{\"location\":\"NYC\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {"prompt_tokens": 15, "completion_tokens": 8, "total_tokens": 23}
        })
        .to_string();
    }

    // Check for embedding endpoint
    if request_line.contains("/embeddings") {
        return serde_json::json!({
            "object": "list",
            "data": [{
                "object": "embedding",
                "index": 0,
                "embedding": [0.1, 0.2, 0.3, 0.4, 0.5]
            }],
            "model": "text-embedding-3-small",
            "usage": {"prompt_tokens": 5, "total_tokens": 5}
        })
        .to_string();
    }

    // Default chat completion response
    serde_json::json!({
        "id": "chatcmpl-mock",
        "object": "chat.completion",
        "created": 1234567890,
        "model": "mock-model",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "Hello from OpenAI mock! This is a deterministic response."
            },
            "finish_reason": "stop"
        }],
        "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
    })
    .to_string()
}

fn build_anthropic_response(_request_line: &str, body: &str) -> String {
    let is_stream = body.contains("\"stream\": true") || body.contains("\"stream\":true");

    if is_stream {
        return format!(
            "event: message_start\ndata: {{\"type\":\"message_start\",\"message\":{{\"id\":\"msg_mock\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-3\",\"stop_reason\":null,\"usage\":{{\"input_tokens\":10,\"output_tokens\":0}}}}}}\n\nevent: content_block_start\ndata: {{\"type\":\"content_block_start\",\"index\":0,\"content_block\":{{\"type\":\"text\",\"text\":\"Hello\"}}}}\n\nevent: content_block_delta\ndata: {{\"type\":\"content_block_delta\",\"index\":0,\"delta\":{{\"type\":\"text_delta\",\"text\":\" from Anthropic mock!\"}}}}\n\nevent: content_block_stop\ndata: {{\"type\":\"content_block_stop\",\"index\":0}}\n\nevent: message_delta\ndata: {{\"type\":\"message_delta\",\"delta\":{{\"stop_reason\":\"end_turn\"}},\"usage\":{{\"input_tokens\":10,\"output_tokens\":5}}}}\n\nevent: message_stop\ndata: {{\"type\":\"message_stop\"}}\n\n"
        );
    }

    let has_tools = body.contains("\"tools\"");

    if has_tools {
        return serde_json::json!({
            "id": "msg_mock",
            "type": "message",
            "role": "assistant",
            "content": [
                {"type": "text", "text": "Let me check the weather for you."},
                {"type": "tool_use", "id": "tu_mock_1", "name": "get_weather", "input": {"location": "NYC"}}
            ],
            "model": "claude-3-haiku",
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 15, "output_tokens": 10}
        }).to_string();
    }

    serde_json::json!({
        "id": "msg_mock",
        "type": "message",
        "role": "assistant",
        "content": [{"type": "text", "text": "Hello from Anthropic mock! This is a deterministic response."}],
        "model": "claude-3-haiku",
        "stop_reason": "end_turn",
        "usage": {"input_tokens": 10, "output_tokens": 5}
    }).to_string()
}

fn build_gemini_response(request_line: &str, body: &str) -> String {
    if request_line.contains("streamGenerateContent") {
        return format!(
            "data: {{\"candidates\":[{{\"content\":{{\"parts\":[{{\"text\":\"Hello \"}}],\"role\":\"model\"}},\"finishReason\":null}}]}}\n\ndata: {{\"candidates\":[{{\"content\":{{\"parts\":[{{\"text\":\"from Gemini mock!\"}}],\"role\":\"model\"}},\"finishReason\":\"STOP\"}}]}}\n\n"
        );
    }

    let has_tools = body.contains("\"tools\"");

    if has_tools {
        return serde_json::json!({
            "candidates": [{
                "content": {
                    "parts": [
                        {"text": "Let me check the weather for you."},
                        {"functionCall": {"name": "get_weather", "args": {"location": "NYC"}}}
                    ],
                    "role": "model"
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {"promptTokenCount": 10, "candidatesTokenCount": 5}
        })
        .to_string();
    }

    serde_json::json!({
        "candidates": [{
            "content": {
                "parts": [{"text": "Hello from Gemini mock! This is a deterministic response."}],
                "role": "model"
            },
            "finishReason": "STOP"
        }],
        "usageMetadata": {"promptTokenCount": 10, "candidatesTokenCount": 5}
    })
    .to_string()
}

fn build_ollama_response(request_line: &str, body: &str) -> String {
    if request_line.contains("/api/embed") {
        return serde_json::json!({
            "model": "llama3.2",
            "embeddings": [[0.1, 0.2, 0.3, 0.4, 0.5]]
        })
        .to_string();
    }

    let is_stream = body.contains("\"stream\": true") || body.contains("\"stream\":true");

    if is_stream {
        return format!(
            "{{\"model\":\"llama3.2\",\"created_at\":\"2024-01-01T00:00:00Z\",\"message\":{{\"role\":\"assistant\",\"content\":\"Hello \"}},\"done\":false}}\n{{\"model\":\"llama3.2\",\"created_at\":\"2024-01-01T00:00:01Z\",\"message\":{{\"role\":\"assistant\",\"content\":\"from Ollama mock!\"}},\"done\":false}}\n{{\"model\":\"llama3.2\",\"created_at\":\"2024-01-01T00:00:02Z\",\"message\":{{\"role\":\"assistant\",\"content\":\"\"}},\"done\":true,\"total_duration\":1000000,\"prompt_eval_count\":10,\"eval_count\":5}}\n"
        );
    }

    serde_json::json!({
        "model": "llama3.2",
        "created_at": "2024-01-01T00:00:00Z",
        "message": {"role": "assistant", "content": "Hello from Ollama mock! This is a deterministic response."},
        "done": true,
        "total_duration": 1000000,
        "prompt_eval_count": 10,
        "eval_count": 5
    }).to_string()
}

fn build_error_response(provider_type: &str, status: u16) -> String {
    match provider_type {
        "openai" => serde_json::json!({
            "error": {
                "message": format!("HTTP {}: simulated error", status),
                "type": "api_error",
                "code": status
            }
        })
        .to_string(),
        "anthropic" => serde_json::json!({
            "error": {
                "message": format!("HTTP {}: simulated error", status),
                "type": "api_error"
            }
        })
        .to_string(),
        "gemini" => serde_json::json!({
            "error": {
                "code": status,
                "message": format!("HTTP {}: simulated error", status),
                "status": "ERROR"
            }
        })
        .to_string(),
        "ollama" => serde_json::json!({
            "error": format!("HTTP {}: simulated error", status)
        })
        .to_string(),
        _ => format!("{{ \"error\": \"HTTP {}: simulated error\" }}", status),
    }
}

fn build_large_response(provider_type: &str, size: usize) -> String {
    let content = "A".repeat(size.saturating_sub(200).min(100_000));
    match provider_type {
        "openai" => serde_json::json!({
            "id": "chatcmpl-mock",
            "object": "chat.completion",
            "created": 1234567890,
            "model": "mock-model",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": content},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5000, "total_tokens": 5010}
        })
        .to_string(),
        _ => format!("{{\"content\": \"{}\"}}", content),
    }
}
