//! Chrome DevTools Protocol (CDP) implementation for BrowserOS.
//!
//! This is the **only** crate that depends on CDP-specific concepts.
//! It provides:
//!
//! - [`CdpConnection`] — WebSocket transport and command/response routing
//! - [`CdpSession`] — CDP session management (target attachment)
//! - Bridge trait implementations for `BrowserPort`, `SessionPort`,
//!   `PagePort`, `FramePort`, `ElementPort`, `LocatorPort`,
//!   `NetworkPort`, `InputPort`, `StoragePort`, `DialogPort`,
//!   `DownloadPort`, and `LocatorEngine`
//!
//! # Protocol Isolation
//!
//! No CDP type leaks into any public API outside this crate. All browser
//! interaction is through [`browseros_bridge`] trait objects.

pub mod backend;
pub mod command;
pub mod config;
pub mod connection;
pub mod error;
pub mod event;
pub mod factory;
pub mod protocol;
pub mod serializer;
pub mod session;
pub mod traits;
pub mod transport;
pub mod transport_ws;

pub use backend::CdpBrowserProcess;
pub use command::*;
pub use config::CdpConfig;
pub use connection::CdpConnection;
pub use error::{CdpError, CdpResult};
pub use event::EventDispatcher;
pub use factory::CdpBackendFactory;
pub use protocol::{BrowserVersion, ProtocolVersion, TargetInfo};
pub use serializer::{CdpEvent, CdpMessage, CdpRequest, CdpResponse, RequestIdGenerator};
pub use session::{CdpSession, TargetManager};
pub use traits::*;
pub use transport::CdpTransport;
