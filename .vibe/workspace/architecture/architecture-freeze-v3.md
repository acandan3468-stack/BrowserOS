# Architecture Freeze v3 — BrowserOS

**Status:** FROZEN  
**Date:** 2026-07-08  
**Source:** Direct codebase audit  
**Supersedes:** architecture-v2.md, phase1-freeze.md  

---

## 1. Crate Inventory (14 crates)

### Core Runtime (7 crates)

| # | Crate | Lines | Public API | Status |
|---|-------|-------|-----------|--------|
| 1 | `browseros-types` | ~2,500 | Event trait, MessageEnvelope, BrowserOsError, 15 ID types, Clock, ComponentState, SemVer, Priority | ✅ FROZEN |
| 2 | `browseros-config` | ~700 | Config trait, RootConfig, ConfigLoader, ConfigSource (File/Env/Default), ConfigValidator | ✅ FROZEN |
| 3 | `browseros-observability` | ~1,200 | Logger, MetricsRegistry (Counter/Gauge/Histogram), Tracer (SpanGuard), DiagnosticsCollector, ExportManager | ✅ FROZEN |
| 4 | `browseros-event-bus` | 312 | EventBus (publish/subscribe/unsubscribe), EventHandler trait, InMemoryEventBus | ✅ FROZEN |
| 5 | `browseros-lifecycle` | 385 | LifecycleManager (register/transition/state), LifecycleState (6 states), StateTransitionError | ✅ FROZEN |
| 6 | `browseros-scheduler` | 273 | Scheduler (schedule_after/schedule_on_event/cancel/list), ScheduledTask, SchedulerError | ✅ FROZEN |
| 7 | `browseros-runtime` | 380 | RuntimeContext (10 Arc fields), RuntimeBuilder, RuntimeError | ✅ FROZEN |

### Storage (1 crate)

| # | Crate | Lines | Public API | Status |
|---|-------|-------|-----------|--------|
| 8 | `browseros-storage` | ~200 | StorageManager (start/stop), StorageEvent, StorageError | 🔴 STUB |

### Browser Automation (5 crates)

| # | Crate | Lines | Public API | Status |
|---|-------|-------|-----------|--------|
| 9 | `browseros-bridge` | ~1,500 | 12 Port traits, BridgeError, BridgeResult, LocatorStrategy, 5 ID types, shared types | ✅ FROZEN |
| 10 | `browseros-browser` | ~1,500 | BrowserManager, BrowserProcess, CdpBrowserBackend, BrowserState, BrowserHandle | 🟡 FROZEN (partial) |
| 11 | `browseros-page` | ~2,500 | PageLifecycle, NavigationHistory, PageContentExtractor, DialogAutoHandler, PageWaiter, FrameTree | 🟡 FROZEN (partial) |
| 12 | `browseros-cdp` | ~6,000 | CdpConnection, CdpSession, EventDispatcher, CommandBuilder, CdpError, Transport (WebSocket) | 🟡 FROZEN (partial) |
| 13 | `browseros-dom` | ~3,000 | ElementHandle, ShadowRootHandle, FrameHandle, NodeSnapshot, DomError, DomEvent (30), SelectorEngine, Locator, MutationObserver | ✅ FROZEN |

### Testing (1 crate)

| # | Crate | Lines | Public API | Status |
|---|-------|-------|-----------|--------|
| 14 | `browseros-stress-tests` | ~3,000 | 10 stress/soak/chaos test files | ✅ COMPLETE |

---

## 2. Dependency Graph (Verified from Cargo.toml)

```
browseros-types (zero internal deps)
  │
  ├── browseros-config
  ├── browseros-observability
  │
  ├── browseros-event-bus
  ├── browseros-lifecycle
  ├── browseros-scheduler
  ├── browseros-storage
  │
  ├── browseros-runtime (depends on ALL above)
  │
  ├── browseros-bridge
  ├── browseros-browser
  ├── browseros-page
  ├── browseros-cdp
  ├── browseros-dom
  │
  └── browseros-stress-tests (depends on nearly all)
```

**Circular dependencies: NONE**  
**Architecture violations: NONE** — All dependencies flow downward: types → bridge → domain crates → browser → runtime

---

## 3. RuntimeContext (Frozen)

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

**10 fields, all Arc-based, immutable after construction.**  
**Construction:** `RuntimeContext::init(config)` or `RuntimeContext::builder()`.  
**No setters, no mutation after init.**

---

## 4. EventBus Contract (Frozen)

- **publish(event: impl Event)** — synchronous, in-process delivery to all subscribers
- **subscribe(event_id: EventId, handler: Box<dyn EventHandler>)** → SubscriptionHandle
- **unsubscribe(handle: SubscriptionHandle)** — removes subscription
- **Thread safety:** `Arc<RwLock<HashMap<EventId, Vec<Box<dyn EventHandler>>>>>`
- **No middleware, no dead letter, no routing** (by design)
- **Subscribe by EventId only** (not by category)

---

## 5. Scheduler Contract (Frozen)

