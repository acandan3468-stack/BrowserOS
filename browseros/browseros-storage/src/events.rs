use browseros_bridge::identifiers::PageId;
use browseros_bridge::types::Cookie;
use browseros_types::event::EventMetadata;

/// Emitted when a cookie is added to the page.
#[derive(Debug, Clone)]
pub struct CookieAdded {
    pub metadata: EventMetadata,
    pub page_id: PageId,
    pub cookie: Cookie,
}

/// Emitted when a cookie is removed from the page.
#[derive(Debug, Clone)]
pub struct CookieRemoved {
    pub metadata: EventMetadata,
    pub page_id: PageId,
    pub cookie_name: String,
    pub cookie_url: String,
}

/// Emitted when web storage is cleared.
#[derive(Debug, Clone)]
pub struct StorageCleared {
    pub metadata: EventMetadata,
    pub page_id: PageId,
    /// "local", "session", or "all"
    pub storage_type: String,
}
