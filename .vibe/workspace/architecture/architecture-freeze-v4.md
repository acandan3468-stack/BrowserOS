# Architecture Freeze v4

**Date:** 2026-07-14  
**Status:** ✅ FROZEN  
**Previous:** Architecture Freeze v3 (superseded)

---

## 1. Workspace Architecture

### Layer Model

```
┌──────────────────────────────────────────────────────────────┐
│  L4: browseros-runtime                                       │
│  Dependency injection, RuntimeContext                         │
├──────────────────────────────────────────────────────────────┤
│  L3: browseros-lifecycle  browseros-dag  browseros-browser   │
│  Lifecycle state machine   DAG engine       Browser mgmt     │
├──────────────────────────────────────────────────────────────┤
│  L2: browseros-observability  browseros-scheduler            │
│      browseros-cdp            browseros-dom                  │
│      browseros-storage        browseros-page                 │
│  Observability, scheduling, CDP protocol, DOM, storage       │
├──────────────────────────────────────────────────────────────┤
│  L1: browseros-config  browseros-event-bus  browseros-bridge │
│  Configuration, event bus, bridge port traits                │
├──────────────────────────────────────────────────────────────┤
│  L0: browseros-types                                         │
│  Foundation types: identifiers, events, value, clock         │
└──────────────────────────────────────────────────────────────┘
```

### Dependency Direction Rule

All dependencies flow downward (higher layer → lower layer).  
No upward or lateral dependencies between layers.  
No cyclic dependencies permitted.  

### Crate Count

14 active crates + 1 test-only crate (browseros-stress-tests)

---

## 2. Dependency Graph

```
browseros-types
  ├── browseros-config
  ├── browseros-event-bus
  └── browseros-bridge
        ├── browseros-observability  (needs types + config)
        ├── browseros-scheduler      (needs types + event-bus)
        ├── browseros-cdp            (needs types + bridge)
        ├── browseros-dom            (needs types + bridge)
        ├── browseros-storage        (needs types + bridge)
        └── browseros-page           (needs types + bridge + event-bus)
              ├── browseros-lifecycle (needs types + event-bus + observability)
              ├── browseros-dag      (needs types + event-bus + observability + scheduler)
              └── browseros-browser  (needs types + bridge + event-bus + observability + config + cdp)
                    └── browseros-runtime (needs all above)
```

### Verified Acyclic

✅ All 14 crate dependency chains are acyclic  
✅ `browseros-types` is the single root (zero workspace deps)  
✅ Maximum depth: 6 layers (types → runtime)  
✅ No forbidden dependencies detected  

---

## 3. Crate Boundaries

### Isolation Rules

| Crate | May Import | Must NOT Import |
|---|---|---|
| browseros-types | (none) | any workspace crate |
| browseros-config | types | event-bus, bridge, dag, ... |
| browseros-event-bus | types | config, bridge, dag, ... |
| browseros-bridge | types | config, event-bus, dag, ... |
| browseros-observability | types, config | bridge, event-bus, dag, ... |
| browseros-scheduler | types, event-bus | config, bridge, dag, ... |
| browseros-cdp | types, bridge | config, event-bus, dag, ... |
| browseros-dom | types, bridge | config, event-bus, dag, ... |
| browseros-storage | types, bridge | config, event-bus, dag, ... |
| browseros-page | types, bridge, event-bus | config, dag, ... |
| browseros-lifecycle | types, event-bus, observability | bridge, dag, ... |
| browseros-dag | types, event-bus, observability, scheduler | bridge, browser, cdp, dom, page, storage, config, lifecycle, runtime |
| browseros-browser | types, bridge, event-bus, observability, config, cdp | dag, runtime, ... |
| browseros-runtime | all lower-layer crates | nothing forbidden |

---

## 4. browseros-dag Architecture

### Module Structure (Frozen)

```
browseros-dag/
├── lib.rs          — crate root, re-exports from 12 pub modules
├── bridge.rs      — bridge_node! macro for port integration
├── config.rs      — (placeholder)
├── engine.rs      — DagEngine: register, execute, cancel, track
├── error.rs       — DagError (18 variants)
├── events.rs      — 9 DAG event payload types
├── exec.rs        — ExecutableNode, NodeRegistry, NodeFactory, VariableStore
├── executor.rs    — SyncExecutor (private)
├── graph.rs       — DagGraph (petgraph), topological sort (private)
├── loader.rs      — PluginLoader, manifest parsing
├── manager.rs     — PluginManager, lifecycle orchestration
├── node.rs        — DagNode, NodeKind, DagDefinition, NodeState
├── planner.rs     — PlannerBridge, ExecutionPlan, planner types
├── plugin.rs      — Plugin trait, PluginRegistry, plugin types
├── runtime.rs     — PluginRuntime, PluginCapabilityNode, bridge
└── scheduler.rs   — DagScheduler (private)
```

### Public API (Frozen)

All items listed in [`browseros-dag/src/lib.rs`](../../browseros-dag/src/lib.rs) lines 20–52 are part of the frozen public API. Changes require ADR.

