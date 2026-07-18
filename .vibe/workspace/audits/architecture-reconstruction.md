# Architecture Reconstruction — BrowserOS

**Date:** 2026-07-08  
**Method:** Reconstructed entirely from source code  

---

## 1. Crate Dependency Graph (Actual from Cargo.toml)

```
browseros-types (zero deps beyond serde/chrono/uuid/thiserror)
  |
  ├── browseros-config           (depends on browseros-types)
  ├── browseros-observability    (depends on browseros-types, browseros-config)
  │
  ├── browseros-event-bus        (depends on browseros-types)
  ├── browseros-lifecycle        (depends on browseros-types, browseros-observability)
  ├── browseros-scheduler        (depends on browseros-types, browseros-event-bus)
  ├── browseros-storage          (depends on browseros-types)
  │
  ├── browseros-runtime          (depends on ALL above + tokio)
  │
  ├── browseros-bridge           (depends on browseros-types)
  ├── browseros-browser          (depends on browseros-types, bridge, event-bus, observability)
  ├── browseros-page             (depends on browseros-types, bridge, event-bus, observability)
  ├── browseros-cdp              (depends on browseros-types, bridge + external)
  ├── browseros-dom              (depends on browseros-types, bridge)
  │
  └── browseros-stress-tests     (depends on nearly all crates)
```

**Key observation:** The crate graph is a **star pattern** with `browseros-types` at the center. All Phase 2 crates depend on `browseros-types` + `browseros-bridge`. The runtime crates (event-bus, lifecycle, scheduler, runtime) form a separate group.

---

## 2. Runtime Architecture

### 2.1 RuntimeContext (The Composition Root)

**File:** browseros-runtime/src/lib.rs

`RuntimeContext` is an immutable struct holding `Arc` references to all shared infrastructure:

```rust
pub struct RuntimeContext {
    pub config: Arc<RootConfig>,
    pub clock: Arc<dyn Clock>,           // Injectable Clock trait
    pub cancellation: CancellationToken,  // For cooperative shutdown
    pub logger: Arc<Logger>,
    pub metrics: Arc<MetricsRegistry>,
    pub tracer: Arc<Tracer>,
    pub event_bus: Arc<EventBus>,
    pub scheduler: Arc<Scheduler>,
    pub lifecycle: Arc<LifecycleManager>,
    pub storage_manager: Arc<StorageManager>,
}
```

**Construction:** Via `RuntimeBuilder` which:
1. Creates default or custom components
2. Installs components into RuntimeContext
3. Returns `Result<RuntimeContext, RuntimeError>`

**Immutability:** ALL fields are `pub` but behind `Arc`. No setters. Once constructed, the context is read-only.

### 2.2 Event Bus

**File:** browseros-event-bus/src/lib.rs

```rust
pub struct EventBus {
    subscribers: Arc<RwLock<HashMap<EventId, Vec<Box<dyn EventHandler>>>>>,
    sequence: AtomicU64,
}
```

- **Implementation:** `InMemoryEventBus` (private struct wrapped by `EventBus` Arc)
- **Publish:** Synchronous — iterates subscribers and calls handle() immediately
- **Subscribe:** Registers handler by EventId
- **Unsubscribe:** Removes handler by SubscriptionHandle
- **No middleware** — No middleware chain, no dead letter queue, no routing
- **No async** — Fully synchronous

**Critical issue:** Subscribes by `EventId` (UUID), not by `EventCategory` or event type. This means subscribers must know the exact event ID to subscribe. This does not match the planned canonical event hierarchy (SystemEvent, DomainEvent, MetricEvent, InternalEvent).

### 2.3 Lifecycle Manager

**File:** browseros-lifecycle/src/lib.rs

```rust
pub enum LifecycleState {
    Uninitialized,
    Initializing,
    Running,
    Stopping,
    Stopped,
    Failed,
}
```

**State machine transitions:**
```
Uninitialized → Initializing
Initializing → Running | Failed
Running → Stopping | Failed
Stopping → Stopped | Failed
Stopped → (terminal)
Failed → (terminal)
```

**Transition validation:** `can_transition_to()` checks allowed transitions and returns error on invalid.

**No ManagedComponent trait** — The LifecycleManager manages abstract states (tracked as `HashMap<String, LifecycleState>`), not typed components. No health checks, no resource tracking, no dependency ordering.

### 2.4 Scheduler

**File:** browseros-scheduler/src/lib.rs

