use crate::config::CdpConfig;
use crate::error::{CdpError, CdpResult};
use crate::event::EventDispatcher;
use crate::serializer::{
    deserialize_message, serialize_request, CdpMessage, CdpRequest, RequestIdGenerator,
};
use crate::transport::CdpTransport;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

#[allow(dead_code)]
struct PendingResponse {
    result: std::sync::mpsc::Receiver<CdpResult<serde_json::Value>>,
}

pub struct CdpConnection {
    transport: Arc<Mutex<Box<dyn CdpTransport>>>,
    events: Arc<Mutex<EventDispatcher>>,
    #[allow(clippy::type_complexity)]
    pending: Arc<Mutex<Vec<(u64, std::sync::mpsc::Sender<CdpResult<serde_json::Value>>)>>>,
    id_gen: Arc<RequestIdGenerator>,
    config: Arc<CdpConfig>,
    running: Arc<AtomicBool>,
    _reader_thread: Option<JoinHandle<()>>,
}

impl CdpConnection {
    pub fn new(transport: Box<dyn CdpTransport>, config: CdpConfig) -> Self {
        Self {
            transport: Arc::new(Mutex::new(transport)),
            events: Arc::new(Mutex::new(EventDispatcher::new())),
            pending: Arc::new(Mutex::new(Vec::new())),
            id_gen: Arc::new(RequestIdGenerator::new()),
            config: Arc::new(config),
            running: Arc::new(AtomicBool::new(false)),
            _reader_thread: None,
        }
    }

    pub fn connect(&mut self, url: &str) -> CdpResult<()> {
        {
            let mut transport = self.transport.lock().map_err(|e| {
                CdpError::Transport(format!("CdpConnection: transport lock poisoned: {e}"))
            })?;
            transport.connect(url)?;
        }
        self.running.store(true, Ordering::SeqCst);
        let transport = self.transport.clone();
        let events = self.events.clone();
        let pending = self.pending.clone();
        let running = self.running.clone();
        let handle = thread::spawn(move || {
            while running.load(Ordering::SeqCst) {
                let data = {
                    let t = transport.lock().unwrap_or_else(|e| e.into_inner());
                    t.receive(Duration::ZERO)
                };
                match data {
                    Ok(Some(json)) => match deserialize_message(&json) {
                        Ok(CdpMessage::Response(response)) => {
                            let id = response.id;
                            let result = if let Some(ref err) = response.error {
                                Err(CdpError::Protocol {
                                    code: err.code,
                                    message: err.message.clone(),
                                    data: err.data.as_ref().map(|d| d.to_string()),
                                })
                            } else {
                                Ok(response.result.unwrap_or(serde_json::Value::Null))
                            };
                            let mut pending = pending.lock().unwrap_or_else(|e| e.into_inner());
                            if let Some(pos) = pending.iter().position(|(pid, _)| *pid == id) {
                                let (_, sender) = pending.remove(pos);
                                let _ = sender.send(result);
                            }
                        }
                        Ok(CdpMessage::Event(event)) => {
                            events
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .dispatch_event(&event);
                        }
                        Err(e) => {
                            let mut pending = pending.lock().unwrap_or_else(|e| e.into_inner());
                            for (_, sender) in pending.drain(..) {
                                let _ = sender.send(Err(CdpError::Protocol {
                                    code: -1,
                                    message: format!("Protocol error: {e}"),
                                    data: None,
                                }));
                            }
                        }
                    },
                    Ok(None) => {
                        // Release the transport lock before sleeping so the main
                        // thread can call send() without starvation.
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(e) => {
                        if !running.load(Ordering::SeqCst) {
                            break;
                        }
                        let mut pending = pending.lock().unwrap_or_else(|e| e.into_inner());
                        for (_, sender) in pending.drain(..) {
                            let _ = sender.send(Err(e.clone()));
                        }
                    }
                }
            }
        });
        self._reader_thread = Some(handle);
        Ok(())
    }

    pub fn send<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> CdpResult<T> {
        self.send_internal(method, params, None)
    }

    pub fn send_with_session<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
        session_id: &str,
    ) -> CdpResult<T> {
        self.send_internal(method, params, Some(session_id))
    }

