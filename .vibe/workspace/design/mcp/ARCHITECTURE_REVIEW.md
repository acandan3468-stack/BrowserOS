# browseros-mcp Architecture Review

**Document:** ARCHITECTURE_REVIEW.md  
**Phase:** Design Freeze  
**Date:** 2026-07-16  

---

## 1. Architecture Consistency Review

Cross-check of all 8 design documents against the 4 baseline documents.

### 1.1 Baseline Documents

| ID | Document | Status |
|----|----------|--------|
| B1 | MCP_ARCHITECTURE_REVIEW.md | Architectual baseline |
| B2 | MCP_TOOL_ROADMAP.md | Tool categorization and phase mapping |
| B3 | MCP_API_SURFACE.md | API schemas and error codes |
| B4 | PHASE6_PREPARATION.md | Migration plan and crate structure |

### 1.2 New Documents

| ID | Document | Cross-Reference |
|----|----------|-----------------|
| D1 | ARCHITECTURE.md | B1, B4 |
| D2 | TOOL_REGISTRY_DESIGN.md | B2, B3 |
| D3 | SESSION_MODEL.md | B1, B4 |
| D4 | NOTIFICATION_MODEL.md | B3 |
| D5 | ERROR_MODEL.md | B3 |
| D6 | IMPLEMENTATION_PLAN.md | B1, B2, B4 |

### 1.3 Consistency Findings

#### Finding 1: Tool Names — Aliases Resolve Inconsistency (✅ RESOLVED)

**Issue:** `MCP_API_SURFACE.md` (B3) uses `dom/query` while `TOOL_REGISTRY_DESIGN.md` (D2) registers `dom/query_selector` and defines `dom/query` as an alias.

**Resolution:** The alias mechanism in D2 explicitly handles this. `dom/query` is a documented alias of `dom/query_selector`. Both names resolve to the same tool.

**Consistency:** ✅

#### Finding 2: Error Code -32000 Duplication (✅ RESOLVED)

**Issue:** Both `MCP_API_SURFACE.md` (B3) and `ERROR_MODEL.md` (D5) define `-32000` as "Application Error". B3 treats it as a generic fallback, D5 preserves this but adds more specific codes (-32001 through -32022).

**Resolution:** D5 explicitly states "-32000 is the generic fallback when no specific error code applies". This is consistent with B3.

**Consistency:** ✅

#### Finding 3: Phase 6 Scope Alignment (✅ CONSISTENT)

