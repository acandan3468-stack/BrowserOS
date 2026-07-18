ARCHIVED  
  
This document has been archived. Its content has been merged into the canonical documentation set.  
See .vibe/workspace/architecture/architecture-v2.md, .vibe/workspace/architecture/design-decisions.md, etc. for current information.
  
---  
  
# Vibe Coder Kit — Agent Instructions

## Quick Start

1. Read `.vibe/config.json` for project configuration
2. Read `.vibe/state/derived-state.json` for current state
3. Read the relevant flow file from `.vibe/flows/`
4. Follow the steps in the flow file
5. Update state after completing each step

## Core Rules

### Rule 1: Safety First
- Never expose secrets in code
- Never run destructive commands without approval
- Always validate user input

### Rule 2: State Management
- Use EventStore for all state changes
- Never edit derived-state.json directly
- Append events, don't modify

### Rule 3: Workflow
- Follow the DAG in phase-graph.json
- Validate transitions before executing
- Respect entry/exit criteria

### Rule 4: Quality
- Write tests before code (TDD)
- Maintain > 80% coverage
- Run lint and type checks

### Rule 5: Knowledge
- Document decisions in knowledge base
- Update INDEX.md after adding entries
- Capture lessons learned

### Rule 6: Honesty
- Say "I don't know" when uncertain
- Present pros and cons
- State assumptions clearly

### Rule 7: Security
- Use security rules plugin
- Scan code before committing
- Never commit secrets

### Rule 8: Team
- Respect role-based access
- Follow governance rules
- Document for others

## File Structure

```
.vibe/
├── config.json           # Project configuration
├── phase-graph.json      # Workflow DAG
├── core/                 # Core modules
│   ├── index.ts          # Main exports
│   ├── event-store.ts    # State management
│   ├── dag.ts            # Workflow engine
│   ├── plugin-registry.ts
│   ├── circuit-breaker.ts
│   ├── saga.ts
│   ├── idempotency.ts
│   ├── validator.ts
│   ├── cli.ts
│   ├── telemetry.ts
│   ├── health-check.ts
│   ├── cost-tracker.ts
│   ├── knowledge-store.ts
│   └── team-config.ts
├── flows/                # Workflow definitions
├── plugins/              # Rule plugins
├── behaviors/            # Behavior protocols
├── state/                # Event store
├── memory/               # Knowledge base
├── workspace/            # Working files
└── templates/            # CI/CD templates
```

## Commands

- `vibe init` — Initialize project
- `vibe status` — Show current status
- `vibe doctor` — Health check
- `vibe rollback` — Rollback last action

## Anchored Summary

### Goal
- Complete `browseros-dom` implementation, pass architecture audit against 6 frozen docs, then freeze Phase 2.5 and design Phase 2.6 (`browseros-network`).

### Constraints
- All mandatory fixes: generation-based stale detection, frozen event model, `#[non_exhaustive]` on DomError, missing API methods, snapshot parameters, id.rs module
- Do NOT add new features, redesign, or proceed to next phase until fixes are verified
- Preserve frozen public APIs, crate boundaries, protocol isolation
- No EventBus or runtime dependencies in DOM or Network crates
- Network crate: browser-agnostic, zero CDP in public API, bridge-first, plugin-safe, thread-safe by design, public API minimized

### Progress
**Done:**
- Architecture audit against all 6 frozen docs: identified C1, C2, H1–H4, M1–M6
- C1 — Generation-based stale detection: `known_generation: u64` in ElementHandle, `check_stale()`, `is_stale()`, all 6 struct literals updated
- C2 — Frozen event model: rewrote `events.rs` with exact 30 payload types from `dom-event-model.md`, removed implementation-specific naming, removed undocumented correlation fields, added `DomOperation` enum
- H1 — `#[non_exhaustive]` on DomError: added attribute, added `ClosedShadowRoot(String)` and `CrossOriginFrame` variants
- H2 — Missing API methods: `ShadowRootHandle::is_closed()`, `FrameHandle::is_cross_origin()`
- H3 — Snapshot API parameters: `FrameHandle::snapshot(max_depth, selector_filter)`, `snapshot_all()`, `ElementHandle::snapshot_with_depth()`, `NodeSnapshot::from_node_info_depth()`
- H4 — `id.rs` module: created, re-exports `HandleId` from `browseros-types`, registered in `lib.rs`
- Verification: `cargo build` clean, `cargo clippy` 0 warnings, `cargo fmt --check` clean, `cargo test` 0/0 passing
- M1/M3/M4 triage: all classified SAFE TO DEFER (POST-PHASE-2 TECHNICAL DEBT for M1, SAFE TO DEFER for M3/M4)
- Phase 2.5 APPROVED FOR FREEZE
- **Phase 2.6 Design (COMPLETED):** 6 documents produced for `browseros-network`:
  - `docs/phase2.6-network-architecture.md` — 12-module layout, dependency graph, ownership model, lifecycle (request/interception/websocket/download), thread model, boundary clarifications
  - `docs/network-api-design.md` — `NetworkHandle`, `RequestId` (UUID v7), `InterceptionBuilder`, `CookieManager`, `NetworkConditionBuilder`, `WebSocketInfo`, HAR types (gated), `NetworkError` with `#[non_exhaustive]`
  - `docs/network-event-model.md` — `NetworkEvent` enum with 29 variants across 9 categories (Request Lifecycle, Response Lifecycle, Interception, Navigation, WebSocket, Download, Network Conditions, Auth, Cache), all payloads defined
  - `docs/network-lifetime-model.md` — request identity, redirect chain tracking (new RequestId per hop, linked via `original_request_id`), completion/failure/abortion lifecycles, streaming exclusion rationale, cancellation semantics, no stale detection
  - `docs/network-freeze-plan.md` — SAFE TO FREEZE (NetworkHandle, RequestId, Interception*), KEEP FLEXIBLE (CookieManager, WebSocketInfo, HAR), EXPERIMENTAL (Auth events, streaming, mocking)
  - `docs/network-risk-analysis.md` — 16 risks across architecture/implementation/protocol/concurrency/future-compatibility categories, each with severity/likelihood/mitigation
