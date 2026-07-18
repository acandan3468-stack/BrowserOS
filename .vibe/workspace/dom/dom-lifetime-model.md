# DOM Lifetime Model

**Status:** FROZEN — Lifetime model locked. Implementation in progress.

---

## 1. Element Identity

### 1.1 HandleId

Every `ElementHandle` has a `HandleId` — a UUID v7 that is assigned at creation and never changes for the lifetime of the handle. The `HandleId` is the **stable identity** of the handle, NOT the backend element.

```
HandleId = UUID v7 (time-ordered, unique)
```

The `HandleId` survives:
- Page navigation (same page session)
- Element re-rendering (React/Vue update cycles)
- Attribute changes
- Class changes
- Style changes
- DOM tree re-organization (within same document)

The `HandleId` does NOT survive:
- Element removal from the DOM
- Cross-document navigation (new page loads)
- Explicit invalidation

### 1.2 Generation Counter

Each `ElementHandle` carries an `AtomicU64` generation counter. The generation increments when:
- The owning page navigates (incremented synchronously within `navigate()`, NOT via async event callback)
- The element is explicitly invalidated
- The element's backend node ID becomes stale

**IMPORTANT — Navigation Race Elimination:** The generation counter for a frame is incremented **synchronously** as part of the `page.navigate()` or equivalent method, before returning to the consumer. This eliminates the race window between a navigation completing (back-end event arriving) and the consumer issuing a subsequent DOM operation. The increment is NOT triggered by an async event callback — that would create a window where stale handles appear valid.

```rust
struct ElementHandle {
    inner: Arc<dyn ElementPort>,
    handle_id: HandleId,
    generation: Arc<AtomicU64>,     // Shared across clones of the same handle
    known_generation: u64,          // The generation at which this handle was created
    frame_id: FrameId,
    page_id: PageId,
}
```

When a backend operation returns a stale-element error:
1. The handle checks current_generation vs known_generation
2. If current > known, return `DomError::StaleElement`
3. If equal, forward to backend; if backend returns stale, increment generation and return error

### 1.3 Stale Detection Flow

```
Consumer calls element.text_content()
  → ElementHandle checks: known_generation == current_generation?
    → NO  → return DomError::StaleElement { handle_id, generation }
    → YES → forward to ElementPort
      → backend returns BridgeError::ElementStale
        → increment current_generation
        → return DomError::StaleElement { handle_id, generation }
      → backend returns value
        → return value to consumer
```

### 1.4 Re-acquisition

Consumers can re-acquire an element after staleness by:
1. Using a locator to find the element again (fresh handle, new generation)
2. Checking `page.dom_snapshot()` and re-querying
3. The old handle is NOT auto-repaired — explicit re-acquisition is required

---

## 2. Handle Ownership

### 2.1 Cloning

Cloning an `ElementHandle` creates a new handle pointing to the same element:
- Same `HandleId`
- Same `Arc<AtomicU64>` generation counter (shared)
- Same `Arc<dyn ElementPort>` (shared backend connection)

```rust
let h1 = page.query(".button").unwrap().unwrap();
let h2 = h1.clone();

// h1 and h2 share the same generation counter
// A navigation will stale BOTH handles simultaneously
```

### 2.2 Dropping

Dropping an `ElementHandle` does NOT invalidate it. Other clones of the same handle remain valid. There is no RAII-based invalidation — handles are logically independent of backend resources.

### 2.3 Send + Sync

All handle types are `Send + Sync`. Multiple threads can hold handles to the same element. The underlying `Arc<dyn ElementPort>` serializes all backend calls.

---

## 3. Cross-Navigation Behavior

### 3.1 Page Navigation

When a page navigates (new document loaded):
1. All existing `ElementHandle`s bound to that page become stale
2. The page's generation counter is atomically incremented
3. The `CdpSession` (or equivalent) generates a new root NodeId
4. Consumers must re-acquire elements after navigation

