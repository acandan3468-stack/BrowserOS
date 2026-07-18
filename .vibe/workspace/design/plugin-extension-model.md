# Plugin & Extension Model

**Principle:** Plugins extend BrowserOS without modifying core crates. Everything a plugin can do is defined by the capabilities it registers and the events it subscribes to.

---

## 1. Extension Points

```
Plugin → registers capabilities → advertised to all subscribers
Plugin → subscribes to events → reacts to browser state changes
Plugin → emits custom events → publishes typed domain events
Plugin → provides services → injectable through RuntimeContext
Plugin → intercepts hooks → wraps existing trait methods
Plugin → defines commands → callable by agents and other plugins
```

### 1.1 Capabilities

A capability is a named, versioned, typed contract:

```rust
CapabilityDefinition {
    id: CapabilityId,       // e.g. "browser.screenshot"
    version: SemVer,        // e.g. "1.0.0"
    description: String,    // "Take full-page or element screenshots"
    interface: String,      // Schema or trait name for type-level checking
}
```

Capabilities are registered in the `CapabilityRegistry`. Any plugin can query:

```rust
if runtime.plugin().has_capability(&CapabilityId::from_string("browser.screenshot")) {
    // screenshot is available
}
```

### 1.2 Commands

A command is a callable operation. Commands are registered by plugins and invoked by any caller through the plugin service:

```rust
// Plugin registers a command
plugin_registry.register_command(CommandDefinition {
    name: "ai.extract_structured_data",
    handler: Arc::new(|params: Value| -> Result<Value> {
        // extract structured data from page content
    }),
});

// Any caller invokes it
let result = runtime.plugin().execute(
    "ai.extract_structured_data",
    json!({ "schema": { "name": "string", "price": "number" } })
)?;
```

### 1.3 Hooks

Hooks wrap existing bridge trait methods. A plugin can intercept any `PagePort`, `BrowserPort`, etc. method:

```rust
// Hook registration
plugin_registry.register_hook(HookDefinition {
    target: "PagePort::navigate",
    phase: HookPhase::Before,  // or After, or Around
    handler: Arc::new(|params: HookParams| -> HookResult {
        let url = params.args["url"].as_str().unwrap();
        info!("About to navigate to: {url}");
        HookResult::Proceed  // or HookResult::Override(...) or HookResult::Abort(...)
    }),
});
```

Hook phases:
- `Before` — runs before the target method, can abort or modify args
- `After` — runs after the target method, can modify the result
- `Around` — replaces the target method entirely

### 1.4 Custom Events

Plugins emit typed domain events:

```rust
// Plugin defines a custom event
pub struct AiAnalysisCompleted {
    pub metadata: EventMetadata,
    pub page_id: PageId,
    pub analysis_type: String,
    pub result: Value,
}

impl Event for AiAnalysisCompleted {
    fn kind(&self) -> &'static str { "ai.analysis.completed" }
    fn category(&self) -> EventCategory { EventCategory::Domain }
    fn metadata(&self) -> &EventMetadata { &self.metadata }
    fn as_any(&self) -> &dyn Any { self }
}

// Plugin publishes it
bus.publish(Box::new(AiAnalysisCompleted { ... }));
```

Other plugins and agents subscribe to custom events without knowing the emitting plugin.

### 1.5 Services

Plugins can provide injectable services. Services are registered by trait type:

```rust
plugin_registry.register_service::<dyn AiService>(Box::new(MyAiServiceImpl {
    api_key: config.get("api_key"),
}));

// Another plugin retrieves the service
let ai_service = runtime.plugin().get_service::<dyn AiService>()?;
```

---

## 2. Plugin Lifecycle

```
Register ─► Load ─► Init ─► Start ─► Running ─► Stop ─► Unload
                  │                           │
                  └─► Error ──────────────────┘
```

| Phase | Description |
|-------|-------------|
| Register | Plugin descriptor added to registry, capabilities checked |
| Load | Binary loaded (WASM/native), entry point resolved |
| Init | `Plugin::init()` called — setup, register capabilities, subscribe to events |
| Start | `Plugin::start()` called — begin active work |
| Running | Plugin is active, responding to events and commands |
| Stop | `Plugin::stop()` called — flush, unsubscribe, release resources |
| Unload | Plugin descriptor removed from registry |

---

## 3. Plugin Types

### 3.1 Built-in Plugins (in-crate)

Rust trait objects compiled into BrowserOS:

```rust
let plugin = MyPlugin::new();
runtime.plugin().register(Box::new(plugin))?;
```

