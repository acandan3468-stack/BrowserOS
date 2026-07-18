use std::io::Write;

use serde_json::Value;

fn main() {
    for line in std::io::stdin().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => return,
        };

        let req: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => {
                respond(serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": {"code": -32700, "message": "Parse error"}
                }));
                continue;
            }
        };

        let id = &req["id"];

        match req.get("method").and_then(|m| m.as_str()) {
            None | Some("notifications/initialized") => {}
            Some("initialize") => {
                respond(serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "protocolVersion": "2024-11-05",
                        "capabilities": {"tools": {}},
                        "serverInfo": {"name": "echo-server", "version": "1.0"}
                    }
                }));
            }
            Some("tools/list") => {
                respond(serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "tools": [
                            {
                                "name": "echo",
                                "description": "Echo input arguments back",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {
                                        "message": {"type": "string"}
                                    }
                                }
                            }
                        ]
                    }
                }));
            }
            Some("tools/call") => {
                let args = &req["params"]["arguments"];
                let text = format!("Echo: {}", serde_json::to_string(args).unwrap_or_default());
                respond(serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "content": [{"type": "text", "text": text}]
                    }
                }));
            }
            Some(method) => {
                respond(serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {"code": -32601, "message": format!("Unknown method: {method}")}
                }));
            }
        }
    }
}

fn respond(resp: Value) {
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{}", serde_json::to_string(&resp).unwrap());
    let _ = stdout.flush();
}
