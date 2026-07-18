# Repository Documentation Normalization Report

> **Date:** 2026-07-18  
> **Phase:** Phase 6.2 — Pre-GitHub Preparation  
> **Status:** Findings and recommendations only — no files moved yet  

---

## 1. Workspace Convention Summary

From `.vibe/workspace/reports/documentation-migration-report.md`, the canonical rule is:

> `browseros/` → ONLY code (Rust source, Cargo files, build artifacts)  
> `.vibe/` → ALL knowledge (architecture, ADRs, audits, reports, plans, designs)

The `.vibe/workspace/` hierarchy:

| Directory | Content | Rule |
|-----------|---------|------|
| `architecture/` | Canonical architecture docs | Updated per freeze |
| `runtime/` | Runtime documentation | Event model, integration |
| `dom/` | DOM layer docs | Phase 2.5 frozen docs |
| `design/` | Future design docs | Network, plugin, session |
| `plans/` | Original plans | Historical, immutable |
| `references/` | Historical references | Phase 2 docs, changelogs |
| `audits/` | Audit outputs | Pre-July-18 audits |
| `reports/` | Reports | Phase reports, hardening |
| `dag/` | DAG docs | DAG-specific (nonstandard location) |
| `audit/` | New audits | Post-July-18 audit outputs |
| `archive/docs-archive/` | Archived files | Historical, read-only |
| `memory/` | Agent memory | Reserved |

---

## 2. Document Inventory (144 .md files)

### 2.1 Canonical (in correct locations) — 78 files ✅

| Location | Count | Description |
|----------|-------|-------------|
| `.vibe/behaviors/` | 3 | Agent behavior protocols |
| `.vibe/flows/` | 7 | Agent workflow definitions |
| `.vibe/memory/knowledge/` | 1 | INDEX.md |
| `.vibe/state/` | 1 | STATE.md |
| `.vibe/workspace/architecture/` | 6 | Architecture, invariants, ADRs, freeze docs |
| `.vibe/workspace/runtime/` | 4 | Event model, technical debt v2/v3, integration |
| `.vibe/workspace/dom/` | 6 | DOM design + freeze docs |
| `.vibe/workspace/design/` | 14 | Network design (6) + session design (10) + browser-abstractions + plugin-extension |
| `.vibe/workspace/plans/` | 4 | plan.md, architecture-invariants, architecture-review, design-freeze |
| `.vibe/workspace/references/` | 5 | Phase2 arch, crate-map, risk-analysis, CHANGELOG, KNOWN_LIMITATIONS |
| `.vibe/workspace/audits/` | 9 | Pre-July-18 audit outputs |
| `.vibe/workspace/audit/` | 5 | Post-July-18 audit outputs (DEPENDENCY_GRAPH, PHASE6_RISK_REGISTER, etc.) |
| `.vibe/workspace/reports/` | 15 | Phase reports, hardening, refactor reports |
| `.vibe/workspace/dag/` | 7 | DAG implementation plan, test plan, invariants, etc. |
| `.vibe/workspace/archive/` | 26 | Archived stress-tests (10), knowledge (8), phase1 (1), phase2-reports (7) |
| Root `AGENTS.md` | 1 | Root-level agent instructions |

### 2.2 Violating convention (in `browseros/` but should be in `.vibe/`) — 32 files ❌

