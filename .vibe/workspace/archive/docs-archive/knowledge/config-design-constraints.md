ARCHIVED

This document has been archived. Its content has been merged into the canonical documentation set (design-decisions.md).
See docs/design-decisions.md for current information.

---

# Configuration Model Design Constraints — BrowserOS

## Part 1 — Design Constraints

### 1.1 What Configuration MUST Support

| Category | Examples | Rationale |
|----------|----------|-----------|
| Network bindings | `event.bus.host`, `event.bus.port`, `metrics.otlp.endpoint` | System must bind to configurable addresses |
| Buffer sizes | `event.bus.buffer_size`, `store.write_batch_size` | Performance tuning without recompile |
| Timeouts | `event.bus.delivery_timeout_ms`, `scheduler.task_timeout_ms` | INV-017: every async op has configurable timeout |
| Retry policies | `event.bus.max_retries`, `event.bus.backoff_base_ms` | Per-component retry tuning |
| Log/metric/trace levels | `observability.log.level`, `observability.metrics.export_interval_s` | Operational control |
| Store paths | `store.event_store.path`, `store.snapshot.interval_s` | File system layout flexibility |
| Plugin control | `plugin.<name>.enabled`, `plugin.scan_path` | Dynamic enable/disable |
| Component tunables | `lifecycle.health_check_interval_s`, `dag.max_concurrency` | Per-component tuning |
| Resource limits | `resource.<component>.memory_bytes` (Phase 2) | Quota enforcement |
| Graceful degradation | `system.circuit_breaker.threshold`, `system.circuit_breaker.reset_s` | INV-031: degrade gracefully |

### 1.2 What Configuration MUST NEVER Control

| Category | Reasoning |
|----------|-----------|
| Core data model definitions | `Event` trait, `MessageEnvelope`, `BrowserOsError`, all ID types — these are code-level contracts, not tunables |
| Event handler logic | The behavior of event handlers is application code, not deployment config |
| Crate boundaries | Module composition (which crates exist, what they depend on) is a compile-time decision |
| Security trust zones | What constitutes a "system module" vs "plugin" is an architectural distinction, not a deployment switch |
| Clock abstraction | INV-004: Time is always injectable through `Clock` trait — config never supplies time sources |
| Event ordering guarantees | The causality chain (`correlation_id`, `causation_id`) is a protocol invariant, not configurable |
| State derivation rules | INV-006: State is derived from events — this is etched in stone |

### 1.3 What MUST Remain Runtime-Only

| Config | Why Runtime-Only |
|--------|------------------|
| `observability.log.level` | Operators change log levels during incident response — hot-reload target (Phase 2) |
| `observability.log.format` | Output format switch between prod and dev |
| `plugin.<name>.enabled` | LifecycleManager controls this, not config file |
| Stream positions | Owned by State Store, snapshot-based, not config |
| Active subscriptions | Owned by Event Bus runtime state |

### 1.4 What MUST Be Compile-Time

| Item | Reason |
|------|--------|
| Crate structure (11 crates) | Type-level boundaries enforced by compiler |
| ID types (EventId, ModuleId, etc.) | Type safety depends on distinct types |
| Event trait + marker traits | Event hierarchy is a design invariant |
| MessageEnvelope structure | Protocol definition — every module depends on it |
| Error type taxonomy | `ErrorKind`, `ErrorSeverity` — retry decisions depend on these |
| Architecture invariants (32 rules) | Code reviews enforce these — config cannot override |
| ComponentState machine transitions | 7 states, 11 transitions — compile-time validation |
| Serde serialization format | Binary format is a wire contract |

---

## Part 2 — Configuration Layer Boundaries

### 2.1 Global Config (`browseros_config::RootConfig`)

Config that applies system-wide, affecting infrastructure rather than individual components:

```yaml
system:
  circuit_breaker:
    threshold: 10
    reset_seconds: 60

observability:
  log:
    level: info
    format: json
    path: ./logs/browseros.log
  metrics:
    export_interval_s: 10
    otlp_endpoint: http://localhost:4317
  tracer:
    sampling_ratio: 0.1
    otlp_endpoint: http://localhost:4317

store:
  event_store:
    path: ./data/events
  snapshot:
    interval_s: 300
```

