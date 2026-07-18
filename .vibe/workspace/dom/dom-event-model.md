# DOM Event Model

**Status:** FROZEN — Event model locked. Implementation in progress.

---

## 1. Design

- `browseros-dom` defines **event payload types only**. It does NOT depend on `EventBus`.
- Upper layers (runtime, plugin system) create EventBus event variants from these payloads.
- Payloads are `Clone + Send + Sync` plain data structures (no trait objects, no backreferences).
- Every event carries the source `PageId` and optional `FrameId`.
- Events are emitted by the DOM-wiring layer (between `browseros-dom` and `browseros-browser`) when the DOM crate's operations produce state changes.

---

## 2. Event Catalog

### 2.1 Element Lifecycle

| Event | Payload | Trigger |
|-------|---------|---------|
| `ElementCreated` | `page_id, frame_id, handle_id, tag_name, node_type` | New element handle created via query |
| `ElementRemoved` | `page_id, frame_id, handle_id, tag_name` | Element removed from DOM (via mutation or external action) |
| `ElementStale` | `page_id, frame_id, handle_id, generation` | Element becomes stale |
| `ElementReacquired` | `page_id, frame_id, old_handle_id, new_handle_id` | Same element located again after staleness |

### 2.2 Attribute & Property Changes

| Event | Payload | Trigger |
|-------|---------|---------|
| `AttributeChanged` | `page_id, frame_id, handle_id, name, old_value, new_value` | Element attribute set/removed |
| `AttributeRemoved` (REMOVED) | — | Redundant — `AttributeChanged { new_value: None }` covers removal |
| `TextContentChanged` | `page_id, frame_id, handle_id, old_text, new_text` | Element text content changed |
| `InnerHtmlChanged` | `page_id, frame_id, handle_id` | Element innerHTML replaced |
| `ClassListChanged` | `page_id, frame_id, handle_id, added: Vec<String>, removed: Vec<String>` | Element classes toggled/added/removed |
| `StyleChanged` | `page_id, frame_id, handle_id, property, old_value, new_value` | Inline style property changed |

### 2.3 DOM Structure Changes

| Event | Payload | Trigger |
|-------|---------|---------|
| `ChildNodeInserted` | `page_id, frame_id, parent_handle_id, child_handle_id, child_tag_name` | Child element inserted |
| `ChildNodeRemoved` | `page_id, frame_id, parent_handle_id, child_handle_id, child_tag_name` | Child element removed |
| `SubtreeModified` | `page_id, frame_id, root_handle_id` | Deep DOM mutation in subtree |
| `NodeReplaced` | `page_id, frame_id, parent_handle_id, old_handle_id, new_handle_id` | Child element replaced |

### 2.4 Navigation & Frame Changes

| Event | Payload | Trigger |
|-------|---------|---------|
| `FrameAttached` | `page_id, frame_id, parent_frame_id: Option<FrameId>, url` | New frame attached to page |
| `FrameDetached` | `page_id, frame_id, parent_frame_id: Option<FrameId>` | Frame removed from page |
| `FrameNavigated` | `page_id, frame_id, url` | Frame navigated to new URL |
| `FrameClearedForNavigation` | `page_id, frame_id` | Frame about to navigate (DOM cleared) |
| `DocumentUpdated` | `page_id, frame_id` | Document updated (DOM content changed) |

### 2.5 Focus & Selection

| Event | Payload | Trigger |
|-------|---------|---------|
| `ElementFocused` | `page_id, frame_id, handle_id, tag_name` | Element received focus |
| `ElementBlurred` | `page_id, frame_id, handle_id, tag_name` | Element lost focus |
| `SelectionChanged` | `page_id, frame_id, selected_text, handle_id: Option<HandleId>` | Text selection changed |
| `ScrollPositionChanged` | `page_id, frame_id, scroll_x, scroll_y` | Page/frame scrolled |

### 2.6 Shadow DOM

