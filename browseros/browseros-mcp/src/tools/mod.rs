pub mod system;

use std::collections::HashMap;
use std::sync::RwLock;

use browseros_llm::mcp::jsonrpc::McpToolDef;

use crate::types::{
    McpToolContext, ToolCapability, ToolError, ToolOutput, ToolPermission, ToolVisibility,
};

/// Trait that every MCP tool must implement.
pub trait McpTool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn input_schema(&self) -> serde_json::Value;
    fn execute(
        &self,
        ctx: McpToolContext,
        params: serde_json::Value,
    ) -> Result<ToolOutput, ToolError>;

    fn display_name(&self) -> Option<&str> {
        None
    }

    fn visibility(&self) -> ToolVisibility {
        ToolVisibility::Public
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![]
    }

    fn permissions(&self) -> Vec<ToolPermission> {
        vec![]
    }

    fn is_hidden(&self) -> bool {
        false
    }

    fn is_internal(&self) -> bool {
        false
    }

    fn is_diagnostic(&self) -> bool {
        false
    }

    fn categories(&self) -> Vec<&str> {
        vec![]
    }

    fn tool_def(&self) -> McpToolDef {
        McpToolDef {
            name: self.name().to_string(),
            description: self.description().to_string(),
            input_schema: self.input_schema(),
        }
    }
}

struct RegisteredTool {
    tool: Box<dyn McpTool>,
}

/// Thread-safe registry of MCP tools.
pub struct ToolRegistry {
    tools: RwLock<HashMap<String, RegisteredTool>>,
    namespace_index: RwLock<HashMap<String, Vec<String>>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        ToolRegistry {
            tools: RwLock::new(HashMap::new()),
            namespace_index: RwLock::new(HashMap::new()),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn register(&self, name: &str, tool: Box<dyn McpTool>) -> Result<(), RegistrationError> {
        if !name.contains('/') {
            return Err(RegistrationError::InvalidName(
                "tool name must contain namespace/name pattern".into(),
            ));
        }

        let mut tools = self
            .tools
            .write()
            .map_err(|_| RegistrationError::Internal("lock poisoned".into()))?;

        if tools.contains_key(name) {
            return Err(RegistrationError::AlreadyExists(name.into()));
        }

        let namespace = name.rsplitn(2, '/').last().unwrap_or("").to_string();
        if let Ok(mut index) = self.namespace_index.write() {
            index.entry(namespace).or_default().push(name.to_string());
        }

        tools.insert(name.to_string(), RegisteredTool { tool });
        Ok(())
    }

    pub fn execute_by_name(
        &self,
        name: &str,
        ctx: McpToolContext,
        params: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let tools = self
            .tools
            .read()
            .map_err(|_| ToolError::ExecutionError("lock poisoned".into()))?;
        let registered = tools
            .get(name)
            .ok_or_else(|| ToolError::NotFound(name.into()))?;
        registered.tool.execute(ctx, params)
    }

    pub fn list_tools(&self) -> Vec<McpToolDef> {
        let tools = match self.tools.read() {
            Ok(guard) => guard,
            Err(_) => return vec![],
        };
        tools
            .values()
            .filter(|rt| {
                let v = rt.tool.visibility();
                let h = rt.tool.is_hidden();
                let i = rt.tool.is_internal();
                let d = rt.tool.is_diagnostic();
                !h && !i && !d && v.is_visible()
            })
            .map(|rt| rt.tool.tool_def())
            .collect()
    }

    pub fn list_all(&self) -> Vec<McpToolDef> {
        let tools = match self.tools.read() {
            Ok(guard) => guard,
            Err(_) => return vec![],
        };
        tools.values().map(|rt| rt.tool.tool_def()).collect()
    }

    pub fn len(&self) -> usize {
        self.tools.read().map(|g| g.len()).unwrap_or(0)
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Errors that can occur during tool registration.
#[derive(Debug)]
pub enum RegistrationError {
    AlreadyExists(String),
    InvalidName(String),
    Internal(String),
}

impl std::fmt::Display for RegistrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegistrationError::AlreadyExists(name) => write!(f, "tool already registered: {name}"),
            RegistrationError::InvalidName(msg) => write!(f, "invalid tool name: {msg}"),
            RegistrationError::Internal(msg) => write!(f, "internal error: {msg}"),
        }
    }
}

impl std::error::Error for RegistrationError {}

