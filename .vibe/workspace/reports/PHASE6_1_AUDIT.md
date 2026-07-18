# Phase 6.1 Engineering Freeze Review — browseros-mcp

**Audit Date:** 2026-07-18  
**Auditor:** Engineering Freeze Review  
**Scope:** `browseros/browseros-mcp/` — all modules, public API, tests

---

## Executive Summary

`browseros-mcp` is a clean, well-structured crate with strong separation of concerns, minimal dependencies, and a solid test suite. The implementation is production-viable for the Phase 6.1 scope (stdio transport, JSON-RPC dispatcher, ToolRegistry, system tools).

One architectural concern exists: **no design document in the workspace specified `browseros-mcp` as a standalone server crate**. The only MCP-related design (`llm-mcp-adapter.md`) describes a client-side MCP *adapter* within `browseros-llm`. This freeze review can proceed, but an ADR should be recorded to formalize the server crate's existence.

All acceptance criteria pass or partially pass. No blocking issues are present. Four medium-severity findings and six recommendations are documented below.

| Score | Value |
|-------|-------|
| Architecture | 8/10 |
| Maintainability | 9/10 |
| Production Readiness | 7/10 |

---

## 1. Architecture Consistency

### Design Documents Reviewed
- `architecture-freeze-v3.md` — 14 frozen crates; `browseros-mcp` is absent
- `architecture-invariants-v3.md` — No `browseros-mcp` references
- `phase1-freeze.md` — Lists future crates (dag, plugin, network, input, artifact); `browseros-mcp` absent
- `design-decisions.md` — 21 ADRs; none cover an MCP server crate
- `architecture-v2.md` — Crate inventory stops at 14 crates
- `llm-mcp-adapter.md` — Describes MCP as a **client-side adapter** inside `browseros-llm` for connecting TO external MCP servers

### Finding A1 — Missing design document (MEDIUM)
`browseros-mcp` exists as a standalone server crate but is not documented in any architecture freeze document. The sole MCP design (`llm-mcp-adapter.md`) describes an entirely different concept (client adapter).

**Evidence:** The `llm-mcp-adapter.md` diagram shows MCP adapter as a subordinate of `browseros-llm`, connecting *outbound* to external MCP servers. `browseros-mcp` is the *inbound* server — other MCP clients connect TO BrowserOS.

**Impact:** Future maintainers will not find design rationale for this crate's existence.

**Recommendation:** File an ADR (ADR-022) documenting `browseros-mcp` as the external-facing MCP server crate.

### Finding A2 — Dependency direction (INFO)
`browseros-mcp` depends on `browseros-llm` for JSON-RPC types (`browseros_llm::mcp::jsonrpc`). This means the MCP *server* depends on the LLM crate, which also contains MCP *client* code.

**Impact:** Low — the JSON-RPC types are genuinely shared. Consider moving `JsonRpcMessage`, `JsonRpcError`, etc. to `browseros-types` or a shared `browseros-jsonrpc` crate if the server crate would otherwise be in the `browseros-llm` dependency tree.

### Deviation Summary

| # | Expected (from docs) | Actual | Severity |
|---|----------------------|--------|----------|
| 1 | MCP is a client adapter in `browseros-llm` | MCP is a standalone server crate | Info — new crate, not a violation |
| 2 | No MCP server crate in crate inventory | `browseros-mcp` exists | MEDIUM — needs ADR |
| 3 | `ToolRegistry` design per `llm-mcp-adapter.md` (3 methods) | `ToolRegistry` (deviated) | Info — adaptation for server context |

---

## 2. Public API Review

### 2.1 Module Visibility

| Module | Visibility | Has Doc Comment? | Issue |
|--------|-----------|------------------|-------|
| `error` | `pub` | Module-level: No | Minor |
| `server` | `pub` | Module-level: No | Minor |
| `tools` | `pub` | Module-level: No | Minor |
| `types` | `pub` | Module-level: No | Minor |
| `lib.rs` | `pub` | Yes (crate-level) | ✓ |

### 2.2 Re-exports (lib.rs)

All re-exports are clean. One concern: `McpToolDef` is re-exported but is a type from `browseros-llm` — consumers will depend on `browseros-llm` transitively.

### 2.3 McpTool Trait (tools/mod.rs:13)

| Method | Required? | Issue |
|--------|-----------|-------|
| `name()` | Required | ✓ |
| `description()` | Required | ✓ |
| `input_schema()` | Required | ✓ |
| `execute()` | Required | ✓ |
| `display_name()` | Optional | ✓ |
| `visibility()` | Optional | ✓ |
| `capabilities()` | Optional | ✓ |
| `permissions()` | Optional | ✓ |
| `is_hidden()` | Optional | ⚠️ REDUNDANT: duplicates `visibility() == ToolVisibility::Hidden` |
| `is_internal()` | Optional | ⚠️ REDUNDANT: duplicates `visibility() == ToolVisibility::Internal` |
| `is_diagnostic()` | Optional | ⚠️ REDUNDANT: duplicates `visibility() == ToolVisibility::Diagnostic` |
| `categories()` | Optional | ✓ |
| `tool_def()` | Optional | ✓ |

