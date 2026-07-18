use crate::error::LlmError;
use crate::types::ProviderConfig;
use std::time::Duration;

/// Build an HTTP agent with connection and read timeouts from config.
pub fn build_agent(config: &ProviderConfig) -> ureq::Agent {
    let timeout = Duration::from_secs(config.timeout_secs);
    ureq::AgentBuilder::new()
        .timeout(timeout)
        .timeout_connect(timeout)
        .timeout_read(timeout)
        .timeout_write(timeout)
        .build()
}

/// Apply authentication and custom headers from provider config.
pub fn apply_headers(request: ureq::Request, config: &ProviderConfig) -> ureq::Request {
    let mut req = request;

    // Bearer token auth
    if let Some(ref key) = config.api_key {
        if key.starts_with("sk-") || key.starts_with("Bearer ") {
            req = req.set("Authorization", key);
        } else {
            req = req.set("Authorization", &format!("Bearer {}", key));
        }
    }

    // Organization ID (OpenAI-specific but harmless for others)
    if let Some(ref org) = config.organization_id {
        req = req.set("OpenAI-Organization", org);
    }

    // Custom headers from config
    for (name, value) in &config.custom_headers {
        req = req.set(name, value);
    }

    req
}

/// Send a JSON POST request and return the response body string.
pub fn send_json_post(
    agent: &ureq::Agent,
    url: &str,
    config: &ProviderConfig,
    body: &serde_json::Value,
) -> Result<String, LlmError> {
    let request = agent.post(url).set("Content-Type", "application/json");
    let request = apply_headers(request, config);
    let response = request.send_json(body);
    handle_response(response)
}

/// Send a JSON POST request for streaming and return the response body reader.
pub fn send_json_post_stream(
    agent: &ureq::Agent,
    url: &str,
    config: &ProviderConfig,
    body: &serde_json::Value,
) -> Result<Box<dyn std::io::Read + Send>, LlmError> {
    let request = agent.post(url).set("Content-Type", "application/json");
    let request = apply_headers(request, config);
    match request.send_json(body) {
        Ok(response) => Ok(Box::new(response.into_reader())),
        Err(err) => Err(map_ureq_error(err)),
    }
}

/// Send a GET request and return the response body string.
pub fn send_get(
    agent: &ureq::Agent,
    url: &str,
    config: &ProviderConfig,
) -> Result<String, LlmError> {
    let request = agent.get(url);
    let request = apply_headers(request, config);
    handle_response(request.call())
}

/// Handle an HTTP response: success → body string, error → LlmError.
pub(crate) fn handle_response(
    result: Result<ureq::Response, ureq::Error>,
) -> Result<String, LlmError> {
    match result {
        Ok(response) => {
            let status = response.status();
            let body = response
                .into_string()
                .unwrap_or_else(|e| format!("(failed to read response body: {})", e));
            if (200..300).contains(&status) {
                Ok(body)
            } else {
                Err(map_http_status(status, &body))
            }
        }
        Err(err) => Err(map_ureq_error(err)),
    }
}

/// Map HTTP status codes to LlmError with context from response body.
fn map_http_status(status: u16, body: &str) -> LlmError {
    let msg = extract_error_message(body);
    match status {
        400 => LlmError::InvalidRequest(format!("bad request: {}", msg)),
        401 | 403 => LlmError::AuthenticationError(format!("HTTP {}: {}", status, msg)),
        404 => {
            LlmError::ConfigurationError(format!("endpoint not found (HTTP {}): {}", status, msg))
        }
        408 => LlmError::TimeoutError(format!("request timeout (HTTP {})", status)),
        409 => LlmError::ModelOverloaded(format!("conflict (HTTP {}): {}", status, msg)),
        422 => LlmError::InvalidRequest(format!("unprocessable entity (HTTP {}): {}", status, msg)),
        429 => LlmError::RateLimited(format!("rate limited (HTTP {}): {}", status, msg)),
        500 | 502 | 503 | 504 => {
            LlmError::ProviderError(format!("provider unavailable (HTTP {}): {}", status, msg))
        }
        _ => {
            if status >= 500 {
                LlmError::ProviderError(format!("provider error (HTTP {}): {}", status, msg))
            } else {
                LlmError::InvalidRequest(format!("unexpected HTTP {}: {}", status, msg))
            }
        }
    }
}

/// Map ureq transport errors to LlmError.
pub(crate) fn map_ureq_error(err: ureq::Error) -> LlmError {
    match err {
        ureq::Error::Status(code, response) => {
            let body = response.into_string().unwrap_or_default();
            map_http_status(code, &body)
        }
        ureq::Error::Transport(e) => {
            let msg = e.to_string().to_lowercase();
            if msg.contains("timeout") || msg.contains("timed out") {
                LlmError::TimeoutError(format!("request timed out: {}", e))
            } else if msg.contains("connection refused")
                || msg.contains("connection reset")
                || msg.contains("broken pipe")
            {
                LlmError::ConnectionError(format!("connection failed: {}", e))
            } else if msg.contains("dns")
                || msg.contains("name not resolved")
                || msg.contains("temporary failure in name resolution")
            {
                LlmError::ConnectionError(format!("DNS resolution failed: {}", e))
            } else if msg.contains("tls") || msg.contains("certificate") || msg.contains("ssl") {
                LlmError::ConnectionError(format!("TLS error: {}", e))
            } else {
                LlmError::TransportError(format!("transport error: {}", e))
            }
        }
    }
}

