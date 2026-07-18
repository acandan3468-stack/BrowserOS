# browseros-mcp Tool Registry Design

**Document:** TOOL_REGISTRY_DESIGN.md  
**Phase:** Design Freeze  
**Date:** 2026-07-16  

---

## 1. ToolRegistry

### 1.1 Structure

```rust
pub struct ToolRegistry {
    tools: RwLock<HashMap<String, RegisteredTool>>,
    namespace_index: HashMap<&str, Vec<&str>>,   // "dom" → ["dom/query", "dom/click", ...]
}

struct RegisteredTool {
    tool: Box<dyn McpTool>,
    metadata: ToolMetadata,
}

pub struct ToolMetadata {
    name: String,
    display_name: Option<String>,
    namespace: String,            // "dom", "browser", "system", etc.
    version: semver::Version,
    visibility: ToolVisibility,
    capabilities: Vec<ToolCapability>,
    permissions: Vec<ToolPermission>,
    is_hidden: bool,
    is_internal: bool,
    is_diagnostic: bool,
    added_at: chrono::DateTime<chrono::Utc>,
    deprecation: Option<DeprecationInfo>,
}
```

### 1.2 McpTool Trait

```rust
pub trait McpTool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn input_schema(&self) -> serde_json::Value;
    fn execute(&self, ctx: McpToolContext, params: serde_json::Value) -> Result<ToolOutput, ToolError>;

    // Optional metadata methods
    fn display_name(&self) -> Option<&str> { None }
    fn version(&self) -> semver::Version { semver::Version::new(1, 0, 0) }
    fn visibility(&self) -> ToolVisibility { ToolVisibility::Public }
    fn capabilities(&self) -> Vec<ToolCapability> { vec![] }
    fn permissions(&self) -> Vec<ToolPermission> { vec![] }
    fn is_hidden(&self) -> bool { false }
    fn is_internal(&self) -> bool { false }
    fn is_diagnostic(&self) -> bool { false }
    fn categories(&self) -> Vec<&str> { vec![] }
}
```

### 1.3 Visibility and Access Control

```rust
#[non_exhaustive]
pub enum ToolVisibility {
    Public,           // Visible to all clients
    Internal,         // Visible to authenticated internal clients
    Hidden,           // Can be called but not listed in tools/list
    Diagnostic,       // Only visible in debug mode
}

#[non_exhaustive]
pub enum ToolCapability {
    RequiresSession,
    ReadOnly,
    MutatesState,
    RequiresBrowser,
    RequiresLLM,
    RequiresNetwork,
    LongRunning,
    CanProduceProgress,
    SupportsCancellation,
    Idempotent,
}

#[non_exhaustive]
pub enum ToolPermission {
    /// No special permission required
    None,
    /// Requires an active session
    SessionRequired,
    /// Requires browser access
    BrowserAccess,
    /// Requires network interception
    NetworkAccess,
    /// Requires workflow execution
    WorkflowExecution,
    /// Requires credential access
    CredentialAccess,
    /// Admin-level access
    Admin,
}
```

---

## 2. Registration

### 2.1 Static Registration (Phase 6)

Tools are registered at server startup via McpServerBuilder:

```rust
let server = McpServerBuilder::new()
    .with_runtime(runtime)
    .with_tool("system/health", SystemHealthTool::new())
    .with_tool("system/version", SystemVersionTool::new())
    .with_tool("session/create", SessionCreateTool::new())
    .with_tool("session/close", SessionCloseTool::new())
    .build();
```

### 2.2 Dynamic Registration (Phase 8+)

```rust
// Called by plugins at runtime
registry.register(name: &str, tool: Box<dyn McpTool>) -> Result<(), RegistrationError>;
registry.unregister(name: &str) -> Result<(), RegistrationError>;

// Validation rules:
// 1. Name must match namespace/name pattern
// 2. Namespace must exist (created on first registration)
// 3. Duplicate name → RegistrationError::AlreadyExists
// 4. Plugin tools prefixed with "ext/" namespace
```

---

## 3. Discovery (tools/list)

### 3.1 Default Response

Returns all tools where:
- `visibility` is `Public`
- `is_hidden` is `false`
- `is_internal` is `false`
- `is_diagnostic` is `false`

### 3.2 Filtered Response

Support MCP protocol extensions for filtering:

```json
{
  "name": "tools/list",
  "params": {
    "capabilities": ["RequiresBrowser", "ReadOnly"],
    "namespace": "dom",
    "include_hidden": false,
    "include_internal": false,
    "include_diagnostic": false,
    "session_id": "sess_abc123"
  }
}
```

Filter logic:
1. Start with all registered tools
2. Apply visibility filter (default: exclude hidden/internal/diagnostic)
3. Apply capability filter (AND — tool must have ALL requested)
4. Apply namespace filter (exact match)
5. Apply session_id filter (only tools compatible with session state)

### 3.3 Lookup Complexity

