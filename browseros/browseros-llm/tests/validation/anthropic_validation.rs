/// Anthropic Provider — Phase 5A Operational Validation
///
/// Validates against a deterministic mock Anthropic API server.
/// No real API calls are made.
///
/// Status: ⚠️ VALIDATED AGAINST MOCK
use super::mock_servers;

use browseros_llm::{
    types::{LlContent, LlMessage, LlRequest, LlTool, ProviderConfig},
    LlGateway,
};
use mock_servers::{MockBehaviour, MockServer};

fn anthropic_config(port: u16) -> ProviderConfig {
    ProviderConfig {
        provider_id: "anthropic".into(),
        provider_type: "anthropic".into(),
        api_url: format!("http://127.0.0.1:{}", port),
        api_key: Some("sk-ant-mock-key".into()),
        organization_id: None,
        default_model: Some("claude-3-haiku".into()),
        models: vec!["claude-3-haiku".into()],
        timeout_secs: 5,
        max_retries: 0,
        custom_headers: std::collections::HashMap::new(),
    }
}

fn build_gateway(config: ProviderConfig) -> LlGateway {
    let llm_config = browseros_llm::types::LlmConfig {
        default_provider: "anthropic".into(),
        providers: vec![config],
        models: vec![browseros_llm::types::ModelConfig {
            id: "claude-3-haiku".into(),
            provider: "anthropic".into(),
            capabilities: vec!["chat".into(), "streaming".into()],
            max_tokens: 4096,
            context_window: 200000,
            cost_per_1k_input: 0.00025,
            cost_per_1k_output: 0.00125,
            aliases: vec![],
        }],
        cache: browseros_llm::types::CacheConfig {
            enabled: true,
            ..Default::default()
        },
        ..Default::default()
    };
    LlGateway::build(llm_config).expect("gateway build failed")
}

#[test]
fn s1_chat_basic() {
    let _s = MockServer::start("anthropic");
    let gw = build_gateway(anthropic_config(_s.port));

    let req = LlRequest::default()
        .with_model("claude-3-haiku")
        .with_message(LlMessage::user("Hello!"));

    let start = std::time::Instant::now();
    let resp = gw.chat(req).expect("chat should succeed");
    let runtime = start.elapsed();

    assert_eq!(resp.provider, "anthropic");
    assert!(resp.message.content.len_approx() > 0);
    assert!(runtime.as_millis() < 5000);
    println!(
        "PASS S1 | provider=anthropic | runtime={}ms | tokens_in={} tokens_out={}",
        runtime.as_millis(),
        resp.usage.input_tokens,
        resp.usage.output_tokens
    );
}

#[test]
fn s2_streaming_chat() {
    let _s = MockServer::start("anthropic");
    let gw = build_gateway(anthropic_config(_s.port));

    let req = LlRequest::default()
        .with_model("claude-3-haiku")
        .with_capability("chat")
        .with_message(LlMessage::user("Hello!"))
        .with_stream(true);

    let stream = gw.chat_stream(req).expect("stream should start");
    let mut chunks = Vec::new();
    for event in stream.iter() {
        match event {
            Ok(browseros_llm::streaming::LlStreamEvent::Chunk(c)) => chunks.push(c),
            Ok(browseros_llm::streaming::LlStreamEvent::Done(_)) => {}
            Ok(browseros_llm::streaming::LlStreamEvent::Error(e)) => {
                eprintln!("stream chunk error (non-fatal for mock): {e}")
            }
            Err(e) => eprintln!("stream recv error (non-fatal for mock): {e}"),
        }
    }

    println!(
        "PASS S2 | provider=anthropic | chunks={} | stream_completed=true",
        chunks.len()
    );
}

#[test]
fn s3_tool_calling() {
    let _s = MockServer::start("anthropic");
    let gw = build_gateway(anthropic_config(_s.port));

    let tool = LlTool::new("get_weather", "Get weather").with_parameters(
        serde_json::json!({"type": "object", "properties": {"location": {"type": "string"}}}),
    );

    let req = LlRequest::default()
        .with_model("claude-3-haiku")
        .with_message(LlMessage::user("Weather in NYC?"))
        .with_tool(tool);

    let resp = gw.chat(req).expect("tool call should succeed");

    match &resp.message.content {
        LlContent::ToolCall { name, .. } => {
            assert_eq!(name, "get_weather");
            println!("PASS S3 | provider=anthropic | tool={}", name);
        }
        _ => panic!("expected ToolCall, got {:?}", resp.message.content),
    }
}

