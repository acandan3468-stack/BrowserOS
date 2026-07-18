use crate::tools::McpTool;
use crate::types::{McpToolContext, ToolCapability, ToolError, ToolOutput, ToolVisibility};

/// Returns server and protocol version information.
pub struct SystemVersionTool;

impl SystemVersionTool {
    pub fn new() -> Self {
        SystemVersionTool
    }
}

impl Default for SystemVersionTool {
    fn default() -> Self {
        Self::new()
    }
}

impl McpTool for SystemVersionTool {
    fn name(&self) -> &str {
        "system/version"
    }

    fn description(&self) -> &str {
        "Returns server version, protocol version, and registered tool count"
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
        let info = serde_json::json!({
            "server_name": "browseros-mcp",
            "server_version": "0.1.0",
            "protocol_version": "2024-11-05",
        });

        let text = serde_json::to_string_pretty(&info)
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
