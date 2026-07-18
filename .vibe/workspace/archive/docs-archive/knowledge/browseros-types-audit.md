ARCHIVED

This document has been archived. Its content has been merged into the canonical documentation set (design-decisions.md).
See docs/design-decisions.md for current information.

---

# Consistency Audit — browseros-types

## Cross-Module Dependency Graph

```
identifiers.rs ──► (none)
value.rs       ──► (none)
event.rs       ──► identifiers.rs, message.rs (via ModuleId)
message.rs     ──► identifiers.rs, value.rs
error.rs       ──► value.rs
clock.rs       ──► (none)
component.rs   ──► (none)
module.rs      ──► message.rs (via ModuleId), value.rs (via ModuleType)
```

**Current dependency pattern is clean** — no cycles, no deep chains. Maximum chain depth is 2 (identifiers → event → message → ... but message is already the trunk). The `identifiers.rs` and `value.rs` modules serve their intended role as leaf-level foundations.

---

## Issue 1: ModuleId in message.rs (Medium Severity)

**What:** `ModuleId` is defined in `message.rs` but used by `event.rs` (as `EventMetadata.source`) and `module.rs` (as `ModuleDescriptor.id`). This means `event.rs` and `module.rs` must depend on `message.rs` for a type that has nothing to do with messaging.

**Effect:** Hidden coupling — if `message.rs` adds/removes a field or changes the `MessageEnvelope` API, `event.rs` and `module.rs` are unnecessarily affected. If the messaging protocol evolves independently from the module lifecycle, there's tension.

**Also:** `ModuleId` is the only ID type NOT in `identifiers.rs`. Every other ID (14 types) lives there. ModuleId's absence violates the principle of least surprise.

**Fix:** Move `ModuleId` to `identifiers.rs`.

```rust
// In identifiers.rs:
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModuleId(Uuid);

impl ModuleId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl Default for ModuleId { ... }
impl Display for ModuleId { ... }
impl FromStr for ModuleId { ... }
```

Then `message.rs` imports `ModuleId` from `identifiers.rs`, and `event.rs` + `module.rs` can also import from `identifiers.rs` without pulling in the messaging module.

---

## Issue 2: EventMetadata::new() violates INV-004 (Medium Severity)

**What:** Architecture inviolable #4 states: "Time always comes from injectable Clock." However, `EventMetadata::new()` calls `Utc::now()` directly:

```rust
pub fn new(source: ModuleId, content_type: &'static str) -> Self {
    Self {
        id: EventId::new(),
        causation_id: CausationId::new(),  // new causation id (root event)
        correlation_id: CorrelationId::new(),
        source,
        timestamp: Utc::now(),   // ← INV-004 violation
        content_type,
    }
}
```

Similarly, `MessageEnvelopeBuilder::build()` calls `Utc::now()`.

**Effect:** Any test that creates events or messages has non-deterministic timestamps. This makes it impossible to:
- Assert on timestamp values in tests
- Use time-based event ordering in deterministic tests
- Simulate different times (time travel in tests)

**Fix (Option A — Minimal):** Make `timestamp` a parameter:

```rust
pub fn new(source: ModuleId, content_type: &'static str, timestamp: DateTime<Utc>) -> Self {
    Self { timestamp, ... }
}
```

This is the MINOR REFACTOR approach — it doesn't require Clock in the type system, just makes the caller responsible for time.

**Fix (Option B — Full INV-004 compliance):** Accept `&dyn Clock`:

```rust
pub fn new(source: ModuleId, content_type: &'static str, clock: &dyn Clock) -> Self {
    Self { timestamp: clock.now(), ... }
}
```

This is architecturally purest but requires Clock in time-dependent code paths.

**Recommendation:** Option A for simplicity. Full Clock compliance can be added in Phase 1.3 when the Event Bus is built and tests need deterministic time.

---

## Issue 3: MessageEnvelopeBuilder::build() violates INV-004 (Medium Severity)

Same root cause as Issue 2, same fix approach. The `build()` method sets `timestamp: Utc::now()`.

---

## Issue 4: ModuleType lacks #[non_exhaustive] (Low Severity)

**What:** `ModuleType` is an enum with 7 variants. New module types will be added in Phase 2 (e.g. Adapter, Gateway). Without `#[non_exhaustive]`, adding a variant is a breaking change.

**Effect:** Any `match ModuleType { ... }` in user code (or even internal code) that is non-exhaustive will fail to compile when a new variant is added.

**Fix:** Add `#[non_exhaustive]` to `ModuleType`.

---

## Issue 5: Match Completeness Gaps (Low Severity)

I reviewed every `match` block in the crate. All are exhaustive. No gaps found.

---

## Issue 6: CausationId semantics undocumented (Low Severity)

**What:** `CausationId` wraps `Uuid` but semantically represents "the EventId of the event that caused this event." The connection to `EventId` (also a `Uuid` newtype) is implicit — there's no documented relationship or conversion.

**Effect:** New developers won't know that causation_id should be set to some other event's EventId. The `CausationId::new()` constructor hides this semantic: it creates a brand-new UUID, not a reference to a previous event.

**Fix:** Add a `CausationId::from_event_id(event_id: EventId)` constructor:

```rust
impl CausationId {
    pub fn from_event_id(id: EventId) -> Self {
        Self(id.into_inner())  // or id.0
    }
}
```

---

## INV Compliance Summary

| INV # | Description | Status |
|-------|-------------|--------|
| INV-001 | Structured errors, no stringly typed | ✅ — ErrorKind, ErrorSeverity, ErrorCode |
| INV-002 | Retry discipline | ✅ — RetryPolicy defined |
| INV-003 | Config externalized | ✅ — deferred to browseros-config |
| INV-004 | Clock injectable | ❌ — EventMetadata, MessageEnvelope violate |
| INV-005 | No blocking I/O | ✅ — no I/O at all |
| INV-006 | All public items documented | ✅ |
| INV-007 | Serde on all data types | ✅ |
| INV-008 | No panics across boundaries | ✅ — panics only in builder (programmer error) |
| INV-009–011 | (tracing/metrics/observability) | ✅ — deferred to later crates |
| INV-012 | Send + Sync | ✅ |
| INV-013 | Graceful degradation | ✅ — deferred |
| INV-014 | Deterministic tests | ❌ — Utc::now() prevents this |
| INV-015–032 | (not applicable to types crate) | N/A |

---

## Naming Convention Audit

- All ID types follow `XxxId` pattern — consistent ✅
- All error-related types use `ErrorXxx` — consistent ✅
- Module files use snake_case.rs — consistent ✅
- Public methods use snake_case — consistent ✅
- Enum variants use PascalCase — consistent ✅
- Acronyms: `HttpModule` not `HTTPModule` — consistent ✅

All naming is internally consistent and follows Rust conventions (RFC 430).

