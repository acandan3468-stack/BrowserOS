# DOM Freeze Plan

**Status:** FROZEN — Freeze plan locked. Implementation in progress.

---

## 1. Freeze Classification by API Surface

### 1.1 SAFE TO FREEZE (Stable Design, Minimal Change Expected)

| API | Rationale |
|-----|-----------|
| `ElementHandle` | Core identity model is stable. Stale detection, clone semantics, generation counters are well-understood. |
| `HandleId` | UUID v7 identity is simple and final. No change expected. |
| `NodeHandle` | Enum-based dispatch covers all node types. Adding new variants is backward-compatible. |
| `NodeType` | Mirrors DOM spec. Frozen by W3C. |
| `FrameHandle` | Frame tree hierarchy is well-understood. |
| `ShadowRootHandle` | Open/closed modes are DOM standard. |
| `ShadowRootMode` | Two-value enum. DOM spec frozen. |
| `DomSnapshot` | Snapshot is a pure data structure. Schema is stable. |
| `NodeSnapshot` | Pure data. Adding optional fields is backward-compatible. |
| `BoundingBox` | Four-coordinate rectangle. Universal. |
| `BoxEdges` | Four-edge spacing. Universal. |
| `ElementState` | Six states cover all standard element observability checks. |
| `DomError` | Error variants are comprehensive and model all known failure modes. Marked `#[non_exhaustive]` for forward compatibility. |
| `DomResult` | Type alias. No change expected. |
| `MutationObserver` | Mirrors DOM spec. Well-understood. |
| `MutationRecord` | Mirrors DOM spec. |
| `TreeWalker` | Mirrors DOM spec traversal model. |
| `AncestorIterator` | Linear iterator from node to root. Simple. |
| `DescendantIterator` | Depth-first tree walk. Simple. |
| `Attributes` | CRUD over attribute map. Universal. |
| `ClassList` | DOM classList API is stable. |
| `WhatToShow` | Mirrors DOM NodeFilter constants. |

### 1.2 KEEP FLEXIBLE (Design Reasonably Stable But May Evolve)

| API | Rationale | Expected Change |
|-----|-----------|-----------------|
| `LocatorBuilder` | Fluent builder pattern is stable, but AI-assisted locator methods may be added. | New factory methods for AI locators. |
| `Locator` | Wait/resolve/assert semantics are stable, but timeout handling may evolve. | Additional wait strategies, retry policies. |
| `ElementCollection` | Live vs static semantics are stable, but the interface for lazy iteration may need adjustment. | Indexing/caching optimizations. |
| `NodeCollection` | Same as ElementCollection. | Same. |
| `StyleDeclaration` | Browser CSS property naming differences may require normalization. | Property name mapping/normalization. |
| `ComputedStyle` | Pseudo-element support may need expansion. | Multi-pseudo-element queries. |
| `MutationBatch` | Apply ownership model may need `self` (not `&self`) depending on implementation locking strategy. Backends do NOT support rollback. | May switch from `&self` to `self` for `apply()`. Rollback removed entirely. |
| `MutationObserverOptions` | Options struct may gain new fields for backend-specific features. | Additional filter options. |
| `Dataset` | data-* attribute convention is standard, but value encoding may vary. | Encoding normalization. |
| `AriaAttributes` | ARIA spec evolves. New attributes added periodically. | New named accessors. |
| `Snapshot diffing` | Diff algorithm improvements over time. | More efficient diffing. |

### 1.3 EXPERIMENTAL (Design May Change Significantly)

| API | Rationale |
|-----|-----------|
| `AiLocator` | AI-assisted locator strategies are an active area. The interface format (natural language, confidence, fallback) is not yet settled. |
| `Live NodeCollection/ElementCollection` | Live collections that re-query on each access may have unpredictable performance characteristics. May be replaced with an explicit `refresh()` pattern. |
| `In-memory CSS selector matching on snapshots` | `NodeSnapshot.find_by_selector()` requires a CSS selector engine in Rust. If no suitable library is available, this may be removed or delegated to the backend. |
| `Cross-frame element move` | `Mutation::MoveNode` across frames has complex semantics (different document, different backend). May be restricted to same-frame moves initially. |

---

## 2. Crate Dependency Freeze

| Dependency | Status | Notes |
|------------|--------|-------|
| `browseros-types` | FROZEN | No changes needed. All types exist. |
| `browseros-bridge` | FROZEN | `ElementPort`, `LocatorEngine`, `LocatorStrategy` are frozen. |
| `serde` | FROZEN | Serialization for snapshots and event payloads. |
| `serde_json` | FROZEN | JSON value handling. |
| `thiserror` | FROZEN | Error type derive. |
| `uuid` | START | Only if HandleId generation is needed at DOM level (preferred: `HandleId` wraps `browseros-types` ID type). |

---

## 3. What We Gain by Freezing Now

1. **Implementation can proceed with confidence.** Every public API is classified as stable, flexible, or experimental. Implementers know which parts are frozen and which may change.

2. **Cross-crate contracts are locked.** `browseros-input` knows it receives `ElementHandle`. `browseros-page` knows it supplies frames to `FrameHandle`. No cascading changes.

3. **Testing strategy is clear.** Stable APIs get exhaustive unit tests. Flexible APIs get contract tests. Experimental APIs get integration tests.

4. **Documentation is authorable.** Doc comments written now won't need rewriting for stable APIs.

5. **Plugin system integration is predictable.** Plugins consume `ElementHandle` through stable bridge traits. The plugin API freezes alongside the DOM API.

---

## 4. Freeze Exceptions

The following MAY change after freeze without a second architecture review:

- **EXPERIMENTAL APIs** (listed above) may change, be removed, or be replaced
- `LocatorBuilder` may gain new factory methods (adding is not breaking)
- `ElementCollection` may gain new convenience methods (adding is not breaking)
- Error messages in `DomError` variants may be refined for clarity
- Performance optimization may change internal APIs (not public)

Everything else requires a new architecture review before changing.