```rust
pub struct Scheduler {
    event_bus: Arc<EventBus>,
    tasks: Arc<RwLock<HashMap<TaskId, ScheduledTask>>>,
    pending: Arc<Mutex<HashMap<TaskId, JoinHandle<()>>>>,
}
```

**API:**
- `schedule_once(delay, task)` — Run task after delay
- `schedule_on_event(trigger, task)` — Run task when matching event published
- `cancel(id)` — Cancel scheduled task
- `list()` — List all scheduled tasks

**Implementation:** Uses `tokio::spawn` for delayed tasks. Event-triggered tasks subscribe to EventBus and check event type match.

**Delayed execution:** Spawns async task that calls `tokio::time::sleep(delay)` then executes the callback.

**Event-triggered:** Subscribes to EventBus with filter, executes callback when matching event arrives.

**No persistence** — All tasks are in-memory. Lost on restart.

### 2.5 Config System

**File:** browseros-config/src/

```rust
pub trait Config: Send + Sync + std::fmt::Debug {
    fn schema() -> &'static str;
    fn merge(&mut self, other: &Self);
}

pub struct ConfigLoader {
    sources: Vec<Box<dyn ConfigSource>>,
    validators: Vec<Box<dyn ConfigValidator>>,
}

pub enum ConfigSource {
    Default(serde_json::Value),
    File(PathBuf),
    Env(String),  // Prefix-based: BROWSEROS_*
}
```

**Layer precedence:** Last source wins (additive merge). Default → File → Env.

**RootConfig:** Registry of component configs accessed by name. Components call `for_component::<T>("name")` to get their config block.

### 2.6 Observability System

**Logger:**
- Structured JSON logging with configurable levels
- Fields attached via `Logger::with()`

**MetricsRegistry:**
- Counters (monotonic), Gauges (point-in-time), Histograms (distribution)
- In-memory storage via `Arc<RwLock<HashMap>>`
- No OTLP export yet (planned groundwork)

**Tracer:**
- Span-based tracing with `SpanGuard` (drop-based completion)
- Manual parent-child span linking
- In-memory only, no export

**Diagnostics:**
- `DiagnosticsCollector` gathers system metrics (memory via sysinfo, thread counts)
- Used by stress tests

---

## 3. Concurrency Model

### 3.1 Thread Safety

- **All public traits require `Send + Sync`** — Verified in invariants
- **Ownership through `Arc`** — Every shared component is `Arc<T>`
- **Interior mutability** via `RwLock` (read-heavy) and `Mutex` (write-heavy)
- **`parking_lot::RwLock`** used in EventBus for performance
- **`std::sync::Mutex`** used in Scheduler

### 3.2 Synchronous API with Async Transport

The Phase 2 architecture uses a synchronous main API with an async CDP transport thread:

```
Browser API (sync) → CdpConnection (async WebSocket reader thread)
                    → EventBus (sync publish to subscribers)
```

This is implemented in `browseros-cdp/src/connection.rs`:
1. Main thread sends CDP commands via WebSocket
2. Reader thread receives CDP events/messages
3. Reader thread dispatches via EventDispatcher → synchronous callbacks

### 3.3 Cancellation

`CancellationToken` from browseros-types supports cooperative cancellation:
- `is_cancelled()` — Non-blocking check
- `cancel()` — Sets cancelled flag
- Checked in Scheduler before executing tasks

---

## 4. Message Flow

### 4.1 Event Protocol

```
Component A → MessageEnvelope → EventBus.publish() → Subscribers
                                                           ↓
                                                    EventHandler::handle(Event)
```

The `MessageEnvelope` carries:
- `id: MessageId` (UUID v7)
- `correlation_id: CorrelationId` — Traces multi-step operations
- `causation_id: Option<MessageId>` — Causal relationship
- `source: ModuleId` — Who sent it
- `destination: Option<ModuleId>` — Who should receive
- `timestamp: DateTime<Utc>`
- `payload: Vec<u8>` — Serialized event body
- `content_type: ContentType` — Schema identifier + version
- `priority: Priority` — Low/Normal/High/Critical
- `ttl: Option<Duration>` — Expiry
- `trace_context: Option<TraceContext>` — OpenTelemetry propagation

### 4.2 Event Types

**System Events:** ConfigChanged, Error, ServiceStart, ServiceStop
**Domain Events:** Component registered, Component starting, Component started, Component stopping, Component stopped, Component failed
**Metric Events:** Counter incremented, Gauge set, Histogram observed
**Internal Events:** (Not used in current codebase — reserved for future)

