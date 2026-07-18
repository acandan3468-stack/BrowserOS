# Architecture Review — BrowserOS Phase 1 (Final)

## 1. Critique of Original Plan

### 1.1 Single Crate Design ❌

**Original choice:** One crate (`browseros-core`) with `mod.rs` modules.

**Problem:** Module boundaries are social convention, not enforced. Any module can import internal details of any other module. For a system that will grow to 15+ subsystems over time, this guarantees architectural drift.

**Fix:** Multi-crate workspace with one crate per bounded context. Cross-crate dependencies are explicit in `Cargo.toml`. A `browseros-core` facade crate re-exports the public API.

### 1.2 Event Bus = Scheduler ❌

**Original choice:** Event Bus handles both pub/sub and scheduling (priorities).

**Problem:** Scheduling (deferred execution, cron, retry with backoff) is a separate concern from event routing. Conflating them creates a god module.

**Fix:** Separate `Scheduler` crate. Event Bus routes messages. Scheduler emits events on the bus when tasks trigger. (See Part 7 for further simplification.)

### 1.3 No Event Hierarchy ❌

**Original choice:** Events are opaque structs with a `kind()` method.

**Problem:** With 100+ event types, there's no taxonomy. Consumers can't subscribe to "all lifecycle events".

**Fix:** Canonical event hierarchy:
```
Event
├── SystemEvent (service start/stop, config change, error)
├── DomainEvent (task submitted, dag completed, plugin loaded)
├── MetricEvent (counter increment, gauge set, histogram observe)
└── InternalEvent (command pattern for internal routing)
```

### 1.4 No Message Protocol ❌

**Original choice:** No standard inter-module communication format.

**Problem:** Each subsystem invents its own message format. Integration becomes point-to-point translation. No tracing across boundaries.

**Fix:** Internal Message Protocol defines the envelope every subsystem uses.

### 1.5 Plugin Registry vs Capability Registry ❌

**Original choice:** Plugin registry also handles capability lookup.

**Problem:** Two different responsibilities. Plugin lifecycle is orthogonal to capability discovery.

**Fix:** Separate `CapabilityRegistry`. `PluginRegistry` handles lifecycle. Plugins register their capabilities during `init()`.

### 1.6 Logger Bundled Metrics ❌

**Original choice:** Logger includes metrics.

**Problem:** Logs and metrics have different consumers, retention policies, and query patterns.

**Fix:** Separate `MetricsRegistry` within the same `browseros-observability` crate. Logger stays focused on structured log output.

### 1.7 No Lifecycle Manager ❌

**Original choice:** Each component manages its own startup/shutdown.

**Problem:** Race conditions on shutdown, no graceful degradation, no dependency-aware start order.

**Fix:** `LifecycleManager` tracks state for every registered component. Start/stop in dependency order.

### 1.8 Service Container Overengineered ❌

**Original choice:** Full DI container (`browseros-di`).

**Revised decision:** Removed. Replaced by `RuntimeContext` + manual wiring. Rust's type system and ownership model make DI containers less valuable than in dynamic languages. `RuntimeContext` provides shared access to all core services without the complexity of a generic DI framework.

### 1.9 Resource Manager Premature ❌

**Original choice:** Separate `browseros-resource` crate.

**Revised decision:** Scoped to a module within `browseros-lifecycle`. Full resource management (CPU, memory, fd quotas) is needed in Phase 2 when browser runtimes are involved. For Phase 1, a basic memory tracker within LifecycleManager is sufficient.

### 1.10 Error System is Weak ❌

**Original choice:** `thiserror` enum per module.

**Fix:** Hierarchical error system embedded in `browseros-types`.

### 1.11 State Store is Generic ❌

**Original choice:** Generic snapshot/transaction store.

**Fix:** Design with entity-component-access pattern. Snapshots are delta-compressed. Temporal queries are a first-class API.

### 1.12 DAG Engine Lacks Observability ❌

