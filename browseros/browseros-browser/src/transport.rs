use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Internal transport abstraction.
///
/// The transport layer is responsible for communication with the browser
/// engine.  Upper layers never know whether communication happens through
/// CDP, WebSocket, pipe, stdio, or anything else.
///
/// Currently a placeholder that tracks connection state.
/// The CDP crate will provide the concrete implementation.
#[derive(Clone)]
pub struct TransportManager {
    state: Arc<TransportState>,
}

struct TransportState {
    connected: AtomicBool,
}

impl TransportManager {
    /// Create a new disconnected transport manager.
    pub fn new() -> Self {
        Self {
            state: Arc::new(TransportState {
                connected: AtomicBool::new(false),
            }),
        }
    }

    /// Returns `true` if the transport is currently connected.
    pub fn is_connected(&self) -> bool {
        self.state.connected.load(Ordering::SeqCst)
    }

    /// Mark the transport as connected.
    pub fn mark_connected(&self) {
        self.state.connected.store(true, Ordering::SeqCst);
    }

    /// Mark the transport as disconnected.
    pub fn mark_disconnected(&self) {
        self.state.connected.store(false, Ordering::SeqCst);
    }
}

impl Default for TransportManager {
    fn default() -> Self {
        Self::new()
    }
}
