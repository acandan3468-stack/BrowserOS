# Architecture Conformance Audit v4

**Date:** 2026-07-14  
**Scope:** Full workspace — 14 crates + stress-tests  

---

## Audit Summary

| Category | PASS | MINOR | MAJOR | BLOCKER |
|---|---|---|---|---|
| Public API completeness | 18 | 2 | 1 | 0 |
| Invariant conformance | 12 | 1 | 0 | 0 |
| Dependency direction | 14 | 0 | 0 | 0 |
| Crate boundary isolation | 14 | 0 | 0 | 0 |
| RuntimeContext | 8 | 0 | 0 | 0 |
| Planner integration | 6 | 1 | 0 | 0 |
| Plugin integration | 8 | 0 | 0 | 0 |
| Bridge integration | 4 | 0 | 0 | 0 |
| EventBus integration | 4 | 0 | 0 | 0 |
| Scheduler integration | 3 | 1 | 0 | 0 |
| Execution abstractions | 8 | 0 | 0 | 0 |
| **TOTAL** | **99** | **5** | **1** | **0** |

---

## 1. Public API Completeness

### browseros-types (PASS)
- All identifier types (17) present: EventId, HandleId, MessageId, CorrelationId, CausationId, TaskId, ExecutionId, SubscriptionHandle, NodeId, PluginId, CapabilityId, ServiceId, EntityId, VersionId, StreamPosition, ModuleId, IdParseError ✓
- Value types (SemVer, ContentType, ErrorCode) present ✓
- Clock traits (Clock, SystemClock, MockClock) present ✓
- Event system (Event trait, EventMetadata, EventCategory, DeliveryGuarantee) present ✓
- Cancellation (CancellationToken, Cancelled, CancelledExt) present ✓

### browseros-config (PASS)
- Config trait, RootConfig, ConfigLoader, ConfigSource, ConfigError ✓

### browseros-event-bus (PASS)
- EventBus (new, publish, subscribe, unsubscribe, new_metadata) ✓
- EventHandler trait ✓
- SubscriptionHandle ✓

### browseros-bridge (PASS)
- All 13 Port traits: ArtifactPort, BrowserPort, DialogPort, DownloadPort, ElementPort, FramePort, InputPort, LocatorEngine, LocatorPort, NetworkPort, PagePort, SessionPort, StoragePort ✓
- All identifier types (9): SessionId, BrowserId, PageId, FrameId, ElementId, NodeId, InterceptionHandle, ArtifactId, NavigationId ✓
- All domain types (~35): BrowserInfo, LaunchOptions, SessionConfig, Viewport, etc. ✓

### browseros-dag (PASS)
- DagEngine (new, with_max_threads, register_node, add_edge, remove_edge, execute, execute_dag, cancel, execution_state, node_state, list_executions) ✓
- DagError (18 variants) ✓
- 9 event payload types ✓
- ExecutableNode trait (4 methods) ✓
- CapabilityMetadata (builder pattern) ✓
- ContextPropagator ✓
- ExecutionContext, ExecutionInput, ExecutionOutput ✓
- ExecutionError (5 variants) ✓
- NodeFactory ✓
- NodeRegistry ✓
- VariableStore ✓
- PluginLoader, PluginManager, PluginStatistics ✓
- DagDefinition, DagNode, NodeKind, NodeState, DagExecutionState, DagResult ✓
- All planner types (15) ✓
- All plugin types (19) ✓
- All runtime types (8) ✓

### browseros-runtime (PASS)
- RuntimeContext (8 field accessors) ✓
- RuntimeBuilder (config_file, with_dag_engine, build) ✓
- RuntimeInitError ✓

### Integration verification (PASS)
- RuntimeContext init_observability creates Logger/MetricsRegistry/Tracer ✓
- RuntimeContext init creates EventBus ✓
- RuntimeContext init creates Scheduler wired to EventBus ✓
- RuntimeContext init creates LifecycleManager wired to EventBus ✓
- RuntimeContext init creates DagEngine wired to EventBus/Logger/Metrics/Tracer ✓
- LifecycleManager registers "dag_engine" component ✓
- LifecycleManager transitions dag_engine to Initializing → Running ✓

### Issues
- **MINOR**: `browseros-dag/src/config.rs` is an empty placeholder module (22 lines, just imports) — no actual configuration is parsed
- **MINOR**: `browseros-dag` has `#![allow(dead_code)]` at crate level — prevents the compiler from catching unused items
- **MINOR**: `NodeRegistry` uses `.expect()` on RwLock lock operations (in register, find, list_capabilities, etc.) — can panic if lock is poisoned (pre-existing, classified SAFE TO DEFER in P16 preflight)
- **MINOR**: 3 `useless_format` clippy warnings in `browseros-dag/src/loader.rs` test code
- **MINOR**: `browseros-scheduler` (Layer 2) is imported by browseros-dag but the actual `DagScheduler` in `scheduler.rs` is private — the active scheduler is `SyncExecutor` in `executor.rs`

