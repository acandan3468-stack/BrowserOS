use crate::error::{CdpError, CdpResult};
use crate::transport::CdpTransport;
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

/// Real WebSocket transport for CDP communication using tungstenite.
///
/// Connects to a Chrome DevTools Protocol endpoint (ws://...), sends
/// JSON-encoded CDP commands, and receives JSON-encoded responses and
/// events.  Thread-safe: all mutable state is behind `Arc<Mutex<>>`.
pub struct WebSocketTransport {
    ws: Arc<Mutex<Option<WebSocket<MaybeTlsStream<TcpStream>>>>>,
    connected: Arc<AtomicBool>,
}

impl Default for WebSocketTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl WebSocketTransport {
    /// Create a new unconnected transport.
    pub fn new() -> Self {
        Self {
            ws: Arc::new(Mutex::new(None)),
            connected: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl CdpTransport for WebSocketTransport {
    fn connect(&mut self, url: &str) -> CdpResult<()> {
        let parsed = url::Url::parse(url)
            .map_err(|e| CdpError::InvalidEndpoint(format!("invalid URL {url}: {e}")))?;

        let scheme = parsed.scheme();
        if scheme != "ws" && scheme != "wss" {
            return Err(CdpError::InvalidEndpoint(format!(
                "expected ws:// or wss:// URL, got {scheme}://"
            )));
        }

        let (ws, _) = tungstenite::connect(parsed.as_str())
            .map_err(|e| CdpError::Transport(format!("WebSocket connect failed: {e}")))?;

        // Set non-blocking so receive() never blocks while holding the transport lock.
        if let MaybeTlsStream::Plain(tcp) = ws.get_ref() {
            let _ = tcp.set_nonblocking(true);
        }

        *self
            .ws
            .lock()
            .map_err(|e| CdpError::Transport(format!("WebSocket mutex poisoned: {e}")))? = Some(ws);
        self.connected.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn send(&self, message: &str) -> CdpResult<()> {
        let mut guard = self
            .ws
            .lock()
            .map_err(|_| CdpError::Transport("mutex poisoned".into()))?;
        match guard.as_mut() {
            Some(ws) => ws
                .send(Message::Text(message.into()))
                .map_err(|e| CdpError::Transport(format!("send failed: {e}"))),
            None => Err(CdpError::Transport("WebSocket not connected".into())),
        }
    }

    fn receive(&self, _timeout: Duration) -> CdpResult<Option<String>> {
        let mut guard = self
            .ws
            .lock()
            .map_err(|_| CdpError::Transport("mutex poisoned".into()))?;
        let ws = match guard.as_mut() {
            Some(ws) => ws,
            None => return Err(CdpError::Transport("WebSocket not connected".into())),
        };

        match ws.read() {
            Ok(Message::Text(text)) => Ok(Some(text)),
            Ok(Message::Binary(data)) => Ok(Some(String::from_utf8_lossy(&data).into_owned())),
            Ok(Message::Close(frame)) => {
                self.connected.store(false, Ordering::SeqCst);
                let reason = frame.map(|f| f.reason.to_string()).unwrap_or_default();
                Err(CdpError::Transport(format!("WebSocket closed: {reason}")))
            }
            Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => Ok(None),
            Ok(Message::Frame(_)) => Ok(None),
            Err(tungstenite::Error::Io(ref e))
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                Ok(None)
            }
            Err(tungstenite::Error::ConnectionClosed) => {
                self.connected.store(false, Ordering::SeqCst);
                Err(CdpError::ConnectionClosed)
            }
            Err(e) => Err(CdpError::Transport(format!("receive failed: {e}"))),
        }
    }

    fn close(&mut self) -> CdpResult<()> {
        let mut guard = self
            .ws
            .lock()
            .map_err(|_| CdpError::Transport("mutex poisoned".into()))?;
        if let Some(ws) = guard.as_mut() {
            let _ = ws.close(None);
        }
        *guard = None;
        self.connected.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_new_transport_disconnected() {
        let t = WebSocketTransport::new();
        assert!(!t.is_connected());
    }

    #[test]
    fn test_send_before_connect_errors() {
        let t = WebSocketTransport::new();
        let result = t.send("{}");
        assert!(result.is_err());
        match result {
            Err(CdpError::Transport(msg)) => assert!(msg.contains("not connected")),
            _ => panic!("expected Transport error"),
        }
    }

    #[test]
    fn test_receive_before_connect_errors() {
        let t = WebSocketTransport::new();
        let result = t.receive(Duration::from_millis(10));
        assert!(result.is_err());
        match result {
            Err(CdpError::Transport(msg)) => assert!(msg.contains("not connected")),
            _ => panic!("expected Transport error"),
        }
    }

    #[test]
    fn test_close_on_disconnected_is_noop() {
        let mut t = WebSocketTransport::new();
        assert!(t.close().is_ok());
        assert!(!t.is_connected());
    }

    #[test]
    fn test_close_twice_is_safe() {
        let mut t = WebSocketTransport::new();
        assert!(t.close().is_ok());
        assert!(t.close().is_ok());
        assert!(!t.is_connected());
    }

    #[test]
    fn test_invalid_url_returns_error() {
        let mut t = WebSocketTransport::new();
        let result = t.connect("not-a-url");
        assert!(result.is_err());
        match result {
            Err(CdpError::InvalidEndpoint(_)) => {}
            _ => panic!("expected InvalidEndpoint error"),
        }
    }

    #[test]
    fn test_non_ws_scheme_returns_error() {
        let mut t = WebSocketTransport::new();
        let result = t.connect("http://localhost:9222");
        assert!(result.is_err());
        match result {
            Err(CdpError::InvalidEndpoint(msg)) => assert!(msg.contains("expected ws://")),
            _ => panic!("expected InvalidEndpoint error"),
        }
    }

    #[test]
    fn test_transport_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<WebSocketTransport>();
    }

    #[test]
    fn test_transport_implements_trait() {
        fn assert_trait<T: CdpTransport>() {}
        assert_trait::<WebSocketTransport>();
    }
}
