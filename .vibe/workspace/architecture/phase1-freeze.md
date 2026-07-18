# Phase 1 Freeze — BrowserOS

**Status:** FROZEN  
**Date:** 2026-07-08  
**Supersedes:** .vibe/workspace/plans/design-freeze.md  

---

## Freeze Declaration

The following crates, APIs, and interfaces are **frozen** and must not be changed without an Architecture Decision Record (ADR).

---

## Frozen Crates

| Crate | Version | Freeze Date | Notes |
|-------|---------|-------------|-------|
| `browseros-types` | 0.1.0 | 2026-07-08 | All public types, traits, macros |
| `browseros-config` | 0.1.0 | 2026-07-08 | Config trait, RootConfig, ConfigLoader |
| `browseros-observability` | 0.1.0 | 2026-07-08 | Logger, MetricsRegistry, Tracer |
| `browseros-event-bus` | 0.1.0 | 2026-07-08 | EventBus, EventHandler, InMemoryEventBus |
| `browseros-lifecycle` | 0.1.0 | 2026-07-08 | LifecycleManager, LifecycleState |
| `browseros-scheduler` | 0.1.0 | 2026-07-08 | Scheduler, ScheduledTask |
| `browseros-runtime` | 0.1.0 | 2026-07-08 | RuntimeContext, RuntimeBuilder |
| `browseros-bridge` | 0.1.0 | 2026-07-08 | All 12 Port traits, BridgeError, identifiers |
| `browseros-dom` | 0.1.0 | 2026-07-08 | ElementHandle, DomError, DomEvent, all frozen APIs |

---

## Frozen Public APIs

### browseros-types
- `Event` trait, `EventMetadata`, `EventCategory`, `EventKind`
- `MessageEnvelope`, `MessageEnvelopeBuilder`
- `BrowserOsError`, `ErrorKind`, `ErrorSeverity`, `ErrorContext`, `RetryPolicy`
- All ID types: `EventId`, `MessageId`, `CorrelationId`, `CausationId`, `TaskId`, `ExecutionId`, `SubscriptionHandle`, `HandleId`, `NodeId`, `PluginId`, `CapabilityId`, `ServiceId`, `EntityId`, `VersionId`, `StreamPosition`, `ModuleId`
- `Clock` trait, `SystemClock`, `MockClock`, `CancellationToken`, `Deadline`
- `ComponentState`, `HealthStatus`, `ComponentManifest`, `CapabilityDefinition`, `ResourceRequirements`
- `ModuleDescriptor`, `ModuleType`
- `SemVer`, `ContentType`, `Priority`, `DeliveryGuarantee`, `LogLevel`, `ErrorCode`

### browseros-config
- `Config` trait, `RootConfig`, `ConfigLoader`, `ConfigSource`, `ConfigValidator`

### browseros-observability
- `Logger`, `MetricsRegistry` (Counter, Gauge, Histogram), `Tracer`, `SpanGuard`, `DiagnosticsCollector`, `ExportManager`

### browseros-event-bus
- `EventBus` (publish, subscribe, unsubscribe), `EventHandler` trait, `SubscriptionHandle`, `EventBusError`

### browseros-lifecycle
- `LifecycleManager` (register, transition, state), `LifecycleState` enum, `StateTransitionError`

### browseros-scheduler
- `Scheduler` (schedule_once, schedule_on_event, cancel, list), `ScheduledTask`, `SchedulerError`

### browseros-runtime
- `RuntimeContext` struct (all 10 fields), `RuntimeBuilder`, `RuntimeError`

### browseros-bridge
- All 12 Port traits: `BrowserPort`, `SessionPort`, `PagePort`, `FramePort`, `ElementPort`, `DialogPort`, `DownloadPort`, `InputPort`, `NetworkPort`, `StoragePort`, `LocatorPort`, `ArtifactPort`
- `BridgeError`, `BridgeResult`, `LocatorStrategy`, `LocatorOptions`
- All bridge types: `BrowserId`, `SessionId`, `PageId`, `FrameId`, `ElementId`, `BoxModel`, `Point`, `NodeInfo`, `LaunchOptions`, `SessionConfig`

