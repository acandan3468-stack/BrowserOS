use crate::error::BridgeResult;
use crate::types::DialogInfo;

/// Interface for handling JavaScript dialogs on a page.
///
/// `DialogPort` provides access to pending JavaScript dialogs
/// (alert, confirm, prompt, beforeunload). Callers check for a
/// pending dialog via `next()` and then accept or dismiss it.
pub trait DialogPort: Send + Sync {
    /// Returns the next pending dialog, if any.
    fn next(&self) -> BridgeResult<Option<DialogInfo>>;

    /// Accept the current dialog. If the dialog is a prompt,
    /// the optional `prompt_text` sets the response value.
    fn accept(&self, prompt_text: Option<&str>) -> BridgeResult<()>;

    /// Dismiss the current dialog without accepting.
    fn dismiss(&self) -> BridgeResult<()>;
}