impl From<RegistrationError> for crate::error::McpServerError {
    fn from(e: RegistrationError) -> Self {
        crate::error::McpServerError::InternalError(e.to_string())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::types::McpToolContext;
    use std::sync::Arc;

    pub struct TestTool;

    impl McpTool for TestTool {
        fn name(&self) -> &str {
            "test/hello"
        }
        fn description(&self) -> &str {
            "A test tool"
        }
        fn input_schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object"})
        }
        fn execute(
            &self,
            _ctx: McpToolContext,
            params: serde_json::Value,
        ) -> Result<ToolOutput, ToolError> {
            let name = params
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("world");
            Ok(ToolOutput::text(format!("Hello, {name}!")))
        }
    }

    struct HiddenTool;

    impl McpTool for HiddenTool {
        fn name(&self) -> &str {
            "test/hidden"
        }
        fn description(&self) -> &str {
            "Hidden test tool"
        }
        fn input_schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object"})
        }
        fn execute(
            &self,
            _ctx: McpToolContext,
            _params: serde_json::Value,
        ) -> Result<ToolOutput, ToolError> {
            Ok(ToolOutput::text("hidden"))
        }
        fn is_hidden(&self) -> bool {
            true
        }
    }

    #[test]
    fn registry_register_and_list() {
        let registry = ToolRegistry::new();
        assert!(registry.is_empty());

        registry.register("test/hello", Box::new(TestTool)).unwrap();
        assert_eq!(registry.len(), 1);
        assert!(!registry.is_empty());

        let tools = registry.list_tools();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "test/hello");
    }

    #[test]
    fn registry_duplicate_detection() {
        let registry = ToolRegistry::new();
        registry.register("test/hello", Box::new(TestTool)).unwrap();
        let err = registry
            .register("test/hello", Box::new(TestTool))
            .unwrap_err();
        assert!(matches!(err, RegistrationError::AlreadyExists(_)));
    }

    #[test]
    fn registry_invalid_name() {
        let registry = ToolRegistry::new();
        let err = registry
            .register("invalid", Box::new(TestTool))
            .unwrap_err();
        assert!(matches!(err, RegistrationError::InvalidName(_)));
    }

    #[test]
    fn registry_hidden_tools_excluded_from_list() {
        let registry = ToolRegistry::new();
        registry.register("test/hello", Box::new(TestTool)).unwrap();
        registry
            .register("test/hidden", Box::new(HiddenTool))
            .unwrap();
        let tools = registry.list_tools();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "test/hello");
    }

    #[test]
    fn registry_list_all_includes_hidden() {
        let registry = ToolRegistry::new();
        registry.register("test/hello", Box::new(TestTool)).unwrap();
        registry
            .register("test/hidden", Box::new(HiddenTool))
            .unwrap();
        let tools = registry.list_all();
        assert_eq!(tools.len(), 2);
    }

    #[test]
    fn registry_execute_tool() {
        let registry = ToolRegistry::new();
        registry.register("test/hello", Box::new(TestTool)).unwrap();

        let params = serde_json::json!({"name": "BrowserOS"});
        let result = registry.execute_by_name(
            "test/hello",
            McpToolContext::new(Arc::new(
                browseros_runtime::RuntimeContext::builder()
                    .build()
                    .unwrap(),
            )),
            params,
        );
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(!output.is_error);
        assert_eq!(output.content[0].text, "Hello, BrowserOS!");
    }

    #[test]
    fn registry_execute_not_found() {
        let registry = ToolRegistry::new();
        let result = registry.execute_by_name(
            "nonexistent",
            McpToolContext::new(Arc::new(
                browseros_runtime::RuntimeContext::builder()
                    .build()
                    .unwrap(),
            )),
            serde_json::json!({}),
        );
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ToolError::NotFound(_)));
    }

    #[test]
    fn registry_empty_len() {
        let registry = ToolRegistry::new();
        assert_eq!(registry.len(), 0);
        assert!(registry.is_empty());
    }

    #[test]
    fn registration_error_display() {
        let err = RegistrationError::AlreadyExists("test/x".into());
        assert_eq!(err.to_string(), "tool already registered: test/x");

        let err = RegistrationError::InvalidName("no slash".into());
        assert!(err.to_string().contains("invalid tool name"));

        let err = RegistrationError::Internal("oops".into());
        assert!(err.to_string().contains("internal error"));
    }
}
