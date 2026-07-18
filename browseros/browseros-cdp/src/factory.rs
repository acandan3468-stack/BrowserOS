use std::io::BufRead;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use browseros_bridge::error::{BridgeError, BridgeResult};
use browseros_bridge::traits::BrowserPort;
use browseros_bridge::types::{BrowserInfo, LaunchOptions};

use crate::backend::CdpBrowserProcess;
use crate::config::CdpConfig;
use crate::connection::CdpConnection;
use crate::error::{CdpError, CdpResult};
use crate::transport_ws::WebSocketTransport;

/// Factory for launching and connecting to Chromium-based browsers
/// via the Chrome DevTools Protocol.
pub struct CdpBackendFactory {
    config: CdpConfig,
}

impl CdpBackendFactory {
    pub fn new() -> Self {
        Self {
            config: CdpConfig::default(),
        }
    }

    pub fn with_config(config: CdpConfig) -> Self {
        Self { config }
    }

    pub fn launch(&self, options: LaunchOptions) -> BridgeResult<Box<dyn BrowserPort>> {
        let (child, endpoint) = spawn_chrome(&options)?;
        let browser = connect_to_endpoint(&endpoint, &self.config, Some(child))?;
        Ok(Box::new(browser))
    }

    pub fn connect(&self, endpoint: &str) -> BridgeResult<Box<dyn BrowserPort>> {
        let browser = connect_to_endpoint(endpoint, &self.config, None)?;
        Ok(Box::new(browser))
    }
}

impl Default for CdpBackendFactory {
    fn default() -> Self {
        Self::new()
    }
}

pub fn convert_cdp_error(e: CdpError) -> BridgeError {
    match e {
        CdpError::Transport(msg) => BridgeError::ConnectionRefused(msg),
        CdpError::ConnectionClosed => BridgeError::ConnectionRefused("WebSocket closed".into()),
        CdpError::ConnectionTimeout => BridgeError::ConnectionTimedOut,
        CdpError::CommandTimeout(_ms) => BridgeError::Timeout,
        CdpError::InvalidEndpoint(ep) => BridgeError::InvalidEndpoint(ep),
        CdpError::Protocol { code, message, .. } => {
            BridgeError::Internal(format!("CDP error ({code}): {message}"))
        }
        CdpError::Serialization(msg) => BridgeError::Internal(format!("Serialization: {msg}")),
        CdpError::NoResponse(id) => BridgeError::Internal(format!("No response for command {id}")),
        CdpError::CommandFailed(id, msg) => {
            BridgeError::Internal(format!("Command {id} failed: {msg}"))
        }
        CdpError::SessionNotFound(id) => {
            match id.parse::<browseros_bridge::identifiers::SessionId>() {
                Ok(sid) => BridgeError::SessionNotFound(sid),
                Err(_) => BridgeError::Internal("invalid session id".into()),
            }
        }
        CdpError::TargetNotFound(t) => BridgeError::Internal(format!("Target not found: {t}")),
        CdpError::BrowserProcess(msg) => BridgeError::Internal(format!("Browser process: {msg}")),
        CdpError::NotImplemented(feature) => BridgeError::NotImplemented(feature),
    }
}

pub fn convert_result<T>(result: CdpResult<T>) -> BridgeResult<T> {
    result.map_err(convert_cdp_error)
}

pub fn spawn_chrome(options: &LaunchOptions) -> BridgeResult<(std::process::Child, String)> {
    let executable = options
        .executable
        .as_deref()
        .unwrap_or(std::path::Path::new("chrome"));

    let mut cmd = std::process::Command::new(executable);
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
        .map_err(|e| BridgeError::Internal(format!("failed to spawn Chrome: {e}")))?;

    let endpoint = wait_for_cdp_endpoint(&mut child, options.timeout)?;
    Ok((child, endpoint))
}

pub fn wait_for_cdp_endpoint(
    child: &mut std::process::Child,
    timeout: Duration,
) -> BridgeResult<String> {
    let stderr = child
        .stderr
        .as_mut()
        .ok_or_else(|| BridgeError::Internal("no stderr captured from Chrome".into()))?;

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
                    "failed to read Chrome stderr: {e}"
                )));
            }
        }
    }

    let _ = child.kill();
    let _ = child.wait();
    Err(BridgeError::Timeout)
}

