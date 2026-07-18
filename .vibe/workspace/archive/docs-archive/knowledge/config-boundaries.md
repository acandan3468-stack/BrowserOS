ARCHIVED

This document has been archived. Its content has been merged into the canonical documentation set (design-decisions.md).
See docs/design-decisions.md for current information.

---

# Configuration Layer Boundaries — BrowserOS

## The Config Crate's Contract

`browseros-config` is a mechanism crate. It provides the infrastructure for loading, merging, validating, and accessing configuration. It does NOT define any component's config structure.

```
browseros-config public API:
  RootConfig            — merged config tree
  ConfigSource          — enum(Defaults, File(path), Env)
  ConfigLoader          — builder: add_source → load → validate
  ConfigError           — parse error, validation error, missing field error
  Config trait          — implemented by each component's config struct

browseros-config does NOT define:
  EventBusConfig          ← lives in browseros-event
  LoggerConfig            ← lives in browseros-observability
  SchedulerConfig         ← lives in browseros-scheduler
  StoreConfig             ← lives in browseros-store
  PluginConfig            ← lives in browseros-plugin
  LifecycleConfig         ← lives in browseros-lifecycle
```

This boundary is **mandatory**: if `browseros-config` ever imports a component crate, it creates a circular dependency (component → config → component).

---

## Layer Architecture

```
                    ┌──────────────────────┐
                    │   Environment Vars   │  Layer 3 (LOW trust)
                    │   BROWSEROS_*_KEY    │  Overrides everything
                    └──────────┬───────────┘
                               │ merge
                    ┌──────────▼───────────┐
                    │   Config File        │  Layer 2 (MEDIUM trust)
                    │   config.yaml        │  Overrides defaults
                    └──────────┬───────────┘
                               │ merge
                    ┌──────────▼───────────┐
                    │   Default Config     │  Layer 1 (HIGH trust)
                    │   (baked into binary)│  Must produce working system
                    └──────────┬───────────┘
                               │ validate
                    ┌──────────▼───────────┐
                    │   RootConfig         │  Consumed by RuntimeContext
                    │   (Arc, immutable)   │  Components access their slice
                    └──────────────────────┘
```

### Layer 1 — Default Config

Location: `browseros-config/src/defaults.rs`

```rust
impl Default for RootConfig {
    fn default() -> Self {
        Self {
            event: EventBusConfig {
                buffer_size: 1024,
                delivery_timeout_ms: 5000,
                max_retries: 3,
                backoff_base_ms: 100,
                dead_letter_max_size: 10_000,
            },
            scheduler: SchedulerConfig { /* ... */ },
            observability: ObservabilityConfig { /* ... */ },
            store: StoreConfig { /* ... */ },
            lifecycle: LifecycleConfig { /* ... */ },
            dag: DagConfig { /* ... */ },
            plugin: PluginRootConfig { /* ... */ },
            system: SystemConfig { /* ... */ },
        }
    }
}
```

INV-028 requires: every field has a value. No `Option<T>` without a documented default behavior.

### Layer 2 — Config File

Location: configurable path (default: searches `./browseros.yaml`, `./config/browseros.yaml`, `/etc/browseros/config.yaml`)

Format: YAML (TOML considered but YAML has better ecosystem support for hierarchical config, serde integration, and inline documentation)

```yaml
event:
  bus:
    buffer_size: 2048              # override default of 1024
    delivery_timeout_ms: 10000     # override default of 5000
```