### Finding B1 — Duplicate tool visibility API (MEDIUM)
Three methods (`is_hidden`, `is_internal`, `is_diagnostic`) duplicate the `visibility()` method's `ToolVisibility` enum. This creates two paths for the same concern, increasing maintenance cost and potential for inconsistency.

**Location:** `browseros-mcp/src/tools/mod.rs:39-49`  
**Recommendation:** Remove `is_hidden()`, `is_internal()`, `is_diagnostic()` and derive all filtering from `visibility()` in `list_tools()`.

### Finding B2 — `categories()` lifetime (LOW)
`categories()` returns `Vec<&str>` — the lifetime is tied to `&self`, but callers may need owned strings.

**Location:** `browseros-mcp/src/tools/mod.rs:51`  
**Recommendation:** Change to `Vec<String>` or `&[&str]`.

### Finding B3 — Missing `version()` on McpTool (LOW)
The `McpTool` trait has no `version()` method. Noted in the conversation as "deferred to Phase 8+". This is acceptable for Phase 6.1 but should be tracked.

### Finding B4 — `McpToolContext` lacks session/notification fields (INFO)
Per design intent, session, cancel_token, and notification_sink fields are deferred to Phase 6.2+. Current structure is adequate.

---

## 3. Thread-Safety Review

### 3.1 Arc Usage

| Component | Arc Wrapping | Correct? |
|-----------|-------------|----------|
| `McpServer.runtime` | `Arc<RuntimeContext>` | ✓ |
| `McpServer.registry` | `Arc<ToolRegistry>` | ✓ |
| `McpServer.metrics` | `Arc<ServerMetrics>` (SharedMetrics) | ✓ |
| `McpServer.stdout` | `Arc<Mutex<Stdout>>` | ✓ |
| `McpServer.shutdown` | `Arc<AtomicBool>` | ✓ |

### 3.2 Lock Analysis

| Location | Lock Acquired | Duration | Risk |
|----------|--------------|----------|------|
| `register()` | `tools.write()`, then `namespace_index.write()` | Brief | ✓ — consistent ordering |
| `execute_by_name()` | `tools.read()` | Brief | ✓ |
| `list_tools()` | `tools.read()` | Brief | ✓ |
| `list_all()` | `tools.read()` | Brief | ✓ |
| `write_response()` | `stdout.lock()` | Brief | ✓ |
| `start()` parse error | `stdout.lock()` | Brief | ✓ |

### Finding C1 — Partial state on failed namespace_index write (LOW)
In `register()` (tools/mod.rs:102-105), if `namespace_index.write()` fails (poisoned), the tool is already inserted into `tools` but the namespace index is stale. The error is silently ignored (map_err used only for poison check).

```rust
if let Ok(mut index) = self.namespace_index.write() {
    index.entry(namespace).or_default().push(name.to_string());
}
```

**Recommendation:** Use `?` instead of `if let Ok(...)` to fail the registration if the index lock is poisoned, or roll back the `tools` insertion.

### Finding C2 — Signal handler is not a signal handler (MEDIUM)
`install_signal_handler()` in `lifecycle.rs:10` does not install any OS signal handler. It spawns a thread that polls `shutdown_requested` every second — which is a polling thread, not a signal handler. On Windows, Ctrl+C will NOT trigger shutdown because no `SetConsoleCtrlHandler` is installed.

**Locations:**
- `lifecycle.rs:10-17` (spawns a pointless thread)
- `mod.rs:37` (calls `install_signal_handler`)

**Impact:** Ctrl+C kills the process immediately without cleanup. Shutdown must be driven by stdin EOF or `shutdown` JSON-RPC request.

**Recommendation:** Either:
1. Remove the misleading function entirely (Phase 6.1 shutdown via EOF+RPC is complete without it), or
2. Add `ctrlc` crate dependency and install a real handler.

### Finding C3 — Send + Sync Correctness (PASS)
All public types are `Send + Sync`:
- `McpTool: Send + Sync` — enforced by trait bound
- `ToolRegistry: Send + Sync` — `RwLock<HashMap<...>>` is `Send + Sync`
- `McpServer: Send + Sync` — all fields are `Send + Sync`
- `McpServerError: Send + Sync` — derives from `String`

---

## 4. Performance Review

### Finding D1 — Unnecessary allocation in dispatch loop (LOW)
`context.clone()` at `mod.rs:35` clones `McpToolContext` (cheap — just `Arc` bump). Acceptable.

