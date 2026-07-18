# P14 Report — Plugin Runtime Foundation

**Phase:** P14  
**Status:** ✅ COMPLETED  
**Date:** 2026-07-14  

---

## Summary

Implemented trait-based runtime contracts for every future Plugin system inside `browseros-dag/src/plugin.rs`. All types are strongly typed with serde serialization — no `Any`, no `Box<dyn ...>`, no `serde_json::Value`. The design follows VCK-v4 rules: trait-only `Plugin`, thread-safe `PluginRegistry`, reuse of existing `exec.rs` abstractions, zero runtime behaviour changes.

## Deliverables

### New module: `browseros-dag/src/plugin.rs` (~1310 lines)

| Concept | Type | Description |
|---------|------|-------------|
| `PluginId` | Struct(String newtype) | Unique plugin identifier, serde-serializable |
| `PluginVersion` | Struct(major, minor, patch) | Semver version with comparison and `compatible_with()` |
| `PluginAuthor` | Struct | Author name + optional email/url |
| `PluginCapabilityId` | Struct(String newtype) | Unique capability identifier |
| `PluginCapability` | Struct | Capability ID + `CapabilityMetadata` (from exec.rs) |
| `PluginState` | Enum (9 states) | Discovered → Registered → Validated → Initialized → Running ↔ Paused → Disabled → Unloaded, any → Failed |
| `PluginStatus` | Enum (4 variants) | Healthy / Degraded / Unhealthy / Unknown |
| `PluginPermission` | Enum (7 variants, `#[non_exhaustive]`) | BrowserAccess, NetworkAccess, StorageAccess, FilesystemAccess, ClipboardAccess, DownloadAccess, InputSimulation |
| `PluginDependency` | Struct | plugin_id + required/optional + min/max version with semver matching |
| `PluginMetadata` | Struct | name, version, author, description, homepage, license |
| `PluginHooks` | Struct | before/after execution/node, execution_failed, execution_cancelled |
| `PluginManifest` | Struct | metadata + capabilities + dependencies + permissions + hooks |
| `PluginExecutionMetadata` | Struct | execution_count, last_started_at, last_error, state |
| `PluginContext` | Struct | plugin_id + optional ExecutionContext |
| `PluginRegistration` | Struct | Snapshot of plugin state in registry |
| `PluginValidationResult` | Struct | valid flag + missing deps + version mismatches + errors + warnings |
| `PluginError` | Enum (12 variants, thiserror) | All typed Plugin errors |
| `PluginLifecycle` | Struct (stateless) | `validate_transition()` — 9-state validated transitions |
| `Plugin` trait | Trait (contracts only) | metadata, manifest, capabilities, permissions, dependencies, validate, initialize, shutdown, health |
| `PluginRegistry` | Struct (Arc\<RwLock\<HashMap\>\>) | Thread-safe: register, unregister, lookup, lookup_by_capability, lookup_by_version, lookup_by_state, list_plugins, list_capabilities, verify_dependencies, verify_permissions, health_inspect, validate_plugin, initialize_plugin, shutdown_plugin, transition_to |

### Updated: `browseros-dag/src/lib.rs`

Added `pub mod plugin;` and 17 new re-exports.

### Architecture compliance

- **Plugin Runtime is completely independent** from Browser, CDP, DOM, Planner implementation, MCP transport, LLM implementation
- **Architecture flow preserved**: Planner → Capability Resolution → Plugin Registry → NodeFactory → DagEngine
- **No modifications** to executor.rs, scheduler.rs, graph.rs, events.rs, engine.rs, error.rs, node.rs, bridge.rs, or any external crate
- **Reuses exec.rs types**: CapabilityMetadata, ExecutionContext
- **Zero runtime behaviour changes**: No executor/scheduler/event-bus modifications

## Test Results

### 56 new plugin tests

