ARCHIVED

This document has been archived. Its content has been merged into the canonical documentation set (design-decisions.md).
See docs/design-decisions.md for current information.

---

# Risk Analysis — Configuration System Design

## Risk Register

### CR-01: Config Trait API Instability (Medium-High)

| Field | Value |
|-------|-------|
| **Description** | The `Config` trait is the only shared interface between `browseros-config` and every component crate. If it changes after Phase 1.2, all downstream crates (observability, event, scheduler, store, plugin, lifecycle, core) must be updated. |
| **Likelihood** | Low — the trait is minimal (1 method `namespace()`). But if we discover a need for `validate()`, `merge()`, or `default()` on the trait, the signature changes cascade. |
| **Impact** | High — 7+ crates would need simultaneous updates. Each crate's config struct would need new impl methods. |
| **Mitigation** | Make `namespace()` the ONLY required method on `Config`. Keep validation as an external function in `browseros-config`, not on the trait. Defaults come from `Default` impl on each struct, not from the trait. This keeps the trait surface-area zero beyond identification. |
| **Residual risk** | Very Low — `namespace()` returning `&'static str` is the simplest possible trait. It's hard to imagine needing to change it. |

### CR-02: Dynamic Plugin Config Namespace Complexity (Medium)

| Field | Value |
|-------|-------|
| **Description** | Plugin config requires dynamic dispatch: `plugin.<name>.*` keys must be routed to per-plugin config structs at runtime. This requires `HashMap<String, serde_json::Value>` or similar in `RootConfig`, with per-plugin downcasting. |
| **Likelihood** | Certain — plugin config IS in the design. |
| **Impact** | Medium — the dynamic dispatch adds code complexity. If done wrong, plugins could read each other's config (security violation). |
| **Mitigation** | Use a private `HashMap<String, Arc<dyn Any>>` in `RootConfig`. Expose ONLY `for_plugin::<T>(name: &str) -> Option<Arc<T>>` — no raw access to the HashMap. Each plugin's config block is deserialized independently. Server-side validation catches misconfigurations at startup. |
| **Residual risk** | Low — the pattern is well-understood (type-erased config blocks). |

### CR-03: No Hot Reload — Config Changes Require Restart (Low-Medium)

| Field | Value |
|-------|-------|
| **Description** | Phase 1 config is static (loaded once at startup). Operators who change config files must restart the process. |
| **Likelihood** | Certain — this is by design. |
| **Impact** | Medium — restart causes brief downtime. For a local browser agent runtime (not a web server), this is acceptable. Zero-downtime reload is not a Phase 1 requirement. |
| **Mitigation** | Design the config loading and validation infrastructure to SUPPORT hot reload (through `ConfigChanged` event + `ArcSwap<RootConfig>`), but DON'T IMPLEMENT IT in Phase 1. The slot is reserved. |
| **Residual risk** | Low — Phase 1 scope explicitly excludes hot reload. |

### CR-04: Environment Variable Type Coercion (Low)

| Field | Value |
|-------|-------|
| **Description** | Environment variables are always strings. Config values may be integers, floats, booleans, durations, or nested structures. Type coercion from string must handle all config types. |
| **Likelihood** | Certain — env vars are strings by nature. |
| **Impact** | Low — serde already handles string-to-type deserialization. `Duration` fields need special handling (parse "5s" or accept milliseconds integer). |
| **Mitigation** | Use serde's built-in string deserialization. For durations, accept both "5s" (string) and 5000 (integer milliseconds) with `#[serde(untagged)]` or a custom deserializer. |
| **Residual risk** | Low — well-trodden path. |

### CR-05: File Permission Exposure (Low)

| Field | Value |
|-------|-------|
| **Description** | Config file may contain sensitive paths (e.g., store paths that reveal data layout). If file permissions are loose, an attacker could read the config to learn system layout. |
| **Likelihood** | Low — depends on deployment environment. |
| **Impact** | Low — config file does NOT contain secrets (passwords, API keys, tokens). Those belong in a separate secrets manager (Phase 2+). Config paths are information, not credentials. |
| **Mitigation** | Document that config file should be readable only by the browseros process owner (0600). No secrets stored in config. |
| **Residual risk** | Very Low — information disclosure only, not credential exposure. |

### CR-06: Forward Compatibility of Config File Format (Low)