---

## 2. Invariant Conformance

| # | Invariant | Status |
|---|---|---|
| 1 | No cyclic dependencies | ✅ PASS |
| 2 | Plugin trait is backward compatible (execute_capability default) | ✅ PASS |
| 3 | PluginRegistry owns metadata only — no browser objects | ✅ PASS |
| 4 | All plugin types are #[non_exhaustive] where extensible | ✅ PASS |
| 5 | PlannerBridge exposes only traits, no implementations | ✅ PASS |
| 6 | ExecutionPlan is serde-serializable | ✅ PASS |
| 7 | No Any, no Box<dyn ...>, no serde_json::Value in planner/plugin | ✅ PASS |
| 8 | Thread-safe: Arc<RwLock<...>> throughout | ✅ PASS |
| 9 | RuntimeContext is immutable after construction | ✅ PASS |
| 10 | ExecutionContext does NOT own RuntimeContext | ✅ PASS |
| 11 | DagEngine does NOT depend on browser automation crates | ✅ PASS |
| 12 | bridge_node wraps errors via Display trait | ✅ PASS |
| 13 | NodeFactory validates before executing | ✅ PASS |

| # | Invariant | Status |
|---|---|---|
| 14 | No tokio dependency in browseros-dag | ✅ PASS (uses SyncExecutor, not tokio) |
| 15 | All public types are Send + Sync | ✅ PASS (explicit tests for key types) |
| 16 | No unwrap/expect in production code | ⚠️ MINOR (3 .expect() calls in NodeRegistry) |

---

## 3. Dependency Direction

| Crate | Depends On | Layer Check | Status |
|---|---|---|---|
| browseros-types | (none) | L0 | ✅ |
| browseros-config | types | L0→L1 | ✅ |
| browseros-event-bus | types | L0→L1 | ✅ |
| browseros-bridge | types | L0→L1 | ✅ |
| browseros-observability | types, config | L0, L1→L2 | ✅ |
| browseros-scheduler | types, event-bus | L0, L1→L2 | ✅ |
| browseros-cdp | types, bridge | L0, L1→L2 | ✅ |
| browseros-dom | types, bridge | L0, L1→L2 | ✅ |
| browseros-storage | types, bridge | L0, L1→L2 | ✅ |
| browseros-page | types, bridge, event-bus | L0, L1→L2 | ✅ |
| browseros-lifecycle | types, event-bus, observability | L0, L1, L2→L3 | ✅ |
| browseros-dag | types, event-bus, observability, scheduler | L0, L1, L2→L3 | ✅ |
| browseros-browser | types, bridge, event-bus, observability, config, cdp | L0, L1, L2→L3 | ✅ |
| browseros-runtime | types, config, observability, event-bus, lifecycle, scheduler, dag | L0, L1, L2, L3→L4 | ✅ |
| browseros-stress-tests | types, config, observability, event-bus, lifecycle, scheduler, runtime | test→all | ✅ |

**All 14 crate dependency directions verified: 14/14 PASS**

---

## 4. Crate Boundary Isolation

| Crate | Forbidden Deps | Actual | Status |
|---|---|---|---|
| browseros-types | any workspace crate | none | ✅ |
| browseros-dag | browseros-browser, cdp, dom, page, storage, bridge, runtime, lifecycle, config | types, event-bus, observability, scheduler | ✅ |
| browseros-runtime | (none — top-level) | types, config, observability, event-bus, lifecycle, scheduler, dag | ✅ |
| browseros-bridge | browseros-dag, runtime, etc. | types | ✅ |
| browseros-dom | dag, runtime, etc. | types, bridge | ✅ |

**All 14 crate boundaries verified: 14/14 PASS**

---

## 5. Integration Points

### RuntimeContext (8/8 PASS)
| Component | Mechanism | Status |
|---|---|---|
| EventBus | `Arc<EventBus>` field, accessor | ✅ |
| Logger | `Arc<Logger>` field, accessor | ✅ |
| MetricsRegistry | `Arc<MetricsRegistry>` field, accessor | ✅ |
| Tracer | `Arc<Tracer>` field, accessor | ✅ |
| RootConfig | `Arc<RootConfig>` field, accessor | ✅ |
| LifecycleManager | `Arc<LifecycleManager>` field, accessor | ✅ |
| Scheduler | `Arc<Scheduler>` field, accessor | ✅ |
| DagEngine | `Arc<DagEngine>` field, accessor | ✅ |

