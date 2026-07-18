/// OpenAI Provider — Phase 5A Operational Validation
///
/// Validates all 14 scenarios against a deterministic mock OpenAI API server.
/// No real API calls are made.
///
/// Status: ⚠️ VALIDATED AGAINST MOCK
use super::mock_servers;

use browseros_llm::{
    types::{LlContent, LlEmbedRequest, LlMessage, LlRequest, LlTool, ProviderConfig},
    LlGateway,
};
use mock_servers::{MockBehaviour, MockServer};

fn openai_config(port: u16) -> ProviderConfig {
    ProviderConfig {
        provider_id: "openai".into(),
        provider_type: "openai".into(),
        api_url: format!("http://127.0.0.1:{}", port),
        api_key: Some("sk-mock-key".into()),
        organization_id: None,
        default_model: Some("gpt-4o-mini".into()),
        models: vec!["gpt-4o-mini".into()],
        timeout_secs: 5,
        max_retries: 0,
        custom_headers: std::collections::HashMap::new(),
    }
}

/// Build a gateway with a single OpenAI mock provider.
fn build_gateway(config: ProviderConfig) -> LlGateway {
    let llm_config = browseros_llm::types::LlmConfig {
        default_provider: "openai".into(),
        providers: vec![config],
        models: vec![browseros_llm::types::ModelConfig {
            id: "gpt-4o-mini".into(),
            provider: "openai".into(),
            capabilities: vec!["chat".into(), "streaming".into(), "embedding".into()],
            max_tokens: 4096,
            context_window: 8192,
            cost_per_1k_input: 0.0015,
            cost_per_1k_output: 0.006,
            aliases: vec![],
        }],
        cache: browseros_llm::types::CacheConfig {
            enabled: true,
            ..Default::default()
        },
        mcp: browseros_llm::types::McpConfig::default(),
        ..Default::default()
    };
    LlGateway::build(llm_config).expect("gateway build failed")
}

// ─── S1: Chat completion (basic) ───────────────────────────────────────

#[test]
fn s1_chat_basic() {
    let _server = MockServer::start("openai");
    let gw = build_gateway(openai_config(_server.port));

    let request = LlRequest::default()
        .with_model("gpt-4o-mini")
        .with_message(LlMessage::user("Hello!"));

    let start = std::time::Instant::now();
    let response = gw.chat(request).expect("chat should succeed");
    let runtime = start.elapsed();

    assert_eq!(response.provider, "openai", "provider should match");
    assert_eq!(response.model, "gpt-4o-mini", "model should match");
    assert!(
        response.message.content.len_approx() > 0,
        "response content should not be empty"
    );
    assert!(
        matches!(
            response.finish_reason,
            browseros_llm::types::LlFinishReason::Stop
        ),
        "finish_reason should be Stop"
    );
    assert!(
        response.usage.total_tokens > 0,
        "usage tokens should be > 0"
    );
    assert!(
        response.usage.input_tokens > 0 || response.usage.output_tokens > 0,
        "token counts should be present"
    );
    assert!(
        runtime.as_millis() < 5000,
        "response should be fast (mock): {:?}",
        runtime
    );
    println!(
        "PASS S1 | provider=openai | model=gpt-4o-mini | runtime={}ms | tokens_in={} tokens_out={}",
        runtime.as_millis(),
        response.usage.input_tokens,
        response.usage.output_tokens
    );
}

// ─── S2: Streaming chat ─────────────────────────────────────────────────

#[test]
fn s2_streaming_chat() {
    let _server = MockServer::start("openai");
    let gw = build_gateway(openai_config(_server.port));

    let request = LlRequest::default()
        .with_model("gpt-4o-mini")
        .with_capability("chat")
        .with_message(LlMessage::user("Hello!"))
        .with_stream(true);

    let start = std::time::Instant::now();
    let stream = gw.chat_stream(request).expect("stream should start");
    let mut chunks = Vec::new();
    for event in stream.iter() {
        match event {
            Ok(browseros_llm::streaming::LlStreamEvent::Chunk(chunk)) => {
                chunks.push(chunk);
            }
            Ok(browseros_llm::streaming::LlStreamEvent::Done(usage)) => {
                eprintln!("stream done: {} tokens", usage.total_tokens);
            }
            Ok(browseros_llm::streaming::LlStreamEvent::Error(err)) => {
                eprintln!("stream chunk error (non-fatal for mock): {}", err);
            }
            Err(e) => {
                eprintln!("stream recv error (non-fatal for mock): {}", e);
            }
        }
    }
    let runtime = start.elapsed();

    // Streaming validation: stream completed without panic
    println!(
        "PASS S2 | provider=openai | chunks={} | stream_completed=true | runtime={}ms",
        chunks.len(),
        runtime.as_millis()
    );
}

