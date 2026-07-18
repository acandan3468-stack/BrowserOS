# Design Decisions — BrowserOS (Canonical)

**Status:** LIVE — Consolidated from .vibe/workspace/knowledge/ and derived-state.json  

---

## Architecture Decisions

### ADR-001: Multi-crate workspace
**Decision:** 14+ crates with one bounded context per crate.  
**Reason:** Compile-time boundary enforcement for a system that will grow to 15+ subsystems.  
**Status:** ✅ Implemented  

### ADR-002: Zero-dependency browseros-types
**Decision:** browseros-types has no internal crate dependencies.  
**Reason:** Prevent circular dependencies. Types crate must be the foundation.  
**Status:** ✅ Implemented  

### ADR-003: Event Bus ≠ Scheduler
**Decision:** Separate crates for event routing vs timing.  
**Reason:** Different scalability requirements. Routing vs timing are separate concerns.  
**Status:** ✅ Implemented (browseros-event-bus + browseros-scheduler)  

### ADR-004: Canonical event hierarchy
**Decision:** System / Domain / Metric / Internal event categories.  
**Reason:** With 100+ event types, taxonomy is essential for filtering and subscription.  
**Status:** 🟠 Partial — Hierarchy defined in types but EventBus subscribes by EventId, not category  

### ADR-005: Message Protocol envelope
**Decision:** `MessageEnvelope` as universal inter-module format.  
**Reason:** Cross-subsystem tracing, correlation IDs, uniform delivery guarantees.  
**Status:** ✅ Implemented  

### ADR-006: Separate Plugin and Capability registries
**Decision:** PluginRegistry handles lifecycle; CapabilityRegistry handles service discovery.  
**Reason:** Lifecycle management is orthogonal to service discovery.  
**Status:** ❌ Not implemented (no plugin system exists)  

### ADR-007: Logger ≠ Metrics
**Decision:** Separate Logger and MetricsRegistry within same crate.  
**Reason:** Different consumers, retention policies, and query patterns.  
**Status:** ✅ Implemented  

### ADR-008: LifecycleManager for all components
**Decision:** Centralized LifecycleManager tracks state for every component.  
**Reason:** Race conditions on shutdown, no graceful degradation, no dependency-aware start order.  
**Status:** 🟠 Partial — LifecycleManager exists but is simplified (no ManagedComponent trait, no health, no resources)  

### ADR-009: RuntimeContext replaces DI container
**Decision:** No DI container. RuntimeContext provides shared Arc references.  
**Reason:** Rust's type system makes full DI speculative. RuntimeContext is simpler and sufficient.  
**Status:** ✅ Implemented  

### ADR-010: Resource tracking as module in lifecycle
**Decision:** Basic memory tracking within LifecycleManager. Full resource manager deferred.  
**Reason:** Phase 1 only needs basic tracking. Full CPU/GPU quotas needed in Phase 2+.  
**Status:** ❌ Not implemented (no resource tracking anywhere)  

### ADR-011: State Store designed as World State foundation
**Decision:** Event sourcing with delta-compressed snapshots.  
**Reason:** Foundation for agent world model and temporal queries.  
**Status:** ❌ Not implemented (storage is a stub)  

### ADR-012: Observability as first-class crate
**Decision:** Logger + Metrics + Tracer in dedicated crate, OTLP-ready.  
**Reason:** All subsystems instrumented from day one.  
**Status:** 🟡 Mostly Complete (in-memory only, no OTLP export)  

### ADR-013: Scheduler stripped (no cron)
**Decision:** Delayed + event-triggered only. No cron or periodic scheduling.  
**Reason:** Cron has no use case in a reactive browser agent runtime.  
**Status:** ✅ Implemented  

### ADR-014: Error system merged into types crate
**Decision:** Error types live in browseros-types, not a separate crate.  
**Reason:** Error types are foundational. Separate crate adds unnecessary dependency edges.  
**Status:** ✅ Implemented  

### ADR-015: Bridge pattern over facade
**Decision:** Pure trait definitions in browseros-bridge, implementations in protocol-specific crates.  
**Reason:** Protocol independence. Multiple backends (CDP, Playwright, WebDriver BiDi) without changing consumer code.  
**Status:** ✅ Implemented  

### ADR-016: Synchronous API with async transport
**Decision:** Bridge trait methods are synchronous. Internal CDP thread handles async transport.  
**Reason:** Consistent with Phase 1 synchronous runtime. Agent model expects synchronous operations.  
**Status:** ✅ Implemented  

### ADR-017: Generation-based stale detection (DOM)
**Decision:** ElementHandle uses `known_generation: u64` for stale detection. No auto-repair.  
**Reason:** AtomicU64 generation shared across clones. Explicit re-acquisition via locator.  
**Status:** ✅ Implemented  

### ADR-018: ElementCollection defaults to Static
**Decision:** Static collections materialize all elements at creation time. Live is opt-in.  
**Reason:** Live collections with per-element backend calls have prohibitive performance.  
**Status:** ✅ Implemented  

### ADR-019: DOM crate has no EventBus dependency
**Decision:** DOM defines event payload types only. Upper layers wire to EventBus.  
**Reason:** Keeps DOM a pure leaf crate with no runtime dependencies.  
**Status:** ✅ Implemented  

### ADR-020: LocatorStrategy is #[non_exhaustive]
**Decision:** AI-assisted locator strategies must remain extensible.  
**Reason:** Active R&D area, new strategies expected.  
**Status:** ✅ Implemented  

### ADR-021: ModuleType is #[non_exhaustive]
**Decision:** Future module type variants must not be breaking changes.  
**Reason:** New plugin types (Adapter, Gateway) expected.  
**Status:** ✅ Implemented  

---

## Deferred Decisions

| Decision | Deferred To | Reason |
|----------|-------------|--------|
| CDP type generation (PDL-to-Rust) | Phase 3 | Hand-written types sufficient for Phase 2 |
| Plugin hooks runtime | Phase 3 | Hook definitions in Phase 2 (not done), runtime deferred |
| Dynamic plugin loading (dlopen) | Phase 3+ | Not needed for core browser automation |
| Distributed event bus (NATS/Kafka) | Phase 3+ | In-process bus sufficient for single-process runtime |
| Full resource manager (CPU/GPU) | Phase 2+ | Basic memory tracking deferred indefinitely |
| Secrets management | Phase 3+ | Not needed for current use cases |