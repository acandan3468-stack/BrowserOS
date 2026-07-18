//! # browseros-observability
//!
//! Observability foundation for the BrowserOS runtime.
//!
//! This crate provides the single observability layer used by every
//! other runtime crate.  Nothing outside this crate should directly own
//! logging, tracing, or metrics infrastructure.
//!
//! ## Architecture
//!
//! - **Logger** — structured logging with levels, filtering, and
//!   multiple output sinks (stdout, stderr, file).  Loggers carry
//!   attached fields and can create child loggers for sub-components.
//!
//! - **Tracer** — span-based execution tracking with RAII timing
//!   guards.  Spans record duration, events, and metadata.  A
//!   completion callback receives finished span reports.
//!
//! - **MetricsRegistry** — counter, gauge, and histogram primitives
//!   with lock-free or short-lived RwLock access.  Snapshots provide
//!   point-in-time views for diagnostics.
//!
//! - **Diagnostics** — startup diagnostics, health probes, and
//!   environment reporting.  Build info, OS summary, and metric counts.
//!
//! ## Integration with RuntimeContext
//!
//! Every component receives observability through `RuntimeContext`
//! (in `browseros-runtime`) which holds `Arc<Logger>`, `Arc<MetricsRegistry>`,
//! and `Arc<Tracer>`.
//!
//! ## Crate boundaries
//!
//! `browseros-observability` depends on `browseros-types` and
//! `browseros-config`.  It must never import any other BrowserOS crate.

pub mod config;
pub mod diagnostics;
pub mod export;
pub mod logger;
pub mod metrics;
pub mod tracer;

pub use diagnostics::{
    collect_report, format_report, BuildInfo, DiagnosticReport, EnvironmentSummary, HealthProbe,
    RuntimeStatus,
};
pub use export::{FileSink, OutputSink, StderrSink, StdoutSink};
pub use logger::{FieldValue, FilterDecision, LevelFilter, LogFilter, LogLevel, LogRecord, Logger};
pub use metrics::{
    Bucket, Counter, Gauge, Histogram, HistogramBuckets, HistogramSnapshot, HistogramTimer,
    MetricsRegistry, MetricsSnapshot,
};
pub use tracer::{SpanEvent, SpanGuard, SpanReport, Timer, Tracer};

pub use config::LogConfig;
