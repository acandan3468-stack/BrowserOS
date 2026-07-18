//! LLM Gateway error types.

use std::fmt;

/// Errors that can occur during LLM Gateway operations.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum LlmError {
    // Provider errors
    ProviderError(String),
    ProviderUnavailable(String),
    ProviderRateLimited { retry_after_ms: u64 },
    ProviderAuthFailed(String),
    RateLimited(String),
    AuthenticationError(String),
    ModelOverloaded(String),

    // Request errors
    InvalidRequest(String),
    ContextTooLong { current: usize, max: usize },
    ModelNotFound(String),
    CapabilityNotSupported(String),

    // Response errors
    EmptyResponse,
    ContentFiltered,
    MalformedResponse(String),

    // System errors
    Timeout { elapsed_ms: u64 },
    TimeoutError(String),
    AllProvidersFailed { attempts: Vec<String> },
    CacheError(String),
    ConfigurationError(String),
    TransportError(String),
    ConnectionError(String),
    StreamingUnsupported,
}

impl fmt::Display for LlmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LlmError::ProviderError(msg) => write!(f, "provider error: {}", msg),
            LlmError::ProviderUnavailable(msg) => write!(f, "provider unavailable: {}", msg),
            LlmError::ProviderRateLimited { retry_after_ms } => {
                write!(f, "rate limited, retry after {}ms", retry_after_ms)
            }
            LlmError::ProviderAuthFailed(msg) => write!(f, "authentication failed: {}", msg),
            LlmError::RateLimited(msg) => write!(f, "rate limited: {}", msg),
            LlmError::AuthenticationError(msg) => write!(f, "authentication error: {}", msg),
            LlmError::ModelOverloaded(msg) => write!(f, "model overloaded: {}", msg),
            LlmError::InvalidRequest(msg) => write!(f, "invalid request: {}", msg),
            LlmError::ContextTooLong { current, max } => {
                write!(f, "context too long: {} tokens (max {})", current, max)
            }
            LlmError::ModelNotFound(id) => write!(f, "model not found: {}", id),
            LlmError::CapabilityNotSupported(cap) => {
                write!(f, "capability not supported: {}", cap)
            }
            LlmError::EmptyResponse => write!(f, "empty response from provider"),
            LlmError::ContentFiltered => write!(f, "content filtered by provider"),
            LlmError::MalformedResponse(msg) => write!(f, "malformed response: {}", msg),
            LlmError::Timeout { elapsed_ms } => write!(f, "timeout after {}ms", elapsed_ms),
            LlmError::TimeoutError(msg) => write!(f, "timeout: {}", msg),
            LlmError::AllProvidersFailed { attempts } => {
                write!(f, "all providers failed: {} attempts", attempts.len())
            }
            LlmError::CacheError(msg) => write!(f, "cache error: {}", msg),
            LlmError::ConfigurationError(msg) => write!(f, "configuration error: {}", msg),
            LlmError::TransportError(msg) => write!(f, "transport error: {}", msg),
            LlmError::ConnectionError(msg) => write!(f, "connection error: {}", msg),
            LlmError::StreamingUnsupported => write!(f, "streaming not supported by provider"),
        }
    }
}

impl std::error::Error for LlmError {}

impl LlmError {
    /// Returns `true` if the error is retryable (network issues, rate limits, server errors).
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            LlmError::ProviderRateLimited { .. }
                | LlmError::ProviderUnavailable(_)
                | LlmError::Timeout { .. }
                | LlmError::TimeoutError(_)
                | LlmError::TransportError(_)
                | LlmError::ConnectionError(_)
                | LlmError::ProviderError(_)
                | LlmError::RateLimited(_)
        )
    }

    /// Returns `true` if the error should trigger fallback to another provider.
    pub fn is_fallback_trigger(&self) -> bool {
        matches!(
            self,
            LlmError::ProviderUnavailable(_)
                | LlmError::Timeout { .. }
                | LlmError::TimeoutError(_)
                | LlmError::TransportError(_)
                | LlmError::ConnectionError(_)
        )
    }

    /// Returns `true` if the error is fatal (no retry, no fallback).
    pub fn is_fatal(&self) -> bool {
        matches!(
            self,
            LlmError::InvalidRequest(_)
                | LlmError::ContextTooLong { .. }
                | LlmError::ModelNotFound(_)
                | LlmError::CapabilityNotSupported(_)
                | LlmError::ProviderAuthFailed(_)
                | LlmError::AuthenticationError(_)
                | LlmError::ConfigurationError(_)
        )
    }

    /// Redact sensitive patterns (API keys, tokens) from an error message.
    pub fn sanitize(msg: &str) -> String {
        let mut result = String::with_capacity(msg.len());
        let bytes = msg.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if i + 3 < bytes.len()
                && (bytes[i] == b's' || bytes[i] == b'S')
                && (bytes[i + 1] == b'k' || bytes[i + 1] == b'K')
                && bytes[i + 2] == b'-'
            {
                result.push_str("[KEY_REDACTED]");
                while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                continue;
            }
            if i + 7 < bytes.len() && bytes[i..i + 7].eq_ignore_ascii_case(b"bearer ") {
                result.push_str("Bearer [KEY_REDACTED]");
                i += 7;
                while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                continue;
            }
            result.push(bytes[i] as char);
            i += 1;
        }
        result
    }
}