### 2.2 Component-Level Config

Every runtime component has its own config struct implementing `Config`:

```yaml
event:
  bus:
    buffer_size: 1024
    delivery_timeout_ms: 5000
    max_retries: 3
    backoff_base_ms: 100
    dead_letter_max_size: 10000

scheduler:
  max_concurrent_tasks: 100
  default_task_timeout_ms: 30000
  retry_default_max_retries: 3

lifecycle:
  health_check_interval_s: 30
  startup_timeout_s: 60
  shutdown_timeout_s: 30

dag:
  max_concurrency: 4
  default_node_timeout_ms: 15000
```

Component config structs live in their respective crate, NOT in `browseros-config`. The config crate only provides the loading mechanism.

### 2.3 What Belongs in RuntimeContext

| Item | Type | Source |
|------|------|--------|
| `root_config` | `Arc<RootConfig>` | From `browseros-config` — loaded at startup |
| `clock` | `Arc<dyn Clock>` | From `SystemClock` or `MockClock` (tests) |
| `cancellation_token` | `CancellationToken` | Created at startup, owned by `RuntimeHandle` |
| `event_bus` | `Arc<dyn EventBus>` | Injected by `RuntimeBuilder` |
| `scheduler` | `Arc<dyn Scheduler>` | Injected by `RuntimeBuilder` |
| `lifecycle_manager` | `Arc<dyn LifecycleManager>` | Injected by `RuntimeBuilder` |
| `plugin_registry` | `Arc<dyn PluginRegistry>` | Injected by `RuntimeBuilder` |

**Rule:** `RuntimeContext` holds references to active services. Config is loaded once and stored as `Arc<RootConfig>`. Components access their namespace through `root_config.for_component::<MyConfig>()`.

### 2.4 What Must NEVER Be Configurable

| Item | What Happens Instead |
|------|---------------------|
| Event type hierarchy | Defined in code — compile-time enforcement |
| Crate dependency graph | Enforced by `Cargo.toml` |
| Module lifecycle states | `ComponentState` enum — compile-time |
| Time source (Clock) | Injected via `RuntimeContext` |
| MessageEnvelope structure | Code-level protocol definition |
| Error taxonomy | Code-level — `ErrorKind`, `ErrorSeverity` |
| ID generation strategy | `Uuid::now_v7()` — code-level |
| Plugin trust boundaries | `ModuleType::Core` vs `ModuleType::Plugin` — compile-time |

---

## Part 3 — Distribution Strategy

### 3.1 Evaluation

| Strategy | Pros | Cons |
|----------|------|------|
| Single config file | Simple, predictable | No override mechanism, env-insecure |
| Layered config (defaults → file → env) | Standard pattern, every layer has clear responsibility | Merging complexity |
| Runtime overrides (hot reload) | Zero-downtime changes | INV-023 requires `ConfigChanged` events — Phase 2 |
| Environment-only | No file management | Impossible to configure complex structures |
| Multiple config files per component | Clean separation | File discovery, ordering, merge conflicts |

### 3.2 Decision: THREE-LAYER STATIC CONFIG

```
Layer 1: DefaultConfig (baked into binary)  — trust: high
Layer 2: Config file (YAML/TOML on disk)    — trust: medium
Layer 3: Environment variables (BROWSEROS_*) — trust: low
```

**Rules:**
1. Layer 1 must exist for every config field (INV-028).
2. Layer 2 merges on top of Layer 1 (file key wins).
3. Layer 3 merges on top of Layer 2 (env var wins).
4. Merging is shallow — env vars override at the leaf level (no deep partial merge ambiguity).
5. **No hot reload in Phase 1.** Config is loaded once at startup. Changing config requires restart.
6. Config changes are observable via `ConfigChanged` event on restart (simulated by publishing at startup in Phase 1, full hot-reload in Phase 2).

### 3.3 Environment Variable Convention

```
Pattern: BROWSEROS_<SECTION>_<KEY>
Example: BROWSEROS_EVENT_BUS_BUFFER_SIZE=2048
```