| File | Current Location | Should Be In | Category |
|------|-----------------|--------------|----------|
| `PHASE6_1_AUDIT.md` | `browseros/` root | `.vibe/workspace/reports/` | Freeze audit |
| `PHASE6_1_FIX_REPORT.md` | `browseros/` root | `.vibe/workspace/reports/` | Fix report |
| `browseros-mcp/ARCHITECTURE.md` | `browseros/browseros-mcp/` | `.vibe/workspace/design/mcp/` | MCP design |
| `browseros-mcp/ARCHITECTURE_REVIEW.md` | `browseros/browseros-mcp/` | `.vibe/workspace/design/mcp/` | MCP review |
| `browseros-mcp/ERROR_MODEL.md` | `browseros/browseros-mcp/` | `.vibe/workspace/design/mcp/` | MCP design |
| `browseros-mcp/IMPLEMENTATION_PLAN.md` | `browseros/browseros-mcp/` | `.vibe/workspace/design/mcp/` | MCP plan |
| `browseros-mcp/NOTIFICATION_MODEL.md` | `browseros/browseros-mcp/` | `.vibe/workspace/design/mcp/` | MCP design |
| `browseros-mcp/SESSION_MODEL.md` | `browseros/browseros-mcp/` | `.vibe/workspace/design/mcp/` | MCP design |
| `browseros-mcp/TOOL_REGISTRY_DESIGN.md` | `browseros/browseros-mcp/` | `.vibe/workspace/design/mcp/` | MCP design |
| `browseros-llm/BENCHMARK_SPEC.md` | `browseros/browseros-llm/` | `.vibe/workspace/reports/` | Benchmark spec |
| `browseros-llm/BUG_REPORT.md` | `browseros/browseros-llm/` | `.vibe/workspace/reports/` | Bug report |
| `browseros-llm/FINAL_PHASE3_SUMMARY.md` | `browseros/browseros-llm/` | `.vibe/workspace/reports/` | Phase report |
| `browseros-llm/FINAL_PHASE4_SUMMARY.md` | `browseros/browseros-llm/` | `.vibe/workspace/reports/` | Phase report |
| `browseros-llm/KNOWN_LIMITATIONS.md` | `browseros/browseros-llm/` | `.vibe/workspace/references/` | Known limitations |
| `browseros-llm/MCP_API_SURFACE.md` | `browseros/browseros-llm/` | `.vibe/workspace/design/llm/` | LLM/MCP design |
| `browseros-llm/MCP_ARCHITECTURE_REVIEW.md` | `browseros/browseros-llm/` | `.vibe/workspace/design/llm/` | LLM/MCP review |
| `browseros-llm/MCP_IMPLEMENTATION_DESIGN.md` | `browseros/browseros-llm/` | `.vibe/workspace/design/llm/` | LLM/MCP design |
| `browseros-llm/MCP_REGISTRATION_REPORT.md` | `browseros/browseros-llm/` | `.vibe/workspace/reports/` | Registration report |
| `browseros-llm/MCP_TOOL_ROADMAP.md` | `browseros/browseros-llm/` | `.vibe/workspace/design/llm/` | LLM/MCP roadmap |
| `browseros-llm/PHASE4C_AUDIT.md` | `browseros/browseros-llm/` | `.vibe/workspace/reports/` | Audit report |
| `browseros-llm/PHASE4_PLAN.md` | `browseros/browseros-llm/` | `.vibe/workspace/plans/` | Phase plan |
| `browseros-llm/PHASE5A_EXECUTION_REPORT.md` | `browseros/browseros-llm/` | `.vibe/workspace/reports/` | Execution report |
| `browseros-llm/PHASE5A_PLAN.md` | `browseros/browseros-llm/` | `.vibe/workspace/plans/` | Phase plan |
| `browseros-llm/PHASE5B_EXECUTION_REPORT.md` | `browseros/browseros-llm/` | `.vibe/workspace/reports/` | Execution report |
| `browseros-llm/PHASE6_PREPARATION.md` | `browseros/browseros-llm/` | `.vibe/workspace/design/llm/` | Phase 6 prep |
| `browseros-llm/PRODUCTION_READINESS.md` | `browseros/browseros-llm/` | `.vibe/workspace/reports/` | Production readiness |
| `browseros-llm/PRODUCTION_VALIDATION_CHECKLIST.md` | `browseros/browseros-llm/` | `.vibe/workspace/reports/` | Validation checklist |
| `browseros-llm/PROVIDER_VALIDATION_RESULTS.md` | `browseros/browseros-llm/` | `.vibe/workspace/reports/` | Validation results |
| `browseros-llm/VALIDATION_MATRIX.md` | `browseros/browseros-llm/` | `.vibe/workspace/reports/` | Validation matrix |
| `browseros/.vibe/workspace/architecture/architecture-freeze-v4.md` | `browseros/.vibe/` | `.vibe/workspace/architecture/` | Architecture freeze (v4 supersedes v3) |
| `browseros/.vibe/workspace/llm/*.md` | `browseros/.vibe/` (12 files) | `.vibe/workspace/design/llm/` | LLM design docs |
| `browseros/.vibe/workspace/reports/*.md` | `browseros/.vibe/` (4 files) | `.vibe/workspace/reports/` | DAG reports |

