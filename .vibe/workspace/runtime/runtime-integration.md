# Runtime Integration — Connecting Phase 2 to RuntimeContext

**Principle:** RuntimeContext is the composition root. No subsystem depends on RuntimeContext. RuntimeContext depends on everything. This prevents circular dependencies and ensures testability.

---

## 1. Current RuntimeContext

```rust
pub struct RuntimeContext {
    bus: Arc<EventBus>,
    logger: Arc<Logger>,
    metrics: Arc<MetricsRegistry>,
    tracer: Arc<Tracer>,
    config: Arc<RootConfig>,
    lifecycle: Arc<LifecycleManager>,
    scheduler: Arc<Scheduler>,
}
```

Phase 2 adds browser-related subsystems. The key question: **how** does RuntimeContext expose them?

### Anti-Pattern: God Object

```rust
// ❌ BAD — RuntimeContext would become unmanageable
pub struct RuntimeContext {
    // Phase 1 fields...
    bus, logger, metrics, tracer, config, lifecycle, scheduler,
    // Phase 2 fields — every new subsystem as a direct field
    browser_manager, plugin_registry, capability_registry,
    artifact_store,
    // Phase N fields...
    // This grows forever.
}
```

### Solution: Service Aggregators

RuntimeContext exposes **aggregator handles** that group related subsystems:

```rust
#[derive(Clone)]
pub struct RuntimeContext {
    // Phase 1 — core infrastructure
    bus: Arc<EventBus>,
    logger: Arc<Logger>,
    metrics: Arc<MetricsRegistry>,
    tracer: Arc<Tracer>,
    config: Arc<RootConfig>,
    lifecycle: Arc<LifecycleManager>,
    scheduler: Arc<Scheduler>,

    // Phase 2 — browser services
    browser: Arc<BrowserService>,       // aggregate for browser operations
    plugin: Arc<PluginService>,         // aggregate for plugin operations
    artifact: Arc<ArtifactStore>,       // standalone storage
}
```

Each aggregate is a struct that holds references to its sub-services:

```rust
pub struct BrowserService {
    manager: Arc<BrowserManager>,
    dom: Arc<DomService>,
    page: Arc<PageService>,
    network: Arc<NetworkService>,
    input: Arc<InputService>,
    storage: Arc<StorageService>,
}
```

---

## 2. Service Aggregator Design

### BrowserService

```rust
#[derive(Clone)]
pub struct BrowserService {
    manager: Arc<BrowserManager>,
    dom: Arc<DomService>,
    page: Arc<PageService>,
    network: Arc<NetworkService>,
    input: Arc<InputService>,
    storage: Arc<StorageService>,
}

impl BrowserService {
    /// Browser lifecycle
    pub fn launch(&self, options: LaunchOptions) -> Result<BrowserHandle>;
    pub fn connect(&self, endpoint: &str) -> Result<BrowserHandle>;
    pub fn browsers(&self) -> Vec<BrowserHandle>;
    pub fn default_browser(&self) -> Option<BrowserHandle>;

    /// DOM operations — requires a PageHandle
    pub fn locator(&self, page: &PageHandle) -> LocatorBuilder;
    pub fn snapshot(&self, page: &PageHandle) -> Result<DomSnapshot>;

    /// Page operations
    pub fn wait(&self, page: &PageHandle, condition: WaitCondition) -> Result<()>;
    pub fn extract_text(&self, page: &PageHandle) -> Result<String>;
    pub fn auto_handle_dialogs(&self, page: &PageHandle, strategy: DialogStrategy) -> Result<()>;

    /// Network
    pub fn capture(&self, page: &PageHandle) -> Result<NetworkCapture>;

    /// Input — convenience methods that find + act
    pub fn click(&self, page: &PageHandle, selector: &str) -> Result<()>;
    pub fn fill(&self, page: &PageHandle, selector: &str, text: &str) -> Result<()>;
    pub fn type_text(&self, page: &PageHandle, selector: &str, text: &str) -> Result<()>;
}
```

### PluginService

