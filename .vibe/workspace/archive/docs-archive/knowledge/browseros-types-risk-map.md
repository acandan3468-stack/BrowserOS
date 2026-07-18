ARCHIVED

This document has been archived. Its content has been merged into the canonical documentation set (design-decisions.md).
See docs/design-decisions.md for current information.

---

# Risk Map — browseros-types

## Immediate Risks (Must Fix Before Phase 1.2)

### R-01: EventMetadata/Message timestamps not injectable (Medium)
- **Root cause:** `Utc::now()` called directly (INV-004 violation)
- **Impact when realized:** Phase 1.3 (Event Bus) tests will be non-deterministic. Timeout tests will be slow (real wall clock). Event replay tests cannot verify timestamp ordering.
- **Cost if unfixed:** Growing — each crate that uses EventMetadata inherits the problem
- **Cost to fix now:** Low — single parameter change in 2 constructor functions
- **Trigger:** First test that needs to assert on timestamp ordering
- **Action:** Apply Option A (timestamp parameter) before Phase 1.2. Full Clock compliance in Phase 1.3.

### R-02: ModuleId in wrong module (Medium)
- **Root cause:** Defined in `message.rs`, used by `event.rs` + `module.rs`
- **Impact when realized:** A future refactor of `message.rs` (e.g., changing `ModuleId` to string-based) would have unintended effects on event metadata and module descriptors. Or, a change to `ModuleId` type (e.g., adding fields) would force message.rs to own domain concepts.
- **Cost if unfixed:** Low now, grows as more modules depend on ModuleId
- **Cost to fix now:** Low — pure move, no logic change
- **Trigger:** Any future change to ModuleId or message.rs
- **Action:** Move to `identifiers.rs`

---

## Short-Term Risks (Phase 1.2 – 1.3)

### R-03: #[non_exhaustive] missing on ModuleType (Low)
- **Root cause:** New module types will be added in Phase 2
- **Impact when realized:** Adding `Adapter` or `Gateway` to `ModuleType` will be a semver-breaking change; downstream crates will fail to compile
- **Cost if unfixed:** Low — easy to add when breaking change is acceptable, but it's better to add before other crates start matching on ModuleType
- **Action:** Add `#[non_exhaustive]` now

### R-04: No proc macros — boilerplate scales linearly (Low)
- **Root cause:** All 14 ID types have manually implemented `Display`, `FromStr`, `Default`. Every new ID adds ~15 lines of boilerplate.
- **Impact when realized:** Phase 2 may add 10+ more ID types. Developer fatigue → bugs in boilerplate
- **Cost if unfixed:** Low, predictable (linear). Each new ID is ~5 minutes of copy-paste-editing
- **Action:** Don't fix yet — revisit in Phase 2 when macros are available

### R-05: ModuleType and ModuleDescriptor have overlapping identity info (Low)
- **Root cause:** `ModuleType` is in `value.rs`, `ModuleDescriptor` is in `module.rs`. Both describe module identity at different levels. There's no enforced consistency between them.
- **Impact when realized:** A module could be registered as `ModuleType::Plugin` but have a `ModuleDescriptor` that doesn't match. Not an issue yet (no validation code).
- **Cost if unfixed:** Zero now, moderate in Phase 1.3 when LifecycleManager validates registrations
- **Action:** Document the relationship (`ModuleType` is a classification, `ModuleDescriptor` is a registration entry). Add cross-validation when LifecycleManager is built.

---

## Medium-Term Risks (Phase 1.4 – 1.6)

### R-06: DeliveryGuarantee defined but unused (Low)
- **Root cause:** `DeliveryGuarantee` enum is in `value.rs` but no consumer exists yet
- **Impact when realized:** The Event Bus (Phase 1.3) may need a different delivery model than what was defined. Unused types risk being wrong when finally consumed.
- **Cost if unfixed:** Very low — easy to change the enum or add a new variant
- **Action:** No action — forward-looking type. Validate when Event Bus is built.

### R-07: error_context! macro could produce large error chains (Low)
- **Root cause:** Each `error_context!` call captures `file!(), line!(), column!()` plus a message string. Chained errors (e.g., Module A → Module B → Module C) store 3+ locations + 3+ messages in memory.
- **Impact when realized:** In error-heavy paths (e.g., network timeout while retrying while config is broken), the error accumulator holds 10+ entries. Memory is ~200 bytes per entry — negligible.
- **Cost if unfixed:** Negligible — Rust error reporting in production should use structured logging, not error propagation through the return path
- **Action:** No action. Monitor if error chains grow beyond 10 nested contexts in practice.

### R-08: No type-safe conversions between ID types (Low)
- **Root cause:** No `From<EventId> for CausationId` or similar. The relationship between IDs is semantic and implicit.
- **Impact when realized:** Developer must know that `causation_id` should come from `event_id`. Without a type-safe conversion, bugs where the wrong ID is passed are possible.
- **Cost if unfixed:** Low — typically caught in code review
- **Action:** Add `CausationId::from_event_id(event_id)` as a bridge (see audit.md Issue 6)

---

## Long-Term Risks (Post-Phase 1)

### R-09: No persister abstraction in types (Informational)
- **Root cause:** `browseros-types` has no PERSISTER abstraction (events are stored and replayed).
- **Impact when realized:** Every store implementation must handle serialization/deserialization of events independently.
- **Cost if unfixed:** Unknown — depends on store design in Phase 1.3
- **Action:** Accept — this is correct separation of concerns. Types owns the data models, Store owns persistence.

### R-10: No AsyncTrait in types (Informational)
- **Root cause:** `browseros-types` has zero async dependencies. All traits are synchronous.
- **Impact when realized:** Phase 1.3 introduces tokio. The Clock trait must remain sync (INV-004 says "no blocking", not "no sync in types"). But if any types trait needs async, it would require a dependency on tokio in browseros-types.
- **Cost if unfixed:** Zero — this is intentional. Types should never need async.
- **Action:** Keep browseros-types sync. Future async traits go in their respective crates (browseros-event, browseros-scheduler, etc.)

---

## Risk Matrix

| ID | Severity | Likelihood | Detectability | Priority | Action Window |
|----|----------|------------|---------------|----------|---------------|
| R-01 | High | Certain | High (test failures) | **Critical** | Before Phase 1.2 |
| R-02 | Medium | Low | Low (silent coupling) | **High** | Before Phase 1.2 |
| R-03 | Low | Medium (when new types added) | High (compiler error) | Medium | Phase 1.2 |
| R-04 | Low | Medium | Low | Low | Phase 2 |
| R-05 | Low | Low | Medium | Low | Phase 1.3 |
| R-06 | Low | Low | Low | Low | Phase 1.3 |
| R-07 | Low | Very Low | Low | Low | Never (accept) |
| R-08 | Low | Low | Medium | Low | Phase 1.2 |
| R-09 | None | N/A | N/A | Informational | Accept |
| R-10 | None | N/A | N/A | Informational | Accept |

---

## Summary

**Fix before Phase 1.2 (MINOR REFACTOR):**
- R-01: Make timestamp a parameter in EventMetadata + MessageEnvelope (2 functions)
- R-02: Move ModuleId to identifiers.rs
- R-03: Add `#[non_exhaustive]` to ModuleType
- R-08: Add `CausationId::from_event_id()`

**Accept:**
- R-04 (defer to Phase 2)
- R-05 (document, validate in Phase 1.3)
- R-06 (validate in Phase 1.3)
- R-07 (accept)
- R-09 (accept)
- R-10 (accept)

