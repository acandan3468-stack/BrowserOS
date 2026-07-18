# Documentation Inventory — BrowserOS

**Date:** 2026-07-08  
**Total .md files found:** 72 (excluding target/ and audit outputs)

---

## Section 1: ROOT

| # | File | Purpose | Status | Notes |
|---|------|---------|--------|-------|
| 1 | `AGENTS.md` | Vibe Coder Kit agent instructions + project anchored summary | 🟡 CURRENT | Contains active project state. Should be kept but anchored summary duplicates other docs |

---

## Section 2: .vibe/ — Agent Framework Files

### 2.1 Behaviors (3 files)

| # | File | Purpose | Status | Notes |
|---|------|---------|--------|-------|
| 2 | `.vibe/behaviors/assumption.md` | Agent behavior: state assumptions clearly | ✅ KEEP | Agent instruction, not project doc |
| 3 | `.vibe/behaviors/honesty.md` | Agent behavior: be honest about uncertainty | ✅ KEEP | Agent instruction, not project doc |
| 4 | `.vibe/behaviors/reflection.md` | Agent behavior: reflect before acting | ✅ KEEP | Agent instruction, not project doc |

### 2.2 Flows (7 files)

| # | File | Purpose | Status | Notes |
|---|------|---------|--------|-------|
| 5 | `.vibe/flows/00-init.md` | Initiation flow | ✅ KEEP | Agent workflow definition |
| 6 | `.vibe/flows/01-clarify.md` | Clarification flow | ✅ KEEP | Agent workflow definition |
| 7 | `.vibe/flows/02-brainstorm.md` | Brainstorming flow | ✅ KEEP | Agent workflow definition |
| 8 | `.vibe/flows/03-plan.md` | Planning flow | ✅ KEEP | Agent workflow definition |
| 9 | `.vibe/flows/05-code.md` | Coding flow | ✅ KEEP | Agent workflow definition |
| 10 | `.vibe/flows/06-review.md` | Review flow | ✅ KEEP | Agent workflow definition |
| 11 | `.vibe/flows/08-learn.md` | Learning flow | ✅ KEEP | Agent workflow definition |

### 2.3 Memory/Knowledge (1 file)

| # | File | Purpose | Status | Notes |
|---|------|---------|--------|-------|
| 12 | `.vibe/memory/knowledge/INDEX.md` | Knowledge base index | 🟡 CURRENT | Lightweight index, may need update |

### 2.4 Workspace/Knowledge (6 files)

| # | File | Purpose | Status | Notes |
|---|------|---------|--------|-------|
| 13 | `.vibe/workspace/knowledge/browseros-types-audit.md` | Consistency audit of browseros-types | 🟠 STALE | Audit findings were applied; some issues remain open. Information should be merged into design-decisions.md |
| 14 | `.vibe/workspace/knowledge/browseros-types-knowledge.md` | Design rationale for browseros-types | 🟡 CURRENT | Valuable design rationale. Should be preserved in canonical docs |
| 15 | `.vibe/workspace/knowledge/browseros-types-risk-map.md` | Risk map for types crate | 🟠 STALE | Phase 1 specific; risks resolved or superseded |
| 16 | `.vibe/workspace/knowledge/config-boundaries.md` | Config crate boundary analysis | 🟠 STALE | Phase 1 specific design exploration |
| 17 | `.vibe/workspace/knowledge/config-design-constraints.md` | Config design constraints | 🟡 CURRENT | Design rationale worth preserving |
| 18 | `.vibe/workspace/knowledge/config-risk-analysis.md` | Config risk analysis | 🟠 STALE | Phase 1 specific; risks resolved or superseded |

### 2.5 Workspace/Plans (4 files)

| # | File | Purpose | Status | Notes |
|---|------|---------|--------|-------|
| 19 | `.vibe/workspace/plans/architecture-review.md` | Architecture review — Phase 1 design | 🟠 STALE | Phase 1 specific. Contains architecture decisions now outdated by Phase 2. Some decisions still valid |
| 20 | `.vibe/workspace/plans/architecture-invariants.md` | 32 architecture invariants | 🟡 CURRENT | Important reference but 12/32 violated. Needs v2 |
| 21 | `.vibe/workspace/plans/design-freeze.md` | Phase 1 design freeze document | 🟠 STALE | Phase 1 specific. Phase 2 superseded |
| 22 | `.vibe/workspace/plans/plan.md` | Phase 1 implementation plan | 🟠 STALE | Phase 1 plan, partially implemented, mostly superseded |

### 2.6 Workspace/Reports (2 files)

| # | File | Purpose | Status | Notes |
|---|------|---------|--------|-------|
| 23 | `.vibe/workspace/reports/browseros-config-implementation.md` | Config implementation report | 🟠 STALE | Phase 1 implementation detail |
| 24 | `.vibe/workspace/reports/browseros-types-implementation.md` | Types implementation report | 🟠 STALE | Phase 1 implementation detail |