Environment variables use flat keys (underscore-separated) that map to dot-notation config paths:
```
BROWSEROS_EVENT_BUS_BUFFER_SIZE  →  event.bus.buffer_size
```

### 3.4 Plugin-Level Config Isolation

Each plugin gets its own config namespace under `plugin.<name>.*`:

```yaml
plugin:
  dom_sensor:
    enabled: true
    scan_interval_ms: 100
  vision:
    enabled: false
    model: vit-base
```

Plugins do NOT access global config directly. The `RootConfig` provides `plugin_config(name: &str) -> Option<Arc<dyn Any>>` which returns the deserialized plugin-specific config block. This prevents plugins from reading other components' config.

---

## Part 4 — Failure Modes

### 4.1 Corrupted Config File

| Scenario | Behavior | Error Message |
|----------|----------|---------------|
| YAML parse error | Startup failure | `"Config error: failed to parse /etc/browseros/config.yaml at line 42: expected string, found number"` |
| Invalid types | Startup failure | `"Config error: event.bus.buffer_size expected u32, got 'abc'"` |
| Unknown keys (strict mode) | Warning or error (configurable) | `"Config warning: unknown key 'event.bus.unknown_field' will be ignored"` |

**Policy:**
- ALL config is validated at startup, not lazily
- No silent fallback to defaults when file is corrupted — fail fast
- Validation reports ALL errors, not just the first one (accumulate and display)

### 4.2 Partially Missing Config

- INV-028 guarantees: every field has a default
- Missing optional fields → default value used
- Missing required (no-default) fields → validation error at startup
- Distinction between "not present" and "explicitly null" (YAML `~` vs absent key)

### 4.3 Config Conflicts Across Layers

- Clear precedence: Layer 3 > Layer 2 > Layer 1
- Environment variables always win — no ambiguity
- If an env var references a non-existent config key, it's warned but ignored
- If file and env var have different types for the same key, env var's type is coerced (string → target type via FromStr)

### 4.4 Recovery Strategy

```
Phase 1 recovery:
  1. Validate config at startup
  2. If invalid → log ALL errors → exit with non-zero code
  3. If valid → apply → start runtime
  4. No runtime fallback (config is static for process lifetime)

Phase 2 recovery (planned):
  1. Hot-reload: validate new config before applying
  2. On invalid reload → reject with error, keep running with old config
  3. Partial apply: components that succeed keep running, failed components log and continue with old config
```

---

## Part 5 — Security Constraints

### 5.1 Config Values That MUST NOT Be Externally Modifiable

| Value | Constraint | Mechanism |
|-------|-----------|-----------|
| Plugin scan paths | Must come from config file only (not env vars) | `plugin.scan_path` excluded from env layer |
| Module registration | Controlled by LifecycleManager, not config | `ModuleDescriptor` created by code, not config |
| Capability assignments | Controlled by PluginRegistry | CapabilityRegistry is code-initialized |
| Trust boundaries | `ModuleType` is compile-time | Plugin cannot declare itself as `Core` |
| Event subscription filters | Managed by Event Bus runtime | Subscription is runtime API, not config |

### 5.2 Config Injection Attack Prevention

| Attack Vector | Mitigation |
|---------------|-----------|
| Malicious config file | Config file path is absolute (no relative path swap). Permissions documented. File is parsed, not executed. |
| Environment variable injection | BROWSEROS_ prefix scoping. Whitelist-based env var mapping (unknown vars ignored). No eval/exec from config values. |
| Plugin path traversal | Plugin scan paths validated as existing directories before loading. Paths resolved after config load. |
| Arbitrary config keys | Config structs define exact expected keys. Unknown keys are warned and ignored. |
| Type confusion | Each config value deserialized to expected type. Type mismatch is a hard error (not coercion). |

### 5.3 Trust Boundaries

```
Binary defaults ──── HIGH trust ──── Maintained by the project
       │
       ▼
Config file ──────── MEDIUM trust ── Read-only, requires filesystem access
       │                             (documented minimum: 0600)
       ▼
Env variables ────── LOW trust ───── Broadest attack surface (process environment)
```

