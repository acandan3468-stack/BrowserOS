# ADR Index — BrowserOS

**Status:** LIVE  
**Source:** docs/architecture/design-decisions.md  

---

| ID | Title | Status | Affected Crates | Date | Related ADR |
|----|-------|--------|-----------------|------|-------------|
| ADR-001 | Multi-crate workspace | ✅ Implemented | All | 2026-06-29 | — |
| ADR-002 | Zero-dependency browseros-types | ✅ Implemented | browseros-types | 2026-06-29 | ADR-001 |
| ADR-003 | Event Bus ≠ Scheduler | ✅ Implemented | event-bus, scheduler | 2026-06-29 | — |
| ADR-004 | Canonical event hierarchy | 🟠 Partial | types, event-bus | 2026-06-29 | ADR-003 |
| ADR-005 | Message Protocol envelope | ✅ Implemented | types | 2026-06-29 | ADR-004 |
| ADR-006 | Separate Plugin and Capability registries | ❌ Not implemented | (future: plugin) | 2026-06-29 | — |
| ADR-007 | Logger ≠ Metrics | ✅ Implemented | observability | 2026-06-29 | — |
| ADR-008 | LifecycleManager for all components | 🟠 Partial | lifecycle | 2026-06-29 | — |
| ADR-009 | RuntimeContext replaces DI container | ✅ Implemented | runtime | 2026-06-29 | ADR-001 |
| ADR-010 | Resource tracking as module in lifecycle | ❌ Not implemented | lifecycle | 2026-06-29 | ADR-008 |
| ADR-011 | State Store as World State foundation | ❌ Not implemented | storage | 2026-06-29 | — |
| ADR-012 | Observability as first-class crate | 🟡 Mostly Complete | observability | 2026-06-29 | ADR-007 |
| ADR-013 | Scheduler stripped (no cron) | ✅ Implemented | scheduler | 2026-06-29 | ADR-003 |
| ADR-014 | Error system merged into types crate | ✅ Implemented | types | 2026-06-29 | ADR-002 |
| ADR-015 | Bridge pattern over facade | ✅ Implemented | bridge, cdp | 2026-06-30 | — |
| ADR-016 | Synchronous API with async transport | ✅ Implemented | cdp, browser | 2026-06-30 | ADR-015 |
| ADR-017 | Generation-based stale detection (DOM) | ✅ Implemented | dom | 2026-07-01 | — |
| ADR-018 | ElementCollection defaults to Static | ✅ Implemented | dom | 2026-07-01 | ADR-017 |
| ADR-019 | DOM crate has no EventBus dependency | ✅ Implemented | dom | 2026-07-01 | ADR-015 |
| ADR-020 | LocatorStrategy is #[non_exhaustive] | ✅ Implemented | bridge, dom | 2026-06-30 | ADR-015 |
| ADR-021 | ModuleType is #[non_exhaustive] | ✅ Implemented | types | 2026-06-30 | ADR-002 |

---

## Status Legend

| Status | Meaning |
|--------|---------|
| ✅ Implemented | Decision fully implemented in code |
| 🟡 Mostly Complete | Core implementation exists, minor gaps |
| 🟠 Partial | Significant implementation exists, major gaps |
| ❌ Not implemented | Decision documented but not implemented |

---

## Summary

| Status | Count |
|--------|-------|
| ✅ Implemented | 14 |
| 🟡 Mostly Complete | 1 |
| 🟠 Partial | 2 |
| ❌ Not implemented | 4 |
| **Total** | **21** |

---

## Deferred Decisions

| Decision | Deferred To | Reason |
|----------|-------------|--------|
| CDP type generation (PDL-to-Rust) | Phase 3 | Hand-written types sufficient |
| Plugin hooks runtime | Phase 3 | Hook definitions not done |
| Dynamic plugin loading (dlopen) | Phase 3+ | Not needed yet |
| Distributed event bus (NATS/Kafka) | Phase 3+ | In-process sufficient |
| Full resource manager (CPU/GPU) | Phase 2+ | Deferred indefinitely |
| Secrets management | Phase 3+ | Not needed yet |