**Original choice:** DAG engine tracks internal state.

**Fix:** Every DAG node produces lifecycle events. Events flow through Event Bus → Event Store → replayable audit trail.

---

## 2. Final Crate Structure (Design Freeze)

```
browseros/                              # Workspace root
├── Cargo.toml
│
├── browseros-types/                    # Canonical types + error system
│   ├── Cargo.toml                      #   zero dependencies
│   └── src/
│       ├── lib.rs
│       ├── event.rs                    # Event trait, EventMetadata, EventCategory
│       ├── message.rs                  # MessageEnvelope, ModuleId
│       ├── error.rs                    # BrowserOsError, ErrorKind, ErrorSeverity
│       ├── identifiers.rs              # EventId, TaskId, PluginId, etc.
│       ├── component.rs                # ComponentState, HealthStatus
│       ├── module.rs                   # ModuleId, ModuleType
│       └── value.rs                    # SemVer, ContentType, Priority
│
├── browseros-macros/                   # Proc macros
│   ├── Cargo.toml                      #   depends on browseros-types
│   └── src/lib.rs
│
├── browseros-event/                    # Event Bus + Middleware + Dead Letter
│   ├── Cargo.toml                      #   depends on browseros-types
│   └── src/
│       ├── lib.rs
│       ├── bus.rs                      # EventBus trait, InMemoryEventBus
│       ├── subscription.rs
│       ├── middleware.rs
│       ├── routing.rs
│       └── dead_letter.rs
│
├── browseros-scheduler/                # Scheduler (simplified — no cron)
│   ├── Cargo.toml                      #   depends on browseros-types, browseros-event
│   └── src/
│       ├── lib.rs
│       ├── scheduler.rs               # Scheduler trait, TokioScheduler
│       ├── task.rs                     # ScheduledTask, delay, retry
│       └── trigger.rs                  # EventTrigger, DelayTrigger
│
├── browseros-store/                    # Event Store + State Store
│   ├── Cargo.toml                      #   depends on browseros-types
│   └── src/
│       ├── lib.rs
│       ├── event_store.rs
│       ├── state_store.rs
│       ├── snapshot.rs
│       └── query.rs
│
├── browseros-dag/                      # DAG Engine
│   ├── Cargo.toml                      #   depends on browseros-types, browseros-event
│   └── src/
│       ├── lib.rs
│       ├── graph.rs
│       ├── engine.rs
│       ├── scheduler.rs
│       └── state.rs
│
├── browseros-plugin/                   # Plugin Registry + Capability Registry
│   ├── Cargo.toml                      #   depends on browseros-types, browseros-event, browseros-lifecycle
│   └── src/
│       ├── lib.rs
│       ├── plugin.rs
│       ├── registry.rs
│       ├── capability.rs
│       └── manifest.rs                 # ComponentManifest
│
├── browseros-lifecycle/                # Lifecycle + Health + Resource tracking
│   ├── Cargo.toml                      #   depends on browseros-types, browseros-event, browseros-observability
│   └── src/
│       ├── lib.rs
│       ├── manager.rs                  # LifecycleManager
│       ├── component.rs                # ManagedComponent trait
│       ├── health.rs                   # HealthCheck, HealthStatus, health registry
│       └── resource.rs                 # ResourceTracker (basic memory tracking)
│
├── browseros-observability/            # Logger + Metrics + Tracing
│   ├── Cargo.toml                      #   depends on browseros-types, browseros-config
│   └── src/
│       ├── lib.rs
│       ├── logger.rs
│       ├── metrics.rs
│       ├── tracer.rs
│       └── export.rs
│
├── browseros-config/                   # Configuration System
│   ├── Cargo.toml                      #   depends on browseros-types
│   └── src/
│       ├── lib.rs
│       ├── config.rs
│       ├── layer.rs
│       ├── source.rs
│       └── validator.rs
│
├── browseros-core/                     # Facade crate — RuntimeContext + re-exports
│   ├── Cargo.toml                      #   depends on all above
│   └── src/
│       ├── lib.rs
│       ├── runtime.rs                  # RuntimeContext struct
│       └── builder.rs                  # RuntimeBuilder (wires everything)
│
└── tests/
    ├── Cargo.toml
    └── tests/
```