| Category | Count | Tests |
|----------|-------|-------|
| PluginId | 5 | creation, from_string, equality, hash, serde |
| PluginVersion | 9 | creation, parse valid/invalid/non-numeric, display, ordering, compatible_with (×2), serde |
| PluginAuthor | 3 | creation, with_email/url, serde |
| PluginCapabilityId | 2 | creation, serde |
| PluginCapability | 2 | creation, serde |
| PluginState | 5 | valid transitions, any_to_failed, invalid transitions, display, serde |
| PluginLifecycle | 2 | valid/invalid transition |
| PluginStatus | 1 | ordering |
| PluginPermission | 2 | equality, serde |
| PluginDependency | 6 | creation, optional, version bounds, matches (×3), serde |
| PluginMetadata | 3 | creation, builder, serde |
| PluginHooks | 4 | creation, builder, default, serde |
| PluginManifest | 3 | creation, builder, serde |
| PluginExecutionMetadata | 2 | creation, serde |
| PluginContext | 1 | creation |
| PluginRegistration | 2 | creation, serde |
| PluginValidationResult | 5 | valid_by_default, missing_dep, error, warning, serde |
| PluginError | 3 | not_found, invalid_transition, already_registered |
| PluginRegistry | 28 | register, duplicate, lookup (×2), lookup_by_capability (×2), lookup_by_version, lookup_by_state, list_plugins, list_capabilities, list_capabilities_dedup, transition valid/invalid/nonexistent, full lifecycle, unregister before unloaded, verify_dependencies (×4), verify_permissions (×2), health_inspect, validate_plugin, initialize_plugin, shutdown_plugin, nonexistent operations, Send+Sync, object safety, empty list, running-paused-running cycle, full lifecycle to failed, validation result defaults |

### Build verification

| Check | Result |
|-------|--------|
| `cargo build --workspace` | ✅ Clean |
| `cargo clippy --workspace` | ✅ 0 warnings |
| `cargo fmt --all --check` | ✅ Clean |
| `cargo test --workspace` (DAG) | ✅ 371 passed, 0 failed |
| `cargo test --workspace` (all) | ✅ All workspace crates pass |

## Plugin Architecture Flow

```
Planner → ExecutionPlan → Capability Resolution
                                    ↓
                          PluginRegistry
                          (verify deps/permissions)
                                    ↓
                           NodeFactory
                     (create DagNode per capability)
                                    ↓
                           DagEngine
                        (execute DAG graph)
```

## Plugin Lifecycle

```
Discovered → Registered → Validated → Initialized → Running ↔ Paused
                                                      ↓
                                                Disabled
                                                  ↓
                                            Unloaded (terminal)
                                                  ↓
                                               Failed
```

- Any state can transition to `Failed`
- `Failed` and `Disabled` can transition to `Unloaded`
- All other transitions return `PluginError::InvalidTransition`

## P14 Extension Points (unchanged from P13 analysis)

| Extension Point | Status | Notes |
|-----------------|--------|-------|
| `ExecutableNode` trait | ✅ Ready | P12, used by NodeRegistry |
| `NodeRegistry` | ✅ Ready | P12, thread-safe capability registry |
| `NodeFactory` | ✅ Ready | P12, wraps ExecutableNode into DagNode |
| `ContextPropagator` | ✅ Ready | P12, creates child ExecutionContext |
| `PlannerBridge` trait | ✅ Ready | P13, orchestrator contract |
| `Plugin` trait | ✅ NEW | P14, plugin contract |
| `PluginRegistry` | ✅ NEW | P14, thread-safe plugin metadata registry |

## P14 Design Decisions

1. **Plugin trait contracts only** — no default implementations, no runtime behavior
2. **PluginRegistry owns metadata only** — never stores browser objects
3. **Separate lock per registry** — Arc\<RwLock\<HashMap\<PluginId, StoredPlugin\>\>\> — no lock held across plugin trait call boundaries
4. **StoredPlugin stores both Arc\<dyn Plugin\> and registration snapshot** — enables direct trait access without re-reading from plugin instance
5. **`#[non_exhaustive]` on PluginPermission** — future variants can be added without breaking existing code
6. **PluginVersion uses manual Ord/major-minor-patch** — simple tri-level numeric comparison
7. **PluginId as String newtype with From\<&str\>** — ergonomic construction in integration code
8. **No serde on Plugin trait** — trait objects can't be serialized; only metadata/registration snapshots are serialized
9. **`register_capabilities_with` deferred** — requires new NodeFactory method; moved to future P14.1 if needed
10. **Dependency verification is registry-scoped** — no external service calls; only checks what's in the registry
