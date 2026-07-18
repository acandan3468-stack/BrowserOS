# BrowserOS — Workspace Dependency Graph Analysis

> **Audit Date:** 2026-07-18  
> **Scope:** All 17 workspace crates  

---

## 1. Full Dependency Graph (Compile-Time)

```
browseros-types
  └─ (serde, serde_json, uuid, chrono, thiserror) — no workspace deps

browseros-config
  └─ browseros-types

browseros-observability
  └─ browseros-types, chrono, serde, serde_json, tracing, tracing-subscriber

browseros-event-bus
  └─ browseros-types

browseros-lifecycle
  └─ browseros-types, browseros-event-bus

browseros-scheduler
  └─ browseros-types, browseros-event-bus, chrono, uuid

browseros-bridge
  └─ browseros-types

browseros-runtime
  └─ browseros-types, browseros-config, browseros-observability, browseros-event-bus,
     browseros-lifecycle, browseros-scheduler, browseros-dag

browseros-page
  └─ browseros-types, browseros-browser, browseros-cdp

browseros-dom
  └─ browseros-types, browseros-bridge

browseros-storage
  └─ browseros-types, browseros-config, serde, serde_json, sled

browseros-cdp
  └─ browseros-types, browseros-bridge, browseros-observability, serde, serde_json,
     reqwest, tungstenite, url, base64, sha2

browseros-browser
  └─ browseros-types, browseros-config, browseros-cdp, browseros-bridge,
     browseros-observability, serde, serde_json, reqwest, uuid, which

browseros-llm
  └─ browseros-types, browseros-event-bus, browseros-observability, serde, serde_json,
     ureq, uuid, chrono, rust-embed, base64, sha2

browseros-dag
  └─ browseros-types, browseros-event-bus

browseros-stress-tests
  └─ (dev-deps only): browseros-types, browseros-config, browseros-observability, etc.

browseros-mcp
  └─ browseros-types, browseros-config, browseros-observability, browseros-event-bus,
     browseros-lifecycle, browseros-scheduler, browseros-runtime, browseros-bridge,
     browseros-browser, browseros-page, browseros-cdp, browseros-dom, browseros-storage,
     browseros-dag, browseros-llm, serde, serde_json, uuid, chrono, ureq
```

---

## 2. Layer Diagram

```
Layer 0 — Foundation
  ┌──────────────────┐
  │  browseros-types  │  (zero workspace deps)
  └────────┬─────────┘
           │
  ┌────────┴─────────┐  ┌──────────────────────┐
  │ browseros-config  │  │ browseros-observability│
  └────────┬─────────┘  └──────────┬───────────┘
           │                       │
           └───────────┬───────────┘
                       │
Layer 1 — Infrastructure
           │
  ┌────────┴─────────┐  ┌──────────────────┐  ┌──────────────────┐
  │ browseros-event-bus│  │ browseros-bridge  │  │ browseros-lifecycle│
  └────────┬─────────┘  └──────────────────┘  └────────┬─────────┘
           │                                            │
  ┌────────┴─────────┐  ┌──────────────────┐           │
  │ browseros-scheduler│  │  browseros-dag   │           │
  └────────┬─────────┘  └────────┬─────────┘            │
           │                     │                      │
           └──────────┬──────────┘──────────────────────┘
                      │
Layer 2 — Domain
                      │
  ┌───────────────────┼───────────────────┐
  │                   │                   │
  ┌┴─────────┐  ┌──────┴───────┐  ┌──────┴───────┐
  │browseros-cdp│  │browseros-dom │  │browseros-storage│
  └─────┬─────┘  └──────────────┘  └──────────────┘
        │
  ┌─────┴──────┐  ┌──────────────┐  ┌──────────────┐
  │browseros-browser│  │browseros-llm│  │browseros-page │
  └──────┬─────┘  └──────┬───────┘  └──────┬───────┘
         │                │                 │
         └────────────────┼─────────────────┘
                          │
Layer 3 — Shell
                          │
  ┌───────────────────────┴──────────────────┐
  │              browseros-mcp                │  (9 workspace deps)
  └──────────────────────────────────────────┘
```

---

## 3. Dependency Metrics