### Finding D2 — JSON-to-string round trips (INFO)
`serde_json::to_string_pretty()` in both system tools, then returned as text content, then parsed by client. For system tools this is acceptable, but for high-frequency data tools this adds overhead.

### Finding D3 — No line length limit (LOW)
`StdioReader::read_line()` (transport.rs:20-29) allocates a `String` with no capacity limit. A malicious client sending an unbounded line could exhaust memory.

**Recommendation:** Add a maximum line length (e.g., 10MB) and return `TransportError` on oversize.

### Mutex contention is negligible — all locks held briefly.

---

## 5. Security Review

### Finding E1 — No tool name validation beyond `contains('/')` (LOW)
`register()` only checks that the name contains a `/`. Names like `../../../etc` or `//` or spaces are accepted.

**Location:** `tools/mod.rs:87-91`  
**Recommendation:** Validate that namespace and name are valid identifiers (alphanumeric + underscores/hyphens, no leading dots, no path separators).

### Finding E2 — Silent serialization failure (MEDIUM)
`SystemHealthTool::execute()` and `SystemVersionTool::execute()` use `serde_json::to_string_pretty(&health).unwrap_or_default()`. If serialization fails (impossible for json!() values today, but future refactoring could introduce non-serializable fields), a blank string is returned with `is_error: false` — the client sees a successful response with empty content.

**Location:** `tools/system/health.rs:52`, `tools/system/version.rs:48`  
**Recommendation:** Use `.map_err(|e| ToolError::ExecutionError(e.to_string()))?` instead.

### Finding E3 — Parse error response manually constructed (LOW)
In `mod.rs:67-80`, the parse error response is manually constructed as `serde_json::json!({...})` instead of using `McpServerError::ParseError(...).to_jsonrpc_error()`. This duplicates the error code (-32700) and serialization logic.

**Location:** `server/mod.rs:67-80`  
**Recommendation:** Use `McpServerError::ParseError(e.to_string())` consistently.

### Finding E4 — No request size limits (INFO)
A client could send 1GB of data in a single line. Acceptable for Phase 6.1; add in production hardening.

### stdin/stdout separation: ✅ PASS after `with_stderr_logging()` fix.

---

## 6. Production Readiness

### Strengths
- All 8 integration tests pass, 15 unit tests pass
- Cross-platform (no Unix-specific APIs)
- Clean shutdown via EOF, `shutdown` RPC, or `exit` notification
- Logs go to stderr (fixed), protocol on stdout
- No unwrap in production paths (except `unwrap_or_default` — see E2)
- Builder pattern prevents half-constructed states

### Weaknesses
| Area | Current State | Impact |
|------|--------------|--------|
| Shutdown | Ctrl+C kills process immediately — no cleanup | Medium |
| Metrics | In-memory only, lost on restart | Low |
| Logging | Info-level only, no configurable level | Low |
| Error reporting | Parse errors handled outside error type | Low |
| Diagnostics | No health endpoint beyond basic uptime | Info |

### Finding F1 — Ctrl+C kills without cleanup (MEDIUM)
Same as C2. On Windows, Ctrl+C immediately terminates the process; `Drop` impls run for `Mutex<Stdout>` but not for application-level cleanup.

### Finding F2 — MetricsSnapshot serialized inline uses extra serde pass (LOW)
In `dispatcher.rs:85-90`, metrics snapshot values are read from atomics and assembled into `serde_json::json!()`. Then in `dispatcher.rs:95-98`, they are embedded in another `json!()` — which requires `MetricsSnapshot` to be serializable (it is). But the code manually extracts fields. This is a minor inconsistency — either use `MetricsSnapshot` directly or extract manually.

---

## 7. Code Quality

### Finding G1 — Misleading module name: `lifecycle.rs` (LOW)
The module is named `lifecycle` but contains only `install_signal_handler()` — a single function that does not install a signal handler. No lifecycle management.

**Recommendation:** Rename to `signal.rs` or fold the one function into `mod.rs` or `metrics.rs`.

### Finding G2 — Redundant API surface (MEDIUM)
Same as B1. Three boolean methods on `McpTool` trait duplicate `ToolVisibility` enum.

### Finding G3 — namespace extraction is correct but non-obvious (INFO)
`name.rsplitn(2, '/').last()` correctly extracts the namespace (left side of the rightmost `/`). Consider documenting this behavior.

### Module Boundaries

