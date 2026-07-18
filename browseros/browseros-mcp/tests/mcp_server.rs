use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

const BINARY_NAME: &str = "browseros_mcp_server";

struct McpClient {
    child: Child,
    stdin: std::process::ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
}

impl McpClient {
    fn start() -> Self {
        let binary = std::env::var(format!("CARGO_BIN_EXE_{BINARY_NAME}"))
            .unwrap_or_else(|_| panic!("CARGO_BIN_EXE_{BINARY_NAME} not set"));

        let mut child = Command::new(&binary)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("failed to spawn MCP server");

        let stdin = child.stdin.take().expect("stdin not captured");
        let stdout = BufReader::new(child.stdout.take().expect("stdout not captured"));

        McpClient {
            child,
            stdin,
            stdout,
        }
    }

    fn send(&mut self, json: &str) {
        writeln!(self.stdin, "{json}").expect("write to stdin");
        self.stdin.flush().expect("flush stdin");
    }

    fn receive(&mut self, timeout: Duration) -> String {
        let start = std::time::Instant::now();
        let mut line = String::new();
        loop {
            if start.elapsed() > timeout {
                panic!("response timeout after {timeout:?}");
            }
            match self.stdout.read_line(&mut line) {
                Ok(0) => panic!("stdout closed before response"),
                Ok(_) => return line.trim().to_string(),
                Err(e) => panic!("read error: {e}"),
            }
        }
    }

    fn send_and_receive(&mut self, json: &str, timeout: Duration) -> serde_json::Value {
        self.send(json);
        let line = self.receive(timeout);
        serde_json::from_str(&line).expect("parse JSON response")
    }

    fn close(mut self) {
        drop(self.stdin);
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            if let Ok(Some(_)) = self.child.try_wait() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn mcp_initialize_handshake() {
    let mut client = McpClient::start();
    let timeout = Duration::from_secs(10);

    let resp = client.send_and_receive(
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
        timeout,
    );

    assert_eq!(resp["jsonrpc"], "2.0");
    assert_eq!(resp["id"], 1);
    assert_eq!(resp["result"]["protocolVersion"], "2024-11-05");
    assert_eq!(resp["result"]["serverInfo"]["name"], "browseros-mcp");

    client.send(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);
    client.close();
}

#[test]
fn mcp_tools_list() {
    let mut client = McpClient::start();
    let timeout = Duration::from_secs(10);

    let _ = client.send_and_receive(
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
        timeout,
    );
    client.send(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);

    let resp = client.send_and_receive(
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#,
        timeout,
    );

    assert_eq!(resp["jsonrpc"], "2.0");
    assert_eq!(resp["id"], 2);

    let tools = resp["result"]["tools"].as_array().expect("tools array");
    assert!(tools.len() >= 2, "expected >=2 tools, got {}", tools.len());

    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"system/health"));
    assert!(names.contains(&"system/version"));

    client.close();
}

#[test]
fn mcp_call_system_health() {
    let mut client = McpClient::start();
    let timeout = Duration::from_secs(10);

    let _ = client.send_and_receive(
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
        timeout,
    );
    client.send(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);

    let resp = client.send_and_receive(
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"system/health","arguments":{}}}"#,
        timeout,
    );

    assert_eq!(resp["jsonrpc"], "2.0");
    assert_eq!(resp["id"], 3);

    let content = resp["result"]["content"].as_array().expect("content array");
    assert!(!content.is_empty());
    let text = content[0]["text"].as_str().expect("text field");
    assert!(text.contains("status"));
    assert!(text.contains("healthy"));
    assert!(text.contains("uptime_seconds"));

    client.close();
}

#[test]
fn mcp_call_system_version() {
    let mut client = McpClient::start();
    let timeout = Duration::from_secs(10);

    let _ = client.send_and_receive(
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
        timeout,
    );
    client.send(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);

    let resp = client.send_and_receive(
        r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"system/version","arguments":{}}}"#,
        timeout,
    );

    assert_eq!(resp["jsonrpc"], "2.0");
    assert_eq!(resp["id"], 4);

    let content = resp["result"]["content"].as_array().expect("content array");
    let text = content[0]["text"].as_str().expect("text field");
    assert!(text.contains("server_version"));
    assert!(text.contains("0.1.0"));
    assert!(text.contains("protocol_version"));

    client.close();
}

#[test]
fn mcp_unknown_tool_returns_error() {
    let mut client = McpClient::start();
    let timeout = Duration::from_secs(10);

    let _ = client.send_and_receive(
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
        timeout,
    );
    client.send(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);

    let resp = client.send_and_receive(
        r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"nonexistent/tool","arguments":{}}}"#,
        timeout,
    );

    assert!(resp.get("error").is_some(), "expected error");
    assert_eq!(resp["error"]["code"], -32601);

    client.close();
}

#[test]
fn mcp_unknown_method_returns_error() {
    let mut client = McpClient::start();
    let timeout = Duration::from_secs(10);

    let _ = client.send_and_receive(
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
        timeout,
    );
    client.send(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);

    let resp = client.send_and_receive(
        r#"{"jsonrpc":"2.0","id":6,"method":"some/unknown","params":{}}"#,
        timeout,
    );

    assert!(resp.get("error").is_some(), "expected error");
    assert_eq!(resp["error"]["code"], -32601);

    client.close();
}

#[test]
fn mcp_parse_error_on_invalid_json() {
    let mut client = McpClient::start();
    let timeout = Duration::from_secs(10);

    client.send("this is not valid json");

    let line = client.receive(timeout);
    let resp: serde_json::Value = serde_json::from_str(&line).expect("valid JSON response");

    assert_eq!(resp["error"]["code"], -32700);

    client.close();
}

#[test]
fn mcp_shutdown_via_request() {
    let mut client = McpClient::start();
    let timeout = Duration::from_secs(10);

    let _ = client.send_and_receive(
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
        timeout,
    );
    client.send(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);

    let resp = client.send_and_receive(
        r#"{"jsonrpc":"2.0","id":7,"method":"shutdown","params":{}}"#,
        timeout,
    );
    assert_eq!(resp["id"], 7);
    assert!(resp.get("result").is_some(), "expected result");

    drop(client.stdin);
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut exited = false;
    while std::time::Instant::now() < deadline {
        if let Ok(Some(_)) = client.child.try_wait() {
            exited = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(exited, "process should exit within 5 seconds");
}
