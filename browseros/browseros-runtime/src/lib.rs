//! # browseros-runtime
//!
//! RuntimeContext — dependency injection replacement for BrowserOS.
//!
//! The `RuntimeContext` holds all shared infrastructure components and
//! exposes them as immutable `Arc` references.  It replaces a traditional
//! DI container while respecting Rust's ownership model.
//!
//! ## Contents
//!
//! - `Arc<Logger>` — structured logger (config-driven log level)
//! - `Arc<MetricsRegistry>` — counters, gauges, histograms
//! - `Arc<Tracer>` — span-based execution tracing
//! - `Arc<EventBus>` — in-process event communication
//! - `Arc<LifecycleManager>` — component lifecycle state machine
//! - `Arc<Scheduler>` — delayed + event-triggered execution
//! - `Arc<RootConfig>` — merged component configuration
//! - `Arc<DagEngine>` — DAG execution engine (task orchestration)
//!
//! ## Immutability
//!
//! RuntimeContext is **immutable after construction**.  All fields are
//! behind `Arc` so individual components can be cloned cheaply.
//! There is no setter — everything is established at initialization.
//!
//! ## Initialization
//!
//! ```rust,ignore
//! let ctx = RuntimeContext::init(config_path)?;
//! let logger = ctx.logger();
//! logger.info("application started");
//! ```
//!
//! ## Design rule
//!
//! RuntimeContext provides **access** to infrastructure, not **behaviour**.
//! It must never become a god object — components receive only what they
//! need via their constructor, not the full context.

use std::path::Path;
use std::sync::Arc;

use browseros_config::{ConfigLoader, ConfigSource, RootConfig};
use browseros_dag::DagEngine;
use browseros_event_bus::EventBus;
use browseros_lifecycle::{LifecycleManager, LifecycleState};
use browseros_observability::{
    LevelFilter, LogConfig, Logger, MetricsRegistry, OutputSink, StdoutSink, Tracer,
};
use browseros_scheduler::Scheduler;
use browseros_types::identifiers::ModuleId;
use browseros_types::value::SemVer;

/// Error returned during RuntimeContext initialization.
#[derive(Debug)]
pub enum RuntimeInitError {
    /// Loading or merging configuration failed.
    Config(String),
    /// A required config block was missing or invalid.
    ConfigMissing(String),
    /// Initialization logic failed.
    Init(String),
}

impl std::fmt::Display for RuntimeInitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuntimeInitError::Config(msg) => write!(f, "config error: {}", msg),
            RuntimeInitError::ConfigMissing(msg) => write!(f, "missing config: {}", msg),
            RuntimeInitError::Init(msg) => write!(f, "init error: {}", msg),
        }
    }
}

impl std::error::Error for RuntimeInitError {}

/// Shared runtime infrastructure for all BrowserOS components.
///
/// Created once at startup via [`RuntimeContext::init`] or
/// [`RuntimeContext::builder`].  After creation the context is immutable.
#[derive(Clone)]
pub struct RuntimeContext {
    bus: Arc<EventBus>,
    logger: Arc<Logger>,
    metrics: Arc<MetricsRegistry>,
    tracer: Arc<Tracer>,
    config: Arc<RootConfig>,
    lifecycle: Arc<LifecycleManager>,
    scheduler: Arc<Scheduler>,
    dag: Arc<DagEngine>,
}

impl RuntimeContext {
    /// Initialize the full runtime from a YAML config file.
    ///
    /// Loads config from the given file (plus environment variables),
    /// registers all built-in component configs, creates infrastructure,
    /// and returns a ready-to-use `RuntimeContext`.
    pub fn init(config_path: &Path) -> Result<Self, RuntimeInitError> {
        RuntimeBuilder::default()
            .config_file(config_path.to_path_buf())
            .build()
    }

    /// Create a builder for programmatic (non-file-based) initialization.
    ///
    /// Useful in tests or when config comes from a non-file source.
    pub fn builder() -> RuntimeBuilder {
        RuntimeBuilder::default()
    }

