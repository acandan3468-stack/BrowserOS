ARCHIVED

This document has been archived. Its content has been merged into the canonical documentation set.
See docs/ for current information.

---

# Phase 2 — Remediation Plan: NO-GO → GO

**Derived from:** `phase2-master-audit.md`
**Scope:** Only the fixes required to reach GO status
**Constraint:** No frozen API redesign, no new architecture

---

## 1. Finding Registry

Every finding from the master audit is classified, triaged, and assigned a fix identifier.

### Classification Key

| Class | Meaning |
|-------|---------|
| ARCH | Frozen API or architecture property. Fixes are implementation-only. |
| IMPL | Missing or stubbed implementation code. |
| INT | Integration gap between two crates. |
| DOC | Documentation inconsistent with implementation. |

### Blocking Levels

| Level | Meaning |
|-------|---------|
| **Critical** | Blocks every real browser session. GO cannot be reached without this. |
| **High** | Blocks core agent workflows. GO blocked for non-trivial usage. |
| **Medium** | Blocks specific features. GO reachable without this. |
| **Low** | Quality-of-life improvements. No block. |

### Full Finding Table

| Fix ID | Finding | Class | Level | Blocks Browser? | Blocks Phase 3? | Root Cause | Affected Crates |
|--------|---------|-------|-------|----------------|----------------|------------|-----------------|
| **F1** | WebSocket transport missing (A2/I2) | IMPL | Critical | YES (no CDP I/O) | YES (no protocol layer) | `CdpTransport` has `NullTransport` + `MockTransport` only; no WebSocket library in dependency tree | `browseros-cdp` |
| **F2** | BackendFactory bridge gap (A1/I3) | INT | Critical | YES (cannot register CDP backend) | YES (BI cannot launch browsers) | `CdpBrowserProcess` implements `BrowserPort` but not `BackendFactory`; no struct satisfies the contract | `browseros-cdp`, `browseros-browser` |
| **F3** | Frame tracking missing (A5) | IMPL | High | PARTIAL (single-page navigation works; URL/title empty) | YES (agents need frame context) | `CdpPage` stores no frame state; `frames()` returns empty; `id()` returns random UUID; `url()`/`title()` return `""` | `browseros-cdp` |
| **F4** | Element state detection stubbed (A4) | IMPL | High | PARTIAL (query works; visibility/enabled/selected all return constants) | YES (agents need state checks) | `CdpElement::is_visible()` → `true` always; `is_enabled()` → `true` always; `is_checked()`/`is_selected()`/`is_stable()` → constants | `browseros-cdp` |
| **F5** | Event type system mismatch (A3/D3) | ARCH | High | NO (browser launch and navigation emit no typed events) | YES (event-driven agents depend on consistent types) | Central `event-model.md` uses `request_id: String`; network model uses `RequestId` (UUID v7). Navigation events duplicated across 3 locations. | `browseros-browser::events`, `browseros-dom::events`, `event-model.md`, `network-event-model.md` |
| **F6** | Dialog port non-functional (A7) | IMPL | Medium | NO | PARTIAL (agents cannot handle alerts) | `CdpDialogPort::next()` returns `None` always; no CDP `Page.javascriptDialogOpening` listener | `browseros-cdp` |
| **F7** | Download port non-functional (A7) | IMPL | Medium | NO | PARTIAL (downloads not observable) | `CdpDownloadPort` is entirely `NotImplemented` | `browseros-cdp` |
| **F8** | Network interception stubbed (A7) | IMPL | Medium | NO | PARTIAL (network monitoring/blocking non-functional) | `CdpNetworkPort::add_interception_rule()` and `remove_interception_rule()` → `NotImplemented` | `browseros-cdp` |
| **F9** | Missing crate: browseros-storage (G1/G5) | IMPL | Medium | NO | PARTIAL (cookie/storage operations needed) | Designed in docs but never created as workspace member; `StoragePort` bridge trait exists | `browseros-storage` (new) |
| **F10** | Missing crate: browseros-network (G1/G5) | IMPL | Medium | NO | PARTIAL (network monitoring needed) | Phase 2.6 design frozen but no crate created; `NetworkPort` bridge trait exists | `browseros-network` (new) |
| **F11** | FrameHandle ↔ ElementHandle generation gap (L1) | ARCH | Medium | NO | PARTIAL (cross-frame element staleness unchecked) | `FrameHandle.generation` is not shared with `ElementHandle.generation`; frame navigation invalidates frame handle but not element handles within it | `browseros-dom` |
| **F12** | PageHandle lacks generation counter (L2) | ARCH | Medium | NO | PARTIAL (page navigation stale detection relies on ElementHandle's own mechanism) | `PageHandle` has no generation counter; element handles rely on their own `check_stale()` after navigation | `browseros-browser`, `browseros-dom` |
| **F13** | No cross-crate integration tests (A8) | INT | Medium | NO | NO (risk of undetected regressions) | No test verifies EventBus event flow from `browseros-browser` through to subscribers | `browseros-browser`, `browseros-event-bus` |
| **F14** | Shutdown sequence not enforced (L4) | IMPL | Low | NO | NO | `RuntimeContext::drop` uses `Arc`-wrapped fields; Rust drop order is unspecified; documented sequence not coded | `browseros-runtime` |
| **F15** | selector_filter no-op on FrameHandle::snapshot (I5) | IMPL | Low | NO | NO | Parameter accepted but prefixed with `_` — not passed to backend | `browseros-dom` |
| **F16** | Documentation stale (D1/D3/D4/G2/G4) | DOC | Low | NO | NO | `phase2-crate-map.md` lists identifiers as `types` but live in `bridge`; event models duplicated; crate map lists non-existent crates | `.md` files under `docs/` |

---

## 2. Fix Dependency Graph

Dependencies flow downward. A fix at level N is blocked until all fixes in level N-1 are complete.

```
Level 0 (Foundation)
  │
  ├── F1  WebSocket transport
  │       No deps — new crate dep + struct implementation
  │
  Level 1 (Backend integration)
  │
  ├── F2  BackendFactory implementation
  │       Depends on: F1 (connect needs real transport)
  │
  F1 + F2 are the TWO CRITICAL FIXES.
  Together they unlock every real browser session.
  │
  Level 2 (Page lifecycle)
  │
  ├── F3  Frame tracking for CdpPage (id/url/title/frames)
  │       Depends on: F1 (needs CDP I/O to query frame tree)
  │
  Level 3 (DOM operations)
  │
  ├── F4  Element state detection (visible/enabled/checked/selected/stable)
  │       Depends on: F1 (needs CDP I/O: Runtime.evaluate, DOM.getBoxModel)
  │
  ├── F11 FrameHandle ↔ ElementHandle generation coordination
  │       Depends on: F3 (frame tracking must work first)
  │
  ├── F12 PageHandle generation counter
  │       Depends on: F3 (needs to hook into frame navigation lifecycle)
  │
  Level 4 (Domain crates)
  │
  ├── F6  Dialog port (Page.javascriptDialogOpening listener)
  │       Depends on: F1 (needs CDP event subscription)
  │
  ├── F7  Download port (Browser.setDownloadBehavior, etc.)
  │       Depends on: F1
  │
  ├── F8  Network interception (Fetch.enable, Fetch.requestPaused)
  │       Depends on: F1
  │
  ├── F9  browseros-storage crate
  │       Depends on: nothing (implements StoragePort trait, no CDP dep in crate itself)
  │
  ├── F10 browseros-network crate
  │       Depends on: F8 (network crate wraps CdpNetworkPort)
  │
  Level 5 (Cross-cutting)
  │
  ├── F5  Event type alignment
  │       Depends on: F3 (navigation events need correct IDs)
  │
  ├── F13 Integration tests
  │       Depends on: F1+F2 (tests need real CDP or realistic mock)
  │
  ├── F14 Shutdown sequence
  │       Depends on: nothing (standalone fix in runtime)
  │
  ├── F15 selector_filter fix
  │       Depends on: F4 (filtering needs element state for CSS matching)
  │
  └── F16 Documentation cleanup
          Depends on: all above (document final state)
```

### Critical Path to First Browser Session

The absolute minimal fix set:

```
F1 → F2 → [first browser session]
```

No other fix is required to see `BrowserManager::launch()` succeed, a `BrowserHandle` returned, and a real Chrome process running with CDP communication.

---

## 3. Milestones

### Milestone 1 — Browser Launches

**Fixes required:** F1 (WebSocket transport), F2 (BackendFactory)

**Tasks:**

| # | Task | Crate | Effort |
|---|------|-------|--------|
| 1.1 | Add `tungstenite` dependency to `browseros-cdp/Cargo.toml` | browseros-cdp | 5 min |
| 1.2 | Implement `WebSocketTransport` in `browseros-cdp/src/transport_ws.rs`: `connect()` opens WS handshake, `send()` writes JSON frame, `receive()` reads frames with timeout, `close()` sends close frame, `is_connected()` checks state | browseros-cdp | 4-6 hrs |
| 1.3 | Create `CdpBackendFactory` struct (new file `browseros-cdp/src/factory.rs`) implementing `BackendFactory` from `browseros-bridge`: `name()` → `"chromium"`, `launch()` → spawn Chrome via `std::process::Command`, parse `DevTools listening on ws://...` from stderr, connect via `WebSocketTransport` and return `CdpBrowserProcess`; `connect()` → same path without spawning | browseros-cdp | 3-4 hrs |
| 1.4 | Wire `CdpBackendFactory` into `browseros-cdp/src/lib.rs` as public export | browseros-cdp | 15 min |

**Acceptance criteria:**
- [ ] `Cargo build` and `cargo clippy` pass with zero warnings
- [ ] `WebSocketTransport` unit tests: connect to known endpoint, send message, receive response, close, re-connect
- [ ] `CdpBackendFactory` unit tests: `name()`, factory create → connect returns `BrowserPort`
- [ ] Integration: `BrowserProcess::launch()` spawns Chrome, `WebSocketTransport` connects to the endpoint
- [ ] `BrowserManager::register_backend(CdpBackendFactory::new())` succeeds
- [ ] `BrowserManager::launch(LaunchOptions::default())` returns `Ok(BrowserHandle)` (verified with real Chrome binary or Chrome for Testing)

---

### Milestone 2 — Connect to CDP

**Fixes required:** F1 (already done in M1)

**Tasks:**

| # | Task | Crate | Effort |
|---|------|-------|--------|
| 2.1 | Verify `CdpConnection` reader thread processes incoming CDP messages from real WebSocket | browseros-cdp | 1-2 hrs |
| 2.2 | Verify response ID matching works end-to-end (send `Browser.getVersion`, receive typed response) | browseros-cdp | 1 hr |
| 2.3 | Verify CDP event dispatch (`on_event`, `on_any_event`) receives real events from Chrome | browseros-cdp | 1 hr |

**Acceptance criteria:**
- [ ] `CdpConnection::connect("ws://127.0.0.1:XXXXX")` succeeds against real Chrome
- [ ] `connection.send("Browser.getVersion", None)` returns a `serde_json::Value` with `product`, `userAgent`, `jsVersion` fields
- [ ] `connection.on_event("Target.targetCreated", handler)` fires when a new tab is opened
- [ ] `connection.is_connected()` returns true while WebSocket is open, false after close
- [ ] `connection.close()` sends WebSocket close frame
- [ ] All unit tests pass with `MockTransport` (must not regress)

---

### Milestone 3 — Open a Page

**Fixes required:** F1, F2

**Tasks:**

| # | Task | Crate | Effort |
|---|------|-------|--------|
| 3.1 | Verify `CdpBrowserProcess::create_session()` → `CdpSessionPort` works with real CDP | browseros-cdp | 1 hr |
| 3.2 | Verify `CdpSessionPort::create_page()` → `CdpPage` works | browseros-cdp | 1 hr |
| 3.3 | Verify `BrowserHandle::new_session()` → `SessionHandle` → `new_page()` → `PageHandle` returns valid handle | browseros-browser | 1 hr |

**Acceptance criteria:**
- [ ] `BrowserHandle::new_session(SessionConfig::default())` returns `Ok(SessionHandle)`
- [ ] `SessionHandle::new_page()` returns `Ok(PageHandle)`
- [ ] `PageHandle::id()` returns a non-default `PageId`
- [ ] A real Chrome window/tab is visible (non-headless) or created (headless)
- [ ] Closing the `PageHandle` closes the tab

---

### Milestone 4 — Navigate

**Fixes required:** F1, F2, F3

**Tasks:**

| # | Task | Crate | Effort |
|---|------|-------|--------|
| 4.1 | Implement `CdpPage::url()` — store last-known URL from `Page.navigate` result + subscribe to `Page.frameNavigated` | browseros-cdp | 2-3 hrs |
| 4.2 | Implement `CdpPage::title()` — evaluate `document.title` via CDP or subscribe to `Page.frameStartedLoading`/`Page.frameStoppedLoading` | browseros-cdp | 1-2 hrs |
| 4.3 | Implement `CdpPage::frames()` — query `Page.getFrameTree` on CDP, recursively build child frame list | browseros-cdp | 3-4 hrs |
| 4.4 | Implement `CdpPage::main_frame()` — return `CdpFrame` for the root frame | browseros-cdp | 1 hr |

**Acceptance criteria:**
- [ ] `PageHandle::navigate("https://example.com")` returns `Ok(NavigationState)` with URL `https://example.com/`
- [ ] `PageHandle::url()` returns `https://example.com/` after navigation completes
- [ ] `PageHandle::title()` returns the page `<title>` content
- [ ] `PageHandle::frames().len()` > 0 for pages with iframes
- [ ] Navigation timeout returns error (test with unreachable host)
- [ ] `go_back()`/`go_forward()` change URL correctly

---

### Milestone 5 — Query DOM

**Fixes required:** F1, F2, F3, F4

**Tasks:**

| # | Task | Crate | Effort |
|---|------|-------|--------|
| 5.1 | Implement `CdpElement::is_visible()` — check `getBoxModel` + `offsetParent !== null` via JS evaluate | browseros-cdp | 2-3 hrs |
| 5.2 | Implement `CdpElement::is_enabled()` — check `!disabled` attribute via JS evaluate | browseros-cdp | 1 hr |
| 5.3 | Implement `CdpElement::is_checked()` — read `checked` property for checkboxes/radio via JS evaluate | browseros-cdp | 1 hr |
| 5.4 | Implement `CdpElement::is_selected()` — read `selected` property for `<option>` via JS evaluate | browseros-cdp | 1 hr |
| 5.5 | Implement `CdpElement::is_stable()` — check `getBoxModel` dimensions + CSS transition state via JS evaluate | browseros-cdp | 2-3 hrs |
| 5.6 | Verify end-to-end: locate element via `query_selector`, check all state properties, compare with browser reality | browseros-cdp | 1 hr |

**Acceptance criteria:**
- [ ] `element.is_visible()` matches CSS `visibility` + `display` + `opacity` state
- [ ] `element.is_enabled()` returns `false` for `<button disabled>`, `true` otherwise
- [ ] `element.is_checked()` returns `true` for `<input type="checkbox" checked>`
- [ ] `element.is_stable()` returns `false` for animated/transitioning elements
- [ ] `element.text_content()` returns visible text content
- [ ] `element.bounding_box()` returns correct coordinates
- [ ] Element state methods work on elements inside Shadow DOM

---

### Milestone 6 — Receive Events

**Fixes required:** F1, F2, F3, F5, F13

**Tasks:**

| # | Task | Crate | Effort |
|---|------|-------|--------|
| 6.1 | Audit all event structs in `browseros-browser::events`, `browseros-dom::events`, and `event-model.md`; identify every type mismatch | cross-crate | 2-3 hrs |
| 6.2 | Choose a canonical type for each duplicated event (prefer domain crate's version over central model) | cross-crate | 1 hr |
| 6.3 | Update `event-model.md` to use typed identifiers (`RequestId`, `NavigationId`) instead of `String` | docs | 1 hr |
| 6.4 | Verify `BrowserStarted`, `SessionCreated`, `PageCreated`, `NavigationFinished` events are emitted from the correct crate | browseros-browser | 1 hr |
| 6.5 | Write integration test: create `EventBus`, subscribe to `browser.started`, call `BrowserManager::launch`, verify event received | browseros-browser, browseros-event-bus | 2-3 hrs |

**Acceptance criteria:**
- [ ] `event-model.md` uses `RequestId` (UUID v7) not `String` for `RequestStarted`
- [ ] Navigation events are defined in exactly one place (not duplicated across `event-model.md`, `browseros-browser::events`, `network-event-model.md`)
- [ ] Integration test: `bus.subscribe("browser.started", handler)` receives `BrowserStarted` after `BrowserManager::launch()`
- [ ] Integration test: `bus.subscribe("navigation.finished", handler)` receives `NavigationFinished` after `PageHandle::navigate()`
- [ ] All event payloads implement `Event` trait with correct `kind()` strings
- [ ] No breaking changes to existing event subscribers

---

### Milestone 7 — Network Interception

**Fixes required:** F1, F2, F8, F10

**Tasks:**

| # | Task | Crate | Effort |
|---|------|-------|--------|
| 7.1 | Implement `CdpNetworkPort::add_interception_rule()` — send `Fetch.enable` with URL pattern + resource type filter; map returned `Fetch.requestPaused` events to `InterceptionHandle` | browseros-cdp | 3-4 hrs |
| 7.2 | Implement `CdpNetworkPort::remove_interception_rule()` — send `Fetch.continueRequest`/`Fetch.fulfillRequest` as needed; disable pattern | browseros-cdp | 1-2 hrs |
| 7.3 | Create `browseros-network` crate (skeleton + workspace member) that wraps `NetworkPort` and adds `RequestTracker`, event payloads per frozen design | browseros-network (new) | 4-6 hrs |
| 7.4 | Wire network events through `EventBus`: `RequestStarted`, `ResponseReceived`, `RequestFinished`, `RequestFailed` | browseros-network | 2-3 hrs |

**Acceptance criteria:**
- [ ] `add_interception_rule(InterceptionRule { url_pattern: "*.png", action: Block })` blocks PNG images
- [ ] `remove_interception_rule(handle)` removes the rule, PNG images load again
- [ ] `browseros-network` crate compiles with `cargo build`
- [ ] `network.request_started` event fires when a page makes any HTTP request
- [ ] `network.request_failed` event fires for blocked/errored requests
- [ ] `browseros-network` has no EventBus dependency (payloads only per frozen design)

---

### Milestone 8 — End-to-End Smoke Test

**Fixes required:** F1–F8, F10, F13

**Tasks:**

| # | Task | Crate | Effort |
|---|------|-------|--------|
| 8.1 | Write smoke test (integration test, real Chrome or Chrome for Testing): launch → connect → open page → navigate to page → query DOM → verify element visibility → receive navigation event → capture network request | cross-crate | 4-6 hrs |
| 8.2 | Ensure test runs in CI without human interaction (headless Chrome, known-good binary path) | cross-crate | 2-3 hrs |
| 8.3 | Document test as the canonical "hello world" example for BrowserOS | docs | 1 hr |

**Acceptance criteria:**
- [ ] Smoke test passes on all 3 platforms (Windows, macOS, Linux) or documents platform-specific constraints
- [ ] Test runs headless (no visible window)
- [ ] Test completes in under 30 seconds
- [ ] Test verifies every major API path exercised by a simple agent:
  - `BrowserManager::launch()` → `BrowserHandle`
  - `BrowserHandle::new_session()` → `SessionHandle`
  - `SessionHandle::new_page()` → `PageHandle`
  - `PageHandle::navigate("https://example.com")` → navigation completes
  - `PageHandle::url()` → returns final URL
  - `PageHandle::title()` → returns page title
  - `PageHandle::evaluate("document.title")` → same title
  - `page.locator().css("h1").resolve()` → `ElementHandle`
  - `element.is_visible()` → `true`
  - `element.text_content()` → non-empty string
  - Network request observed via event subscription or network capture

---

## 4. Effort Summary

| Milestone | Critical Path? | Fixes | Estimated Effort |
|-----------|---------------|-------|-----------------|
| M1 — Browser Launches | YES (foundation) | F1, F2 | 8-11 hrs |
| M2 — Connect to CDP | YES (verification) | F1 (done) | 3-4 hrs |
| M3 — Open a Page | YES (verification) | F1, F2 (done) | 3 hrs |
| M4 — Navigate | YES | F3 | 7-10 hrs |
| M5 — Query DOM | YES | F4 | 8-11 hrs |
| M6 — Receive Events | NO (but needed for agents) | F5, F13 | 7-9 hrs |
| M7 — Network Interception | NO (needed for complex agents) | F8, F10 | 9-13 hrs |
| M8 — End-to-End Smoke Test | YES (verification) | all above | 7-10 hrs |
| **Total** | | **F1–F15** | **52-71 hrs** |

### Critical Path (M1 → M3 → M5 → M8)
Minimum to demonstrate a real browser session: **~26-36 hrs**

### Full GO
All milestones including network interception and event flow: **~52-71 hrs**

---

## 5. Exclusions

The following are explicitly **not in scope** for this remediation plan:

| Item | Reason |
|------|--------|
| `browseros-input` crate (design-only) | Not required for core browser session. `InputPort` is implemented in `browseros-cdp::CdpInputPort`. |
| `browseros-artifact` crate (design-only) | Not required. `ArtifactPort` trait exists in bridge. |
| `browseros-plugin` crate (design-only) | Phase 3 domain. Not required for GO. |
| Async runtime support | Deliberately excluded by Phase 2 architecture. |
| AI-assisted locators | Phase 3+ feature. |
| `BrowserOsError` refactoring | Not required for GO. |
| CDP sub-crate splitting | Future concern when CDP exceeds 20 modules. |
| EventBus priority/TTL | Future concern. Current sync dispatch is sufficient. |

---

## 6. Risk Register for the Plan Itself

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Chrome binary not found in CI | High | High | Use `chrome-launcher` crate or auto-download Chrome for Testing |
| WebSocket library incompatibility (Windows vs Unix) | Medium | Medium | `tungstenite` is cross-platform tested |
| CDP protocol differences across Chrome versions | Medium | Medium | Use schema-driven deserialization; run CI against stable Chrome |
| Multi-threading issues in CDP reader thread | Low | High | Existing `Arc<Mutex<>>` patterns already work; add timeout bounds |
| Flaky integration tests (timing-dependent) | Medium | Medium | Add retry logic; use explicit waits not sleeps |

---

## 7. GO Checklist

- [ ] **M1:** `CdpBackendFactory` + `WebSocketTransport` merged; `cargo build` passes
- [ ] **M2:** Live CDP communication verified against Chrome (manually or via CI)
- [ ] **M3:** `BrowserHandle::new_session()` → `PageHandle` returned
- [ ] **M4:** `PageHandle::navigate(url)` succeeds; `url()` / `title()` return correct values
- [ ] **M5:** `ElementHandle::is_visible()` / `is_enabled()` / `is_checked()` / `is_selected()` / `is_stable()` return correct values
- [ ] **M6:** Event type mismatch resolved; integration test verifies EventBus event flow
- [ ] **M7:** Network interception functional; `browseros-network` crate compiles
- [ ] **M8:** End-to-end smoke test passes in CI

When all 8 milestones are green: **GO** — Phase 2 is implementation-ready for production.

