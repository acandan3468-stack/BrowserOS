mod anthropic;
mod gemini;
pub mod http;
mod http_generic;
mod ollama;
mod openai;

use crate::error::LlmError;
use crate::provider::LlProvider;
use crate::types::ProviderConfig;

pub use anthropic::AnthropicAdapter;
pub use gemini::GeminiAdapter;
pub use http_generic::GenericHttpAdapter;
pub use ollama::OllamaAdapter;
pub use openai::OpenAIAdapter;

/// Create a provider adapter from configuration.
pub fn create_provider(config: &ProviderConfig) -> Result<Box<dyn LlProvider>, LlmError> {
    match config.provider_type.as_str() {
        "openai" => Ok(Box::new(OpenAIAdapter::new(config.clone()))),
        "anthropic" => Ok(Box::new(AnthropicAdapter::new(config.clone()))),
        "ollama" => Ok(Box::new(OllamaAdapter::new(config.clone()))),
        "gemini" => Ok(Box::new(GeminiAdapter::new(config.clone()))),
        "generic" | "http" | "custom" => Ok(Box::new(GenericHttpAdapter::new(config.clone()))),
        _ => Err(LlmError::ConfigurationError(format!(
            "unknown provider type '{}' for provider '{}'",
            config.provider_type, config.provider_id
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config(provider_type: &str, provider_id: &str) -> ProviderConfig {
        ProviderConfig {
            provider_id: provider_id.into(),
            provider_type: provider_type.into(),
            api_url: "http://localhost:9999".into(),
            api_key: Some("test-key".into()),
            organization_id: None,
            default_model: Some("test-model".into()),
            models: vec!["test-model".into()],
            timeout_secs: 30,
            max_retries: 3,
            custom_headers: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn create_openai() {
        let cfg = test_config("openai", "my-openai");
        let provider = create_provider(&cfg).unwrap();
        assert_eq!(provider.id(), "my-openai");
        assert!(provider
            .capabilities()
            .contains(&crate::types::ProviderCapability::Chat));
    }

    #[test]
    fn create_anthropic() {
        let cfg = test_config("anthropic", "my-anthropic");
        let provider = create_provider(&cfg).unwrap();
        assert_eq!(provider.id(), "my-anthropic");
    }

    #[test]
    fn create_ollama() {
        let cfg = test_config("ollama", "my-ollama");
        let provider = create_provider(&cfg).unwrap();
        assert_eq!(provider.id(), "my-ollama");
        assert!(provider
            .capabilities()
            .contains(&crate::types::ProviderCapability::Embedding));
    }

    #[test]
    fn create_gemini() {
        let cfg = test_config("gemini", "my-gemini");
        let provider = create_provider(&cfg).unwrap();
        assert_eq!(provider.id(), "my-gemini");
    }

    #[test]
    fn create_generic_http() {
        for pt in &["generic", "http", "custom"] {
            let cfg = test_config(pt, "my-generic");
            let provider = create_provider(&cfg).unwrap();
            assert_eq!(provider.id(), "my-generic", "type={}", pt);
        }
    }

    #[test]
    fn create_unknown_type_returns_error() {
        let cfg = test_config("nonexistent", "bad");
        match create_provider(&cfg) {
            Err(e) => assert!(
                e.to_string().contains("unknown provider type"),
                "got: {}",
                e
            ),
            Ok(_) => panic!("expected error"),
        }
    }
}