### Changes from v1 to Final:

| Crate | v1 | Final | Rationale |
|-------|----|-------|-----------|
| `browseros-error` | Separate | **Merged into types** | Error types are foundational types. No reason to separate. |
| `browseros-di` | Separate | **Removed** | Replaced by RuntimeContext. DI container is speculative complexity for Phase 1. |
| `browseros-resource` | Separate | **Module in lifecycle** | Basic memory tracking is enough for Phase 1. Full resource management in Phase 2. |
| `browseros-scheduler` | Full (with cron) | **Stripped (no cron)** | Cron scheduling is not needed in a browser agent runtime. Delayed + event-triggered is sufficient. |

**Final crate count: 11** (down from 14)

---

## 3. RuntimeContext Design

```rust
/// Shared context passed to every runtime component.
/// Components do NOT wire services manually — they receive this.
pub struct RuntimeContext {
    // ── Core ──────────────────────────────────────────
    pub config: Arc<dyn Config>,
    pub clock: Arc<dyn Clock>,
    pub cancellation: CancellationToken,

    // ── Observability ────────────────────────────────
    pub logger: Arc<Logger>,
    pub metrics: Arc<MetricsRegistry>,
    pub tracer: Arc<Tracer>,

    // ── Communication ────────────────────────────────
    pub event_bus: Arc<dyn EventBus>,
    pub scheduler: Arc<dyn Scheduler>,

    // ── Persistence ──────────────────────────────────
    pub event_store: Arc<dyn EventStore>,
    pub state_store: Arc<dyn StateStore>,

    // ── Discovery ────────────────────────────────────
    pub capabilities: Arc<dyn CapabilityRegistry>,
}
```

**Why each field is in RuntimeContext:**
- `config`: Every component needs config values
- `clock`: Abstract time is the single most important testability feature. Without it, time-dependent tests are flaky.
- `cancellation`: Every async operation must be cancellable. Without it, shutdown leaks tasks.
- `logger/metrics/tracer`: Every component must observe. These are never optional.
- `event_bus`: Primary communication channel. Nearly every component publishes or subscribes.
- `scheduler`: Delayed/retry execution is a cross-cutting need.
- `event_store`/`state_store`: Persistence foundation.
- `capabilities`: Service discovery for plugins and future subsystems.

**What is NOT in RuntimeContext (and why):**
- `PluginRegistry` → plugins use RuntimeContext, they don't own it
- `DagEngine` → specific to execution layer, injected only where needed
- `LifecycleManager` → manages components, not used by components themselves
- `ResourceTracker` → used by LifecycleManager, not by individual components

---

## 4. Component Manifest

```rust
/// Describes a runtime component for registration, dependency resolution,
/// capability discovery, health monitoring, and resource allocation.
pub struct ComponentManifest {
    pub name: String,
    pub version: SemVer,
    pub description: String,

    // Dependencies
    pub dependencies: Vec<ComponentDependency>,       // Other components required
    pub required_capabilities: Vec<CapabilityId>,      // Services this needs

    // Capabilities
    pub provided_capabilities: Vec<CapabilityDefinition>, // Services this provides

    // Resources
    pub resource_requirements: ResourceRequirements,

    // Lifecycle
    pub lifecycle: LifecycleSupport,                    // which lifecycle hooks it implements
    pub health_checks: Vec<HealthCheckDefinition>,
    pub startup_timeout: Duration,
    pub shutdown_timeout: Duration,

    // Config
    pub config_schema: Value,                           // JSON Schema for validation
}
```