### 2.3 Secondary `.vibe/` directory — 17 files ❌

`browseros/.vibe/workspace/` contains 17 files across 3 directories:
- `architecture/` (1): `architecture-freeze-v4.md`
- `llm/` (12): `llm-analysis.md`, `llm-architecture.md`, `llm-combined.md`, `llm-events.md`, `llm-gateway-api.md`, `llm-implementation-completeness.md`, `llm-mcp-adapter.md`, `llm-metrics.md`, `llm-provider-trait.md`, `llm-review.md`, `llm-routing.md`, `llm-streaming.md`
- `reports/` (4): `architecture-conformance-v4.md`, `browseros-dag-final-report.md`, `browseros-dag-p16-report.md`, `final-quality-gate.md`

**Problem:** There should not be a `.vibe/` inside `browseros/`. The root `.vibe/` is the single source of truth.

---

## 3. Document Classification

| Category | Count | Location |
|----------|-------|----------|
| Architecture | 8 | `workspace/architecture/`, `browseros/.vibe/` (v4) |
| ADR | 2 | `workspace/architecture/adr-index.md`, `design-decisions.md` |
| Audit | 14 | `workspace/audits/` (9), `workspace/audit/` (5) |
| Review | 4 | `workspace/audits/implementation-audit.md`, etc. |
| Planning | 7 | `workspace/plans/` (4), `browseros-llm/PHASE4_PLAN.md`, `PHASE5A_PLAN.md` |
| Execution Report | 12 | `workspace/reports/` (various phase reports) |
| Validation | 5 | `browseros-llm/VALIDATION_MATRIX.md`, `PROVIDER_VALIDATION_RESULTS.md`, etc. |
| Benchmark | 1 | `browseros-llm/BENCHMARK_SPEC.md` |
| Risk Register | 2 | `workspace/audit/PHASE6_RISK_REGISTER.md`, `workspace/references/phase2-risk-analysis.md` |
| Roadmap | 1 | `workspace/audits/project-roadmap-status.md` |
| Freeze Report | 2 | `PHASE6_1_AUDIT.md`, `PHASE6_1_FIX_REPORT.md` |
| Implementation Report | 8 | DAG phase reports |
| Workspace Docs | 5 | `workspace/` structure docs |
| Crate Docs | 0 | (intentionally no crate-level docs in `/browseros/`) |
| Developer Docs | 1 | `AGENTS.md` |
| Temporary Notes | 0 | None found |
| MCP Design | 7 | `browseros-mcp/*.md` |
| LLM Design | 24 | `browseros-llm/*.md` + `browseros/.vibe/workspace/llm/*.md` |
| Agent Behavior | 4 | `.vibe/behaviors/` (3) + `.clinerules/markdown-files-merge-skill.md` |
| Agent Workflows | 7 | `.vibe/flows/` |
| Agent Core | 14 | `.vibe/core/*.ts` (TypeScript, not docs) |
| Archive | 26 | `workspace/archive/docs-archive/` |

---

## 4. Issues Found

### 4.1 STATE.md is critically out of date

**Current state:** Claims Phase 5B is complete, lists Phase 3 frozen API, ends history at 2026-07-16.

**Actual state:** Phase 6.1 is frozen. Phase 6.2 SessionManager design is frozen. The workspace audit is complete.

**Impact:** Any reader relying on STATE.md will have incorrect understanding of project status.

### 4.2 derived-state.json is critically out of date

**Current state:** Last entry is `phase2.1-llm-gateway-design-freeze` from 2026-07-15.

