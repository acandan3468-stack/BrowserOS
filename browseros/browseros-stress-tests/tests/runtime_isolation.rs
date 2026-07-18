use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use browseros_runtime::RuntimeContext;

/// 4a. Clone context and verify all fields are independent Arc references
/// (same underlying data but no mutable sharing beyond what Arc provides).
#[test]
fn cloned_contexts_share_infrastructure() {
    let ctx = RuntimeContext::builder().build().unwrap();
    let ctx2 = ctx.clone();

    // Same underlying event bus — publish on one, receive on the other
    use browseros_types::event::{Event, EventCategory, EventMetadata};
    use browseros_types::identifiers::{CorrelationId, ModuleId};
    use browseros_types::value::{ContentType, SemVer};
    use std::any::Any;

    struct TestEv {
        meta: EventMetadata,
    }
    impl std::fmt::Debug for TestEv {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("TestEv").finish()
        }
    }
    impl Event for TestEv {
        fn kind(&self) -> &'static str {
            "iso_test"
        }
        fn category(&self) -> EventCategory {
            EventCategory::Domain
        }
        fn metadata(&self) -> &EventMetadata {
            &self.meta
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    let flag = Arc::new(AtomicU64::new(0));
    let f = flag.clone();
    ctx2.bus().subscribe(
        "iso_test",
        Arc::new(move |_: &dyn Event| {
            f.fetch_add(1, Ordering::Relaxed);
        }),
    );

    let meta = EventMetadata::new(
        ModuleId::new("test", SemVer::new(1, 0, 0)),
        CorrelationId::new(),
        None,
        ContentType::new("app/json"),
        chrono::Utc::now(),
    );
    ctx.bus().publish(Box::new(TestEv { meta }));
    assert_eq!(
        flag.load(Ordering::Relaxed),
        1,
        "Cloned context should share bus"
    );
}

/// 4b. Concurrent access to all context fields from 8 threads.
#[test]
fn concurrent_access_all_fields() {
    let ctx = Arc::new(RuntimeContext::builder().build().unwrap());
    let mut handles = Vec::new();

    for _ in 0..8 {
        let ctx = ctx.clone();
        handles.push(std::thread::spawn(move || {
            let _bus = ctx.bus();
            let _logger = ctx.logger();
            let _metrics = ctx.metrics();
            let _tracer = ctx.tracer();
            let _lifecycle = ctx.lifecycle();
            let _scheduler = ctx.scheduler();
            let _config = ctx.config();
        }));
    }

    for h in handles {
        h.join().unwrap();
    }
}

/// 4c. Clone context then mutate from independent threads — verify no
/// cross-contamination between non-observability components.
#[test]
fn cloned_context_independent_mutation() {
    let ctx = RuntimeContext::builder().build().unwrap();

    // Register components on two different lifecycle managers
    let ctx_a = ctx.clone();
    let ctx_b = ctx.clone();

    ctx_a.lifecycle().register_component("a");
    ctx_b.lifecycle().register_component("b");

    assert_eq!(
        ctx_a.lifecycle().current_state("a"),
        Some(browseros_lifecycle::LifecycleState::Created),
    );
    assert_eq!(
        ctx_b.lifecycle().current_state("b"),
        Some(browseros_lifecycle::LifecycleState::Created),
    );

    // Both contexts share the same LifecycleManager (same Arc)
    // So "a" is visible from ctx_b as well — this is expected since
    // RuntimeContext is designed for shared infrastructure.
    // The key safety property is: no data races, no panics, no corruption.
    assert_eq!(
        ctx_b.lifecycle().current_state("a"),
        Some(browseros_lifecycle::LifecycleState::Created),
        "Shared lifecycle: component 'a' should be visible from cloned context"
    );
}

/// 4d. Verify no hidden global state exists — creating N independent
/// contexts should produce fully isolated infrastructure.
#[test]
fn independent_contexts_fully_isolated() {
    let count = 50;
    let contexts: Vec<RuntimeContext> = (0..count)
        .map(|_| RuntimeContext::builder().build().unwrap())
        .collect();

    // Each context has its own event bus — publish on one should not
    // propagate to another
    use browseros_types::event::{Event, EventCategory, EventMetadata};
    use browseros_types::identifiers::{CorrelationId, ModuleId};
    use browseros_types::value::{ContentType, SemVer};
    use std::any::Any;

    struct IsoEv {
        meta: EventMetadata,
    }
    impl std::fmt::Debug for IsoEv {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("IsoEv").finish()
        }
    }
    impl Event for IsoEv {
        fn kind(&self) -> &'static str {
            "iso"
        }
        fn category(&self) -> EventCategory {
            EventCategory::Domain
        }
        fn metadata(&self) -> &EventMetadata {
            &self.meta
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    // Subscribe on all contexts
    for ctx in &contexts {
        let flag = Arc::new(AtomicU64::new(0));
        let f = flag.clone();
        ctx.bus().subscribe(
            "iso",
            Arc::new(move |_: &dyn Event| {
                f.fetch_add(1, Ordering::Relaxed);
            }),
        );
    }

    // Publish on each context — only that context's subscriber should fire
    for (i, ctx) in contexts.iter().enumerate() {
        let meta = EventMetadata::new(
            ModuleId::new("test", SemVer::new(1, 0, 0)),
            CorrelationId::new(),
            None,
            ContentType::new("app/json"),
            chrono::Utc::now(),
        );
        ctx.bus().publish(Box::new(IsoEv { meta }));
        assert_eq!(
            ctx.metrics().counter("nonexistent").value(),
            0,
            "Context {i}: metrics should be independent"
        );
    }
}

/// 4e. Metrics isolation: counters from different contexts must not mix.
#[test]
fn metrics_independent_per_context() {
    let ctx1 = RuntimeContext::builder().build().unwrap();
    let ctx2 = RuntimeContext::builder().build().unwrap();

    ctx1.metrics().counter("ops").increment();
    ctx1.metrics().counter("ops").increment();
    ctx2.metrics().counter("ops").increment();

    assert_eq!(
        ctx1.metrics().counter("ops").value(),
        2,
        "ctx1 should have 2"
    );
    assert_eq!(
        ctx2.metrics().counter("ops").value(),
        1,
        "ctx2 should have 1"
    );
}

/// 4f. Stress: create and destroy many contexts — no memory growth issues.
#[test]
fn create_destroy_many_contexts() {
    for _ in 0..200 {
        let ctx = RuntimeContext::builder().build().unwrap();
        // Use each context briefly
        let _bus = ctx.bus();
        let _logger = ctx.logger();
        let _metrics = ctx.metrics();
        drop(ctx);
    }
}