---

## Section 3: browseros/docs/ — Architecture & Design (26 files)

### 3.1 Phase 2 Core Documents

| # | File | Purpose | Status | Notes |
|---|------|---------|--------|-------|
| 25 | `browseros/docs/phase2-architecture.md` | Phase 2 foundation architecture | 🟡 CURRENT | Core architecture doc. Good reference but mentions unimplemented subsystems |
| 26 | `browseros/docs/phase2-crate-map.md` | Complete crate layout with API surfaces | 🟡 CURRENT | Valuable reference. 752 lines with detailed API surfaces |
| 27 | `browseros/docs/event-model.md` | Complete runtime event catalog | 🟡 CURRENT | Comprehensive event catalog. Should be canonical |
| 28 | `browseros/docs/PHASE2_FINAL.md` | Phase 2 final release summary | 🟠 STALE | Release-specific summary, superseded by ongoing development |
| 29 | `browseros/docs/phase2-freeze-plan.md` | Freeze analysis for 11 subsystems | 🟠 STALE | Freeze analysis is done; decisions applied |
| 30 | `browseros/docs/phase2-master-audit.md` | Master audit with 16 issues | 🟠 STALE | Audit completed; issues addressed per remediation plan |
| 31 | `browseros/docs/phase2-release-report.md` | Final release report | 🟠 STALE | Historical release document |
| 32 | `browseros/docs/phase2-remediation-plan.md` | NO-GO→GO remediation plan | 🟠 STALE | Remediation actions completed |
| 33 | `browseros/docs/phase2-risk-analysis.md` | Risk analysis | 🟡 CURRENT | Risk info worth preserving |
| 34 | `browseros/docs/CHANGELOG_PHASE2.md` | Phase 2 changelog (527 lines) | 🟡 CURRENT | Historical record, useful for context |
| 35 | `browseros/docs/KNOWN_LIMITATIONS.md` | Documented limitations | 🟡 CURRENT | Valuable limitations reference |

### 3.2 Phase 1 Documentation

| # | File | Purpose | Status | Notes |
|---|------|---------|--------|-------|
| 36 | `browseros/docs/milestone1-audit.md` | Milestone 1 audit | 🟠 STALE | Historical, Phase 1 specific |
| 37 | `browseros/docs/browser-abstractions.md` | Browser abstraction design | 🟡 CURRENT | Bridge trait design, still relevant |

### 3.3 Phase 2 Implementation Reports

| # | File | Purpose | Status | Notes |
|---|------|---------|--------|-------|
| 38 | `browseros/docs/phase2.1-bridge-implementation.md` | Bridge implementation | 🟠 STALE | Implementation report, code is truth |
| 39 | `browseros/docs/phase2.2-browser-implementation.md` | Browser implementation | 🟠 STALE | Implementation report, code is truth |

### 3.4 DOM Documents (Phase 2.5)

| # | File | Purpose | Status | Notes |
|---|------|---------|--------|-------|
| 40 | `browseros/docs/phase2.5-dom-architecture.md` | DOM architecture design | 🟡 CURRENT | Frozen design doc |
| 41 | `browseros/docs/dom-api-design.md` | DOM API design | 🟡 CURRENT | API design, implementation matches |
| 42 | `browseros/docs/dom-event-model.md` | DOM event model (30 payloads) | 🟡 CURRENT | Event model implemented in code |
| 43 | `browseros/docs/dom-freeze-plan.md` | Freeze plan with 3 tiers | 🟡 CURRENT | Freeze decisions applied |
| 44 | `browseros/docs/dom-lifetime-model.md` | DOM lifetime model | 🟡 CURRENT | Lifetime model documented |
| 45 | `browseros/docs/dom-risk-analysis.md` | DOM risk analysis | 🟡 CURRENT | Risk analysis documented |

### 3.5 Network Documents (Phase 2.6 Design)

| # | File | Purpose | Status | Notes |
|---|------|---------|--------|-------|
| 46 | `browseros/docs/network-api-design.md` | Network API design | 🟡 DESIGN | Phase 2.6 design, not implemented |
| 47 | `browseros/docs/network-event-model.md` | Network event model | 🟡 DESIGN | Phase 2.6 design, not implemented |
| 48 | `browseros/docs/network-freeze-plan.md` | Network freeze plan | 🟡 DESIGN | Phase 2.6 design, not implemented |
| 49 | `browseros/docs/network-lifetime-model.md` | Network lifetime model | 🟡 DESIGN | Phase 2.6 design, not implemented |
| 50 | `browseros/docs/network-risk-analysis.md` | Network risk analysis | 🟡 DESIGN | Phase 2.6 design, not implemented |

---