---

## 5. Bridge Pattern (Phase 2 Architecture)

The bridge pattern is the defining architectural innovation of Phase 2:

```
browseros-bridge (traits only — no implementation)
     ↑                    ↑
     │                    │
browseros-cdp       Future: browseros-playwright
(Chromium CDP)       (Playwright backend)
```

**12 Port traits** define the protocol abstraction:
1. `BrowserPort` — Launch/close/kill browser, create sessions
2. `SessionPort` — Session lifecycle, page management
3. `PagePort` — Navigation, content, screenshots, PDF
4. `FramePort` — Frame info, content, child frames
5. `ElementPort` — Query, attributes, visibility, interaction
6. `DialogPort` — Dialog inspection, accept/dismiss
7. `DownloadPort` — Download tracking, cancellation
8. `InputPort` — Keyboard, mouse, file upload
9. `NetworkPort` — Request interception, cookies, conditions
10. `StoragePort` — Cookies, local/session storage, cache
11. `LocatorPort` — Multi-strategy element location
12. `ArtifactPort` — File artifact storage/retrieval

**Currently only one backend exists:** `CdpBrowserBackend` in browseros-cdp.

---

## 6. Ownership Model

```
RuntimeBuilder → RuntimeContext (Arc fields)
                      │
            ┌─────────┼─────────┐
            │         │         │
       EventBus   Scheduler  LifecycleManager
       (Arc)      (Arc)      (Arc)
            │         │
            └────┬────┘
                 │
          BrowserManager
          (owns BrowserProcess → Child process)
```

- **No component owns another** — Everything is behind Arc
- **Single exception:** `BrowserProcess` owns the OS browser child process
- **PluginRegistry missing** — No plugin ownership model exists
- **No CapabilityRegistry** — No service discovery mechanism

---

## 7. Architectural Patterns

### 7.1 Builder Pattern
- `RuntimeBuilder` for RuntimeContext
- `ConfigLoader` for RootConfig
- `MessageEnvelopeBuilder` for MessageEnvelope
- `BrowserManagerBuilder` for BrowserManager

### 7.2 State Machine Pattern
- `LifecycleState` — 6 states for runtime components
- `BrowserState` — 7 states (Disconnected, Connected, Launching, Running, Stopping, Stopped, Failed)
- `PageState` — 6 states (Loading, Navigating, Interactive, Complete, Unloading, Detached)
- `ComponentState` — 7 states (Loading → Init → Start → Ready → Degraded → Stopping → Stopped → Failed)

### 7.3 Error Handling
- `BrowserOsError` — Hierarchical with ErrorKind, ErrorSeverity, ErrorContext, RetryPolicy
- `CdpError` — Classified (Transport, Protocol, Command, Session, Timeout, Internal)
- `BridgeError` — Recoverable/Permanent classification
- `DomError` — `#[non_exhaustive]` with Stale, NotFound, MultipleFound, InvalidSelector, ClosedShadowRoot, CrossOriginFrame variants

### 7.4 Macro Pattern
- `uuid_id!` — Generates UUID-based ID newtypes
- `string_id!` — Generates string-based ID newtypes
- `u64_id!` — Generates u64-based ID newtypes
- `domain_event!` — Generates domain event structs with metadata
- `error_context!` — Generates error context with location

---

## 8. Deviations from Planned Architecture

| Planned Feature | Actual State |
|----------------|--------------|
| Event Bus with Middleware, Dead Letter, Routing | Single file — no middleware, no dead letter, routing by EventId only |
| LifecycleManager with ManagedComponent, Health, Resource | Single file — no ManagedComponent trait, no health system, no resource tracker |
| Scheduler with Trigger types | Single file — no trigger types, mixed delay + event in one struct |
| Event Store + State Store with persistence | STUB — StorageManager with start/stop only, no persistence |
| DAG Engine | NOT IMPLEMENTED |
| Plugin Registry + Capability Registry | NOT IMPLEMENTED |
| browseros-macros (proc macros) | NOT IMPLEMENTED |
| browseros-core facade | Renamed to browseros-runtime |
| browseros-store | Renamed to browseros-storage |
| browseros-event | Renamed to browseros-event-bus |

**New additions not in original plan:**
- browseros-bridge (protocol abstraction)
- browseros-browser (browser management)
- browseros-page (page lifecycle)
- browseros-cdp (CDP protocol)
- browseros-dom (DOM abstraction)
- browseros-stress-tests (testing)