- Internal architectural consistency review: 9 PASS / 1 FAIL (fixed boundary table directionality)
- **Phase 2.6 Design Revision (v2):** Resolved all 5 blocking audit issues:
  - C1 (Redirect RequestId): new RequestId per hop + `original_request_id` chain
  - C2 (Auth credential gap): demoted auth to EXPERIMENTAL, documented no credential delivery
  - H1 (NavigationId): added `navigation_id: Option<NavigationId>` to all 3 navigation payloads
  - H2 (CookieManager::delete): added `url: &str` parameter to match bridge
  - H3 (Dead RequestState::Redirected): removed from enum

### In Progress
- (none — Phase 2.5 frozen, Phase 2.6 designed)

### Blocked
- (none)

### Key Decisions
- `known_generation: u64` captured at creation time in `ElementHandle::new()`, preserved in clone and all child handle constructions
- `is_stale()` returns `true` when `known_generation != current_generation` (no backend call)
- `check_stale()` returns `DomError::StaleElement(handle_id)` on mismatch
- Event payloads follow `dom-event-model.md` exactly: 30 event variants, `DomOperation` enum matching frozen spec
- `FrameHandle::is_cross_origin()` returns `false` — placeholder awaiting bridge extension
- `RequestId` defined in-network (UUID v7, not in `browseros-types`) to keep types crate minimal
- HAR data structures gated behind `#[cfg(feature = "har")]` — KEEP FLEXIBLE
- Auth challenge types classified EXPERIMENTAL — Basic/Digest captured, NTLM/certificate deferred
- No response body streaming in Phase 2.6 — consciously excluded; future opt-in `BodyCaptureMode`
- Network crate: zero runtime dependencies beyond `browseros-types`, `browseros-bridge`, `chrono`, `thiserror`
- Interception rule priority: registration order is backend-defined; documented as such; no priority field in Phase 2.6

### Next Steps
- Phase 2.6 design is complete. No Rust implementation yet. User may approve freezing the design or request revisions before Phase 2.6.1 implementation begins.

### Relevant Files
- `browseros/browseros-dom/src/element.rs` — ElementHandle with `known_generation`, `check_stale()`, `is_stale()`, `snapshot_with_depth()`
- `browseros/browseros-dom/src/events.rs` — `DomEvents` enum matching frozen model
- `browseros/browseros-dom/src/error.rs` — `#[non_exhaustive]` on DomError, `ClosedShadowRoot`, `CrossOriginFrame`
- `browseros/browseros-dom/src/shadow.rs` — `ShadowRootHandle::is_closed()`
- `browseros/browseros-dom/src/frame.rs` — `FrameHandle::is_cross_origin()`, `snapshot(max_depth, selector_filter)`, `snapshot_all()`
- `browseros/browseros-dom/src/snapshot.rs` — `NodeSnapshot::from_node_info_depth()`
- `browseros/browseros-dom/src/id.rs` — new module, re-exports `HandleId`
- `browseros/browseros-dom/src/lib.rs` — `pub mod id;`, `pub use id::HandleId;`
- `.vibe/workspace/design/network-architecture.md` — DESIGN: 12-module layout, ownership, lifecycle
- `.vibe/workspace/design/network-api-design.md` — DESIGN: NetworkHandle, RequestId, InterceptionBuilder, CookieManager, HAR, NetworkError
- `.vibe/workspace/design/network-event-model.md` — DESIGN: 28 NetworkEvent variants, 9 categories
- `.vibe/workspace/design/network-lifetime-model.md` — DESIGN: request lifecycle, redirects, streaming exclusion
- `.vibe/workspace/design/network-freeze-plan.md` — DESIGN: SAFE/KEEP/EXPERIMENTAL classification
- `.vibe/workspace/design/network-risk-analysis.md` — DESIGN: 14 risks with mitigations
- `.vibe/workspace/architecture/architecture-v2.md` — CANONICAL: Single source of truth for architecture
- `.vibe/workspace/architecture/architecture-invariants-v2.md` — CANONICAL: 37 code-verified invariants
- `.vibe/workspace/architecture/design-decisions.md` — CANONICAL: 21 ADRs
- `.vibe/workspace/architecture/phase1-freeze.md` — CANONICAL: Frozen API declaration
