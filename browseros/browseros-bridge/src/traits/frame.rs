use crate::error::BridgeResult;
use crate::identifiers::FrameId;
use crate::traits::PagePort;
use crate::types::JsResult;

/// Interface for a frame (nested browsing context).
///
/// Frames form a tree: the main frame at the root, with child frames
/// for iframes, embeds, and other nested browsing contexts. Each frame
/// has its own document, window, and JavaScript context.
pub trait FramePort: Send + Sync {
    /// Unique identifier for this frame.
    fn id(&self) -> FrameId;

    /// Current URL of the frame's document.
    fn url(&self) -> String;

    /// Current document title within this frame.
    fn title(&self) -> String;

    /// Returns the parent frame ID, or `None` for the main frame.
    fn parent_id(&self) -> Option<FrameId>;

    /// Get the frame's HTML content.
    fn content(&self) -> BridgeResult<String>;

    /// Set the frame's HTML content.
    fn set_content(&self, html: &str) -> BridgeResult<()>;

    /// Evaluate JavaScript in this frame's context.
    fn evaluate(&self, script: &str, arg: Option<&serde_json::Value>) -> BridgeResult<JsResult>;

    /// Returns all child frames.
    fn child_frames(&self) -> Vec<Box<dyn FramePort>>;

    /// Returns the owning page.
    fn page(&self) -> Box<dyn PagePort>;
}
