//! # browseros-config
//!
//! Layered configuration system for the BrowserOS runtime.
//!
//! ## Architecture
//!
//! This crate provides the **mechanism** for loading, merging, and accessing
//! configuration.  It does NOT define any component's config structure —
//! each component crate defines its own config types and implements [`Config`].
//!
//! ## Design principles
//!
//! - **Three-layer static config:** defaults (baked into each component's
//!   `Default` impl) → YAML file → environment variables (`BROWSEROS_*`).
//! - **No hot reload in Phase 1.**  Config is loaded once at startup.
//! - **Fail fast, report all errors.**  Every config problem is reported before
//!   any component starts.
//! - **Component isolation.**  Components access only their own namespace.
//!   Plugins cannot read other plugins' config.
//!
//! ## Quick start
//!
//! ```rust
//! use browseros_config::{ConfigLoader, ConfigSource, RootConfig, Config};
//! use serde::{Deserialize, Serialize};
//!
//! // 1. Define your component's config struct
//! #[derive(Debug, Deserialize, Serialize, Default)]
//! struct MyConfig {
//!     timeout_ms: u64,
//!     host: String,
//! }
//!
//! impl Config for MyConfig {
//!     fn namespace() -> &'static str { "my.component" }
//! }
//!
//! // 2. Load raw config from sources
//! let raw = ConfigLoader::new()
//!     .add_source(ConfigSource::Environment)
//!     .load()
//!     .expect("config load should succeed");
//!
//! // 3. Register and validate your component's config
//! let mut root = RootConfig::new();
//! root.register::<MyConfig>(&raw)
//!     .expect("my.component config should be valid");
//!
//! // 4. Access your config
//! let my_config: &MyConfig = root
//!     .for_component::<MyConfig>()
//!     .expect("my.component must be registered");
//! ```
//!
//! ## Crate boundaries
//!
//! `browseros-config` depends ONLY on `browseros-types`.  It must never import
//! any other BrowserOS crate (prevents circular dependencies).

pub mod config;
pub mod layer;
pub mod source;
pub mod validator;

pub use config::{Config, RootConfig};
pub use layer::ConfigLoader;
pub use source::ConfigSource;
pub use validator::ConfigError;