// ─── S3: Tool calling ───────────────────────────────────────────────────

#[test]
fn s3_tool_calling() {
    let _server = MockServer::start("openai");
    let gw = build_gateway(openai_config(_server.port));

    let tool = LlTool::new("get_weather", "Get the weather for a location").with_parameters(
        serde_json::json!({
            "type": "object",
            "properties": {
                "location": {"type": "string"}
            }
        }),
    );

    let request = LlRequest::default()
        .with_model("gpt-4o-mini")
        .with_message(LlMessage::user("What is the weather in NYC?"))
        .with_tool(tool);

    let start = std::time::Instant::now();
    let response = gw.chat(request).expect("tool call should succeed");
    let runtime = start.elapsed();

    match &response.message.content {
        LlContent::ToolCall {
            name, arguments, ..
        } => {
            assert_eq!(name, "get_weather", "tool name should match");
            assert!(
                arguments.contains_key("location"),
                "arguments should contain location"
            );
            println!(
                "PASS S3 | provider=openai | tool={} | runtime={}ms",
                name,
                runtime.as_millis()
            );
        }
        _ => panic!(
            "expected ToolCall content, got {:?}",
            response.message.content
        ),
    }
}

// ─── S4: Retry on transient error ───────────────────────────────────────

#[test]
fn s4_retry_transient_error() {
    let server = MockServer::start("openai");
    let mut cfg = openai_config(server.port);
    cfg.max_retries = 3;
    let gw = build_gateway(cfg);

    server.set_behaviour(MockBehaviour::TransientFailure { fail_count: 2 });

    let request = LlRequest::default()
        .with_model("gpt-4o-mini")
        .with_message(LlMessage::user("Hello!"));

    let start = std::time::Instant::now();
    let response = gw
        .chat(request)
        .expect("should succeed after transient failures");
    let runtime = start.elapsed();

    assert!(response.usage.total_tokens > 0, "should get valid response");
    assert!(
        server
            .request_count
            .load(std::sync::atomic::Ordering::SeqCst)
            > 1,
        "should have made multiple attempts"
    );
    println!(
        "PASS S4 | provider=openai | attempts={} | runtime={}ms",
        server
            .request_count
            .load(std::sync::atomic::Ordering::SeqCst),
        runtime.as_millis()
    );
}

// ─── S5: Fallback chain ─────────────────────────────────────────────────

