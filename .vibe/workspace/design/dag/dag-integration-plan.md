# DAG Engine Integration Plan — browseros-dag

**Status:** DESIGN  
**Date:** 2026-07-13  

---

## 1. Integration Points

| Integration | Component | Type | Priority |
|------------|-----------|------|----------|
| RuntimeContext | browseros-runtime | New field `dag: Arc<DagEngine>` | Required |
| RuntimeBuilder | browseros-runtime | New method `with_dag_engine()` | Required |
| LifecycleManager | browseros-lifecycle | Register `"dag_engine"` component | Required |
| EventBus | browseros-event-bus | Publish DAG lifecycle events | Required |
| Scheduler | browseros-scheduler | Retry timing via `schedule_after()` | Required |
| Metrics | browseros-observability | DAG execution counters | Required |
| Tracer | browseros-observability | Span wrapping per node | Required |
| Logger | browseros-observability | Structured DAG execution logs | Required |
| Config | browseros-config | DagConfig component | Optional |
| Existing types | browseros-types | NodeId, ExecutionId, RetryPolicy | Required |

---

## 2. Dependency Graph

```
browseros-dag
    │
    ├── browseros-types (NodeId, ExecutionId, RetryPolicy, CancellationToken,
    │                    BrowserOsError, Event, EventMetadata, EventCategory)
    ├── browseros-event-bus (EventBus for publishing events)
    └── browseros-scheduler (schedule_after for retry timing)
```

**NOT imported by DAG engine:**
- browseros-runtime (DAG is a component OF runtime, not the reverse)
- browseros-lifecycle (DAG reports TO lifecycle, does not depend on it)
- browseros-observability (DAG receives Logger/Metrics/Tracer via constructor)
- browseros-config (optional, for DagConfig)
- Any browser crate (bridge, cdp, browser, page, dom)

---

## 3. Integration Steps

### Step 1: Create Cargo.toml

```toml
[package]
name = "browseros-dag"
version = "0.1.0"
edition.workspace = true

[dependencies]
browseros-types = { path = "../browseros-types" }
browseros-event-bus = { path = "../browseros-event-bus" }
browseros-scheduler = { path = "../browseros-scheduler" }

[dev-dependencies]
tempfile = "3"
```

### Step 2: Add to Workspace

```toml
# browseros/Cargo.toml
[workspace]
members = [
    # ... existing crates ...
    "browseros-dag",
]
```

### Step 3: Add DagEngine to RuntimeContext

```rust
// browseros-runtime/src/lib.rs
pub struct RuntimeContext {
    // Existing 7 fields
    bus: Arc<EventBus>,
    logger: Arc<Logger>,
    metrics: Arc<MetricsRegistry>,
    tracer: Arc<Tracer>,
    config: Arc<RootConfig>,
    lifecycle: Arc<LifecycleManager>,
    scheduler: Arc<Scheduler>,
    
    // NEW field
    dag: Arc<DagEngine>,
}

impl RuntimeContext {
    // Existing accessor pattern
    pub fn dag(&self) -> &Arc<DagEngine> {
        &self.dag
    }
}
```

### Step 4: Update RuntimeBuilder

```rust
// browseros-runtime/src/lib.rs
impl RuntimeBuilder {
    // Existing methods...
    
    pub fn build(self) -> Result<RuntimeContext, RuntimeInitError> {
        // ... existing initialization ...
        
        let dag = Arc::new(DagEngine::new(
            bus.clone(),
            logger.clone(),
            metrics.clone(),
            tracer.clone(),
        ));
        
        lifecycle.register_component("dag_engine");
        
        Ok(RuntimeContext {
            // ... existing fields ...
            dag,
        })
    }
}
```

### Step 5: Wire Lifecycle

```rust
// During startup:
lifecycle.transition_to("dag_engine", LifecycleState::Running);

// During shutdown:
lifecycle.transition_to("dag_engine", LifecycleState::Stopping);
lifecycle.transition_to("dag_engine", LifecycleState::Stopped);
```

### Step 6: Register DagConfig (Optional)

```rust
// browseros-dag/src/config.rs
#[derive(Debug, Deserialize, Serialize)]
pub struct DagConfig {
    pub max_concurrency: Option<usize>,
    pub default_timeout_ms: Option<u64>,
    pub execution_ttl_secs: Option<u64>,
}

impl Default for DagConfig {
    fn default() -> Self {
        Self {
            max_concurrency: Some(std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4)),
            default_timeout_ms: Some(30_000),
            execution_ttl_secs: Some(3600),
        }
    }
}

impl Config for DagConfig {
    fn namespace() -> &'static str { "dag" }
}
```

---

## 4. File Changes Summary

| File | Change |
|------|--------|
| `browseros/Cargo.toml` | Add `"browseros-dag"` to workspace members |
| `browseros/browseros-dag/Cargo.toml` | **NEW** — crate manifest |
| `browseros/browseros-dag/src/lib.rs` | **NEW** — public re-exports |
| `browseros/browseros-dag/src/engine.rs` | **NEW** — DagEngine |
| `browseros/browseros-dag/src/graph.rs` | **NEW** — DagGraph |
| `browseros/browseros-dag/src/node.rs` | **NEW** — DagNode, NodeState |
| `browseros/browseros-dag/src/error.rs` | **NEW** — DagError |
| `browseros/browseros-dag/src/scheduler.rs` | **NEW** — topological sort |
| `browseros/browseros-dag/src/events.rs` | **NEW** — DAG event types |
| `browseros/browseros-dag/src/executor.rs` | **NEW** — SyncExecutor |
| `browseros/browseros-runtime/src/lib.rs` | Add `dag: Arc<DagEngine>` field + accessor |
| `browseros/browseros-types/Cargo.toml` | (unchanged — NodeId/ExecutionId already exist) |

---

## 5. Zero-Breakage Guarantee

The DAG crate is **additive**: it adds a new component to RuntimeContext without changing any existing API. No existing code breaks. All existing tests pass unchanged.

### What does NOT change:
- Existing public APIs
- Crate dependency relationships
- Existing event types
- RuntimeBuilder construction
- RuntimeContext accessor signatures

### What changes (backward compatible):
- `RuntimeContext` gains a new field (private, accessed via `dag()` method)
- `RuntimeBuilder` gains a new method `with_dag_engine()`
- Workspace gains a new member crate

---

## 6. Integration Test Strategy

| Test | Scope | What It Verifies |
|------|-------|-----------------|
| `dag_works_with_runtime` | Runtime + DAG | RuntimeContext.dag() returns working engine |
| `dag_events_reachable_via_bus` | Runtime + DAG + EventBus | DAG events published on EventBus |
| `dag_lifecycle_integration` | Runtime + DAG + Lifecycle | DAG component registered in lifecycle |
| `dag_metrics_recorded` | Runtime + DAG + Metrics | Metrics counters incremented |

---

## 7. Cargo Dependency Verification

```toml
# browseros-dag/Cargo.toml — NO tokio, NO browser crates
[dependencies]
browseros-types = { path = "../browseros-types" }       # NodeId, ExecutionId, RetryPolicy
browseros-event-bus = { path = "../browseros-event-bus" }  # EventBus
browseros-scheduler = { path = "../browseros-scheduler" }  # schedule_after

# Forbidden dependencies (will break invariants):
# browseros-bridge ✗
# browseros-cdp ✗
# browseros-browser ✗
# browseros-page ✗
# browseros-dom ✗
# tokio ✗
```
