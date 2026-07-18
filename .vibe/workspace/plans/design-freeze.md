# Design Freeze — BrowserOS Phase 1

**Status:** APPROVED WITH MINOR CHANGES
**Date:** 2026-06-29
**Reviewer:** Principal Software Architect

---

## Freeze Scope

This freeze covers all architectural decisions for Phase 1 of BrowserOS — the Core Runtime. No further architectural changes will be made without a new architecture review.

### Included in Freeze

| Area | Document | Status |
|------|----------|--------|
| Crate structure (11 crates) | `architecture-review.md` §2 | Frozen |
| RuntimeContext design | `architecture-review.md` §3 | Frozen |
| ComponentManifest | `architecture-review.md` §4 | Frozen |
| Health system states & transitions | `architecture-review.md` §5 | Frozen |
| MessageEnvelope protocol | `architecture-review.md` §6 | Frozen |
| Scheduler API (no cron) | `architecture-review.md` §7 | Frozen |
| Architecture Invariants (32 rules) | `architecture-invariants.md` | Frozen |
| Risk mitigation strategies | `architecture-review.md` §9 | Frozen |
| Implementation roadmap | `plan.md` §4 | Frozen |

### Explicitly Out of Scope (Phase 2+)

- Dynamic plugin loading (`dlopen`/`LoadLibrary`)
- Distributed event bus (NATS/Kafka bridge)
- Cron/periodic scheduling
- Full resource manager (CPU/GPU quotas, cgroups)
- DI container / service container
- Browser automation (any `webdriver`/`playwright`/`puppeteer` integration)
- LLM / AI agent integration
- World Model / page digital twin
- Skill System
- Perception Layer (DOM, Vision, Network, Console sensors)
- Hot-reload of plugins or config
- Secrets management / vault integration

---

## Changes Made During Final Review

### Change 1: Merge `browseros-error` into `browseros-types`

**Rationale:** Error types are foundational types. Having them in a separate crate added unnecessary dependency edges without providing any boundary benefit. Every crate that imports `browseros-types` already needs error types.

**Impact on plan.md:**
- Remove `browseros-error` from crate list
- Add `error.rs` module to `browseros-types/src/`
- Error-related dependencies (`thiserror`) move to `browseros-types`

**Impact on roadmap:**
- Phase 1.1: One fewer crate to create
- No change to total effort (same code, fewer files)

### Change 2: Remove `browseros-di` (Service Container)

**Rationale:** A full DI container is speculative complexity for Phase 1. Rust's type system makes constructor injection natural. The `RuntimeContext` struct provides shared access to all core services without a generic DI framework. Rust also lacks runtime type information for generic resolution, making a DI container either unsafe or cumbersome.

**Replacement:** `RuntimeContext` + `RuntimeBuilder`. The builder registers components, validates dependencies, and produces a fully wired runtime. Components receive `Arc<RuntimeContext>` in their constructor.

**Impact on plan.md:**
- Remove `browseros-di` crate
- Add `runtime.rs` and `builder.rs` to `browseros-core`

**Impact on roadmap:**
- Phase 1.4: One fewer crate to implement
- Slight increase in `browseros-core` effort (builder pattern)

### Change 3: Convert `browseros-resource` to module inside `browseros-lifecycle`

**Rationale:** Full resource management (per-process CPU/GPU/memory quotas, cgroups, IOPS tracking) is needed in Phase 2 when browser runtimes are involved. For Phase 1, only basic memory tracking is needed — and it's naturally owned by the LifecycleManager since it tracks per-component budgets.

**Impact on plan.md:**
- Remove `browseros-resource` crate
- Add `resource.rs` module to `browseros-lifecycle/src/`
- `ResourceTracker` exposed through `LifecycleManager`

### Change 4: Strip cron/periodic scheduling from Scheduler

**Rationale:** BrowserOS is a reactive runtime for browser agent operations. Cron scheduling ("run X at 3am daily") has no use case in a system designed for per-task agent execution. Delayed execution (for retry backoff) and event-triggered execution (for reactive pipelines) cover all Phase 1 requirements.

**Impact on plan.md:**
- Remove cron crate dependency
- Simplify `Scheduler` trait to 3 methods: `schedule_once`, `schedule_on_event`, `cancel`
- Remove `cron.rs` module

**Impact on roadmap:**
- Reduced scheduler effort from Medium to Small

---

## Implementation Order (Final)

### Phase 1.1 — Foundation

