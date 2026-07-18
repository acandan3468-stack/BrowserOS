use crate::error::CdpResult;
use std::time::Duration;

pub trait CdpTransport: Send + Sync {
    fn connect(&mut self, url: &str) -> CdpResult<()>;
    fn send(&self, message: &str) -> CdpResult<()>;
    fn receive(&self, timeout: Duration) -> CdpResult<Option<String>>;
    fn close(&mut self) -> CdpResult<()>;
    fn is_connected(&self) -> bool;
}

pub struct NullTransport {
    connected: bool,
}

impl Default for NullTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl NullTransport {
    pub fn new() -> Self {
        Self { connected: false }
    }
}

impl CdpTransport for NullTransport {
    fn connect(&mut self, _url: &str) -> CdpResult<()> {
        self.connected = true;
        Ok(())
    }

    fn send(&self, _message: &str) -> CdpResult<()> {
        Ok(())
    }

    fn receive(&self, _timeout: Duration) -> CdpResult<Option<String>> {
        Ok(None)
    }

    fn close(&mut self) -> CdpResult<()> {
        self.connected = false;
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected
    }
}

#[cfg(test)]
pub mod mock {
    use super::*;
    use crate::error::CdpError;
    use std::sync::{Arc, Mutex};

    /// MockTransport that returns responses only after `send()` is called.
    ///
    /// - `with_responses` pre-loads responses into `pending`
    /// - Each `send()` moves one pending response to `ready`
    /// - `receive()` spin-waits on `ready` so the reader thread picks up
    ///   responses only after the corresponding request was written.
    #[derive(Clone)]
    pub struct MockTransport {
        connected: bool,
        sent_messages: Arc<Mutex<Vec<String>>>,
        pending: Arc<Mutex<Vec<String>>>,
        ready: Arc<Mutex<Vec<String>>>,
        async_events: Arc<Mutex<Vec<String>>>,
        fail_connect: bool,
        fail_send: bool,
    }

    impl MockTransport {
        pub fn new() -> Self {
            Self {
                connected: false,
                sent_messages: Arc::new(Mutex::new(Vec::new())),
                pending: Arc::new(Mutex::new(Vec::new())),
                ready: Arc::new(Mutex::new(Vec::new())),
                async_events: Arc::new(Mutex::new(Vec::new())),
                fail_connect: false,
                fail_send: false,
            }
        }

        pub fn with_responses(responses: Vec<String>) -> Self {
            Self {
                connected: true,
                sent_messages: Arc::new(Mutex::new(Vec::new())),
                pending: Arc::new(Mutex::new(responses)),
                ready: Arc::new(Mutex::new(Vec::new())),
                async_events: Arc::new(Mutex::new(Vec::new())),
                fail_connect: false,
                fail_send: false,
            }
        }

        pub fn with_failure(fail_connect: bool, fail_send: bool) -> Self {
            Self {
                connected: false,
                sent_messages: Arc::new(Mutex::new(Vec::new())),
                pending: Arc::new(Mutex::new(Vec::new())),
                ready: Arc::new(Mutex::new(Vec::new())),
                async_events: Arc::new(Mutex::new(Vec::new())),
                fail_connect,
                fail_send,
            }
        }

        pub fn set_fail_connect(&mut self, fail: bool) {
            self.fail_connect = fail;
        }

        pub fn set_fail_send(&mut self, fail: bool) {
            self.fail_send = fail;
        }

        pub fn sent_messages(&self) -> Vec<String> {
            self.sent_messages.lock().unwrap().clone()
        }

        pub fn push_response(&self, response: String) {
            self.pending.lock().unwrap().push(response);
        }

        pub fn clear_responses(&self) {
            self.pending.lock().unwrap().clear();
            self.ready.lock().unwrap().clear();
        }

        pub fn push_event(&self, event: String) {
            self.async_events.lock().unwrap().push(event);
        }
    }

    impl CdpTransport for MockTransport {
        fn connect(&mut self, _url: &str) -> CdpResult<()> {
            if self.fail_connect {
                return Err(CdpError::Transport("Connection failed".into()));
            }
            self.connected = true;
            Ok(())
        }

        fn send(&self, message: &str) -> CdpResult<()> {
            if self.fail_send {
                return Err(CdpError::Transport("Send failed".into()));
            }
            self.sent_messages.lock().unwrap().push(message.to_string());
            {
                let mut pending = self.pending.lock().unwrap();
                if !pending.is_empty() {
                    let r = pending.remove(0);
                    self.ready.lock().unwrap().push(r);
                }
            }
            Ok(())
        }

        fn receive(&self, timeout: Duration) -> CdpResult<Option<String>> {
            let start = std::time::Instant::now();
            loop {
                {
                    let mut ready = self.ready.lock().unwrap();
                    if !ready.is_empty() {
                        return Ok(Some(ready.remove(0)));
                    }
                }
                {
                    let mut events = self.async_events.lock().unwrap();
                    if !events.is_empty() {
                        return Ok(Some(events.remove(0)));
                    }
                }
                if start.elapsed() >= timeout {
                    return Ok(None);
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }

        fn close(&mut self) -> CdpResult<()> {
            self.connected = false;
            Ok(())
        }

        fn is_connected(&self) -> bool {
            self.connected
        }
    }
}

#[cfg(test)]
mod tests {
    use super::mock::MockTransport;
    use super::*;
    use crate::error::CdpError;

    #[test]
    fn test_null_transport_lifecycle() {
        let mut t = NullTransport::new();
        assert!(!t.is_connected());
        t.connect("ws://localhost:9222").unwrap();
        assert!(t.is_connected());
        t.send("{}").unwrap();
        assert!(t.receive(Duration::from_millis(100)).unwrap().is_none());
        t.close().unwrap();
        assert!(!t.is_connected());
    }

    #[test]
    fn test_mock_transport_connect_success() {
        let mut t = MockTransport::new();
        t.connect("ws://localhost:9222").unwrap();
        assert!(t.is_connected());
    }

    #[test]
    fn test_mock_transport_connect_failure() {
        let mut t = MockTransport::with_failure(true, false);
        let result = t.connect("ws://localhost:9222");
        assert!(result.is_err());
    }

    #[test]
    fn test_mock_transport_send_receive() {
        let t = MockTransport::with_responses(vec![r#"{"id":1,"result":{}}"#.into()]);
        t.send("test").unwrap();
        assert_eq!(t.sent_messages(), vec!["test"]);
        let response = t.receive(Duration::from_millis(100)).unwrap();
        assert_eq!(response, Some(r#"{"id":1,"result":{}}"#.into()));
    }

    #[test]
    fn test_mock_transport_send_failure() {
        let t = MockTransport::with_failure(false, true);
        let result = t.send("test");
        assert!(result.is_err());
        match result.unwrap_err() {
            CdpError::Transport(msg) => assert_eq!(msg, "Send failed"),
            _ => panic!("Expected transport error"),
        }
    }

    #[test]
    fn test_transport_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<NullTransport>();
    }
}