**Usage by Runtime:**
1. `RuntimeBuilder` collects manifests from all registered components
2. Validates no dependency cycles
3. Orders components by dependency (topological sort)
4. Allocates resource budgets
5. Registers health checks
6. Registers capabilities
7. Injects config matching component namespace

---

## 5. Health System

### Health States

```
                ┌──────────┐
                │  LOADING │  (manifest read, deps resolving)
                └────┬─────┘
                     │
                ┌────▼─────┐
                │  INIT    │  (on_init called)
                └────┬─────┘
                     │
                ┌────▼─────┐
                │  START   │  (on_start called)
                └────┬─────┘
                     │
          ┌──────────▼──────────┐
          │       READY         │  ─── normal operation
          └──────────┬──────────┘
                     │
          ┌──────────▼──────────┐
          │     DEGRADED        │  ─── health check failed, still serving
          └──────────┬──────────┘
                     │
          ┌──────────▼──────────┐
          │     STOPPING        │  ─── graceful shutdown
          └──────────┬──────────┘
                     │
          ┌──────────▼──────────┐
          │      STOPPED        │  ─── terminated
          └──────────┬──────────┘
                     │
          ┌──────────▼──────────┐
          │      FAILED         │  ─── unrecoverable
          └─────────────────────┘
```

### Readiness vs Liveness

| Check | Question | States Considered Ready/Live |
|-------|----------|------------------------------|
| Readiness | "Can I send work to this component?" | Ready, Degraded |
| Liveness | "Is this component making progress?" | LOADING, INIT, START, READY, DEGRADED, STOPPING |

### Recovery Strategy

| Current State | Health Check Result | Action |
|--------------|-------------------|--------|
| Ready | Pass | Stay Ready |
| Ready | Fail | → Degraded. Log warning. Retry health check in 5s. |
| Degraded | Pass | → Ready. Log recovery. |
| Degraded | Fail (N times) | → Failed. Escalate to LifecycleManager. |
| Failed | N/A | LifecycleManager decides: restart component or fail system. |

### Health Check Interface

```rust
#[async_trait]
pub trait HealthCheck: Send + Sync {
    fn name(&self) -> &'static str;
    async fn check(&self) -> HealthStatus;
}

pub enum HealthStatus {
    Healthy { latency: Duration },
    Degraded { message: String, latency: Duration },
    Unhealthy { message: String, error: Box<dyn Error> },
}
```

---

## 6. Internal Message Protocol (Final)

```rust
/// Universal envelope for all inter-module communication.
/// Every subsystem sends and receives messages in this format.
pub struct MessageEnvelope {
    // MANDATORY — always present. These fields are required for
    // tracing, causality, accountability, and ordering.

    pub id: MessageId,                    // Unique identifier (UUID v7)
    pub correlation_id: CorrelationId,    // Traces entire operation/saga
    pub causation_id: Option<MessageId>,  // What message caused this
    pub source: ModuleId,                 // Who sent it
    pub destination: Option<ModuleId>,    // Specific recipient (None = topic-based routing)
    pub timestamp: DateTime<Utc>,         // When the message was created
    pub payload: Bytes,                   // Serialized content body
    pub content_type: ContentType,        // Identifies payload schema + version

    // CONDITIONAL — set by specific subsystems

    pub priority: Priority,               // Message priority (affects ordering)
    pub ttl: Option<Duration>,            // Message expiry after timestamp+ttl

    // RUNTIME — set by Event Bus infrastructure

    pub sequence: u64,                    // Monotonic sequence number (assigned by bus)
    pub retry_count: u32,                 // Delivery attempt counter

    // TRACE — propagated across module boundaries

    pub trace_context: Option<TraceContext>,  // OpenTelemetry-compatible trace context
}
```