**Missing phases:** Phase 4C, Phase 5A, Phase 5B, Phase 6.1, Phase 6.2 design, all DAG phases.

**Impact:** The event store is incomplete. Phase transitions since July 15 are not recorded.

### 4.3 Documents in `browseros/` violate workspace convention

32 `.md` files in `browseros/` directories violate the `browseros/ = code only` rule established in the July 8 documentation migration.

### 4.4 Secondary `.vibe/` directory

`browseros/.vibe/` exists with 17 files, duplicating the root `.vibe/` structure. This should be merged into the root `.vibe/`.

### 4.5 `architecture-freeze-v4.md` is in the wrong `.vibe/`

`browseros/.vibe/workspace/architecture/architecture-freeze-v4.md` (dated 2026-07-14) supersedes `architecture-freeze-v3.md` in the canonical location but is hidden in the secondary `.vibe/`. Meanwhile, `architecture-freeze-v3.md` is still the canonical freeze document in `.vibe/workspace/architecture/`.

### 4.6 Duplicate audit directories

`.vibe/workspace/audit/` (5 files, newly added) and `.vibe/workspace/audits/` (9 files, historical). These should be merged into a single directory.

### 4.7 DAG docs in nonstandard location

`workspace/dag/` (7 files) is not part of the canonical directory structure defined in `documentation-migration-report.md`. These should be in `design/dag/` or `reports/`.

### 4.8 Root `PHASE6_1_AUDIT.md` and `PHASE6_1_FIX_REPORT.md`

These critical Phase 6.1 documents are in the `browseros/` root directory, not in `.vibe/workspace/reports/`.

### 4.9 Link verification

Most documents use absolute Path references (e.g., `browseros/browseros-mcp/src/...`) rather than relative markdown links. Cross-document links (e.g., "see architecture-freeze-v3.md") assume files are in the same directory. After relocation, these would break.

---

## 5. Recommended Normalization Actions

### P0 — Fix STATE.md and derived-state.json

**Files:**
- `.vibe/state/STATE.md` — Rewrite to reflect current Phase 6.2 status
- `.vibe/state/derived-state.json` — Append all missing phase transitions
- `.vibe/state/events.jsonl` — Verify append-only integrity

**Rationale:** STATE.md is the single source of truth for project status. It must reflect reality before GitHub release.

### P1 — Merge secondary `.vibe/` into root `.vibe/`

**Move:**
```
browseros/.vibe/workspace/architecture/architecture-freeze-v4.md
  → .vibe/workspace/architecture/architecture-freeze-v4.md
  (and update v3 header to note v4 supersedes it)

browseros/.vibe/workspace/llm/*.md (12 files)
  → .vibe/workspace/design/llm/

browseros/.vibe/workspace/reports/*.md (4 files)
  → .vibe/workspace/reports/
```

**Conflict resolution:** Check for filename collisions. `browseros-dag-final-report.md` and `browseros-dag-p16-report.md` are new (no conflict). `final-quality-gate.md` and `architecture-conformance-v4.md` need verification.

**After merge:** Delete `browseros/.vibe/` directory.

### P2 — Move crate-level docs to `.vibe/workspace/`

**Move `browseros/browseros-mcp/*.md` (7 files):**
```
browseros-mcp/ARCHITECTURE.md
browseros-mcp/ARCHITECTURE_REVIEW.md
browseros-mcp/ERROR_MODEL.md
browseros-mcp/IMPLEMENTATION_PLAN.md
browseros-mcp/NOTIFICATION_MODEL.md
browseros-mcp/SESSION_MODEL.md
browseros-mcp/TOOL_REGISTRY_DESIGN.md
  → .vibe/workspace/design/mcp/
```

