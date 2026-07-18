use crate::value::ErrorCode;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fmt;

/// Classification of an error for handling decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorKind {
    /// Retryable — the operation may succeed on a subsequent attempt.
    Transient,
    /// Will never succeed without changing the input or state.
    Permanent,
    /// User action or configuration change is needed.
    Configuration,
    /// Resource exhaustion — scale capacity or reduce load.
    Resource,
    /// Unexpected invariant violation — likely a bug.
    Internal,
    /// The requested operation is not supported in this context.
    Unsupported,
}

/// Error raised by builder methods when required fields are missing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuilderError {
    /// The `source` field is required but was not set.
    MissingSource,
    /// The `content_type` field is required but was not set.
    MissingContentType,
    /// The `payload` field is required but was not set.
    MissingPayload,
}

impl fmt::Display for BuilderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BuilderError::MissingSource => write!(f, "builder: source is required"),
            BuilderError::MissingContentType => write!(f, "builder: content_type is required"),
            BuilderError::MissingPayload => write!(f, "builder: payload is required"),
        }
    }
}

impl std::error::Error for BuilderError {}

/// Error severity for logging and alerting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorSeverity {
    Debug,
    Info,
    Warning,
    Error,
    Critical,
}

impl ErrorSeverity {
    /// Returns `true` when the severity is at or above the reportable threshold.
    pub fn is_reportable(&self) -> bool {
        matches!(self, ErrorSeverity::Error | ErrorSeverity::Critical)
    }
}

impl PartialOrd for ErrorSeverity {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ErrorSeverity {
    fn cmp(&self, other: &Self) -> Ordering {
        fn rank(s: ErrorSeverity) -> u8 {
            match s {
                ErrorSeverity::Debug => 0,
                ErrorSeverity::Info => 1,
                ErrorSeverity::Warning => 2,
                ErrorSeverity::Error => 3,
                ErrorSeverity::Critical => 4,
            }
        }
        rank(*self).cmp(&rank(*other))
    }
}

/// Context entry for error chains.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorContext {
    pub message: String,
    pub module: String,
    pub file: String,
    pub line: u32,
}

/// Retry policy for transient errors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RetryPolicy {
    /// Retry immediately up to `max_retries` times.
    Immediate { max_retries: u32 },
    /// Exponential backoff with optional jitter.
    ExponentialBackoff {
        initial_delay_ms: u64,
        max_delay_ms: u64,
        multiplier: f64,
        max_retries: u32,
        jitter: bool,
    },
}

impl RetryPolicy {
    /// Returns `Some(duration)` to wait before the next attempt, or `None` if
    /// `max_retries` has been reached.
    pub fn next_delay(&self, attempt: u32) -> Option<std::time::Duration> {
        let max = match self {
            RetryPolicy::Immediate { max_retries } => *max_retries,
            RetryPolicy::ExponentialBackoff { max_retries, .. } => *max_retries,
        };
        if attempt >= max {
            return None;
        }
        match self {
            RetryPolicy::Immediate { .. } => Some(std::time::Duration::ZERO),
            RetryPolicy::ExponentialBackoff {
                initial_delay_ms,
                max_delay_ms,
                multiplier,
                max_retries: _,
                jitter: _,
            } => {
                let delay = (*initial_delay_ms as f64) * multiplier.powi(attempt as i32);
                let delay = (delay as u64).min(*max_delay_ms);
                Some(std::time::Duration::from_millis(delay))
            }
        }
    }
}

/// The canonical BrowserOS error type.
///
/// Carries classification, severity, an error code, human-readable message,
/// chained context entries, an optional source error, and an optional recovery
/// hint.  Builder methods allow ergonomic construction:
///
/// ```ignore
/// BrowserOsError::transient("network timeout")
///     .with_code(ErrorCode::Timeout)
///     .with_severity(ErrorSeverity::Warning)
///     .with_context(error_context!("connection dropped"))
///     .with_source(io_err)
///     .with_recovery("check your network connection");
/// ```
#[derive(Debug)]
pub struct BrowserOsError {
    pub kind: ErrorKind,
    pub severity: ErrorSeverity,
    pub code: ErrorCode,
    pub message: String,
    pub context: Vec<ErrorContext>,
    pub source: Option<Box<dyn std::error::Error + Send + Sync>>,
    pub recovery_hint: Option<String>,
}

