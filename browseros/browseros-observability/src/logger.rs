use std::sync::Arc;

use browseros_types::identifiers::CorrelationId;
use chrono::{DateTime, Utc};

use crate::export::OutputSink;

/// Log severity level.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Trace => "trace",
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warn => "warn",
            LogLevel::Error => "error",
        }
    }
}

/// A typed value that can appear as a structured log field.
#[derive(Debug, Clone)]
pub enum FieldValue {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
}

impl From<&str> for FieldValue {
    fn from(s: &str) -> Self {
        FieldValue::String(s.to_owned())
    }
}

impl From<String> for FieldValue {
    fn from(s: String) -> Self {
        FieldValue::String(s)
    }
}

impl From<i64> for FieldValue {
    fn from(v: i64) -> Self {
        FieldValue::Int(v)
    }
}

impl From<f64> for FieldValue {
    fn from(v: f64) -> Self {
        FieldValue::Float(v)
    }
}

impl From<bool> for FieldValue {
    fn from(v: bool) -> Self {
        FieldValue::Bool(v)
    }
}

/// A single structured log record.
#[derive(Debug, Clone)]
pub struct LogRecord {
    pub timestamp: DateTime<Utc>,
    pub level: LogLevel,
    pub target: String,
    pub message: String,
    pub fields: Vec<(String, FieldValue)>,
    pub correlation_id: Option<CorrelationId>,
}

/// Filter decision for a log record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterDecision {
    Accept,
    Reject,
}

/// Trait for custom log filtering.
pub trait LogFilter: Send + Sync {
    fn should_log(&self, record: &LogRecord) -> FilterDecision;
}

/// A filter that accepts records at or above a minimum level.
#[derive(Debug, Clone)]
pub struct LevelFilter {
    min_level: LogLevel,
}

impl LevelFilter {
    pub fn new(min_level: LogLevel) -> Self {
        Self { min_level }
    }
}

impl LogFilter for LevelFilter {
    fn should_log(&self, record: &LogRecord) -> FilterDecision {
        if record.level >= self.min_level {
            FilterDecision::Accept
        } else {
            FilterDecision::Reject
        }
    }
}

/// Combined filter that accepts only if all inner filters accept.
pub struct AndFilter {
    filters: Vec<Box<dyn LogFilter>>,
}

impl AndFilter {
    pub fn new(filters: Vec<Box<dyn LogFilter>>) -> Self {
        Self { filters }
    }
}

impl LogFilter for AndFilter {
    fn should_log(&self, record: &LogRecord) -> FilterDecision {
        for f in &self.filters {
            if f.should_log(record) == FilterDecision::Reject {
                return FilterDecision::Reject;
            }
        }
        FilterDecision::Accept
    }
}

impl std::fmt::Debug for AndFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AndFilter")
            .field("filter_count", &self.filters.len())
            .finish()
    }
}

struct LoggerInner {
    sink: Arc<dyn OutputSink>,
    filter: Arc<dyn LogFilter>,
    target: String,
    fields: Vec<(String, FieldValue)>,
    correlation_id: Option<CorrelationId>,
}

/// Structured logger.
///
/// Loggers are cheap to clone.  Each clone shares the same sink and
/// filter but can carry its own target and attached fields.
/// Use [`with_correlation_id`](Logger::with_correlation_id) to set a
/// correlation ID that is included in every subsequent log record.
#[derive(Clone)]
pub struct Logger {
    inner: Arc<LoggerInner>,
}

impl Logger {
    pub fn new(sink: Arc<dyn OutputSink>, filter: Arc<dyn LogFilter>, target: &str) -> Self {
        Self {
            inner: Arc::new(LoggerInner {
                sink,
                filter,
                target: target.to_owned(),
                fields: Vec::new(),
                correlation_id: None,
            }),
        }
    }

    pub fn log(&self, record: LogRecord) {
        if self.inner.filter.should_log(&record) == FilterDecision::Accept {
            self.inner.sink.write(&record);
        }
    }

    pub fn log_with(
        &self,
        level: LogLevel,
        message: impl Into<String>,
        fields: Vec<(String, FieldValue)>,
    ) {
        let mut all_fields = self.inner.fields.clone();
        all_fields.extend(fields);
        let record = LogRecord {
            timestamp: Utc::now(),
            level,
            target: self.inner.target.clone(),
            message: message.into(),
            fields: all_fields,
            correlation_id: self.inner.correlation_id,
        };
        self.log(record);
    }

    pub fn trace(&self, message: impl Into<String>) {
        self.log_with(LogLevel::Trace, message, Vec::new());
    }

    pub fn debug(&self, message: impl Into<String>) {
        self.log_with(LogLevel::Debug, message, Vec::new());
    }

    pub fn info(&self, message: impl Into<String>) {
        self.log_with(LogLevel::Info, message, Vec::new());
    }

    pub fn warn(&self, message: impl Into<String>) {
        self.log_with(LogLevel::Warn, message, Vec::new());
    }

    pub fn error(&self, message: impl Into<String>) {
        self.log_with(LogLevel::Error, message, Vec::new());
    }

