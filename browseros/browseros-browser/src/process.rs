use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use browseros_bridge::error::{BridgeError, BridgeResult};
use browseros_bridge::types::LaunchOptions;

/// Outcome of a process health check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessStatus {
    /// Process is running.
    Running,
    /// Process has exited.
    Exited(Option<i32>),
    /// Process status could not be determined.
    Unknown,
}

/// Manages an operating-system browser process.
///
/// On platforms where process management is available, this wraps a
/// `std::process::Child`.  On unsupported platforms or in tests, a
/// no-op stub is used instead.
pub struct BrowserProcess {
    child: Mutex<Option<std::process::Child>>,
    killed: AtomicBool,
    startup_timeout: Duration,
    #[allow(dead_code)]
    started_at: Instant,
    endpoint: Mutex<Option<String>>,
}

impl BrowserProcess {
    /// Launch a new browser process with the given launch options.
    ///
    /// Returns the process handle and the CDP WebSocket endpoint string.
    pub fn launch(options: &LaunchOptions) -> BridgeResult<(Self, String)> {
        let started_at = Instant::now();
        let startup_timeout = options.timeout;

        let mut cmd = std::process::Command::new(
            options
                .executable
                .as_deref()
                .unwrap_or(std::path::Path::new("chrome")),
        );

        cmd.args(&options.args);
        cmd.envs(&options.env);

        if let Some(dir) = &options.user_data_dir {
            cmd.arg(format!("--user-data-dir={}", dir.display()));
        }

        if options.headless {
            cmd.arg("--headless");
        }

        cmd.arg("--remote-debugging-port=0");
        cmd.stderr(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::null());

        let mut child = cmd
            .spawn()
            .map_err(|e| BridgeError::Internal(format!("failed to spawn browser process: {e}")))?;

        let endpoint = wait_for_cdp_endpoint(&mut child, startup_timeout)?;

        Ok((
            Self {
                child: Mutex::new(Some(child)),
                killed: AtomicBool::new(false),
                startup_timeout,
                started_at,
                endpoint: Mutex::new(Some(endpoint.clone())),
            },
            endpoint,
        ))
    }

    /// Create a no-op stub process (for testing or connecting mode).
    pub fn stub(endpoint: Option<String>) -> Self {
        Self {
            child: Mutex::new(None),
            killed: AtomicBool::new(false),
            startup_timeout: Duration::from_secs(30),
            started_at: Instant::now(),
            endpoint: Mutex::new(endpoint),
        }
    }

    /// Returns the CDP WebSocket endpoint, if known.
    pub fn endpoint(&self) -> Option<String> {
        self.endpoint.lock().ok().and_then(|g| g.clone())
    }

    /// Returns `true` if the process was killed via [`kill`](BrowserProcess::kill).
    pub fn was_killed(&self) -> bool {
        self.killed.load(Ordering::SeqCst)
    }

    /// Check the current process status.
    pub fn status(&self) -> ProcessStatus {
        let mut guard = match self.child.lock() {
            Ok(g) => g,
            Err(_) => return ProcessStatus::Unknown,
        };
        match guard.as_mut() {
            Some(child) => match child.try_wait() {
                Ok(Some(status)) => ProcessStatus::Exited(status.code()),
                Ok(None) => ProcessStatus::Running,
                Err(_) => ProcessStatus::Unknown,
            },
            None => ProcessStatus::Unknown,
        }
    }

    /// Returns `true` if the process appears to be alive.
    pub fn is_alive(&self) -> bool {
        self.status() == ProcessStatus::Running
    }

    /// Gracefully close the browser process.
    pub fn close(&self) -> BridgeResult<()> {
        let mut guard = self
            .child
            .lock()
            .map_err(|_| BridgeError::Internal("process lock poisoned".into()))?;
        if let Some(child) = guard.as_mut() {
            #[cfg(windows)]
            {
                let _ = std::process::Command::new("taskkill")
                    .args(["/PID", &child.id().to_string(), "/T"])
                    .output();
            }

            let deadline = Instant::now() + self.startup_timeout;
            while Instant::now() < deadline {
                match child.try_wait() {
                    Ok(Some(_)) => {
                        guard.take();
                        return Ok(());
                    }
                    Ok(None) => std::thread::sleep(Duration::from_millis(100)),
                    Err(_) => break,
                }
            }

            let _ = child.kill();
            let _ = child.wait();
            guard.take();
        }
        Ok(())
    }

    /// Forcefully terminate the browser process.
    pub fn kill(&self) -> BridgeResult<()> {
        self.killed.store(true, Ordering::SeqCst);
        let mut guard = self
            .child
            .lock()
            .map_err(|_| BridgeError::Internal("process lock poisoned".into()))?;
        if let Some(child) = guard.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
            guard.take();
        }
        Ok(())
    }

    /// Wait for the process to exit.
    pub fn wait(&self) -> BridgeResult<Option<i32>> {
        let mut guard = self
            .child
            .lock()
            .map_err(|_| BridgeError::Internal("process lock poisoned".into()))?;
        match guard.as_mut() {
            Some(child) => {
                let status = child
                    .wait()
                    .map_err(|e| BridgeError::Internal(format!("process wait failed: {e}")))?;
                guard.take();
                Ok(status.code())
            }
            None => Ok(None),
        }
    }
}

/// Read stderr from the browser process until the CDP WebSocket endpoint
/// is detected, or the timeout expires.
fn wait_for_cdp_endpoint(
    child: &mut std::process::Child,
    timeout: Duration,
) -> BridgeResult<String> {
    use std::io::BufRead;

    let stderr = child
        .stderr
        .as_mut()
        .ok_or_else(|| BridgeError::Internal("no stderr captured from browser process".into()))?;

    let deadline = Instant::now() + timeout;
    let reader = std::io::BufReader::new(stderr);

    for line in reader.lines() {
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(BridgeError::Timeout);
        }

        match line {
            Ok(text) => {
                if let Some(pos) = text.find("DevTools listening on ws://") {
                    let start = pos + "DevTools listening on ".len();
                    return Ok(text[start..].trim().to_owned());
                }
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(BridgeError::Internal(format!(
                    "failed to read browser stderr: {e}"
                )));
            }
        }
    }

    let _ = child.kill();
    let _ = child.wait();
    Err(BridgeError::Timeout)
}

impl Drop for BrowserProcess {
    fn drop(&mut self) {
        let mut guard = match self.child.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        if let Some(mut child) = guard.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