pub fn connect_to_endpoint(
    endpoint: &str,
    config: &CdpConfig,
    child: Option<std::process::Child>,
) -> BridgeResult<CdpBrowserProcess> {
    let transport = Box::new(WebSocketTransport::new());
    let mut connection = CdpConnection::new(transport, config.clone());
    connection
        .connect(endpoint)
        .map_err(|e| BridgeError::ConnectionRefused(format!("CDP connect failed: {e}")))?;

    let version: serde_json::Value = connection
        .send("Browser.getVersion", None)
        .map_err(convert_cdp_error)?;

    let product = version["product"].as_str().unwrap_or("unknown").to_string();
    let user_agent = version["userAgent"]
        .as_str()
        .unwrap_or("unknown")
        .to_string();
    let js_version = version["jsVersion"]
        .as_str()
        .unwrap_or("unknown")
        .to_string();

    let browser_info = BrowserInfo {
        executable: PathBuf::from(endpoint),
        version: product.clone(),
        user_data_dir: PathBuf::from(""),
    };

    let version_info = crate::protocol::BrowserVersion {
        product,
        revision: version["revision"].as_str().unwrap_or("").to_string(),
        user_agent,
        js_version,
    };

    Ok(CdpBrowserProcess::from_parts(
        Arc::new(connection),
        browser_info,
        version_info,
        Arc::new(Mutex::new(Vec::new())),
        child,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_factory_name_is_chromium() {
        let factory = CdpBackendFactory::new();
        assert_eq!(
            factory.config.connect_timeout,
            CdpConfig::default().connect_timeout
        );
    }

    #[test]
    fn test_factory_with_config() {
        let config = CdpConfig::default().with_connect_timeout(Duration::from_secs(10));
        let factory = CdpBackendFactory::with_config(config);
        assert_eq!(factory.config.connect_timeout, Duration::from_secs(10));
    }

    #[test]
    fn test_factory_default() {
        let factory = CdpBackendFactory::default();
        assert_eq!(
            factory.config.connect_timeout,
            CdpConfig::default().connect_timeout
        );
    }

    #[test]
    fn test_connect_to_nonexistent_endpoint() {
        let factory = CdpBackendFactory::new();
        let result = factory.connect("ws://127.0.0.1:1");
        assert!(result.is_err());
    }

    #[test]
    fn test_factory_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<CdpBackendFactory>();
    }

    #[test]
    fn test_spawn_chrome_invalid_executable() {
        let options = LaunchOptions {
            executable: Some(PathBuf::from("nonexistent-chrome-binary-xyz")),
            headless: true,
            args: Vec::new(),
            env: HashMap::new(),
            user_data_dir: None,
            timeout: Duration::from_secs(5),
        };
        let result = spawn_chrome(&options);
        assert!(result.is_err());
    }

    #[test]
    fn test_wait_for_cdp_endpoint_timeout() {
        let mut child = std::process::Command::new("cmd")
            .arg("/c")
            .arg("echo no endpoint here")
            .stderr(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .spawn()
            .expect("failed to spawn test process");

        let result = wait_for_cdp_endpoint(&mut child, Duration::from_millis(500));
        assert!(result.is_err());
        match result {
            Err(BridgeError::Timeout) => {}
            _ => panic!("expected Timeout error"),
        }
    }

    #[test]
    fn test_convert_transport_error() {
        let cdp_err = CdpError::Transport("connection failed".into());
        let bridge_err = convert_result::<()>(Err(cdp_err));
        match bridge_err {
            Err(BridgeError::ConnectionRefused(msg)) => assert_eq!(msg, "connection failed"),
            _ => panic!("expected ConnectionRefused"),
        }
    }

    #[test]
    fn test_convert_timeout_error() {
        let cdp_err = CdpError::CommandTimeout(5000);
        let bridge_err = convert_result::<()>(Err(cdp_err));
        match bridge_err {
            Err(BridgeError::Timeout) => {}
            _ => panic!("expected Timeout"),
        }
    }
}
