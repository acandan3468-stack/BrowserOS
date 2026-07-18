# BrowserOS — Workspace Architecture Audit

> **Audit Date:** 2026-07-18  
> **Scope:** All 17 workspace crates (browseros-types, config, observability, event-bus, lifecycle, scheduler, runtime, bridge, browser, page, cdp, dom, storage, dag, llm, stress-tests, mcp)  
> **Target:** Assess whether the current architecture can scale through Phases 6 (Planner/Network), 7 (Workflow), and 8 (Distributed Runtime)  

---

## 1. Dependency Graph

**Status: HEALTHY — Phase 4 shape retained; MCP is a terminal consumer.**

- All 17 crates form a stratified DAG with 4 layers: Foundation (types, config, observability) → Infrastructure (event-bus, lifecycle, scheduler, runtime, bridge) → Domain (browser, page, cdp, dom, storage, dag, llm) → Shell (mcp).  
- No cycles. No forbidden edges. No unused edge weights (all `path =` references are exercised in `use` statements).  
- `browseros-mcp` depends on 9 crates (heaviest consumer) — acceptable because MCP is the application shell.  
- `browseros-runtime` depends on 6 infrastructure crates — clean composition root.  
- Risk: High downstream dependency weight on `browseros-types` (10 transitive dependents). Mitigating factor: types are light (serde + uuid + chrono + thiserror, no I/O).  

**Phase 6+ impact:** Adding Planner, Network, Remote Browser, Plugin, Workflow, Cloud/Distributed crates will increase total to ~25 crates. The DAG structure holds if new crates are added at the Domain layer (consuming types, event-bus, config) or the Shell layer (consuming everything). No structural refactoring needed.

---

## 2. RuntimeContext Ownership Model

**Status: AT RISK — God Object pattern emerging; 8 fields today, projected 14+ by Phase 8.**

- `RuntimeContext` in `browseros-runtime` currently holds 8 `Arc` fields: bus, logger, metrics, tracer, config, lifecycle, scheduler, dag.  
- All fields are shared (Arc) with no ability to downsize, swap, or reconfigure after construction.  
- No storage_manager, browser_pool, ll_gateway, network_handle, plugin_registry, session_manager fields yet — these are blocked on Phase 2.6+ or later.  
- `RuntimeBuilder` is additive only — no `without_*` or conditional construction.  

**Phase 6+ impact:** Adding fields for StorageManager, BrowserPool, LlGateway, NetworkHandle, PluginRegistry, SessionManager, WorkflowEngine, DistRuntime would push `RuntimeContext` to 14–16 fields. This is functionally OK but architecturally heavy. A provider registry pattern (trait-object map in an Arena) would decouple construction from consumption.

---

## 3. Thread Model

**Status: MANAGED RISK — 3 named thread pools, 2 unjoined I/O threads, no signal handling.**

### Production thread pools (controlled):
| Pool | Location | Size | Join Mechanism |
|------|----------|------|----------------|
| Scheduler background | `browseros-scheduler` | 1 | mpsc channel drop in Drop impl |
| DAG executor workers | `browseros-dag` | Configurable (default: num_cpus) | Scoped thread scope — guaranteed join at scope exit |
| LLM streaming | `browseros-llm` gateway | Per-stream (1 per stream) | Stream completion (inside spawned closure) |

### Unjoined threads (risk):
| Thread | Location | Lifespan | Risk |
|--------|----------|----------|------|
| CDP reader | `browseros-browser process.rs:81` | Browser process lifetime | No explicit join — thread terminates naturally when process drops or stderr closes. Resource cleanup via Arc refcounting. |
| WebSocket reader | `browseros-cdp transport.rs:42` | Connection lifetime | Same pattern — terminates on connection drop. No explicit join. |

Both unjoined threads terminate reliably through RAII: when the owning handle drops, the underlying I/O handle (ChildStdout / WebSocket) is closed, causing the reader thread's read loop to return EOF/error. This is safe but invisible — no tracing span or lifecycle event marks the termination.

**Phase 6+ impact:** Planner, Workflow, Distributed Runtime will add thread pools. Without a centralized thread pool executor, each subsystem spins its own threads. Risk of thread starvation at high concurrency.

**Recommendation:** Introduce a `ThreadPool` field in `RuntimeContext` before Phase 6.2.

---

## 4. Session Architecture

**Status: NOT STARTED — no SessionManager exists; deferred to Phase 6.2.**

