//! # browseros-event-bus
//!
//! In-process event bus with publish/subscribe for the BrowserOS runtime.
//!
//! The event bus provides the core communication fabric:
//!
//! - **Publish** — dispatch a `Box<dyn Event>` to all subscribers
//! - **Subscribe** — register a handler for a specific event kind string
//! - **Unsubscribe** — remove a handler via its `SubscriptionHandle`
//!
//! Handlers run synchronously in the publisher's thread.  Thread safety is
//! provided via `Arc<RwLock<…>>` — the bus is `Send + Sync`.
//!
//! ## Correlation ID propagation
//!
//! Events carry `EventMetadata` which includes a `correlation_id`.  The bus
//! preserves this metadata.  Downstream consumers (e.g. logging subscribers)
//! extract the correlation ID and pass it to the Logger.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use browseros_types::event::{Event, EventMetadata};
use browseros_types::identifiers::{CorrelationId, ModuleId, SubscriptionHandle};
use browseros_types::value::ContentType;

/// A handler that processes events delivered by the bus.
pub trait EventHandler: Send + Sync {
    /// Process a published event.
    ///
    /// The handler can inspect the event's metadata (including correlation_id)
    /// and its payload via the `Event` trait methods.
    fn handle(&self, event: &dyn Event);
}

impl<F> EventHandler for F
where
    F: Fn(&dyn Event) + Send + Sync,
{
    fn handle(&self, event: &dyn Event) {
        (self)(event)
    }
}

impl std::fmt::Debug for dyn EventHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventHandler").finish()
    }
}

type SubscriberMap = HashMap<String, Vec<(SubscriptionHandle, Arc<dyn EventHandler>)>>;

/// In-process event bus.
///
/// Thread-safe, clone-friendly.  Each clone shares the same subscriber map.
#[derive(Clone)]
pub struct EventBus {
    inner: Arc<EventBusInner>,
}

struct EventBusInner {
    subscribers: RwLock<SubscriberMap>,
    next_handle: AtomicU64,
}