impl BrowserOsError {
    /// Creates a new error with the given kind and message.
    ///
    /// Defaults to [`ErrorSeverity::Error`] and `ErrorCode::new("UNKNOWN")`.
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        BrowserOsError {
            kind,
            severity: ErrorSeverity::Error,
            code: ErrorCode::new("UNKNOWN"),
            message: message.into(),
            context: Vec::new(),
            source: None,
            recovery_hint: None,
        }
    }

    /// Sets the error code.
    pub fn with_code(mut self, code: ErrorCode) -> Self {
        self.code = code;
        self
    }

    /// Sets the severity level.
    pub fn with_severity(mut self, severity: ErrorSeverity) -> Self {
        self.severity = severity;
        self
    }

    /// Appends a context entry describing where the error originated.
    pub fn with_context(mut self, ctx: ErrorContext) -> Self {
        self.context.push(ctx);
        self
    }

    /// Chains a source error that caused this error.
    pub fn with_source(
        mut self,
        source: impl Into<Box<dyn std::error::Error + Send + Sync>>,
    ) -> Self {
        self.source = Some(source.into());
        self
    }

    /// Attaches a human-readable recovery hint.
    pub fn with_recovery(mut self, hint: impl Into<String>) -> Self {
        self.recovery_hint = Some(hint.into());
        self
    }

    /// Returns `true` when the error kind is [`ErrorKind::Transient`].
    pub fn is_retryable(&self) -> bool {
        self.kind == ErrorKind::Transient
    }

    /// Returns a default retry policy when the error is transient.
    pub fn retry_policy(&self) -> Option<RetryPolicy> {
        if self.is_retryable() {
            Some(RetryPolicy::ExponentialBackoff {
                initial_delay_ms: 100,
                max_delay_ms: 30_000,
                multiplier: 2.0,
                max_retries: 5,
                jitter: true,
            })
        } else {
            None
        }
    }
}

impl std::error::Error for BrowserOsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_ref()
            .map(|s| s.as_ref() as &(dyn std::error::Error + 'static))
    }
}

impl fmt::Display for BrowserOsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{:?}] {}: {}", self.kind, self.code, self.message)
    }
}

/// Convenience result type alias.
pub type Result<T> = std::result::Result<T, BrowserOsError>;

/// Creates an [`ErrorContext`] from the current source location.
#[macro_export]
macro_rules! error_context {
    ($msg:expr) => {
        $crate::error::ErrorContext {
            message: $msg.to_string(),
            module: module_path!().to_string(),
            file: file!().to_string(),
            line: line!(),
        }
    };
}

// ---------------------------------------------------------------------------
// Typed constructor helpers
// ---------------------------------------------------------------------------

impl BrowserOsError {
    /// Creates a configuration error ([`ErrorKind::Configuration`]).
    pub fn config_error(msg: impl Into<String>) -> Self {
        Self::new(ErrorKind::Configuration, msg)
    }

    /// Creates a transient (retryable) error ([`ErrorKind::Transient`]).
    pub fn transient(msg: impl Into<String>) -> Self {
        Self::new(ErrorKind::Transient, msg)
    }

    /// Creates a resource-exhaustion error ([`ErrorKind::Resource`]).
    pub fn resource_exhausted(msg: impl Into<String>) -> Self {
        Self::new(ErrorKind::Resource, msg)
    }

    /// Creates an internal invariant error ([`ErrorKind::Internal`]).
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::new(ErrorKind::Internal, msg)
    }

    /// Creates an unsupported-operation error ([`ErrorKind::Unsupported`]).
    pub fn unsupported(msg: impl Into<String>) -> Self {
        Self::new(ErrorKind::Unsupported, msg)
    }
}

// ---------------------------------------------------------------------------
// Conversions from common error types
// ---------------------------------------------------------------------------

impl From<std::io::Error> for BrowserOsError {
    fn from(err: std::io::Error) -> Self {
        BrowserOsError::transient(err.to_string()).with_source(err)
    }
}