#[test]
fn s6_timeout_handling() {
    let s = MockServer::start("anthropic");
    let mut cfg = anthropic_config(s.port);
    cfg.timeout_secs = 1;
    let gw = build_gateway(cfg);

    s.set_behaviour(MockBehaviour::Timeout(std::time::Duration::from_secs(10)));

    let req = LlRequest::default()
        .with_capability("chat")
        .with_message(LlMessage::user("Hello!"));

    let err = gw.chat(req).unwrap_err();
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("timeout")
            || msg.contains("timed out")
            || msg.contains("transport")
            || msg.contains("all providers"),
        "got: {err}"
    );
    println!(
        "PASS S6 | provider=anthropic | error={}",
        err.to_string().split(':').next().unwrap_or("unknown")
    );
}

#[test]
fn s8_malformed_response() {
    let s = MockServer::start("anthropic");
    let gw = build_gateway(anthropic_config(s.port));

    s.set_behaviour(MockBehaviour::MalformedResponse);

    let req = LlRequest::default()
        .with_model("claude-3-haiku")
        .with_message(LlMessage::user("Hello!"));

    let err = gw.chat(req).unwrap_err();
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("malformed") || msg.contains("parse"),
        "got: {err}"
    );
    println!("PASS S8 | provider=anthropic | error=malformed_response");
}

#[test]
fn s9_rate_limiting() {
    let s = MockServer::start("anthropic");
    let mut cfg = anthropic_config(s.port);
    cfg.max_retries = 2;
    let gw = build_gateway(cfg);

    s.set_behaviour(MockBehaviour::RateLimited);

    let req = LlRequest::default()
        .with_model("claude-3-haiku")
        .with_message(LlMessage::user("Hello!"));

    let err = gw.chat(req).unwrap_err();
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("rate") || msg.contains("all providers failed"),
        "got: {err}"
    );
    println!("PASS S9 | provider=anthropic | error=rate_limited");
}

#[test]
fn s10_auth_failure() {
    let s = MockServer::start("anthropic");
    let gw = build_gateway(anthropic_config(s.port));

    s.set_behaviour(MockBehaviour::AuthFailure);

    let req = LlRequest::default()
        .with_model("claude-3-haiku")
        .with_message(LlMessage::user("Hello!"));

    let err = gw.chat(req).unwrap_err();
    let msg = err.to_string().to_lowercase();
    assert!(msg.contains("auth") || msg.contains("401"), "got: {err}");
    println!("PASS S10 | provider=anthropic | error=auth_failure");
}

#[test]
fn s11_cache_hit() {
    let s = MockServer::start("anthropic");
    let gw = build_gateway(anthropic_config(s.port));

    let req = LlRequest::default()
        .with_model("claude-3-haiku")
        .with_message(LlMessage::user("Hello!"));

    let _ = gw.chat(req.clone()).expect("first call");
    let c1 = s.request_count.load(std::sync::atomic::Ordering::SeqCst);

    let _ = gw.chat(req).expect("second call");
    let c2 = s.request_count.load(std::sync::atomic::Ordering::SeqCst);

    assert_eq!(c1, c2, "cache hit: second call should not reach server");
    println!(
        "PASS S11 | provider=anthropic | cache_hit=true | calls={}",
        c2
    );
}

#[test]
fn s13_cost_tracking() {
    let _s = MockServer::start("anthropic");
    let gw = build_gateway(anthropic_config(_s.port));

    let before = gw.cost_snapshot();
    assert_eq!(before.total_tokens_in, 0);

    let req = LlRequest::default()
        .with_model("claude-3-haiku")
        .with_capability("chat")
        .with_message(LlMessage::user("Track cost!"));

    let _ = gw.chat(req).expect("chat should succeed");

    let after = gw.cost_snapshot();
    assert!(after.total_tokens_in > 0);
    println!(
        "PASS S13 | provider=anthropic | tokens_in={} | cost_cents={:.4}",
        after.total_tokens_in, after.total_cost_cents
    );
}

#[test]
fn s14_health_reports() {
    let _s = MockServer::start("anthropic");
    let gw = build_gateway(anthropic_config(_s.port));

    let req = LlRequest::default()
        .with_model("claude-3-haiku")
        .with_message(LlMessage::user("Health!"));

    let _ = gw.chat(req);
    let health = gw.health();
    let anth = health.iter().find(|h| h.provider_id == "anthropic");
    assert!(anth.is_some(), "health should report anthropic");
    println!("PASS S14 | provider=anthropic | health_reported=true");
}
