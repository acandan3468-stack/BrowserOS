# Phase 2 — Foundation Architecture

**Status:** Implemented  
**Design Freeze:** Phase 1 crates are frozen. Phase 2 architecture implemented.  
**Goal:** Transform BrowserOS from a generic runtime fabric into a production browser agent platform.

---

## 1. Architecture Philosophy

BrowserOS follows a **layered bridge** pattern:

```
┌──────────────────────────────────────────────────┐
│                   Agents / CLI / API              │
├──────────────────────────────────────────────────┤
│             High-Level Subsystems                 │
│  DOM │ Page │ Storage                             │
├──────────────────────────────────────────────────┤
│             Bridge Layer (Traits)                  │
│   BrowserPort │ SessionPort │ PagePort │ FramePort│
├──────────────────────────────────────────────────┤
│          Protocol Implementation (CDP)            │
├──────────────────────────────────────────────────┤
│             Chrome / Chromium                      │
└──────────────────────────────────────────────────┘
```

### Key Principles

1. **Protocol independence.** The bridge layer is pure traits. No CDP type ever leaks into any crate except `browseros-cdp`.
2. **Synchronous core.** Phase 1 established a synchronous (std-thread) runtime. Phase 2 respects this. All browser operations block their calling thread — the agent model explicitly expects this.
3. **Event-driven observation.** Browser state changes → EventBus events. Agents observe through events, command through API calls.
4. **No async runtime added.** Chromium's CDP is inherently asynchronous (events fire asynchronously), but the *consumer* interface is synchronous. Internal bridging threads handle CDP→EventBus translation.
5. **RuntimeContext is the composition root.** Every subsystem is reachable through RuntimeContext. No global statics, no DI container.

---

## 2. Subsystem Inventory

| # | Subsystem | Crate | Status | Role |
|---|-----------|-------|--------|------|
| 1 | Bridge Interfaces | `browseros-bridge` | ✅ Implemented | Pure trait definitions and value types |
| 2 | CDP Protocol | `browseros-cdp` | ✅ Implemented | Chrome DevTools Protocol transport + command mapping |
| 3 | Browser Management | `browseros-browser` | ✅ Implemented | Browser lifecycle, sessions, tab management |
| 4 | DOM Model | `browseros-dom` | ✅ Implemented | DOM tree, locators, element handles, shadow DOM |
| 5 | Page Operations | `browseros-page` | ✅ Implemented | Navigation, screenshots, PDF, JS execution, dialogs |
| 6 | Storage | `browseros-storage` | ✅ Implemented | Cookies, LocalStorage, SessionStorage |
| 7 | Network | `browseros-network` | 📐 Design only | Request/response monitoring, interception, WebSocket |
| 8 | Input Simulation | `browseros-input` | 📐 Design only | Mouse, keyboard, touch, file upload |
| 9 | Artifact Store | `browseros-artifact` | 📐 Design only | Download manager, trace/recording storage |
| 10 | Plugin System | `browseros-plugin` | 📐 Design only | Dynamic plugin loading, capability negotiation |

---

## 3. Key Design Decisions

### 3.1 Bridge Pattern over Facade

Each bridge trait (`BrowserPort`, `SessionPort`, `PagePort`, `FramePort`, `ElementPort`, `LocatorEngine`) defines a **minimal protocol contract**. The CDP crate implements them. Alternative protocols (e.g., Playwright protocol, WebDriver BiDi) can add new implementation crates without changing any consumer code.

### 3.2 CDP Thread Model

CDP runs over WebSocket. A single `CdpConnection` owns one WebSocket and one reader thread. The reader thread deserializes CDP events and pushes them into an internal channel. A dispatcher thread translates relevant CDP events into BrowserOS events on the EventBus.

```
CDP WebSocket → Reader Thread → Channel → Dispatcher → EventBus
```

This ensures:
- CDP events never block the publisher
- Multiple sessions share one WebSocket connection
- Event translation is off the critical path

### 3.3 Synchronous API, Asynchronous Transport

Bridge trait methods are synchronous (`fn navigate(&self, url: &str) -> Result<NavigationState>`). Internally, the CDP implementation sends a CDP command and **blocks the calling thread** until the response arrives (or timeout). This is safe because:

- Each CDP session has a dedicated command channel
- Response matching uses CDP command IDs
- The blocking is on a per-thread basis — agent and control threads are separate

### 3.4 EventBus Integration Points

Every significant browser state change produces a domain event on the EventBus:

```
page.created → PageCreated { session_id, page_id, url }
navigation.finished → NavigationFinished { page_id, url, status }
request.sent → RequestStarted { page_id, request_id, url, method }
response.received → ResponseReceived { page_id, request_id, status_code }
element.attached → ElementAttached { page_id, node_id, tag_name }
dialog.opened → DialogOpened { page_id, type, message }
download.finished → DownloadFinished { page_id, url, file_path }
...
```

Agents subscribe to these events to react to browser state changes.

---

## 4. Crate Dependency Graph

```
browseros-types (FROZEN)
  ↑
browseros-bridge (depends only on types + serde + thiserror)
  ↑
browseros-cdp (depends on types + bridge)
browseros-dom (depends on types + bridge)
browseros-page (depends on types + bridge + event-bus)
browseros-storage (depends on types + bridge)
  ↑
browseros-browser (depends on types + bridge + cdp + config + event-bus + observability)
  ↑
browseros-runtime (composition root for all Phase 2 crates)
```

**No circular dependencies.** The dependency direction is always: types → bridge → domain crates → browser → runtime.

### Design-only crates (Phase 3+)

The following crates are documented but not yet implemented:

- `browseros-network` — request/response monitoring, interception, WebSocket
- `browseros-input` — mouse, keyboard, touch, file upload
- `browseros-artifact` — download manager, trace/recording storage
- `browseros-plugin` — dynamic plugin loading, capability negotiation

---

## 5. Document Map

| Document | Content |
|----------|---------|
| `phase2-architecture.md` | This file — overall architecture and design decisions |
| `phase2-crate-map.md` | Complete crate layout, API surfaces, dependencies |
| `browser-abstractions.md` | Interface definitions for every bridge trait |
| `runtime-integration.md` | How RuntimeContext connects to Phase 2 subsystems |
| `event-model.md` | Complete runtime event catalog |
| `plugin-extension-model.md` | Plugin system architecture |
| `phase2-risk-analysis.md` | Architectural risks and mitigations |
| `phase2-freeze-plan.md` | Freeze recommendations per subsystem |
