# BrowserOS — Phase 1: Core Runtime Plan (Revised)

## Architecture Summary

BrowserOS is an **event-driven modular runtime** designed for browser agent orchestration. Phase 1 delivers the foundational runtime only — no browser logic, no LLM integration.

**Guiding principle:** Every module is independently testable, replaceable, and has explicit boundary contracts. The entire system is built on strongly-typed events flowing through a canonical message protocol.

## Workspace Structure

```
browseros/
├── Cargo.toml
├── browseros-types/           # Canonical types + error system + message protocol
├── browseros-macros/          # Proc macros (#[derive(Event)], etc.)
├── browseros-config/          # Layered configuration
├── browseros-observability/   # Logger + Metrics + Tracing
├── browseros-lifecycle/       # Lifecycle + Health + Resource tracking
├── browseros-event/           # Event Bus + Middleware + Dead Letter
├── browseros-scheduler/       # Delayed/event-triggered scheduler (no cron)
├── browseros-store/           # Event Store + State Store
├── browseros-dag/             # DAG Engine
├── browseros-plugin/          # Plugin Registry + Capability Registry
├── browseros-core/            # Facade crate — RuntimeContext + RuntimeBuilder
└── tests/                     # Integration tests
```

## 12 ADRs (Architecture Decision Records)

See `.vibe/workspace/plans/architecture-review.md` for full rationale.

| ADR | Decision |
|-----|----------|
| 1 | Multi-crate workspace for compile-time boundary enforcement |
| 2 | Zero-dependency `browseros-types` to prevent circular deps |
| 3 | Event Bus != Scheduler (separate concerns) |
| 4 | Canonical event hierarchy (System / Domain / Metric / Internal) |
| 5 | Message Protocol envelope for cross-subsystem tracing |
| 6 | Plugin lifecycle != Capability discovery (separate registries) |
| 7 | Logger != Metrics (different consumers/policies) |
| 8 | LifecycleManager for all components |
| 9 | RuntimeContext replaces DI container (Rust's type system makes full DI unnecessary) |
| 10 | Resource tracking as module in lifecycle (full Resource Manager in Phase 2) |
| 11 | State Store designed as World State foundation |
| 12 | Observability as first-class crate, OTLP-ready |
| 13 | Scheduler stripped to delayed + event-triggered only (no cron) |
| 14 | Error system merged into types crate |

## Implementation Roadmap

### Phase 1.1 — Foundation
```
1. browseros-types     (Medium)  — zero-dep types, events, errors, message protocol
2. browseros-macros    (Small)   — proc macros for event derive
3. browseros-config    (Medium)  — layered config, env overrides, validation
```

### Phase 1.2 — Observability
```
4. browseros-observability (Medium) — Logger, MetricsRegistry, Tracer, Timer
```

### Phase 1.3 — Runtime Core
```
5. browseros-lifecycle  (Medium)  — ManagedComponent, LifecycleManager, Health, Resource tracker
6. browseros-event      (Large)   — EventBus, Middleware, DeadLetter
7. browseros-scheduler  (Small)   — delayed + event-triggered (no cron)
8. browseros-store      (Large)   — EventStore + StateStore
```

### Phase 1.4 — Execution
```
9. browseros-dag       (Large)   — DagEngine, topological sort, parallel exec
10. browseros-plugin    (Large)   — PluginRegistry + CapabilityRegistry
```

### Phase 1.5 — Integration
```
11. browseros-core      (Medium)  — RuntimeContext + RuntimeBuilder facade
12. Integration tests   (Medium)  — Full lifecycle, DAG, store integ
```

## Dependency Graph

```
browseros-types (zero deps)
  ├── browseros-macros
  ├── browseros-config
  ├── browseros-observability
  │     ├── browseros-lifecycle  (includes health + resource tracking)
  │     │     ├── browseros-event
  │     │     │     ├── browseros-scheduler  (delayed + event-triggered)
  │     │     │     └── browseros-store
  │     │     │           ├── browseros-dag
  │     │     │           └── browseros-plugin
  │     │     └──────────────────────┘
  │     └────────────────────────────┘
  └── browseros-core (facade — RuntimeContext + RuntimeBuilder)
        └── tests
```

## Testing Strategy

| Level | Scope | Method |
|-------|-------|--------|
| Unit | Per-crate, per-module | `cargo test -p <crate>` with mocked dependencies |
| Integration | Cross-crate flows | `cargo test --test <name>` — real module wiring |
| Property-based | Event ordering, DAG validity | `proptest` for random DAG generation, state machine tests |
| Doc tests | Public API examples | Rust doc tests inline |

**Coverage target:** > 80% across all crates.

## Dependencies (external crates)

| Crate | Purpose |
|-------|---------|
| `tokio` (full) | Async runtime, timers, channels |
| `serde` + `serde_json` | Serialization |
| `tracing` + `tracing-subscriber` | Structured logging substrate |
| `uuid` v7 | Time-sortable event IDs |
| `chrono` | UTC timestamps |
| `thiserror` | Error derives |
| `parking_lot` | Fast RwLock/Mutex |
| `dashmap` | Concurrent maps for registries |
| `petgraph` | DAG graph operations |
| `proptest` (dev) | Property-based testing |
| `criterion` (dev) | Benchmarks |
| `tempfile` (dev) | Temporary files for store tests |

## Definition of Done (Phase 1)

- [ ] All 11 crates compile with zero warnings (`RUSTFLAGS="-D warnings"`)
- [ ] `cargo clippy` — no warnings across workspace
- [ ] `cargo test` — all unit/integration/doc tests pass
- [ ] Test coverage > 80% per crate
- [ ] All public APIs have doc comments
- [ ] All 32 Architecture Invariants are satisfied (see `architecture-invariants.md`)
- [ ] Design Freeze document (design-freeze.md) sign-off complete
- [ ] `cargo doc --no-deps` — complete documentation