- **schedule_after(delay: Duration, task: ScheduledTask)** → TaskId
- **schedule_on_event(event_type: &str, task: ScheduledTask)** → TaskId
- **cancel(id: TaskId)** → Result
- **list()** → Vec<ScheduledTask>
- **In-memory only** — no persistence
- **Uses std::thread** for delayed execution (not tokio)

---

## 6. Lifecycle Contract (Frozen)

- **States:** Created → Initializing → Running → Stopping → Stopped | Degraded | Failed
- **register(name: &str)** — register a component
- **transition(name: &str, to: LifecycleState)** — attempt state transition
- **state(name: &str)** → Option<LifecycleState>
- **Emits LifecycleTransitionEvent** on every transition
- **No ManagedComponent trait** — string-keyed tracking only

---

## 7. Config Contract (Frozen)

- **Three-layer precedence:** Default → File (JSON/YAML) → Env (`BROWSEROS_` prefix)
- **Config trait:** `schema()` + `merge()`
- **RootConfig::for_component::<T>(name)** — typed component config access
- **ConfigLoader** — builder pattern with source registration
- **ConfigValidator** — required field validation

---

## 8. Observability Contract (Frozen)

- **Logger** — structured JSON, level filtering, field attachment
- **MetricsRegistry** — Counter (monotonic), Gauge (point), Histogram (distribution)
- **Tracer** — span-based with SpanGuard (drop-based completion)
- **DiagnosticsCollector** — memory (sysinfo), thread count, uptime
- **ExportManager** — interval-based snapshot (stdout only)
- **No OTLP export** — in-memory only

---

## 9. Bridge Contract (Frozen)

- **12 Port traits** in browseros-bridge, implementations in browseros-cdp
- **No CDP type leaks** into any crate except browseros-cdp
- **Synchronous API** — all methods block calling thread
- **Async transport** — CDP reader thread handles WebSocket

### Port Traits (all frozen)

| Trait | Methods | CDP Impl |
|-------|---------|----------|
| BrowserPort | 7 | ✅ |
| SessionPort | 4 | ✅ |
| PagePort | 8 | ✅ |
| FramePort | 8 | ✅ |
| ElementPort | ~20 | ✅ |
| DialogPort | 3 | ✅ |
| DownloadPort | 5 | ✅ |
| InputPort | 5 | ❌ |
| NetworkPort | 8 | ❌ |
| StoragePort | 6 | ❌ |
| LocatorPort | 3 | ❌ |
| ArtifactPort | 5 | ❌ |

---

## 10. DOM Contract (Frozen)

- **ElementHandle** — generation-based stale detection (`known_generation: u64`)
- **ShadowRootHandle** — `is_closed()`
- **FrameHandle** — `snapshot(max_depth, selector_filter)`, `snapshot_all()`
- **NodeSnapshot** — `from_node_info_depth()`
- **DomError** — `#[non_exhaustive]` (Stale, NotFound, MultipleFound, InvalidSelector, ClosedShadowRoot, CrossOriginFrame)
- **DomEvent** — 30 variants + DomOperation enum
- **ElementCollection** — Static default, Live opt-in
- **No EventBus dependency** — DOM defines event payload types only
- **MutationObserver** — polling-based via generation counter

---

## 11. Ownership Rules

1. **All shared state behind Arc** — No component owns another
2. **RuntimeContext is read-only** after construction
3. **BrowserProcess owns OS child process** — only exception
4. **No global statics** — everything through RuntimeContext
5. **Send + Sync on all public traits**

---

## 12. Concurrency Model

1. **Synchronous core** — all public APIs are sync
2. **CDP reader thread** — async WebSocket → sync EventBus dispatch
3. **Interior mutability** — RwLock (read-heavy), Mutex (write-heavy)
4. **CancellationToken** — cooperative cancellation
5. **No async runtime** in core crates

---

## 13. Forbidden Dependencies

- Phase 1 crates (event-bus, lifecycle, scheduler, runtime) MUST NOT depend on Phase 2 crates (bridge, cdp, browser, page, dom)
- browseros-types MUST NOT depend on any other internal crate
- browseros-bridge MUST NOT depend on browseros-cdp
- browseros-dom MUST NOT depend on browseros-event-bus

---

## 14. Extension Points

| Extension | Mechanism | Status |
|-----------|-----------|--------|
| New browser backend | Implement Port traits | ✅ Ready |
| New event types | Implement Event trait | ✅ Ready |
| New config sources | Implement ConfigSource | ✅ Ready |
| New config validators | Implement ConfigValidator | ✅ Ready |
| New observability export | Implement ExportBackend | 🟡 Partial |
| Plugin system | (future: browseros-plugin) | ❌ Missing |
| DAG engine | (future: browseros-dag) | ❌ Missing |

---

## 15. Change Process

Any change to a frozen API requires:

1. **ADR** — Architecture Decision Record
2. **Impact analysis** — affected crates and consumers
3. **Migration path** — how existing code updates
4. **Review** — at least one other contributor

**Not frozen (may change without ADR):**
- Internal impl details (pub(crate))
- Test utilities
- Error messages
- browseros-storage (stub, will be replaced)
- browseros-stress-tests (testing only)
- Future crates (dag, plugin, network, input, artifact)