// ────────────────────────────────────────────────────────────────────────────
// From implementations for external error types
// ────────────────────────────────────────────────────────────────────────────

impl From<serde_json::Error> for LlmError {
    fn from(e: serde_json::Error) -> Self {
        LlmError::MalformedResponse(format!("JSON error: {}", e))
    }
}

impl From<std::sync::mpsc::RecvError> for LlmError {
    fn from(_: std::sync::mpsc::RecvError) -> Self {
        LlmError::TransportError("channel disconnected".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_provider_error() {
        let err = LlmError::ProviderError("rate limit".into());
        assert!(err.to_string().contains("rate limit"));
    }

    #[test]
    fn display_rate_limited() {
        let err = LlmError::ProviderRateLimited {
            retry_after_ms: 5000,
        };
        assert!(err.to_string().contains("5000"));
    }

    #[test]
    fn display_model_not_found() {
        let err = LlmError::ModelNotFound("gpt-5".into());
        assert!(err.to_string().contains("gpt-5"));
    }

    #[test]
    fn retryable_classification() {
        assert!(LlmError::ProviderRateLimited {
            retry_after_ms: 1000
        }
        .is_retryable());
        assert!(LlmError::ProviderUnavailable("down".into()).is_retryable());
        assert!(LlmError::Timeout { elapsed_ms: 5000 }.is_retryable());
        assert!(LlmError::TransportError("conn".into()).is_retryable());
        assert!(!LlmError::ProviderAuthFailed("bad key".into()).is_retryable());
        assert!(!LlmError::ConfigurationError("bad".into()).is_retryable());
    }

    #[test]
    fn fallback_trigger() {
        assert!(LlmError::ProviderUnavailable("down".into()).is_fallback_trigger());
        assert!(LlmError::Timeout { elapsed_ms: 5000 }.is_fallback_trigger());
        assert!(!LlmError::ProviderRateLimited {
            retry_after_ms: 1000
        }
        .is_fallback_trigger());
        assert!(!LlmError::ModelNotFound("x".into()).is_fallback_trigger());
    }

    #[test]
    fn fatal_classification() {
        assert!(LlmError::InvalidRequest("bad".into()).is_fatal());
        assert!(LlmError::ContextTooLong {
            current: 10,
            max: 5
        }
        .is_fatal());
        assert!(LlmError::ProviderAuthFailed("bad key".into()).is_fatal());
        assert!(!LlmError::ProviderUnavailable("down".into()).is_fatal());
    }

    #[test]
    fn error_std_error_trait() {
        use std::error::Error;
        let err = LlmError::EmptyResponse;
        let _: &dyn Error = &err;
    }

    #[test]
    fn sanitize_redacts_openai_key() {
        let msg = "API key: sk-proj-abc123def456";
        let sanitized = LlmError::sanitize(msg);
        assert!(!sanitized.contains("sk-proj-abc123def456"));
        assert!(sanitized.contains("[KEY_REDACTED]"));
    }

    #[test]
    fn sanitize_redacts_bearer_token() {
        let msg = "Authorization: Bearer sk-test-key-here";
        let sanitized = LlmError::sanitize(msg);
        assert!(!sanitized.contains("sk-test-key-here"));
        assert!(sanitized.contains("[KEY_REDACTED]"));
    }

    #[test]
    fn sanitize_preserves_normal_text() {
        let msg = "Normal error message without keys";
        let sanitized = LlmError::sanitize(msg);
        assert_eq!(sanitized, msg);
    }

    #[test]
    fn from_serde_json_error() {
        let json_err = serde_json::from_str::<i32>("not-a-number").unwrap_err();
        let llm_err: LlmError = json_err.into();
        assert!(matches!(llm_err, LlmError::MalformedResponse(_)));
    }

    #[test]
    fn error_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<LlmError>();
        assert_sync::<LlmError>();
    }
}