impl From<serde_json::Error> for BrowserOsError {
    fn from(err: serde_json::Error) -> Self {
        BrowserOsError::internal(err.to_string()).with_source(err)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    // ─── ErrorKind ────────────────────────────────────────────────────────

    #[test]
    fn error_kind_clone_copy_eq() {
        let a = ErrorKind::Transient;
        let b = a;
        assert_eq!(a, b);
        assert_ne!(ErrorKind::Transient, ErrorKind::Permanent);
        assert_eq!(ErrorKind::Permanent, ErrorKind::Permanent);
        assert_eq!(ErrorKind::Configuration, ErrorKind::Configuration);
        assert_eq!(ErrorKind::Resource, ErrorKind::Resource);
        assert_eq!(ErrorKind::Internal, ErrorKind::Internal);
        assert_eq!(ErrorKind::Unsupported, ErrorKind::Unsupported);
    }

    // ─── ErrorSeverity ───────────────────────────────────────────────────

    #[test]
    fn error_severity_is_reportable() {
        assert!(!ErrorSeverity::Debug.is_reportable());
        assert!(!ErrorSeverity::Info.is_reportable());
        assert!(!ErrorSeverity::Warning.is_reportable());
        assert!(ErrorSeverity::Error.is_reportable());
        assert!(ErrorSeverity::Critical.is_reportable());
    }

    #[test]
    fn error_severity_ordering() {
        assert!(ErrorSeverity::Debug < ErrorSeverity::Info);
        assert!(ErrorSeverity::Info < ErrorSeverity::Warning);
        assert!(ErrorSeverity::Warning < ErrorSeverity::Error);
        assert!(ErrorSeverity::Error < ErrorSeverity::Critical);
        assert!(ErrorSeverity::Critical > ErrorSeverity::Debug);
        assert_eq!(
            ErrorSeverity::Warning.cmp(&ErrorSeverity::Warning),
            Ordering::Equal
        );
        assert_eq!(
            ErrorSeverity::Debug.cmp(&ErrorSeverity::Critical),
            Ordering::Less
        );
    }

    #[test]
    fn error_severity_partial_ord() {
        use std::cmp::Ordering;
        assert_eq!(
            ErrorSeverity::Debug.partial_cmp(&ErrorSeverity::Info),
            Some(Ordering::Less)
        );
        assert_eq!(
            ErrorSeverity::Critical.partial_cmp(&ErrorSeverity::Error),
            Some(Ordering::Greater)
        );
    }

    #[test]
    fn error_severity_clone_copy() {
        let a = ErrorSeverity::Error;
        let b = a;
        assert_eq!(a, b);
    }

    // ─── ErrorContext and error_context! ──────────────────────────────────

    #[test]
    fn error_context_creation() {
        let ctx = error_context!("something went wrong");
        assert_eq!(ctx.message, "something went wrong");
        assert_eq!(ctx.module, "browseros_types::error::tests");
        assert!(ctx.file.ends_with("error.rs"));
        assert!(ctx.line > 0);
    }

    #[test]
    fn error_context_clone() {
        let ctx = error_context!("test");
        let cloned = ctx.clone();
        assert_eq!(ctx.message, cloned.message);
        assert_eq!(ctx.module, cloned.module);
        assert_eq!(ctx.file, cloned.file);
        assert_eq!(ctx.line, cloned.line);
    }

    // ─── RetryPolicy ──────────────────────────────────────────────────────

    #[test]
    fn retry_policy_immediate() {
        let policy = RetryPolicy::Immediate { max_retries: 3 };
        assert_eq!(policy.next_delay(0), Some(std::time::Duration::ZERO));
        assert_eq!(policy.next_delay(1), Some(std::time::Duration::ZERO));
        assert_eq!(policy.next_delay(2), Some(std::time::Duration::ZERO));
        assert_eq!(policy.next_delay(3), None);
    }

    #[test]
    fn retry_policy_immediate_zero_max() {
        let policy = RetryPolicy::Immediate { max_retries: 0 };
        assert_eq!(policy.next_delay(0), None);
    }

    #[test]
    fn retry_policy_exponential_backoff() {
        let policy = RetryPolicy::ExponentialBackoff {
            initial_delay_ms: 100,
            max_delay_ms: 10_000,
            multiplier: 2.0,
            max_retries: 4,
            jitter: false,
        };
        // attempt 0 -> 100ms
        assert_eq!(
            policy.next_delay(0),
            Some(std::time::Duration::from_millis(100))
        );
        // attempt 1 -> 200ms
        assert_eq!(
            policy.next_delay(1),
            Some(std::time::Duration::from_millis(200))
        );
        // attempt 2 -> 400ms
        assert_eq!(
            policy.next_delay(2),
            Some(std::time::Duration::from_millis(400))
        );
        // attempt 3 -> 800ms
        assert_eq!(
            policy.next_delay(3),
            Some(std::time::Duration::from_millis(800))
        );
        // attempt 4 == max_retries -> None
        assert_eq!(policy.next_delay(4), None);
    }

    #[test]
    fn retry_policy_exponential_backoff_caps_at_max() {
        let policy = RetryPolicy::ExponentialBackoff {
            initial_delay_ms: 1_000,
            max_delay_ms: 2_500,
            multiplier: 3.0,
            max_retries: 10,
            jitter: false,
        };
        // attempt 0 = 1000
        assert_eq!(
            policy.next_delay(0),
            Some(std::time::Duration::from_millis(1_000))
        );
        // attempt 1 = 3000 -> capped at 2500
        assert_eq!(
            policy.next_delay(1),
            Some(std::time::Duration::from_millis(2_500))
        );
        // all subsequent are also capped
        assert_eq!(
            policy.next_delay(5),
            Some(std::time::Duration::from_millis(2_500))
        );
    }

    #[test]
    fn retry_policy_immediate_clone() {
        let policy = RetryPolicy::Immediate { max_retries: 5 };
        let cloned = policy.clone();
        assert_eq!(policy.next_delay(0), cloned.next_delay(0));
        assert_eq!(policy.next_delay(5), cloned.next_delay(5));
    }

    // ─── BrowserOsError ───────────────────────────────────────────────────

    #[test]
    fn error_new() {
        let err = BrowserOsError::new(ErrorKind::Permanent, "something failed");
        assert_eq!(err.kind, ErrorKind::Permanent);
        assert_eq!(err.severity, ErrorSeverity::Error);
        assert_eq!(err.code, ErrorCode::new("UNKNOWN"));
        assert_eq!(err.message, "something failed");
        assert!(err.context.is_empty());
        assert!(err.source.is_none());
        assert!(err.recovery_hint.is_none());
    }

    #[test]
    fn error_with_code() {
        let err =
            BrowserOsError::new(ErrorKind::Transient, "msg").with_code(ErrorCode::new("TIMEOUT"));
        assert_eq!(err.code, ErrorCode::new("TIMEOUT"));
    }

    #[test]
    fn error_with_severity() {
        let err =
            BrowserOsError::new(ErrorKind::Internal, "msg").with_severity(ErrorSeverity::Warning);
        assert_eq!(err.severity, ErrorSeverity::Warning);
    }

    #[test]
    fn error_with_context() {
        let ctx = error_context!("first frame");
        let err = BrowserOsError::new(ErrorKind::Transient, "msg").with_context(ctx.clone());
        assert_eq!(err.context.len(), 1);
        assert_eq!(err.context[0].message, "first frame");

        // append multiple
        let err = err.with_context(error_context!("second frame"));
        assert_eq!(err.context.len(), 2);
    }

    #[test]
    fn error_with_source() {
        let io_err = std::io::Error::new(std::io::ErrorKind::ConnectionReset, "conn reset");
        let err = BrowserOsError::new(ErrorKind::Transient, "msg").with_source(io_err);
        assert!(err.source.is_some());
    }

    #[test]
    fn error_with_recovery() {
        let err = BrowserOsError::new(ErrorKind::Configuration, "bad config")
            .with_recovery("check your config file");
        assert_eq!(err.recovery_hint, Some("check your config file".into()));
    }

    #[test]
    fn error_is_retryable() {
        assert!(BrowserOsError::new(ErrorKind::Transient, "x").is_retryable());
        assert!(!BrowserOsError::new(ErrorKind::Permanent, "x").is_retryable());
        assert!(!BrowserOsError::new(ErrorKind::Configuration, "x").is_retryable());
        assert!(!BrowserOsError::new(ErrorKind::Resource, "x").is_retryable());
        assert!(!BrowserOsError::new(ErrorKind::Internal, "x").is_retryable());
        assert!(!BrowserOsError::new(ErrorKind::Unsupported, "x").is_retryable());
    }

    #[test]
    fn error_retry_policy_transient() {
        let err = BrowserOsError::transient("timeout");
        let policy = err.retry_policy();
        assert!(policy.is_some());
        if let Some(RetryPolicy::ExponentialBackoff { max_retries, .. }) = policy {
            assert_eq!(max_retries, 5);
        } else {
            panic!("expected ExponentialBackoff");
        }
    }

    #[test]
    fn error_retry_policy_non_transient() {
        let err = BrowserOsError::new(ErrorKind::Permanent, "fatal");
        assert!(err.retry_policy().is_none());

        let err = BrowserOsError::internal("bug");
        assert!(err.retry_policy().is_none());
    }

    #[test]
    fn error_display() {
        let err = BrowserOsError::new(ErrorKind::Transient, "network timeout")
            .with_code(ErrorCode::new("NET_TIMEOUT"));
        let s = err.to_string();
        assert!(s.contains("Transient"));
        assert!(s.contains("NET_TIMEOUT"));
        assert!(s.contains("network timeout"));
    }

    #[test]
    fn error_source_chain() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err = BrowserOsError::transient("io failure").with_source(io_err);
        let source = std::error::Error::source(&err);
        assert!(source.is_some());
        let source_msg = source.unwrap().to_string();
        assert!(
            source_msg.contains("file not found"),
            "source: {}",
            source_msg
        );
    }