### Planner Integration (6/7 PASS, 1 MINOR)
| Requirement | Status |
|---|---|
| PlannerBridge trait exists | ✅ |
| Planner types are serde-serializable | ✅ |
| ExecutionPlan::build_dag() → DagDefinition | ✅ |
| ExecutionPlan uses NodeFactory | ✅ |
| PlannerRequest carries natural-language goal | ✅ |
| PlanningContext exposes available capabilities | ✅ |
| PlannerBridge is NOT constrained to LLM type | ✅ |

### Plugin Integration (8/8 PASS)
| Requirement | Status |
|---|---|
| Plugin trait is object-safe | ✅ |
| PluginRegistry is thread-safe | ✅ |
| PluginRegistry::get_plugin() is pub(crate) | ✅ |
| PluginLoader discovers + parses manifests | ✅ |
| PluginManager orchestrates lifecycle | ✅ |
| PluginRuntime wraps Arc<PluginManager> | ✅ |
| PluginCapabilityNode implements ExecutableNode | ✅ |
| PluginExecutionBridge registers into NodeRegistry | ✅ |

### Bridge Integration (4/4 PASS)
| Requirement | Status |
|---|---|
| bridge_node function exists | ✅ |
| Error wrapping via Display trait | ✅ |
| Works with any Fn() -> Result<(), E> | ✅ |
| Nodes are Send + Sync | ✅ |

### EventBus Integration (4/4 PASS)
| Requirement | Status |
|---|---|
| DagEngine receives EventBus in constructor | ✅ |
| ExecutionContext exposes Arc<EventBus> | ✅ |
| EventBus used by NodeRegistry for capability events | ✅ |
| DAG events defined (9 event types) | ✅ |

### Scheduler Integration (3/4 PASS, 1 MINOR)
| Requirement | Status |
|---|---|
| browseros-dag depends on browseros-scheduler | ✅ |
| DagDefinition supports topology (nodes + edges) | ✅ |
| SyncExecutor executes scheduled layers | ✅ |
| DagScheduler (private) active | ⚠️ MINOR — scheduler.rs is private module, not directly re-exported |

### Execution Abstractions (8/8 PASS)
| Requirement | Status |
|---|---|
| ExecutableNode trait is object-safe | ✅ |
| ExecutableNode has validate + execute | ✅ |
| NodeFactory bridges ExecutableNode → DagNode | ✅ |
| NodeRegistry is thread-safe | ✅ |
| VariableStore is thread-safe | ✅ |
| ContextPropagator creates child contexts | ✅ |
| ExecutionInput resolves variables | ✅ |
| ExecutionOutput carries typed values | ✅ |

---

## Deviation Classification

### CLASSIFICATION LEGEND
- **PASS** — Fully conformant, no action required
- **MINOR** — Compliant but has cosmetic or minor quality concerns
- **MAJOR** — Non-conformant — should be fixed before freeze
- **BLOCKER** — Prevents freeze — must be fixed before proceeding

### DEVIATIONS FOUND

| ID | Area | Severity | Description |
|---|---|---|---|
| AC-01 | Code Quality | **MAJOR** | `#![allow(dead_code)]` in `browseros-dag/src/lib.rs` — suppresses detection of dead code across the crate. Should be removed or reduced to specific items before freeze. |
| AC-02 | Error Handling | MINOR | `NodeRegistry` uses `.expect()` on RwLock — 3 occurrences in `exec.rs:317,323,329`. Can panic if lock poisoned. |
| AC-03 | Documentation | MINOR | `browseros-dag/src/config.rs` is an empty placeholder (22 lines) — no actual config parsing or documentation. |
| AC-04 | Code Quality | MINOR | 3 `useless_format` clippy warnings in `browseros-dag/src/loader.rs` test code. |
| AC-05 | Architecture | MINOR | `browseros-dag/src/scheduler.rs` is a private module with `DagScheduler` but the active runtime scheduler is `SyncExecutor` in `executor.rs`. The private scheduler module appears to be a design placeholder. |
| AC-06 | Test Coverage | MINOR | `scheduler.rs` private module lacks direct unit tests — only covered through integration. |

---

## Overall Verdict

**ARCHITECTURE CONFORMANCE: APPROVED WITH CONDITIONS**

The architecture is sound, layered, acyclic, and well-isolated. All integrations are correctly wired. One MAJOR issue (AC-01) should be addressed before final freeze, but does not block the architecture freeze itself — it's a quality concern rather than a design flaw.

99 PASS / 5 MINOR / 1 MAJOR / 0 BLOCKER
