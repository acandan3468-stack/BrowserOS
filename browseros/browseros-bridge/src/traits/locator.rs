use crate::error::BridgeResult;
use crate::locator::LocatorStrategy;
use crate::traits::ElementPort;
use std::time::Duration;

/// Declarative element finding for a page.
///
/// `LocatorPort` resolves a `LocatorStrategy` into element handles.
/// Unlike direct DOM traversal, locators support auto-waiting and
/// can be retried until a timeout.
pub trait LocatorPort: Send + Sync {
    /// Locate the first element matching the strategy.
    fn locate(&self, strategy: &LocatorStrategy) -> BridgeResult<Option<Box<dyn ElementPort>>>;

    /// Locate all elements matching the strategy.
    fn locate_all(&self, strategy: &LocatorStrategy) -> BridgeResult<Vec<Box<dyn ElementPort>>>;

    /// Wait for an element matching the strategy to appear within the timeout.
    fn wait_for(
        &self,
        strategy: &LocatorStrategy,
        timeout: Duration,
    ) -> BridgeResult<Box<dyn ElementPort>>;

    /// Wait for the absence of any element matching the strategy.
    fn wait_for_absence(&self, strategy: &LocatorStrategy, timeout: Duration) -> BridgeResult<()>;
}

/// Low-level DOM querying for a frame or page.
///
/// `LocatorEngine` provides direct DOM traversal operations without
/// auto-waiting or retry logic. It is the underlying engine that
/// `LocatorPort` implementations use.
pub trait LocatorEngine: Send + Sync {
    /// Find the first element matching a CSS selector.
    fn query_selector(&self, selector: &str) -> BridgeResult<Option<Box<dyn ElementPort>>>;

    /// Find all elements matching a CSS selector.
    fn query_selector_all(&self, selector: &str) -> BridgeResult<Vec<Box<dyn ElementPort>>>;

    /// Find elements by their visible text content.
    fn query_by_text(&self, text: &str, exact: bool) -> BridgeResult<Vec<Box<dyn ElementPort>>>;

    /// Find elements by an XPath expression.
    fn query_by_xpath(&self, expression: &str) -> BridgeResult<Vec<Box<dyn ElementPort>>>;
}