## Section 4: browseros-stress-tests/ — Reports (7 files)

| # | File | Purpose | Status | Notes |
|---|------|---------|--------|-------|
| 51 | `browseros/browseros-stress-tests/failure-analysis.md` | Stress test failure analysis | 🟠 STALE | Historical test result |
| 52 | `browseros/browseros-stress-tests/failure-root-cause.md` | Root cause analysis | 🟠 STALE | Historical test result |
| 53 | `browseros/browseros-stress-tests/long-run-stability-analysis.md` | Long-run stability | 🟠 STALE | Historical test result |
| 54 | `browseros/browseros-stress-tests/memory-growth-curve.md` | Memory growth analysis | 🟠 STALE | Historical test result |
| 55 | `browseros/browseros-stress-tests/memory-profile-report.md` | Memory profile report | 🟠 STALE | Historical test result |
| 56 | `browseros/browseros-stress-tests/performance-summary.md` | Performance summary | 🟠 STALE | Historical test result |
| 57 | `browseros/browseros-stress-tests/real-soak-execution-report.md` | Soak execution report | 🟠 STALE | Historical test result |
| 58 | `browseros/browseros-stress-tests/scheduler-thread-analysis.md` | Scheduler thread analysis | 🟠 STALE | Historical test result |
| 59 | `browseros/browseros-stress-tests/soak-test-report.md` | Soak test report | 🟠 STALE | Historical test result |
| 60 | `browseros/browseros-stress-tests/stress-test-report.md` | Stress test report | 🟠 STALE | Historical test result |

---

## Section 5: Audit Outputs (just created — 8 files)

| # | File | Purpose | Status | Notes |
|---|------|---------|--------|-------|
| 61 | `browseros/executive-summary.md` | Executive audit summary | ✅ NEW | Audit output — reference for refactoring |
| 62 | `browseros/project-current-state.md` | Complete project inventory | ✅ NEW | Audit output — reference for refactoring |
| 63 | `browseros/architecture-reconstruction.md` | Architecture from code | ✅ NEW | Audit output — reference for architecture-v2 |
| 64 | `browseros/implementation-audit.md` | Implementation status | ✅ NEW | Audit output — reference |
| 65 | `browseros/technical-debt-report.md` | Technical debt findings | ✅ NEW | Audit output — basis for technical-debt-v2 |
| 66 | `browseros/test-audit.md` | Test coverage analysis | ✅ NEW | Audit output — reference |
| 67 | `browseros/project-roadmap-status.md` | Roadmap vs actual | ✅ NEW | Audit output — reference |
| 68 | `browseros/next-milestone.md` | Recommended next step | ✅ NEW | Audit output — reference |

---

## Section 6: .vibe/ — Other (4 files)

| # | File | Purpose | Status | Notes |
|---|------|---------|--------|-------|
| 69 | `.vibe/memory/knowledge/INDEX.md` | (already listed as #12) | — | — |
| 70 | `.vibe/workspace/knowledge/browseros-types-risk-map.md` | (already listed as #15) | — | — |
| 71 | `.vibe/workspace/knowledge/config-risk-analysis.md` | (already listed as #18) | — | — |
| 72 | `.vibe/workspace/reports/browseros-config-implementation.md` | (already listed as #23) | — | — |

---

## Summary

| Category | Count | Current | Stale | Design | New |
|----------|-------|---------|-------|--------|-----|
| Root | 1 | 1 | 0 | 0 | 0 |
| .vibe/agent | 11 | 11 | 0 | 0 | 0 |
| .vibe/knowledge | 6 | 2 | 4 | 0 | 0 |
| .vibe/plans | 4 | 1 | 3 | 0 | 0 |
| .vibe/reports | 2 | 0 | 2 | 0 | 0 |
| docs/ Phase2 | 11 | 4 | 7 | 0 | 0 |
| docs/ Phase1 | 2 | 1 | 1 | 0 | 0 |
| docs/ Impl | 2 | 0 | 2 | 0 | 0 |
| docs/ DOM | 6 | 6 | 0 | 0 | 0 |
| docs/ Network | 5 | 0 | 0 | 5 | 0 |
| stress-test reports | 10 | 0 | 10 | 0 | 0 |
| Audit outputs | 8 | 0 | 0 | 0 | 8 |
| **Total** | **68** | **26** | **29** | **5** | **8** |

## Recommended Actions

1. **KEEP:** .vibe/agent files (11), current docs (26), DOM docs (6), some knowledge files (2)
2. **MERGE into canonical docs:** Valuable content from stale docs (7 Phase2 docs, 4 knowledge files)
3. **ARCHIVE:** Stale/stress-test reports (29 files) → move to docs/archive/
4. **CREATE:** architecture-v2.md, invariants-v2.md, design-decisions.md, crate-index.md
5. **REMOVE after merge:** Duplicate/obsolete information