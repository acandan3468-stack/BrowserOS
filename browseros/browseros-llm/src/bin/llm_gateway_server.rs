use std::io::Write;

use browseros_llm::{LlEmbedRequest, LlGateway, LlMessage, LlRequest, LlmConfig};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let config = if args.len() > 1 {
        let path = &args[1];
        let content = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("failed to read config '{path}': {e}"));
        serde_json::from_str::<LlmConfig>(&content)
            .unwrap_or_else(|e| panic!("failed to parse config '{path}': {e}"))
    } else {
        eprintln!("WARN: no config file argument provided, using default (empty) configuration");
        LlmConfig::default()
    };

    let gateway = LlGateway::build(config).expect("failed to build LLM gateway");

    for line in std::io::stdin().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        let req: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => {
                respond(serde_json::json!({
                    "jsonrpc": "2.0", "id": null,
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
                    "jsonrpc": "2.0", "id": id,
                    "result": {
                        "protocolVersion": "2024-11-05",
                        "capabilities": {"tools": {}},
                        "serverInfo": {"name": "browseros-llm-gateway", "version": "0.1.0"}
                    }
                }));
            }
            Some("tools/list") => {
                respond(serde_json::json!({
                    "jsonrpc": "2.0", "id": id,
                    "result": {
                        "tools": [
                            {
                                "name": "chat",
                                "description": "Send a chat message to an LLM and get a text response",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {
                                        "model": {"type": "string", "description": "Model ID (optional)"},
                                        "capability": {"type": "string", "description": "Capability hint: chat, fast, cheap (optional)"},
                                        "system": {"type": "string", "description": "System prompt"},
                                        "messages": {
                                            "type": "array",
                                            "items": {
                                                "type": "object",
                                                "properties": {
                                                    "role": {"type": "string", "enum": ["user", "assistant", "system"]},
                                                    "content": {"type": "string"}
                                                },
                                                "required": ["role", "content"]
                                            },
                                            "description": "Chat messages"
                                        }
                                    },
                                    "required": ["messages"]
                                }
                            },
                            {
                                "name": "embed",
                                "description": "Generate embeddings for text",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {
                                        "model": {"type": "string", "description": "Model ID (optional)"},
                                        "input": {"type": "string", "description": "Text to embed"}
                                    },
                                    "required": ["input"]
                                }
                            },
                            {
                                "name": "health",
                                "description": "Check health status of all configured LLM providers",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {}
                                }
                            }
                        ]
                    }
                }));
            }
            Some("tools/call") => {
                let tool_name = req["params"]["name"].as_str().unwrap_or("");
                let t_args = &req["params"]["arguments"];
                match tool_name {
                    "chat" => handle_chat(&gateway, id, t_args),
                    "embed" => handle_embed(&gateway, id, t_args),
                    "health" => handle_health(&gateway, id, t_args),
                    _ => {
                        respond(serde_json::json!({
                            "jsonrpc": "2.0", "id": id,
                            "error": {"code": -32601, "message": format!("Unknown tool: {tool_name}")}
                        }));
                    }
                }
            }
            Some(m) => {
                respond(serde_json::json!({
                    "jsonrpc": "2.0", "id": id,
                    "error": {"code": -32601, "message": format!("Unknown method: {m}")}
                }));
            }
        }
    }
}

fn handle_chat(gateway: &LlGateway, id: &serde_json::Value, args: &serde_json::Value) {
    let mut request = LlRequest::default().with_capability("chat");

    if let Some(model) = args.get("model").and_then(|m| m.as_str()) {
        if !model.is_empty() {
            request = request.with_model(model);
        }
    }
    if let Some(system) = args.get("system").and_then(|s| s.as_str()) {
        if !system.is_empty() {
            request = request.with_message(LlMessage::system(system));
        }
    }
    if let Some(msgs) = args.get("messages").and_then(|m| m.as_array()) {
        for msg in msgs {
            let role = msg["role"].as_str().unwrap_or("user");
            let content = msg["content"].as_str().unwrap_or("");
            match role {
                "system" => request = request.with_message(LlMessage::system(content)),
                "assistant" => request = request.with_message(LlMessage::assistant(content)),
                _ => request = request.with_message(LlMessage::user(content)),
            }
        }
    }

    match gateway.chat(request) {
        Ok(response) => {
            let text = match &response.message.content {
                browseros_llm::LlContent::Text(t) => t.clone(),
                _ => "(non-text response)".to_string(),
            };
            respond(serde_json::json!({
                "jsonrpc": "2.0", "id": id,
                "result": {
                    "content": [{"type": "text", "text": text}],
                    "meta": {
                        "model": response.model,
                        "provider": response.provider,
                        "tokens_in": response.usage.input_tokens,
                        "tokens_out": response.usage.output_tokens,
                        "cost_cents": response.usage.cost_estimate_cents
                    }
                }
            }));
        }
        Err(err) => {
            respond(serde_json::json!({
                "jsonrpc": "2.0", "id": id,
                "error": {"code": -32000, "message": err.to_string()}
            }));
        }
    }
}

fn handle_embed(gateway: &LlGateway, id: &serde_json::Value, args: &serde_json::Value) {
    let input_text = args["input"].as_str().unwrap_or("").to_string();
    let model = args["model"].as_str().map(|s| s.to_string());

    let request = LlEmbedRequest {
        input: vec![input_text],
        model,
        ..Default::default()
    };

    match gateway.embed(request) {
        Ok(response) => {
            respond(serde_json::json!({
                "jsonrpc": "2.0", "id": id,
                "result": {
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string(&response.embeddings).unwrap_or_default()
                    }],
                    "meta": {
                        "model": response.model,
                        "tokens": response.usage.total_tokens,
                        "dimensions": response.dimensions
                    }
                }
            }));
        }
        Err(err) => {
            respond(serde_json::json!({
                "jsonrpc": "2.0", "id": id,
                "error": {"code": -32000, "message": err.to_string()}
            }));
        }
    }
}

fn handle_health(gateway: &LlGateway, id: &serde_json::Value, _args: &serde_json::Value) {
    let reports = gateway.health();
    let mut lines = Vec::new();
    for r in &reports {
        lines.push(format!(
            "provider={} status={:?} latency_ms={} error_rate={:.2} models={}",
            r.provider_id,
            r.status,
            r.latency_p50_ms,
            r.error_rate,
            r.models.join(","),
        ));
    }
    let text = if lines.is_empty() {
        "no providers configured".to_string()
    } else {
        lines.join("\n")
    };
    respond(serde_json::json!({
        "jsonrpc": "2.0", "id": id,
        "result": {
            "content": [{"type": "text", "text": text}]
        }
    }));
}

fn respond(resp: serde_json::Value) {
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{}", serde_json::to_string(&resp).unwrap());
    let _ = stdout.flush();
}