The file parser deserializes into `RootConfig` directly using serde. Unknown keys are warned but not errored (forward compatibility when newer versions add fields that older config files don't know about).

### Layer 3 — Environment Variables

Naming: `BROWSEROS_<SECTION>_<KEY>` (all caps, underscore separated)

```bash
export BROWSEROS_EVENT_BUS_BUFFER_SIZE=4096
export BROWSEROS_SCHEDULER_MAX_CONCURRENT_TASKS=200
```

Environment variables are mapped to config keys by the env source, which:
1. Strips `BROWSEROS_` prefix
2. Converts to lowercase dot-notation: `event.bus.buffer_size`
3. Sets the value as a string (serde handles type coercion from string)
4. Unknown keys (not matching any field in RootConfig) are warned and ignored

A whitelist of env-accessible keys is maintained to prevent security-sensitive fields from being set via env:

```rust
/// Keys that are NOT accessible via environment variables.
const ENV_BLOCKLIST: &[&str] = &[
    "plugin.scan_path",
    "store.event_store.path",
];
```

---

## Component Config Access Pattern

Every component accesses config through `RuntimeContext`, never by parsing files directly:

```rust
// In component constructor:
impl MyComponent {
    pub fn new(ctx: Arc<RuntimeContext>) -> Self {
        let config: Arc<MyConfig> = ctx
            .config()
            .for_component::<MyConfig>()
            .expect("MyConfig must be valid — validated at startup");
        // ...
    }
}
```

`RootConfig::for_component::<T>()` returns `Option<Arc<T>>`. It works by:
1. The component's config struct implements `Config` (provides namespace)
2. `RootConfig` stores a `HashMap<&'static str, Arc<dyn Any>>` of validated config blocks
3. `for_component` looks up by `T::namespace()` and downcasts

This means validation happens ONCE at startup. After that, config access is an O(1) HashMap lookup + pointer cast.

---

## Config Trait

```rust
/// Every component config block implements this trait.
pub trait Config: Debug + Send + Sync + 'static + DeserializeOwned {
    /// The dot-notation namespace for this config block (e.g. "event.bus").
    fn namespace() -> &'static str;
}
```

Components do not need to know about `RootConfig`, `ConfigSource`, `ConfigLoader`, or `ConfigError`. They only need:
1. Their own `Config` struct (defined in their crate)
2. `Config` trait impl
3. `RuntimeContext::config().for_component::<Self>()`

This minimizes coupling: changing the config loading mechanism does NOT affect components.

---

## Plugin Config Isolation

Plugins live under `plugin.<name>.*` namespace. The config loader handles this with a special deserialization step:

```yaml
plugin:
  dom_sensor:
    enabled: true
    scan_interval_ms: 100
```

`PluginRootConfig` stores a `HashMap<String, serde_json::Value>` keyed by plugin name. When a plugin requests its config:

```rust
let my_config: DomSensorConfig = ctx
    .config()
    .for_plugin::<DomSensorConfig>("dom_sensor")
    .expect("dom_sensor config must be valid");
```

This is dynamically typed — each plugin's config is independently deserialized to its own struct type. If deserialization fails, the plugin load fails with a clear error message.

**Isolation guarantee:** Plugin A can NEVER read Plugin B's config. `for_plugin::<T>(name)` only returns the config block for `plugin.<name>.*`.

---

## Layer Conflict Resolution Rules

```
Rule 1: Exact match wins
  Env var BROWSEROS_EVENT_BUS_BUFFER_SIZE=4096
  Config file: event.bus.buffer_size: 2048
  Result: 4096 (env wins)

Rule 2: Partial override copies entire parent
  Env var BROWSEROS_EVENT_BUS_BUFFER_SIZE=4096
  Config file: event.bus.delivery_timeout_ms: 10000
  The "event.bus" block is NOT merged at field level.
  Instead, the whole block from the file is used as base,
  then env overrides are applied field-by-field.
  This prevents inconsistent partial merges.

Rule 3: Unknown env vars are warned, not errored
  BROWSEROS_XYZZY=42 → warning at startup: "Unknown config key: xyzzy"

Rule 4: Env blocklist is enforced
  BROWSEROS_PLUGIN_SCAN_PATH=/malicious → error at startup:
  "Config error: plugin.scan_path cannot be set via environment variable"
```

---

## Validation Flow

```
ConfigLoader::load()
    │
    ├── 1. Deserialize defaults → RootConfig
    ├── 2. Deserialize file     → RootConfig (patch defaults)
    ├── 3. Map env vars         → RootConfig (patch file)
    │
    ├── 4. Validate merged config
    │       │
    │       ├── Range checks: buffer_size > 0, timeout < max, etc.
    │       ├── Consistency: dead_letter_max_size >= buffer_size
    │       └── Security: blocked keys not set via env
    │
    ├── 5. Collect ALL errors (not just first)
    │       If errors exist → log all → return Err(ConfigError::Validation { errors: [...] })
    │
    └── 6. Split into per-component blocks
            Store in HashMap<&'static str, Arc<dyn Any>>
            Return Ok(RootConfig { blocks })
```

**Key principle:** Validate all, fail fast, report everything. The user gets a complete list of every config problem in one startup attempt.