    // ─── Typed constructors ───────────────────────────────────────────────

    #[test]
    fn error_config_error() {
        let err = BrowserOsError::config_error("missing setting");
        assert_eq!(err.kind, ErrorKind::Configuration);
        assert_eq!(err.message, "missing setting");
    }

    #[test]
    fn error_transient() {
        let err = BrowserOsError::transient("retry later");
        assert_eq!(err.kind, ErrorKind::Transient);
        assert_eq!(err.message, "retry later");
    }

    #[test]
    fn error_resource_exhausted() {
        let err = BrowserOsError::resource_exhausted("out of memory");
        assert_eq!(err.kind, ErrorKind::Resource);
        assert_eq!(err.message, "out of memory");
    }

    #[test]
    fn error_internal() {
        let err = BrowserOsError::internal("unexpected null");
        assert_eq!(err.kind, ErrorKind::Internal);
        assert_eq!(err.message, "unexpected null");
    }

    #[test]
    fn error_unsupported() {
        let err = BrowserOsError::unsupported("feature not implemented");
        assert_eq!(err.kind, ErrorKind::Unsupported);
        assert_eq!(err.message, "feature not implemented");
    }

    // ─── From impls ───────────────────────────────────────────────────────

    #[test]
    fn error_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::TimedOut, "timed out");
        let err: BrowserOsError = io_err.into();
        assert_eq!(err.kind, ErrorKind::Transient);
        assert!(err.source.is_some());
    }

    #[test]
    fn error_from_serde_json_error() {
        let json_err = serde_json::from_str::<serde_json::Value>("invalid").unwrap_err();
        let err: BrowserOsError = json_err.into();
        assert_eq!(err.kind, ErrorKind::Internal);
        assert!(err.source.is_some());
    }

    // ─── Result alias ─────────────────────────────────────────────────────

    #[test]
    fn result_alias() {
        let ok: Result<i32> = Ok(42);
        assert!(ok.is_ok());

        let err: Result<i32> = Err(BrowserOsError::transient("fail"));
        assert!(err.is_err());
    }

    // ─── Full builder chains ──────────────────────────────────────────────

    #[test]
    fn error_full_builder_chain() {
        let io_err = std::io::Error::new(std::io::ErrorKind::ConnectionReset, "reset");
        let err = BrowserOsError::transient("connection lost")
            .with_code(ErrorCode::new("CONN_RESET"))
            .with_severity(ErrorSeverity::Warning)
            .with_context(error_context!("send failed"))
            .with_context(error_context!("module: event-bus"))
            .with_source(io_err)
            .with_recovery("check network and retry");

        assert_eq!(err.kind, ErrorKind::Transient);
        assert_eq!(err.code, ErrorCode::new("CONN_RESET"));
        assert_eq!(err.severity, ErrorSeverity::Warning);
        assert_eq!(err.context.len(), 2);
        assert_eq!(err.context[0].message, "send failed");
        assert_eq!(err.context[1].message, "module: event-bus");
        assert!(err.source.is_some());
        assert_eq!(err.recovery_hint, Some("check network and retry".into()));
        assert!(err.is_retryable());
        assert!(err.retry_policy().is_some());
    }
}