| Metric | Value |
|--------|-------|
| Total crates | 17 |
| Non-test crates | 16 (stress-tests is test-only) |
| Layers | 4 (Foundation, Infrastructure, Domain, Shell) |
| Cyclic dependencies | **0** |
| Forbidden dependencies | **0** |
| Heaviest consumer | `browseros-mcp` (9 paths) |
| Lightest consumers | `browseros-dag`, `browseros-dom`, `browseros-storage` (1–3 paths) |
| Most depended-upon | `browseros-types` (10 dependents) |
| Edge weight overhead (unused path deps) | **0** — all workspace deps are exercised in `use` statements |

---

## 4. Dependency Types

| Source | Target | Edge Type | Layer Transition | Status |
|--------|--------|-----------|------------------|--------|
| config | types | compile | 0→0 | ✅ Normal |
| observability | types | compile | 0→0 | ✅ Normal |
| event-bus | types | compile | 1→0 | ✅ Normal |
| lifecycle | types, event-bus | compile | 1→0, 1→1 | ✅ Normal |
| scheduler | types, event-bus | compile | 1→0, 1→1 | ✅ Normal |
| bridge | types | compile | 1→0 | ✅ Normal |
| runtime | types, config, observability, event-bus, lifecycle, scheduler, dag | compile | 1→0, 1→0, 1→0, 1→1, 1→1, 1→1, 2→1 | ✅ Normal (composition root) |
| cdp | types, bridge, observability | compile | 2→0, 2→1, 2→0 | ✅ Normal |
| browser | types, config, cdp, bridge, observability | compile | 2→0, 2→0, 2→2, 2→1, 2→0 | ✅ Normal |
| page | types, browser, cdp | compile | 2→0, 2→2, 2→2 | ✅ Normal |
| dom | types, bridge | compile | 2→0, 2→1 | ✅ Normal |
| storage | types, config | compile | 2→0, 2→0 | ✅ Normal |
| dag | types, event-bus | compile | 2→0, 2→1 | ✅ Normal |
| llm | types, event-bus, observability | compile | 2→0, 2→1, 2→0 | ✅ Normal |
| mcp | 9 workspace crates | compile | 3→all | ✅ Normal (shell) |

---

## 5. Phase 6+ Impact Analysis

### Adding new crates (estimated ~8 new crates):

| New Crate | Layer | Would Depend On | Risk |
|-----------|-------|-----------------|------|
| browseros-network | Domain | types, bridge, config, observability | Low — follows dom/browser pattern |
| browseros-planner | Domain | types, event-bus, scheduler, dag, runtime | Low — planner is a consumer |
| browseros-session | Infrastructure | types, event-bus | Low — small, isolated |
| browseros-plugin-core | Domain | types, event-bus, dag | Low — similar to dag |
| browseros-plugin-* | Domain | plugin-core, depends on feature | Low — isolated |
| browseros-workflow | Domain | types, event-bus, dag, scheduler, plugin-core | Medium — aggregator |
| browseros-remote | Domain | types, bridge, config | Low — network boundary |
| browseros-dist-runtime | Infrastructure | types, event-bus, scheduler, remote | Low — coordinator |

### Risk analysis for new edges:
- No crate would depend on `browseros-mcp` (MCP is terminal) — ✅
- No crate would create cycles — ✅ (new crates only consume, not produce circular references)
- `browseros-runtime` already serves as the composition root; new crates add fields to `RuntimeContext` — ⚠️ acceptable

### Forbidden patterns (none violated):
1. No crate may depend on `browseros-mcp` (shell must not be imported by domain)
2. No crate in Layer 2 may depend on `browseros-runtime` (runtime is a composition root for the shell only)
3. No crate may depend on `browseros-stress-tests` (dev-dependency only)

---

## 6. Recommendations

1. MAINTAIN current dependency stratification when adding new crates — keep Foundation light, Infrastructure focused, Domain isolated, Shell thin.
2. CONSIDER a `browseros-core` umbrella crate for the most common re-export pattern (types + config + event-bus) if MCP re-export chain grows beyond 9 path deps.
3. AUDIT `browseros-config` dependency in `browseros-browser` — it imports config at compile time for browser-level configuration, which is correct.