**Issue:** `MCP_TOOL_ROADMAP.md` (B2) defines Phase 6 as 8 tools (session/* + llm/models + system/config + system/metrics + system/logs). `IMPLEMENTATION_PLAN.md` (D6) breaks this into 5 sub-phases (6.1–6.5).

**Verification:** All tools from B2 are covered in D6:
- `session/create` → D6.2 ✅
- `session/close` → D6.2 ✅
- `session/list` → D6.2 ✅
- `session/config` → D6.2 ✅
- `llm/models` → D6.3 ✅
- `system/config` → D6.5 ✅
- `system/metrics` → D6.5 ✅
- `system/logs` → D6.1 (via system/health + version) ✅

**Consistency:** ✅

#### Finding 4: Chat Tool Exclusion (✅ CONSISTENT)

**Issue:** `MCP_ARCHITECTURE_REVIEW.md` (B1) explicitly states chat tool "production MCP server'dan KALDIRILMALIDIR". Neither `TOOL_REGISTRY_DESIGN.md` (D2) nor `ARCHITECTURE.md` (D1) includes a chat tool.

**Verification:** Chat tool is absent from all new documents. It remains only in `browseros-llm` as a diagnostic tool.

**Consistency:** ✅

#### Finding 5: Notification Type Coverage (✅ CONSISTENT)

**Issue:** `MCP_API_SURFACE.md` (B3) lists 9 notification types. `NOTIFICATION_MODEL.md` (D4) covers all 9 with payloads and delivery guarantees.

**Verification:**
- `notifications/progress` → D4 Section 2.1 ✅
- `notifications/browser/*` → D4 Section 2.2, 2.3 ✅
- `notifications/dom/mutation` → D4 Section 2.5 ✅
- `notifications/network/*` → D4 Section 2.6 ✅
- `notifications/workflow/*` → D4 Section 2.7 ✅
- `notifications/session/*` → D4 Section 2.8 ✅

**Consistency:** ✅

#### Finding 6: Error Code Alignment (✅ CONSISTENT)

**Issue:** `MCP_API_SURFACE.md` (B3) defines 13 error codes. `ERROR_MODEL.md` (D5) defines 30 codes (22 application + 4 protocol + 4 internal).

**Verification:** B3's 13 codes are a subset of D5's 30 codes. All B3 codes appear in D5 with the same meaning and numeric value.

**Consistency:** ✅

#### Finding 7: Session Ownership (✅ CONSISTENT)

**Issue:** `PHASE6_PREPARATION.md` (B4) states "Sessions own browser instances". `SESSION_MODEL.md` (D3) confirms this design.

**Verification:** D3 Section 5.1 explicitly states "BrowserInstance is owned by exactly one SessionState."

**Consistency:** ✅

#### Finding 8: RuntimeContext Role (✅ CONSISTENT)

**Issue:** `PHASE6_PREPARATION.md` (B4) defines RuntimeContext as composition root with `llm` and `browser_pool` fields. `ARCHITECTURE.md` (D1) uses the same model.

**Verification:** D1 Section 6.1 shows `RuntimeContext::llm()` and `RuntimeContext::browser_pool()`.

**Consistency:** ✅

#### Finding 9: Tool Count — Roadmap vs Registry (⚠️ MINOR ISSUE)

**Issue:** `MCP_TOOL_ROADMAP.md` (B2) lists 8 tools for Phase 6. `TOOL_REGISTRY_DESIGN.md` (D2) registers 7 tool modules (6.1 = system/health + system/version, 6.2 = session/*, 6.3 = llm/models, 6.5 = system/config + system/metrics).

**Variance:** `system/logs` from B2 is classified as Phase 7+ in D6 (`IMPLEMENTATION_PLAN.md` doesn't include it in Phase 6). This is a deliberate scope adjustment — logs tool is pushed to Phase 7.

**Resolution:** Accepted as a scope decision. `system/logs` is deferred.

**Consistency:** ⚠️ Minor (documented)

#### Finding 10: BrowserPool API — Not Yet Defined (⚠️ DEPENDENCY GAP)

**Issue:** `SESSION_MODEL.md` (D3) and `ARCHITECTURE.md` (D1) assume `BrowserPool::allocate()` and `BrowserPool::shutdown()` exist. These are NOT yet implemented in `browseros-browser`.

**Risk:** Session creation cannot be implemented until BrowserPool exposes the required API.

**Mitigation:** Phase 6.2 depends on this API. Document as a pre-requisite.

**Consistency:** ⚠️ Dependency (not an inconsistency, but a implementation pre-condition)

### 1.4 Terminology Check

| Term | Used In | Consistent? |
|------|---------|-------------|
| BrowserInstance | D1, D3 | ✅ |
| BrowserPool | D1, D3, B4 | ✅ |
| McpTool | D1, D2 | ✅ |
| ToolRegistry | D1, D2 | ✅ |
| SessionManager | D1, D3 | ✅ |
| SessionState | D1, D3 | ✅ |
| NotificationDispatcher | D1, D4 | ✅ |
| McpServerError | D5 | ✅ |
| ToolOutput | D1, D2 | ✅ |
| ToolError | D1, D2, D5 | ✅ |
| McpToolContext | D1 | ✅ |
| CancelToken | D1, D3 | ✅ |

All terminology is consistent across all documents.

---

## 2. Architecture Audit

### Scoring Rubric

Each dimension scored 1–10. 10 = perfect. 1 = fundamentally broken.

### 2.1 Modularity

**Score: 9 / 10**

**Justification:** The crate is decomposed into 6 top-level modules (server, session, tools, notify, error, types) with clear responsibilities. The tool layer is further split by domain (system, session, browser, dom, network, workflow, llm). No god modules. No circular dependencies between modules.

**Deduction (-1):** The tool/llm module exists only as a placeholder for future Phase 7+ functionality. In Phase 6, its single tool (`llm/models`) could live in system/ without creating a separate module. Acceptable as future-proofing.

### 2.2 Maintainability

**Score: 9 / 10**

**Justification:** All lifecycle documents (startup, shutdown, session, request, notification, cancellation) are explicitly defined in ARCHITECTURE.md with sequence diagrams. Error handling has a single source of truth (ERROR_MODEL.md). A new developer can understand the entire flow without reading code.

**Deduction (-1):** The ToolRegistry's McpTool trait has 4 optional methods + 6 trait methods = 10 methods total. In practice, most implementations override only 3 (name, description, input_schema, execute). The trait could benefit from a derive macro or default implementations grouped into a helper trait. Acceptable for Phase 6.

### 2.3 Runtime Complexity

**Score: 9 / 10**

**Justification:** All ToolRegistry operations are O(1) or O(n) with small n. SessionManager operations are O(1) via HashMap. Request dispatch is O(1) for tool lookup + O(1) for session lookup. Notification dispatch is O(s) where s = subscriber count (expected < 10).

**Deduction (-1):** Tool list with capability filtering is O(n) with a full scan. For n > 500, a capability index would be needed. Acceptable for Phase 6 (expected n < 100).

### 2.4 API Stability

**Score: 8 / 10**

**Justification:** The external MCP API is versioned per tool (semver). Error codes are frozen in ERROR_MODEL.md. Tool names use namespace/name convention. Backwards compatibility through aliases.

**Deduction (-2):** The MCP protocol version is hardcoded as "2024-11-05" in the initialize response. Future protocol versions may require negotiation. Additionally, tool visibility (Public/Hidden/Internal/Diagnostic) and capability filtering are not part of the MCP standard — they are BrowserOS extensions that may not be supported by all clients.

### 2.5 Thread Safety

**Score: 9 / 10**

**Justification:** Explicit synchronization strategy documented in ARCHITECTURE.md Section 5.2. Lock ordering discipline documented. All shared state uses RwLock or Mutex. Tools are Send + Sync. CancelToken uses AtomicBool.

**Deduction (-1):** No deadlock detection at runtime. The lock ordering discipline is enforced by convention, not by the compiler. A future improvement could add runtime lock ordering validation (lockdep-style).

### 2.6 Memory Ownership

**Score: 10 / 10**

**Justification:** Complete ownership model with a resource ownership map (ARCHITECTURE.md Section 3.1). Every resource has exactly one owner. SessionState owns BrowserInstance exclusively. Resources are released when their owner is dropped — no manual cleanup outside of Drop. Arc used for shared immutable data, not for shared mutable state.

### 2.7 Extensibility

**Score: 9 / 10**

**Justification:** McpTool trait is the primary extension point. External crates can implement McpTool and register via McpServerBuilder. Transport is abstracted. Future plugin system can load .so/.dll. Namespace isolation for plugins (ext/ prefix).

**Deduction (-1):** The plugin loading mechanism is described but not designed in detail. Phase 8+ will need a separate PLUGIN_SYSTEM_DESIGN.md. Acceptable for Phase 6.

### 2.8 Performance

**Score: 8 / 10**

**Justification:** Synchronous (blocking) I/O model avoids async runtime overhead. Per-call worker threads provide isolation. Mutex hold times are bounded (RwLock for read-heavy access). Notification backpressure prevents subscriber starvation.

**Deduction (-2):** The per-call thread model does not scale to thousands of concurrent connections. Each concurrent tool call consumes an OS thread. For BrowserOS's target use case (1–10 concurrent sessions, 1–4 tools per session), this is acceptable. A thread pool with a configurable max is specified (default: 16). If BrowserOS becomes a network service (SSE/WebSocket), an async runtime would be needed.

### 2.9 Testability

**Score: 9 / 10**

**Justification:** Tool implementations are decoupled from transport (they receive McpToolContext). Transport can be mocked. SessionManager can be tested without a real browser. Phase 6.6 dedicates 530 LOC to integration tests and benchmarks.

**Deduction (-1):** The binary entry point (`browseros_mcp_server::main()`) is not directly testable. Integration tests must spawn the binary as a subprocess. Acceptable — this is the standard pattern for MCP servers.

### 2.10 MCP Compatibility

**Score: 9 / 10**

**Justification:** JSON-RPC 2.0 compliant. Standard MCP lifecycle (initialize → notifications/initialized → tools/list → tools/call). Standard error codes. Stdio transport. Reuses browseros-llm::mcp::jsonrpc types. Protocol version 2024-11-05.

**Deduction (-1):** BrowserOS extends MCP with non-standard features (notification types beyond progress, capability filtering, tool visibility). These are optional extensions — the server works correctly without them, and standard MCP clients can use the basic tool set without understanding extensions.

### 2.11 Long-term Evolution

**Score: 8 / 10**

**Justification:** The architecture explicitly accounts for phases 6 through 8+. Tool categories are designed for incremental addition. Transport extensibility (WebSocket/SSE in Phase 8). Plugin system planned. Distributed sessions planned.

**Deduction (-2):** The workspace-level dependency graph has a risk of cyclic dependencies as more crates need to reference MCP types. Resolution strategy is documented (re-export from browseros-llm), but this adds coupling between browseros-llm and browseros-mcp that should be monitored.

---

## 3. Overall Score

| Dimension | Score |
|-----------|-------|
| Modularity | 9 |
| Maintainability | 9 |
| Runtime Complexity | 9 |
| API Stability | 8 |
| Thread Safety | 9 |
| Memory Ownership | 10 |
| Extensibility | 9 |
| Performance | 8 |
| Testability | 9 |
| MCP Compatibility | 9 |
| Long-term Evolution | 8 |
| **Average** | **8.8 / 10** |

---

## 4. Remaining Tradeoffs

| Tradeoff | Decision | Rationale |
|----------|----------|-----------|
| OS threads vs async | OS threads (blocking) | Matches rest of BrowserOS. Thread pool limits concurrency. Async can be added later for network transport. |
| Session persistence vs in-memory | In-memory (Phase 6) | BrowserOS is a local runtime. Session state cannot survive process death. Persistence hooks defined for Phase 8. |
| Plugin system vs built-in tools | Built-in (Phase 6), plugin (Phase 8) | All Phase 6 tools are core BrowserOS capabilities. Plugin system would add libloading dependency prematurely. |
| Tool visibility extensions vs standard MCP | Extensions (optional) | Standard MCP clients ignore unknown fields in tools/list. BrowserOS extensions are backward-compatible. |
| JSON Schema validation vs manual | JSON Schema (external crate) | Adds one dependency but eliminates field-level validation bugs. Schema can be reused by MCP clients. |

---

## 5. Risks Accepted

| Risk | Severity | Mitigation |
|------|----------|------------|
| BrowserPool API not yet implemented | High | Documented as dependency for Phase 6.2. Session creation cannot be implemented without it. |
| RuntimeContext lacks `llm` and `browser_pool` fields | High | These must be added to browseros-runtime. If not feasible, McpServer can build them independently. |
| `system/logs` deferred to Phase 7 | Low | Not critical for Phase 6. Log level can be set via environment variable in Phase 6. |
| Thread-per-call doesn't scale to 1000+ connections | Low | BrowserOS target: 1–10 sessions. If needed, switch to async in Phase 8. |

---

## 6. Risks Eliminated

| Risk | Eliminated By |
|------|---------------|
| Chat tool pollutes MCP API | Explicitly excluded from browseros-mcp. Remains in browseros-llm as diagnostic. |
| Stdout corruption (Phase 5B bug) | Observability design uses StderrSink. ARCHITECTURE.md confirms this. |
| Circular dependency browseros-mcp → browseros-llm → browseros-mcp | browseros-mcp depends on browseros-llm, not vice versa. |
| God crate (all capabilities in one module) | Split into 7 domain-specific tool modules + 6 server modules. |
| Notification flooding clients | Throttle policies, batching, and backpressure defined for every notification type. |

---

## 7. Implementation Readiness Assessment

| Criteria | Status |
|----------|--------|
| Architecture defined | ✅ |
| Module tree defined | ✅ |
| All lifecycles defined | ✅ |
| Error model complete | ✅ |
| Notification model complete | ✅ |
| Session model complete | ✅ |
| Tool registry designed | ✅ |
| Extension points identified | ✅ |
| Phase 6 split into sub-phases | ✅ |
| Implementation order defined | ✅ |
| Acceptance criteria per phase | ✅ |
| Risk analysis per phase | ✅ |
| Test strategy per phase | ✅ |
| LOC estimates per phase | ✅ |
| External dependencies identified | 🔴 BrowserPool API |
| Cross-document consistency | ⚠️ 2 minor findings resolved |

**The single blocking dependency is `BrowserPool` API availability in `browseros-browser`.**
Everything else is ready for implementation.

---

## 8. Verdict

```
DESIGN FREEZE APPROVED
```

**Conditions:**
1. `browseros-browser::BrowserPool` must expose `allocate(config) -> BrowserInstance` before Phase 6.2 implementation begins.
2. `browseros-runtime::RuntimeContext` must expose `llm()` and `browser_pool()` accessors before Phase 6.1 implementation begins.
3. `system/logs` tool is deferred to Phase 7 (documented scope change).

**Remaining work for implementation team:**
- Phase 6.1: Crate skeleton, transport, dispatcher, system tools, ToolRegistry
- Phase 6.2: SessionManager, BrowserPool integration
- Phase 6.3: LLM integration
- Phase 6.4: Notification infrastructure
- Phase 6.5: Config and metrics tools
- Phase 6.6: Integration tests, performance baseline
- Phase 6.7: Documentation

All architectural decisions are documented. No major ambiguity remains. Implementation can begin with Phase 6.1.
