# DOM API Design

**Status:** FROZEN — API locked. Implementation in progress.

---

## 1. Core Types

### 1.1 ElementHandle

**Purpose:** Stable handle to a DOM element. Primary user-facing type for element interaction. Wraps a bridge `Box<dyn ElementPort>` and adds identity tracking, stale detection, frame awareness, and high-level operations.

**Ownership:** The consumer owns the handle. Cloning creates a new handle pointing to the same element (same HandleId, same generation). The handle is `Clone + Send + Sync`.

**Mutability:** All methods take `&self`. The handle is logically immutable — the underlying element's DOM state may change via the browser, but the handle itself doesn't mutate. State-modifying operations (setAttribute, set innerHTML) are `&self`.

**Thread Safety:** `Send + Sync`. Multiple threads can hold handles to the same element. All backend operations are serialized through the bridge's `ElementPort` which is `Send + Sync`.

**Lifetime Expectations:** Handles live until dropped. A handle becomes stale after navigation, DOM removal, or explicit invalidation. Stale handles still exist (they're not invalidated by dropping) but all operations return `DomError::StaleElement`.

```
ElementHandle {
    inner: Arc<dyn ElementPort>,           // Bridge to backend
    locator_engine: Arc<dyn LocatorEngine>, // For locator resolution
    handle_id: HandleId,                    // Stable identity (UUID v7)
    generation: Arc<AtomicU64>,             // Incremented on cross-navigation
    known_generation: u64,                  // Captured at creation for stale detection
    frame_id: FrameId,                      // Owning frame
    page_id: PageId,                        // Owning page
    node_info: OnceLock<Option<NodeInfo>>,  // Cached snapshot (optional)
}
```

**Methods:**
- `id()` -> `HandleId` — stable identity (UUID v7)
- `backend_id()` -> `ElementId` — bridge-level element identifier (backend-dependent, may change on re-render)
- `tag_name()` -> `DomResult<String>`
- `text_content()` -> `DomResult<String>`
- `inner_html()` -> `DomResult<String>`
- `outer_html()` -> `DomResult<String>`
- `attributes()` -> `DomResult<Attributes>`
- `attribute(name)` -> `DomResult<Option<String>>`
- `set_attribute(name, value)` -> `DomResult<()>`
- `remove_attribute(name)` -> `DomResult<()>`
- `has_attribute(name)` -> `DomResult<bool>`
- `class_list()` -> `DomResult<ClassList>`
- `style()` -> `DomResult<StyleDeclaration>`
- `computed_style(pseudo_el)` -> `DomResult<ComputedStyle>`
- `bounding_box()` -> `DomResult<Option<BoundingBox>>`
- `is_visible()` -> `DomResult<bool>`
- `is_enabled()` -> `DomResult<bool>`
- `is_checked()` -> `DomResult<bool>`
- `is_selected()` -> `DomResult<bool>`
- `is_stable()` -> `DomResult<bool>`
- `state()` -> `DomResult<ElementState>`
- `focus()` -> `DomResult<()>`
- `scroll_into_view()` -> `DomResult<()>`
- `click_point()` -> `DomResult<Point>` — center of element for input
- `snapshot()` -> `DomResult<NodeSnapshot>` — materialize full node info
- `matches(selector)` -> `DomResult<bool>` — CSS selector match
- `query(selector)` -> `DomResult<Option<ElementHandle>>`
- `query_all(selector)` -> `DomResult<ElementCollection>`
- `query_xpath(expr)` -> `DomResult<ElementCollection>`
- `query_by_text(text, exact)` -> `DomResult<ElementCollection>`
- `parent()` -> `DomResult<Option<ElementHandle>>`
- `children()` -> `DomResult<ElementCollection>`
- `next_sibling()` -> `DomResult<Option<ElementHandle>>`
- `previous_sibling()` -> `DomResult<Option<ElementHandle>>`
- `closest(selector)` -> `DomResult<Option<ElementHandle>>`
- `ancestors()` -> `AncestorIterator` (lazy)
- `descendants()` -> `DescendantIterator` (lazy)
- `shadow_root()` -> `DomResult<Option<ShadowRootHandle>>`
- `owning_frame()` -> `FrameHandle`
- `set_inner_html(html)` -> `DomResult<()>`
- `set_text_content(text)` -> `DomResult<()>`
- `remove()` -> `DomResult<()>`
- `is_stale()` -> `bool` (no backend call)
- `backend_element_port()` -> `&dyn ElementPort` (escape hatch, public for bridge interop)

### 1.2 NodeHandle

**Purpose:** Represents any DOM node (not just elements). Can be Element, Text, Comment, DocumentFragment, etc. Uses an enum internally to dispatch operations.

**Ownership:** Consumer-owned, Clone + Send + Sync.

**Mutability:** Immutable handle. Read operations only. Mutations happen through ElementHandle.

```rust
enum NodeHandleKind {
    Element(ElementHandle),
    Text(TextNode),
    Comment(CommentNode),
    DocumentFragment(DocumentFragment),
    ShadowRoot(ShadowRootHandle),
}

struct NodeHandle {
    kind: NodeHandleKind,
    node_type: NodeType,
    handle_id: HandleId,
}
```

**Methods:**
- `node_type()` -> `NodeType`
- `id()` -> `HandleId`
- `as_element()` -> `Option<ElementHandle>`
- `parent_node()` -> `DomResult<Option<NodeHandle>>`
- `child_nodes()` -> `DomResult<NodeCollection>`
- `text_content()` -> `DomResult<String>`
- `snapshot()` -> `DomResult<NodeSnapshot>`

### 1.3 TextNode (`pub(crate)`, accessed via `NodeHandle.as_text()`)

**Purpose:** Represents a text node in the DOM tree. Read-only handle to text content.

```rust
struct TextNode {
    handle_id: HandleId,
    element_port: Arc<dyn ElementPort>,  // Parent element for backend access
    text: String,
}
```

### 1.4 CommentNode (`pub(crate)`, accessed via `NodeHandle.as_comment()`)

**Purpose:** Represents an HTML/XML comment node.

```rust
struct CommentNode {
    handle_id: HandleId,
    data: OnceLock<String>,
}
```

### 1.5 DocumentFragment (`pub(crate)`, accessed via `NodeHandle.as_document_fragment()`)

**Purpose:** Represents a document fragment for DOM insertion operations.

```rust
struct DocumentFragment {
    handle_id: HandleId,
    nodes: Vec<ElementHandle>,
}
```

### 1.6 ShadowRootHandle

**Purpose:** Handle to a Shadow DOM root. See Shadow DOM section for details.

```rust
struct ShadowRootHandle {
    host: ElementHandle,
    mode: ShadowRootMode,
    handle_id: HandleId,
    element_port: Arc<dyn ElementPort>,
}

enum ShadowRootMode {
    Open,
    Closed,
}
```

### 1.7 NodeSnapshot

**Purpose:** Materialized snapshot of a DOM node tree at a point in time. Immutable after creation. Used for offline analysis, diffing, and artifact storage.

**Ownership:** Consumer-owned, Clone + Send + Sync. No backend connection — a pure data structure.

```rust
struct NodeSnapshot {
    node_id: NodeId,
    handle_id: HandleId,
    node_type: NodeType,
    tag_name: String,
    attributes: HashMap<String, String>,
    text: String,
    children: Vec<NodeSnapshot>,
    frame_id: Option<FrameId>,
    bounding_box: Option<BoundingBox>,
    is_visible: bool,
    is_enabled: bool,
    timestamp: DateTime<Utc>,
}

impl NodeSnapshot {
    fn find_id(&self, id: &str) -> Option<&NodeSnapshot>;
    fn find_by_tag(&self, tag: &str) -> Vec<&NodeSnapshot>;
    fn find_by_text(&self, text: &str, exact: bool) -> Vec<&NodeSnapshot>;
    fn find_by_attr(&self, name: &str, value: &str) -> Vec<&NodeSnapshot>;
    fn path(&self) -> String;  // CSS selector path from root
}
```

### 1.8 DomSnapshot

**Purpose:** A complete DOM snapshot for a page or frame at a moment in time.

```rust
struct DomSnapshot {
    root: NodeSnapshot,
    url: String,
    title: String,
    timestamp: DateTime<Utc>,
    frame_id: FrameId,
    frame_count: usize,
    metadata: HashMap<String, String>,
}

impl DomSnapshot {
    fn root(&self) -> &NodeSnapshot;
    fn find_id(&self, id: &str) -> Option<&NodeSnapshot>;
    fn find_by_tag(&self, tag: &str) -> Vec<&NodeSnapshot>;
    fn find_by_text(&self, text: &str, exact: bool) -> Vec<&NodeSnapshot>;
    fn find_by_selector(&self, selector: &str) -> Vec<&NodeSnapshot>;
}
```

---

## 2. Collection Types

### 2.1 NodeCollection

**Purpose:** A collection of NodeHandles. Can be live (backed by a query that refreshes) or static (materialized at collection time).

```rust
enum CollectionKind {
    Live(Arc<dyn LiveQuery>),       // Re-queries backend on each access
    Static(Vec<NodeHandle>),        // Materialized at creation
}

struct NodeCollection {
    kind: CollectionKind,
    length: usize,  // may be stale for Live
}

impl NodeCollection {
    fn get(&self, index: usize) -> DomResult<Option<NodeHandle>>;
    fn iter(&self) -> NodeIterator;
    fn filter(predicate: impl Fn(&NodeHandle) -> bool) -> DomResult<NodeCollection>;
    fn to_vec(&self) -> DomResult<Vec<NodeHandle>>;
    fn is_live(&self) -> bool;
    fn snapshot(&self) -> DomResult<Vec<NodeSnapshot>>;
}
```

### 2.2 ElementCollection

**Purpose:** An element-only collection (filters out non-element nodes). Same live/static semantics.

```rust
struct ElementCollection {
    inner: NodeCollection,
}

impl ElementCollection {
    fn get(&self, index: usize) -> DomResult<Option<ElementHandle>>;
    fn iter(&self) -> ElementIterator;
    fn first(&self) -> DomResult<Option<ElementHandle>>;
    fn last(&self) -> DomResult<Option<ElementHandle>>;
    fn count(&self) -> DomResult<usize>;
    fn nth(&self, n: usize) -> DomResult<Option<ElementHandle>>;
    fn to_vec(&self) -> DomResult<Vec<ElementHandle>>;
    fn snapshot(&self) -> DomResult<Vec<NodeSnapshot>>;
}
```

---

## 3. Attribute Types

### 3.1 Attributes

**Purpose:** Typed access to element attributes. Named collection that supports iteration, indexing, and type conversion.

```rust
struct Attributes {
    inner: Arc<dyn ElementPort>,
    handle: ElementHandle,  // For stale check
}

impl Attributes {
    fn get(&self, name: &str) -> DomResult<Option<String>>;
    fn set(&self, name: &str, value: &str) -> DomResult<()>;
    fn remove(&self, name: &str) -> DomResult<()>;
    fn has(&self, name: &str) -> DomResult<bool>;
    fn len(&self) -> DomResult<usize>;
    fn names(&self) -> DomResult<Vec<String>>;
    fn entries(&self) -> DomResult<Vec<(String, String)>>;
    fn dataset(&self) -> DomResult<Dataset>;
    fn aria(&self) -> DomResult<AriaAttributes>;
}

struct Dataset(Attributes);  // filters to data-* attributes, camelCase keys

struct AriaAttributes {
    // Named accessors for common ARIA attributes
    fn role(&self) -> DomResult<Option<String>>;
    fn label(&self) -> DomResult<Option<String>>;
    fn describedby(&self) -> DomResult<Option<String>>;
    fn expanded(&self) -> DomResult<Option<bool>>;
    fn hidden(&self) -> DomResult<Option<bool>>;
    fn pressed(&self) -> DomResult<Option<bool>>;
    fn selected(&self) -> DomResult<Option<bool>>;
    fn checked(&self) -> DomResult<Option<bool>>;
}
```

### 3.2 ClassList

**Purpose:** Typed access to element CSS classes. Live view of the class attribute.

```rust
struct ClassList {
    inner: Arc<dyn ElementPort>,
    handle: ElementHandle,
}

impl ClassList {
    fn contains(&self, class: &str) -> DomResult<bool>;
    fn add(&self, class: &str) -> DomResult<()>;
    fn remove(&self, class: &str) -> DomResult<()>;
    fn toggle(&self, class: &str) -> DomResult<bool>;
    fn replace(&self, old: &str, new: &str) -> DomResult<()>;
    fn len(&self) -> DomResult<usize>;
    fn values(&self) -> DomResult<Vec<String>>;
}
```

---

## 4. Style Types

### 4.1 StyleDeclaration

**Purpose:** Read-write access to an element's inline style property. Maps to the `style` attribute.

```rust
struct StyleDeclaration {
    inner: Arc<dyn ElementPort>,
    handle: ElementHandle,
}

impl StyleDeclaration {
    fn get(&self, property: &str) -> DomResult<Option<String>>;
    fn set(&self, property: &str, value: &str) -> DomResult<()>;
    fn remove(&self, property: &str) -> DomResult<()>;
    fn css_text(&self) -> DomResult<String>;
    fn set_css_text(&self, css: &str) -> DomResult<()>;
    fn len(&self) -> DomResult<usize>;
    fn entries(&self) -> DomResult<Vec<(String, String)>>;
}
```

### 4.2 ComputedStyle

**Purpose:** Read-only access to an element's computed (post-CSS cascade) style. Always goes to backend.

```rust
struct ComputedStyle {
    inner: Arc<dyn ElementPort>,
    handle: ElementHandle,
    pseudo_element: Option<String>,  // "::before", "::after", etc.
}

impl ComputedStyle {
    fn get(&self, property: &str) -> DomResult<Option<String>>;
    fn all(&self) -> DomResult<HashMap<String, String>>;
}
```

---

## 5. Geometry Types

### 5.1 BoundingBox

**Purpose:** Element bounding rectangle. Replaces `BoxModel` with a simpler, more ergonomic type.

```rust
struct BoundingBox {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl BoundingBox {
    fn from(model: &BoxModel) -> Self;  // Construct from bridge BoxModel
    fn center(&self) -> Point;
    fn intersects(&self, other: &BoundingBox) -> bool;
    fn contains(&self, point: &Point) -> bool;
    fn area(&self) -> f64;
}

// BoxEdges re-exported from browseros-bridge — no standalone DOM definition.
```

---

## 6. Locator Types

### 6.1 LocatorBuilder

**Purpose:** Builds `LocatorStrategy` values with a fluent API. The primary way consumers create locators.

```rust
struct LocatorBuilder {
    strategy: LocatorStrategy,
}

impl LocatorBuilder {
    // Factory methods
    fn css(selector: &str) -> Self;
    fn xpath(expr: &str) -> Self;
    fn text(text: &str) -> Self;
    fn exact_text(text: &str) -> Self;
    fn role(role: &str) -> Self;
    fn label(text: &str) -> Self;
    fn placeholder(text: &str) -> Self;
    fn test_id(id: &str) -> Self;
    fn alt_text(text: &str) -> Self;
    fn title(text: &str) -> Self;

    // Composition methods
    fn and(self, other: Self) -> Self;
    fn or(self, other: Self) -> Self;
    fn child(self, parent: Self) -> Self;
    fn nth(self, index: usize) -> Self;
    fn visible(self) -> Self;

    // Build
    fn build(&self) -> LocatorStrategy;
}

// AI-assisted locator
struct AiLocator {
    description: String,  // Natural language description
    confidence: Option<f64>,
}

impl LocatorBuilder {
    fn ai(description: &str) -> Self;  // Uses LLM/backend to infer strategy
}
```

### 6.2 Locator (high-level)

**Purpose:** A locator bound to a specific page/frame. Provides wait, resolve, and assertion operations.

```rust
struct Locator {
    strategy: LocatorStrategy,
    engine: Arc<dyn LocatorEngine>,
    frame_id: FrameId,
    timeout: Duration,
}

impl Locator {
    fn resolve(&self) -> DomResult<ElementHandle>;
    fn resolve_all(&self) -> DomResult<ElementCollection>;
    fn wait(&self, timeout: Option<Duration>) -> DomResult<ElementHandle>;
    fn wait_for_absence(&self, timeout: Option<Duration>) -> DomResult<()>;
    fn is_visible(&self) -> DomResult<bool>;
    fn is_enabled(&self) -> DomResult<bool>;
    fn count(&self) -> DomResult<usize>;
    fn snapshot(&self) -> DomResult<Vec<NodeSnapshot>>;
    fn filter(&self, additional: &LocatorStrategy) -> Self;
    fn first(&self) -> Self;  // :nth(0)
    fn last(&self) -> Self;
    fn nth(&self, index: usize) -> Self;
}
```

---

## 7. Frame Types

### 7.1 FrameHandle

**Purpose:** Handle to a frame within a page. Provides frame-local DOM operations.

```rust
struct FrameHandle {
    frame_id: FrameId,
    locator_engine: Arc<dyn LocatorEngine>,
    page_id: PageId,
    parent_frame_id: Option<FrameId>,
    generation: Arc<AtomicU64>,  // Incremented on frame detachment
    frame_port: Arc<dyn FramePort>,  // Provides url(), title() live from backend
}

impl FrameHandle {
    fn id(&self) -> FrameId;
    fn parent_id(&self) -> Option<FrameId>;
    fn is_main_frame(&self) -> bool;
    fn url(&self) -> DomResult<String>;   // current frame URL
    fn title(&self) -> DomResult<String>; // current frame title
    fn query(&self, selector: &str) -> DomResult<Option<ElementHandle>>;
    fn query_all(&self, selector: &str) -> DomResult<ElementCollection>;
    fn query_xpath(&self, expr: &str) -> DomResult<ElementCollection>;
    fn query_by_text(&self, text: &str, exact: bool) -> DomResult<ElementCollection>;
    fn locator(&self) -> LocatorBuilder;
    fn snapshot(&self) -> DomResult<DomSnapshot>;
    fn evaluate(&self, script: &str) -> DomResult<JsResult>;
    fn child_frames(&self) -> DomResult<Vec<FrameHandle>>;
}
```

---

## 8. Mutation Types

### 8.1 MutationBatch

**Purpose:** Groups multiple DOM mutations into a single batch. All mutations are applied in order. If any mutation fails, already-applied mutations are NOT rolled back (no browser backend guarantees transactional DOM). Consumers should check individual results.

```rust
enum Mutation {
    SetAttribute { name: String, value: String },
    RemoveAttribute { name: String },
    SetTextContent { text: String },
    SetInnerHtml { html: String },
    RemoveElement,
    InsertBefore { new_child: ElementHandle, reference: Option<ElementHandle> },
    AppendChild { child: ElementHandle },
    ReplaceChild { new_child: ElementHandle, old_child: ElementHandle },
    RemoveChild { child: ElementHandle },
    CloneNode { deep: bool },  // Returns new handle
    MoveNode { target_parent: ElementHandle, reference: Option<ElementHandle> },
}

struct MutationBatch {
    mutations: Vec<Mutation>,
}

impl MutationBatch {
    fn new() -> Self;
    fn push(mutation: Mutation) -> &mut Self;
    fn apply(&self, target: &ElementHandle) -> DomResult<()>;
    fn apply_all(targets: &[(&ElementHandle, Vec<Mutation>)]) -> DomResult<()>;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool;
}
```

### 8.2 MutationObserver

**Purpose:** Observes DOM mutations on a specific element or subtree. Wraps backend mutation events and delivers typed mutation records.

```rust
struct MutationRecord {
    target: HandleId,
    added_nodes: Vec<NodeHandle>,
    removed_nodes: Vec<NodeHandle>,
    attribute_name: Option<String>,
    old_value: Option<String>,
    new_value: Option<String>,
    mutation_type: MutationType,
}

enum MutationType {
    ChildList,
    Attributes,
    CharacterData,
    Subtree,
}

struct MutationObserver {
    callback: Box<dyn Fn(&[MutationRecord]) + Send + Sync>,
    options: MutationObserverOptions,
}

struct MutationObserverOptions {
    child_list: bool,
    attributes: bool,
    character_data: bool,
    subtree: bool,
    attribute_filter: Option<Vec<String>>,
    attribute_old_value: bool,
    character_data_old_value: bool,
}

impl MutationObserver {
    fn new(callback: impl Fn(&[MutationRecord]) + Send + Sync + 'static,
           options: MutationObserverOptions) -> Self;
    fn observe(&self, target: &ElementHandle) -> DomResult<ObserverHandle>;
    fn disconnect(&self, handle: &ObserverHandle) -> DomResult<()>;
    fn take_records(&self, handle: &ObserverHandle) -> Vec<MutationRecord>;
}
```

---

## 9. Traversal Types

### 9.1 TreeWalker

**Purpose:** Walks the DOM tree with optional node filter. Can be constrained by what-to-show mask and custom filter callback.

**Note:** Every traversal step checks generation on `current` and `root`. If either is stale, returns `DomError::StaleElement`.

```rust
struct TreeWalker {
    current: Option<ElementHandle>,
    root: ElementHandle,
    what_to_show: NodeFilter,
}

bitflags WhatToShow {
    const ELEMENT = 1;
    const TEXT = 2;
    const COMMENT = 4;
    const DOCUMENT_FRAGMENT = 8;
    const ALL = !0;
}

impl TreeWalker {
    fn new(root: &ElementHandle, what_to_show: WhatToShow) -> Self;
    fn parent_node(&mut self) -> DomResult<Option<ElementHandle>>;
    fn first_child(&mut self) -> DomResult<Option<ElementHandle>>;
    fn last_child(&mut self) -> DomResult<Option<ElementHandle>>;
    fn next_sibling(&mut self) -> DomResult<Option<ElementHandle>>;
    fn previous_sibling(&mut self) -> DomResult<Option<ElementHandle>>;
    fn next_node(&mut self) -> DomResult<Option<ElementHandle>>;
    fn previous_node(&mut self) -> DomResult<Option<ElementHandle>>;
    fn current_node(&self) -> Option<&ElementHandle>;
    fn reset(&mut self);
}
```

### 9.2 Iterators

```rust
struct AncestorIterator { /* yields ElementHandle from parent up to root */ }
struct DescendantIterator { /* yields ElementHandle in depth-first order */ }
struct SiblingIterator { /* yields ElementHandle siblings */ }
struct ElementIterator { /* wraps NodeCollection yielding ElementHandles */ }
struct NodeIterator { /* wraps NodeCollection yielding NodeHandles */ }

impl Iterator for AncestorIterator { type Item = DomResult<ElementHandle>; }
impl Iterator for DescendantIterator { type Item = DomResult<ElementHandle>; }
impl Iterator for SiblingIterator { type Item = DomResult<ElementHandle>; }
impl Iterator for ElementIterator { type Item = DomResult<ElementHandle>; }
impl Iterator for NodeIterator { type Item = DomResult<NodeHandle>; }
```

---

## 10. Snapshot Types

### 10.1 DomSnapshot (detailed)

```rust
struct DomSnapshot {
    root: NodeSnapshot,
    url: String,
    title: String,
    timestamp: DateTime<Utc>,
    frame_count: usize,
    metadata: HashMap<String, String>,
}

impl DomSnapshot {
    fn root(&self) -> &NodeSnapshot;
    fn find_id(&self, id: &str) -> Option<&NodeSnapshot>;
    fn find_by_tag(&self, tag: &str) -> Vec<&NodeSnapshot>;
    fn find_by_text(&self, text: &str, exact: bool) -> Vec<&NodeSnapshot>;
    fn find_by_selector(&self, selector: &str) -> Vec<&NodeSnapshot>;  // gated behind feature flag 'snapshot-css-matching'
}

```

---

## 11. Public API Surface Summary

| Category | Types | Classification |
|----------|-------|---------------|
| Element Identity | `ElementHandle`, `HandleId` | SAFE TO FREEZE |
| Node Identity | `NodeHandle` | SAFE TO FREEZE |
| Node Variants | `TextNode`, `CommentNode`, `DocumentFragment` | `pub(crate)` — accessed via `NodeHandle.as_*()` |
| Shadow DOM | `ShadowRootHandle`, `ShadowRootMode` | SAFE TO FREEZE |
| Frame | `FrameHandle` | SAFE TO FREEZE |
| Snapshot (core) | `DomSnapshot`, `NodeSnapshot` | SAFE TO FREEZE |
| Snapshot (diff) | `diff()`, `to_json()`, `to_pretty_json()`, `DiffEntry` | KEEP FLEXIBLE (gated behind `snapshot-diff` feature) |
| Snapshot (CSS) | `find_by_selector()` | KEEP FLEXIBLE (gated behind `snapshot-css-matching` feature) |
| Collections | `NodeCollection`, `ElementCollection` | KEEP FLEXIBLE (backend iteration API may evolve) |
| Locator | `LocatorBuilder`, `Locator` | KEEP FLEXIBLE (AI locator API is evolving) |
| Traversal | `TreeWalker`, `AncestorIterator`, `DescendantIterator` | SAFE TO FREEZE |
| Mutation | `MutationBatch`, `MutationObserver`, `MutationRecord` | KEEP FLEXIBLE (apply ownership model may adjust) |
| Attributes | `Attributes`, `ClassList`, `Dataset`, `AriaAttributes` | SAFE TO FREEZE |
| Style | `StyleDeclaration`, `ComputedStyle` | KEEP FLEXIBLE (browser CSS API differences) |
| State | `ElementState`, state check methods | SAFE TO FREEZE |
| Error | `DomError` | SAFE TO FREEZE |
| Events | `DomEvent` types (payloads only) | SAFE TO FREEZE |
