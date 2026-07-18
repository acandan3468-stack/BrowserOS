# Session Manager Implementation Plan

> **Design Phase:** Phase 6.2  
> **Status:** Design Freeze Candidate  
> **Estimated Effort:** 10–14 engineering days  

---

## 1. Implementation Phases

### Phase 1: Foundation (Days 1–3)

**Goal:** Define traits, types, and the core state machine. No resource bindings yet.

| Step | Task | Files | Dependencies |
|------|------|-------|-------------|
| 1.1 | Create `browseros-session` crate with Cargo.toml | `Cargo.toml` | — |
| 1.2 | Define `SessionId` (UUID v7 wrapper) | `src/id.rs` | `uuid` (v7 feature) |
| 1.3 | Define `SessionState` enum with all 9 states | `src/state.rs` | 1.2 |
| 1.4 | Implement compile-time transition validation matrix | `src/state.rs` | 1.3 |
| 1.5 | Define `SessionError` enum | `src/error.rs` | — |
| 1.6 | Define `SessionConfig` struct | `src/config.rs` | 1.2 |
| 1.7 | Define `Session` struct with state machine + timers (no resources) | `src/session.rs` | 1.2–1.6 |
| 1.8 | Define `SessionFilter`, `SessionSummary`, `SessionHandle` | `src/session.rs` | 1.7 |
| 1.9 | Define `BrowserPoolProvider` trait | `src/providers/browser_pool.rs` | 1.2 |
| 1.10 | Define `StorageProvider` trait | `src/providers/storage.rs` | 1.2 |
| 1.11 | Define `LlmProvider` trait | `src/providers/llm.rs` | 1.2 |
| 1.12 | Define `Resource`, `ResourceSet`, `ResourceType` | `src/resource.rs` | 1.7 |
| 1.13 | Unit tests for state machine transitions | `tests/state.rs` | 1.4 |
| 1.14 | Unit tests for SessionId generation and ordering | `tests/id.rs` | 1.2 |

**Verification:** All invalid state transitions return `SessionError::InvalidState`. SessionId v7 strings are lexicographically sortable by creation time.

### Phase 2: SessionManager Core (Days 4–6)

**Goal:** Implement SessionManager with session creation, destroy, acquire, release, and query.

| Step | Task | Files | Dependencies |
|------|------|-------|-------------|
| 2.1 | Define `SessionRegistry` (RwLock<HashMap<...>>) | `src/registry.rs` | 1.7 |
| 2.2 | Define `SessionGuard` with Drop impl | `src/session.rs` | 1.7, 2.1 |
| 2.3 | Implement `SessionManager::create()` | `src/manager.rs` | 2.1, 1.9–1.12 |
| 2.4 | Implement `SessionManager::destroy()` | `src/manager.rs` | 2.1, 2.3 |
| 2.5 | Implement `SessionManager::get()`, `list()`, `exists()` | `src/manager.rs` | 2.1 |
| 2.6 | Implement `SessionManager::acquire()` | `src/manager.rs` | 2.1, 2.2 |
| 2.7 | Implement `SessionManager::release()` | `src/manager.rs` | 2.1, 2.6 |
| 2.8 | Add session count limits (per-client and global) | `src/manager.rs` | 2.3 |
| 2.9 | Unit tests for create/destroy/acquire/release cycle | `tests/manager.rs` | 2.3–2.8 |
| 2.10 | Unit tests for session count limits | `tests/limits.rs` | 2.8 |

**Verification:** Session lifecycle test: Created → Starting → Ready → Busy → Ready → Idle → Closing → Closed. All transitions validated.

### Phase 3: Resource Binding (Days 7–9)

**Goal:** Wire resource acquisition/release through providers. Integrate with BrowserPool, Storage, LLM.