### browseros-dom
- `ElementHandle`, `ShadowRootHandle`, `FrameHandle`, `NodeSnapshot`, `ElementCollection`
- `DomError` (all variants), `DomEvent` (all 30 variants), `DomOperation`
- `SelectorEngine`, `Locator`, `MutationObserver`, `DomState`, `StyleCalculator`, `Traversal`

---

## Frozen RuntimeContext Interface

```rust
pub struct RuntimeContext {
    pub config: Arc<RootConfig>,
    pub clock: Arc<dyn Clock>,
    pub cancellation: CancellationToken,
    pub logger: Arc<Logger>,
    pub metrics: Arc<MetricsRegistry>,
    pub tracer: Arc<Tracer>,
    pub event_bus: Arc<EventBus>,
    pub scheduler: Arc<Scheduler>,
    pub lifecycle: Arc<LifecycleManager>,
    pub storage_manager: Arc<StorageManager>,
}
```

**No fields may be added or removed without an ADR.**

---

## Frozen EventBus Contract

- `publish(event: Event)` — synchronous, in-process delivery
- `subscribe(event_id: EventId, handler: Box<dyn EventHandler>)` — returns `SubscriptionHandle`
- `unsubscribe(handle: SubscriptionHandle)` — removes subscription
- Events are immutable after publication
- No middleware, no dead letter, no routing (by design — may be added via ADR)

---

## Frozen Scheduler Contract

- `schedule_once(delay: Duration, task: ScheduledTask)` — returns `TaskId`
- `schedule_on_event(trigger: EventTrigger, task: ScheduledTask)` — returns `TaskId`
- `cancel(id: TaskId)` — cancels scheduled task
- `list()` — lists all scheduled tasks
- In-memory only (no persistence)

---

## Frozen Config Contract

- Three-layer precedence: Default → File (JSON/YAML) → Env (`BROWSEROS_` prefix)
- `Config` trait with `schema()` and `merge()`
- `RootConfig::for_component::<T>(name)` for component config access
- Required field validation via `ConfigValidator`

---

## Frozen Observability Contract

- `Logger` with structured JSON output and level filtering
- `MetricsRegistry` with Counter, Gauge, Histogram
- `Tracer` with span-based tracing and `SpanGuard`
- In-memory storage (no OTLP export — may be added via ADR)

---

## Frozen DOM Contracts

- Generation-based stale detection (`known_generation: u64`)
- No auto-repair — explicit re-acquisition via locator
- `ElementCollection` defaults to Static (Live is opt-in)
- No EventBus dependency — DOM defines event payload types only
- `#[non_exhaustive]` on `DomError` and `LocatorStrategy`

---

## Frozen Browser Contracts

- Bridge pattern: traits in `browseros-bridge`, implementations in `browseros-cdp`
- No CDP type leaks into any crate except `browseros-cdp`
- Synchronous API with async transport (CDP reader thread)
- `#[non_exhaustive]` on `LocatorStrategy`

---

## Change Process

Any change to a frozen API requires:

1. **ADR** — Architecture Decision Record documenting the change
2. **Impact analysis** — Which crates and consumers are affected
3. **Migration path** — How existing code will be updated
4. **Review** — At least one other contributor must approve

---

## Not Frozen (May Change Without ADR)

- Internal implementation details (`pub(crate)` items)
- Test utilities and test helpers
- Error messages and log messages
- Documentation comments
- `browseros-storage` (explicitly a stub — will be replaced)
- `browseros-stress-tests` (testing crate, no public API)
- Future crates: `browseros-dag`, `browseros-plugin`, `browseros-network`, `browseros-input`, `browseros-artifact`