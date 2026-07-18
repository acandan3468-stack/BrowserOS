ARCHIVED

This document has been archived. Its content has been merged into the canonical documentation set (design-decisions.md).
See docs/design-decisions.md for current information.

---

# Design Rationale — browseros-types

## Module-by-Module Analysis

### identifiers.rs

**Why it exists:** Type safety. In a system with 15+ subsystems, passing raw `Uuid` values between components is error-prone — an EventId can be mistaken for a TaskId or CorrelationId. Each newtype wrapper prevents this class of bug at compile time.

**Alternatives considered:**
- Single generic `Id<T>` type with phantom data — rejected because it adds complexity without benefit (you can't `Display` a generic Id cleanly, and serde requires type parameters to be instantiated)
- Raw `Uuid`/`String`/`u64` everywhere — rejected because it sacrifices all type safety
- Proc macro `#[derive(Id)]` — deferred to `browseros-macros` in Phase 2

**Trade-offs:**
- 14 types vs 1 generic: more code, but zero-cost at runtime and compile-time safety
- Macro-generated impls vs manual: harder to debug macro errors, but eliminates boilerplate for 14 types
- UUID v7 (time-sortable) vs v4 (random): v7 enables temporal ordering of event logs without an extra timestamp index

**Permanent decisions:**
- UUID-based IDs must always use `Uuid::now_v7()` as the generation strategy
- All IDs must implement `Display + FromStr + Default + Clone + Copy + Eq + Hash + Serialize + Deserialize`
- String-based IDs are transparent serde (human-readable in JSON)
- UUID-based IDs are transparent serde (as UUID strings)

**Flexible decisions:**
- New ID types can be added freely; removing existing ones is a breaking change
- The macro approach can be replaced by proc macros later as an optimization

---

### value.rs

**Why it exists:** Shared value objects that don't belong in any specific module. `SemVer`, `Priority`, `LogLevel` are used across the entire runtime. Without a shared home, they'd be duplicated or cause circular dependencies.

**Alternatives considered:**
- `SemVer` in a separate `semver` crate — unnecessary; we don't need pre-release/build metadata parsing
- `Priority` as integer 0-3 — rejected because named variants are self-documenting and type-safe
- `ModuleType` as freeform string — rejected; the 7 variants cover all known module categories. New variants can be added without breaking changes if handled with `#[non_exhaustive]` (currently not, which is a minor risk)

**Trade-offs:**
- `SemVer` is a custom struct, not the crates.io `semver` crate — keeps dependency count minimal but lacks pre-release/build-metadata support (not needed)
- `ErrorCode` as String newtype vs enum — String is infinitely extensible; enum would require breaking changes for each new code

**Permanent decisions:**
- `SemVer` must always have `Ord` (enables version comparison across the system)
- `Priority` must always have `PartialOrd` (enables priority queue ordering)

**Flexible decisions:**
- `DeliveryGuarantee` is defined but unused — will be consumed by the Event Bus in Phase 1.3
- `ModuleType` variants may grow as new plugin types emerge (e.g. `Adapter`, `Gateway`)

---

### event.rs

**Why it exists:** The `Event` trait is the contract every event in the system must satisfy. Without it, the Event Bus (Phase 1.3) has no uniform interface to publish, subscribe, or filter events.

**Alternatives considered:**
- Single `Event` enum with all variants — rejected; impossible to extend without modifying core code, violates open-closed principle
- No trait, just `Box<dyn Any>` — rejected; loses `kind()` and `category()` which are essential for routing and filtering
- Category marker traits (`SystemEvent`, `DomainEvent`, etc.) vs a single `category()` method — both exist; the marker traits enable compile-time bounds ("only system events can do X") while the method enables runtime filtering

**Trade-offs:**
- Trait with 3 methods vs 10: minimal interface is easier to implement but harder to extend later
- `kind()` returns `&'static str` vs enum: string is extensible without breaking changes; consumers parse/compare strings

**Permanent decisions:**
- Every event in the system must implement `Event`
- `EventMetadata` must contain `(id, causation_id, correlation_id, source, timestamp, content_type)` — these 6 fields form the audit trail

**Flexible decisions:**
- Marker traits can be added or removed as the event hierarchy evolves
- `EventMetadata::new()` currently calls `Utc::now()` directly — **violates INV-004**. Should accept `timestamp` parameter instead.

---

### message.rs

**Why it exists:** The `MessageEnvelope` is the universal inter-module protocol. Every subsystem communicates through this envelope. Without it, every module pair would need its own serialization format.

**Alternatives considered:**
- Generic type parameter `MessageEnvelope<T>` — rejected because serialization boundaries (e.g. sending between Rust and Python modules) would require type erasure anyway
- Flat fields vs builder pattern — builder prevents construction of invalid envelopes (missing required fields cause panic at build(), not mysterious errors later)
- `ModuleId` in its own module vs in message.rs — **current location is suboptimal** (see audit.md)

**Trade-offs:**
- `payload: Vec<u8>` loses type information — but enables the envelope to carry any serialized format (JSON, protobuf, msgpack) without generic parameters
- Builder panics vs returns Result: panic chosen because missing required fields is a programmer error, not a runtime condition

**Permanent decisions:**
- `MessageEnvelope` is the ONLY inter-module communication format
- `source` and `content_type` are always required

**Flexible decisions:**
- New fields can be added to `MessageEnvelope`; removing fields is breaking
- `DeliveryGuarantee` was designed for the envelope but not included — **may need to be added in Phase 1.3**
- `ModuleId` should probably be moved to its own location to reduce coupling

---

### error.rs

**Why it exists:** Shared error taxonomy. Without `BrowserOsError`, error handling across modules would be inconsistent: some would return `String`, some `thiserror` enums, some `anyhow::Error`. The `ErrorKind` enables uniform retry decisions.

**Alternatives considered:**
- Per-module error enums with `From` impls — standard Rust pattern, but without a shared taxonomy, callers can't uniformly decide "should I retry?"
- `anyhow::Error` everywhere — loses classification (retryable? configuration? resource?)
- No error type (just panics) — rejected; panics don't compose

**Trade-offs:**
- Single error type vs error enum per module: single type is less expressive but enables uniform handling. Per-module subtypes can be added via `ErrorCode` without breaking the single-type API
- `BrowserOsError` owns `String` fields vs references: owned is simpler and works with async lifetimes

**Permanent decisions:**
- `ErrorKind` determines retryability (`Transient = retryable`, everything else = not)
- `ErrorSeverity` determines reportability (Error and Critical are reportable)
- `error_context!` macro captures source location for every error

**Flexible decisions:**
- `RetryPolicy` is not yet consumed by any runtime component — will be used by Scheduler in Phase 1.3
- New typed constructors can be added as common error patterns emerge

---

### clock.rs

**Why it exists:** Time abstraction is the single most important testing enabler. Without `Clock`, every test that involves timeouts, delays, or scheduling must use `tokio::time::sleep`, making tests slow and flaky.

**Alternatives considered:**
- No Clock trait, just `Utc::now()` everywhere — rejected; makes deterministic testing impossible
- Use `tokio::time::Instant` — rejected because browseros-types has zero async/IO dependencies
- `MockClock` with interior mutability vs immutable: interior mutability enables sharing across components

**Trade-offs:**
- `Mutex<DateTime<Utc>>` vs `RwLock` vs atomics: Mutex is fine for infrequent reads/writes in tests
- `SystemClock` is a unit struct — zero overhead at runtime

**Permanent decisions:**
- Every time-dependent component MUST receive `Clock` through `RuntimeContext`
- `MockClock` MUST be available for tests
- `SystemClock::now()` must return `Utc::now()`

**Flexible decisions:**
- `CancellationToken` is currently independent of Clock (doesn't need it). If timeout support is added later, it would need Clock.

---

### component.rs

**Why it exists:** Every runtime component follows the same lifecycle. Without `ComponentState`, lifecycle management would be ad-hoc (booleans, integer states, or unenforced conventions). The state machine prevents illegal transitions.

**Alternatives considered:**
- State as integer constants — rejected; no type safety
- State as simple String — rejected; no transition validation
- State as enum with transition methods only — the `can_transition_to` method enables both the LifecycleManager and tests to validate transitions explicitly

**Trade-offs:**
- 7 states + 11 valid transitions: comprehensive but not overwhelming. Every state is reachable and every transition is meaningful
- Terminal states (`Failed`, `Stopped`) can't transition out — simplifies lifecycle guarantees

**Permanent decisions:**
- The 7 states and 11 transitions in the state machine are frozen
- `can_transition_to` must always validate all 7×7=49 pairs

**Flexible decisions:**
- New `HealthStatus` variants can be added if monitoring requirements grow
- `ComponentManifest` fields may expand as deployment requirements grow

---

### module.rs

**Why it exists:** `ModuleDescriptor` is a thin envelope for the LifecycleManager to track registered modules. It's intentionally minimal — just identity, type, and enabled flag.

**Alternatives considered:**
- No module.rs, inline ModuleDescriptor in lifecycle crate — rejected; lifecycle depends on browseros-types, so the type must be here
- Merge into component.rs — possible but conceptually distinct (component = managed runtime participant, module = descriptor)

**Trade-offs:**
- Very thin (1 struct, 4 fields) — almost unnecessary but serves as a clear extension point

**Permanent decisions:**
- `ModuleDescriptor` must reference `ModuleId` and `ModuleType`

**Flexible decisions:**
- More fields can be added as the lifecycle manager evolves

