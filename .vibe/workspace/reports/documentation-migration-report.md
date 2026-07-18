# Documentation Migration Report — BrowserOS

**Date:** 2026-07-08  

---

## Workspace Philosophy

```
browseros/     → ONLY code (Rust source, Cargo files, build artifacts)
.vibe/         → ALL knowledge (architecture, ADRs, audits, reports, plans, designs)
```

This separation ensures:
- **Code stays clean** — No documentation clutter in source directories
- **Knowledge stays organized** — All documentation in one place with clear structure
- **Long-term maintainability** — New contributors know exactly where to find information

---

## .vibe/ Structure (After Migration)

```
.vibe/
├── config.json                    # Project configuration
├── phase-graph.json               # Workflow DAG
│
├── behaviors/                     # Agent behavior protocols (3 files)
│   ├── assumption.md
│   ├── honesty.md
│   └── reflection.md
│
├── core/                          # Agent core modules (TypeScript)
│   ├── circuit-breaker.ts
│   ├── cli.ts
│   ├── cost-tracker.ts
│   ├── dag.ts
│   ├── event-store.ts
│   ├── health-check.ts
│   ├── idempotency.ts
│   ├── index.ts
│   ├── knowledge-store.ts
│   ├── plugin-registry.ts
│   ├── saga.ts
│   ├── team-config.ts
│   ├── telemetry.ts
│   └── validator.ts
│
├── flows/                         # Agent workflow definitions (7 files)
│   ├── 00-init.md
│   ├── 01-clarify.md
│   ├── 02-brainstorm.md
│   ├── 03-plan.md
│   ├── 05-code.md
│   ├── 06-review.md
│   └── 08-learn.md
│
├── memory/                        # Agent memory
│   └── knowledge/
│       └── INDEX.md
│
├── state/                         # Agent state tracking
│   ├── derived-state.json
│   └── events.jsonl
│
├── templates/                     # CI/CD templates
│   └── github-actions.yml
│
└── workspace/                     ← ALL PROJECT DOCUMENTATION
    │
    ├── architecture/              ← CANONICAL ARCHITECTURE (5 files)
    │   ├── architecture-v2.md             # Single source of truth
    │   ├── architecture-invariants-v2.md  # 37 code-verified invariants
    │   ├── design-decisions.md            # 21 ADRs
    │   ├── adr-index.md                   # ADR index
    │   └── phase1-freeze.md               # Frozen API declaration
    │
    ├── runtime/                   ← RUNTIME DOCS (3 files)
    │   ├── event-model.md
    │   ├── runtime-integration.md
    │   └── technical-debt-v2.md
    │
    ├── dom/                       ← DOM LAYER DOCS (6 files)
    │   ├── phase2.5-dom-architecture.md
    │   ├── dom-api-design.md
    │   ├── dom-event-model.md
    │   ├── dom-freeze-plan.md
    │   ├── dom-lifetime-model.md
    │   └── dom-risk-analysis.md
    │
    ├── design/                    ← FUTURE DESIGN DOCS (8 files)
    │   ├── browser-abstractions.md
    │   ├── plugin-extension-model.md
    │   ├── network-architecture.md
    │   ├── network-api-design.md
    │   ├── network-event-model.md
    │   ├── network-freeze-plan.md
    │   ├── network-lifetime-model.md
    │   └── network-risk-analysis.md
    │
    ├── plans/                     ← ORIGINAL PLANS (4 files)
    │   ├── plan.md
    │   ├── architecture-review.md
    │   ├── architecture-invariants.md
    │   └── design-freeze.md
    │
    ├── references/                ← HISTORICAL REFERENCES (5 files)
    │   ├── phase2-architecture.md
    │   ├── phase2-crate-map.md
    │   ├── phase2-risk-analysis.md
    │   ├── CHANGELOG_PHASE2.md
    │   └── KNOWN_LIMITATIONS.md
    │
    ├── audits/                    ← AUDIT OUTPUTS (9 files)
    │   ├── executive-summary.md
    │   ├── project-current-state.md
    │   ├── architecture-reconstruction.md
    │   ├── implementation-audit.md
    │   ├── technical-debt-report.md
    │   ├── test-audit.md
    │   ├── project-roadmap-status.md
    │   ├── next-milestone.md
    │   └── documentation-inventory.md
    │
    ├── reports/                   ← REFACTOR REPORTS (2 files)
    │   ├── documentation-refactor-report.md
    │   └── repository-stabilization-report.md
    │
    ├── knowledge/                 ← (reserved for future knowledge base)
    │
    └── archive/                   ← ARCHIVED FILES (26 files)
        └── docs-archive/
            ├── phase1/            (1 file)
            ├── phase2-reports/    (7 files)
            ├── stress-tests/      (10 files)
            ├── knowledge/         (8 files)
            └── legacy/            (empty, reserved)
```

