# Phase 6.1 Required Fixes — Implementation Report

**Date:** 2026-07-18

---

## 1. Files Changed

| File | Change |
|------|--------|
| `browseros-mcp/src/server/lifecycle.rs` | Replaced `install_signal_handler()` with documented comment explaining Phase 6.1 shutdown strategy |
| `browseros-mcp/src/server/mod.rs` | Removed `pub mod lifecycle;`, removed import of `install_signal_handler`, removed call in `start()` |
| `browseros-mcp/src/lib.rs` | Updated module tree comment (removed "lifecycle" from server submodule list) |
| `browseros-mcp/src/tools/system/health.rs` | `unwrap_or_default()` → `map_err()` with explicit error propagation |
| `browseros-mcp/src/tools/system/version.rs` | `unwrap_or_default()` → `map_err()` with explicit error propagation |

---

## 2. Required Fix #1 — Signal Handler

**Strategy chosen: Option A** (remove misleading pseudo handler).

### Rationale

- No existing workspace crate provides cross-platform signal handling.
- Adding a new dependency (e.g. `ctrlc`) solely for signal handling violates the crate's minimal-dependency constraint.
- Phase 6.1 shutdown is already complete via three mechanisms:
  - **stdin EOF** — `McpServer::start()` returns when `StdioReader::read_line()` returns `TransportError`
  - **`shutdown` JSON-RPC request** — sets `shutdown_requested` flag, main loop breaks
  - **`exit` JSON-RPC notification** — sets `shutdown_requested` flag, main loop breaks
- The pseudo function spawned a thread that polled `shutdown_requested` every second — this duplicated the main loop's existing check and did not actually install any OS signal handler.

### What Changed

1. `lifecycle.rs`: Removed the `install_signal_handler()` function and its imports. Replaced with a doc comment documenting how Phase 6.1 shutdown actually works, and noting that real signal handling is deferred to Phase 6.2.

2. `server/mod.rs`:
   - Removed `pub mod lifecycle;` from module declarations (the module is no longer needed, but the file is retained as a documentation anchor for Phase 6.2).
   - Removed `use crate::server::lifecycle::install_signal_handler;`.
   - Removed `install_signal_handler(&self.metrics);` from `McpServer::start()`.

3. `lib.rs`: Updated module tree comment to remove "lifecycle" from the server submodule list.

### Impact

- `lifecycle.rs` still exists as a file but contains only a comment — no code, no exports. It serves as a placeholder for Phase 6.2+.
- Zero behavioral change to startup/shutdown logic.
- All 23 tests continue to pass.

---

## 3. Required Fix #2 — Error Propagation

### Audit of `unwrap_or_default()` occurrences

Only two occurrences existed:

| File | Line (before) | What changed |
|------|---------------|-------------|
| `tools/system/health.rs` | 52 | `serde_json::to_string_pretty(&health).unwrap_or_default()` |
| `tools/system/version.rs` | 48 | `serde_json::to_string_pretty(&info).unwrap_or_default()` |

### Fix applied to both files

```rust
// Before:
Ok(ToolOutput::text(
    serde_json::to_string_pretty(&health).unwrap_or_default(),
))

// After:
let text = serde_json::to_string_pretty(&health)
    .map_err(|e| ToolError::ExecutionError(format!("serialization error: {e}")))?;
Ok(ToolOutput::text(text))
```

The `serde_json::json!()` values are trivially serializable today, so this path would never fail in current code. However, future refactoring could introduce non-serializable fields (e.g. `Arc<dyn Any>`, recursive structures). The fix ensures:
- Serialization failures produce a proper JSON-RPC error response (`-32000` / `ApplicationError`).
- Error context is preserved in the message.
- No silent empty-string fallback that clients cannot distinguish from a valid response.

---

## 4. Quality Gate Results

| Gate | Result |
|------|--------|
| `cargo fmt --check -p browseros-mcp` | ✅ Clean |
| `cargo clippy -p browseros-mcp -- -D warnings` | ✅ Clean |
| `cargo clippy --workspace -- -D warnings` | ✅ Clean |
| `cargo test -p browseros-mcp` | ✅ 23/23 pass (15 unit + 8 integration) |
| `cargo test --workspace` | ✅ 595+ pass (only pre-existing `smoke_browser_launch_and_connect` fails — Chrome not installed) |

---

## 5. Remaining Recommendations Intentionally Deferred

Per audit scope, the following are **NOT implemented** (tracked for future phases):

| ID | Recommendation | Phase |
|----|---------------|-------|
| A1 | File ADR-022 for `browseros-mcp` crate existence | Workspace docs |
| B1/G2 | Remove `is_hidden()`, `is_internal()`, `is_diagnostic()` (duplicate `visibility()`) | Phase 6.2+ |
| C1 | Fix partial state on poisoned `namespace_index` write | Phase 6.2+ |
| D3 | Add max line length to `StdioReader` | Phase 6.2+ |
| E1 | Validate tool name format | Phase 6.2+ |
| E3 | Use `McpServerError::ParseError` instead of manual JSON in main loop | Phase 6.2+ |
| G1 | Rename `lifecycle.rs` to `signal.rs` or inline | Phase 6.2+ |

---

## 6. Freeze Eligibility

**Phase 6.1 is now eligible for DESIGN FREEZE.**

All 16 acceptance criteria: **15 ✅ PASS, 1 ⚠️ PARTIAL** (pre-existing `unwrap_or_default` — now FIXED).
Both required fixes from the audit are implemented.
All quality gates pass.
