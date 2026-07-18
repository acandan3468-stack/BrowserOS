use crate::serializer::{CdpEvent, CdpMessage};
use std::sync::Arc;

pub type EventCallback = Arc<dyn Fn(&str, &serde_json::Value) + Send + Sync>;

#[derive(Default)]
pub struct EventDispatcher {
    callbacks: Vec<(String, EventCallback)>,
}

impl EventDispatcher {
    pub fn new() -> Self {
        Self {
            callbacks: Vec::new(),
        }
    }

    pub fn register(&mut self, method: &str, callback: EventCallback) {
        self.callbacks.push((method.to_string(), callback));
    }

    pub fn register_for_all(&mut self, callback: EventCallback) {
        self.callbacks.push(("*".to_string(), callback));
    }

    pub fn dispatch(&self, method: &str, params: &serde_json::Value) {
        for (registered_method, callback) in &self.callbacks {
            if registered_method == "*" || registered_method == method {
                callback(method, params);
            }
        }
    }

    pub fn dispatch_event(&self, event: &CdpEvent) {
        let params = event.params.as_ref().unwrap_or(&serde_json::Value::Null);
        self.dispatch(&event.method, params);
    }

    pub fn dispatch_message(&self, message: &CdpMessage) {
        if let CdpMessage::Event(event) = message {
            self.dispatch_event(event);
        }
    }

    pub fn clear(&mut self) {
        self.callbacks.clear();
    }

    pub fn len(&self) -> usize {
        self.callbacks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.callbacks.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn test_dispatch_event() {
        let dispatcher = EventDispatcher::new();
        let called = Arc::new(AtomicUsize::new(0));
        let called2 = called.clone();

        let mut d = dispatcher;
        d.register(
            "Page.load",
            Arc::new(move |_method, _params| {
                called2.fetch_add(1, Ordering::SeqCst);
            }),
        );

        let event = CdpEvent {
            method: "Page.load".into(),
            params: Some(serde_json::json!({})),
            session_id: None,
        };
        d.dispatch_event(&event);
        assert_eq!(called.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_dispatch_wrong_method() {
        let called = Arc::new(AtomicUsize::new(0));
        let called2 = called.clone();

        let mut d = EventDispatcher::new();
        d.register(
            "Page.load",
            Arc::new(move |_method, _params| {
                called2.fetch_add(1, Ordering::SeqCst);
            }),
        );

        let event = CdpEvent {
            method: "Page.domContentEventFired".into(),
            params: Some(serde_json::json!({})),
            session_id: None,
        };
        d.dispatch_event(&event);
        assert_eq!(called.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn test_wildcard_callback() {
        let called = Arc::new(AtomicUsize::new(0));
        let called2 = called.clone();

        let mut d = EventDispatcher::new();
        d.register_for_all(Arc::new(move |_method, _params| {
            called2.fetch_add(1, Ordering::SeqCst);
        }));

        let event = CdpEvent {
            method: "Network.requestWillBeSent".into(),
            params: Some(serde_json::json!({})),
            session_id: None,
        };
        d.dispatch_event(&event);
        assert_eq!(called.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_dispatch_message_response_ignored() {
        let called = Arc::new(AtomicUsize::new(0));
        let called2 = called.clone();

        let mut d = EventDispatcher::new();
        d.register_for_all(Arc::new(move |_method, _params| {
            called2.fetch_add(1, Ordering::SeqCst);
        }));

        let msg = CdpMessage::Response(crate::serializer::CdpResponse {
            id: 1,
            result: Some(serde_json::json!({})),
            error: None,
            session_id: None,
        });
        d.dispatch_message(&msg);
        assert_eq!(called.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn test_multiple_callbacks() {
        let count = Arc::new(AtomicUsize::new(0));
        let c1 = count.clone();
        let c2 = count.clone();

        let mut d = EventDispatcher::new();
        d.register(
            "Page.load",
            Arc::new(move |_, _| {
                c1.fetch_add(1, Ordering::SeqCst);
            }),
        );
        d.register(
            "Page.load",
            Arc::new(move |_, _| {
                c2.fetch_add(1, Ordering::SeqCst);
            }),
        );

        let event = CdpEvent {
            method: "Page.load".into(),
            params: Some(serde_json::json!({})),
            session_id: None,
        };
        d.dispatch_event(&event);
        assert_eq!(count.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn test_clear() {
        let mut d = EventDispatcher::new();
        d.register("Page.load", Arc::new(|_, _| {}));
        d.register("Page.domContentEventFired", Arc::new(|_, _| {}));
        assert_eq!(d.len(), 2);
        d.clear();
        assert_eq!(d.len(), 0);
        assert!(d.is_empty());
    }

    #[test]
    fn test_dispatch_event_no_params() {
        let called = Arc::new(AtomicUsize::new(0));
        let c = called.clone();

        let mut d = EventDispatcher::new();
        d.register(
            "Page.load",
            Arc::new(move |_, _| {
                c.fetch_add(1, Ordering::SeqCst);
            }),
        );

        let event = CdpEvent {
            method: "Page.load".into(),
            params: None,
            session_id: None,
        };
        d.dispatch_event(&event);
        assert_eq!(called.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_dispatcher_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<EventDispatcher>();
    }
}
