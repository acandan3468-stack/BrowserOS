# Changelog — Phase 2

All notable changes to the BrowserOS Phase 2 implementation.

---

## [2.0.0-rc] — 2026-07-03

### Final Release Candidate — RC Cleanup Complete

#### Fixed
- **5 CDP mock test failures** — root causes were incomplete mock transport responses (missing `DOM.getDocument` calls, wrong response structure), not timing issues
- **17 rustdoc warnings** — converted cross-crate doc links to plain text where link targets are not accessible across crate boundaries
- **4 stale documentation files** — `phase2-architecture.md`, `phase2-crate-map.md`, `phase2-master-audit.md`, `browser-abstractions.md` updated to match actual implementation
- **8 clippy warnings** — `clone_on_copy`, `match_like_matches_macro`, `unnecessary_lazy_evaluations`, `new_without_default` (x2), `type_complexity` (x2), `too_many_arguments` (suppressed)
- **Unused `chrono` dependency** removed from `browseros-storage`

#### Changed
- `browser-abstractions.md` — complete rewrite to match actual bridge trait signatures
- `phase2-architecture.md` — updated status, dependency graph, subsystem inventory
- `phase2-crate-map.md` — corrected `BrowserPort` (added `launch`, `is_alive`), `SessionPort` (added `config`), `DownloadPort` (corrected `cancel_download`, `wait_for_completion`)
- `dom-api-design.md` — corrected `ElementHandle` struct layout (added `locator_engine`, `known_generation`, fixed `generation` type), `TextNode.text` type, `backend_element_port` visibility

#### Verification
- `cargo fmt --check` — clean
- `cargo clippy --workspace` — 0 warnings
- `cargo test --workspace` — 900 tests, 0 failures
- `cargo doc --workspace --no-deps` — 0 warnings

---

## [2.0.0-rc1] — 2026-07-02

### Release Candidate 1

#### Added
- **`browseros-storage` crate** — StorageManager, Cookie CRUD, localStorage, sessionStorage, events
  - 13 unit tests, 6 integration tests
  - `StorageError` with `#[non_exhaustive]`
  - Event payloads: `CookieAdded`, `CookieRemoved`, `StorageCleared`

#### Fixed
- **Documentation audit** — synchronized `phase2-architecture.md`, `phase2-crate-map.md`, `browser-abstractions.md`
- **Unused `chrono` dependency** removed from storage crate

---

## [2.0.0-beta] — 2026-06-30

### Phase 2.5 — DOM Model Complete

#### Added
- **`browseros-dom` crate** — ElementHandle, Locator, Snapshot, ShadowRoot, MutationObserver
  - Generation-based stale detection (`known_generation: u64`)
  - `DomEvent` enum with 30 event payloads matching `dom-event-model.md`
  - `DomError` with `#[non_exhaustive]`, `ClosedShadowRoot`, `CrossOriginFrame` variants
  - `ShadowRootHandle::is_closed()`, `FrameHandle::is_cross_origin()` (placeholder)
  - `FrameHandle::snapshot(max_depth, selector_filter)`, `snapshot_all()`
  - `ElementHandle::snapshot_with_depth()`, `NodeSnapshot::from_node_info_depth()`
  - `HandleId` re-exported via `id.rs` module

#### Verified
- `cargo build` clean
- `cargo clippy` 0 warnings
- `cargo fmt --check` clean
- `cargo test` all passing

---

## [2.0.0-alpha] — 2026-06-25

### Phase 2.4 — Page Operations

#### Added
- `browseros-page` — PageWaiter, DialogAutoHandler, FrameTree, ContentExtractor, NavigationHistory
- Page-level events: DialogOpened, DialogClosed, FrameAttached, FrameDetached, NavigationStarted, NavigationFinished, etc.

---

## [1.5.0] — 2026-06-20

### Phase 2.3 — CDP Protocol Implementation

#### Added
- `browseros-cdp` — WebSocket transport, CdpConnection, CdpSession, all bridge trait implementations
- CDP command modules: page, runtime, dom, target, browser, network, input, storage, css, fetch
- EventDispatcher for CDP event routing
- TargetManager for multi-target session management

---

## [1.0.0] — 2026-06-15

### Phase 2.1–2.2 — Foundation & Bridge

#### Added
- `browseros-types` — Foundation types, identifiers, events, error model
- `browseros-bridge` — 12 protocol-agnostic port traits
- `browseros-browser` — BrowserManager, BrowserHandle, SessionHandle, PageHandle
- `browseros-config` — Multi-source config loader
- `browseros-event-bus` — Typed event bus
- `browseros-observability` — Logger, metrics, tracing
- `browseros-lifecycle` — Component lifecycle state machine
- `browseros-scheduler` — Delayed task scheduling
- `browseros-runtime` — RuntimeContext composition root