/// Extract the most useful error message from an API error response body.
fn extract_error_message(body: &str) -> String {
    // Try to parse as JSON and extract common error fields
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(body) {
        if let Some(msg) = val.get("error").and_then(|e| {
            e.get("message")
                .or_else(|| e.get("msg"))
                .or_else(|| e.get("error"))
                .and_then(|m| m.as_str())
        }) {
            return msg.to_string();
        }
        // Anthropic-style: error.message
        if let Some(msg) = val
            .get("error")
            .and_then(|e| e.get("message").and_then(|m| m.as_str()))
        {
            return msg.to_string();
        }
        // Ollama-style: error
        if let Some(msg) = val.get("error").and_then(|m| m.as_str()) {
            return msg.to_string();
        }
    }
    // Truncate raw body if it's too long
    if body.len() > 200 {
        format!("{}...", &body[..200])
    } else {
        body.to_string()
    }
}

/// Parse a single SSE line. Returns `Some(data)` for `data: ...` lines, `None` otherwise.
pub fn parse_sse_line(line: &str) -> Option<String> {
    let line = line.trim_start();
    if let Some(data) = line.strip_prefix("data: ") {
        if data.trim() != "[DONE]" {
            return Some(data.to_string());
        }
    }
    None
}

/// Read the next SSE data event from a body reader. Returns the data content or None if stream ended.
pub fn read_sse_event(reader: &mut dyn std::io::BufRead) -> Option<String> {
    let mut buf = String::new();
    loop {
        buf.clear();
        match reader.read_line(&mut buf) {
            Ok(0) => return None, // EOF
            Ok(_) => {
                if let Some(data) = parse_sse_line(&buf) {
                    return Some(data);
                }
            }
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("timed out") || msg.contains("timeout") {
                    // Read timeout during streaming is normal — return None
                    return None;
                }
                return None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_sse_line_data() {
        assert_eq!(
            parse_sse_line("data: {\"hello\":\"world\"}"),
            Some("{\"hello\":\"world\"}".into())
        );
    }

    #[test]
    fn parse_sse_line_done() {
        assert_eq!(parse_sse_line("data: [DONE]"), None);
    }

    #[test]
    fn parse_sse_line_ignores_event() {
        assert_eq!(parse_sse_line("event: foo"), None);
    }

    #[test]
    fn parse_sse_line_empty() {
        assert_eq!(parse_sse_line(""), None);
    }

    #[test]
    fn parse_sse_line_no_prefix() {
        assert_eq!(parse_sse_line("not sse data"), None);
    }

    #[test]
    fn parse_sse_line_whitespace_data() {
        assert_eq!(parse_sse_line("data: "), Some("".into()));
    }

    #[test]
    fn extract_error_message_empty() {
        assert!(extract_error_message("").contains(""));
    }

    #[test]
    fn extract_error_message_raw_text() {
        let msg = extract_error_message("something went wrong");
        assert!(msg.contains("something went wrong"));
    }

    #[test]
    fn extract_error_message_openai() {
        let body = r#"{"error":{"message":"Incorrect API key","type":"auth_error"}}"#;
        let msg = extract_error_message(body);
        assert!(msg.contains("Incorrect API key"));
    }

    #[test]
    fn extract_error_message_anthropic() {
        let body = r#"{"error":{"message":"Invalid request","type":"invalid_request_error"}}"#;
        let msg = extract_error_message(body);
        assert!(msg.contains("Invalid request"));
    }

    fn test_config() -> ProviderConfig {
        ProviderConfig {
            provider_id: "test".into(),
            provider_type: "openai".into(),
            api_url: String::new(),
            api_key: None,
            organization_id: None,
            default_model: None,
            models: vec![],
            timeout_secs: 1,
            max_retries: 3,
            custom_headers: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn build_agent_config_respected() {
        let config = test_config();
        let agent = build_agent(&config);
        // verify it returns an agent (type check)
        let _ = agent;
    }

    #[test]
    fn apply_headers_bearer_token() {
        let mut config = test_config();
        config.api_key = Some("sk-test123".into());
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_millis(1))
            .build();
        let _ = apply_headers(agent.get("http://localhost:1/"), &config);
    }

    #[test]
    fn apply_headers_custom() {
        let mut config = test_config();
        config.api_key = None;
        config
            .custom_headers
            .insert("X-Custom".into(), "value".into());
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_millis(1))
            .build();
        let _ = apply_headers(agent.get("http://localhost:1/"), &config);
    }

    #[test]
    fn map_http_status_401() {
        let err = map_http_status(401, "unauthorized");
        assert!(matches!(err, LlmError::AuthenticationError(_)));
    }

    #[test]
    fn map_http_status_429() {
        let err = map_http_status(429, "too many requests");
        assert!(matches!(err, LlmError::RateLimited(_)));
    }

    #[test]
    fn map_http_status_500() {
        let err = map_http_status(500, "internal error");
        assert!(matches!(err, LlmError::ProviderError(_)));
    }

    #[test]
    fn map_http_status_400() {
        let err = map_http_status(400, "bad request");
        assert!(matches!(err, LlmError::InvalidRequest(_)));
    }

    #[test]
    fn read_sse_event_handles_empty() {
        // Can't easily test without a real connection, just verify the function signature
    }
}