    fn send_internal<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
        session_id: Option<&str>,
    ) -> CdpResult<T> {
        let id = self.id_gen.next();
        let request = CdpRequest {
            id,
            method: method.to_string(),
            params,
            session_id: session_id.map(|s| s.to_string()),
        };

        let (tx, rx) = std::sync::mpsc::channel();
        {
            let mut pending = self.pending.lock().map_err(|e| {
                CdpError::Transport(format!("CdpConnection: send pending lock poisoned: {e}"))
            })?;
            pending.push((id, tx));
        }

        let json = serialize_request(&request)?;
        {
            let transport = self.transport.lock().map_err(|e| {
                CdpError::Transport(format!("CdpConnection: send transport lock poisoned: {e}"))
            })?;
            transport.send(&json)?;
        }

        let timeout = self.config.command_timeout;
        match rx.recv_timeout(timeout) {
            Ok(Ok(value)) => serde_json::from_value(value)
                .map_err(|e| CdpError::Serialization(format!("Failed to deserialize result: {e}"))),
            Ok(Err(e)) => Err(e),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                let mut pending = self.pending.lock().map_err(|e| {
                    CdpError::Transport(format!(
                        "CdpConnection: timeout pending lock poisoned: {e}"
                    ))
                })?;
                pending.retain(|(pid, _)| *pid != id);
                Err(CdpError::CommandTimeout(timeout.as_millis() as u64))
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err(CdpError::ConnectionClosed),
        }
    }

    pub fn on_event<F>(&self, method: &str, callback: F)
    where
        F: Fn(&str, &serde_json::Value) + Send + Sync + 'static,
    {
        self.events
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .register(method, Arc::new(callback));
    }

    pub fn on_any_event<F>(&self, callback: F)
    where
        F: Fn(&str, &serde_json::Value) + Send + Sync + 'static,
    {
        self.events
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .register_for_all(Arc::new(callback));
    }

    pub fn is_connected(&self) -> bool {
        self.running.load(Ordering::SeqCst)
            && self
                .transport
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_connected()
    }

    pub fn close(&mut self) -> CdpResult<()> {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self._reader_thread.take() {
            let _ = handle.join();
        }
        self.transport
            .lock()
            .map_err(|e| CdpError::Transport(format!("CdpConnection: close lock poisoned: {e}")))?
            .close()
    }

    pub fn config(&self) -> &CdpConfig {
        &self.config
    }

    pub fn set_config(&mut self, config: CdpConfig) {
        self.config = Arc::new(config);
    }
}

impl Drop for CdpConnection {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::mock::MockTransport;

    #[test]
    fn test_connection_lifecycle() {
        let mut conn = CdpConnection::new(Box::new(MockTransport::new()), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        assert!(conn.is_connected());
        conn.close().unwrap();
        assert!(!conn.is_connected());
    }

    #[test]
    fn test_connect_failure() {
        let mut t = MockTransport::new();
        t.set_fail_connect(true);
        let mut conn = CdpConnection::new(Box::new(t), CdpConfig::default());
        let result = conn.connect("ws://localhost:9222");
        assert!(result.is_err());
        match result.unwrap_err() {
            CdpError::Transport(_) => {}
            _ => panic!("Expected transport error"),
        }
    }

    #[test]
    fn test_send_and_receive_response() {
        let transport =
            MockTransport::with_responses(vec![r#"{"id":1,"result":{"value":"ok"}}"#.into()]);
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let result: serde_json::Value = conn
            .send(
                "Runtime.evaluate",
                Some(serde_json::json!({
                    "expression": "1+1"
                })),
            )
            .unwrap();
        assert_eq!(result["value"], "ok");
    }

    #[test]
    fn test_send_with_command_timeout() {
        let transport = MockTransport::new();
        let mut config = CdpConfig::default();
        config.command_timeout = Duration::from_millis(10);
        let mut conn = CdpConnection::new(Box::new(transport), config);
        conn.connect("ws://localhost:9222").unwrap();
        let result: Result<serde_json::Value, _> = conn.send(
            "Runtime.evaluate",
            Some(serde_json::json!({
                "expression": "1+1"
            })),
        );
        match result {
            Err(CdpError::CommandTimeout(_)) => {}
            _ => panic!("Expected command timeout error, got: {result:?}"),
        }
    }

    #[test]
    fn test_event_registration() {
        let transport = MockTransport::with_responses(vec![]);
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();

        use std::sync::atomic::AtomicBool;
        let fired = Arc::new(AtomicBool::new(false));
        let f = fired.clone();
        conn.on_event("Page.load", move |_method, _params| {
            f.store(true, Ordering::SeqCst);
        });
        // push an event into the transport (it won't be read until data available)
        // this tests the registration more than runtime since mock transport blocks
        assert!(!fired.load(Ordering::SeqCst));
        conn.close().unwrap();
    }

    #[test]
    fn test_multiple_pending_requests() {
        let transport = MockTransport::with_responses(vec![
            r#"{"id":1,"result":{"a":1}}"#.into(),
            r#"{"id":2,"result":{"b":2}}"#.into(),
        ]);
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();

        let r1: serde_json::Value = conn.send("method.one", None).unwrap();
        let r2: serde_json::Value = conn.send("method.two", None).unwrap();
        assert_eq!(r1["a"], 1);
        assert_eq!(r2["b"], 2);
    }

    #[test]
    fn test_protocol_error_response() {
        let transport = MockTransport::with_responses(vec![
            r#"{"id":1,"error":{"code":-32601,"message":"Method not found"}}"#.into(),
        ]);
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let result: Result<serde_json::Value, _> = conn.send("Unknown.method", None);
        match result {
            Err(CdpError::Protocol { code, .. }) => assert_eq!(code, -32601),
            _ => panic!("Expected protocol error"),
        }
    }

    #[test]
    fn test_connection_drop_closes() {
        let transport = MockTransport::new();
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        assert!(conn.is_connected());
        drop(conn);
    }

    #[test]
    fn test_config_accessor() {
        let mut conn = CdpConnection::new(Box::new(MockTransport::new()), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        assert_eq!(conn.config().command_timeout, Duration::from_secs(60));
    }

    #[test]
    fn test_connection_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<CdpConnection>();
        assert_sync::<CdpConnection>();
    }
}