| Module | Responsibility | Clean? |
|--------|---------------|--------|
| `error.rs` | Server errors + JSON-RPC mapping | ✓ |
| `types.rs` | Domain types | ✓ |
| `tools/mod.rs` | Trait + registry | ✓ (except redundant methods) |
| `tools/system/` | System tools | ✓ |
| `server/mod.rs` | Builder + main loop | ✓ |
| `server/transport.rs` | Stdio reader | ✓ |
| `server/dispatcher.rs` | Request routing | ✓ |
| `server/lifecycle.rs` | Signal polling | ⚠️ Misnamed |
| `server/metrics.rs` | Counters + snapshot | ✓ |

---

## 8. Acceptance Criteria Verification

| # | Criterion | Result | Evidence |
|---|-----------|--------|----------|
| 1 | Crate skeleton | ✅ PASS | `Cargo.toml`, `lib.rs`, module structure |
| 2 | Stdio transport | ✅ PASS | `server/transport.rs` — `StdioReader` with `BufReader<Stdin>` |
| 3 | JSON-RPC dispatcher | ✅ PASS | `server/dispatcher.rs` — `initialize`, `tools/list`, `tools/call`, `shutdown` |
| 4 | ToolRegistry | ✅ PASS | `tools/mod.rs` — register, execute_by_name, list_tools, list_all |
| 5 | McpTool trait | ✅ PASS | `tools/mod.rs` — 4 required + 8 optional methods |
| 6 | system/health + system/version | ✅ PASS | `tools/system/health.rs`, `tools/system/version.rs` |
| 7 | 15 unit + 8 integration tests | ✅ PASS | `cargo test -p browseros-mcp` — 23 tests pass |
| 8 | No placeholder implementations | ✅ PASS | No `todo!()`, `unimplemented!()`, or `panic!()` in production |
| 9 | No unwrap() in production paths | ⚠️ PARTIAL | `unwrap_or_default()` in health.rs:52, version.rs:48 silently swallows errors |
| 10 | Zero unsafe | ✅ PASS | `grep -r unsafe` — zero occurrences |
| 11 | clippy clean (-D warnings) | ✅ PASS | `cargo clippy -p browseros-mcp -- -D warnings` — clean |
| 12 | cargo fmt clean | ✅ PASS | `cargo fmt --check -p browseros-mcp` — clean |
| 13 | Preserve frozen architecture decisions | ✅ PASS | No frozen API is modified |
| 14 | Stop at dependency boundaries | ✅ PASS | All required APIs exist in dependent crates |
| 15 | Minimal dependencies | ✅ PASS | 4 direct dependencies (runtime, llm, serde, serde_json, chrono) |
| 16 | RuntimeContext injection (no globals) | ✅ PASS | `McpServerBuilder.with_runtime(Arc<RuntimeContext>)` |

---

## Issue Summary

### Critical (Must Fix Before Freeze)
None.

### Required Fixes (Should Fix Before Production)
| ID | Description | Location | Severity |
|----|------------|----------|----------|
| C2/F1 | `install_signal_handler` does not install a signal handler; Ctrl+C kills without cleanup | `lifecycle.rs:10` | MEDIUM |
| E2 | `unwrap_or_default()` silently swallows serialization errors | `health.rs:52`, `version.rs:48` | MEDIUM |

### Recommended Improvements
| ID | Description | Location | Severity |
|----|------------|----------|----------|
| A1 | File ADR for `browseros-mcp` crate existence | Workspace docs | MEDIUM |
| B1/G2 | Remove `is_hidden()`, `is_internal()`, `is_diagnostic()` — duplicate `visibility()` | `tools/mod.rs:39-49` | MEDIUM |
| C1 | Fix partial state on poisoned namespace_index write | `tools/mod.rs:102-105` | LOW |
| D3 | Add max line length to StdioReader | `transport.rs:20` | LOW |
| E1 | Validate tool name format | `tools/mod.rs:87` | LOW |
| E3 | Use `McpServerError::ParseError` instead of manual JSON | `mod.rs:67-80` | LOW |
| G1 | Rename `lifecycle.rs` to `signal.rs` or inline | `lifecycle.rs` | LOW |

---

## Final Verdict

```
╔══════════════════════════════════════════════════════════╗
║                APPROVED WITH REQUIRED FIXES              ║
╠══════════════════════════════════════════════════════════╣
║  No blocking issues. Two medium-severity fixes needed   ║
║  before production deployment. Freeze is otherwise      ║
║  acceptable for the Phase 6.1 scope.                    ║
╚══════════════════════════════════════════════════════════╝
```

### Required Before Production

1. **C2/F1 — Signal handling:** Either remove the misleading `install_signal_handler` (shutdown via EOF+RPC is complete) or add real Ctrl+C handling.
2. **E2 — Silent error swallowing:** Replace `unwrap_or_default()` with `map_err` in both system tools so serialization failures produce proper error responses.

### Approved For

- Phase 6.1 freeze
- Integration with Phase 6.2 (SessionManager)
- All existing tests and tool implementations