---

## Migration Summary

| Metric | Value |
|--------|-------|
| Total .md files in repository | 78 |
| Files moved from browseros/docs/ → .vibe/workspace/ | 35 |
| Files already in .vibe/ (kept in place) | 11 |
| Files archived (previously moved) | 26 |
| Files created during refactor | 8 |
| Empty directories removed from browseros/ | 8 |
| .md files remaining in browseros/ | 0 (only target/ build artifact) |

---

## Files Migrated (35 files)

### From browseros/docs/architecture/ → .vibe/workspace/architecture/ (5)
- architecture-v2.md, architecture-invariants-v2.md, design-decisions.md, adr-index.md, phase1-freeze.md

### From browseros/docs/runtime/ → .vibe/workspace/runtime/ (3)
- event-model.md, runtime-integration.md, technical-debt-v2.md

### From browseros/docs/dom/ → .vibe/workspace/dom/ (6)
- phase2.5-dom-architecture.md, dom-api-design.md, dom-event-model.md, dom-freeze-plan.md, dom-lifetime-model.md, dom-risk-analysis.md

### From browseros/docs/design/ → .vibe/workspace/design/ (8)
- browser-abstractions.md, plugin-extension-model.md, network-architecture.md, network-api-design.md, network-event-model.md, network-freeze-plan.md, network-lifetime-model.md, network-risk-analysis.md

### From browseros/docs/references/ → .vibe/workspace/references/ (5)
- phase2-architecture.md, phase2-crate-map.md, phase2-risk-analysis.md, CHANGELOG_PHASE2.md, KNOWN_LIMITATIONS.md

### From browseros/docs/audits/ → .vibe/workspace/audits/ (9)
- executive-summary.md, project-current-state.md, architecture-reconstruction.md, implementation-audit.md, technical-debt-report.md, test-audit.md, project-roadmap-status.md, next-milestone.md, documentation-inventory.md

### From browseros/docs/reports/ → .vibe/workspace/reports/ (2)
- documentation-refactor-report.md, repository-stabilization-report.md

### From browseros/docs/archive/ → .vibe/workspace/archive/docs-archive/ (1 directory, 26 files)
- Entire archive directory moved preserving structure

---

## browseros/ Clean State

After migration, `browseros/` contains ONLY:

- **Source code** — 14 Rust crates
- **Cargo files** — Cargo.toml, Cargo.lock
- **Build artifacts** — target/
- **Test files** — test_timing.rs

**Zero documentation files** remain in browseros/ (excluding target/ build artifacts).

---

## Canonical Document Locations

| Document | New Location |
|----------|-------------|
| Architecture | `.vibe/workspace/architecture/architecture-v2.md` |
| Invariants | `.vibe/workspace/architecture/architecture-invariants-v2.md` |
| Design Decisions | `.vibe/workspace/architecture/design-decisions.md` |
| ADR Index | `.vibe/workspace/architecture/adr-index.md` |
| Phase 1 Freeze | `.vibe/workspace/architecture/phase1-freeze.md` |
| Technical Debt | `.vibe/workspace/runtime/technical-debt-v2.md` |
| Event Model | `.vibe/workspace/runtime/event-model.md` |
| DOM Docs | `.vibe/workspace/dom/` |
| Network Design | `.vibe/workspace/design/` |
| Audit Reports | `.vibe/workspace/audits/` |
| Refactor Reports | `.vibe/workspace/reports/` |
| Original Plans | `.vibe/workspace/plans/` |
| Archived Docs | `.vibe/workspace/archive/docs-archive/` |

---

## Advantages of New Structure

1. **Clear separation** — Code in browseros/, knowledge in .vibe/
2. **Single entry point** — All documentation starts from `.vibe/workspace/architecture/architecture-v2.md`
3. **No duplication** — Every piece of information lives in exactly one place
4. **Extensible** — New categories can be added as subdirectories of `.vibe/workspace/`
5. **Backward compatible** — Original plans remain in `.vibe/workspace/plans/` for reference
6. **Archive preserved** — All historical documents are in `.vibe/workspace/archive/docs-archive/` with ARCHIVED tags