The generation increment is triggered synchronously by:
- The `page.navigate()` method (increments before returning to consumer — no async window)
- Frame detachment events (handled synchronously in the frame's integration code)
- Explicit invalidation

### 3.2 Same-Document Navigation

For same-document navigations (hash changes, pushState, replaceState):
- Element handles do NOT become stale
- The DOM tree is unchanged (no new document)
- Handles continue to work normally

### 3.3 Cross-Origin Navigation

Cross-origin navigation invalidates all handles on the page (same as regular navigation). Additionally:
- Handles from the previous origin MUST NOT work with the new origin's DOM
- The bridge backend enforces this by returning stale errors for old node IDs
- Frame handles for cross-origin iframes may become detached

---

## 4. Frame-Aware Identity

### 4.1 Element-to-Frame Binding

Every `ElementHandle` knows its owning `FrameId`. The handle uses this to:
- Route DOM operations through the correct frame's `ElementPort`
- Detect when an element moves to a different frame (e.g., via `document.adoptNode()`)
- Validate that operations on elements in different frames are permitted

### 4.2 Cross-Frame Element Reference

When an element from one frame is passed to an operation in another frame (e.g., input.click on an element in an iframe from the main frame), the handle automatically routes through the correct frame's backend. No consumer action required.

### 4.3 Frame Detachment

When a frame is detached (iframe removed from DOM, frame navigated away):
- All element handles for that frame become stale
- The frame's `generation: Arc<AtomicU64>` counter is incremented
- `FrameHandle` operations check `known_generation == current_generation` before forwarding
- Child frame handles are invalidated recursively

---

## 5. Shadow DOM Identity

### 5.1 Open Shadow Roots

Open shadow roots are traversable. Their elements get `ElementHandle`s with:
- The same `page_id` as the host
- The same `frame_id` as the host
- A `shadow_root_id` marker for disambiguation

### 5.2 Closed Shadow Roots

Closed shadow roots are NOT traversable. The API raises `DomError::ClosedShadowRoot` when attempting to pierce a closed shadow root. The `ShadowRootHandle.is_closed()` method returns `true`.

### 5.3 Shadow Root Lifespan

Shadow root handles are valid as long as:
- The host element is in the DOM
- The host element is not stale
- The shadow root is not detached (closed shadow roots remain attached until the host is removed)

---

## 6. Handle Invalidation Scenarios

| Event | Behavior | Stale Detection |
|-------|----------|----------------|
| Page navigates | All handles on page become stale | Generation increment |
| Frame navigates | All handles in frame become stale | Frame generation increment |
| Element removed from DOM | Element handles become stale | Backend returns ElementStale |
| Element re-rendered (same node ID) | Handles remain valid | Backend returns normally |
| Shadow root detached | Shadow root handles become stale | Backend returns ElementStale |
| Session closed | All session handles become stale | ConnectionClosed error |
| Browser closed | All handles become stale | ConnectionClosed error |

---

## 7. Thread Safety Summary

| Type | Send | Sync | Clone | Backend Call Pattern |
|------|------|------|-------|---------------------|
| `ElementHandle` | Yes | Yes | Yes | Serialized via Arc |
| `NodeHandle` | Yes | Yes | Yes | Serialized via Arc |
| `ShadowRootHandle` | Yes | Yes | Yes | Serialized via Arc |
| `FrameHandle` | Yes | Yes | Yes | Generation check + backend calls via LocatorEngine + FramePort |
| `NodeSnapshot` | Yes | Yes | Yes | No backend (pure data) |
| `DomSnapshot` | Yes | Yes | Yes | No backend (pure data) |
| `MutationBatch` | Yes | Yes | Yes | Applied once |
| `MutationObserver` | Yes | Yes | No | Callback registered |
| `Locator` | Yes | Yes | Yes | Read-only, no backend |
| `LocatorBuilder` | Yes | Yes | Yes | No backend |
| `ClassList` | Yes | Yes | Yes | Serialized via ElementHandle |
| `StyleDeclaration` | Yes | Yes | Yes | Serialized via ElementHandle |
| `ComputedStyle` | Yes | Yes | Yes | Serialized via ElementHandle |
| `Attributes` | Yes | Yes | Yes | Serialized via ElementHandle |
| `TreeWalker` | No | No | No | Mutable state, single-thread |
| `AncestorIterator` | No | No | No | Mutable state, single-thread |
| `DescendantIterator` | No | No | No | Mutable state, single-thread |