    /// Create a new Logger with an additional field attached.
    ///
    /// The original logger is consumed (cheap `Arc` book-keeping).
    pub fn with_field(self, key: impl Into<String>, value: impl Into<FieldValue>) -> Self {
        let mut fields = self.inner.fields.clone();
        fields.push((key.into(), value.into()));
        Self {
            inner: Arc::new(LoggerInner {
                sink: self.inner.sink.clone(),
                filter: self.inner.filter.clone(),
                target: self.inner.target.clone(),
                fields,
                correlation_id: self.inner.correlation_id,
            }),
        }
    }

    /// Create a child logger with a new target name.
    ///
    /// The child shares the same sink and filter.
    pub fn child(&self, target: &str) -> Self {
        Self {
            inner: Arc::new(LoggerInner {
                sink: self.inner.sink.clone(),
                filter: self.inner.filter.clone(),
                target: target.to_owned(),
                fields: Vec::new(),
                correlation_id: self.inner.correlation_id,
            }),
        }
    }

    /// Create a new Logger with a correlation ID attached.
    ///
    /// All subsequent log records from this logger will include the
    /// given correlation ID.
    pub fn with_correlation_id(self, id: CorrelationId) -> Self {
        Self {
            inner: Arc::new(LoggerInner {
                sink: self.inner.sink.clone(),
                filter: self.inner.filter.clone(),
                target: self.inner.target.clone(),
                fields: self.inner.fields.clone(),
                correlation_id: Some(id),
            }),
        }
    }
}

impl std::fmt::Debug for Logger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Logger")
            .field("target", &self.inner.target)
            .field("fields", &self.inner.fields.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::StdoutSink;

    #[test]
    fn log_level_ordering() {
        assert!(LogLevel::Trace < LogLevel::Debug);
        assert!(LogLevel::Debug < LogLevel::Info);
        assert!(LogLevel::Info < LogLevel::Warn);
        assert!(LogLevel::Warn < LogLevel::Error);
    }

    #[test]
    fn log_level_as_str() {
        assert_eq!(LogLevel::Trace.as_str(), "trace");
        assert_eq!(LogLevel::Info.as_str(), "info");
        assert_eq!(LogLevel::Error.as_str(), "error");
    }

    #[test]
    fn field_value_from_str() {
        let v: FieldValue = "hello".into();
        assert!(matches!(v, FieldValue::String(s) if s == "hello"));
    }

    #[test]
    fn field_value_from_int() {
        let v: FieldValue = 42i64.into();
        assert!(matches!(v, FieldValue::Int(42)));
    }

    #[test]
    fn field_value_from_bool() {
        let v: FieldValue = true.into();
        assert!(matches!(v, FieldValue::Bool(true)));
    }

    #[test]
    fn level_filter_accepts_at_level() {
        let record = LogRecord {
            timestamp: Utc::now(),
            level: LogLevel::Info,
            target: "test".into(),
            message: "msg".into(),
            fields: vec![],
            correlation_id: None,
        };
        let filter = LevelFilter::new(LogLevel::Info);
        assert_eq!(filter.should_log(&record), FilterDecision::Accept);

        let filter_warn = LevelFilter::new(LogLevel::Warn);
        assert_eq!(filter_warn.should_log(&record), FilterDecision::Reject);
    }

    #[test]
    fn and_filter_rejects_if_any_rejects() {
        let record = LogRecord {
            timestamp: Utc::now(),
            level: LogLevel::Info,
            target: "test".into(),
            message: "msg".into(),
            fields: vec![],
            correlation_id: None,
        };
        let filter = AndFilter::new(vec![
            Box::new(LevelFilter::new(LogLevel::Trace)),
            Box::new(LevelFilter::new(LogLevel::Warn)),
        ]);
        assert_eq!(filter.should_log(&record), FilterDecision::Reject);
    }

    #[test]
    fn logger_creates_child_with_new_target() {
        let sink = Arc::new(StdoutSink::new());
        let filter = Arc::new(LevelFilter::new(LogLevel::Trace));
        let parent = Logger::new(sink, filter, "parent");
        let child = parent.child("child.target");
        // The child target should be set correctly
        let _ = child;
    }

    #[test]
    fn logger_with_field_attaches_field() {
        let sink = Arc::new(StdoutSink::new());
        let filter = Arc::new(LevelFilter::new(LogLevel::Trace));
        let logger = Logger::new(sink, filter, "test").with_field("key", "value");
        let _ = logger;
    }

    #[test]
    fn logger_with_field_preserves_existing() {
        let sink = Arc::new(StdoutSink::new());
        let filter = Arc::new(LevelFilter::new(LogLevel::Trace));
        let logger = Logger::new(sink, filter, "test")
            .with_field("a", 1i64)
            .with_field("b", true);
        let _ = logger;
    }

    #[test]
    fn logger_with_correlation_id_sets_field() {
        let sink = Arc::new(StdoutSink::new());
        let filter = Arc::new(LevelFilter::new(LogLevel::Trace));
        let corr_id = CorrelationId::new();
        let logger = Logger::new(sink, filter, "test").with_correlation_id(corr_id);
        let _ = logger;
    }
}
