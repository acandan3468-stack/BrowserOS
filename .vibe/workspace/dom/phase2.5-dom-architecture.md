# Phase 2.5 — DOM Architecture

**Status:** FROZEN — Architecture locked. Implementation in progress.  
**Goal:** Define the complete architecture of `browseros-dom` before any code is written.  
**Constraints:** No implementation code. No Rust code generation. Architecture freeze only.

---

## 1. Responsibilities

### 1.1 What browseros-dom OWNS

- **Element Identity Model** — stable element handles with stale detection, cross-frame identity, cross-navigation identity
- **DOM Traversal** — tree walking (parent, children, siblings, ancestors, descendants, closest), iterator support, lazy evaluation
- **DOM Snapshot** — materialized tree snapshots with query capabilities (find by id, tag, text, attribute)
- **Locator Integration** — strategy builder, high-level locate/wait operations, AI-assisted locator bridge
- **Shadow DOM** — open/closed shadow root discovery, traversal, piercing rules
- **Frame Model** — frame-aware element operations, element migration across frames, iframe handling
- **DOM Queries** — querySelector, querySelectorAll, XPath evaluation, text search, element matching
- **Node Collections** — live and static NodeList/ElementCollection with filtering, indexing, iteration
- **Element State** — visibility, stability, enabled/disabled, checked/selected, editable, stable
- **Computed Style** — CSS computed style access, pseudo-element style queries
- **Style Declarations** — inline style read/write, class list manipulation
- **Attribute API** — typed attribute access (get, set, has, remove, enumerate), data attributes, ARIA attributes
- **DOM Mutation** — attribute updates, text content changes, innerHTML/outerHTML, DOM insertion/removal, cloning, adoption
- **Batch Mutations** — transaction-style mutation groups with atomic apply semantics (no rollback; backends don't guarantee transactional DOM)
- **Mutation Observation** — observe DOM changes via callback (wraps the backend's mutation event stream)
- **Node Identity** — NodeId allocation, backend-node-id mapping, remote-object-id lifecycle

### 1.2 What browseros-dom MUST NOT own

- **CDP protocol types** — zero CDP types. All protocol interaction goes through `browseros-bridge` traits
- **Browser lifecycle** — launch, close, kill, process management. That is `browseros-browser`
- **Session management** — session creation, detach, target management. That is `browseros-browser`
- **Page navigation** — navigate, reload, goBack, goForward. That is `browseros-page`
- **Network operations** — request interception, cache, cookies. That is `browseros-network`
- **Input simulation** — click, type, press, scroll. That is `browseros-input`
- **Storage** — localStorage, sessionStorage, cookies. That is `browseros-storage`
- **Dialogs** — alert/confirm/prompt handling. That is `browseros-page`
- **Downloads** — download monitoring. That is `browseros-download`
- **Artifact storage** — file persistence. That is `browseros-artifact`
- **Plugin system** — plugin lifecycle, capability registry. That is `browseros-plugin`
- **EventBus** — no direct EventBus dependency. DOM events are emitted through callbacks that upper layers wire to EventBus
- **Transport** — no WebSocket, no pipe, no connection management
- **Async runtime** — all API is synchronous. No async/future
- **Accessibility tree** — not exposed by the bridge yet. Deferred to a future phase when `ElementPort` extends to support accessibility traversal.

### 1.3 Boundary Clarifications

| Neighboring Crate | What browseros-dom consumes | What browseros-dom provides |
|---|---|---|
| `browseros-bridge` | `ElementPort`, `LocatorEngine`, `LocatorStrategy`, `NodeInfo`, `BoxModel`, `ElementState`, `NodeType` | — (bridge is the contract) |
| `browseros-page` | — (sibling) | `DomSnapshot` for page content extraction |
| `browseros-browser` | — (sibling, higher-level) | `ElementHandle`, `LocatorBuilder` for PageHandle consumers |
| `browseros-cdp` | — (isolated by bridge) | — (no direct interaction) |
| `browseros-network` | — (sibling) | Element identity for request attribution |
| `browseros-input` | `ElementHandle` for input targets | Stable element identity + hit point calculation |
| `browseros-storage` | — (sibling) | — (no interaction) |
| `browseros-artifact` | — (sibling) | DOM snapshot storage |
| `browseros-plugin` | `ElementHandle` for plugin DOM access | DOM observation callbacks |
| `browseros-runtime` | Composition | Public API integration |

---

## 2. Module Layout

```
browseros-dom/src/
├── lib.rs                  # Crate root, public re-exports
├── error.rs                # DomError (#[non_exhaustive]) + DomResult
├── element.rs              # ElementHandle (stable handle with stale detection)
├── node.rs                 # NodeHandle, TextNode, CommentNode, DocumentFragment
├── snapshot.rs             # DomSnapshot, NodeSnapshot, snapshot query methods
├── traversal.rs            # TreeWalker, NodeIterator, ancestor/descendant iterators
├── query.rs                # querySelector, querySelectorAll, XPath query, text query
├── locator.rs              # LocatorBuilder, locator composition, AI locator bridge
├── selector.rs             # CSS selector matching (is(), matches()), pseudo-class support
├── shadow.rs               # ShadowRootHandle, open/closed shadow root traversal
├── frame.rs                # FrameHandle, cross-frame element operations
├── mutation.rs             # MutationBatch, mutation operations, transaction semantics
├── mutation_observer.rs    # MutationObserver, mutation callback registration
├── style.rs                # ComputedStyle, StyleDeclaration, ClassList
├── attributes.rs           # Attributes, typed attribute access, dataset, aria
├── collection.rs           # NodeCollection, ElementCollection, NodeList, ElementList
├── state.rs                # ElementState evaluation (visible, stable, enabled, etc.)
├── events.rs               # DomEvent types (emitted by upper layers via EventBus)
└── id.rs                   # HandleId (re-exported from browseros-types), identity tracking
```

---

## 3. Dependency Graph

```
browseros-types (FROZEN)
  ↑
browseros-bridge (FROZEN)
  ↑
browseros-dom (NEW)
  ├── deps: browseros-types, browseros-bridge
  ├── serde, serde_json, thiserror
  └── NO: browseros-cdp, browseros-page, browseros-browser, EventBus, observability
```

---

## 4. Design Principles

1. **Handle stability over backend identity.** The `ElementHandle` wraps a bridge `ElementPort` but adds a layer of identity that survives navigation (within the same page session). Backend node IDs change on navigation; handle IDs don't.

2. **Explicit staleness.** Every handle carries a generation counter. Any DOM operation on a stale handle returns `DomError::StaleElement`. There is no silent auto-reconnection.

3. **Lazy by default, eager on demand.** Traversal operations return lazy iterators. Snapshot materialization is explicit. Query results are lazy references backed by the handle.

4. **Frame-aware transparency.** Element operations across frames look the same to the consumer. The handle knows which frame it belongs to and routes operations through the correct `ElementPort`.

5. **No implicit backend calls.** Every DOM operation that hits the backend is explicit. `text_content()`, `inner_html()`, `get_attribute()` go to the backend each time. Caching is opt-in via `snapshot()`.

6. **Transaction semantics for mutations.** Multiple DOM mutations can be grouped into a `MutationBatch` with atomic-apply semantics: all mutations are applied in order; if any fails, already-applied mutations are NOT rolled back (no browser backend guarantees transactional DOM). Consumers should check results and handle partial failure.

7. **Locator strategies are composable.** Nested, And, Or, Child, Nth variants allow building complex locators from simple ones without string concatenation.

8. **No EventBus dependency.** The crate defines DOM event types (payload structs only). Upper layers (runtime, plugin) wire callbacks to the EventBus. This keeps `browseros-dom` a leaf crate.

9. **Bridge trait escape hatches are crate-internal.** Methods like `backend_element_port()` are `pub(crate)` — external crates use the DOM API exclusively. No bypassing the DOM layer.

10. **CSS property names are strings.** ComputedStyle and StyleDeclaration use `&str` property names because:
    - The W3C CSS spec defines property names as strings
    - 500+ properties make an enum impractical
    - Custom properties (`--my-prop`) cannot be enum variants
    - Different backends may normalize differently