| Step | Task | Files | Dependencies |
|------|------|-------|-------------|
| 3.1 | Implement resource binding in SessionInner | `src/resource.rs` | 2.1 |
| 3.2 | Implement resource release in dependency order | `src/resource.rs` | 3.1 |
| 3.3 | Implement BrowserPoolProvider adapter (bridge to browseros-browser) | `src/providers/browser_pool.rs` | 1.9, BrowserPool |
| 3.4 | Implement StorageProvider adapter | `src/providers/storage.rs` | 1.10, StorageManager |
| 3.5 | Implement LlmProvider adapter | `src/providers/llm.rs` | 1.11, LlGateway |
| 3.6 | Add eager resource acquisition during `Starting` phase | `src/session.rs` | 2.3, 3.1–3.5 |
| 3.7 | Add lazy resource acquisition on first `resource()` call | `src/session.rs` | 2.6, 3.1 |
| 3.8 | Add SessionGuard::resource<T>() accessor | `src/session.rs` | 2.2, 3.1 |
| 3.9 | Integration tests with mock providers | `tests/resources.rs` | 3.1–3.8 |

**Verification:** Resource allocation order is correct (reverse of release order). Lazy resources are not allocated until first access. Double resource release is idempotent.

### Phase 4: Timers and GC (Days 10–11)

**Goal:** Implement idle timeout, suspend, garbage collection, and tombstone management.

| Step | Task | Files | Dependencies |
|------|------|-------|-------------|
| 4.1 | Implement idle timer (schedule, cancel, reset) | `src/session.rs` | 2.1, Scheduler |
| 4.2 | Implement suspend timer | `src/session.rs` | 4.1 |
| 4.3 | Implement Idle → Suspended → Closing timer chain | `src/session.rs` | 4.2 |
| 4.4 | Implement `GarbageCollector` background worker | `src/gc.rs` | 2.1 |
| 4.5 | Implement tombstone TTL expiration | `src/gc.rs` | 4.4 |
| 4.6 | Implement `SessionManager::garbage_collect()` | `src/manager.rs` | 4.4, 4.5 |
| 4.7 | Timer tests (idle → suspend → close) | `tests/timers.rs` | 4.1–4.3 |
| 4.8 | GC tests (tombstone cleanup, count limits) | `tests/gc.rs` | 4.4–4.6 |

**Verification:** Session auto-transitions through idle → suspend → close. GC removes tombstones after TTL. GC respects global session limit.

### Phase 5: Observability and Events (Days 12–13)

**Goal:** Event publishing, statistics, health reporting.

| Step | Task | Files | Dependencies |
|------|------|-------|-------------|
| 5.1 | Define all session events (created, starting, ready, busy, idle, suspended, closing, closed, failed, resource_bound, resource_released) | `src/event.rs` | 1.3 |
| 5.2 | Publish events from state transitions (lock-free pattern) | `src/session.rs` | 5.1 |
| 5.3 | Implement `SessionStatistics` with atomic counters | `src/stats.rs` | 1.2 |
| 5.4 | Increment statistics on state transitions | `src/session.rs` | 5.3 |
| 5.5 | Implement `SessionManager::health()` | `src/manager.rs` | 2.1 |
| 5.6 | Implement `SessionManager::statistics()` | `src/manager.rs` | 5.3 |
| 5.7 | Event integration tests | `tests/events.rs` | 5.1–5.6 |

**Verification:** Every state transition produces exactly one event. Events are published after locks are released. Health report reflects accurate session counts.

### Phase 6: Shutdown and Integration (Day 14)

**Goal:** Implement graceful shutdown. Integrate with RuntimeContext.

| Step | Task | Files | Dependencies |
|------|------|-------|-------------|
| 6.1 | Implement `SessionManager::shutdown()` | `src/manager.rs` | 2.1, 2.4 |
| 6.2 | Add shutdown flag check to create() and acquire() | `src/manager.rs` | 6.1 |
| 6.3 | Wire SessionManager into RuntimeContext | `browseros-runtime/src/lib.rs` | 6.1 |
| 6.4 | Add `session_manager` field to RuntimeContext | `browseros-runtime/src/lib.rs` | 6.3 |
| 6.5 | Connect SessionManager to BrowserPool in RuntimeBuilder | `browseros-runtime/src/lib.rs` | 6.4 |
| 6.6 | Integration test: full system startup → session create → work → shutdown | `tests/shutdown.rs` | 6.1–6.5 |

