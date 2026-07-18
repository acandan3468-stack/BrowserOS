# P13 Report — Planner & MCP Integration Foundation

**Phase:** P13  
**Status:** ✅ COMPLETED  
**Date:** 2026-07-14  

---

## Summary

Implemented orchestration contracts for future Planner (LLM, MCP, Plugin, CLI, Test) implementations inside `browseros-dag`. The design follows VCK-v4 rules: trait-only `PlannerBridge`, strongly typed serde-serializable types, zero runtime-behaviour changes, reuse of existing `exec.rs` abstractions.

## Deliverables

### New module: `browseros-dag/src/planner.rs` (~780 lines)

| Concept | Type | Description |
|---------|------|-------------|
| `PlannerRequest` | Struct | Goal + context + parameters + metadata |
| `PlannerResponse` | Struct | Plan + validation + metadata + duration |
| `PlanningContext` | Struct | Available capabilities, constraints, mode, correlation ID |
| `ExecutionPlan` | Struct | Ordered intents + dependencies + variables + constraints |
| `ExecutionIntent` | Struct | Capability + input + target + constraints + hints |
| `ExecutionTarget` | Enum | Local / Remote(peer) / Any |
| `ExecutionMode` | Enum | Sequential / Parallel / Hybrid{max_concurrent} |
| `ExecutionPriority` | Enum | Low / Normal / High / Critical |
| `ExecutionHints` | Struct | Estimated complexity, duration, intensity, retry strategy, tags |
| `ExecutionConstraints` | Struct | Timeout, max_retries, max_concurrency, required caps, resource limits |
| `PlannerMetadata` | Struct | Name + version + planner type |
| `PlannerType` | Enum | LLM / MCP / Plugin / RuleEngine / CLI / Test / Custom |
| `PlanningResult` | Enum | Success / InsufficientCapabilities / ValidationFailure / GraphError / Cancelled |
| `PlanValidationResult` | Struct | valid + step_results + errors + warnings |
| `StepValidationResult` | Struct | intent_id + valid + capability_found + params_valid + constraints_satisfied |
| `PlanningError` | Enum | CapabilityNotFound / ValidationFailed / DagConstructionFailed / VariableResolutionFailed / Cancelled |
| `PlannerBridge` | Trait | metadata() + plan() + validate_plan() + list_supported_goals() |

### Extended: `CapabilityMetadata` in `exec.rs`

- Added `version: Option<String>` — semantic version for capability negotiation
- Added `constraints: HashMap<String, String>` — implementation-specific constraints
- Added `with_version()` and `with_constraint()` builder methods
- All existing tests pass unchanged

### Updated: `browseros-dag/src/lib.rs`

- Added `pub mod planner;`
- Added 13 re-exports from planner.rs

## Key Design Decisions

1. **Trait-only PlannerBridge** — No struct implementations in `browseros-dag`. Future LLM, MCP, Plugin, CLI, or Test crates implement `PlannerBridge` without changing `browseros-dag`.

2. **Strongly typed, no Any** — Every concept is a dedicated enum or struct with serde Serialize/Deserialize. No `Box<dyn Any>`, no `serde_json::Value`, no dynamic typing.

3. **build_dag() reuses NodeFactory** — `ExecutionPlan::build_dag()` converts each `ExecutionIntent` into a `DagNode` via `NodeFactory::create_node()`, reusing `ExecutionContext`, `VariableStore`, and `NodeRegistry` from P12.

4. **Box<ExecutionPlan> in PlanningResult** — Clippy's `large_enum_variant` warning fixed by boxing the Success variant (296 bytes → 8 bytes in the enum).

5. **#[derive(Default)] with #[default]** — `ExecutionTarget`, `ExecutionMode`, `ExecutionPriority` use derive-based Default with `#[default]` annotation instead of manual impl.

## Capability Negotiation Design

- `CapabilityMetadata.version` enables future MCP/LLM planners to check version compatibility
- `CapabilityMetadata.constraints` enables resource-aware scheduling (memory, CPU, concurrency limits)
- `PlannerBridge::validate_plan()` checks capability existence + param validity before execution
- `ExecutionHints` carries AI-friendly estimates for optimization

## Test Coverage (56 new tests)

| Category | Tests | What they verify |
|----------|-------|------------------|
| PlannerRequest | 3 | Construction, builder, serde roundtrip |
| PlannerResponse | 2 | Construction, serde roundtrip |
| PlanningContext | 3 | Construction, builder, serde |
| ExecutionPlan | 7 | Construction, builder, validation (valid/missing/OOB/self-dep), DAG build (success/OOB/missing/node+edge counts) |
| ExecutionIntent | 3 | Construction, builder, serde roundtrip |
| ExecutionTarget | 2 | Default, serde all variants |
| ExecutionMode | 2 | Default, serde all variants |
| ExecutionPriority | 3 | Default, ordering, serde |
| ExecutionHints | 3 | new, builder, serde |
| ExecutionConstraints | 3 | new, builder, serde |
| PlannerMetadata | 2 | new, serde |
| PlannerType | 1 | Serde all 7 variants |
| PlanningResult | 1 | Serde all 5 variants |
| PlanValidationResult | 3 | new, combine, serde |
| StepValidationResult | 1 | Serde |
| PlanningError | 2 | Display, is Error |
| PlannerBridge | 5 | Object safe, metadata, plan, validate_plan, list_supported_goals |
| Send+Sync | 1 | All types are Send + Sync |
| Integration | 1 | Planner → build_dag → DagEngine |
| Edge cases | 5 | Empty plan valid, empty plan build_dag, default validation, default hints, default constraints |

## Results

```
$ cargo clippy --package browseros-dag
    0 warnings

$ cargo test --package browseros-dag
    277 passed, 0 failed

$ cargo test --workspace --exclude browseros-stress-tests
    488 passed, 1 failed (pre-existing: smoke_browser_launch_and_connect — Chrome not installed)
```

## Architecture Violations Check

All 32 frozen invariants (architecture-freeze-v3.md) are preserved:
- No browser crate dependency added
- No tokio dependency
- Zero unwrap/expect/panic in production code
- Plannertype are all strongly typed (serde)
- PlannerBridge is trait-only, no implementation
- build_dag() goes through NodeFactory → DagEngine, never bypassing
- No executor/scheduler/event-bus/runtime modifications
