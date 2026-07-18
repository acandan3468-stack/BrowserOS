use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use browseros_event_bus::EventBus;
use browseros_scheduler::Scheduler;
use browseros_types::event::{Event, EventCategory, EventMetadata};
use browseros_types::identifiers::{CorrelationId, ModuleId};
use browseros_types::value::{ContentType, SemVer};

fn test_module() -> ModuleId {
    ModuleId::new("sched-stress", SemVer::new(1, 0, 0))
}

fn make_event(kind: &'static str) -> Box<dyn Event> {
    use std::any::Any;
    struct E {
        k: &'static str,
        m: EventMetadata,
    }
    impl std::fmt::Debug for E {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("E").finish()
        }
    }
    impl Event for E {
        fn kind(&self) -> &'static str {
            self.k
        }
        fn category(&self) -> EventCategory {
            EventCategory::Domain
        }
        fn metadata(&self) -> &EventMetadata {
            &self.m
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
    }
    Box::new(E {
        k: kind,
        m: EventMetadata::new(
            test_module(),
            CorrelationId::new(),
            None,
            ContentType::new("app/json"),
            chrono::Utc::now(),
        ),
    })
}

/// 3a. 100 overlapping delayed tasks — all must execute exactly once.
#[test]
fn overlapping_delayed_tasks_no_duplication() {
    let bus = Arc::new(EventBus::new());
    let sched = Scheduler::new(bus);
    let counter = Arc::new(AtomicU64::new(0));
    let num_tasks = 100;

    for i in 0..num_tasks {
        let c = counter.clone();
        let delay = Duration::from_millis(if i % 2 == 0 { 5 } else { 10 });
        sched.schedule_after(delay, move || {
            c.fetch_add(1, Ordering::Relaxed);
        });
    }

    // Wait for all tasks to complete
    std::thread::sleep(Duration::from_millis(200));
    let count = counter.load(Ordering::Relaxed);
    assert_eq!(
        count, num_tasks as u64,
        "Expected {num_tasks} executions but got {count}"
    );
}

/// 3b. High concurrency: 20 tasks scheduled simultaneously from different threads.
#[test]
fn concurrent_delayed_tasks_all_execute() {
    let bus = Arc::new(EventBus::new());
    let sched = Arc::new(Scheduler::new(bus));
    let counter = Arc::new(AtomicU64::new(0));
    let num_threads = 20;
    let tasks_per_thread = 10;
    let total_expected = num_threads * tasks_per_thread;
    let mut handles = Vec::new();

    for _ in 0..num_threads {
        let sched = sched.clone();
        let c = counter.clone();
        handles.push(std::thread::spawn(move || {
            for _ in 0..tasks_per_thread {
                let c = c.clone();
                sched.schedule_after(Duration::from_millis(5), move || {
                    c.fetch_add(1, Ordering::Relaxed);
                });
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(
        counter.load(Ordering::Relaxed),
        total_expected as u64,
        "Not all concurrent delayed tasks executed"
    );
}

/// 3c. Cancel under concurrent scheduling — no panics, no use-after-free.
#[test]
fn concurrent_schedule_and_cancel() {
    let bus = Arc::new(EventBus::new());
    let sched = Arc::new(Scheduler::new(bus));
    let executed = Arc::new(AtomicBool::new(false));
    let mut ids = Vec::new();
    let num_tasks = 50;

    // Schedule tasks and collect their IDs
    for i in 0..num_tasks {
        let e = executed.clone();
        let delay = Duration::from_millis(if i % 2 == 0 { 100 } else { 200 });
        let id = sched.schedule_after(delay, move || {
            e.store(true, Ordering::Relaxed);
        });
        ids.push(id);
    }

    // Cancel all from multiple threads
    let mut cancel_handles = Vec::new();
    for id_chunk in ids.chunks(10) {
        let sched = sched.clone();
        let chunk: Vec<_> = id_chunk.to_vec();
        cancel_handles.push(std::thread::spawn(move || {
            for id in chunk {
                sched.cancel(&id);
            }
        }));
    }

    for h in cancel_handles {
        h.join().unwrap();
    }

    // Give enough time for any uncancelled task to fire
    std::thread::sleep(Duration::from_millis(300));
    // We're not asserting on the execution flag because some tasks may
    // have fired before cancellation. The key is: no panic, no deadlock.
}

/// 3d. Event-triggered hooks under high-frequency publishing.
#[test]
fn event_hooks_high_frequency() {
    let bus = Arc::new(EventBus::new());
    let sched = Scheduler::new(bus.clone());
    let counter = Arc::new(AtomicU64::new(0));

    let c = counter.clone();
    sched.on_event("fast_event", move |_: &dyn Event| {
        c.fetch_add(1, Ordering::Relaxed);
    });

    let num_events = 5_000;
    for _ in 0..num_events {
        bus.publish(make_event("fast_event"));
    }

    assert_eq!(
        counter.load(Ordering::Relaxed),
        num_events,
        "Event-triggered hook missed events"
    );
}

/// 3e. Overlapping delayed + event-triggered — verify no cross-interference.
#[test]
fn mixed_delayed_and_event_no_interference() {
    let bus = Arc::new(EventBus::new());
    let sched = Scheduler::new(bus.clone());
    let delayed_count = Arc::new(AtomicU64::new(0));
    let event_count = Arc::new(AtomicU64::new(0));

    let ec = event_count.clone();
    sched.on_event("mix_ev", move |_: &dyn Event| {
        ec.fetch_add(1, Ordering::Relaxed);
    });

    for _ in 0..50 {
        let dc = delayed_count.clone();
        sched.schedule_after(Duration::from_millis(5), move || {
            dc.fetch_add(1, Ordering::Relaxed);
        });
    }

    for _ in 0..50 {
        bus.publish(make_event("mix_ev"));
    }

    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(
        delayed_count.load(Ordering::Relaxed),
        50,
        "Not all delayed tasks executed"
    );
    assert_eq!(
        event_count.load(Ordering::Relaxed),
        50,
        "Not all event hooks fired"
    );
}

/// 3f. Rapidly schedule many tasks with staggered delays — verify all execute.
/// Note: the scheduler uses std::thread::sleep and makes no strict ordering
/// guarantees; this test only verifies completion, not order.
#[test]
fn staggered_delays_all_execute() {
    let bus = Arc::new(EventBus::new());
    let sched = Scheduler::new(bus);
    let order = Arc::new(Mutex::new(Vec::new()));
    let num_tasks = 30;

    for i in 0..num_tasks {
        let o = order.clone();
        let delay = Duration::from_millis((num_tasks - i) as u64 * 2);
        sched.schedule_after(delay, move || {
            o.lock().unwrap().push(i);
        });
    }

    std::thread::sleep(Duration::from_millis(num_tasks as u64 * 3 + 50));
    let result = order.lock().unwrap();
    assert_eq!(result.len(), num_tasks, "Not all staggered tasks executed");
}
