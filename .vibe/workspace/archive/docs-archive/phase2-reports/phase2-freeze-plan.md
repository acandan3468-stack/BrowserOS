ARCHIVED

This document has been archived. Its content has been merged into the canonical documentation set.
See docs/ for current information.

---

# Phase 2 — Freeze Plan

For every Phase 2 subsystem, specify whether interfaces and architecture are safe to freeze or must remain flexible, with justification.

---

## 1. `browseros-bridge`

**Verdict: SAFE TO FREEZE** after initial design review.

**Justification:** Bridge traits (BrowserPort, SessionPort, PagePort, etc.) are the contract between BrowserOS and any browser automation protocol. They must be stable — changing a trait method signature breaks every implementation and every consumer. The 11 traits defined in this document cover 95%+ of browser automation operations. Additions (new traits, new methods) are possible backward-compatibly through default implementations or new sub-traits.

**Freeze scope:**
- Method signatures of all 11 traits
- Value types (NodeInfo, BoxModel, Cookie, etc.)
- Error codes for standard failure modes
- LocatorStrategy enum variants

**Keep flexible:**
- Default method implementations (can be added without breaking)
- `LocatorStrategy` could gain future variants (e.g., `AiVision`, `AriaPattern`) — these are additive

---

## 2. `browseros-cdp`

**Verdict: KEEP FLEXIBLE** for the entire Phase 2.

**Justification:** CDP is a living protocol. Chrome adds, deprecates, and removes CDP domains every release. The implementation will evolve continuously. Internal modules (commands, events, transport) are private to the crate and can change freely.

**Keep flexible:**
- Internal module structure
- CDP type deserialization handling
- Connection retry logic
- Event→Bridge translation
- Platform-specific browser discovery

**Stable contract:**
- The trait implementations (impl XxxPort for CdpXxx) — these must match bridge traits
- `CdpConnection::connect()` and `CdpSession::send()` signatures
- Error translation from CDP errors to browseros errors

---

## 3. `browseros-browser`

**Verdict: SAFE TO FREEZE** service-level API.

**Justification:** `BrowserManager`, `BrowserHandle`, `SessionHandle`, `PageHandle` are the primary public API for agents. These structs are thin wrappers around bridge traits — the complexity lives in CDP. The composition `Manager → Handle → Trait` is stable.

**Freeze scope:**
- `BrowserManager::launch()` / `connect()` / `browsers()` / `default_browser()`
- `BrowserHandle::info()` / `new_session()` / `close()` / `kill()`
- `SessionHandle::pages()` / `new_page()` / `close_page()` / `close()`
- `PageHandle::navigate()` / `evaluate()` / `screenshot()` / `close()` etc.

**Keep flexible:**
- Browser discovery heuristics (locating Chrome executable)
- Launch options structure (new launch flags can be added)
- Session configuration (new config keys are additive)

---

## 4. `browseros-dom`

**Verdict: KEEP FLEXIBLE** until AI-assisted locators stabilize.

**Justification:** DOM querying and locator strategies are an active research area. AI-assisted element selection (vision-based, LLM-guided) will likely produce new locator strategies that don't exist today. The `LocatorStrategy` enum should remain `#[non_exhaustive]`.

**Freeze scope:**
- `DomSnapshot` structure (node tree shape is stable)
- `ElementState` enum
- `LocatorBuilder` core methods (css, xpath, text, role, test_id)

**Keep flexible:**
- `LocatorStrategy` variants (`#[non_exhaustive]`)
- AI-assisted locator strategies (future Phase 3/4)
- DOM snapshot depth limits and filtering options
- Shadow DOM traversal heuristics

---

## 5. `browseros-page`

**Verdict: SAFE TO FREEZE** operations that map to standard CDP domains.

**Justification:** Page operations (navigate, screenshot, PDF, evaluate, content) are mature CDP domains that haven't changed significantly in years. `WaitCondition` and `DialogStrategy` cover the common cases.

**Freeze scope:**
- `PageWaiter` API
- `ContentExtractor` API
- `DialogAutoHandler` API
- `WaitCondition` core variants

**Keep flexible:**
- New `WaitCondition` variants (e.g., `NetworkBecameIdle`, `JsVariable`)
- Extraction schema format (may evolve based on LLM needs)
- Dialog auto-handle strategies (accept/dismiss/prompt text)

---

## 6. `browseros-network`

**Verdict: SAFE TO FREEZE** core interception and monitoring.

**Justification:** Network monitoring is well-understood. Request/response structures, interception rules, and conditions are mature across Playwright/Puppeteer/Selenium. The existing bridge types cover it.

**Freeze scope:**
- `RequestInfo`, `ResponseInfo`, `TimingInfo` structures
- `InterceptionRule` and `InterceptionAction`
- `NetworkCapture` API (start/stop/filter)
- `NetworkConditions`