```rust
#[derive(Clone)]
pub struct PluginService {
    plugins: Arc<PluginRegistry>,
    capabilities: Arc<CapabilityRegistry>,
}

impl PluginService {
    pub fn register(&self, plugin: Box<dyn Plugin>) -> Result<PluginId>;
    pub fn unregister(&self, id: &PluginId) -> Result<()>;
    pub fn get(&self, id: &PluginId) -> Option<Arc<dyn Plugin>>;
    pub fn list(&self) -> Vec<PluginDescriptor>;
    pub fn find_by_capability(&self, cap: &CapabilityId) -> Vec<PluginDescriptor>;
    pub fn has_capability(&self, cap: &CapabilityId) -> bool;
    pub fn start_all(&self) -> Result<()>;
    pub fn stop_all(&self) -> Result<()>;
}
```

---

## 3. Construction Flow

```
RuntimeContext::init(config_path)
  │
  ├── 1. Load config (Phase 1 — unchanged)
  │
  ├── 2. Create core infrastructure (Phase 1 — unchanged)
  │   ├── EventBus
  │   ├── Logger + Metrics + Tracer
  │   ├── Scheduler
  │   └── LifecycleManager
  │
  ├── 3. Create Phase 2 subsystems
  │   ├── ArtifactStore(base_path)
  │   ├── BrowserManager(bus, config, logger)
  │   │   └── DomService, PageService, NetworkService,
  │   │       InputService, StorageService
  │   └── PluginRegistry(bus, logger)
  │       └── CapabilityRegistry
  │
  ├── 4. Enable lifecycle integration
  │   ├── LifecycleManager::register_component("browser")
  │   ├── LifecycleManager::register_component("plugin_system")
  │   └── LifecycleManager::register_component("artifact_store")
  │
  └── 5. Return RuntimeContext
```

### Construction Code Sketch

```rust
impl RuntimeContext {
    pub fn init(config_path: &Path) -> Result<Self, RuntimeInitError> {
        // Phase 1 — unchanged
        let config = load_config(config_path)?;
        let bus = Arc::new(EventBus::new());
        let logger = Arc::new(Logger::new(...));
        let metrics = Arc::new(MetricsRegistry::new());
        let tracer = Arc::new(Tracer::new());
        let scheduler = Arc::new(Scheduler::new(bus.clone()));
        let lifecycle = Arc::new(LifecycleManager::new(bus.clone(), logger.clone(), ...));
        lifecycle.register_component("runtime");

        // Phase 2
        let artifact_config = config.for_component::<ArtifactConfig>().unwrap_or_default();
        let artifact_store = Arc::new(ArtifactStore::new(artifact_config.base_path));

        let browser_config = config.for_component::<BrowserConfig>().unwrap_or_default();
        let browser_manager = Arc::new(BrowserManager::new(
            bus.clone(), logger.clone(), &browser_config
        ));

        let plugin_config = config.for_component::<PluginConfig>().unwrap_or_default();
        let plugin_registry = Arc::new(PluginRegistry::new(bus.clone(), logger.clone()));
        let capability_registry = Arc::new(CapabilityRegistry::new());

        // Lifecycle integration
        lifecycle.register_component("browser");
        lifecycle.register_component("plugin_system");
        lifecycle.register_component("artifact_store");

        // Phase 1 transition
        lifecycle.transition_to("runtime", LifecycleState::Running);

        Ok(RuntimeContext {
            bus, logger, metrics, tracer, config,
            lifecycle, scheduler,
            browser: Arc::new(BrowserService {
                manager: browser_manager,
                dom: Arc::new(DomService::new()),
                page: Arc::new(PageService::new()),
                network: Arc::new(NetworkService::new()),
                input: Arc::new(InputService::new()),
                storage: Arc::new(StorageService::new()),
            }),
            plugin: Arc::new(PluginService {
                plugins: plugin_registry,
                capabilities: capability_registry,
            }),
            artifact: artifact_store,
        })
    }
}
```

---

## 4. Accessor Methods

RuntimeContext exposes accessors for each aggregate:

