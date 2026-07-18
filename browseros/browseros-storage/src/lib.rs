pub mod error;
pub mod events;
pub mod manager;

pub use error::{StorageError, StorageResult};
pub use events::{CookieAdded, CookieRemoved, StorageCleared};
pub use manager::StorageManager;

pub use browseros_bridge::identifiers::PageId;
pub use browseros_bridge::traits::StoragePort;
pub use browseros_bridge::types::{Cookie, SameSitePolicy, StorageEntry};
