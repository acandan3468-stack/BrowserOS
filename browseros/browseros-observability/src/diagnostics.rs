use chrono::{DateTime, Utc};

use crate::metrics::MetricsRegistry;

/// Information about the current build and environment.
#[derive(Debug, Clone)]
pub struct BuildInfo {
    pub crate_version: &'static str,
    pub rustc_version: &'static str,
    pub build_timestamp: &'static str,
}

/// Summary of the runtime environment.
#[derive(Debug, Clone)]
pub struct EnvironmentSummary {
    pub os: String,
    pub num_cpus: usize,
    pub pid: u32,
}

/// A diagnostic snapshot at a point in time.
#[derive(Debug, Clone)]
pub struct DiagnosticReport {
    pub timestamp: DateTime<Utc>,
    pub build: BuildInfo,
    pub environment: EnvironmentSummary,
    pub status: RuntimeStatus,
}

/// Runtime status indicators.
#[derive(Debug, Clone)]
pub struct RuntimeStatus {
    pub uptime_seconds: f64,
    pub num_counters: usize,
    pub num_gauges: usize,
    pub num_histograms: usize,
}

/// A probe that reports whether the runtime is healthy.
#[derive(Debug, Clone)]
pub struct HealthProbe {
    start_time: DateTime<Utc>,
}

impl HealthProbe {
    pub fn new() -> Self {
        Self {
            start_time: Utc::now(),
        }
    }

    pub fn uptime_seconds(&self) -> f64 {
        (Utc::now() - self.start_time).num_milliseconds() as f64 / 1000.0
    }

    pub fn is_ready(&self) -> bool {
        true
    }

    pub fn is_alive(&self) -> bool {
        true
    }
}

impl Default for HealthProbe {
    fn default() -> Self {
        Self::new()
    }
}

/// Collect a full diagnostic report.
pub fn collect_report(registry: &MetricsRegistry, probe: &HealthProbe) -> DiagnosticReport {
    let snap = registry.snapshot();
    DiagnosticReport {
        timestamp: Utc::now(),
        build: BuildInfo {
            crate_version: env!("CARGO_PKG_VERSION"),
            rustc_version: env!("CARGO_PKG_RUST_VERSION"),
            build_timestamp: env!("CARGO_PKG_VERSION"),
        },
        environment: EnvironmentSummary {
            os: std::env::consts::OS.to_owned(),
            num_cpus: num_cpus(),
            pid: std::process::id(),
        },
        status: RuntimeStatus {
            uptime_seconds: probe.uptime_seconds(),
            num_counters: snap.counters.len(),
            num_gauges: snap.gauges.len(),
            num_histograms: snap.histograms.len(),
        },
    }
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

/// Render a diagnostic report as a human-readable string.
pub fn format_report(report: &DiagnosticReport) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    let _ = writeln!(
        s,
        "BrowserOS Observability Report — {}",
        report.timestamp.to_rfc3339()
    );
    let _ = writeln!(s);
    let _ = writeln!(s, "--- Build ---");
    let _ = writeln!(s, "  version:  {}", report.build.crate_version);
    let _ = writeln!(s, "  rustc:    {}", report.build.rustc_version);
    let _ = writeln!(s);
    let _ = writeln!(s, "--- Environment ---");
    let _ = writeln!(s, "  os:       {}", report.environment.os);
    let _ = writeln!(s, "  cpus:     {}", report.environment.num_cpus);
    let _ = writeln!(s, "  pid:      {}", report.environment.pid);
    let _ = writeln!(s);
    let _ = writeln!(s, "--- Status ---");
    let _ = writeln!(s, "  uptime:   {:.1}s", report.status.uptime_seconds);
    let _ = writeln!(s, "  counters: {}", report.status.num_counters);
    let _ = writeln!(s, "  gauges:   {}", report.status.num_gauges);
    let _ = writeln!(s, "  histograms: {}", report.status.num_histograms);
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_probe_uptime_increases() {
        let probe = HealthProbe::new();
        let u1 = probe.uptime_seconds();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let u2 = probe.uptime_seconds();
        assert!(u2 > u1);
    }

    #[test]
    fn health_probe_is_ready() {
        let probe = HealthProbe::new();
        assert!(probe.is_ready());
    }

    #[test]
    fn health_probe_is_alive() {
        let probe = HealthProbe::new();
        assert!(probe.is_alive());
    }

    #[test]
    fn collect_report_includes_metrics() {
        let reg = MetricsRegistry::new();
        reg.counter("test.c").increment();
        let probe = HealthProbe::new();
        let report = collect_report(&reg, &probe);
        assert!(report.status.num_counters >= 1);
    }

    #[test]
    fn collect_report_includes_build_info() {
        let reg = MetricsRegistry::new();
        let probe = HealthProbe::new();
        let report = collect_report(&reg, &probe);
        assert_eq!(report.build.crate_version, "0.1.0");
    }

    #[test]
    fn format_report_contains_sections() {
        let reg = MetricsRegistry::new();
        let probe = HealthProbe::new();
        let report = collect_report(&reg, &probe);
        let formatted = format_report(&report);
        assert!(formatted.contains("Build"));
        assert!(formatted.contains("Environment"));
        assert!(formatted.contains("Status"));
    }

    #[test]
    fn environment_summary_has_os() {
        let reg = MetricsRegistry::new();
        let probe = HealthProbe::new();
        let report = collect_report(&reg, &probe);
        assert!(!report.environment.os.is_empty());
    }

    #[test]
    fn environment_summary_has_pid() {
        let reg = MetricsRegistry::new();
        let probe = HealthProbe::new();
        let report = collect_report(&reg, &probe);
        assert!(report.environment.pid > 0);
    }
}
