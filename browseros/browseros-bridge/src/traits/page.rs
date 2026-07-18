use crate::error::BridgeResult;
use crate::identifiers::PageId;
use crate::traits::{
    DialogPort, DownloadPort, FramePort, InputPort, LocatorPort, NetworkPort, StoragePort,
};
use crate::types::{
    JsResult, NavigationState, PdfOptions, ScreenshotOptions, Viewport, WaitCondition,
};

/// Interface for a single page (tab).
///
/// `PagePort` is the primary interface for agent operations. It provides
/// navigation, JavaScript execution, content capture, and access to
/// sub-ports for DOM, network, input, storage, dialog, and download
/// operations.
pub trait PagePort: Send + Sync {
    /// Unique identifier for this page.
    fn id(&self) -> PageId;

    /// Current URL of the page.
    fn url(&self) -> String;

    /// Current document title.
    fn title(&self) -> String;

    /// Navigate to a URL. Returns the final navigation state.
    fn navigate(&self, url: &str) -> BridgeResult<NavigationState>;

    /// Reload the current page.
    fn reload(&self) -> BridgeResult<NavigationState>;

    /// Navigate back in history.
    fn go_back(&self) -> BridgeResult<NavigationState>;

    /// Navigate forward in history.
    fn go_forward(&self) -> BridgeResult<NavigationState>;

    /// Evaluate JavaScript in the page context.
    fn evaluate(&self, script: &str, arg: Option<&serde_json::Value>) -> BridgeResult<JsResult>;

    /// Evaluate JavaScript and return the result as an element handle.
    fn evaluate_handle(
        &self,
        script: &str,
        arg: Option<&serde_json::Value>,
    ) -> BridgeResult<Box<dyn ElementPort>>;

    /// Take a screenshot of the page.
    fn screenshot(&self, options: ScreenshotOptions) -> BridgeResult<Vec<u8>>;

    /// Generate a PDF of the page.
    fn pdf(&self, options: PdfOptions) -> BridgeResult<Vec<u8>>;

    /// Get the full page HTML content.
    fn content(&self) -> BridgeResult<String>;

    /// Set the page HTML content.
    fn set_content(&self, html: &str) -> BridgeResult<()>;

    /// Set the page viewport size.
    fn set_viewport(&self, viewport: Viewport) -> BridgeResult<()>;

    /// Wait for a condition to be met.
    fn wait_for(&self, condition: WaitCondition) -> BridgeResult<()>;

    /// Get all frames in this page.
    fn frames(&self) -> Vec<Box<dyn FramePort>>;

    /// Get the main frame of this page.
    fn main_frame(&self) -> Box<dyn FramePort>;

    /// Close this page.
    fn close(&self) -> BridgeResult<()>;

    // ——— Sub-port accessors ———

    /// Returns the locator port for this page.
    fn locator(&self) -> Box<dyn LocatorPort>;

    /// Returns the network port for this page.
    fn network(&self) -> Box<dyn NetworkPort>;

    /// Returns the input port for this page.
    fn input(&self) -> Box<dyn InputPort>;

    /// Returns the storage port for this page.
    fn storage(&self) -> Box<dyn StoragePort>;

    /// Returns the dialog port for this page.
    fn dialog(&self) -> Box<dyn DialogPort>;

    /// Returns the download port for this page.
    fn download(&self) -> Box<dyn DownloadPort>;
}

use crate::traits::element::ElementPort;