    #[allow(clippy::type_complexity)]
    fn init_observability(
        root: &RootConfig,
        sink: Arc<dyn OutputSink>,
    ) -> Result<(Arc<Logger>, Arc<MetricsRegistry>, Arc<Tracer>), RuntimeInitError> {
        let log_config = root
            .for_component::<LogConfig>()
            .cloned()
            .unwrap_or_default();

        let filter = Arc::new(LevelFilter::new(log_config.level));

        let logger = Arc::new(Logger::new(sink, filter, "browseros"));
        let metrics = Arc::new(MetricsRegistry::new());
        let tracer = Arc::new(Tracer::new());

        Ok((logger, metrics, tracer))
    }

    // ─── Accessors ──────────────────────────────────────────────────────

    /// The in-process event bus for publish/subscribe communication.
    pub fn bus(&self) -> &Arc<EventBus> {
        &self.bus
    }

    /// The system logger.
    pub fn logger(&self) -> &Arc<Logger> {
        &self.logger
    }

    /// The metrics registry holding counters, gauges, and histograms.
    pub fn metrics(&self) -> &Arc<MetricsRegistry> {
        &self.metrics
    }

    /// The span-based execution tracer.
    pub fn tracer(&self) -> &Arc<Tracer> {
        &self.tracer
    }

    /// The merged, validated configuration tree for all components.
    pub fn config(&self) -> &Arc<RootConfig> {
        &self.config
    }

    /// The lifecycle manager for component state transitions.
    pub fn lifecycle(&self) -> &Arc<LifecycleManager> {
        &self.lifecycle
    }

    /// The scheduler for delayed and event-triggered execution.
    pub fn scheduler(&self) -> &Arc<Scheduler> {
        &self.scheduler
    }

    /// The DAG execution engine for multi-step task orchestration.
    pub fn dag(&self) -> &Arc<DagEngine> {
        &self.dag
    }
}

impl std::fmt::Debug for RuntimeContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuntimeContext").finish()
    }
}

/// Builder for constructing `RuntimeContext` programmatically.
///
/// Allows injecting custom implementations for tests or advanced scenarios.
/// By default, creates a minimal configuration with sensible defaults.
#[derive(Default)]
pub struct RuntimeBuilder {
    config_path: Option<std::path::PathBuf>,
    dag_engine: Option<Arc<DagEngine>>,
    log_sink: Option<Arc<dyn OutputSink>>,
}

impl RuntimeBuilder {
    /// Set the path to the YAML config file.
    pub fn config_file(mut self, path: std::path::PathBuf) -> Self {
        self.config_path = Some(path);
        self
    }

    /// Inject a custom `DagEngine` instance.
    ///
    /// When not called, the builder creates a default `DagEngine` wired
    /// to the runtime's EventBus, Logger, MetricsRegistry, and Tracer.
    pub fn with_dag_engine(mut self, dag: Arc<DagEngine>) -> Self {
        self.dag_engine = Some(dag);
        self
    }

    /// Set a custom log output sink.
    ///
    /// Defaults to `StdoutSink` when not called.
    pub fn with_log_sink(mut self, sink: Arc<dyn OutputSink>) -> Self {
        self.log_sink = Some(sink);
        self
    }

    /// Route all log output to stderr instead of the default stdout.
    ///
    /// Shorthand for `.with_log_sink(Arc::new(StderrSink::new()))`.
    /// Useful for protocol servers (e.g. MCP) where stdout carries
    /// structured messages rather than log text.
    pub fn with_stderr_logging(self) -> Self {
        self.with_log_sink(Arc::new(browseros_observability::StderrSink::new()))
    }

