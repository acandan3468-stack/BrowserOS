# browseros-mcp Implementation Plan

**Document:** IMPLEMENTATION_PLAN.md  
**Phase:** Design Freeze  
**Date:** 2026-07-16  

---

## Phase 6.1 — Crate Skeleton + System Tools

**Objective:** Establish the crate structure, McpServer startup/shutdown lifecycle, transport layer, and first system tools.

### Deliverables

| File | LOC | Complexity | Dependencies |
|------|-----|------------|--------------|
| `Cargo.toml` | 30 | Low | browseros-runtime, browseros-llm, serde, serde_json, chrono |
| `src/lib.rs` | 20 | Low | None |
| `src/bin/browseros_mcp_server.rs` | 60 | Low | args parse, build, start |
| `src/server/mod.rs` | 100 | Medium | McpServerBuilder, McpServer struct |
| `src/server/transport.rs` | 80 | Medium | StdioReader, line read/write |
| `src/server/dispatcher.rs` | 120 | High | JSON-RPC dispatch, tool call routing |
| `src/server/lifecycle.rs` | 80 | Medium | Startup, shutdown, signal handling |
| `src/server/metrics.rs` | 40 | Low | Server-level atomic counters |
| `src/error.rs` | 100 | Medium | McpServerError, From impls |
| `src/types.rs` | 40 | Low | Shared internal types |
| `src/tools/mod.rs` | 80 | Medium | McpTool trait, ToolRegistry |
| `src/tools/system/mod.rs` | 20 | Low | Module re-exports |
| `src/tools/system/health.rs` | 60 | Medium | Health report from RuntimeContext |
| `src/tools/system/version.rs` | 20 | Low | Version info |

**Phase total:** ~850 LOC  
**Complexity:** Medium  
**Risk:** Low (no browser/network dependencies)

### Acceptance Criteria

1. `cargo build` succeeds
2. `browseros_mcp_server --help` prints usage
3. Server starts, responds to `initialize`, `tools/list`, `system/health`, `system/version`
4. Server shuts down cleanly on stdin EOF
5. Server shuts down cleanly on SIGTERM (Unix)
6. Logs go to stderr, JSON-RPC to stdout
7. All 34 Phase 5B MCP protocol tests pass against the new binary
8. `cargo test` passes for the entire workspace (595+ tests)

### Exit Criteria

- [x] Crate compiles
- [x] Stdio transport works
- [x] Dispatcher handles initialize / tools/list / tools/call
- [x] Health and version tools functional
- [x] Shutdown is clean
- [x] All existing tests still pass

### Risk

- JSON-RPC type duplication between `browseros-llm::mcp::jsonrpc` and `browseros-mcp`. Mitigation: Re-export from `browseros-llm`.

### Test Strategy

- Unit tests for dispatcher (no transport)
- Unit tests for ToolRegistry (registration, lookup, filtering)
- Integration tests: spawn binary, send JSON-RPC over stdio, verify responses (reuse Phase 5B patterns)
- Signal handling: spawn binary, send SIGTERM, verify clean exit

---

## Phase 6.2 — Session Management

**Objective:** Session lifecycle, SessionManager, BrowserPool integration.

### Deliverables

| File | LOC | Complexity | Dependencies |
|------|-----|------------|--------------|
| `src/session/mod.rs` | 80 | Medium | SessionManager, SessionId |
| `src/session/state.rs` | 100 | High | SessionState, resource ownership |
| `src/session/pool.rs` | 60 | Medium | BrowserPool integration |
| `src/session/credentials.rs` | 50 | Low | In-memory credential store |
| `src/tools/session/mod.rs` | 20 | Low | Module re-exports |
| `src/tools/session/create.rs` | 80 | Medium | Session creation logic |
| `src/tools/session/close.rs` | 60 | Medium | Session destruction logic |
| `src/tools/session/list.rs` | 30 | Low | List active sessions |
| `src/tools/session/configure.rs` | 40 | Low | Session config |

**Phase total:** ~520 LOC  
**Complexity:** High (resource ownership)  
**Risk:** Medium (browser allocation failure handling)

