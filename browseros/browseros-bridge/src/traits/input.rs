use crate::error::BridgeResult;
use crate::traits::ElementPort;
use crate::types::ClickOptions;
use std::path::PathBuf;
use std::time::Duration;

/// Interface for input simulation on a page.
///
/// `InputPort` provides mouse, keyboard, touch, and file upload
/// operations. All methods operate on element handles and dispatch
/// the corresponding browser-level input events.
pub trait InputPort: Send + Sync {
    /// Click an element with the given options.
    fn click(&self, element: &dyn ElementPort, options: ClickOptions) -> BridgeResult<()>;

    /// Double-click an element.
    fn dblclick(&self, element: &dyn ElementPort, options: ClickOptions) -> BridgeResult<()>;

    /// Clear existing input and type text into a form field.
    fn fill(&self, element: &dyn ElementPort, text: &str) -> BridgeResult<()>;

    /// Type text into an element with an optional delay between keystrokes.
    fn type_text(&self, element: &dyn ElementPort, text: &str, delay: Duration)
        -> BridgeResult<()>;

    /// Press and release a keyboard key.
    fn press_key(&self, key: &str) -> BridgeResult<()>;

    /// Hover the mouse over an element.
    fn hover(&self, element: &dyn ElementPort) -> BridgeResult<()>;

    /// Scroll the page by the given deltas.
    fn scroll(&self, delta_x: f64, delta_y: f64) -> BridgeResult<()>;

    /// Drag from source element and drop on target element.
    fn drag_and_drop(&self, source: &dyn ElementPort, target: &dyn ElementPort)
        -> BridgeResult<()>;

    /// Upload file(s) by selecting them in a file chooser.
    fn upload_file(&self, element: &dyn ElementPort, paths: &[PathBuf]) -> BridgeResult<()>;

    /// Select option(s) in a `<select>` element by their values.
    fn select_option(&self, element: &dyn ElementPort, values: &[&str]) -> BridgeResult<()>;

    /// Check a checkbox or radio element.
    fn check(&self, element: &dyn ElementPort) -> BridgeResult<()>;

    /// Uncheck a checkbox element.
    fn uncheck(&self, element: &dyn ElementPort) -> BridgeResult<()>;
}
