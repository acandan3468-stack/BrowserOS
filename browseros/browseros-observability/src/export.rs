use std::fs::{File, OpenOptions};
use std::io::Write as IoWrite;
use std::path::Path;
use std::sync::Mutex;

use serde_json::json;

use crate::logger::{FieldValue, LogLevel, LogRecord};

/// Trait for log output destinations.
pub trait OutputSink: Send + Sync {
    fn write(&self, record: &LogRecord);
    fn flush(&self);
}

/// Sink that writes to stdout.
pub struct StdoutSink {
    use_json: bool,
}

impl StdoutSink {
    pub fn new() -> Self {
        Self { use_json: false }
    }

    pub fn with_json(mut self) -> Self {
        self.use_json = true;
        self
    }
}

impl OutputSink for StdoutSink {
    fn write(&self, record: &LogRecord) {
        let line = format_record(record, self.use_json);
        let _ = std::io::stdout().lock().write_all(line.as_bytes());
    }

    fn flush(&self) {
        let _ = std::io::stdout().lock().flush();
    }
}

/// Sink that writes to stderr.
pub struct StderrSink {
    use_json: bool,
}

impl StderrSink {
    pub fn new() -> Self {
        Self { use_json: false }
    }

    pub fn with_json(mut self) -> Self {
        self.use_json = true;
        self
    }
}

impl OutputSink for StderrSink {
    fn write(&self, record: &LogRecord) {
        let line = format_record(record, self.use_json);
        let _ = std::io::stderr().lock().write_all(line.as_bytes());
    }

    fn flush(&self) {
        let _ = std::io::stderr().lock().flush();
    }
}

/// Sink that writes to a file.
pub struct FileSink {
    file: Mutex<File>,
    use_json: bool,
}

impl FileSink {
    pub fn new(path: &Path, use_json: bool) -> std::io::Result<Self> {
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Self {
            file: Mutex::new(file),
            use_json,
        })
    }
}

impl OutputSink for FileSink {
    fn write(&self, record: &LogRecord) {
        let line = format_record(record, self.use_json);
        if let Ok(mut f) = self.file.lock() {
            let _ = f.write_all(line.as_bytes());
            let _ = f.flush();
        }
    }

    fn flush(&self) {
        if let Ok(mut f) = self.file.lock() {
            let _ = f.flush();
        }
    }
}

fn format_record(record: &LogRecord, use_json: bool) -> String {
    if use_json {
        format_json(record)
    } else {
        format_text(record)
    }
}

fn level_label(level: LogLevel) -> &'static str {
    match level {
        LogLevel::Trace => "TRACE",
        LogLevel::Debug => "DEBUG",
        LogLevel::Info => "INFO",
        LogLevel::Warn => "WARN",
        LogLevel::Error => "ERROR",
    }
}

fn format_text(record: &LogRecord) -> String {
    let ts = record.timestamp.format("%Y-%m-%dT%H:%M:%S%.3fZ");
    let level = level_label(record.level);
    let corr = record
        .correlation_id
        .as_ref()
        .map(|c| format!(" [{}]", c))
        .unwrap_or_default();
    let fields = if record.fields.is_empty() {
        String::new()
    } else {
        let f: Vec<String> = record
            .fields
            .iter()
            .map(|(k, v)| format!(" {}={}", k, field_display(v)))
            .collect();
        f.concat()
    };
    format!(
        "{} {} {}{} - {}{}\n",
        ts, level, record.target, corr, record.message, fields
    )
}

fn format_json(record: &LogRecord) -> String {
    let fields: serde_json::Value = record
        .fields
        .iter()
        .map(|(k, v)| (k.clone(), field_to_json(v)))
        .collect();
    let mut obj = json!({
        "timestamp": record.timestamp.to_rfc3339(),
        "level": level_label(record.level),
        "target": record.target,
        "message": record.message,
        "fields": fields,
    });
    if let Some(cid) = &record.correlation_id {
        obj["correlation_id"] = json!(cid.to_string());
    }
    serde_json::to_string(&obj).unwrap_or_default() + "\n"
}

fn field_display(v: &FieldValue) -> String {
    match v {
        FieldValue::String(s) => s.clone(),
        FieldValue::Int(i) => i.to_string(),
        FieldValue::Float(f) => format!("{:.4}", f),
        FieldValue::Bool(b) => b.to_string(),
    }
}

fn field_to_json(v: &FieldValue) -> serde_json::Value {
    match v {
        FieldValue::String(s) => json!(s),
        FieldValue::Int(i) => json!(i),
        FieldValue::Float(f) => json!(f),
        FieldValue::Bool(b) => json!(b),
    }
}

impl Default for StdoutSink {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for StderrSink {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for StdoutSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StdoutSink")
            .field("use_json", &self.use_json)
            .finish()
    }
}

impl std::fmt::Debug for StderrSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StderrSink")
            .field("use_json", &self.use_json)
            .finish()
    }
}

impl std::fmt::Debug for FileSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileSink").finish()
    }
}
