use crate::error::BridgeResult;
use crate::identifiers::InterceptionHandle;
use crate::types::{InterceptionRule, NetworkConditions};

/// Interface for network monitoring and interception on a page.
///
/// `NetworkPort` controls how network requests are handled: offline
/// emulation, request interception, cache management, and network
/// condition simulation.
pub trait NetworkPort: Send + Sync {
    /// Enable or disable offline mode.
    fn set_offline(&self, offline: bool) -> BridgeResult<()>;

    /// Set network conditions (latency, throughput).
    fn set_conditions(&self, conditions: NetworkConditions) -> BridgeResult<()>;

    /// Register a request interception rule. Returns a handle for removal.
    fn add_interception_rule(&self, rule: InterceptionRule) -> BridgeResult<InterceptionHandle>;

    /// Remove a previously registered interception rule.
    fn remove_interception_rule(&self, handle: &InterceptionHandle) -> BridgeResult<()>;

    /// Clear the browser cache.
    fn clear_cache(&self) -> BridgeResult<()>;

    /// Clear all cookies.
    fn clear_cookies(&self) -> BridgeResult<()>;
}