**Verification:** `shutdown()` closes all sessions within timeout. `create()` fails after shutdown starts. BrowserPool receives all browser releases.

---

## 2. Test Strategy

### Unit Tests
- State machine transitions (every allowed and disallowed transition)
- SessionId generation, ordering, parsing
- SessionConfig validation
- Timer scheduling and cancellation
- GC threshold enforcement

### Integration Tests
- Full lifecycle with mock providers
- Concurrent session creation from multiple threads
- Resource leak detection (assert Arcs dropped after close)
- Shutdown with busy sessions (graceful + force)

### Property-Based Tests
- SessionId uniqueness under concurrent generation (1000 threads)
- State machine transitions are deterministic
- Resource release order is always consistent

---

## 3. Dependencies

### New Dependencies for `browseros-session/Cargo.toml`

```toml
[dependencies]
# Workspace
browseros-types = { path = "../browseros-types" }
browseros-event-bus = { path = "../browseros-event-bus" }
browseros-config = { path = "../browseros-config" }

# External
uuid = { version = "1", features = ["v7", "serde"] }
chrono = "0.4"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "1"

[dev-dependencies]
browseros-scheduler = { path = "../browseros-scheduler" }
tokio = { version = "1", features = ["rt", "macros"] }  # for async test helpers
```

### BrowserPool Provider Trait (in browseros-types or browseros-session)

```rust
// Defines the contract between SessionManager and BrowserPool
// Implementation in browseros-browser
pub trait BrowserPoolProvider: Send + Sync {
    fn acquire(&self, session_id: SessionId) -> Result<BrowserLease, PoolError>;
    fn release(&self, lease: BrowserLease) -> Result<(), PoolError>;
    fn health(&self) -> PoolHealthReport;
}
```

---

## 4. RuntimeContext Modifications

```rust
// browseros-runtime/src/lib.rs — additions
pub struct RuntimeContext {
    // existing fields...
    
    // NEW:
    pub session_manager: Arc<SessionManager>,
}

impl RuntimeBuilder {
    // NEW:
    pub fn with_session_manager(mut self, pool: Arc<dyn BrowserPoolProvider>) -> Self {
        let config = self.config.clone().expect("config must be set before session manager");
        let bus = self.bus.clone().expect("event bus must be set before session manager");
        self.session_manager = Some(Arc::new(SessionManager::new(
            SessionManagerConfig::from_config(&config),
            pool,
            bus,
        )));
        self
    }
}
```

---

## 5. BrowserPool Modifications

BrowserPool exists conceptually but may not be fully implemented as described in BROWSER_POOL_DESIGN.md. If `browseros-browser`'s `BrowserManager` is the starting point:

1. Extract `BrowserPoolProvider` trait (or define in browseros-types).
2. Implement trait on `BrowserManager` (or new `BrowserPoolWrapper`).
3. Add `acquire(session_id)` method that finds/launches an idle browser.
4. Add `release(lease)` method that returns browser to idle.
5. Add health check loop.

If `BrowserManager` is close but not exact, wrap it:

```rust
struct BrowserPoolAdapter {
    manager: Arc<BrowserManager>,
    config: BrowserPoolConfig,
    idle_browsers: Mutex<VecDeque<BrowserId>>,
}
impl BrowserPoolProvider for BrowserPoolAdapter { ... }
```

---

## 6. Acceptance Criteria (Implementation)

1. **Compile:** `cargo build` in workspace completes with zero errors and zero warnings.
2. **Lint:** `cargo clippy` produces zero warnings for `browseros-session`.
3. **Format:** `cargo fmt --check` passes.
4. **Tests:** `cargo test -p browseros-session` passes with >90% line coverage.
5. **Docs:** All public API items have doc comments. `cargo doc --no-deps` completes.
6. **No leaks:** Session with one browser, after close, leaves zero references to the browser (Arc::strong_count == 1, owned by pool).
7. **No deadlocks:** Concurrent access from 16 threads for 60s with 100 sessions produces zero deadlocks (verified with test harness).
8. **Deterministic shutdown:** `shutdown(5s, 10s)` completes within 15s regardless of session count.
