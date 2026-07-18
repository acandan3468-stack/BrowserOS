use browseros_llm::LlProvider;
use browseros_llm::{McpAdapter, McpServerConfig};
use serde_json;

fn echo_server_config() -> (McpServerConfig, String) {
    let exe = env!("CARGO_BIN_EXE_mcp_echo_server").to_string();
    let config = McpServerConfig {
        name: "echo".into(),
        command: exe,
        args: vec![],
        transport: "stdio".into(),
        base_url: None,
        auto_start: true,
    };
    (config, "mcp-echo".into())
}

#[test]
fn mcp_integration_discover_and_call_tools() {
    let (config, _model_id) = echo_server_config();
    let adapter = McpAdapter::new(config).expect("failed to spawn echo server");
    adapter.initialize().expect("failed to initialize");

    // Verify tools are discovered
    let tools = adapter.tools();
    assert_eq!(tools.len(), 1, "expected 1 tool");
    assert_eq!(tools[0].name, "echo");

    // Verify tool capabilities appear
    let caps = adapter.capabilities();
    assert!(caps.contains(&browseros_llm::types::ProviderCapability::ToolUse));

    // Call the echo tool
    let result = adapter
        .call_tool("echo", serde_json::json!({"message": "hello world"}), 5000)
        .expect("tool call failed");
    assert!(
        result.contains("Echo:"),
        "response should contain Echo: but got: {result}"
    );
    assert!(
        result.contains("hello"),
        "response should contain hello but got: {result}"
    );
}

#[test]
fn mcp_integration_unknown_tool_returns_error() {
    let (config, _model_id) = echo_server_config();
    let adapter = McpAdapter::new(config).expect("failed to spawn echo server");
    adapter.initialize().expect("failed to initialize");

    let result = adapter.call_tool("nonexistent-tool", serde_json::json!({}), 1000);
    assert!(result.is_err(), "expected error for unknown tool");
    match result {
        Err(browseros_llm::mcp::errors::McpError::ToolNotFound(name)) => {
            assert_eq!(name, "nonexistent-tool");
        }
        Err(other) => panic!("expected ToolNotFound, got: {other:?}"),
        Ok(_) => unreachable!(),
    }
}

#[test]
fn mcp_integration_health_after_initialize() {
    let (config, _model_id) = echo_server_config();
    let adapter = McpAdapter::new(config).expect("failed to spawn echo server");
    adapter.initialize().expect("failed to initialize");

    let health = adapter.health();
    assert!(
        matches!(health.status, browseros_llm::types::ProviderHealth::Healthy),
        "expected Healthy after init, got: {:?}",
        health.status
    );
}

#[test]
fn mcp_integration_health_before_initialize() {
    let (config, _model_id) = echo_server_config();
    let adapter = McpAdapter::new(config).expect("failed to spawn echo server");

    let health = adapter.health();
    assert!(
        matches!(
            health.status,
            browseros_llm::types::ProviderHealth::Unavailable { .. }
        ),
        "expected Unavailable before init, got: {:?}",
        health.status
    );
}

#[test]
fn mcp_integration_drop_cleans_up_process() {
    let (config, _model_id) = echo_server_config();
    let adapter = McpAdapter::new(config).expect("failed to spawn echo server");
    adapter.initialize().expect("failed to initialize");

    // Drop the adapter — should kill the subprocess
    drop(adapter);

    // Creating a new adapter with the same command confirms no port/file contention
    let (config2, _model_id2) = echo_server_config();
    let adapter2 = McpAdapter::new(config2).expect("failed to spawn echo server after drop");
    adapter2
        .initialize()
        .expect("failed to initialize after drop");
    let tools = adapter2.tools();
    assert_eq!(
        tools.len(),
        1,
        "tools should still be discoverable after re-spawn"
    );
}

#[test]
fn mcp_integration_sequential_tool_calls() {
    let (config, _model_id) = echo_server_config();
    let adapter = McpAdapter::new(config).expect("failed to spawn echo server");
    adapter.initialize().expect("failed to initialize");

    for i in 0..5 {
        let result = adapter
            .call_tool("echo", serde_json::json!({"counter": i}), 5000)
            .unwrap_or_else(|e| panic!("tool call {i} failed: {e}"));
        assert!(
            result.contains(&format!("{i}")),
            "call {i} should contain counter value"
        );
    }
}
