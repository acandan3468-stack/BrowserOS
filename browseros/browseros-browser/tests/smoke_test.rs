use std::sync::Arc;

use browseros_bridge::types::LaunchOptions;
use browseros_event_bus::EventBus;
use browseros_observability::{LevelFilter, LogLevel, LogRecord, Logger, OutputSink};

use browseros_browser::cdp_backend::CdpBrowserBackend;
use browseros_browser::config::BrowserConfig;
use browseros_browser::BackendFactory;
use browseros_browser::BrowserManager;

struct NoopSink;

impl OutputSink for NoopSink {
    fn write(&self, _record: &LogRecord) {}
    fn flush(&self) {}
}

fn chrome_on_path() -> bool {
    let candidates = if cfg!(target_os = "windows") {
        vec!["chrome.exe", "chromium.exe", "msedge.exe"]
    } else {
        vec![
            "chrome",
            "chromium",
            "google-chrome",
            "google-chrome-stable",
            "msedge",
        ]
    };
    let path = std::env::var_os("PATH").unwrap_or_default();
    if candidates
        .iter()
        .any(|name| std::env::split_paths(&path).any(|dir| dir.join(name).exists()))
    {
        return true;
    }
    let common = if cfg!(target_os = "windows") {
        vec![
            r"C:\Program Files\Google\Chrome\Application\chrome.exe",
            r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
            r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        ]
    } else {
        vec![
            "/usr/bin/google-chrome",
            "/usr/bin/google-chrome-stable",
            "/usr/bin/chromium",
            "/usr/bin/chromium-browser",
        ]
    };
    common.iter().any(|p| std::path::Path::new(p).exists())
}

fn test_manager() -> BrowserManager {
    let bus = Arc::new(EventBus::new());
    let logger = Arc::new(Logger::new(
        Arc::new(NoopSink),
        Arc::new(LevelFilter::new(LogLevel::Warn)),
        "smoke-test",
    ));
    let config = BrowserConfig::default();
    let manager = BrowserManager::new(bus, logger, &config);
    manager.register_backend(Box::new(CdpBrowserBackend::new()));
    manager
}

#[test]
fn smoke_browser_launch_and_connect() {
    if !chrome_on_path() {
        eprintln!("SKIP: no Chromium-based browser found on PATH");
        return;
    }

    let manager = test_manager();

    let options = LaunchOptions {
        executable: None,
        headless: true,
        args: vec!["--no-sandbox".into(), "--disable-gpu".into()],
        env: std::collections::HashMap::new(),
        user_data_dir: None,
        timeout: std::time::Duration::from_secs(30),
    };

    let handle = manager.launch(options).expect("launch should succeed");
    let info = handle.info();
    assert!(
        !info.version.is_empty(),
        "browser version should not be empty"
    );
    assert!(
        info.version.contains("Chrome")
            || info.version.contains("Chromium")
            || info.version.contains("Edge"),
        "version should mention the browser engine: got {}",
        info.version
    );

    handle.close().expect("close should succeed");
    manager.shutdown();
}

#[test]
fn smoke_register_and_query_backend_name() {
    let backend = CdpBrowserBackend::new();
    assert_eq!(backend.name(), "chromium");
}

#[test]
fn smoke_launch_without_chrome_graceful_error() {
    // When Chrome is absent, launch must return a sensible error.
    let manager = test_manager();
    let options = LaunchOptions {
        executable: Some(std::path::PathBuf::from("nonexistent-browser-binary-xyz")),
        headless: true,
        args: Vec::new(),
        env: std::collections::HashMap::new(),
        user_data_dir: None,
        timeout: std::time::Duration::from_secs(5),
    };
    let result = manager.launch(options);
    assert!(
        result.is_err(),
        "launch with invalid executable should fail"
    );
    let err = format!("{}", result.err().unwrap());
    assert!(
        err.contains("spawn")
            || err.contains("failed")
            || err.contains("NotFound")
            || err.contains("not found"),
        "error message should indicate failure to spawn: {err}"
    );
}
