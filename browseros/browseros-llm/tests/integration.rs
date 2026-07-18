use browseros_llm::types::{
    CacheConfig, LlEmbedRequest, LlMessage, LlRequest, LlmConfig, ModelConfig, ProviderConfig,
};
use browseros_llm::LlGateway;

fn provider_cfg(id: &str, ptype: &str) -> ProviderConfig {
    ProviderConfig {
        provider_id: id.into(),
        provider_type: ptype.into(),
        api_url: "http://127.0.0.1:1".into(),
        api_key: Some("test-key".into()),
        organization_id: None,
        default_model: None,
        models: vec!["test-model".into()],
        timeout_secs: 1,
        max_retries: 0,
        custom_headers: std::collections::HashMap::new(),
    }
}

fn model_cfg(id: &str, provider: &str) -> ModelConfig {
    ModelConfig {
        id: id.into(),
        provider: provider.into(),
        capabilities: vec![],
        max_tokens: 4096,
        context_window: 8192,
        cost_per_1k_input: 1.0,
        cost_per_1k_output: 2.0,
        aliases: vec![],
    }
}

fn build(config: LlmConfig) -> LlGateway {
    match LlGateway::build(config) {
        Ok(gw) => gw,
        Err(e) => panic!("build failed: {}", e),
    }
}

#[test]
fn gateway_build_with_single_provider() {
    let config = LlmConfig {
        providers: vec![provider_cfg("openai", "openai")],
        models: vec![model_cfg("test-model", "openai")],
        ..Default::default()
    };
    let gw = build(config);
    // Health tracker starts empty; providers are populated on use
    assert!(gw.health().is_empty());
    assert!(gw.cost_snapshot().total_cost_cents == 0.0);
}

#[test]
fn gateway_build_with_duplicate_provider_ids() {
    let config = LlmConfig {
        providers: vec![
            provider_cfg("dup", "openai"),
            provider_cfg("dup", "anthropic"),
        ],
        ..Default::default()
    };
    match LlGateway::build(config) {
        Err(e) => {
            let msg = e.to_string();
            assert!(
                msg.contains("duplicate") || msg.contains("already registered"),
                "unexpected error: {}",
                msg
            );
        }
        Ok(_) => panic!("expected duplicate error"),
    }
}

#[test]
fn gateway_build_with_unknown_provider_type() {
    let config = LlmConfig {
        providers: vec![provider_cfg("bad", "nonexistent")],
        ..Default::default()
    };
    match LlGateway::build(config) {
        Err(_) => {}
        Ok(_) => panic!("expected error"),
    }
}

#[test]
fn gateway_build_with_cache_disabled() {
    let config = LlmConfig {
        providers: vec![provider_cfg("openai", "openai")],
        models: vec![model_cfg("test-model", "openai")],
        cache: CacheConfig {
            enabled: false,
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(build(config).health().is_empty());
}

#[test]
fn gateway_build_with_zero_providers() {
    let config = LlmConfig {
        providers: vec![],
        ..Default::default()
    };
    assert!(build(config).health().is_empty());
}

#[test]
fn gateway_build_with_minimal_config() {
    assert!(build(LlmConfig::default()).health().is_empty());
}

#[test]
fn gateway_default_is_usable() {
    assert!(LlGateway::default().health().is_empty());
}

#[test]
fn gateway_default_chat_unknown_model() {
    let req = LlRequest::default()
        .with_model("nonexistent")
        .with_message(LlMessage::user("hi"));
    if let Err(e) = LlGateway::default().chat(req) {
        assert!(e.to_string().contains("not found") || e.to_string().contains("unknown"));
    }
}

#[test]
fn gateway_default_embed_unknown_model() {
    let req = LlEmbedRequest {
        model: Some("nonexistent".into()),
        input: vec!["hello".into()],
        ..Default::default()
    };
    assert!(LlGateway::default().embed(req).is_err());
}

#[test]
fn gateway_default_stream_unknown_model() {
    let req = LlRequest::default()
        .with_model("nonexistent")
        .with_message(LlMessage::user("hi"));
    if let Err(e) = LlGateway::default().chat_stream(req) {
        assert!(e.to_string().contains("not found") || e.to_string().contains("unknown"));
    }
}

#[test]
fn gateway_default_cost_snapshot_is_zero() {
    assert_eq!(LlGateway::default().cost_snapshot().total_cost_cents, 0.0);
}

#[test]
fn gateway_default_clear_cache_does_not_panic() {
    LlGateway::default().clear_cache();
}

#[test]
fn gateway_send_sync() {
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}
    assert_send::<LlGateway>();
    assert_sync::<LlGateway>();
}

#[test]
fn gateway_concurrent_access_from_multiple_threads() {
    let gw = std::sync::Arc::new(build(LlmConfig::default()));
    let mut handles = Vec::new();
    for _ in 0..10 {
        let gw = std::sync::Arc::clone(&gw);
        handles.push(std::thread::spawn(move || {
            let _ = gw.health();
            let _ = gw.cost_snapshot();
            gw.clear_cache();
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
}

#[test]
fn gateway_concurrent_health_from_many_threads() {
    let gw = std::sync::Arc::new(LlGateway::default());
    let mut handles = Vec::new();
    for _ in 0..20 {
        let gw = std::sync::Arc::clone(&gw);
        handles.push(std::thread::spawn(move || {
            for _ in 0..10 {
                let _ = gw.health();
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
}
