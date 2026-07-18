use std::sync::Arc;

use browseros_event_bus::EventBus;
use browseros_observability::{LevelFilter, LogLevel, LogRecord, Logger, OutputSink};

use crate::backend::BackendFactory;
use crate::config::BrowserConfig;
use crate::manager::BrowserManager;

/// Builder for [`BrowserManager`] with test-friendly defaults.
pub struct BrowserManagerBuilder {
    bus: Option<Arc<EventBus>>,
    logger: Option<Arc<Logger>>,
    config: BrowserConfig,
    backends: Vec<Box<dyn BackendFactory>>,
}

impl BrowserManagerBuilder {
    /// Create a new builder with default configuration.
    pub fn new() -> Self {
        Self {
            bus: None,
            logger: None,
            config: BrowserConfig::default(),
            backends: Vec::new(),
        }
    }

    /// Set the event bus.
    pub fn with_bus(mut self, bus: Arc<EventBus>) -> Self {
        self.bus = Some(bus);
        self
    }

    /// Set the logger.
    pub fn with_logger(mut self, logger: Arc<Logger>) -> Self {
        self.logger = Some(logger);
        self
    }

    /// Set the browser configuration.
    pub fn with_config(mut self, config: BrowserConfig) -> Self {
        self.config = config;
        self
    }

    /// Register a backend factory.
    pub fn with_backend(mut self, factory: Box<dyn BackendFactory>) -> Self {
        self.backends.push(factory);
        self
    }

    /// Build the [`BrowserManager`].
    pub fn build(self) -> BrowserManager {
        let bus = self.bus.unwrap_or_else(|| Arc::new(EventBus::new()));
        let logger = self.logger.unwrap_or_else(|| {
            Arc::new(Logger::new(
                Arc::new(NoopSink),
                Arc::new(LevelFilter::new(LogLevel::Warn)),
                "browseros-browser",
            ))
        });

        let manager = BrowserManager::new(bus, logger, &self.config);
        for factory in self.backends {
            manager.register_backend(factory);
        }
        manager
    }
}

impl Default for BrowserManagerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// A sink that discards all log records (for tests and defaults).
struct NoopSink;

impl OutputSink for NoopSink {
    fn write(&self, _record: &LogRecord) {}
    fn flush(&self) {}
}