**Cross-boundary rule:** Higher-trust layers cannot be overridden by lower-trust layers in security-sensitive keys (plugin paths, trust classifications).

---

## Part 6 — Future Impact Analysis

### 6.1 Impact on Event System

```
browseros-event depends on browseros-config for:
  event.bus.buffer_size         — queue depth
  event.bus.delivery_timeout_ms — per-message delivery deadline
  event.bus.max_retries         — failed delivery retry count
  event.bus.dead_letter_max_size — dead letter retention
  event.bus.worker_count        — concurrent delivery workers (future)
```

**Design implication:** The Event Bus config struct (`EventBusConfig`) must be defined in `browseros-event` crate, not in `browseros-config`. The config crate only provides the loading mechanism. This ensures that changing event bus behavior doesn't require changes to the config crate.

### 6.2 Impact on Scheduler

```
browseros-scheduler depends on browseros-config for:
  scheduler.max_concurrent_tasks      — global pool limit
  scheduler.default_task_timeout_ms   — per-task timeout
  scheduler.retry_default_max_retries — default retry budget
```

**Design implication:** Scheduler's config namespace is `scheduler.*`. Task-level overrides come from task metadata (published with the task event), NOT from config. Config provides defaults only.

### 6.3 Impact on Plugin System

```
browseros-plugin depends on browseros-config for:
  plugin.scan_path         — directory to scan for plugins
  plugin.<name>.enabled    — per-plugin enable/disable
  plugin.<name>.path       — (optional) specific plugin binary path
```

**Design implication:** Plugin config uses a dynamic namespace (`plugin.<name>.*`). The config loader must support "wildcard" sections — if a key `plugin.dom_sensor.path` exists, it's routed to the `dom_sensor` plugin's config block. This requires `RootConfig::plugin_config(name: &str)` returning an opaque blob.

### 6.4 Impact on Resource Management

```
browseros-lifecycle (resource module) depends on browseros-config for:
  resource.<component>.memory_bytes  — per-component memory cap
  resource.<component>.max_tasks     — per-component task concurrency cap
```

**Design implication:** Resource config is similar to plugin config — per-component namespace. The same wildcard pattern applies: `resource.<component>.*` maps to `ResourceLimits` struct.

### 6.5 Impact on RuntimeContext

```
browseros-core depends on browseros-config for:
  root_config: Arc<RootConfig>  — passed through RuntimeContext
```

**Design implication:** `RuntimeContext` stores the fully loaded, validated, and merged `RootConfig`. Every component accesses their config via:

```rust
// In any component's constructor:
let my_config: Arc<EventBusConfig> = ctx
    .config()
    .for_component::<EventBusConfig>()
    .expect("event bus config must be valid");
```

This method does a runtime type check (downcast) from the stored `RootConfig` — config validation at startup guarantees this never fails at runtime.

---

## Part 7 — Decision Gate

### Analysis

**Strengths of the proposed design:**
- Three-layer static config is the simplest correct design
- INV-028 is trivially satisfied (defaults always work)
- Component configs are decoupled from loading mechanism
- No hot-reload complexity in Phase 1
- Security boundaries are clear
- Validation at startup catches all errors before any component runs

**Risks:**
1. **Config Trait API stability** — If the minimal `Config` trait changes after `browseros-observability` depends on it, 6+ crates need updates. Mitigation: keep trait to `namespace()` only.
2. **Wildcard sections** (`plugin.<name>.*`, `resource.<component>.*`) require dynamic dispatch in the config loader — slight complexity increase.
3. **No hot reload** means operator config changes require process restart. This is acceptable for Phase 1 — hot reload is explicitly Phase 2.

**Verdict:**

CONFIG DESIGN IS SAFE — PROCEED TO PHASE 1.2

The design follows the simplest correct architecture for Phase 1. The three-layer static config with startup validation covers all known requirements without premature complexity. Hot reload is deferred to Phase 2 with the architecture already supporting it (the `ConfigChanged` event slot is reserved in the event hierarchy).

One constraint for Phase 1.2 implementation:
> `browseros-config` provides mechanism (loading, merging, validation, environment binding). It does NOT define any component's config struct. Each component crate defines its own config struct and implements `Config`.