**Move `browseros/browseros-llm/*.md` (21 files):**
```
Phase reports (PHASE4_PLAN.md, PHASE4C_AUDIT.md, PHASE5A_PLAN.md,
  PHASE5A_EXECUTION_REPORT.md, PHASE5B_EXECUTION_REPORT.md,
  PHASE6_PREPARATION.md, FINAL_PHASE3_SUMMARY.md, FINAL_PHASE4_SUMMARY.md)
  → .vibe/workspace/reports/

Design docs (MCP_API_SURFACE.md, MCP_ARCHITECTURE_REVIEW.md,
  MCP_IMPLEMENTATION_DESIGN.md, MCP_TOOL_ROADMAP.md)
  → .vibe/workspace/design/llm/

Validation (VALIDATION_MATRIX.md, PROVIDER_VALIDATION_RESULTS.md,
  PRODUCTION_VALIDATION_CHECKLIST.md, BENCHMARK_SPEC.md)
  → .vibe/workspace/reports/

References (KNOWN_LIMITATIONS.md, BUG_REPORT.md,
  MCP_REGISTRATION_REPORT.md, PRODUCTION_READINESS.md)
  → .vibe/workspace/references/
```

**Move root `PHASE6_1_*` files:**
```
browseros/PHASE6_1_AUDIT.md → .vibe/workspace/reports/phase6.1-audit.md
browseros/PHASE6_1_FIX_REPORT.md → .vibe/workspace/reports/phase6.1-fix-report.md
```

### P3 — Merge `audit/` and `audits/` directories

**Consolidate:**
```
.vibe/workspace/audit/*.md (5 files)
  → .vibe/workspace/audits/
```
Delete `audit/` directory (keep `audits/` as canonical).

### P4 — Move DAG docs to canonical location

**Option A:** Move to `workspace/design/dag/` (if considered design docs)
**Option B:** Move to `workspace/reports/` (if considered implementation reports)

Recommend **Option A** — the DAG docs contain architecture and API design, not just reports.

### P5 — Repair cross-document links

After moves, every document that references another document by relative path (e.g., `../architecture/architecture-freeze-v3.md`) must be updated.

### P6 — Update architecture-freeze-v3.md

Add header to `architecture-freeze-v3.md` noting it is superseded by v4. Copy `architecture-freeze-v4.md` to canonical location.

---

## 6. STATE.md Rewrite Recommendation

STATE.md should be rewritten to contain:

1. **Current Phase:** Phase 6.2 — SessionManager Design (DESIGN FREEZE)
2. **Phase History:** Append Phase 4C, Phase 5A, Phase 5B, Phase 6.1, Phase 6.2 design
3. **Frozen APIs:** Update to reflect Phase 4C frozen API (browseros-mcp), NOT Phase 3
4. **Key Decisions:** Add SessionManager design decisions
5. **Next Steps:** Phase 6.2 implementation
6. **Remove** the detailed Phase 3 frozen API listing (it belongs in the freeze doc, not STATE.md)

---

## 7. GitHub Readiness Score

| Dimension | Score | Notes |
|-----------|-------|-------|
| Root layout | 6/10 | No root README, no LICENSE |
| Crate layout | 9/10 | Clean Cargo workspace |
| Docs layout | 4/10 | 32 docs in wrong locations, stale STATE.md |
| Examples | 0/10 | No example directory |
| Configs | 3/10 | No CI/CD, no .gitignore, no issue templates |
| Scripts | 3/10 | No build scripts, no dev scripts |
| Tools | 5/10 | `.vibe/tools/` doesn't exist |
| License | 0/10 | No LICENSE file |
| README | 0/10 | No root README.md |

**Overall GitHub Readiness: 3/10**

---

## 8. Repository Cleanliness Score

| Dimension | Score | Notes |
|-----------|-------|-------|
| Source code | 9/10 | Clean, well-organized crates |
| Documentation | 4/10 | 32 files in wrong locations, 2 stale state files |
| Build artifacts | 6/10 | `target/` should be gitignored |
| Temporary files | 10/10 | None found |
| Editor files | 8/10 | No `.vscode/`, no `.idea/` |
| Secrets | 10/10 | No secrets detected (see §10) |
| Git history | 5/10 | No commits yet (untracked) |

**Overall Cleanliness: 7/10**

---

## 9. Recommended .gitignore

No `.gitignore` exists. Recommended contents:

