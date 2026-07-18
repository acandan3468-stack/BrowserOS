# Architecture Invariants — BrowserOS

> These rules are the constitution of BrowserOS. Every contributor must uphold them.
> Violations introduce technical debt, testing fragility, or runtime failures.

---

## Data Flow

### INV-001: Components communicate only through events.
No runtime component calls methods on another runtime component directly. All inter-component communication passes through the Event Bus. *Exception: utility crates (`browseros-types`) define shared types but contain no runtime logic.*

### INV-002: Events are immutable after publication.
Once an event is published via `EventBus::publish`, no component may modify it. Event handlers receive a read-only reference.

### INV-003: Every message carries correlation_id and causation_id.
The `correlation_id` traces an entire operation across all components. The `causation_id` records what caused this specific message. Both are mandatory in `MessageEnvelope`.

### INV-004: Time always comes from an injectable `Clock`.
No component calls `std::time::Instant::now()`, `tokio::time::sleep`, or `chrono::Utc::now()` directly. All time operations go through the `Clock` trait from `RuntimeContext`.

### INV-005: No shared mutable state.
All shared state is encapsulated behind `Arc<dyn Trait>` with interior mutability (`RwLock`, `Mutex`, `dashmap`). No component holds a `&mut` reference to shared state.

### INV-006: State is derived from events, not the reverse.
The Event Store is the system of record. The State Store is a derived cache of the current snapshot. If the State Store is lost, it can be rebuilt by replaying the Event Store.

---

## Module Boundaries

### INV-007: Public APIs are backward compatible within a major version.
Breaking changes (removing a public item, changing a method signature, tightening bounds) require a major version bump. Adding items is always safe.

### INV-008: Every public API item has a doc comment.
No `pub` item exists without a doc comment. This includes traits, structs, methods, functions, constants, and type aliases. Internal items (`pub(crate)`) are encouraged but not required to be documented.

### INV-009: Every crate has a single public facade module.
The top-level `lib.rs` re-exports only the intended public API. Internal modules use `pub(crate)` visibility. Consumers depend on the crate, not on internal module paths.

### INV-010: No crate depends on another crate's internal modules.
Cross-crate visibility is always through the public API. Importing `browseros_event::internal::...` from another crate is forbidden by crate design (those items are `pub(crate)`).

### INV-011: Runtime components never own other runtime components.
Ownership is through `Arc` references. Components do not hold a unique owner relationship to other components. The `LifecycleManager` orchestrates start/stop order but does not own components in the ownership sense.

---

## Testing

### INV-012: Every component is independently testable.
All external dependencies are expressed as traits (not concrete types). A test can replace any dependency with a mock implementation.

### INV-013: Integration tests wire real implementations, not mocks.
Unit tests use mocks for isolation. Integration tests (`tests/` directory) use real implementations to verify cross-component behavior. The `RuntimeBuilder` from `browseros-core` produces real wired systems for integration tests.

### INV-014: Every state machine validates illegal transitions.
State transitions that are logically invalid (e.g., `Stopped → Running`, `Failed → Ready`) must be caught at the earliest possible point: compile time via types, or runtime via `debug_assert!` or explicit error returns.

### INV-015: Time-dependent tests use MockClock, never real time.
Tests that involve delays, timeouts, or scheduling must use `MockClock` from `browseros-types::clock`. Real-time waits (`tokio::time::sleep`) in tests are forbidden — they make tests slow and flaky.

---

## Execution

### INV-016: Every long-running operation supports cancellation.
Operations that run longer than a configurable threshold must periodically check the `CancellationToken` from `RuntimeContext`. On cancellation, they must clean up and return promptly.

### INV-017: Every async operation has a configurable timeout.
No `tokio::spawn` or `Scheduler::schedule` call runs without an associated timeout. Timeouts are always configurable through the component's config namespace.

### INV-018: No blocking I/O in async contexts.
Blocking operations (file I/O, DNS lookups, CPU-intensive work) run on `tokio::task::spawn_blocking`. The main async runtime never blocks.

### INV-019: Panics never cross component boundaries.
Every async task (`tokio::spawn`, `Scheduler::schedule`, subscription handler) wraps its body in `std::panic::catch_unwind` or equivalent. Panics are converted to errors, logged, and optionally published as events.

---

## Observability

### INV-020: Every component exposes metrics.
At minimum: operation count (`counter`), error count (`counter`), and latency distribution (`histogram`). Additional metrics are added per-component as needed.

### INV-021: Every component produces trace spans.
At minimum: one span per public method. Spans capture the operation name, duration, and any relevant parameters (sensitive data excluded).

### INV-022: Errors are never silently swallowed.
Every `Result::Err` is handled. The minimum handling is logging at `WARN` or `ERROR` level. Critical errors are published as `SystemEvent::Error` on the Event Bus.

### INV-023: Configuration changes are observable.
When configuration is reloaded, a `ConfigChanged` event is published on the Event Bus. Components that cache config values must subscribe and invalidate their cache.

---

## Resource Management

### INV-024: Every component respects resource quotas.
When a component's resource usage exceeds its quota, it returns `ResourceExceeded` error. It must not silently exceed the limit.

### INV-025: All acquired resources are released in reverse acquisition order.
Resources (memory allocations, file handles, network connections) use RAII wrappers. Manual `release()` is an anti-pattern.

---

## Plugin & Extension

### INV-026: Plugins never access internal core APIs.
Plugins interact only through the `Plugin` trait and public capability interfaces. A plugin cannot import `browseros_event::internal` or equivalent internal APIs.

### INV-027: Capability registration is explicit.
A component cannot be used through a capability interface unless it explicitly registers that capability with `CapabilityRegistry`. No implicit capability discovery.

---

## Configuration

### INV-028: Default configuration always produces a working system.
A fresh installation with no configuration files and no environment variables must start without errors. Every non-optional config field has a sensible default.

### INV-029: Configuration keys use dot notation with a BROWSEROS_ prefix.
In code: `event.bus.buffer_size`. In environment: `BROWSEROS_EVENT_BUS_BUFFER_SIZE`. In files: YAML/TOML with nested keys.

---

## Evolution

### INV-030: Experimental features are gated behind Cargo features.
Features that are not yet stable must be behind a Cargo feature flag. The default feature set produces a stable system.

### INV-031: The system degrades gracefully.
Failure of any single component must not crash the entire runtime. The health system detects failures and either restarts the component or propagates a graceful system shutdown.

### INV-032: Thread safety is explicit.
All public traits require `Send + Sync`. Types that are not thread-safe are documented as such and never appear in public interfaces.
