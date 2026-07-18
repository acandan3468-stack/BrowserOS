//! # browseros-scheduler
//!
//! Minimal delayed execution and event-triggered scheduling for BrowserOS.
//!
//! ## Delayed execution
//!
//! Schedule a closure to run after a duration:
//!
//! ```rust
//! # use std::sync::Arc;
//! # use browseros_scheduler::Scheduler;
//! # use browseros_event_bus::EventBus;
//! # let bus = Arc::new(EventBus::new());
//! # let sched = Scheduler::new(bus);
//! let id = sched.schedule_after(
//!     std::time::Duration::from_millis(100),
//!     || println!("task executed"),
//! );
//! ```
//!
//! ## Event-triggered hooks
//!
//! Register a callback that fires when a specific event kind is published:
//!
//! ```rust
//! # use std::sync::Arc;
//! # use browseros_scheduler::Scheduler;
//! # use browseros_event_bus::EventBus;
//! # let bus = Arc::new(EventBus::new());
//! # let sched = Scheduler::new(bus);
//! use browseros_types::event::Event;
//!
//! sched.on_event("my.event", |event: &dyn Event| {
//!     println!("received event: {}", event.kind());
//! });
//! ```

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use browseros_event_bus::EventBus;
use browseros_types::identifiers::TaskId;

struct ScheduledTask {
    cancelled: Arc<AtomicBool>,
}

/// Minimal scheduler for delayed and event-triggered execution.
///
/// - Delayed tasks run after a duration in a spawned thread.
/// - Event-triggered hooks subscribe to the event bus.
///
/// Tasks can be cancelled via [`cancel`](Scheduler::cancel).
#[derive(Clone)]
pub struct Scheduler {
    inner: Arc<SchedulerInner>,
}

struct SchedulerInner {
    bus: Arc<EventBus>,
    tasks: Mutex<HashMap<TaskId, ScheduledTask>>,
}

impl Scheduler {
    /// Create a new scheduler bound to the given event bus.
    pub fn new(bus: Arc<EventBus>) -> Self {
        Self {
            inner: Arc::new(SchedulerInner {
                bus,
                tasks: Mutex::new(HashMap::new()),
            }),
        }
    }

    /// Schedule a callback to run after the given duration.
    ///
    /// The callback runs in a newly spawned thread.
    /// Returns a `TaskId` that can be used with [`cancel`](Scheduler::cancel).
    pub fn schedule_after<F>(&self, delay: Duration, callback: F) -> TaskId
    where
        F: FnOnce() + Send + 'static,
    {
        let task_id = TaskId::new();
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancelled_clone = cancelled.clone();

        self.inner.tasks.lock().unwrap().insert(
            task_id,
            ScheduledTask {
                cancelled: cancelled.clone(),
            },
        );

        std::thread::spawn(move || {
            std::thread::sleep(delay);
            if !cancelled_clone.load(Ordering::Relaxed) {
                callback();
            }
        });

        task_id
    }

    /// Register a callback that fires whenever an event of the given kind
    /// is published on the bus.
    pub fn on_event<F>(&self, event_kind: &str, callback: F)
    where
        F: Fn(&dyn browseros_types::event::Event) + Send + Sync + 'static,
    {
        self.inner.bus.subscribe(event_kind, Arc::new(callback));
    }

    /// Cancel a previously scheduled task.
    ///
    /// Returns `true` if the task was found and cancelled,
    /// `false` if it was already removed or never existed.
    pub fn cancel(&self, id: &TaskId) -> bool {
        let mut tasks = self.inner.tasks.lock().unwrap();
        if let Some(task) = tasks.remove(id) {
            task.cancelled.store(true, Ordering::Relaxed);
            true
        } else {
            false
        }
    }
}

impl std::fmt::Debug for Scheduler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Scheduler").finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use browseros_types::event::{Event, EventCategory, EventMetadata};
    use browseros_types::identifiers::{CorrelationId, ModuleId};
    use browseros_types::value::{ContentType, SemVer};
    use std::any::Any;
    use std::sync::Mutex;

    struct TestEvent {
        kind: &'static str,
        metadata: EventMetadata,
    }

    impl std::fmt::Debug for TestEvent {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("TestEvent").finish()
        }
    }

    impl Event for TestEvent {
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

    fn make_event(kind: &'static str) -> Box<dyn Event> {
        Box::new(TestEvent {
            kind,
            metadata: EventMetadata::new(
                test_module(),
                CorrelationId::new(),
                None,
                ContentType::new("app/json"),
                chrono::Utc::now(),
            ),
        })
    }

    #[test]
    fn delayed_task_executes() {
        let bus = Arc::new(EventBus::new());
        let sched = Scheduler::new(bus);
        let flag = Arc::new(Mutex::new(false));
        let f = flag.clone();

        sched.schedule_after(Duration::from_millis(10), move || {
            *f.lock().unwrap() = true;
        });

        std::thread::sleep(Duration::from_millis(50));
        assert!(*flag.lock().unwrap());
    }

    #[test]
    fn delayed_task_can_be_cancelled() {
        let bus = Arc::new(EventBus::new());
        let sched = Scheduler::new(bus);
        let flag = Arc::new(Mutex::new(false));
        let f = flag.clone();

        let id = sched.schedule_after(Duration::from_millis(200), move || {
            *f.lock().unwrap() = true;
        });

        sched.cancel(&id);
        std::thread::sleep(Duration::from_millis(50));
        // Give the thread time to check cancellation after its delay
        assert!(!*flag.lock().unwrap());
    }

    #[test]
    fn cancel_nonexistent_task_returns_false() {
        let bus = Arc::new(EventBus::new());
        let sched = Scheduler::new(bus);
        assert!(!sched.cancel(&TaskId::new()));
    }

    #[test]
    fn event_triggered_hook_fires() {
        let bus = Arc::new(EventBus::new());
        let sched = Scheduler::new(bus.clone());
        let received = Arc::new(Mutex::new(false));
        let r = received.clone();

        sched.on_event("test.event", move |_ev: &dyn Event| {
            *r.lock().unwrap() = true;
        });

        bus.publish(make_event("test.event"));
        assert!(*received.lock().unwrap());
    }

    #[test]
    fn event_triggered_hook_not_called_for_wrong_kind() {
        let bus = Arc::new(EventBus::new());
        let sched = Scheduler::new(bus.clone());
        let received = Arc::new(Mutex::new(false));
        let r = received.clone();

        sched.on_event("target", move |_ev: &dyn Event| {
            *r.lock().unwrap() = true;
        });

        bus.publish(make_event("other"));
        assert!(!*received.lock().unwrap());
    }

    #[test]
    fn multiple_delayed_tasks_all_execute() {
        let bus = Arc::new(EventBus::new());
        let sched = Scheduler::new(bus);
        let counter = Arc::new(Mutex::new(0u32));

        for _ in 0..5 {
            let c = counter.clone();
            sched.schedule_after(Duration::from_millis(5), move || {
                *c.lock().unwrap() += 1;
            });
        }

        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(*counter.lock().unwrap(), 5);
    }
}
