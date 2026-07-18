# BrowserOS Architecture — v2 (Canonical)

**Status:** LIVE — This document is the single source of truth for BrowserOS architecture.  
**Supersedes:** phase2-architecture.md, architecture-review.md, design-freeze.md  

---

## 1. Architecture Overview

BrowserOS is an **event-driven modular runtime** for browser agent orchestration. It follows a **layered bridge pattern**:

```
┌──────────────────────────────────────────────────┐
│              Future: Agents / CLI / API           │
├──────────────────────────────────────────────────┤
│              High-Level Subsystems                │
│  DOM │ Page │ Storage │ (Network) │ (Input)       │
├──────────────────────────────────────────────────┤
│              Bridge Layer (Traits)                 │
│  12 Port traits (BrowserPort, SessionPort, ...)   │
├──────────────────────────────────────────────────┤
│          Protocol Implementation (CDP)            │
├──────────────────────────────────────────────────┤
│             Chrome / Chromium                      │
└──────────────────────────────────────────────────┘
```

### Key Principles

1. **Protocol independence** — Bridge layer is pure traits. No CDP type leaks into any crate except `browseros-cdp`.
2. **Synchronous core** — All browser operations block their calling thread. The agent model explicitly expects this.
3. **Event-driven observation** — Browser state changes → EventBus events. Agents observe through events, command through API calls.
4. **RuntimeContext is the composition root** — Every subsystem is reachable through RuntimeContext. No global statics, no DI container.
5. **No async runtime** — Internal bridging threads handle CDP→EventBus translation. Consumer interface is synchronous.

---

## 2. Crate Inventory (14 crates)

### Core Runtime (7 crates)

| Crate | Status | Purpose |
|-------|--------|---------|
| `browseros-types` | ✅ Complete | Canonical types, event hierarchy, message protocol, error system, identifiers, clock |
| `browseros-config` | ✅ Complete | Layered configuration (File/Env/Default), validation, component config registry |
| `browseros-observability` | ✅ Complete | Logger, MetricsRegistry (counter/gauge/histogram), Tracer (span-based), Diagnostics |
| `browseros-event-bus` | ✅ Complete | In-process pub/sub EventBus. No middleware, no dead letter, no routing |
| `browseros-lifecycle` | 🟠 Partial | Lifecycle state machine (6 states). No ManagedComponent trait, no health system |
| `browseros-scheduler` | ✅ Complete | Delayed + event-triggered task execution. In-memory only, no persistence |
| `browseros-runtime` | ✅ Complete | RuntimeContext composition root + RuntimeBuilder |

### Storage (1 crate)

| Crate | Status | Purpose |
|-------|--------|---------|
| `browseros-storage` | 🔴 Stub | StorageManager scaffold only. No EventStore, no StateStore, no persistence |

### Browser Automation (5 crates)

| Crate | Status | Purpose |
|-------|--------|---------|
| `browseros-bridge` | ✅ Complete (design) | 12 Port traits defining protocol abstraction. No implementations |
| `browseros-browser` | 🟠 Partial | Browser process management, lifecycle, CDP backend |
| `browseros-page` | 🟠 Partial | Page lifecycle, navigation, content extraction, dialog handling, waiter |
| `browseros-cdp` | 🟡 Mostly Complete | Chrome DevTools Protocol: WebSocket transport, command execution, event dispatch |
| `browseros-dom` | 🟡 Mostly Complete (FROZEN) | DOM abstraction: element handles, shadow DOM, snapshots, events, mutation observation |

### Testing (1 crate)

| Crate | Status | Purpose |
|-------|--------|---------|
| `browseros-stress-tests` | ✅ Complete | 10 stress/soak/chaos test files with analysis reports |

### Missing (not implemented)

| Planned Crate | Status | Blocked By |
|---------------|--------|------------|
| `browseros-dag` | ❌ Missing | Nothing — ready to implement |
| `browseros-plugin` | ❌ Missing | DAG engine |
| `browseros-macros` | ❌ Missing | Nothing — ready to implement |
| `browseros-network` | ❌ Missing (designed) | DAG engine (recommended order) |
| `browseros-input` | ❌ Missing | Network crate |
| `browseros-artifact` | ❌ Missing | Network crate |

---

## 3. Dependency Graph (Actual)