### 3.2 WASM Plugins

WASM bytecode loaded at runtime:

```rust
// Config specifies plugin paths
plugins:
  - name: "ai-extractor"
    path: "./plugins/ai-extractor.wasm"
    enabled: true
```

WASM plugins run in a sandboxed environment. Communication is through:
1. WIT-defined interface (WebAssembly Interface Types)
2. `PluginContext` — access to EventBus, Logger, Config
3. Callback-based — no direct access to host memory

### 3.3 External Process Plugins

Separate processes communicating over stdin/stdout or Unix sockets:

```rust
PluginEntryPoint::External(PathBuf)  // shared library or executable
```

---

## 4. PluginContext

The minimal context given to each plugin:

```rust
#[derive(Clone)]
pub struct PluginContext {
    pub bus: Arc<EventBus>,
    pub logger: Arc<Logger>,
    pub metrics: Arc<MetricsRegistry>,
    pub config: Arc<dyn PluginConfigAccess>,
    pub plugin_id: PluginId,
    pub data_dir: PathBuf,
}

impl PluginContext {
    pub fn capability_registry(&self) -> Arc<CapabilityRegistry>;
    pub fn plugin_registry(&self) -> Arc<PluginRegistry>;
}
```

**Key rule:** Plugins do NOT get access to `RuntimeContext` directly. They get `PluginContext`, which is a subset. This prevents plugins from depending on the composition root.

---

## 5. Capability Negotiation

Before a plugin starts, the `CapabilityRegistry` checks:

```rust
fn check_requirements(plugin: &dyn Plugin) -> Result<()> {
    for req in plugin.required_capabilities() {
        if !self.has(&req) {
            return Err(BrowserOsError::unsupported(
                "Missing required capability: {req}"
            ));
        }
    }
    Ok(())
}
```

If required capabilities are missing, the plugin is rejected with a clear error.

### Capability Conflict

If two plugins register the same capability ID, the second registration fails unless the version is higher (newer version replaces older):

```rust
fn register(&self, capability: CapabilityDefinition) -> Result<()> {
    if let Some(existing) = self.get(&capability.id) {
        if capability.version <= existing.version {
            return Err(BrowserOsError::resource_exhausted(
                "Capability already registered with version >= {existing.version}"
            ));
        }
    }
    // replace or insert
}
```

---

## 6. Isolation & Security

| Concern | Mechanism |
|---------|-----------|
| Memory safety | WASM sandbox (for WASM plugins) |
| File system | `data_dir` is the only writable path; configured per plugin |
| Network | Plugins cannot open raw sockets; use EventBus for IPC |
| Event flooding | Plugin runs in publisher's thread; slow plugin = slow events |
| CPU exhaustion | WASM plugins have instruction budget; native plugins are trusted |
| Capability escalation | Plugin cannot use capabilities it didn't declare as required |

---

## 7. Configuration

Plugins receive isolated config under `plugin.<name>`:

```yaml
plugin:
  my_plugin:
    api_key: "sk-..."
    model: "gpt-4"
    max_retries: 3
```

Plugin config is retrieved via:

```rust
let cfg: MyPluginConfig = ctx.config.for_plugin("my_plugin")?;
```

No plugin can read another plugin's config. The `ConfigSource::Environment` path `plugin.*` is blocked for env var injection.

---

## 8. Built-in Capabilities for Phase 2

These capabilities are registered by the core BrowserOS crates during boot:

| Capability ID | Description | Owner |
|--------------|-------------|-------|
| `browser.launch` | Launch and manage browser processes | browseros-browser |
| `browser.connect` | Connect to existing browser | browseros-browser |
| `page.navigate` | Navigate pages | browseros-page |
| `page.screenshot` | Take screenshots | browseros-page |
| `page.pdf` | Generate PDFs | browseros-page |
| `page.javascript` | Execute JavaScript | browseros-page |
| `dom.query` | Query DOM elements | browseros-dom |
| `dom.snapshot` | Take DOM snapshots | browseros-dom |
| `input.click` | Click elements | browseros-input |
| `input.type` | Type text | browseros-input |
| `input.fill` | Fill form fields | browseros-input |
| `network.monitor` | Monitor network traffic | browseros-network |
| `network.intercept` | Intercept network requests | browseros-network |
| `storage.cookies` | Manage cookies | browseros-storage |
| `storage.web` | Read/write web storage | browseros-storage |
| `artifact.store` | Store artifacts | browseros-artifact |
| `artifact.retrieve` | Retrieve artifacts | browseros-artifact |