**Mandatory field rationale:**
- `id`: Every message is uniquely identifiable. Enables dedup, logging, and causality tracking.
- `correlation_id`: The single most important field. Without it, you cannot trace an operation across 10+ components. Every message in a saga shares the same correlation_id.
- `causation_id`: What caused this message. Builds the causal DAG for debugging. Without it, you see individual events but can't reconstruct "why".
- `source`: Accountability and debugging. Every component must identify itself.
- `destination`: Direct routing. When set, only the named module processes it.
- `timestamp`: Temporal ordering and TTL enforcement.
- `payload` + `content_type`: Actual message content. ContentType includes schema version for evolution.
- `priority`: Affects processing order. Critical for system events vs informational.

---

## 7. Scheduler (Simplified)

### Decision: Remove cron scheduling. Remove periodic scheduling.

**Rationale:** BrowserOS is a reactive runtime for browser agent operations. Cron scheduling ("run X at 3am daily") is not a requirement. The scheduler exists for:

1. **Delayed execution** — "run this task in 5 seconds" (retry backoff)
2. **Event-triggered execution** — "when event Y occurs, schedule task X"
3. **Retry scheduling** — "retry with exponential backoff"

All three can be implemented on top of tokio timers + Event Bus without a cron parser.

### Simplified API

```rust
pub trait Scheduler: Send + Sync {
    fn schedule_once(&self, delay: Duration, task: ScheduledTask) -> Result<TaskId, SchedulerError>;
    fn schedule_on_event(&self, trigger: EventTrigger, task: ScheduledTask) -> Result<TaskId, SchedulerError>;
    fn cancel(&self, id: TaskId) -> Result<(), SchedulerError>;
    fn list(&self) -> Vec<ScheduledTask>;
}
```

This replaces the original four-method API that included cron and periodic.

---

## 8. Architecture Invariants

> These are the constitution of BrowserOS. Every contributor must uphold them.

### Data Flow

1. **Components communicate only through events.** No direct method calls between runtime components. (Exception: utility crates like `browseros-types`.)

2. **Events are immutable after publication.** No component may modify a published event.

3. **Messages carry correlation_id and causation_id.** Every message in a saga shares the same correlation_id.

4. **Time always comes from an injectable Clock.** Never use `std::time` or `chrono::Utc::now()` directly in any testable component.

5. **No shared mutable state.** All shared state is behind Arc<dyn Trait> with interior mutability.

6. **State is derived from events, not the reverse.** Event Store is the source of truth. State Store is a cache of the current derived state.

### Boundaries

7. **Public APIs are backward compatible within a major version.** Breaking changes require a major version bump.

8. **Every public API is documented.** No undocumented public items.

9. **Every crate has a single public interface module.** Internal modules are `pub(crate)`.

10. **No crate depends on another crate's internal modules.** Cross-crate visibility is always through public traits.

11. **Runtime components never own other runtime components.** Ownership is through Arc references. LifecycleManager coordinates start/stop.

### Testing

12. **Every component is independently testable.** All external dependencies are traits that can be mocked.

13. **Integration tests wire real components, never mocks.** Unit tests use mocks; integration tests use real implementations.

14. **Every state machine validates illegal transitions at compile time or at panic time.** Invalid state transitions must be caught by tests.

15. **Time-dependent tests use a MockClock, not real time.** Tests must not use `tokio::time::sleep`.

### Execution

16. **Every long-running operation supports cancellation.** Operations must check `CancellationToken` periodically.

17. **Every async operation has a configurable timeout.** No unbounded waits.

18. **No blocking I/O in async contexts.** Blocking operations run on `tokio::task::spawn_blocking`.

19. **Panics never cross component boundaries.** Every async task catches panics and converts to errors.

### Observability

20. **Every component exposes metrics.** At minimum: operation count, error count, latency histogram.

21. **Every component produces trace spans.** At minimum: one span per public method.

22. **Errors are never silently swallowed.** All errors are logged at appropriate level and optionally published as events.

23. **Configuration changes are observable.** Config reload emits a `ConfigChanged` event.

### Resource Management

24. **Every component respects resource quotas.** When a quota is exceeded, the component returns `ResourceExceeded` error instead of proceeding.