    /// Build the `RuntimeContext`.
    pub fn build(self) -> Result<RuntimeContext, RuntimeInitError> {
        let root = if let Some(ref path) = self.config_path {
            Self::load_config(path)?
        } else {
            RootConfig::new()
        };

        let sink: Arc<dyn OutputSink> =
            self.log_sink.unwrap_or_else(|| Arc::new(StdoutSink::new()));
        let (logger, metrics, tracer) = RuntimeContext::init_observability(&root, sink)?;
        let bus = Arc::new(EventBus::new());
        let scheduler = Arc::new(Scheduler::new(bus.clone()));
        let module_id = ModuleId::new("lifecycle", SemVer::new(0, 1, 0));
        let lifecycle = Arc::new(LifecycleManager::new(
            bus.clone(),
            logger.clone(),
            module_id,
        ));

        let dag = self.dag_engine.unwrap_or_else(|| {
            Arc::new(DagEngine::new(
                bus.as_ref().clone(),
                logger.as_ref().clone(),
                metrics.as_ref().clone(),
                tracer.as_ref().clone(),
            ))
        });

        lifecycle.register_component("dag_engine");
        lifecycle.transition_to("dag_engine", LifecycleState::Initializing);
        lifecycle.transition_to("dag_engine", LifecycleState::Running);

        Ok(RuntimeContext {
            bus,
            logger,
            metrics,
            tracer,
            config: Arc::new(root),
            lifecycle,
            scheduler,
            dag,
        })
    }

    fn load_config(config_path: &Path) -> Result<RootConfig, RuntimeInitError> {
        let raw = ConfigLoader::new()
            .add_source(ConfigSource::File(config_path.to_path_buf()))
            .add_source(ConfigSource::Environment)
            .load()
            .map_err(|e| RuntimeInitError::Config(e.to_string()))?;

        let mut root = RootConfig::new();
        root.register::<LogConfig>(&raw)
            .map_err(|e| RuntimeInitError::ConfigMissing(e.to_string()))?;

        Ok(root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::any::Any;
    use std::path::PathBuf;
    use std::sync::mpsc;
    use tempfile::TempDir;

    use browseros_dag::{DagExecutionState, DagNode};
    use browseros_types::identifiers::NodeId;

    fn write_test_config(dir: &TempDir) -> PathBuf {
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, "logging:\n  level: Debug\n  json: false\n").unwrap();
        path
    }

    #[test]
    fn runtime_context_init_from_file() {
        let dir = TempDir::new().unwrap();
        let path = write_test_config(&dir);
        let ctx = RuntimeContext::init(&path).unwrap();

        // All components should be accessible
        let _logger = ctx.logger();
        let _metrics = ctx.metrics();
        let _bus = ctx.bus();
        let _tracer = ctx.tracer();
        let _lifecycle = ctx.lifecycle();
        let _scheduler = ctx.scheduler();
        let _config = ctx.config();
    }

    #[test]
    fn runtime_context_builder_default() {
        let ctx = RuntimeContext::builder().build().unwrap();
        let _logger = ctx.logger();
    }

    #[test]
    fn runtime_context_bus_publish_subscribe() {
        let ctx = RuntimeContext::builder().build().unwrap();

        use browseros_types::event::{Event, EventCategory, EventMetadata};
        use browseros_types::identifiers::{CorrelationId, ModuleId};
        use browseros_types::value::{ContentType, SemVer};

        struct TestPayload {
            metadata: EventMetadata,
        }

        impl std::fmt::Debug for TestPayload {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct("TestPayload").finish()
            }
        }

        impl Event for TestPayload {
            fn kind(&self) -> &'static str {
                "runtime.test"
            }
            fn category(&self) -> EventCategory {
                EventCategory::Domain
            }
            fn metadata(&self) -> &EventMetadata {
                &self.metadata
            }
            fn as_any(&self) -> &dyn Any {
                self
            }
        }

        let received = Arc::new(std::sync::Mutex::new(false));
        let r = received.clone();

        ctx.bus().subscribe(
            "runtime.test",
            Arc::new(move |_ev: &dyn Event| {
                *r.lock().unwrap() = true;
            }),
        );

        let meta = EventMetadata::new(
            ModuleId::new("test", SemVer::new(1, 0, 0)),
            CorrelationId::new(),
            None,
            ContentType::new("app/json"),
            chrono::Utc::now(),
        );