```rust
impl RuntimeContext {
    // Phase 1 — unchanged
    pub fn bus(&self) -> &Arc<EventBus>;
    pub fn logger(&self) -> &Arc<Logger>;
    pub fn metrics(&self) -> &Arc<MetricsRegistry>;
    pub fn tracer(&self) -> &Arc<Tracer>;
    pub fn config(&self) -> &Arc<RootConfig>;
    pub fn lifecycle(&self) -> &Arc<LifecycleManager>;
    pub fn scheduler(&self) -> &Arc<Scheduler>;

    // Phase 2
    pub fn browser(&self) -> &Arc<BrowserService>;
    pub fn plugin(&self) -> &Arc<PluginService>;
    pub fn artifacts(&self) -> &Arc<ArtifactStore>;
}
```

---

## 5. Subsystem Communication

Subsystems communicate through **two mechanisms**:

### 5.1 Direct API Calls (synchronous)

```
Agent code → RuntimeContext.browser().launch() → BrowserManager
            → RuntimeContext.browser().click(page, "#btn")
```

Direct calls are for imperative commands. The caller blocks until the operation completes or fails.

### 5.2 EventBus Events (asynchronous observation)

```
CDP event → CdpSession dispatcher → EventBus.publish(BrowserEvent)
Agent or Plugin → bus.subscribe("page.created", handler)
```

Events are for observation. Agents subscribe to events to react to browser state changes without polling.

### 5.3 Cross-Subsystem Coordination via EventBus

- DOM changes → `ElementAttached`, `ElementDetached`
- Network activity → `RequestStarted`, `ResponseReceived`
- Navigation → `NavigationStarted`, `NavigationFinished`
- Dialog → `DialogOpened`

These events flow through the EventBus. Plugins and agents can subscribe without knowing about each other.

---

## 6. Preventing Circular Dependencies

The dependency direction is always:

```
browseros-runtime → (everything)
```

No Phase 2 crate depends on browseros-runtime. The composition is one-way:

```
                    ┌──────────────────┐
                    │  browseros-runtime│ (depends on ALL)
                    └──┬───┬───┬───┬───┘
                       │   │   │   │
              ┌────────┘   │   │   └────────┐
              ▼            ▼   ▼             ▼
       browseros-browser  DOM  Network ...  Plugin
              │            │   │             │
              └──────┬─────┘   │             │
                     ▼         │             │
              browseros-bridge ◄─────────────┘
                     │
                     ▼
              browseros-types
```

**Key rule:** If crate A needs something from crate B, but B is "above" A in this graph, the dependency is inverted: move the needed type or trait into `browseros-bridge` or `browseros-types`.

---

## 7. Shutdown Sequence

```
RuntimeContext.drop()
  │
  ├── 1. PluginService.stop_all() — plugins clean up
  ├── 2. BrowserService.close_all() — browser processes terminate
  ├── 3. LifecycleManager.shutdown_all() — lifecycle events emitted
  ├── 4. Scheduler tasks cancelled
  └── 5. EventBus drops (all subscribers released)
```

RuntimeContext contains only `Arc` handles. Destructors fire on drop. The browser service's `Drop` implementation ensures child processes are killed even on panic.

---

## 8. Testing Integration

### Test Stubs

Each Phase 2 crate provides a `mock` feature or test module:

```rust
// browseros-bridge (test module)
pub mod mock {
    pub struct MockPage { ... }
    impl PagePort for MockPage { /* predictable responses */ }

    pub struct MockBrowser { ... }
    impl BrowserPort for MockBrowser { /* single session, configurable */ }
}
```

### Test Runtime

```rust
fn test_runtime() -> RuntimeContext {
    let mut config = RootConfig::new();
    // Phase 2 uses mock implementations
    RuntimeContext::builder()
        .with_mock_browser()
        .with_test_config()
        .build()
        .unwrap()
}
```

The builder pattern extends:

```rust
impl RuntimeBuilder {
    pub fn with_mock_browser(mut self) -> Self;
    pub fn with_mock_plugins(mut self) -> Self;
    pub fn with_test_artifact_store(mut self, tmp_dir: PathBuf) -> Self;
    pub fn with_browser_launcher(mut self, launcher: Box<dyn BrowserPort>) -> Self;
}
```

Tests pass a mock `BrowserPort` directly, skipping CDP entirely. This makes unit tests fast and deterministic.
