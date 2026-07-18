# DOM Risk Analysis

**Status:** FROZEN — Risk analysis locked. Implementation in progress.

---

## 1. Architectural Risks

### R1: Live Collection Performance

**Risk:** `NodeCollection` and `ElementCollection` support a "live" mode that re-queries the backend on each access. If consumers iterate a live collection with many elements, each `get()` call triggers a backend round-trip, causing O(n) sequential CDP calls on every access.

**Severity:** Medium

**Likelihood:** High — consumers will naturally iterate collections in loops.

**Mitigation:**
- Default collections to `Static` (materialized on creation). Live mode is opt-in.
- Live collections expose `snapshot()` which materializes all elements in one backend call.
- Documentation warns about live collection performance.
- Implementation may add batching (one backend call per batch of element requests).

### R2: Handle Identity After Re-render

**Risk:** Modern frameworks (React, Vue, Svelte) frequently re-render the DOM. An element removed and re-inserted in the same position may get a new backend node ID, causing the handle to become stale even though the element is "the same" from the user's perspective.

**Severity:** Medium

**Likelihood:** High — single-page applications re-render constantly.

**Mitigation:**
- Stale detection is explicit and predictable. Consumers know when a handle is stale.
- Locator-based re-acquisition is ergonomic: `page.locator(...).resolve()` finds the element again.
- Framework-specific heuristics are NOT built into the DOM crate (keeps it backend-agnostic).
- A future `browseros-plugin` could implement framework-specific re-acquisition strategies.

### R3: Cross-Origin Frame Isolation

**Risk:** Cross-origin iframes have strict security policies. Operations on elements in cross-origin iframes may fail with security errors from the backend, producing confusing errors.

**Severity:** Low

**Likelihood:** Medium — cross-origin iframes are common.

**Mitigation:**
- `FrameHandle` has an `is_cross_origin()` method that returns `true` if the frame's origin differs from the main frame.
- Operations on cross-origin frames return `DomError::CrossOriginFrame` immediately (no backend call).
- Documentation clearly states cross-origin limitations.

### R4: Closed Shadow Root Exposure

**Risk:** Backend may expose closed shadow roots via internal CDP commands (like `DOM.getDocument` with `pierce` flag). Consumers may rely on this behavior, creating a hidden dependency on non-standard backend behavior.

**Severity:** Medium

**Likelihood:** Medium — developers who discover the ability will use it.

**Mitigation:**
- The `ShadowRootHandle` explicitly reports `mode: Closed`.
- Piercing closed shadow roots requires an explicit opt-in flag: `enable_closed_piercing: bool` in a configuration struct.
- The default is `false`. Closed roots are not traversable by default.
- Documentation warns that closed root traversal is non-standard and may not work in all backends.

### R5: Navigation Race on Generation Increment

**Risk:** If the generation counter is incremented via an async event callback (e.g., `Page.frameNavigated`), there is a window where a navigation has completed but the generation counter hasn't been updated yet. A consumer issuing an operation during this window would use a stale handle that appears valid.

**Severity:** High

**Likelihood:** Low (with synchronous increment mitigation)

**Mitigation:**
- The generation counter is incremented **synchronously** within `page.navigate()`, before the call returns to the consumer
- No generation change is triggered by async event callbacks alone
- The race window is eliminated entirely — the consumer sees a consistent state after `navigate()` returns
- Frame attachment/detachment generation increments follow the same synchronous pattern

---

## 2. Performance Risks

### P1: Snapshot Materialization Size

**Risk:** A `DomSnapshot` of a large page (thousands of elements, deep trees) could be megabytes of serialized data. Creating such a snapshot blocks the calling thread and consumes significant memory.

**Severity:** Medium

**Likelihood:** Medium — large pages exist.

