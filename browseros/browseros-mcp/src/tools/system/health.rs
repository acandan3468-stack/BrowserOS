use crate::server::metrics::uptime_secs;
use crate::tools::McpTool;
use crate::types::{McpToolContext, ToolCapability, ToolError, ToolOutput, ToolVisibility};

/// Reports server health status including uptime and subsystem state.
pub struct SystemHealthTool;

impl SystemHealthTool {
    pub fn new() -> Self {
        SystemHealthTool
    }
}

impl Default for SystemHealthTool {
    fn default() -> Self {
        Self::new()
    }
}

impl McpTool for SystemHealthTool {
    fn name(&self) -> &str {
        "system/health"
    }

    fn description(&self) -> &str {
        "Reports server health status including uptime and subsystem state"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    fn execute(
        &self,
        _ctx: McpToolContext,
        _params: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let health = serde_json::json!({
            "status": "healthy",
            "uptime_seconds": uptime_secs(),
            "server": {
                "name": "browseros-mcp",
                "version": "0.1.0",
            },
        });

        let text = serde_json::to_string_pretty(&health)
            .map_err(|e| ToolError::ExecutionError(format!("serialization error: {e}")))?;
        Ok(ToolOutput::text(text))
    }

    fn visibility(&self) -> ToolVisibility {
        ToolVisibility::Public
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly]
    }
}
