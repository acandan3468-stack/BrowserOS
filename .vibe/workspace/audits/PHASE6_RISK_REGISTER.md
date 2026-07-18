# BrowserOS — Phase 6 Risk Register

> **Audit Date:** 2026-07-18  
> **Scope:** All 17 workspace crates, projected Phase 6–8 changes  
> **Rating Scale:** Severity (Critical/High/Medium/Low), Likelihood (Certain/Likely/Possible/Unlikely)  

---

## Architecture Risks

### ARC-001: RuntimeContext God Object

| Field | Value |
|-------|-------|
| **Risk** | `RuntimeContext` grows to 14–16 fields by Phase 8, creating an unwieldy god object that requires full construction for testing and violates interface segregation. |
| **Severity** | Medium |
| **Likelihood** | Likely (projected additions are certain) |
| **Impact** | Increased test complexity, monolithic dependency, slower compile times |
| **Mitigation** | Adopt provider pattern (CoreProvider + ServiceProvider + ExtensionRegistry) before Phase 8 |
| **Owner** | Runtime team |
| **Target Phase** | Phase 8 design phase |

### ARC-002: No SessionManager

| Field | Value |
|-------|-------|
| **Risk** | Planner and Workflow cannot bind browser/DOM/network/LLM resources to a user workflow session. Each subsystem passes handles independently, creating inconsistent ownership patterns. |
| **Severity** | High |
| **Likelihood** | Certain (Phase 6.2 cannot start without session concept) |
| **Impact** | Inconsistent resource binding, resource leaks (orphaned browsers), complex cancellation logic |
| **Mitigation** | Design SessionManager API in Phase 6.1 design phase. Minimum: create/get/list/destroy with per-session resource handles. |
| **Owner** | Architecture team |
| **Target Phase** | Phase 6.1 (design) → Phase 6.2 (implementation) |

### ARC-003: No BrowserPool

| Field | Value |
|-------|-------|
| **Risk** | Planner launches browsers via `BrowserManager::launch()` which incurs 2–5s process startup per call. No pooling means no reuse, no pre-warming, no rate-limited allocation. |
| **Severity** | High |
| **Likelihood** | Likely (Planner will allocate multiple browsers for parallel tasks) |
| **Impact** | High-latency task execution, uncontrolled resource consumption, browser process churn |
| **Mitigation** | Wrap `BrowserManager` with `BrowserPool` struct. API: acquire(timeout), release(id), health(), resize(min, max). Can be implemented in `browseros-browser` or a new `browseros-browser-pool` crate. |
| **Owner** | Browser team |
| **Target Phase** | Phase 6.2 |

### ARC-004: No ThreadPool

| Field | Value |
|-------|-------|
| **Risk** | Each subsystem spawns its own threads (Scheduler: 1, DAG: configurable, LLM streaming: per-stream). Without a centralized thread pool, thread count is unbounded. Risk of thread starvation at high concurrency. |
| **Severity** | Medium |
| **Likelihood** | Possible (Phase 7+ adds more thread-spawning subsystems) |
| **Impact** | Resource exhaustion, thread starvation, difficult debugging |
| **Mitigation** | Add `ThreadPool` field to `RuntimeContext` before Phase 6.2. Migrate subsystems to use it. |
| **Owner** | Runtime team |
| **Target Phase** | Phase 6.2 |

---

## Implementation Risks

### IMP-001: Unjoined Background Threads

| Field | Value |
|-------|-------|
| **Risk** | Two production threads (CDP reader in browseros-browser, WebSocket reader in browseros-cdp) have no explicit `join()`. They terminate via RAII (handle drop closes I/O → read loop returns EOF). While functionally safe, there is no tracing span or lifecycle event to confirm termination, and resource cleanup is invisible. |
| **Severity** | Low |
| **Likelihood** | Unlikely (pattern has been tested and works) |
| **Impact** | Difficult debugging if threads do not terminate cleanly; no telemetry |
| **Mitigation** | Add a tracing event in the thread closure exit path. Optionally add a `JoinHandle` to the owning struct for explicit join. |
| **Owner** | Browser team |
| **Target Phase** | Phase 6.2 (low priority) |

### IMP-002: Fragmented Shutdown

| Field | Value |
|-------|-------|
| **Risk** | No global shutdown coordinator. Each component drops independently via Arc refcounting. Drop order is not guaranteed. In-flight CDP commands, pending LLM requests, and active DAG tasks may be terminated without cleanup. |
| **Severity** | High |
| **Likelihood** | Likely (no coordinator exists; Arc refcounting is fragile) |
| **Impact** | Resource leaks, orphaned child processes, data corruption in storage |
| **Mitigation** | Implement `ShutdownCoordinator` in `browseros-lifecycle` with: (1) shutdown barrier, (2) per-component timeout, (3) ordered shutdown sequence, (4) telemetry. API: `coordinator.shutdown(timeout)` → `Result<Vec<ComponentStatus>>`. |
| **Owner** | Runtime team |
| **Target Phase** | Phase 6.2 |