**Mitigation:**
- `DomSnapshot` supports depth limits and subtree constraints: `snapshot(max_depth: Option<u32>, selector_filter: Option<&str>)`.
- The default snapshot depth is limited to a configurable maximum (default 10 levels).
- Deep snapshots require explicit: `snapshot_all()`.
- Streaming snapshot API: `snapshot_stream()` returns an `Iterator<Item = DomResult<NodeSnapshot>>` for incremental processing.

### P2: Frequent Stale-Check Overhead

**Risk:** Every DOM operation checks handle generation before forwarding to the backend. On hot paths (e.g., iterating many elements in a tight loop), the atomic load on the generation counter adds measurable overhead.

**Severity:** Low

**Likelihood:** Medium — depends on usage patterns.

**Mitigation:**
- Generation check is a single `AtomicU64::load()` with `Relaxed` ordering (cheapest atomic operation).
- If profiling shows this as a bottleneck, an `assume_fresh()` method can skip the check (unsafe escape hatch).
- Collection iteration can cache generation at the start of the loop.

### P3: MutationObserver Backend Polling

**Risk:** If the backend does not provide push-based mutation events (e.g., CDP's `DOM.childNodeInserted`), the `MutationObserver` would need to poll the DOM tree for changes, creating overhead proportional to polling frequency.

**Severity:** High

**Likelihood:** Low — CDP provides push mutation events. Other backends may not.

**Mitigation:**
- The mutation observer is designed for push-based backends (CDP events → callback invocation).
- For polling-based backends, a `PollingMutationObserver` adapter can be added (not in the core crate).
- The core `MutationObserver` panics with a clear error if no push event source is configured.

---

## 3. Ownership Risks

### O1: Handle Leak via Arc Cycles

**Risk:** If an `ElementHandle` captures an `Arc<dyn ElementPort>` that holds a reference back to the same handle (e.g., via event callback), a reference cycle is created that prevents deallocation.

**Severity:** Low

**Likelihood:** Low — careful ownership design prevents cycles.

**Mitigation:**
- `ElementHandle` does NOT hold any callback that references itself.
- `MutationObserver` uses `Weak<dyn ElementPort>` for observation targets.
- All callback registrations are `Fn` (not `FnOnce`) to allow explicit disconnection.
- Documentation warns against creating cycles in custom callbacks.

### O2: Handle Proliferation

**Risk:** Each query operation creates new `ElementHandle`s with new `HandleId`s. If consumers query the same element repeatedly without reusing handles, memory usage grows unbounded and cache locality degrades.

**Severity:** Low

**Likelihood:** Medium — common in loop-based scraping code.

**Mitigation:**
- Handles are lightweight: `HandleId` (16 bytes UUID) + `Arc` pointers (~24 bytes) + generation counter (~8 bytes) ≈ 50-100 bytes each.
- 10,000 handles consume ~1 MB — not a practical concern.
- Consumers are encouraged to reuse handles via cloning.
- The frame-level `element_cache` can deduplicate handles within a frame (optional, not default).

---

## 4. API Risks

### A1: Method Explosion on ElementHandle

**Risk:** `ElementHandle` is designed to be the primary user-facing type. As more features are added, it may accumulate dozens of methods, becoming a "god object."

**Severity:** Medium

**Likelihood:** Medium — natural accretion over time.

**Mitigation:**
- Methods are grouped into sub-objects: `handle.attributes()`, `handle.style()`, `handle.classList()`, `handle.computed_style()`.
- Navigation-like methods (query, parent, children) are on the handle directly.
- Feature-specific methods (shadow DOM, frame operations) are on their respective sub-objects.
- The freeze plan explicitly classifies what can be added without review.

### A2: Bridge Trait Abstraction Leak

**Risk:** If `ElementHandle` provides an `as_backend_port()` method (for implementing crates that need direct backend access), consumers outside the DOM crate may depend on backend-specific features, breaking protocol isolation.

**Severity:** High

**Likelihood:** Medium — consumers often ask for escape hatches.

**Mitigation:**
- `backend_element_port()` returns `&dyn ElementPort` — still protocol-agnostic.
- No method exposes protocol-specific types (no CdpElement, no WebDriver element).
- The method is `pub(crate)` — external crates use the DOM API exclusively. No bypassing the DOM layer.
- Documented as an escape hatch for implementing crates within the workspace only.

---

## 5. Extensibility Risks

### E1: AI Locator Interface Instability

**Risk:** AI-assisted locators are an active research area. The interface format (natural language description, confidence score, fallback strategy, multi-modal input) may need frequent changes.

**Severity:** Low

**Likelihood:** High — AI interfaces evolve rapidly.

**Mitigation:**
- `AiLocator` is classified as EXPERIMENTAL in the freeze plan.
- The `LocatorBuilder::ai()` method accepts a simple string description. Complex AI strategies are added as separate methods, not modifications to existing ones.
- AI locator results are validated against the strategy builder: `locator.ai("...").with_fallback(LocatorBuilder::css("..."))`.

### E2: Future Backend Incompatibility

**Risk:** A future backend (Firefox, WebKit, Playwright protocol) may not support all DOM operations that CDP supports (e.g., `DOM.getDocument` with depth, `DOM.querySelector`, getBoxModel).

**Severity:** High

**Likelihood:** Low — most backends support these core operations.

**Mitigation:**
- The bridge `ElementPort` defines the minimal contract. Operations not supported by a backend return `BridgeError::NotImplemented`.
- The DOM crate maps `BridgeError::NotImplemented` to `DomError::NotSupported`.
- Consumers check for `DomError::NotSupported` and provide fallback behavior.
- New backends implement the subset they support; unsupported operations fail gracefully.

### E3: WASM/Non-Native Target Compatibility

**Risk:** If `browseros-dom` is ever compiled to WASM (e.g., for in-browser agent execution), threading primitives (`std::thread`, `Arc`, `Mutex`) may not be available or may behave differently.

**Severity:** Low

**Likelihood:** Low — WASM compilation is not a Phase 2 goal.

**Mitigation:**
- No WASM-specific design decisions are made now.
- The architecture uses `Arc` for sharing, which works in WASM (single-threaded).
- `std::sync::Mutex` can be replaced with `std::cell::RefCell` in WASM builds via conditional compilation.
- No `std::thread` usage in the DOM crate itself.

---

## 6. Risk Summary

| ID | Risk | Severity | Likelihood | Mitigation |
|----|------|----------|------------|------------|
| R1 | Live collection performance | Medium | High | Default static, opt-in live, snapshot for bulk |
| R2 | Handle identity after re-render | Medium | High | Explicit staleness, locator re-acquisition |
| R3 | Cross-origin frame isolation | Low | Medium | Early detection, clear error, doc warnings |
| R4 | Closed shadow root exposure | Medium | Medium | Default-off piercing, mode reporting |
| R5 | Navigation race on generation increment | High | Low | Synchronous increment in navigate(), no async window |
| P1 | Snapshot size | Medium | Medium | Depth limits, streaming API |
| P2 | Stale-check overhead | Low | Medium | Relaxed atomics, assume_fresh escape hatch |
| P3 | MutationObserver polling | High | Low | Push-based design, poll adapter separate |
| O1 | Arc cycle leaks | Low | Low | Weak refs in callbacks, doc warnings |
| O2 | Handle proliferation | Low | Medium | Lightweight handles, optional dedup |
| A1 | Method explosion | Medium | Medium | Sub-object grouping, freeze plan |
| A2 | Bridge trait leak | High | Medium | Restricted escape hatch, doc warnings |
| E1 | AI locator instability | Low | High | Experimental classification, string input |
| E2 | Future backend incompatibility | High | Low | NotImplemented → NotSupported mapping |
| E3 | WASM compatibility | Low | Low | No WASM changes now, conditional compilation |

**Overall Risk Level:** Low. No blocking risks identified. All risks have clear mitigations. The highest-severity risks (P3, A2, E2) have low likelihood and concrete mitigations.