| Event | Payload | Trigger |
|-------|---------|---------|
| `ShadowRootAttached` | `page_id, frame_id, host_handle_id, shadow_root_mode` | Shadow root attached to element |
| `ShadowRootDetached` | `page_id, frame_id, host_handle_id` | Shadow root removed |
| `ShadowDomSlotChanged` | `page_id, frame_id, host_handle_id, slot_name, assigned_nodes` | Slotted content changed |

### 2.7 Mutation Observation

| Event | Payload | Trigger |
|-------|---------|---------|
| `MutationObserved` | `page_id, frame_id, target_handle_id, mutation_type, records: Vec<MutationRecord>` | Registered MutationObserver fired |

**Note:** `MutationBatchStarted` and `MutationBatchFinished` were removed in revision — batch boundaries are an implementation strategy, not a domain concept. Consumers observe individual mutation records only.

### 2.8 DOM Snapshot

| Event | Payload | Trigger |
|-------|---------|---------|
| `SnapshotCreated` | `page_id, frame_id, node_count, depth` | DOM snapshot taken |
| `SnapshotStored` | `page_id, artifact_id: ArtifactId` | Snapshot persisted to artifact store |

### 2.9 Error Events

| Event | Payload | Trigger |
|-------|---------|---------|
| `DomOperationFailed` | `page_id, frame_id, handle_id: Option<HandleId>, operation, error` | DOM operation failed |
| `DomTimeout` | `page_id, frame_id, operation: DomOperation, timeout: Duration` | DOM operation timed out |

---

## 3. Payload Structures

```rust
struct ElementCreatedPayload {
    page_id: PageId,
    frame_id: FrameId,
    handle_id: HandleId,
    tag_name: String,
    node_type: NodeType,
}

struct ElementRemovedPayload {
    page_id: PageId,
    frame_id: FrameId,
    handle_id: HandleId,
    tag_name: String,
}

struct ElementStalePayload {
    page_id: PageId,
    frame_id: FrameId,
    handle_id: HandleId,
    generation: u64,
}

struct AttributeChangedPayload {
    page_id: PageId,
    frame_id: FrameId,
    handle_id: HandleId,
    name: String,
    old_value: Option<String>,
    new_value: Option<String>,
}

struct ElementReacquiredPayload {
    page_id: PageId,
    frame_id: FrameId,
    old_handle_id: HandleId,
    new_handle_id: HandleId,
}

struct ChildNodeInsertedPayload {
    page_id: PageId,
    frame_id: FrameId,
    parent_handle_id: HandleId,
    child_handle_id: HandleId,
    child_tag_name: String,
}

struct FrameAttachedPayload {
    page_id: PageId,
    frame_id: FrameId,
    parent_frame_id: Option<FrameId>,
    url: String,
}

/// Type-safe DOM operation categories for error and timeout events.
enum DomOperation {
    QuerySelector,
    QuerySelectorAll,
    XPathEvaluation,
    TextQuery,
    AttributeGet,
    AttributeSet,
    AttributeRemove,
    TextContent,
    InnerHtml,
    OuterHtml,
    Snapshot,
    ComputeStyle,
    BoundingBox,
    ScrollIntoView,
    Focus,
    MutationApply,
    TreeWalk,
    Evaluate,
    Locate,
}

struct DomOperationFailedPayload {
    page_id: PageId,
    frame_id: FrameId,
    handle_id: Option<HandleId>,
    operation: DomOperation,
    error: String,
}
```

---

## 4. Wiring to EventBus

The DOM crate defines only the payload types above. Wiring to EventBus is the responsibility of the integration layer:

```rust
// In browseros-runtime or a dom-bridge module:
fn wire_dom_events(page: &PageHandle, bus: &EventBus) {
    let observer = MutationObserver::new(move |records| {
        for record in records {
            bus.emit(DomEvent::MutationObserved(MutationObservedPayload {
                page_id: page.id(),
                frame_id: frame_id,
                target_handle_id: record.target,
                mutation_type: record.type,
                records: records.clone(),
            }));
        }
    }, MutationObserverOptions::all());
    observer.observe(page.root_element());
}
```

This keeps `browseros-dom` a pure data crate with no runtime dependencies while allowing full event observability.