- `CdpSession` exists in `browseros-cdp` for CDP-level session management (used for OOPIF, service workers).  
- `SessionHandle` exists in `browseros-browser` but is a thin Deref wrapper around `Arc<Mutex<CdpSession>>`.  
- No unified session concept ties together browser, DOM, network, LLM, MCP, storage for a single user workflow.  
- No session lifecycle (create, attach, detach, destroy), no session-level isolation, no session configuration.  

**Phase 6+ impact:** Planner and Workflow both require a multi-resource session. Without SessionManager, developers must manually pass individual handles through execution contexts. This creates an inconsistent ownership pattern across subsystems.

**Recommendation:** Design SessionManager in Phase 6.1 design phase. Minimum API: `create(config)`, `get(id)`, `list(filter)`, `destroy(id)`. Each session should reference BrowserInstance, NetworkContext, and LlGateway.

---

## 5. BrowserPool Readiness

**Status: FOUNDATIONAL — BrowserManager exists but is not a pool.**

- `BrowserManager` in `browseros-browser/src/manager.rs` provides `launch()` (allocates a new browser) and `close_browser()` (deallocates).  
- Storage is `RwLock<Vec<Arc<Mutex<BrowserInstance>>>>` — two-level locking permits concurrent operations on different browsers but serializes iteration (e.g., health checks).  
- No pool sizing, no reuse after crash, no MinIdle/MaxIdle, no pre-warming, no config-driven auto-scaling, no parallel launch.  
- `BrowserInstance` wraps `BrowserProcess` + `BrowserHandle` — clean design, easy to extend.  

**Phase 6+ impact:** Planner will allocate browsers for parallel task execution. Without pooling, each `launch()` call incurs full process startup cost (~2–5s). Rate-limited autoscaling and connection-level browser reuse are essential for acceptable latency.

**Recommendation:** Wrap `BrowserManager` with a `BrowserPool` struct (in its own crate or in `browseros-browser`) before Phase 6.2. Pool API: `acquire(timeout)`, `release(InstanceId)`, `health()`, `resize(MinIdle, MaxIdle)`.

---

## 6. Event Architecture

**Status: SUFFICIENT FOR PHASE 6 — simple publish/subscribe, synchronous handlers, no backpressure.**

- `InMemoryEventBus` in `browseros-event-bus` implements `subscribe(handler, filter)`, `publish(event)`, `unsubscribe(id)`.  
- Handlers are called synchronously in the publisher's thread — no backpressure, no async event loop, no dead letter queue.  
- Filtering is by `EventType` (a string key) — no content-based filtering, no priority, no routing.  
- `MessageEnvelope` wraps each event with `timestamp`, `correlation_id`, `source_id`, `sequence_number`.  

**Phase 6+ impact:** Workflow engine (Phase 7) will produce high-frequency events (task start/complete/error, graph transitions, retries). Synchronous handlers block the publisher and risk cascading latency. Async event pipeline with buffering and backpressure is required before Phase 7.

**Recommendation:** Extend EventBus with optional async delivery (work-stealing thread pool) and `BackpressureMode` (Blocking, DropOldest, DropNewest) before Phase 7. Keep synchronous mode as default for simplicity.

---

## 7. API Surface Stability

**Status: STABLE — 4 frozen crates, remaining crates in active development.**

| Crate | Stability | Public API Change Frequency |
|-------|-----------|----------------------------|
| browseros-types | FROZEN (Phase 1) | Zero changes planned |
| browseros-config | STABLE | Minor additions only |
| browseros-observability | STABLE | Minor additions only |
| browseros-event-bus | STABLE | Minor additions only |
| browseros-lifecycle | PLACEHOLDER | Gutted after signal removal; full rewrite in Phase 6.2 |
| browseros-scheduler | STABLE | Minor additions only |
| browseros-runtime | STABLE | New fields added per phase |
| browseros-bridge | STABLE (Phase 1) | Minor additions only |
| browseros-browser | STABLE | Minor additions expected for pool support |
| browseros-page | STABLE | Minor additions only |
| browseros-cdp | STABLE | Minor additions only |
| browseros-dom | FROZEN (Phase 2.5) | Zero changes planned |
| browseros-storage | ACTIVE | Being designed/implemented in current phase |
| browseros-dag | STABLE | Minor additions only |
| browseros-llm | STABLE | Minor additions expected |
| browseros-mcp | NEW (Phase 6.1) | May change as usage patterns emerge |

**Phase 6+ impact:** Stable crates provide a solid foundation for new code. The primary risk is changing the stable API of `RuntimeContext` (adding fields is additive and safe). `LifecycleManager` will need a breaking rewrite in Phase 6.2.

