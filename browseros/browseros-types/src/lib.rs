//! # browseros-types
//!
//! Canonical types, event hierarchy, message protocol, error system, and shared
//! contracts for the BrowserOS runtime.
//!
//! This crate is the **zero-dependency leaf** of the BrowserOS workspace. Every
//! other crate imports these types. No other BrowserOS crate is imported here.
//!
//! ## Invariants upheld by this crate
//!
//! - No runtime behaviour (no async, no I/O)
//! - No business logic
//! - No mutable global state
//! - Thread-safe by default (`Send + Sync`)
//! - Every public item is documented

pub mod clock;
pub mod component;
pub mod error;
pub mod event;
pub mod identifiers;
pub mod message;
pub mod module;
pub mod value;