| Step | Crate | Key Files | Dependencies |
|------|-------|-----------|-------------|
| 1 | `browseros-types` | `event.rs`, `message.rs`, `error.rs`, `identifiers.rs`, `component.rs`, `module.rs`, `value.rs`, `clock.rs` | None |
| 2 | `browseros-macros` | Proc macros for Event derive | `browseros-types` |
| 3 | `browseros-config` | `config.rs`, `layer.rs`, `source.rs`, `validator.rs` | `browseros-types` |

### Phase 1.2 — Observability

| Step | Crate | Key Files | Dependencies |
|------|-------|-----------|-------------|
| 4 | `browseros-observability` | `logger.rs`, `metrics.rs`, `tracer.rs`, `export.rs` | `browseros-types`, `browseros-config` |

### Phase 1.3 — Runtime Core

| Step | Crate | Key Files | Dependencies |
|------|-------|-----------|-------------|
| 5 | `browseros-lifecycle` | `manager.rs`, `component.rs`, `health.rs`, `resource.rs` | `browseros-types`, `browseros-observability` |
| 6 | `browseros-event` | `bus.rs`, `subscription.rs`, `middleware.rs`, `routing.rs`, `dead_letter.rs` | `browseros-types`, `browseros-config`, `browseros-lifecycle` |
| 7 | `browseros-scheduler` | `scheduler.rs`, `task.rs`, `trigger.rs` | `browseros-types`, `browseros-event` |
| 8 | `browseros-store` | `event_store.rs`, `state_store.rs`, `snapshot.rs`, `query.rs` | `browseros-types`, `browseros-config`, `browseros-observability` |

### Phase 1.4 — Execution

| Step | Crate | Key Files | Dependencies |
|------|-------|-----------|-------------|
| 9 | `browseros-dag` | `graph.rs`, `engine.rs`, `scheduler.rs`, `state.rs` | `browseros-types`, `browseros-event` |
| 10 | `browseros-plugin` | `plugin.rs`, `registry.rs`, `capability.rs`, `manifest.rs` | `browseros-types`, `browseros-event`, `browseros-lifecycle`, `browseros-dag` |

### Phase 1.5 — Integration

| Step | Crate | Key Files | Dependencies |
|------|-------|-----------|-------------|
| 11 | `browseros-core` | `lib.rs`, `runtime.rs`, `builder.rs` | All above |
| 12 | Integration tests | `event_lifecycle.rs`, `dag_execution.rs`, `plugin_lifecycle.rs`, `state_persistence.rs`, `observability_integration.rs` | All above |

---

## Key Interfaces (Stable)

The following interfaces are frozen and must not change without a new architecture review:

```rust
// browseros-types
pub trait Event { ... }
pub struct EventMetadata { ... }
pub struct MessageEnvelope { ... }
pub struct BrowserOsError { ... }
pub enum ErrorKind { ... }
pub trait Clock { ... }
pub struct CancellationToken { ... }

// browseros-event
pub trait EventBus { ... }
pub trait EventMiddleware { ... }
pub struct SubscriptionHandle { ... }

// browseros-scheduler
pub trait Scheduler { ... }
pub struct ScheduledTask { ... }

// browseros-store
pub trait EventStore { ... }
pub trait StateStore { ... }

// browseros-dag
pub trait DagEngine { ... }
pub struct DagGraph { ... }

// browseros-plugin
pub trait Plugin { ... }
pub trait CapabilityRegistry { ... }
pub struct ComponentManifest { ... }

// browseros-lifecycle
pub trait ManagedComponent { ... }
pub trait LifecycleManager { ... }
pub trait HealthCheck { ... }

// browseros-observability
pub struct Logger { ... }
pub struct MetricsRegistry { ... }
pub struct Tracer { ... }

// browseros-config
pub trait Config { ... }

// browseros-core
pub struct RuntimeContext { ... }
pub struct RuntimeBuilder { ... }
```

---

## Verification Checklist

Before any code is merged:

- [ ] All architectural decisions in `architecture-review.md` are followed
- [ ] All 32 invariants in `architecture-invariants.md` are satisfied
- [ ] No out-of-scope functionality from §Explicitly Out of Scope is included
- [ ] All 11 crate boundaries are respected (no cross-crate internal imports)
- [ ] All public APIs match the Key Interfaces above

---

## Sign-off

| Role | Decision |
|------|----------|
| Principal Software Architect | APPROVED WITH MINOR CHANGES |

Phase 1 implementation may begin.