---

## 8. Extension Points

**Status: INCOMPLETE — Plugin system not started; DAG provides one extension axis.**

| Extension Point | Status | Phase Target |
|-----------------|--------|--------------|
| DAG nodes (custom task types) | Implemented — `DagTask` trait in browseros-dag | Current |
| Plugin system (PluginRegistry, PluginRuntime) | Defined in browseros-dag but stubbed | Phase 7 |
| Capability registry (feature negotiation) | Not started | Phase 7 |
| MCP tools (server-side) | Implemented — `ToolProvider` trait in browseros-mcp | Current |
| MCP adapters (client-side) | Implemented — `McpAdapter` in browseros-llm | Current |
| Custom LLM adapters | Implemented — 5 built-in (OpenAI, Anthropic, Google, Ollama, OpenRouter) | Current |
| Custom storage backends | Being designed | Phase 2.6/6 |
| Distributed executor | Not started | Phase 8 |

**Phase 6+ impact:** Phase 7 (Workflow) requires the Plugin system to allow user-defined workflow steps. The current `DagTask` trait provides task-level extensibility but not workflow-level extensibility. The Plugin system must be designed before Phase 7 implementation begins.

---

## 9. Production Readiness — Summary

| Dimension | Rating | Key Gap |
|-----------|--------|---------|
| Configuration | ✅ GOOD | File + Env + Default layers complete |
| Logging | ✅ GOOD | Structured JSON, stdout/stderr split |
| Metrics | ⚠️ PARTIAL | Counter/Gauge/Histogram exist but unused by other crates |
| Tracing | ⚠️ PARTIAL | Tracer exists but unused by other crates |
| Health checking | ⚠️ PARTIAL | MCP server only; no component aggregation |
| Shutdown | ❌ FRAGMENTED | LifecycleManager.shutdown_all() exists but no global shutdown coordinator |
| Crash recovery | ❌ NONE | Panics abort process; no supervisor |
| Resource cleanup | ⚠️ PARTIAL | RAII-based but no guarantee of clean teardown |
| Security | ❌ NONE | No secret management, no auth, no message size limits |
| Timeouts | ⚠️ PARTIAL | LLM, CDP, Browser have timeouts; not universal |
| Rate limiting | ❌ NONE | No rate limit anywhere |
| OOM protection | ❌ NONE | No memory limits |
| Backpressure | ❌ NONE | No backpressure in event bus or APIs |

Detailed review in `PRODUCTION_READINESS_REVIEW.md`.

---

## 10. Verdict by Architecture Dimension

| Dimension | Verdict | Required for Phase 6+ |
|-----------|---------|----------------------|
| Dependency Graph | ✅ APPROVED | No changes needed |
| RuntimeContext | ⚠️ ACCEPTABLE | Provider pattern recommended before Phase 8 |
| Thread Model | ⚠️ ACCEPTABLE | ThreadPool recommended before Phase 6.2 |
| Session Architecture | ❌ NOT READY | SessionManager required before Phase 6.2 |
| BrowserPool | ❌ NOT READY | BrowserPool wrapper required before Phase 6.2 |
| Event Architecture | ✅ APPROVED | Async delivery recommended before Phase 7 |
| API Surface | ✅ APPROVED | LifecycleManager rewrite acceptable in Phase 6.2 |
| Extension Points | ❌ NOT READY | Plugin system required before Phase 7 |
| Production Readiness | ⚠️ ACCEPTABLE | Phase 8 will not be production-grade without addressing shutdown, crash recovery, security, backpressure |

---

## Final Verdict

**APPROVED WITH REQUIRED CHANGES**

The workspace architecture is fundamentally sound and can scale through Phases 6, 7, and 8 with targeted additions. However, three blocking items must be resolved before Phase 6.2 implementation begins:

1. **SessionManager design** — required for Planner to bind resources to a workflow session  
2. **BrowserPool wrapper** — required for Planner to efficiently allocate browser instances  
3. **ThreadPool in RuntimeContext** — required to avoid uncontrolled thread proliferation across subsystems  

Four strong recommendations for Phase 7 readiness:

4. **Plugin system design** (start before Phase 7, implement during Phase 7)  
5. **Async event pipeline** (backpressure + work-stealing delivery)  
6. **Provider registry pattern** for RuntimeContext (prepare for Phase 8)  
7. **Global shutdown coordinator** (prepare for production deployment)
