//! # browseros-browser
//!
//! Browser lifecycle management, session management, tab/page lifecycle.
//!
//! This crate is the **first concrete implementation layer** in the
//! BrowserOS stack.  It implements the `browseros-bridge` trait interfaces
//! and orchestrates the browser lifecycle.
//!
//! ## Architecture
//!
//! - [`BrowserManager`] — entry point for all browser operations.
//! - [`BrowserHandle`], [`SessionHandle`], [`PageHandle`] — managed
//!   handles that wrap bridge trait objects.
//! - [`BrowserState`] — explicit lifecycle state machine.
//! - [`BrowserConfig`] — configuration for the browser subsystem.
//! - [`BackendFactory`] / [`BackendRegistry`] — pluggable browser backends.
//! - [`TransportManager`] — internal transport abstraction.
//! - [`BrowserProcess`] — OS process management.
//!
//! ## Event Emission
//!
//! This crate emits domain events through the `EventBus`:
//!
//! - `browser.started` / `browser.closed`
//! - `browser.crashed` / `browser.disconnected`
//! - `session.created` / `session.closed`
//! - `page.created` / `page.closed`
//!
//! ## Backend Model
//!
//! The browser backend is completely replaceable.  Chromium is only one
//! backend.  No Chromium-specific types are exposed outside this crate.

pub mod backend;
pub mod builder;
pub mod cdp_backend;
pub mod config;
pub mod events;
pub mod handle;
pub mod lifecycle;
pub mod manager;
pub mod process;
pub mod transport;

pub use backend::{BackendFactory, BackendRegistry};
pub use builder::BrowserManagerBuilder;
pub use config::BrowserConfig;
pub use events::{
    BrowserClosed, BrowserCrashed, BrowserDisconnected, BrowserStarted, PageClosed, PageCreated,
    SessionClosed, SessionCreated,
};
pub use handle::{BrowserHandle, FrameHandle, PageHandle, SessionHandle};
pub use lifecycle::BrowserState;
pub use manager::BrowserManager;
pub use process::BrowserProcess;
pub use transport::TransportManager;