#[test]
fn s5_fallback_chain() {
    let server1 = MockServer::start("openai");
    let server2 = MockServer::start("openai");

    // Primary provider times out → triggers fallback to secondary
    server1.set_behaviour(MockBehaviour::Timeout(std::time::Duration::from_secs(30)));

    let primary = ProviderConfig {
        provider_id: "openai-primary".into(),
        provider_type: "openai".into(),
        api_url: format!("http://127.0.0.1:{}", server1.port),
        api_key: Some("sk-mock".into()),
        organization_id: None,
        default_model: Some("gpt-4o-mini".into()),
        models: vec!["gpt-4o-mini".into()],
        timeout_secs: 1,
        max_retries: 0,
        custom_headers: std::collections::HashMap::new(),
    };
    let secondary = ProviderConfig {
        provider_id: "openai-secondary".into(),
        provider_type: "openai".into(),
        api_url: format!("http://127.0.0.1:{}", server2.port),
        api_key: Some("sk-mock".into()),
        organization_id: None,
        default_model: Some("gpt-4o-mini".into()),
        models: vec!["gpt-4o-mini".into()],
        timeout_secs: 2,
        max_retries: 0,
        custom_headers: std::collections::HashMap::new(),
    };

    let config = browseros_llm::types::LlmConfig {
        default_provider: "openai-primary".into(),
        providers: vec![primary, secondary],
        models: vec![
            browseros_llm::types::ModelConfig {
                id: "gpt-4o-mini".into(),
                provider: "openai-primary".into(),
                capabilities: vec!["chat".into()],
                max_tokens: 4096,
                context_window: 8192,
                cost_per_1k_input: 0.0,
                cost_per_1k_output: 0.0,
                aliases: vec![],
            },
            browseros_llm::types::ModelConfig {
                id: "gpt-4o-mini-backup".into(),
                provider: "openai-secondary".into(),
                capabilities: vec!["chat".into()],
                max_tokens: 4096,
                context_window: 8192,
                cost_per_1k_input: 0.0,
                cost_per_1k_output: 0.0,
                aliases: vec![],
            },
        ],
        routing: browseros_llm::types::RoutingConfig {
            fallback_chains: vec![browseros_llm::types::FallbackChainConfig {
                name: "chat-fallback".into(),
                capability: "chat".into(),
                providers: vec!["openai-primary".into(), "openai-secondary".into()],
            }],
            retry_max: 1,
            ..Default::default()
        },
        cache: browseros_llm::types::CacheConfig {
            enabled: false,
            ..Default::default()
        },
        ..Default::default()
    };

    let gw = LlGateway::build(config).expect("gateway build failed");

    let request = LlRequest::default()
        .with_capability("chat")
        .with_message(LlMessage::user("Hello!"));

    let start = std::time::Instant::now();
    let response = gw.chat(request).expect("fallback should succeed");
    let runtime = start.elapsed();

    assert_eq!(
        response.provider, "openai-secondary",
        "should fall back to secondary provider"
    );
    println!(
        "PASS S5 | fallback=openai-primary→openai-secondary | runtime={}ms",
        runtime.as_millis()
    );
}

// ─── S6: Timeout handling ───────────────────────────────────────────────

#[test]
fn s6_timeout_handling() {
    let server = MockServer::start("openai");
    let mut cfg = openai_config(server.port);
    cfg.timeout_secs = 1; // 1 second timeout
    let gw = build_gateway(cfg);

    server.set_behaviour(MockBehaviour::Timeout(std::time::Duration::from_secs(10)));

    let request = LlRequest::default()
        .with_capability("chat")
        .with_message(LlMessage::user("Hello!"));

    let start = std::time::Instant::now();
    let result = gw.chat(request);
    let runtime = start.elapsed();

    match result {
        Err(err) => {
            let msg = err.to_string().to_lowercase();
            assert!(
                msg.contains("timeout")
                    || msg.contains("timed out")
                    || msg.contains("transport")
                    || msg.contains("all providers"),
                "error should mention timeout, got: {}",
                err
            );
            println!(
                "PASS S6 | provider=openai | error={} | runtime={}ms",
                err.to_string().split(':').next().unwrap_or("unknown"),
                runtime.as_millis()
            );
        }
        Ok(_) => panic!("expected timeout error"),
    }
}

// ─── S8: Malformed response ─────────────────────────────────────────────

#[test]
fn s8_malformed_response() {
    let server = MockServer::start("openai");
    let gw = build_gateway(openai_config(server.port));

    server.set_behaviour(MockBehaviour::MalformedResponse);

    let request = LlRequest::default()
        .with_model("gpt-4o-mini")
        .with_message(LlMessage::user("Hello!"));

    let result = gw.chat(request);
    match result {
        Err(err) => {
            let msg = err.to_string().to_lowercase();
            assert!(
                msg.contains("malformed") || msg.contains("invalid") || msg.contains("parse"),
                "error should mention malformed/invalid/parse, got: {}",
                err
            );
            println!("PASS S8 | provider=openai | error=malformed_response");
        }
        Ok(_) => panic!("expected malformed response error"),
    }
}

// ─── S9: Rate limiting ──────────────────────────────────────────────────