| Operation | Complexity | Structure |
|-----------|------------|-----------|
| Register | O(1) | HashMap insert |
| Unregister | O(1) | HashMap remove |
| Lookup by name | O(1) | HashMap get |
| List all (unfiltered) | O(n) | HashMap values |
| List by namespace | O(m) | Namespace index (m = tools in namespace) |
| List by capability | O(n) | Full scan + filter |
| List by permission | O(n) | Full scan + filter |

Optimization for Phase 8+: Add capability index and permission index if profiling shows n > 500.

---

## 4. Metadata and Schema Storage

### 4.1 JSON Schema

Each tool provides its `input_schema` as a JSON Schema (draft 2020-12). The schema is:

- Stored in memory as `serde_json::Value`
- Returned verbatim in `tools/list` response as `inputSchema`
- Validated server-side on each `tools/call` invocation
- Validation: structural (required fields, types) but not semantic (e.g., "valid URL" is runtime)

### 4.2 Schema Validation

```rust
fn validate_params(schema: &serde_json::Value, params: &serde_json::Value) -> Result<(), ToolError> {
    // Use the json schema's meta-schema to validate
    // On failure: ToolError::InvalidParams with field-level details
}
```

Implementation note: `jsonschema` or `valico` crate for JSON Schema validation. Minimal dependency footprint.

---

## 5. Namespace Hierarchy

```
system/         → health, config, metrics, version, logs, shutdown
session/        → create, close, list, config, credentials
browser/        → navigate, go_back, go_forward, reload, screenshot, pdf,
                  evaluate, title, url, tabs, new_tab, close_tab, wait_navigation
dom/            → query_selector, query_selector_all, click, type_text, select,
                  hover, focus, scroll_into_view, get_text, get_attribute,
                  set_attribute, get_html, set_html, get_value, set_value,
                  snapshot, wait_for_element, wait_for_function, observe_mutations,
                  find_by_text, find_by_placeholder, find_by_role, find_by_label
network/        → intercept, continue_request, abort_request, fulfill_request,
                  set_conditions, clear_conditions, list_requests,
                  get_cookies, set_cookie, delete_cookie, clear_cookies,
                  get_har, start_har, stop_har
storage/        → localstorage_get, localstorage_set, localstorage_delete,
                  localstorage_clear,
                  sessionstorage_get, sessionstorage_set, sessionstorage_delete,
                  sessionstorage_clear,
                  indexeddb_list, indexeddb_delete
workflow/       → execute, plan, status, cancel, list, result, define, validate, logs
llm/            → models (diagnostic only)
ext/            → future plugin namespace (reserved)
```

---

## 6. Aliases

Tools can have alternative names for backwards compatibility or convenience:

```rust
ToolMetadata {
    name: "dom/query_selector",
    aliases: vec!["dom/query", "dom/$"],
    // ...
}
```

Alias resolution:
1. Look up exact name
2. If not found, look up aliases index
3. If still not found → ToolError::NotFound

---

## 7. Versioning

### 7.1 Tool Versioning

Each tool carries a semantic version. When a tool's input schema or behavior changes:

- **Major version bump:** Breaking change (field removed, type changed)
- **Minor version bump:** Non-breaking addition (new optional field)
- **Patch version bump:** Bug fix, no schema change

### 7.2 MCP API Versioning

The server exposes `serverInfo.version` during `initialize`. This is the overall BrowserOS version. Individual tool versions are discoverable via metadata.

### 7.3 Deprecation

```rust
struct DeprecationInfo {
    deprecation_message: String,
    replacement_tool: Option<String>,
    removal_version: semver::Version,
}
```

Deprecated tools still appear in `tools/list` but with a `deprecated` flag set to `true`.

---

## 8. Capability Filtering

Tool capabilities enable client-side filtering. A client can request "all read-only browser tools" or "all tools that support cancellation".

Internal use: The `workflow/plan` tool uses capability filtering to select which tools are available for planning.

---

## 9. Hidden and Internal Tools

| Visibility | Shown in tools/list? | Callable? | Use Case |
|------------|---------------------|-----------|----------|
| Public | Yes | Yes | Normal operation |
| Hidden | No | Yes (if client knows name) | Migration, legacy support |
| Internal | No | Yes (with auth) | Internal BrowserOS tools |
| Diagnostic | No | Yes (debug mode) | Health checks, testing |

---

## 10. Future Plugin Registration (Phase 8+)

```rust
// Plugin registration pattern
pub trait McpPlugin: Send + Sync {
    fn name(&self) -> &str;
    fn register_tools(&self, registry: &mut ToolRegistry) -> Result<(), RegistrationError>;
}

// Dynamic library loading
// Each .so/.dll exposes `fn mcp_plugin() -> Box<dyn McpPlugin>`
// Server loads plugins from config.plugins_dir at startup
```

External plugins are prefixed with `ext/{plugin_name}/` to avoid namespace collisions.

---

*This document defines the complete Tool Registry design. It contains no Rust code, no Cargo.toml, and no placeholders.*