        ctx.bus().publish(Box::new(TestPayload { metadata: meta }));
        assert!(*received.lock().unwrap());
    }

    #[test]
    fn runtime_context_logger_is_usable() {
        let ctx = RuntimeContext::builder().build().unwrap();
        ctx.logger().info("runtime context test message");
        // No panic means success
    }

    #[test]
    fn runtime_context_logger_respects_config_level() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, "logging:\n  level: Error\n  json: false\n").unwrap();
        let ctx = RuntimeContext::init(&path).unwrap();
        ctx.logger().error("this should pass the Error filter");
        ctx.logger().info("this should be filtered out");
        // No panic means success
    }

    #[test]
    fn runtime_context_is_cloneable() {
        let ctx = RuntimeContext::builder().build().unwrap();
        let ctx2 = ctx.clone();
        let _bus = ctx2.bus();
    }

    #[test]
    fn runtime_context_is_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<RuntimeContext>();
        assert_sync::<RuntimeContext>();
    }

    #[test]
    fn runtime_context_metrics_works() {
        let ctx = RuntimeContext::builder().build().unwrap();
        let c = ctx.metrics().counter("test.counter");
        c.increment();
        assert_eq!(c.value(), 1);
    }

    // ─── P10: DAG Runtime Integration ─────────────────────────────────────

    #[test]
    fn runtime_context_dag_default_construction() {
        let ctx = RuntimeContext::builder().build().unwrap();
        let dag = ctx.dag();
        assert!(dag.list_executions().is_empty());
    }

    #[test]
    fn runtime_context_dag_custom_injection() {
        let ctx1 = RuntimeContext::builder().build().unwrap();
        let dag_arc = ctx1.dag().clone();
        let ctx2 = RuntimeContext::builder()
            .with_dag_engine(dag_arc.clone())
            .build()
            .unwrap();
        assert!(Arc::ptr_eq(&dag_arc, ctx2.dag()));
    }

    #[test]
    fn runtime_context_dag_is_usable() {
        let ctx = RuntimeContext::builder().build().unwrap();
        let node = DagNode::command(NodeId::from_string("a"), "test-node", Box::new(|| Ok(())));
        ctx.dag().register_node(node).unwrap();
        let result = ctx.dag().execute(&[NodeId::from_string("a")]).unwrap();
        assert_eq!(result.state, DagExecutionState::Completed);
    }

    #[test]
    fn runtime_context_dag_lifecycle_registered() {
        let ctx = RuntimeContext::builder().build().unwrap();
        let state = ctx.lifecycle().current_state("dag_engine");
        assert_eq!(state, Some(LifecycleState::Running));
    }

    #[test]
    fn runtime_context_dag_thread_safe() {
        let ctx = RuntimeContext::builder().build().unwrap();
        let shared = Arc::new(ctx);
        let (tx, rx) = mpsc::channel();

        let handle = std::thread::spawn(move || {
            let dag = shared.dag();
            let node =
                DagNode::command(NodeId::from_string("b"), "thread-node", Box::new(|| Ok(())));
            dag.register_node(node).unwrap();
            let result = dag.execute(&[NodeId::from_string("b")]).unwrap();
            tx.send(result.state).unwrap();
        });

        let state = rx.recv().unwrap();
        assert_eq!(state, DagExecutionState::Completed);
        handle.join().unwrap();
    }

    #[test]
    fn runtime_context_dag_multiple_contexts() {
        let ctx1 = RuntimeContext::builder().build().unwrap();
        let ctx2 = RuntimeContext::builder().build().unwrap();

        // Each context has its own DagEngine
        assert!(!Arc::ptr_eq(ctx1.dag(), ctx2.dag()));

        ctx1.dag()
            .register_node(DagNode::command(
                NodeId::from_string("x"),
                "ctx1-node",
                Box::new(|| Ok(())),
            ))
            .unwrap();
        let result1 = ctx1.dag().execute(&[NodeId::from_string("x")]).unwrap();
        assert_eq!(result1.state, DagExecutionState::Completed);
    }

    #[test]
    fn runtime_context_dag_is_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<DagEngine>();
        assert_sync::<DagEngine>();
    }
}