**Keep flexible:**
- New resource types (as Chrome adds them)
- HAR export format (can evolve independently)
- WebSocket message structure (additive fields)

---

## 7. `browseros-input`

**Verdict: SAFE TO FREEZE** core action types.

**Justification:** Input simulation (click, type, scroll, drag) is a solved problem. MouseButton, KeyboardAction, and ActionSequence cover all standard interactions.

**Freeze scope:**
- `ClickOptions`, `MouseButton`, `ActionSequence`
- `InputSimulator` core methods
- `FileChooser` API

**Keep flexible:**
- Touch action simulation (mobile is Phase 3+ territory)
- Pointer event granularity (additive pressure/tilt fields)

---

## 8. `browseros-storage`

**Verdict: SAFE TO FREEZE.**

**Justification:** Cookies and web storage APIs are standardized across all browser automation tools. The API surface is small (get/set/delete for cookies, key-value for storage) and unlikely to change.

---

## 9. `browseros-artifact`

**Verdict: SAFE TO FREEZE** for Phase 2.

**Justification:** Artifact storage is a simple file-based store. CRUD operations on bytes with metadata. The API is minimal and stable.

**Keep flexible:**
- Storage backend (local FS → S3/GCS is a future concern)
- Retention/pruning policies (additive, backward-compatible)
- Metadata schema (custom tags per user)

---

## 10. `browseros-plugin`

**Verdict: KEEP FLEXIBLE** — this is the riskiest subsystem.

**Justification:** Plugin loading is a novel addition to BrowserOS. WASM plugin execution, hook interception, and capability negotiation are unproven in this codebase. The exact plugin API will evolve as real plugins are built.

**Freeze scope:**
- `PluginContext` structure (minimal set of capabilities)
- `Plugin` trait lifecycle methods (init/start/stop)
- `PluginRegistry` registration/deregistration
- `CapabilityRegistry` API
- Config isolation (plugin namespace pattern)

**Keep flexible:**
- `PluginEntryPoint` (may add new loading mechanisms: WASM, Lua, external process)
- Hook system (Before/After/Around phases may need refinement)
- Command system parameter types
- Plugin → Host communication mechanism
- Dependency resolution between plugins

---

## 11. `browseros-bridge` — LocatorStrategy

**Verdict: KEEP FLEXIBLE** — mark `#[non_exhaustive]`.

**Justification:** Locator strategies are the primary extension point for AI agents. Future strategies (vision-based selection, LLM-generated selectors, semantic role resolution) must be additive. The existing Phase 1 `#[non_exhaustive]` pattern for `ModuleType` should be applied here.

---

## 12. Event Model

**Verdict: SAFE TO FREEZE** event kind strings and core event structs.

**Justification:** The event kind strings (`browser.*`, `page.*`, `network.*`, etc.) form a stable contract. Agents depend on these strings for subscription. Renaming a kind string is a breaking change. New event kinds can always be added.

**Freeze scope:**
- Event kind string namespace pattern (e.g., `network.request_started`)
- Event struct core fields (metadata + identifying fields)
- EventCategory assignment (Domain/System/Internal)

**Keep flexible:**
- New event kinds (additive, don't freeze the catalog)
- Optional fields on event structs

---

## Freeze Summary

| Subsystem | Freeze Decision | Rationale |
|-----------|----------------|-----------|
| `browseros-bridge` (traits) | **SAFE TO FREEZE** | Contract layer — must be stable |
| `browseros-bridge` (LocatorStrategy) | **KEEP FLEXIBLE** | AI-assisted locators are evolving |
| `browseros-cdp` | **KEEP FLEXIBLE** | CDP protocol evolves continuously |
| `browseros-browser` | **SAFE TO FREEZE** | Thin wrapper around stable traits |
| `browseros-dom` | **KEEP FLEXIBLE** | Locator/AI strategies not settled |
| `browseros-page` | **SAFE TO FREEZE** | Mature CDP domains |
| `browseros-network` | **SAFE TO FREEZE** | Well-understood domain |
| `browseros-input` | **SAFE TO FREEZE** | Solved problem |
| `browseros-storage` | **SAFE TO FREEZE** | Minimal, stable API |
| `browseros-artifact` | **SAFE TO FREEZE** | Simple file store |
| `browseros-plugin` | **KEEP FLEXIBLE** | Unproven, will evolve |
| Runtime integration | **SAFE TO FREEZE** | Service aggregator pattern is stable |
| Event model | **SAFE TO FREEZE** (kinds + structs) | Agent subscription contract |

**Bottom line:** 7 subsystems safe to freeze, 4 keep flexible. Plugin system is the main uncertainty.

