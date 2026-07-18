//! BrowserOS Bridge — Stable Abstraction Layer for Browser Engines
//!
//! This crate defines the contract between BrowserOS and any browser
//! automation protocol (CDP, WebDriver BiDi, Playwright protocol, etc.).
//!
//! # Design
//!
//! - **Traits only** — no implementation logic, no protocol dependencies.
//! - **Pure leaf crate** — depends only on `browseros-types`.
//! - **Object-safe traits** — all traits can be used as `dyn Trait`.
//! - **Strongly-typed errors** — `BridgeError` covers every failure mode.
//! - **No async** — all methods are synchronous.
//! - **No runtime** — no EventBus, no Config, no Observability.
//!
//! # Port Hierarchy
//!
//! ```text
//! BrowserPort
//!   └── SessionPort (1:N)
//!         └── PagePort (1:N)
//!               ├── FramePort (1:N, tree)
//!               ├── ElementPort (N, tree via query)
//!               ├── LocatorPort (1 per page)
//!               ├── NetworkPort (1 per page)
//!               ├── InputPort (1 per page)
//!               ├── StoragePort (1 per page)
//!               ├── DialogPort (1 per page)
//!               └── DownloadPort (1 per session)
//! ```

pub mod error;
pub mod identifiers;
pub mod locator;
pub mod traits;
pub mod types;

pub use error::{BridgeError, BridgeResult, BrowserClosedInfo, BrowserCrashedInfo};
pub use identifiers::*;
pub use locator::LocatorStrategy;
pub use traits::*;
pub use types::*;