### Acceptance Criteria

1. `session/create` returns valid SessionId
2. `session/close` returns usage summary
3. `session/list` returns all active sessions
4. Session idle timeout fires correctly
5. Max concurrent sessions enforced
6. Browser instance released on session close
7. Two sessions can operate independently

### Exit Criteria

- [x] Session create/close/list functional
- [x] Resource cleanup verified (no browser leak)
- [x] Idle timeout works
- [x] Session isolation verified (two concurrent sessions)

### Risk

- Browser instance leak if session close fails mid-way. Mitigation: Drop guard pattern — BrowserInstance is dropped when SessionState is dropped, even if close() panics.

### Test Strategy

- Unit tests: SessionManager (create, close, list, idle timeout)
- Integration tests: create session, verify browser is running, close session, verify browser stops
- Concurrency test: create N sessions, verify each gets unique browser
- Error test: exceed session limit, expect -32002

---

## Phase 6.3 — LLM Integration

**Objective:** Expose LLM capabilities through `browseros-llm` integration via existing RuntimeContext.

### Deliverables

| File | LOC | Complexity | Dependencies |
|------|-----|------------|--------------|
| `src/tools/llm/mod.rs` | 20 | Low | Module re-exports |
| `src/tools/llm/models.rs` | 40 | Low | List available models |

**Phase total:** ~60 LOC  
**Complexity:** Low  
**Risk:** Low (delegates entirely to browseros-llm)

### Acceptance Criteria

1. `llm/models` returns list of available LLM models from LlGateway
2. Models match those configured in `config.llm`

### Exit Criteria

- [x] Tool returns model list
- [x] Integration with LlGateway verified

### Risk

None. Pure delegation.

### Test Strategy

- Integration test: verify `llm/models` returns expected models
- Reuse existing LlGateway unit tests

---

## Phase 6.4 — Notification Infrastructure

**Objective:** Notification dispatcher, EventBus bridge, progress notifications.

### Deliverables

| File | LOC | Complexity | Dependencies |
|------|-----|------------|--------------|
| `src/notify/mod.rs` | 80 | Medium | NotificationDispatcher, subscriber registry |
| `src/notify/progress.rs` | 40 | Low | Progress notification builder |
| `src/notify/events.rs` | 100 | High | EventBus → notification mapping |
| `src/notify/throttle.rs` | 60 | Medium | Per-type throttle policies |

**Phase total:** ~280 LOC  
**Complexity:** Medium (concurrent subscriber management)  
**Risk:** Low

### Acceptance Criteria

1. EventBus events produce MCP notifications
2. Progress notifications emitted during long-running tools
3. Subscriber channels have correct capacity
4. Backpressure handling: overflow → drop oldest
5. Throttle policies enforced

### Exit Criteria

- [x] Notification dispatcher functional
- [x] EventBus integration verified
- [x] Progress notifications work
- [x] Backpressure tested

### Risk

- High-frequency notifications (DOM mutation, network) overwhelm subscriber channels. Mitigation: Batch, throttle, and configurable buffer sizes.

### Test Strategy

- Unit tests: subscriber registry, filter, throttle
- Integration tests: verify notification appears on stdout after EventBus emit
- Backpressure test: flood EventBus, verify subscriber doesn't block, verify dropped counter

---

## Phase 6.5 — System Config + Metrics Tools

**Objective:** Remaining system tools for configuration and monitoring.

### Deliverables

| File | LOC | Complexity | Dependencies |
|------|-----|------------|--------------|
| `src/tools/system/config.rs` | 50 | Medium | Config read/write |
| `src/tools/system/metrics.rs` | 40 | Low | Metrics snapshot |

**Phase total:** ~90 LOC  
**Complexity:** Low  
**Risk:** Low

### Acceptance Criteria

1. `system/config` returns current configuration (sanitized)
2. `system/metrics` returns server-level metrics

### Exit Criteria

- [x] Config tool functional
- [x] Metrics tool functional

### Risk