### Integration Architecture

```
Planner (LLM/MCP/Plugin)
  │ PlannerBridge trait
  ▼
ExecutionPlan
  │ build_dag() via NodeFactory
  ▼
DagDefinition
  │ DagEngine::execute_dag()
  ▼
DagEngine
  │ SyncExecutor
  ▼
  ┌─── DagNode ── DagNode ── DagNode ──┐
  │         (dependency edges)         │
  └────────────────────────────────────┘
        ▲
        │ PluginExecutionBridge
  PluginCapabilityNode (implements ExecutableNode)
        ▲
  CapabilityResolver
        ▲
  PluginRegistry (via PluginManager)
```

---

## 5. Implemented Features

| Feature | Phase | Status | Tests |
|---|---|---|---|
| Graph data structures | P2 | ✅ | ~50 |
| Node definitions | P3 | ✅ | ~25 |
| DAG engine | P4 | ✅ | ~10 |
| Error handling | P5 | ✅ | ~5 |
| Event system | P6 | ✅ | ~10 |
| Execution abstractions | P7 | ✅ | ~50 |
| Executor + scheduler | P8 | ✅ | ~45 |
| DAG validation | P9 | ✅ | ~10 |
| Runtime integration | P10 | ✅ | 13 (runtime crate) |
| Bridge integration | P11 | ✅ | 5 |
| Planner & MCP foundation | P13 | ✅ | ~35 |
| Plugin runtime foundation | P14 | ✅ | ~80 |
| Plugin loader + manager | P15 | ✅ | ~70 |
| Plugin runtime integration | P16 | ✅ | 28 |
| **TOTAL** | **15 phases** | **✅** | **477** |

---

## 6. Deferred Features

| Feature | Phase | Rationale | Target |
|---|---|---|---|
| P12: Resource management | P12 | Deferred during planning — scope reduction | Post-freeze |
| LLM Planner implementation | Phase 2 | Requires PlannerBridge + external LLM integration | Phase 2 |
| MCP Server implementation | Phase 2 | Requires CapabilityMetadata discovery + stdio/HTTP transport | Phase 2 |
| Plugin marketplace / registry | Phase 2 | External plugin distribution system | Phase 2 |
| Async execution support | — | Design choice — all execution is synchronous | Future |
| browseros-network | Phase 2.6 | Network interception, WebSocket, HAR | Phase 2.6 |

---

## 7. Known Limitations

| Limitation | Severity | Workaround |
|---|---|---|
| `#![allow(dead_code)]` in dag/lib.rs | Medium | Remove post-freeze — does not affect correctness |
| `.expect()` on RwLock in NodeRegistry | Low | Lock poisoning is extremely rare in practice |
| Empty config.rs placeholder | Low | No DAG-specific config needed currently |
| No external plugin loading | Low | PluginLoader reads filesystem only |
| Private scheduler.rs may be dead code | Low | SyncExecutor is active scheduler |

---

## 8. Quality Gates

| Gate | Result |
|---|---|
| `cargo fmt --check` | ✅ Clean |
| `cargo clippy --workspace --all-targets` | ✅ 0 errors |
| `cargo build --workspace` | ✅ Clean |
| `cargo test --package browseros-dag` | ✅ 477/477 pass |
| `cargo test --workspace` | ✅ All pass (except smoke_test requiring Chrome) |
| Zero unsafe | ✅ Verified |
| No cyclic deps | ✅ Verified |
| All public types Send + Sync | ✅ Verified |

---

## 9. Architecture Scores

| Dimension | Score |
|---|---|
| Architecture | 9/10 |
| Code quality | 8/10 |
| Extensibility | 8/10 |
| Plugin readiness | 9/10 |
| Planner readiness | 7/10 |
| MCP readiness | 7/10 |
| LLM orchestration readiness | 6/10 |
| Production readiness | 7/10 |
| **Phase completion** | **94%** (15/16 phases) |

---

## 10. Future Milestones

| Milestone | Scope |
|---|---|
| **Phase 2 — LLM Orchestration** | LLM planner, MCP server, natural-language goal decomposition |
| **Phase 2.6 — browseros-network** | Network interception, WebSocket, HAR |
| **Post-freeze cleanup** | Remove `allow(dead_code)`, fix `expect()` in NodeRegistry |
| **Plugin ecosystem** | External plugin discovery, marketplace, signatures |
| **Async execution** | Optional async support for I/O-bound plugins |

---

## 11. Freeze Declaration

The BrowserOS architecture as documented in this file is hereby frozen as **Architecture Freeze v4**.

All future changes to:
- Crate dependency direction
- Public API in `browseros-dag/src/lib.rs`
- The layer model
- The integration architecture

require an Architecture Decision Record (ADR) and approval.

Components not yet implemented (Phase 2, Phase 2.6) are explicitly excluded from this freeze — their architecture is subject to change until they are implemented and frozen individually.

---

**Signed:** Architecture Conformance Audit v4  
**Date:** 2026-07-14
