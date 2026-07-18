# Benchmark Specification — browseros-llm

## Purpose

Define a repeatable, platform-independent methodology for measuring
browseros-llm performance across all provider adapters and Gateway subsystems.

This document defines **what** to measure and **how** — not the results.
Actual measurements will be performed in Phase 5B.

## Environment specification

| Parameter | Fixed value | Notes |
|-----------|-------------|-------|
| CPU | TBD (record at run time) | |
| RAM | TBD (record at run time) | |
| OS | Windows 11 / Ubuntu 22.04 / macOS 14 | Run on all 3 |
| Rust | stable (MSRV = 1.81) | `rustc --version` |
| Build | `--release` | Profile with LTO |
| Network | > 100 Mbps, < 10 ms to provider | Record latency to each endpoint |
| Ollama model | `llama3.2:3b` | Local, quantised Q4_K_M |
| MCP server | `@modelcontextprotocol/server-filesystem` | Node.js 20+ |
| Warm-up | 10 requests (discarded) | Eliminate cold-start bias |
| Iterations | 100 per metric | Sufficient for p50/p95/p99 |
| Cooldown | 5 s between test groups | Prevent thermal throttling |

## Metrics categories

### M1 — Request latency (chat)

| Metric | Unit | Collection | Definition |
|--------|------|------------|------------|
| p50 | ms | Per-request timer | Median of 100 requests |
| p95 | ms | Per-request timer | 95th percentile |
| p99 | ms | Per-request timer | 99th percentile |
| Max | ms | Per-request timer | Maximum observed |
| Mean | ms | Per-request timer | Arithmetic mean |

**Payload**: `LlRequest` with 3 messages (system + user + assistant), 500 tokens total.
**Model**: Smallest available per provider (gpt-4o-mini, claude-3-haiku, gemini-1.5-flash, llama3.2).

### M2 — Streaming latency

| Metric | Unit | Collection | Definition |
|--------|------|------------|------------|
| TTFT (time to first token) | ms | First `Chunk` event | From `chat_stream()` call to first chunk |
| Tokens per second | t/s | Total tokens / total duration | Wall-clock from first to last chunk |
| Inter-token latency p50 | ms | Time between consecutive chunks | Median gap |
| Inter-token latency p95 | ms | Time between consecutive chunks | 95th percentile |

**Payload**: Same as M1 with `stream: true`.
**Token counting**: Use provider-reported token counts for consistency.

### M3 — Tool-calling latency

| Metric | Unit | Collection | Definition |
|--------|------|------------|------------|
| Tool call p50 | ms | Single tool call | From `chat()` to response with `ToolCalls` finish_reason |
| Tool call with 5 tools | ms | Single request | 5 `LlTool` definitions, measure response time |
| MCP tool execution | ms | From `call_tool()` to result | Excluding subprocess spawn |

### M4 — Throughput (chat)

| Metric | Unit | Collection | Definition |
|--------|------|------------|------------|
| Peak RPS | req/s | Increment concurrency until saturation | Count completed requests / duration |
| Requests at concurrency 1 | req/s | Sequential loop | |
| Requests at concurrency 4 | req/s | 4 threads | |
| Requests at concurrency 8 | req/s | 8 threads | |

### M5 — Cache performance

| Metric | Unit | Collection | Definition |
|--------|------|------------|------------|
| Cache hit latency | µs | Repeated identical request | From `chat()` with cache hit |
| Cache miss latency | ms | First request (cold) | Same as M1 |
| Hit ratio inflection | % | Vary TTL (60/300/600/3600 s) | Throughput when cache starts evicting |
| Memory usage per 1000 entries | MB | Insert 1000 distinct responses | `cache.max_entries = 1000` |

### M6 — Allocation profile

| Metric | Unit | Collection | Definition |
|--------|------|------------|------------|
| Allocations per `chat()` | count | `dhat` or `alloc-count` heap profiler | Mode = cold request |
| Allocations per `chat_stream()` | count | Same | Include background thread |
| Allocations per `call_tool()` (MCP) | count | Same | Include JSON-RPC parsing |
| Bytes allocated per `chat()` | bytes | Same | |
| Peak heap | MB | Same | Max RSS during 100 requests |

### M7 — CPU profile

| Metric | Unit | Collection | Definition |
|--------|------|------------|------------|
| CPU time per chat | ms | `std::time::Instant` on caller thread | Excludes network I/O wait |
| CPU time per stream chunk | µs | Per chunk in background thread | |
| CPU time per MCP tool call | ms | Adapter-side only | |
| Lock contention | events | `parking_lot` or std Mutex instrumentation | Count of spins > 1 ms |

### M8 — Startup time

| Metric | Unit | Collection | Definition |
|--------|------|------------|------------|
| `LlGateway::build()` (no MCP) | ms | Single measurement | 1 provider, 1 model |
| `LlGateway::build()` (1 MCP server) | ms | Single measurement | MCP stdio subprocess |
| `LlGateway::build()` (3 MCP servers) | ms | Single measurement | Parallel subprocess startup |
| First request latency | ms | First `chat()` after build | Including potential lazy init |

### M9 — Streaming memory stability

| Metric | Unit | Collection | Definition |
|--------|------|------------|------------|
| RSS delta during 60 s stream | MB | `memory-stats` before/after | Long-running stream |
| Background thread stack | KB | Platform default | `pthread_get_stacksize_np` or equivalent |
| Channel buffer occupancy | % | Peak `len()` of `mpsc::SyncSender` | At consumer = slow reader |

### M10 — MCP-specific

| Metric | Unit | Collection | Definition |
|--------|------|------------|------------|
| Subprocess spawn time | ms | `Command::spawn` to first stdin write | |
| JSON-RPC roundtrip (empty) | µs | `initialize` handshake | Excluding server processing |
| Thread allocation per tool call | count | Thread count before/after | Verify thread-per-read pattern |
| Concurrent tool call throughput | calls/s | 10 parallel callers | Adapter thread safety |

## Benchmark harness

### Required capabilities

1. Accept provider TOML config file
2. Accept metric selection (`--metrics M1,M2,...`)
3. Output JSON results per metric (p50, p95, p99, mean, stddev, count)
4. Accept `--iterations N`, `--warmup N`, `--concurrency N`
5. Accept `--format json|markdown|html`
6. Output machine-readable timestamped file for CI comparison

### Framework recommendation

A standalone binary (`benches/production_bench.rs` or a crate example)
using `std::time::Instant` + `criterion` for statistical rigour.
No async runtimes.

### Environment variables

```
OPENAI_API_KEY=
ANTHROPIC_API_KEY=
GEMINI_API_KEY=
```

## Result format

```json
{
  "benchmark": "browseros-llm phase 5B",
  "timestamp": "2026-07-18T00:00:00Z",
  "environment": {
    "os": "windows 11",
    "cpu": "AMD Ryzen 9 7950X",
    "ram_gb": 64,
    "rustc": "1.81.0",
    "build_profile": "release"
  },
  "results": [
    {
      "metric": "M1.chat_latency.p50",
      "provider": "openai",
      "unit": "ms",
      "value": 1234.5,
      "count": 100,
      "p95": 1500.0,
      "p99": 1800.0,
      "stddev": 120.0
    }
  ]
}
```