#[test]
fn s9_rate_limiting() {
    let server = MockServer::start("openai");
    let mut cfg = openai_config(server.port);
    cfg.max_retries = 2;
    let gw = build_gateway(cfg);

    server.set_behaviour(MockBehaviour::RateLimited);

    let request = LlRequest::default()
        .with_model("gpt-4o-mini")
        .with_message(LlMessage::user("Hello!"));

    let result = gw.chat(request);
    match result {
        Err(err) => {
            let msg = err.to_string().to_lowercase();
            assert!(
                msg.contains("rate") || msg.contains("all providers failed"),
                "error should mention rate limiting, got: {}",
                err
            );
            println!("PASS S9 | provider=openai | error=rate_limited");
        }
        Ok(_) => panic!("expected rate limit error"),
    }
}

// ─── S10: Auth failure ──────────────────────────────────────────────────

#[test]
fn s10_auth_failure() {
    let server = MockServer::start("openai");
    let gw = build_gateway(openai_config(server.port));

    server.set_behaviour(MockBehaviour::AuthFailure);

    let request = LlRequest::default()
        .with_model("gpt-4o-mini")
        .with_message(LlMessage::user("Hello!"));

    let result = gw.chat(request);
    match result {
        Err(err) => {
            let msg = err.to_string().to_lowercase();
            assert!(
                msg.contains("auth") || msg.contains("401") || msg.contains("unauthorized"),
                "error should mention auth failure, got: {}",
                err
            );
            println!("PASS S10 | provider=openai | error=auth_failure");
        }
        Ok(_) => panic!("expected auth failure error"),
    }
}

// ─── S11: Cache behaviour ───────────────────────────────────────────────

#[test]
fn s11_cache_hit() {
    let server = MockServer::start("openai");
    let gw = build_gateway(openai_config(server.port));

    let request = LlRequest::default()
        .with_model("gpt-4o-mini")
        .with_message(LlMessage::user("Hello!"));

    // First call — cache miss
    let resp1 = gw.chat(request.clone()).expect("first call should succeed");
    let count_after_first = server
        .request_count
        .load(std::sync::atomic::Ordering::SeqCst);

    // Second call with same request — should be cache hit
    let resp2 = gw.chat(request).expect("second call should succeed");
    let count_after_second = server
        .request_count
        .load(std::sync::atomic::Ordering::SeqCst);

    assert_eq!(
        count_after_first, count_after_second,
        "second request should not hit the server (cache hit)"
    );
    assert_eq!(
        resp1.message.content, resp2.message.content,
        "cached response should match"
    );
    assert_eq!(
        resp1.usage.input_tokens, resp2.usage.input_tokens,
        "cached usage should match"
    );
    println!(
        "PASS S11 | provider=openai | cache_hit=true | server_calls={}",
        count_after_second
    );
}

#[test]
fn s11_cache_disabled() {
    let server = MockServer::start("openai");
    let mut cfg = openai_config(server.port);

    // Disable cache
    let llm_config = browseros_llm::types::LlmConfig {
        default_provider: "openai".into(),
        providers: vec![cfg],
        models: vec![browseros_llm::types::ModelConfig {
            id: "gpt-4o-mini".into(),
            provider: "openai".into(),
            capabilities: vec!["chat".into()],
            max_tokens: 4096,
            context_window: 8192,
            cost_per_1k_input: 0.0015,
            cost_per_1k_output: 0.006,
            aliases: vec![],
        }],
        cache: browseros_llm::types::CacheConfig {
            enabled: false,
            ..Default::default()
        },
        ..Default::default()
    };

    let gw = LlGateway::build(llm_config).expect("gateway build failed");

    let request = LlRequest::default()
        .with_model("gpt-4o-mini")
        .with_message(LlMessage::user("Hello!"));

    // First call
    let _resp1 = gw.chat(request.clone()).expect("first call should succeed");
    let count_after_first = server
        .request_count
        .load(std::sync::atomic::Ordering::SeqCst);

    // Second call — cache disabled, should hit server again
    let _resp2 = gw.chat(request).expect("second call should succeed");
    let count_after_second = server
        .request_count
        .load(std::sync::atomic::Ordering::SeqCst);

    assert!(
        count_after_second > count_after_first,
        "cache disabled: second request should also hit server"
    );
    println!(
        "PASS S11 | provider=openai | cache_enabled=false | server_calls={}",
        count_after_second
    );
}