impl EventBus {
    /// Create a new empty event bus.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(EventBusInner {
                subscribers: RwLock::new(HashMap::new()),
                next_handle: AtomicU64::new(1),
            }),
        }
    }

    /// Publish an event to all subscribers of its kind.
    ///
    /// All matching handlers are invoked synchronously in the caller's thread.
    /// If a handler panics, the panic propagates.
    pub fn publish(&self, event: Box<dyn Event>) {
        let kind = event.kind().to_owned();
        let handlers = {
            let map = self.inner.subscribers.read().unwrap();
            map.get(&kind).cloned().unwrap_or_default()
        };
        for (_, handler) in &handlers {
            handler.handle(event.as_ref());
        }
    }

    /// Register a handler for a specific event kind.
    ///
    /// Returns a `SubscriptionHandle` that can be used with
    /// [`unsubscribe`](EventBus::unsubscribe).
    pub fn subscribe(
        &self,
        event_kind: &str,
        handler: Arc<dyn EventHandler>,
    ) -> SubscriptionHandle {
        let _ = self.inner.next_handle.fetch_add(1, Ordering::Relaxed);
        let handle = SubscriptionHandle::new();

        let mut map = self.inner.subscribers.write().unwrap();
        map.entry(event_kind.to_owned())
            .or_default()
            .push((handle, handler));

        handle
    }

    /// Remove a previously registered handler.
    ///
    /// No-op if the handle was already removed or never registered.
    pub fn unsubscribe(&self, handle: &SubscriptionHandle) {
        let mut map = self.inner.subscribers.write().unwrap();
        for handlers in map.values_mut() {
            handlers.retain(|(h, _)| h != handle);
        }
    }

    /// Helper: create a minimal event metadata for testing or infrastructure use.
    pub fn new_metadata(source: ModuleId, correlation_id: CorrelationId) -> EventMetadata {
        EventMetadata::new(
            source,
            correlation_id,
            None,
            ContentType::new("application/x-browseros-event"),
            chrono::Utc::now(),
        )
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for EventBus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventBus").finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use browseros_types::event::EventCategory;
    use browseros_types::value::SemVer;
    use std::any::Any;
    use std::sync::Mutex;

    struct TestPayload {
        kind: &'static str,
        metadata: EventMetadata,
    }

    impl std::fmt::Debug for TestPayload {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("TestPayload").finish()
        }
    }

    impl Event for TestPayload {
        fn kind(&self) -> &'static str {
            self.kind
        }
        fn category(&self) -> EventCategory {
            EventCategory::Domain
        }
        fn metadata(&self) -> &EventMetadata {
            &self.metadata
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    fn test_module() -> ModuleId {
        ModuleId::new("test", SemVer::new(1, 0, 0))
    }

    fn test_event(kind: &'static str) -> Box<dyn Event> {
        Box::new(TestPayload {
            kind,
            metadata: EventBus::new_metadata(test_module(), CorrelationId::new()),
        })
    }

    #[test]
    fn publish_to_subscribed_handler() {
        let bus = EventBus::new();
        let received = Arc::new(Mutex::new(false));
        let r = received.clone();

        let handler: Arc<dyn EventHandler> = Arc::new(move |_: &dyn Event| {
            *r.lock().unwrap() = true;
        });
        bus.subscribe("test.event", handler);

        bus.publish(test_event("test.event"));
        assert!(*received.lock().unwrap());
    }

    #[test]
    fn publish_to_unsubscribed_kind_does_nothing() {
        let bus = EventBus::new();
        let received = Arc::new(Mutex::new(false));
        let r = received.clone();

        let handler: Arc<dyn EventHandler> = Arc::new(move |_: &dyn Event| {
            *r.lock().unwrap() = true;
        });
        bus.subscribe("target", handler);

        bus.publish(test_event("other"));
        assert!(!*received.lock().unwrap());
    }

    #[test]
    fn subscribe_and_unsubscribe() {
        let bus = EventBus::new();
        let received = Arc::new(Mutex::new(0usize));
        let r = received.clone();

        let handler: Arc<dyn EventHandler> = Arc::new(move |_: &dyn Event| {
            *r.lock().unwrap() += 1;
        });
        let handle = bus.subscribe("test.event", handler);

        bus.publish(test_event("test.event"));
        assert_eq!(*received.lock().unwrap(), 1);

        bus.unsubscribe(&handle);
        bus.publish(test_event("test.event"));
        assert_eq!(*received.lock().unwrap(), 1);
    }

    #[test]
    fn multiple_subscribers_all_called() {
        let bus = EventBus::new();
        let counter = Arc::new(Mutex::new(0usize));

        let c1 = counter.clone();
        let h1: Arc<dyn EventHandler> = Arc::new(move |_: &dyn Event| {
            *c1.lock().unwrap() += 1;
        });
        bus.subscribe("ev", h1);
        let c2 = counter.clone();
        let h2: Arc<dyn EventHandler> = Arc::new(move |_: &dyn Event| {
            *c2.lock().unwrap() += 1;
        });
        bus.subscribe("ev", h2);

        bus.publish(test_event("ev"));
        assert_eq!(*counter.lock().unwrap(), 2);
    }

    #[test]
    fn handler_receives_event_metadata() {
        let bus = EventBus::new();
        let received_kind = Arc::new(Mutex::new(String::new()));
        let rk = received_kind.clone();

        let handler: Arc<dyn EventHandler> = Arc::new(move |ev: &dyn Event| {
            *rk.lock().unwrap() = ev.kind().to_owned();
        });
        bus.subscribe("meta_test", handler);

        bus.publish(test_event("meta_test"));
        assert_eq!(*received_kind.lock().unwrap(), "meta_test");
    }

    #[test]
    fn concurrent_publish_is_safe() {
        let bus = EventBus::new();
        let counter = Arc::new(Mutex::new(0usize));

        let c = counter.clone();
        let handler: Arc<dyn EventHandler> = Arc::new(move |_: &dyn Event| {
            *c.lock().unwrap() += 1;
        });
        bus.subscribe("concurrent", handler);

        let bus_clone = bus.clone();
        let t1 = std::thread::spawn(move || {
            for _ in 0..100 {
                bus_clone.publish(test_event("concurrent"));
            }
        });

        let bus_clone2 = bus.clone();
        let t2 = std::thread::spawn(move || {
            for _ in 0..100 {
                bus_clone2.publish(test_event("concurrent"));
            }
        });

        t1.join().unwrap();
        t2.join().unwrap();
        assert_eq!(*counter.lock().unwrap(), 200);
    }

    #[test]
    fn new_metadata_has_correlation_id() {
        let corr_id = CorrelationId::new();
        let meta = EventBus::new_metadata(test_module(), corr_id);
        assert_eq!(meta.correlation_id, corr_id);
    }
}