### IMP-003: No CDP Command Timeout

| Field | Value |
|-------|-------|
| **Risk** | `CdpSession::send_command()` uses `recv_timeout()` with no timeout (infinity). If the browser stops responding, the caller blocks forever, holding any lock acquired before the call. |
| **Severity** | High |
| **Likelihood** | Possible (browser crash, network partition, CDP deadlock) |
| **Impact** | Complete process hang if lock is held; resource leak (blocked threads accumulate) |
| **Mitigation** | Add configurable default timeout to `CdpSession` (default: 30s). Timeout returns `CdpError::Timeout`. Overridable per-command. |
| **Owner** | CDP team |
| **Target Phase** | Phase 6.2 |

### IMP-004: MCP Server Blocking Read

| Field | Value |
|-------|-------|
| **Risk** | MCP server's `read_line()` in the request loop blocks indefinitely. A slow or malicious client can hold the connection open without sending data, blocking all other clients. |
| **Severity** | Medium |
| **Likelihood** | Possible (network issues, DoS attack, client bug) |
| **Impact** | Denial of service (one client blocks the entire MCP server) |
| **Mitigation** | Add per-connection read timeout. Consider concurrent connection limit. |
| **Owner** | MCP team |
| **Target Phase** | Phase 6.2 |

---

## Protocol Risks

### PRO-001: Remote Browser Security

