use crate::identifiers::{BrowserId, ElementId, FrameId, PageId, SessionId};
use crate::locator::LocatorStrategy;
use std::time::Duration;
use thiserror::Error;

/// Strongly-typed error for all browser bridge operations.
///
/// Every variant carries structured information about the failure.
/// No stringly-typed errors, no `anyhow`, no boxed errors.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BridgeError {
    // ——— Browser ———
    #[error("browser process closed: {0}")]
    BrowserClosed(BrowserClosedInfo),

    #[error("browser process crashed: {0}")]
    BrowserCrashed(BrowserCrashedInfo),

    #[error("browser disconnected: {reason}")]
    BrowserDisconnected {
        browser_id: Option<BrowserId>,
        reason: String,
    },

    // ——— Session ———
    #[error("session closed: {0}")]
    SessionClosed(SessionId),

    #[error("session not found: {0}")]
    SessionNotFound(SessionId),

    // ——— Page ———
    #[error("page closed: {0}")]
    PageClosed(PageId),

    #[error("page not found: {0}")]
    PageNotFound(PageId),

    #[error("navigation timed out after {timeout:?} for {url}")]
    NavigationTimeout {
        page_id: PageId,
        url: String,
        timeout: Duration,
    },

    #[error("navigation failed: {url} — {reason}")]
    NavigationFailed {
        page_id: PageId,
        url: String,
        reason: String,
    },

    // ——— Frame ———
    #[error("frame detached: {0}")]
    FrameDetached(FrameId),

    #[error("frame not found: {0}")]
    FrameNotFound(FrameId),

    // ——— Element ———
    #[error("element stale: {0} — DOM node no longer attached")]
    ElementStale(ElementId),

    #[error("element not found")]
    ElementNotFound,

    #[error("element is not visible")]
    ElementNotVisible,

    #[error("element is not interactable: {0}")]
    ElementNotInteractable(String),

    // ——— Locator ———
    #[error("locator timed out after {timeout:?} on strategy {strategy}")]
    LocatorTimeout {
        strategy: Box<LocatorStrategy>,
        timeout: Duration,
    },

    #[error("locator matched {count} elements when exactly 1 was expected")]
    LocatorAmbiguous {
        strategy: Box<LocatorStrategy>,
        count: usize,
    },

    #[error("locator matched no elements")]
    LocatorNoMatch,

    // ——— JavaScript ———
    #[error("JavaScript execution error on page {page_id}: {message}")]
    JavascriptError {
        page_id: PageId,
        message: String,
        stack: Option<String>,
    },

    #[error("JavaScript type error: expected {expected}, got {actual}")]
    JavascriptTypeError {
        expected: &'static str,
        actual: String,
    },

    // ——— Network ———
    #[error("request interception rule not found")]
    InterceptionRuleNotFound,

    #[error("network unreachable")]
    NetworkUnreachable,

    // ——— Download ———
    #[error("download failed: {download_id} — {reason}")]
    DownloadFailed { download_id: String, reason: String },

    #[error("download not found: {download_id}")]
    DownloadNotFound { download_id: String },

    // ——— Dialog ———
    #[error("no dialog open")]
    NoDialogOpen,

    // ——— Storage ———
    #[error("storage access denied: {0}")]
    StorageAccessDenied(String),

    // ——— Permission ———
    #[error("permission denied: {0}")]
    PermissionDenied(String),

    // ——— Connection ———
    #[error("invalid endpoint: {0}")]
    InvalidEndpoint(String),

    #[error("connection refused: {0}")]
    ConnectionRefused(String),

    #[error("connection timed out")]
    ConnectionTimedOut,

    // —── Invalid State ———
    #[error("invalid handle: {handle_type}")]
    InvalidHandle { handle_type: &'static str },

    #[error("operation not supported: {0}")]
    NotImplemented(&'static str),

    #[error("operation timed out")]
    Timeout,

    // ——— Internal ———
    #[error("internal error: {0}")]
    Internal(String),
}

impl BridgeError {
    /// Returns `true` if the error is recoverable (retry may help).
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            Self::Timeout
                | Self::ConnectionTimedOut
                | Self::ConnectionRefused(_)
                | Self::NetworkUnreachable
        )
    }

    /// Returns `true` if the target resource is gone permanently.
    pub fn is_permanent(&self) -> bool {
        matches!(
            self,
            Self::BrowserClosed(_)
                | Self::BrowserCrashed(_)
                | Self::SessionClosed(_)
                | Self::PageClosed(_)
                | Self::FrameDetached(_)
                | Self::ElementStale(_)
        )
    }
}

// ——— Structured info types ———

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{reason} (exit code: {exit_code:?})")]
pub struct BrowserClosedInfo {
    pub browser_id: BrowserId,
    pub exit_code: Option<i32>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("crash: {crash_reason}")]
pub struct BrowserCrashedInfo {
    pub browser_id: BrowserId,
    pub crash_reason: String,
    pub dump_path: Option<String>,
}

/// Convenience alias for bridge operations.
pub type BridgeResult<T> = Result<T, BridgeError>;