25. **File handles are always closed when done.** Use RAII wrappers.

### Misc

26. **Plugins never access internal core APIs.** Plugins interact only through the Plugin trait and public capability interfaces.

27. **Default configuration always produces a working system.** No configuration should be required for basic operation.

28. **Feature flags for experimental features.** Experimental APIs are gated behind Cargo features.

29. **The system degrades gracefully.** Failure of one component should not crash the entire runtime.

30. **Thread safety is explicit.** All public traits require `Send + Sync`.

---

## 9. Risk Assessment

### Critical Risks

| Risk | Why | Impact | Mitigation |
|------|-----|--------|------------|
| Event Bus becomes bottleneck | All communication flows through one bus | System-wide throughput limit | InMemoryEventBus uses tokio broadcast channels. Phase 2: partitioned buses. |
| Plugin API too rigid | Plugin trait is defined before real plugins exist | Need to break API in Phase 2 | Keep Plugin trait minimal (init/start/stop). Add methods through capability interfaces, not Plugin trait. |

### High Risks

| Risk | Why | Impact | Mitigation |
|------|-----|--------|------------|
| DAG engine parallel executor complexity | Race conditions in concurrent DAG node execution | Non-deterministic failures, flaky tests | Property-based testing with random DAGs. State machine validation on every transition. |
| State Store delta compression | Hard to get right without real workloads | Suboptimal storage, wasted dev time | Start with full snapshots. Delta compression is a future optimization. |
| RuntimeContext circular dependency | Event Bus needs config, Config needs nothing, but lifecycle needs event bus for lifecycle events | Compile error or runtime deadlock | Dependency graph prevents cycles. Lifecycle events are on a separate internal channel, not the main Event Bus. |

### Medium Risks

| Risk | Why | Impact | Mitigation |
|------|-----|--------|------------|
| Proc macro learning curve | `browseros-macros` requires understanding of Rust proc macros | Slower development | Macros are optional. Manual trait impl works. |
| Scheduler task persistence | Tasks need to survive restarts | Lost scheduled tasks on crash | Phase 1: accept loss. Phase 2: persist to Event Store. |
| Health check overhead | Polling health checks add latency to every operation | Slight performance degradation | Health checks are short (timeboxed). Long checks run on separate interval. |

### Low Risks

| Risk | Why | Impact | Mitigation |
|------|-----|--------|------------|
| JSONL file corruption | Event Store writes to file | Data loss | Atomic writes via temp file + rename. Corruption detection via checksum. |
| Config env prefix collision | BROWSEROS_ prefix conflicts with other tools | Confusing config | Use `BROWSEROS_` prefix. Document all env vars. |

---

## 10. Design Freeze Decision

**Verdict: APPROVED WITH MINOR CHANGES**

### Changes applied during this review:

| Change | From | To |
|--------|------|----|
| `browseros-error` crate | Separate crate | Merged into `browseros-types` |
| `browseros-di` crate | Separate crate | Removed. Replaced by `RuntimeContext`. |
| `browseros-resource` crate | Separate crate | Module in `browseros-lifecycle` |
| `browseros-scheduler` (cron) | Full scheduler with cron | Delayed + event-triggered only |
| Plugin Manifest | Implicit | Explicit `ComponentManifest` |
| Health System | Basic health status | Full state machine with recovery |
| Message Protocol fields | Initial design | Mandatory vs Conditional separation |
| Architecture documentation | Code comments | `Architecture Invariants` document |

### What is explicitly NOT in Phase 1:

- No dynamic plugin loading (FFI dlopen/LoadLibrary)
- No distributed event bus
- No cron scheduling
- No full resource manager (CPU/GPU quotas)
- No DI container
- No browser automation
- No LLM integration
- No World Model
- No Skill System

### Phase 1 is ready for implementation.

The architecture is stable, the invariants are documented, the risks are identified and mitigated. Implementation can begin with confidence that Phase 1 will not require a rewrite in Phase 2.
