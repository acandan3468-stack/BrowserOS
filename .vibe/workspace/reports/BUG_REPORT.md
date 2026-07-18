# Bug Report — browseros-llm MCP Server

## Bugs Found During Phase 5B (Real MCP Client Validation)

---

### B1: Observability Logs Written to stdout (CRITICAL)

**Status:** FIXED  
**Severity:** Critical  
**Found:** 2026-07-16, Phase 5B execution

#### Description
`LlmTelemetry::new_enabled()` in `src/telemetry.rs:150` hardcodes `StdoutSink`, causing all `browseros-observability` log output (INFO, DEBUG, WARN) to go to stdout. In MCP mode, stdout is the exclusive JSON-RPC transport channel — any non-JSON output corrupts the protocol stream, causing client-side `SyntaxError: Unexpected non-whitespace character after JSON`.

#### Impact
- MCP client receives log lines (e.g., `2026-07-16T22:50:30.325Z DEBUG browseros-llm - llm_cache_miss key=...`) instead of JSON-RPC responses
- `chat` and `embed` tool calls always crash the protocol stream on the first log-producing operation
- Server binary is effectively unusable as an MCP server
- No test caught this because all 595 prior tests use `cargo test`'s captured stdout, where log interleaving is invisible

#### Root Cause
```rust
// telemetry.rs:150-154 (BEFORE)
pub fn new_enabled() -> Self {
    let metrics = MetricsRegistry::new();
    let filter = Arc::new(LevelFilter::new(LogLevel::Trace));
    let sink = Arc::new(browseros_observability::export::StdoutSink::new());  // WRONG
    let logger = Logger::new(sink, filter, "browseros-llm");
```

#### Fix
Changed `StdoutSink::new()` → `StderrSink::new()` on line 153 of `src/telemetry.rs`. Logs now correctly route to stderr, preserving stdout for JSON-RPC.

```rust
// telemetry.rs:150-154 (AFTER)
pub fn new_enabled() -> Self {
    let metrics = MetricsRegistry::new();
    let filter = Arc::new(LevelFilter::new(LogLevel::Trace));
    let sink = Arc::new(browseros_observability::export::StderrSink::new());  // FIXED
    let logger = Logger::new(sink, filter, "browseros-llm");
```

#### Verification
- Rebuilt `llm_gateway_server.exe` with fix
- Full e2e MCP test: 34/34 PASS (100%)
- `cargo test`: all 595 existing tests still pass
- Server now handles chat/embed/health without corrupting stdout
- No observable behavior change for non-MCP use cases

#### Files Changed
| File | Change |
|------|--------|
| `browseros/browseros-llm/src/telemetry.rs:153` | `StdoutSink::new()` → `StderrSink::new()` |

---

### No Other Bugs Found

All other MCP protocol behaviors were validated as correct:
- JSON-RPC framing: correct `\n`-delimited messages
- Error codes: `-32700` (Parse), `-32601` (MethodNotFound), `-32000` (Application) all conform to MCP spec
- Notification handling: `notifications/initialized` correctly produces no response
- Shutdown: clean exit code 0 on stdin EOF
- Reconnect: fresh server initializes and operates correctly
- Error messages: descriptive, human-readable
- Input validation: empty messages, missing required fields, nonexistent models all handled gracefully

---

## Known Limitations (Non-Bugs)

These are documented design limitations from `KNOWN_LIMITATIONS.md`, not bugs:

| ID | Limitation | Severity |
|----|-----------|----------|
| L1 | No TLS support (stdio transport only) | Medium |
| L2 | No streaming support (chat blocks) | Medium |
| L3 | No graceful `SIGTERM`/`SIGINT` handler | Low |
| L4 | No `ping`/`pong` implementation | Low |
| L5 | No authentication/authorization | Low |
| L6 | No progress notifications for long operations | Low |
| L7 | No request cancellation (no `$/cancelRequest`) | Low |
| L8 | No resource/prompt capabilities (MCP resources/prompts not exposed) | Low |
| L9 | Chat tool `inputSchema` uses `enum` for role but validates at runtime | Low |
| L10 | Config has `api_key: null` for validation (requires real key for production) | Info |
| L11 | Empty configuration causes `"no providers configured"` health message | Info |
| L12 | No performance telemetry (latency histograms, success rates not exposed via MCP) | Low |

---

*Bug report generated from Phase 5B validation execution. 1 critical bug found and fixed. 0 remaining bugs.*