```
# Rust
target/
Cargo.lock

# IDE
.vscode/
.idea/
*.swp
*.swo
*~

# OS
.DS_Store
Thumbs.db

# Build
*.o
*.exe
*.dll
*.so
*.dylib

# Logs
*.log
*.csv
*.json
mcp_e2e_results.csv
mcp_e2e_results.json

# Test artifacts
browseros/browseros-llm/mcp_e2e_test.mjs
browseros/browseros-llm/mcp_e2e_test.ps1
```

---

## 10. Secret Scan Results

| Search Target | Found? | Location |
|--------------|--------|----------|
| API keys (sk-, pk-) | No | — |
| Bearer tokens | No | — |
| OpenAI keys | No | — |
| Anthropic keys | No | — |
| Gemini keys | No | — |
| GitHub tokens (ghp_, ghs_) | No | — |
| Passwords | No | — |
| Private URLs | No | — |
| localhost-only configs | No | — |
| Machine-specific paths | No | — |
| Temporary files | No | — |
| Editor files (.vscode, .idea) | No | — |

**No secrets found.** The repository is clean.

---

## 11. Recommended LICENSE

**Recommendation:** MIT License

**Rationale:** BrowserOS is a research/pre-alpha project. MIT is the most permissive license, maximizing adoption and contribution. If commercial concerns arise later, relicense to Apache 2.0.

---

## 12. Recommended Repository Description

> BrowserOS — Modular, event-driven browser agent runtime. Type-safe DOM automation, LLM gateway, CDP protocol bridge, and MCP server for building programmable browser agents. Written in Rust.

---

## 13. Recommended GitHub Topics

```
browser-automation
web-scraping
rust
cdp
mcp
llm
model-context-protocol
dom-automation
event-driven
agent-framework
```

---

## 14. First Public Release Checklist

### Before Release

- [ ] **P0:** Rewrite STATE.md to reflect current phase
- [ ] **P0:** Append missing events to derived-state.json
- [ ] **P1:** Merge `browseros/.vibe/` into root `.vibe/`
- [ ] **P1:** Move 32 docs from `browseros/` to `.vibe/workspace/`
- [ ] **P2:** Consolidate `audit/` → `audits/`
- [ ] **P2:** Move DAG docs to canonical location (`design/dag/`)
- [ ] **P2:** Update architecture-freeze-v3 with v4 supersession
- [ ] **P3:** Repair cross-document links after moves
- [ ] **P3:** Create `.gitignore`
- [ ] **P3:** Create `LICENSE` (MIT)
- [ ] **P3:** Create root `README.md` with project overview
- [ ] **P3:** Verify `cargo build --workspace` passes
- [ ] **P3:** Verify `cargo test --workspace` passes (ignoring Chrome-dependent test)

### After Release

- [ ] Add CI/CD workflow (`.github/workflows/`)
- [ ] Add issue templates (`.github/ISSUE_TEMPLATE/`)
- [ ] Add contributing guide (`CONTRIBUTING.md`)
- [ ] Add code of conduct (`CODE_OF_CONDUCT.md`)
- [ ] Add examples directory (`examples/`)
- [ ] Add changelog (`CHANGELOG.md`)
- [ ] Consider `rustsec/advisory-db` for dependency audit

---

## 15. Summary

| Metric | Value |
|--------|-------|
| Total .md files | 144 |
| In correct location | 78 (54%) |
| In wrong location | 32 (22%) |
| In secondary .vibe | 17 (12%) |
| Archive | 26 (18%) |
| State files out of date | 2 (STATE.md, derived-state.json) |
| No .gitignore | ❌ |
| No LICENSE | ❌ |
| No root README | ❌ |
| Secrets detected | 0 ✅ |
| Temporary/editor files | 0 ✅ |
| GitHub Readiness | 3/10 |
| Repository Cleanliness | 7/10 |

The repository needs approximately **3–4 hours of documentation normalization work** before it is ready for public GitHub release. No Rust code changes are required. The normalization should begin with P0 items (STATE.md, derived-state.json) before any file moves, as the state files are the most critical for contributor onboarding.