| Field | Value |
|-------|-------|
| **Risk** | Remote browser support (Phase 2.6+) requires CDP over WebSocket. CDP commands can access arbitrary file system paths, execute JavaScript in the browser context, and exfiltrate data. No TLS, no authentication, no sandboxing. |
| **Severity** | Critical |
| **Likelihood** | Possible (if remote browser feature is enabled without security) |
| **Impact** | Full browser compromise, data exfiltration, host system access (via file:// protocol) |
| **Mitigation** | Mandatory TLS for remote CDP connections. Per-connection authentication token. CDP command allowlist/blocklist. Process sandboxing (see Phase 1 freeze doc §7.2). |
| **Owner** | Security team (to be formed) |
| **Target Phase** | Phase 6.1 (design) → Phase 2.6+ (implementation) |

### PRO-002: No Plugin Sandboxing

| Field | Value |
|-------|-------|
| **Risk** | Phase 7 Plugin system will load user-provided code (via WASM or dynamic library). Without sandboxing, plugins have full access to the runtime: config, browser handles, CDP, storage, network, LLM API keys. |
| **Severity** | Critical |
| **Likelihood** | Possible (plugin system will allow third-party code execution) |
| **Impact** | Arbitrary code execution in the runtime process; full system compromise |
| **Mitigation** | Design plugin sandbox from the start (Phase 7 design phase). WASM with limited host functions. Capability-based access (plugin declares required capabilities, runtime enforces). Separate process for untrusted plugins. |
| **Owner** | Architecture team |
| **Target Phase** | Phase 7 design phase |

---

## Concurrency Risks

### CON-001: EventBus Handler Blocking

| Field | Value |
|-------|-------|
| **Risk** | EventBus handlers run synchronously in the publisher's thread. A slow or blocking handler delays all subsequent handlers and the publisher. This creates cascading latency in the scheduler (which publishes events in the background thread) and DAG (which publishes events in executor threads). |
| **Severity** | Medium |
| **Likelihood** | Likely (handlers may do I/O: CDP commands, LLM calls, file storage) |
| **Impact** | Scheduler latency spikes, DAG execution delays, unpredictable timing |
| **Mitigation** | Add async delivery mode to EventBus (work-stealing thread pool). Add BackpressureMode enum (Blocking, DropOldest, DropNewest). Default to synchronous for backward compatibility. |
| **Owner** | EventBus team |
| **Target Phase** | Phase 7 design phase (before Workflow engine) |

### CON-002: BrowserManager Lock Contention

| Field | Value |
|-------|-------|
| **Risk** | BrowserManager uses `RwLock<Vec<Arc<Mutex<BrowserInstance>>>>`. Iterating the Vec requires a read lock, which blocks concurrent `launch()`/`close_browser()` (write lock). With many browser instances, iteration for health checks or broadcast events blocks allocation. |
| **Severity** | Low |
| **Likelihood** | Unlikely (browser count is expected to be <50) |
| **Impact** | Brief latency spikes during health check iteration |
| **Mitigation** | If browser count exceeds 100, migrate to `DashMap` or sharded Vec. Not needed for Phase 6. |
| **Owner** | Browser team |
| **Target Phase** | Phase 8 (if scaling demands it) |

### CON-003: MCP Server Single-Threaded

| Field | Value |
|-------|-------|
| **Risk** | MCP server processes all requests sequentially in a single thread (read-dispatch-write loop). A long-running tool (LLM chat, CDP command, storage query) blocks all other clients. |
| **Severity** | Medium |
| **Likelihood** | Likely (LLM chat takes 1–30s, CDP commands take 0.1–5s) |
| **Impact** | Low throughput, head-of-line blocking, poor concurrent experience |
| **Mitigation** | Add concurrent request handling in MCP server: spawn tool execution to worker threads, return results asynchronously. Keep JSON-RPC response ordering for correctness. |
| **Owner** | MCP team |
| **Target Phase** | Phase 6.2 (design) → Phase 7 (implementation) |

---

## Future Compatibility Risks

### FUT-001: Plugin API Stability

| Field | Value |
|-------|-------|
| **Risk** | Plugin system API (Phase 7) must remain stable across BrowserOS versions. If not designed with versioning from the start, plugin developers will face breaking changes with every release. |
| **Severity** | Medium |
| **Likelihood** | Likely (without explicit versioning, API drift is inevitable) |
| **Impact** | Plugin ecosystem fragmentation, developer frustration |
| **Mitigation** | Define Plugin API as a versioned trait from Phase 7 design phase. Use semver for plugin compatibility. Provide migration guide for each version bump. |
| **Owner** | Architecture team |
| **Target Phase** | Phase 7 design phase |

### FUT-002: Distributed Runtime Protocol Design

| Field | Value |
|-------|-------|
| **Risk** | Phase 8 distributed runtime requires a wire protocol for cross-process communication. Without early design consideration, a naive protocol choice may limit scalability, observability, or security. |
| **Severity** | Medium |
| **Likelihood** | Possible (Phase 8 is far out; early design avoids rework) |
| **Impact** | Significant rework of distributed runtime in Phase 8 |
| **Mitigation** | Document protocol requirements in Phase 6 architecture docs: serialization format (protobuf/capnp/flatbuffers), transport (gRPC/NATS/ZeroMQ), authentication, observability hooks. |
| **Owner** | Architecture team |
| **Target Phase** | Phase 6 design phase (requirements doc) → Phase 8 (implementation) |

### FUT-003: Storage Schema Migration

| Field | Value |
|-------|-------|
| **Risk** | `browseros-storage` uses sled (embedded database). Sled has no built-in schema migration. As data structures evolve across phases, existing databases become incompatible. |
| **Severity** | Medium |
| **Likelihood** | Likely (storage schema will change between phases) |
| **Impact** | Data loss, manual migration scripts, version-locked databases |
| **Mitigation** | Add version field to storage metadata. Implement migration runner in `browseros-storage` that detects old versions and upgrades in-place. Provide `sled` export/import for major version bumps. |
| **Owner** | Storage team |
| **Target Phase** | Phase 6.2 |

---

## Summary by Severity

| Severity | Count | Key Risks |
|----------|-------|-----------|
| Critical | 2 | PRO-001 (Remote Browser Security), PRO-002 (Plugin Sandboxing) |
| High | 4 | ARC-002 (No SessionManager), ARC-003 (No BrowserPool), IMP-002 (Fragmented Shutdown), IMP-003 (No CDP Timeout) |
| Medium | 7 | ARC-001 (God Object), ARC-004 (No ThreadPool), IMP-004 (MCP Blocking Read), CON-001 (EventBus Handler Blocking), CON-003 (MCP Single-Threaded), FUT-001 (Plugin API Stability), FUT-002 (Distributed Protocol), FUT-003 (Storage Migration) |
| Low | 2 | IMP-001 (Unjoined Threads), CON-002 (BrowserManager Lock Contention) |

---

## Immediate Action Items (Phase 6.1/6.2)

| # | Action | Risk Addressed | Effort | Priority |
|---|--------|---------------|--------|----------|
| 1 | Design SessionManager API | ARC-002 | 2 days | BLOCKING |
| 2 | Implement BrowserPool wrapper | ARC-003 | 3 days | BLOCKING |
| 3 | Add ThreadPool to RuntimeContext | ARC-004 | 1 day | HIGH |
| 4 | Add CDP command default timeout | IMP-003 | 1 day | HIGH |
| 5 | Implement ShutdownCoordinator | IMP-002 | 3 days | HIGH |
| 6 | Wire metrics + tracing into production code | (General readiness) | 2 days | HIGH |
| 7 | Add MCP server read timeout | IMP-004 | 0.5 day | MEDIUM |
| 8 | Design storage migration strategy | FUT-003 | 1 day | MEDIUM |
| 9 | Document distributed runtime protocol requirements | FUT-002 | 1 day | MEDIUM |
