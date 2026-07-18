use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

fn startup_instant() -> &'static Instant {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now)
}

/// Returns the number of seconds since server startup.
pub fn uptime_secs() -> u64 {
    startup_instant().elapsed().as_secs()
}

/// Server-level atomic counters.
pub struct ServerMetrics {
    pub requests_total: AtomicU64,
    pub tools_called_total: AtomicU64,
    pub errors_total: AtomicU64,
    pub shutdown_requested: AtomicBool,
}

impl ServerMetrics {
    pub fn new() -> Self {
        ServerMetrics {
            requests_total: AtomicU64::new(0),
            tools_called_total: AtomicU64::new(0),
            errors_total: AtomicU64::new(0),
            shutdown_requested: AtomicBool::new(false),
        }
    }

    pub fn increment_requests(&self) {
        self.requests_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_tools_called(&self) {
        self.tools_called_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_errors(&self) {
        self.errors_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            uptime_seconds: uptime_secs(),
            requests_total: self.requests_total.load(Ordering::Relaxed),
            tools_called_total: self.tools_called_total.load(Ordering::Relaxed),
            errors_total: self.errors_total.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MetricsSnapshot {
    pub uptime_seconds: u64,
    pub requests_total: u64,
    pub tools_called_total: u64,
    pub errors_total: u64,
}

impl Default for ServerMetrics {
    fn default() -> Self {
        Self::new()
    }
}

pub type SharedMetrics = Arc<ServerMetrics>;