- Exposing sensitive config values. Mitigation: Sanitize API keys and secrets before returning.

### Test Strategy

- Unit tests: config sanitization
- Integration: verify tools return expected values

---

## Phase 6.6 — Integration + Hardening

**Objective:** End-to-end testing, edge cases, performance baseline.

### Deliverables

| File | LOC | Complexity | Dependencies |
|------|-----|------------|--------------|
| `tests/mcp_e2e.rs` | 200 | Medium | Phase 5B tests adapted for browseros-mcp |
| `tests/session_e2e.rs` | 150 | Medium | Session lifecycle |
| `tests/notification_e2e.rs` | 100 | Medium | Notification delivery |
| `tests/concurrent_e2e.rs` | 150 | High | Concurrency, limits |
| `benches/startup.rs` | 50 | Low | Startup time |
| `benches/dispatch.rs` | 80 | Low | Dispatch throughput |

**Phase total:** ~730 LOC  
**Complexity:** Medium  
**Risk:** Low

### Acceptance Criteria

1. All existing 595+ tests + new tests pass
2. 34 Phase 5B scenario equivalents pass
3. Concurrency test: 4 concurrent sessions, 16 concurrent tool calls
4. Startup time < 100ms (cold, no browser)
5. Dispatch throughput > 1000 msg/s (simple tools)

### Exit Criteria

- [x] All tests pass
- [x] Performance baseline established
- [x] CI integration verified

### Risk

- Concurrency bugs (deadlocks, data races). Mitigation: Lock ordering enforced at compile-time where possible, runtime deadlock detection optionally.

### Test Strategy

- All acceptance criteria serve as test specifications.
- Stress test: 100 concurrent tool calls, verify no crashes

---

## Phase 6.7 — Documentation + Freeze

**Objective:** Complete documentation for Phase 6 release.

### Deliverables

| File | LOC | Complexity |
|------|-----|------------|
| `README.md` | 100 | Low |
| `PHASE6_COMPLETION.md` | 50 | Low |
| Doc comments on all public items | — | Medium |

### Acceptance Criteria

- All public API items have doc comments
- README explains setup, configuration, basic usage
- Completion report written

---

## Phase Summary

| Phase | LOC | Complexity | Risk | Dependencies |
|-------|-----|------------|------|--------------|
| 6.1 Skeleton + System | 850 | Medium | Low | browseros-runtime, browseros-llm |
| 6.2 Session Management | 520 | High | Medium | browseros-browser (BrowserPool) |
| 6.3 LLM Integration | 60 | Low | Low | browseros-llm |
| 6.4 Notification Infrastructure | 280 | Medium | Low | browseros-event-bus |
| 6.5 Config + Metrics | 90 | Low | Low | browseros-runtime |
| 6.6 Integration + Hardening | 730 | Medium | Low | All above |
| 6.7 Documentation + Freeze | 150 | Low | Low | All above |
| **Total Phase 6** | **~2680** | | | |

---

## Post-Phase 6 (Future)

| Feature | Phase | Estimated LOC | Dependencies |
|---------|-------|---------------|--------------|
| Browser tools (navigate, screenshot, evaluate) | 7 | 800 | browseros-browser, browseros-page |
| DOM tools (query, click, type, snapshot, observe) | 7 | 1200 | browseros-dom, browseros-bridge |
| Network tools (intercept, conditions, cookies) | 7 | 600 | browseros-storage, browseros-network |
| Workflow tools (execute, plan, status, cancel) | 8 | 500 | browseros-dag, browseros-llm |
| Storage tools (localstorage, sessionstorage, indexeddb) | 8 | 400 | browseros-storage |
| WebSocket/SSE transport | 8 | 300 | tungstenite (existing dep) |
| Event subscription (client opt-in) | 8 | 200 | browseros-event-bus |
| Plugin system (dynamic loading) | 8+ | 400 | libloading |
| Distributed sessions | 8+ | 600 | redis, serde |

---

*This document defines the complete Implementation Plan. It contains no Rust code, no Cargo.toml, and no placeholders.*
