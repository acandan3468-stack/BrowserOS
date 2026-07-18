# BrowserOS

A modular browser automation framework built in Rust. Provides programmatic control over Chromium-based browsers via the Chrome DevTools Protocol (CDP), with an integrated LLM gateway, MCP server, DAG execution engine, and session management.

## Architecture

```
browseros-types/          # Core types shared across all crates
browseros-config/         # Configuration management
browseros-observability/  # Telemetry and metrics
browseros-event-bus/      # Inter-crate event dispatch
browseros-lifecycle/      # Service lifecycle management
browseros-scheduler/      # Task scheduling
browseros-runtime/        # Async runtime abstraction
browseros-bridge/         # Browser-agnostic protocol bridge
browseros-browser/        # Browser instance management
browseros-page/           # Page/tab lifecycle
browseros-cdp/            # Chrome DevTools Protocol client
browseros-dom/            # DOM manipulation API
browseros-storage/        # State persistence
browseros-dag/            # Directed Acyclic Graph execution engine
browseros-llm/            # LLM gateway (multi-provider, streaming, caching)
browseros-mcp/             # Model Context Protocol server
```

## Status

Workspace with 17 Rust crates. Core runtime and browser automation layers are complete. LLM gateway is production-ready with 6 provider adapters. MCP server is frozen. DAG engine supports parallel execution, retry, timeout, and MCP planning integration.

## Quick Start

```bash
cargo build --workspace
cargo test --workspace
```

Requires Rust nightly and a Chromium-based browser for integration tests.

## License

MIT
