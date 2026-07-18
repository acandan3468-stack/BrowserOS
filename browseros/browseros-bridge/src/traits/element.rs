use crate::error::BridgeResult;
use crate::identifiers::ElementId;
use crate::traits::FramePort;
use crate::types::{BoxModel, NodeInfo, Point};
use std::collections::HashMap;

/// Interface for a live DOM element handle.
///
/// An `ElementPort` represents a reference to a DOM element. The element
/// may become stale if the DOM node is removed. After staleness, every
/// method returns `Err(BridgeError::ElementStale(...))`.
pub trait ElementPort: Send + Sync {
    /// Unique identifier for this element.
    fn id(&self) -> ElementId;

    /// HTML tag name (e.g., `"div"`, `"button"`).
    fn tag_name(&self) -> String;

    /// Visible text content of the element.
    fn text_content(&self) -> BridgeResult<String>;

    /// Inner HTML of the element.
    fn inner_html(&self) -> BridgeResult<String>;

    /// Outer HTML of the element (including the element itself).
    fn outer_html(&self) -> BridgeResult<String>;

    /// Get an attribute value by name.
    fn get_attribute(&self, name: &str) -> BridgeResult<Option<String>>;

    /// Set an attribute value by name.
    fn set_attribute(&self, name: &str, value: &str) -> BridgeResult<()>;

    /// Returns `true` if the element has the given attribute.
    fn has_attribute(&self, name: &str) -> BridgeResult<bool>;

    /// Returns all attributes as a map.
    fn attributes(&self) -> BridgeResult<HashMap<String, String>>;

    /// Bounding box of the element in viewport coordinates.
    fn bounding_box(&self) -> BridgeResult<Option<BoxModel>>;

    /// Returns `true` if the element is visible (has size and is not hidden).
    fn is_visible(&self) -> BridgeResult<bool>;

    /// Returns `true` if the element is enabled (not disabled).
    fn is_enabled(&self) -> BridgeResult<bool>;

    /// Returns `true` if the element is checked (checkbox/radio).
    fn is_checked(&self) -> BridgeResult<bool>;

    /// Returns `true` if the element is selected (option).
    fn is_selected(&self) -> BridgeResult<bool>;

    /// Returns `true` if the element is stable (not moving, has final layout).
    fn is_stable(&self) -> BridgeResult<bool>;

    /// Scroll the element into view.
    fn scroll_into_view(&self) -> BridgeResult<()>;

    /// Returns the center point of the element for clicking.
    fn click_point(&self) -> BridgeResult<Point>;

    /// Query for child elements by CSS selector.
    fn query_selector(&self, selector: &str) -> BridgeResult<Option<Box<dyn ElementPort>>>;

    /// Query for all child elements matching a CSS selector.
    fn query_selector_all(&self, selector: &str) -> BridgeResult<Vec<Box<dyn ElementPort>>>;

    /// Focus the element.
    fn focus(&self) -> BridgeResult<()>;

    /// Hover the mouse over the element.
    fn hover(&self) -> BridgeResult<()>;

    /// Returns a full snapshot of the element's DOM node.
    fn snapshot(&self) -> BridgeResult<NodeInfo>;

    /// Returns the frame that owns this element.
    fn owning_frame(&self) -> Box<dyn FramePort>;
}

// Default implementations for trivially derivable methods.
impl dyn ElementPort {
    /// Returns `true` if the element matches a CSS selector string.
    pub fn is_matches(&self, selector: &str) -> BridgeResult<bool> {
        self.query_selector(selector).map(|r| r.is_some())
    }

    /// Returns the computed CSS property value.
    pub fn get_computed_style(&self, _property: &str) -> BridgeResult<String> {
        Err(crate::error::BridgeError::NotImplemented("computed style"))
    }
}
