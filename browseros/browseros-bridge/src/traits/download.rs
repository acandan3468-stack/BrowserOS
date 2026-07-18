use crate::error::BridgeResult;
use crate::types::DownloadInfo;
use std::path::PathBuf;
use std::time::Duration;

/// Interface for managing file downloads on a page.
///
/// `DownloadPort` provides access to active and completed downloads,
/// cancellation, and download path configuration.
pub trait DownloadPort: Send + Sync {
    /// Returns all downloads (active and completed).
    fn downloads(&self) -> Vec<DownloadInfo>;

    /// Cancel a download by its string identifier.
    fn cancel_download(&self, id: &str) -> BridgeResult<()>;

    /// Set the directory where downloaded files are saved.
    fn set_download_path(&self, path: PathBuf) -> BridgeResult<()>;

    /// Returns the current download directory.
    fn download_path(&self) -> PathBuf;

    /// Blocks until all in-progress downloads complete or the timeout expires.
    fn wait_for_completion(&self, timeout: Duration) -> BridgeResult<Vec<DownloadInfo>>;
}