```
browseros-types (zero deps beyond serde/chrono/uuid/thiserror)
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

**No circular dependencies.** Direction is always: types → bridge → domain crates → browser → runtime.

---

## 4. Runtime Architecture

### 4.1 RuntimeContext

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

**Construction:** Via `RuntimeBuilder` — creates default or custom components, wires them, returns `Result<RuntimeContext, RuntimeError>`.

**Immutability:** All fields are `pub` but behind `Arc`. No setters. Read-only after construction.

### 4.2 EventBus

- **Implementation:** In-memory, synchronous pub/sub
- **Subscribe by:** `EventId` (UUID) — NOT by category or type
- **No middleware, no dead letter, no routing**
- **Thread safety:** `Arc<RwLock<HashMap<EventId, Vec<Box<dyn EventHandler>>>>>`

### 4.3 LifecycleManager

- **States:** Uninitialized → Initializing → Running → Stopping → Stopped | Failed
- **Tracking:** String-keyed `HashMap<String, LifecycleState>`
- **Missing:** ManagedComponent trait, health checks, resource tracking, dependency ordering

### 4.4 Scheduler

- **API:** `schedule_once(delay, task)`, `schedule_on_event(trigger, task)`, `cancel(id)`, `list()`
- **Implementation:** `tokio::spawn` for delayed tasks, EventBus subscription for event-triggered
- **No persistence, no retry, no priority ordering**

### 4.5 Config System

- **Sources:** Default → File (JSON/YAML) → Env (`BROWSEROS_` prefix)
- **Access:** `RootConfig::for_component::<T>("name")`
- **Validation:** Required field checks via `ConfigValidator` trait

### 4.6 Observability

- **Logger:** Structured JSON with level filtering
- **MetricsRegistry:** Counters, Gauges, Histograms (in-memory)
- **Tracer:** Span-based with `SpanGuard` (drop-based completion)
- **Export:** In-memory only — no OTLP export yet

---

## 5. Bridge Pattern

```
browseros-bridge (traits only)
     ↑                    ↑
browseros-cdp       Future backends
(Chromium CDP)       (Playwright, WebDriver BiDi)
```

**12 Port traits:**
1. `BrowserPort` — Launch/close/kill browser, create sessions
2. `SessionPort` — Session lifecycle, page management
3. `PagePort` — Navigation, content, screenshots, PDF
4. `FramePort` — Frame info, content, child frames
5. `ElementPort` — Query, attributes, visibility, interaction
6. `DialogPort` — Dialog inspection, accept/dismiss
7. `DownloadPort` — Download tracking, cancellation
8. `InputPort` — Keyboard, mouse, file upload (no CDP impl)
9. `NetworkPort` — Request interception, cookies (no CDP impl)
10. `StoragePort` — Cookies, local/session storage (no CDP impl)
11. `LocatorPort` — Multi-strategy element location (no CDP impl)
12. `ArtifactPort` — File artifact storage/retrieval (no CDP impl)

**Implemented in CDP:** BrowserPort, SessionPort, PagePort, FramePort, ElementPort, DialogPort, DownloadPort

---

## 6. CDP Thread Model

```
CDP WebSocket → Reader Thread → Channel → Dispatcher → EventBus
```

- Single `CdpConnection` owns one WebSocket + one reader thread
- Reader thread deserializes CDP events → internal channel
- Dispatcher thread translates CDP events → BrowserOS events on EventBus
- Bridge trait methods are synchronous — calling thread blocks until response arrives

---

## 7. Concurrency Model

- **All public traits require `Send + Sync`**
- **Ownership through `Arc`** — Every shared component is `Arc<T>`
- **Interior mutability** via `RwLock` (read-heavy) and `Mutex` (write-heavy)
- **`parking_lot::RwLock`** in EventBus
- **`std::sync::Mutex`** in Scheduler
- **Cancellation** via `CancellationToken` (cooperative, non-blocking check)

---

## 8. Event Flow

```
Component A → MessageEnvelope → EventBus.publish() → Subscribers
                                                           ↓
                                                    EventHandler::handle(Event)
```

**MessageEnvelope fields:**
- `id: MessageId` (UUID v7)
- `correlation_id: CorrelationId` — Traces multi-step operations
- `causation_id: Option<MessageId>` — Causal relationship
- `source: ModuleId` — Who sent it
- `destination: Option<ModuleId>` — Specific recipient
- `timestamp: DateTime<Utc>`
- `payload: Vec<u8>` — Serialized event body
- `content_type: ContentType` — Schema identifier + version
- `priority: Priority` — Low/Normal/High/Critical
- `ttl: Option<Duration>` — Expiry
- `trace_context: Option<TraceContext>` — OpenTelemetry propagation

---

## 9. DOM Layer (Phase 2.5 — Frozen)

- **ElementHandle** with generation-based stale detection (`known_generation: u64`)
- **ShadowRootHandle** with `is_closed()`
- **FrameHandle** with snapshot API (`snapshot(max_depth, selector_filter)`)
- **NodeSnapshot** with `from_node_info_depth()`
- **30 DomEvent variants** + `DomOperation` enum
- **ElementCollection** (static default, live opt-in)
- **MutationObserver** (polling-based via generation counter)
- **No EventBus dependency** — DOM crate defines event payload types only

---

## 10. Execution Layer (Missing)

### DAG Engine (browseros-dag) — NOT IMPLEMENTED
- Required for task orchestration, parallel execution, retry logic
- No dependencies on browser crates — pure runtime crate
- Recommended next milestone

### Plugin System (browseros-plugin) — NOT IMPLEMENTED
- PluginRegistry + CapabilityRegistry
- Depends on DAG engine for task execution

---

## 11. Future Layers (Not Started)

| Layer | Prerequisites |
|-------|---------------|
| Network (Phase 2.6) | DAG engine |
| Perception (Vision, Console) | Network, DOM |
| Planning | DAG, Plugin |
| Skills | Planning, Perception |
| LLM Integration | Planning, Skills |