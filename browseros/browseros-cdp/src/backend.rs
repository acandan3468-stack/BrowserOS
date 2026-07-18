use std::path::PathBuf;
use std::process::Child;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::config::CdpConfig;
use crate::connection::CdpConnection;
use crate::error::{CdpError, CdpResult};
use crate::protocol::BrowserVersion;
use crate::session::{CdpSession, TargetManager};
use crate::traits::*;
use crate::transport::{CdpTransport, NullTransport};
use browseros_bridge::*;

#[allow(dead_code)]
pub struct CdpBrowserProcess {
    connection: Arc<CdpConnection>,
    browser_info: BrowserInfo,
    version: BrowserVersion,
    sessions: Arc<Mutex<Vec<CdpSession>>>,
    child: Mutex<Option<Child>>,
}

impl CdpBrowserProcess {
    pub fn from_parts(
        connection: Arc<CdpConnection>,
        browser_info: BrowserInfo,
        version: BrowserVersion,
        sessions: Arc<Mutex<Vec<CdpSession>>>,
        child: Option<Child>,
    ) -> Self {
        Self {
            connection,
            browser_info,
            version,
            sessions,
            child: Mutex::new(child),
        }
    }

    pub fn connect(endpoint: &str, config: CdpConfig) -> CdpResult<Self> {
        let transport: Box<dyn CdpTransport> = Box::new(NullTransport::new());
        let mut connection = CdpConnection::new(transport, config);
        connection.connect(endpoint)?;

        let version: serde_json::Value = connection.send("Browser.getVersion", None)?;
        let product = version["product"].as_str().unwrap_or("unknown").to_string();
        let user_agent = version["userAgent"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        let js_version = version["jsVersion"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();

        let bv = BrowserVersion {
            product: product.clone(),
            revision: version["revision"].as_str().unwrap_or("").to_string(),
            user_agent,
            js_version,
        };

        Ok(Self {
            connection: Arc::new(connection),
            browser_info: BrowserInfo {
                executable: PathBuf::from(endpoint),
                version: product,
                user_data_dir: PathBuf::from(""),
            },
            version: bv,
            sessions: Arc::new(Mutex::new(Vec::new())),
            child: Mutex::new(None),
        })
    }

    pub fn connection(&self) -> &Arc<CdpConnection> {
        &self.connection
    }

    pub fn config(&self) -> &CdpConfig {
        self.connection.config()
    }

    pub fn target_manager(&self) -> TargetManager {
        TargetManager::new(self.connection.clone())
    }

    pub fn endpoint_url(&self) -> String {
        "ws://localhost:9222".to_string()
    }

    /// Wait for the child process to exit, with a timeout.
    /// On timeout or error, force-kills the child.
    fn wait_for_exit(&self, timeout: Duration) {
        let mut guard = match self.child.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        let child = match guard.as_mut() {
            Some(c) => c,
            None => return,
        };

        let deadline = Instant::now() + timeout;
        loop {
            match child.try_wait() {
                Ok(Some(_status)) => {
                    *guard = None;
                    return;
                }
                Ok(None) => {
                    if Instant::now() > deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        *guard = None;
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    *guard = None;
                    return;
                }
            }
        }
    }
}

impl BrowserPort for CdpBrowserProcess {
    fn info(&self) -> BrowserInfo {
        self.browser_info.clone()
    }

    fn launch(&self, _options: LaunchOptions) -> BridgeResult<()> {
        Err(BridgeError::NotImplemented("launch via CdpBrowserProcess"))
    }

    fn create_session(&self, config: SessionConfig) -> BridgeResult<Box<dyn SessionPort>> {
        let target_mgr = self.target_manager();
        let session = target_mgr
            .create_target("about:blank")
            .map_err(map_cdp_error)?;
        let cdp_session_port = CdpSessionPort::new(
            CdpSession::new(
                self.connection.clone(),
                &session.target_id,
                session.session_id(),
            ),
            config,
        );
        Ok(Box::new(cdp_session_port))
    }

    fn sessions(&self) -> Vec<Box<dyn SessionPort>> {
        Vec::new()
    }

    fn close(&self) -> BridgeResult<()> {
        if self.connection.is_connected() {
            let _ = self
                .connection
                .send::<serde_json::Value>("Browser.close", None);
        }

        self.wait_for_exit(Duration::from_secs(5));
        Ok(())
    }

    fn kill(&self) -> BridgeResult<()> {
        let mut guard = self
            .child
            .lock()
            .map_err(|_| BridgeError::Internal("child lock poisoned".into()))?;
        if let Some(ref mut child) = *guard {
            let _ = child.kill();
            let _ = child.wait();
            *guard = None;
        }
        Ok(())
    }

    fn is_alive(&self) -> bool {
        let mut guard = match self.child.lock() {
            Ok(g) => g,
            Err(_) => return false,
        };
        match guard.as_mut() {
            Some(child) => matches!(child.try_wait(), Ok(None)),
            None => self.connection.is_connected(),
        }
    }
}

impl Drop for CdpBrowserProcess {
    fn drop(&mut self) {
        let mut child = match self.child.lock() {
            Ok(mut g) => g.take(),
            Err(_) => return,
        };
        if let Some(ref mut c) = child {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

fn map_cdp_error(e: CdpError) -> BridgeError {
    match e {
        CdpError::Transport(msg) => BridgeError::ConnectionRefused(msg),
        CdpError::ConnectionClosed => BridgeError::ConnectionRefused("Connection closed".into()),
        CdpError::ConnectionTimeout => BridgeError::ConnectionTimedOut,
        CdpError::CommandTimeout(_ms) => BridgeError::Timeout,
        CdpError::Protocol { code, message, .. } => {
            BridgeError::Internal(format!("CDP error ({code}): {message}"))
        }
        CdpError::Serialization(msg) => BridgeError::Internal(format!("Serialization: {msg}")),
        CdpError::NoResponse(id) => BridgeError::Internal(format!("No response for command {id}")),
        CdpError::CommandFailed(id, msg) => {
            BridgeError::Internal(format!("Command {id} failed: {msg}"))
        }
        CdpError::SessionNotFound(_id) => BridgeError::SessionNotFound(SessionId::default()),
        CdpError::TargetNotFound(t) => BridgeError::Internal(format!("Target not found: {t}")),
        CdpError::InvalidEndpoint(ep) => BridgeError::InvalidEndpoint(ep),
        CdpError::BrowserProcess(msg) => BridgeError::Internal(format!("Browser process: {msg}")),
        CdpError::NotImplemented(feature) => BridgeError::NotImplemented(feature),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_browser_process() -> CdpBrowserProcess {
        CdpBrowserProcess {
            connection: Arc::new(CdpConnection::new(
                Box::new(NullTransport::new()),
                CdpConfig::default(),
            )),
            browser_info: BrowserInfo {
                executable: PathBuf::from("chrome"),
                version: "Chrome/120.0.0.0".into(),
                user_data_dir: PathBuf::from("/tmp"),
            },
            version: BrowserVersion {
                product: "Chrome/120.0.0.0".into(),
                revision: "@abc".into(),
                user_agent: "Mozilla/5.0".into(),
                js_version: "12.0".into(),
            },
            sessions: Arc::new(Mutex::new(Vec::new())),
            child: Mutex::new(None),
        }
    }

    #[test]
    fn test_browser_process_info() {
        let bp = create_test_browser_process();
        let info = bp.info();
        assert!(info.version.contains("Chrome"));
    }

    #[test]
    fn test_browser_process_is_alive() {
        let mut conn = CdpConnection::new(Box::new(NullTransport::new()), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let bp = CdpBrowserProcess {
            connection: Arc::new(conn),
            browser_info: BrowserInfo {
                executable: PathBuf::from("chrome"),
                version: "Chrome/120.0.0.0".into(),
                user_data_dir: PathBuf::from("/tmp"),
            },
            version: BrowserVersion {
                product: "Chrome/120.0.0.0".into(),
                revision: "@abc".into(),
                user_agent: "Mozilla/5.0".into(),
                js_version: "12.0".into(),
            },
            sessions: Arc::new(Mutex::new(Vec::new())),
            child: Mutex::new(None),
        };
        assert!(bp.is_alive());
    }

    #[test]
    fn test_browser_process_sessions_empty() {
        let bp = create_test_browser_process();
        let sessions = bp.sessions();
        assert!(sessions.is_empty());
    }

    #[test]
    fn test_close_without_child_is_ok() {
        let bp = create_test_browser_process();
        assert!(bp.close().is_ok());
    }

    #[test]
    fn test_kill_without_child_is_ok() {
        let bp = create_test_browser_process();
        assert!(bp.kill().is_ok());
    }

    #[test]
    fn test_is_alive_with_child_proc_spawned_then_exited() {
        let child = std::process::Command::new("cmd")
            .arg("/c")
            .arg("echo alive && exit 0")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("failed to spawn test process");

        let bp = CdpBrowserProcess {
            connection: Arc::new(CdpConnection::new(
                Box::new(NullTransport::new()),
                CdpConfig::default(),
            )),
            browser_info: BrowserInfo {
                executable: PathBuf::from("test"),
                version: "test".into(),
                user_data_dir: PathBuf::from(""),
            },
            version: BrowserVersion {
                product: "test".into(),
                revision: "".into(),
                user_agent: "".into(),
                js_version: "".into(),
            },
            sessions: Arc::new(Mutex::new(Vec::new())),
            child: Mutex::new(Some(child)),
        };

        // give the child time to exit
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert!(!bp.is_alive());
    }

    #[test]
    fn test_drop_kills_child() {
        let child = std::process::Command::new("cmd")
            .arg("/c")
            .arg("echo sleeping && timeout /t 10")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("failed to spawn test process");

        let pid = child.id();

        {
            let _bp = CdpBrowserProcess {
                connection: Arc::new(CdpConnection::new(
                    Box::new(NullTransport::new()),
                    CdpConfig::default(),
                )),
                browser_info: BrowserInfo {
                    executable: PathBuf::from("test"),
                    version: "test".into(),
                    user_data_dir: PathBuf::from(""),
                },
                version: BrowserVersion {
                    product: "test".into(),
                    revision: "".into(),
                    user_agent: "".into(),
                    js_version: "".into(),
                },
                sessions: Arc::new(Mutex::new(Vec::new())),
                child: Mutex::new(Some(child)),
            };
            // _bp dropped here - should kill child
        }

        // On Windows, ReadProcessExit on a killed process succeeds
        // Verify the child is no longer running by trying to wait on it
        // (we can't easily re-acquire the handle, but we can try to open it)
        let process_handle = std::process::Command::new("cmd")
            .arg("/c")
            .arg(format!("tasklist /FI \"PID eq {pid}\" 2>nul | findstr {pid} >nul && (echo running) || (echo not running)"))
            .output()
            .expect("failed to check process");
        let output = String::from_utf8_lossy(&process_handle.stdout);
        assert!(
            output.contains("not running"),
            "child process {pid} was still running after drop: {output}"
        );
    }

    #[test]
    fn test_map_transport_error() {
        let err = CdpError::Transport("connection refused".into());
        let bridge_err = map_cdp_error(err);
        match bridge_err {
            BridgeError::ConnectionRefused(_) => {}
            _ => panic!("Expected ConnectionRefused"),
        }
    }

    #[test]
    fn test_map_timeout_error() {
        let err = CdpError::CommandTimeout(5000);
        let bridge_err = map_cdp_error(err);
        match bridge_err {
            BridgeError::Timeout => {}
            _ => panic!("Expected Timeout"),
        }
    }

    #[test]
    fn test_map_not_implemented() {
        let err = CdpError::NotImplemented("test");
        let bridge_err = map_cdp_error(err);
        match bridge_err {
            BridgeError::NotImplemented(feature) => assert_eq!(feature, "test"),
            _ => panic!("Expected NotImplemented"),
        }
    }

    #[test]
    fn test_browser_process_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<CdpBrowserProcess>();
    }
}
