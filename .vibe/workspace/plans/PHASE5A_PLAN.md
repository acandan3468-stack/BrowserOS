# Phase 5A Plan — Operational Validation

## Objective

Validate browseros-llm against real-world LLM providers, production scenarios,
and edge cases. Phase 5A produces validation **definitions only** — no code,
no benchmarks, no optimizations.

## Scope

### Targets (6 providers)

| # | Provider | Type | Auth required | Notes |
|---|----------|------|---------------|-------|
| 1 | OpenAI | Remote (HTTP) | API key | Chat, streaming, tool calls, embeddings |
| 2 | Anthropic | Remote (HTTP) | API key | Chat, streaming, tool calls |
| 3 | Gemini | Remote (HTTP) | API key | Chat, streaming, tool calls, embeddings |
| 4 | Ollama | Local (HTTP) | None | Chat, streaming, embeddings (no tools) |
| 5 | Generic HTTP | Remote (HTTP) | Varies | Custom endpoint, OpenAI-compatible |
| 6 | MCP stdio | Local (subprocess) | None | Tool discovery + execution via subprocess |

### Validation scenarios (14)

| # | Scenario | Description |
|---|----------|-------------|
| 1 | Chat completion (basic) | Simple request, verify response |
| 2 | Streaming chat | SSE stream, verify chunk sequence |
| 3 | Tool calling | Function-based tool use, verify tool execution |
| 4 | Retry on transient error | Simulate network blip, verify retry |
| 5 | Fallback chain | Primary fails → fallback, verify switch |
| 6 | Timeout handling | Exceed timeout, verify error |
| 7 | Request cancellation | Drop stream handle mid-flight |
| 8 | Malformed response | Inject bad data, verify graceful error |
| 9 | Rate limiting | Exceed rate limit, verify backoff |
| 10 | Auth failure | Invalid/expired key, verify error |
| 11 | Cache behaviour | Repeated request, verify cache hit |
| 12 | Telemetry correctness | Events emitted, verify path |
| 13 | Cost tracking | Token counts, verify accuracy |
| 14 | Health reporting | Provider up/down, verify health state |

### Out of scope

- Performance benchmarking (Phase 5B)
- Optimization or code changes
- New provider adapters (Phase 5C)
- Async runtime migration (Phase 6+)
- 3rd-party API stability testing

## Risk assessment

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Real API calls incur cost | High | Low | Use smallest/cheapest models; mock for auth/rate-limit tests |
| API key leakage in logs | Low | High | All keys via env vars; never hardcoded in tests |
| Network-dependent tests flaky | Medium | Medium | Retry wrapper; mark as `#[ignore]` when offline |
| Provider API changes | Low | Medium | Pin test models; document version used per provider |
| Rate-limiting during test suite | Medium | Low | Sequential execution; delay between runs |
| MCP subprocess platform differences | Medium | Medium | Test on Windows + Linux + macOS runners |

## Exit criteria

1. All 6 providers validated against all applicable scenarios (84+ cells in matrix)
2. Validation matrix published with pass/fail/skip per cell
3. Each failure has a linked issue or documented known limitation
4. Benchmark spec defines methodology, metrics, and environment
5. All quality gates pass after validation tests are added (build, fmt, clippy)
6. No code changes to frozen Phase 4 public API