| Field | Value |
|-------|-------|
| **Description** | As new config fields are added in future versions, old config files won't have them. If deserialization is strict, old files will fail to load with new binaries. |
| **Likelihood** | Certain — config will grow over time. |
| **Impact** | Medium — users would be forced to update config files on every upgrade. |
| **Mitigation** | Use `#[serde(default)]` on every field in every config struct. Unknown keys are warned, not errored. This means old config files work with new binaries; missing fields get defaults. |
| **Residual risk** | Very Low — standard serde pattern, well-proven. |

### CR-07: RuntimeContext Becomes a God Object (Medium-High)

| Field | Value |
|-------|-------|
| **Description** | If every component accesses config through `RuntimeContext`, and `RuntimeContext` also holds services (EventBus, Scheduler, etc.), it may accumulate too many responsibilities. |
| **Likelihood** | Medium — this is a known risk of the context pattern. |
| **Impact** | High — a God object creates implicit coupling between all components. Changes to one service's config affect the central object. |
| **Mitigation** | **Strict layering:** `RuntimeContext` holds references, not behavior. Config is one field (`config: Arc<RootConfig>`). Services are separate fields (`event_bus: Arc<dyn EventBus>`). Components extract only what they need in their constructor. The `RuntimeBuilder` is the only code that populates `RuntimeContext`. No component ever writes to `RuntimeContext`. |
| **Residual risk** | Low — the facade pattern is intentional. `RuntimeContext` is THE integration point. The risk is mitigated by keeping it as a pure data holder with no business logic. |

---

## Risk Matrix

| ID | Severity | Likelihood | Detection | Priority | Action |
|----|----------|------------|-----------|----------|--------|
| CR-01 | High | Low | Late (compile error in downstream crate) | **HIGH** | Keep `Config` trait to 1 method. Freeze before Phase 1.2. |
| CR-02 | Medium | Certain | Early (runtime error at plugin load) | **Medium** | Isolated dynamic dispatch with type erasure. Test thoroughly. |
| CR-03 | Medium | Certain | N/A (by design) | **Low** | Accept. Slot reserved for Phase 2. |
| CR-04 | Low | Certain | Early (test coverage) | **Low** | Standard serde string deserialization. |
| CR-05 | Low | Low | Late (security audit) | **Low** | Document permissions. No secrets in config. |
| CR-06 | Low | Certain | Late (user reports missing fields) | **Medium** | `#[serde(default)]` on all fields. Ignore unknown keys. |
| CR-07 | High | Medium | Late (tight coupling emerges) | **HIGH** | Pure data holder. No logic. Strict separation. |

---

## Decision Gate

**Verdict: CONFIG DESIGN IS SAFE — PROCEED TO PHASE 1.2**

### Justification

1. **The Config trait is minimal enough to freeze.** `namespace() -> &'static str` is unlikely to need change. This is the single point of coupling between `browseros-config` and all downstream crates.

2. **Three-layer static config is the simplest correct design.** It satisfies INV-028 (defaults always work), INV-029 (dot-notation + BROWSEROS_ prefix), and INV-023 (config changes are observable — by restart in Phase 1, by event in Phase 2).

3. **Component config isolation prevents coupling.** Each crate defines its own config struct. Changing one component's config never affects another. The `RootConfig` is the only integration point.

4. **The biggest risk was verified as acceptable.** CR-01 (Config trait changes) is mitigated by making the trait impossibly simple. CR-07 (God object) is mitigated by making RuntimeContext a pure data holder.

### Phase 1.2 Implementation Requirements

These constraints MUST be followed during Phase 1.2 implementation:

| # | Constraint | Reason |
|---|-----------|--------|
| 1 | `Config` trait must have EXACTLY 1 method: `fn namespace() -> &'static str` | CR-01: every method beyond namespace is a coupling point |
| 2 | `browseros-config` must NOT import any component crate | Circular dependency prevention |
| 3 | Every config field must have `#[serde(default)]` | CR-06: forward compatibility |
| 4 | Unknown keys in config file must be warned, not errored | CR-06: forward compatibility |
| 5 | `for_plugin::<T>(name)` must be the ONLY plugin config access path | CR-02: isolation guarantee |
| 6 | `RootConfig` must expose config blocks via `Arc<dyn Any>` downcast, not direct field access | CR-07: decouples config structure from components |
| 7 | Validation must collect ALL errors before returning | Fail-fast, report-everything principle |
| 8 | Environment variable blocklist must be enforced | CR-05: security boundary |
| 9 | `ConfigError` must include source path and line number for file errors | Debuggability |
| 10 | No hot reload infrastructure in Phase 1 — defer to Phase 2 | Scope control |