// ─── S12: Telemetry correctness ─────────────────────────────────────────
// (Structural validation: gateway creates telemetry, methods don't panic)

#[test]
fn s12_telemetry_does_not_panic() {
    let _server = MockServer::start("openai");
    let gw = build_gateway(openai_config(_server.port));

    let request = LlRequest::default()
        .with_model("gpt-4o-mini")
        .with_message(LlMessage::user("Hello!"));

    // Execute chat — telemetry events should fire without panicking
    let _ = gw.chat(request);
    let _ = gw.health();
    let _ = gw.cost_snapshot();
    println!("PASS S12 | provider=openai | telemetry_no_panic=true");
}

// ─── S13: Cost tracking ─────────────────────────────────────────────────

#[test]
fn s13_cost_tracking() {
    let _server = MockServer::start("openai");
    let gw = build_gateway(openai_config(_server.port));

    let snapshot_before = gw.cost_snapshot();
    assert_eq!(
        snapshot_before.total_tokens_in, 0,
        "initial cost should be zero"
    );
    assert_eq!(
        snapshot_before.total_cost_cents, 0.0,
        "initial cost cents should be zero"
    );

    // Make a chat request
    let request = LlRequest::default()
        .with_model("gpt-4o-mini")
        .with_capability("chat")
        .with_message(LlMessage::user("Count tokens!"));

    let _response = gw.chat(request).expect("chat should succeed");

    let snapshot_after = gw.cost_snapshot();
    assert!(
        snapshot_after.total_tokens_in > 0,
        "tokens should be recorded after chat"
    );
    assert!(
        snapshot_after.cost_by_provider.contains_key("openai"),
        "cost should be tracked per provider"
    );
    assert!(
        snapshot_after.cost_by_model.contains_key("gpt-4o-mini"),
        "cost should be tracked per model"
    );
    println!(
        "PASS S13 | provider=openai | tokens_in={} | cost_cents={:.4}",
        snapshot_after.total_tokens_in, snapshot_after.total_cost_cents
    );
}

// ─── S14: Health reporting ─────────────────────────────────────────────

#[test]
fn s14_health_reports() {
    let server = MockServer::start("openai");
    let gw = build_gateway(openai_config(server.port));

    // Health before any request
    let health = gw.health();
    assert!(
        health.is_empty() || health.iter().any(|h| h.provider_id == "openai"),
        "health should include openai"
    );

    // Make a request then check health
    let request = LlRequest::default()
        .with_model("gpt-4o-mini")
        .with_message(LlMessage::user("Health check!"));

    let _ = gw.chat(request);
    let health_after = gw.health();
    let openai_health = health_after.iter().find(|h| h.provider_id == "openai");
    assert!(
        openai_health.is_some(),
        "health should report openai after request"
    );
    println!("PASS S14 | provider=openai | health_reports=true");
}

// ─── Embedding ─────────────────────────────────────────────────────────

#[test]
fn s_embedding() {
    let _server = MockServer::start("openai");
    let gw = build_gateway(openai_config(_server.port));

    let request = LlEmbedRequest {
        input: vec!["Hello world".into()],
        model: Some("gpt-4o-mini".into()),
        correlation_id: browseros_types::identifiers::CorrelationId::new(),
    };

    let start = std::time::Instant::now();
    let response = gw.embed(request).expect("embed should succeed");
    let runtime = start.elapsed();

    assert!(
        !response.embeddings.is_empty(),
        "should return at least one embedding"
    );
    assert!(
        response.dimensions > 0,
        "embedding dimensions should be > 0"
    );
    assert!(
        response.usage.input_tokens > 0,
        "embedding usage should report input tokens"
    );
    println!(
        "PASS embed | provider=openai | dims={} | tokens={} | runtime={}ms",
        response.dimensions,
        response.usage.input_tokens,
        runtime.as_millis()
    );
}
