//! Model Registry — registered models, aliases, capabilities, and cost metadata.
//!
//! The registry is fully immutable after construction. All lookup operations
//! are O(1) hash-map lookups. Capability and alias indices are pre-built
//! during construction to avoid runtime iteration.

use crate::error::LlmError;
use crate::provider::LlProvider;
use crate::types::ModelConfig;
use std::collections::{HashMap, HashSet};

/// Registry of all configured models and their provider mappings.
///
/// Immutable after construction. All lookups are O(1).
#[derive(Debug)]
pub struct ModelRegistry {
    /// Model ID → ModelConfig
    models: HashMap<String, ModelConfig>,
    /// Provider ID → index into the providers vec passed at construction
    providers: HashMap<String, usize>,
    /// Capability name → model IDs ordered by registration priority
    capability_index: HashMap<String, Vec<String>>,
    /// Provider ID → list of model IDs registered to that provider
    provider_models: HashMap<String, Vec<String>>,
    /// Alias → model ID
    alias_index: HashMap<String, String>,
    /// Set of provider IDs known to be local (localhost)
    local_providers: HashSet<String>,
    /// Model IDs in registration order
    registration_order: Vec<String>,
}

impl ModelRegistry {
    /// Create a new registry from configuration and provider list.
    ///
    /// Returns `ConfigurationError` if a model references an unknown provider
    /// or a duplicate model/alias is detected.
    pub fn new(
        model_configs: &[ModelConfig],
        providers: &[Box<dyn LlProvider>],
    ) -> Result<Self, LlmError> {
        let mut models = HashMap::new();
        let mut provider_map = HashMap::new();
        let mut capability_index: HashMap<String, Vec<String>> = HashMap::new();
        let mut provider_models: HashMap<String, Vec<String>> = HashMap::new();
        let mut alias_index: HashMap<String, String> = HashMap::new();
        let mut local_providers: HashSet<String> = HashSet::new();
        let mut registration_order: Vec<String> = Vec::new();

        for (idx, provider) in providers.iter().enumerate() {
            let pid = provider.id().to_string();
            if provider_map.contains_key(&pid) {
                return Err(LlmError::ConfigurationError(format!(
                    "duplicate provider id: '{}'",
                    pid
                )));
            }
            provider_map.insert(pid, idx);
        }

        for mc in model_configs {
            if !provider_map.contains_key(&mc.provider) {
                return Err(LlmError::ConfigurationError(format!(
                    "model '{}' references unknown provider '{}'",
                    mc.id, mc.provider
                )));
            }

            if models.contains_key(&mc.id) {
                return Err(LlmError::ConfigurationError(format!(
                    "duplicate model id: '{}'",
                    mc.id
                )));
            }

            for alias in &mc.aliases {
                if alias_index.contains_key(alias) {
                    return Err(LlmError::ConfigurationError(format!(
                        "duplicate alias '{}' for model '{}'",
                        alias, mc.id
                    )));
                }
                alias_index.insert(alias.clone(), mc.id.clone());
            }

            if mc.capabilities.iter().any(|c| c == "local") {
                local_providers.insert(mc.provider.clone());
            }

            for cap in &mc.capabilities {
                if cap != "local" {
                    capability_index
                        .entry(cap.clone())
                        .or_default()
                        .push(mc.id.clone());
                }
            }

            provider_models
                .entry(mc.provider.clone())
                .or_default()
                .push(mc.id.clone());

            models.insert(mc.id.clone(), mc.clone());
            registration_order.push(mc.id.clone());
        }

        Ok(Self {
            models,
            providers: provider_map,
            capability_index,
            provider_models,
            alias_index,
            local_providers,
            registration_order,
        })
    }

    // ─── Model Lookups ─────────────────────────────────────────────────────

    /// Look up a model config by model ID.
    pub fn get(&self, model_id: &str) -> Option<&ModelConfig> {
        self.models.get(model_id)
    }

    /// Resolve a model ID by alias.
    pub fn resolve_alias<'a>(&'a self, input: &'a str) -> Option<&'a str> {
        // First check if input is a direct model ID
        if self.models.contains_key(input) {
            return Some(input);
        }
        // Then check aliases
        self.alias_index.get(input).map(|s| s.as_str())
    }

    /// Look up a model config by ID or alias.
    pub fn get_by_id_or_alias(&self, input: &str) -> Option<&ModelConfig> {
        let resolved = self.resolve_alias(input)?;
        self.models.get(resolved)
    }

    /// Return all model IDs that support a given capability.
    /// Models are returned in registration order (highest priority first).
    pub fn find_by_capability(&self, capability: &str) -> Vec<String> {
        self.capability_index
            .get(capability)
            .cloned()
            .unwrap_or_default()
    }

    // ─── Provider Lookups ──────────────────────────────────────────────────

    /// Get the provider index (into the original providers vec) for a provider ID.
    pub fn provider_index(&self, provider_id: &str) -> Option<usize> {
        self.providers.get(provider_id).copied()
    }

    /// Return the provider index for a given model ID.
    pub fn provider_for_model(&self, model_id: &str) -> Option<usize> {
        self.models
            .get(model_id)
            .and_then(|m| self.providers.get(&m.provider).copied())
    }

    /// Return the provider ID for a given model.
    pub fn provider_id_for_model(&self, model_id: &str) -> Option<&str> {
        self.models.get(model_id).map(|m| m.provider.as_str())
    }

    /// List all model IDs registered to a given provider.
    pub fn models_for_provider(&self, provider_id: &str) -> Vec<String> {
        self.provider_models
            .get(provider_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Check if a provider is running locally.
    pub fn is_local_provider(&self, provider_id: &str) -> bool {
        self.local_providers.contains(provider_id)
    }

    // ─── Model Metadata Queries ────────────────────────────────────────────

    /// Get context window size for a model.
    pub fn context_window(&self, model_id: &str) -> Option<usize> {
        self.models.get(model_id).map(|m| m.context_window)
    }

    /// Get max output tokens for a model.
    pub fn max_tokens(&self, model_id: &str) -> Option<usize> {
        self.models.get(model_id).map(|m| m.max_tokens)
    }

    /// Get cost per 1k input/output tokens.
    pub fn cost_rates(&self, model_id: &str) -> Option<(f64, f64)> {
        self.models
            .get(model_id)
            .map(|m| (m.cost_per_1k_input, m.cost_per_1k_output))
    }

    /// Get the capabilities list for a model.
    pub fn capabilities_for_model(&self, model_id: &str) -> Option<&[String]> {
        self.models.get(model_id).map(|m| m.capabilities.as_slice())
    }

    /// Get all aliases for a model.
    pub fn aliases_for_model(&self, model_id: &str) -> Option<&[String]> {
        self.models.get(model_id).map(|m| m.aliases.as_slice())
    }

    /// Estimate cost in cents for a given token count.
    pub fn estimate_cost(&self, model_id: &str, tokens_in: u64, tokens_out: u64) -> f64 {
        let (rate_in, rate_out) = self.cost_rates(model_id).unwrap_or((0.0, 0.0));
        let cost = (tokens_in as f64 / 1000.0 * rate_in) + (tokens_out as f64 / 1000.0 * rate_out);
        crate::provider::round_to_millicents(cost)
    }

    // ─── Enumeration ───────────────────────────────────────────────────────

    /// Return all registered model IDs.
    pub fn all_models(&self) -> Vec<String> {
        self.registration_order.clone()
    }

    /// Return all registered provider IDs.
    pub fn all_providers(&self) -> Vec<String> {
        self.providers.keys().cloned().collect()
    }

    /// Return the number of registered models.
    pub fn model_count(&self) -> usize {
        self.models.len()
    }

    /// Return the number of registered providers.
    pub fn provider_count(&self) -> usize {
        self.providers.len()
    }

    /// Check if a model ID exists.
    pub fn has_model(&self, model_id: &str) -> bool {
        self.models.contains_key(model_id)
    }

    /// Check if a provider ID exists.
    pub fn has_provider(&self, provider_id: &str) -> bool {
        self.providers.contains_key(provider_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::LlmError;
    use crate::provider::LlProvider;
    use crate::types::{
        ProviderCapability, ProviderEmbedRequest, ProviderEmbedResponse, ProviderHealthResult,
        ProviderRequest, ProviderResponse, ProviderStream,
    };

    // ─── Mock Provider ────────────────────────────────────────────────────

    struct TestProvider {
        id: String,
        models: Vec<String>,
        caps: Vec<ProviderCapability>,
    }

    impl TestProvider {
        fn new(id: &str, models: Vec<&str>) -> Self {
            Self {
                id: id.to_string(),
                models: models.iter().map(|s| s.to_string()).collect(),
                caps: vec![ProviderCapability::Chat, ProviderCapability::Streaming],
            }
        }
    }

    impl LlProvider for TestProvider {
        fn id(&self) -> &str {
            &self.id
        }
        fn name(&self) -> &str {
            &self.id
        }
        fn capabilities(&self) -> Vec<ProviderCapability> {
            self.caps.clone()
        }
        fn models(&self) -> Vec<String> {
            self.models.clone()
        }
        fn chat(&self, _request: ProviderRequest) -> Result<ProviderResponse, LlmError> {
            Err(LlmError::ProviderUnavailable("mock".into()))
        }
        fn chat_stream(&self, _request: ProviderRequest) -> Result<ProviderStream, LlmError> {
            Err(LlmError::StreamingUnsupported)
        }
        fn embed(&self, _request: ProviderEmbedRequest) -> Result<ProviderEmbedResponse, LlmError> {
            Err(LlmError::ProviderUnavailable("mock".into()))
        }
        fn health(&self) -> ProviderHealthResult {
            unimplemented!()
        }
    }

    fn make_config(id: &str, provider: &str, caps: Vec<&str>, aliases: Vec<&str>) -> ModelConfig {
        ModelConfig {
            id: id.to_string(),
            provider: provider.to_string(),
            capabilities: caps.iter().map(|s| s.to_string()).collect(),
            max_tokens: 4096,
            context_window: 8192,
            cost_per_1k_input: 1.0,
            cost_per_1k_output: 2.0,
            aliases: aliases.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn build_providers() -> Vec<Box<dyn LlProvider>> {
        vec![
            Box::new(TestProvider::new("openai", vec!["gpt-4", "gpt-3.5"])),
            Box::new(TestProvider::new("anthropic", vec!["claude-3"])),
            Box::new(TestProvider::new("ollama", vec!["llama3"])),
        ]
    }

    // ─── Registration ──────────────────────────────────────────────────────

    #[test]
    fn register_single_model() {
        let providers = build_providers();
        let configs = vec![make_config("gpt-4", "openai", vec!["chat"], vec![])];
        let registry = ModelRegistry::new(&configs, &providers).unwrap();
        assert_eq!(registry.model_count(), 1);
        assert_eq!(registry.provider_count(), 3);
        assert!(registry.has_model("gpt-4"));
    }

    #[test]
    fn register_multiple_models_same_provider() {
        let providers = build_providers();
        let configs = vec![
            make_config("gpt-4", "openai", vec!["chat"], vec![]),
            make_config("gpt-3.5", "openai", vec!["chat"], vec![]),
        ];
        let registry = ModelRegistry::new(&configs, &providers).unwrap();
        assert_eq!(registry.model_count(), 2);
        assert_eq!(registry.models_for_provider("openai").len(), 2);
    }

    #[test]
    fn register_providers_across_backends() {
        let providers = build_providers();
        let configs = vec![
            make_config("gpt-4", "openai", vec!["chat"], vec![]),
            make_config("claude-3", "anthropic", vec!["chat"], vec![]),
            make_config("llama3", "ollama", vec!["chat"], vec![]),
        ];
        let registry = ModelRegistry::new(&configs, &providers).unwrap();
        assert_eq!(registry.model_count(), 3);
        assert_eq!(registry.all_providers().len(), 3);
    }

    // ─── Duplicate Detection ──────────────────────────────────────────────

    #[test]
    fn duplicate_model_id_returns_error() {
        let providers = build_providers();
        let configs = vec![
            make_config("gpt-4", "openai", vec!["chat"], vec![]),
            make_config("gpt-4", "openai", vec!["chat"], vec![]),
        ];
        let err = ModelRegistry::new(&configs, &providers).unwrap_err();
        assert!(format!("{}", err).contains("duplicate model"));
    }

    #[test]
    fn duplicate_provider_id_returns_error() {
        let providers: Vec<Box<dyn LlProvider>> = vec![
            Box::new(TestProvider::new("openai", vec![])),
            Box::new(TestProvider::new("openai", vec![])),
        ];
        let configs = vec![make_config("gpt-4", "openai", vec!["chat"], vec![])];
        let err = ModelRegistry::new(&configs, &providers).unwrap_err();
        assert!(format!("{}", err).contains("duplicate provider"));
    }

    #[test]
    fn duplicate_alias_returns_error() {
        let providers = build_providers();
        let configs = vec![
            make_config("gpt-4", "openai", vec!["chat"], vec!["fast"]),
            make_config("gpt-3.5", "openai", vec!["chat"], vec!["fast"]),
        ];
        let err = ModelRegistry::new(&configs, &providers).unwrap_err();
        assert!(format!("{}", err).contains("duplicate alias"));
    }

    #[test]
    fn unknown_provider_returns_error() {
        let providers = build_providers();
        let configs = vec![make_config("unknown", "nonexistent", vec!["chat"], vec![])];
        let err = ModelRegistry::new(&configs, &providers).unwrap_err();
        assert!(format!("{}", err).contains("unknown provider"));
    }

    // ─── Alias Resolution ─────────────────────────────────────────────────

    #[test]
    fn resolve_alias_direct_model_id() {
        let providers = build_providers();
        let configs = vec![make_config("gpt-4", "openai", vec!["chat"], vec!["fast"])];
        let registry = ModelRegistry::new(&configs, &providers).unwrap();
        assert_eq!(registry.resolve_alias("gpt-4"), Some("gpt-4"));
    }

    #[test]
    fn resolve_alias_works() {
        let providers = build_providers();
        let configs = vec![make_config("gpt-4", "openai", vec!["chat"], vec!["fast"])];
        let registry = ModelRegistry::new(&configs, &providers).unwrap();
        assert_eq!(registry.resolve_alias("fast"), Some("gpt-4"));
    }

    #[test]
    fn resolve_unknown_alias_returns_none() {
        let providers = build_providers();
        let configs = vec![make_config("gpt-4", "openai", vec!["chat"], vec![])];
        let registry = ModelRegistry::new(&configs, &providers).unwrap();
        assert!(registry.resolve_alias("nonexistent").is_none());
    }

    #[test]
    fn get_by_id_or_alias_with_id() {
        let providers = build_providers();
        let configs = vec![make_config("gpt-4", "openai", vec!["chat"], vec!["fast"])];
        let registry = ModelRegistry::new(&configs, &providers).unwrap();
        assert!(registry.get_by_id_or_alias("gpt-4").is_some());
    }

    #[test]
    fn get_by_id_or_alias_with_alias() {
        let providers = build_providers();
        let configs = vec![make_config("gpt-4", "openai", vec!["chat"], vec!["fast"])];
        let registry = ModelRegistry::new(&configs, &providers).unwrap();
        assert!(registry.get_by_id_or_alias("fast").is_some());
    }

    // ─── Capability Index ─────────────────────────────────────────────────

    #[test]
    fn find_by_capability_returns_matching_models() {
        let providers = build_providers();
        let configs = vec![
            make_config("gpt-4", "openai", vec!["chat", "planning"], vec![]),
            make_config("gpt-3.5", "openai", vec!["chat"], vec![]),
            make_config("claude-3", "anthropic", vec!["planning"], vec![]),
        ];
        let registry = ModelRegistry::new(&configs, &providers).unwrap();
        let planning = registry.find_by_capability("planning");
        assert_eq!(planning.len(), 2);
        assert!(planning.contains(&"gpt-4".to_string()));
        assert!(planning.contains(&"claude-3".to_string()));
    }

    #[test]
    fn find_by_capability_unknown_returns_empty() {
        let providers = build_providers();
        let configs = vec![make_config("gpt-4", "openai", vec!["chat"], vec![])];
        let registry = ModelRegistry::new(&configs, &providers).unwrap();
        assert!(registry.find_by_capability("vision").is_empty());
    }

    #[test]
    fn find_by_capability_maintains_registration_order() {
        let providers = build_providers();
        let configs = vec![
            make_config("gpt-4", "openai", vec!["code"], vec![]),
            make_config("claude-3", "anthropic", vec!["code"], vec![]),
            make_config("llama3", "ollama", vec!["code"], vec![]),
        ];
        let registry = ModelRegistry::new(&configs, &providers).unwrap();
        let code = registry.find_by_capability("code");
        assert_eq!(code[0], "gpt-4");
        assert_eq!(code[1], "claude-3");
        assert_eq!(code[2], "llama3");
    }

    // ─── Provider Lookups ─────────────────────────────────────────────────

    #[test]
    fn provider_for_model_returns_correct_index() {
        let providers = build_providers();
        let configs = vec![make_config("gpt-4", "openai", vec!["chat"], vec![])];
        let registry = ModelRegistry::new(&configs, &providers).unwrap();
        assert_eq!(registry.provider_for_model("gpt-4"), Some(0));
    }

    #[test]
    fn provider_for_unknown_model_returns_none() {
        let providers = build_providers();
        let configs = vec![make_config("gpt-4", "openai", vec!["chat"], vec![])];
        let registry = ModelRegistry::new(&configs, &providers).unwrap();
        assert!(registry.provider_for_model("nonexistent").is_none());
    }

    #[test]
    fn provider_id_for_model() {
        let providers = build_providers();
        let configs = vec![make_config("gpt-4", "openai", vec!["chat"], vec![])];
        let registry = ModelRegistry::new(&configs, &providers).unwrap();
        assert_eq!(registry.provider_id_for_model("gpt-4"), Some("openai"));
    }

    #[test]
    fn models_for_provider() {
        let providers = build_providers();
        let configs = vec![
            make_config("gpt-4", "openai", vec!["chat"], vec![]),
            make_config("gpt-3.5", "openai", vec!["chat"], vec![]),
        ];
        let registry = ModelRegistry::new(&configs, &providers).unwrap();
        let models = registry.models_for_provider("openai");
        assert_eq!(models.len(), 2);
    }

    #[test]
    fn models_for_unknown_provider_returns_empty() {
        let providers = build_providers();
        let configs = vec![make_config("gpt-4", "openai", vec!["chat"], vec![])];
        let registry = ModelRegistry::new(&configs, &providers).unwrap();
        assert!(registry.models_for_provider("nonexistent").is_empty());
    }

    // ─── Local Provider Detection ─────────────────────────────────────────

    #[test]
    fn local_provider_detected_via_capability() {
        let providers = build_providers();
        let configs = vec![make_config(
            "llama3",
            "ollama",
            vec!["chat", "local"],
            vec![],
        )];
        let registry = ModelRegistry::new(&configs, &providers).unwrap();
        assert!(registry.is_local_provider("ollama"));
    }

    #[test]
    fn non_local_provider_not_detected() {
        let providers = build_providers();
        let configs = vec![make_config("gpt-4", "openai", vec!["chat"], vec![])];
        let registry = ModelRegistry::new(&configs, &providers).unwrap();
        assert!(!registry.is_local_provider("openai"));
    }

    // ─── Metadata Queries ─────────────────────────────────────────────────

    #[test]
    fn context_window_lookup() {
        let providers = build_providers();
        let configs = vec![make_config("gpt-4", "openai", vec!["chat"], vec![])];
        let registry = ModelRegistry::new(&configs, &providers).unwrap();
        assert_eq!(registry.context_window("gpt-4"), Some(8192));
    }

    #[test]
    fn context_window_unknown_returns_none() {
        let providers = build_providers();
        let configs = vec![make_config("gpt-4", "openai", vec!["chat"], vec![])];
        let registry = ModelRegistry::new(&configs, &providers).unwrap();
        assert!(registry.context_window("nonexistent").is_none());
    }

    #[test]
    fn max_tokens_lookup() {
        let providers = build_providers();
        let configs = vec![make_config("gpt-4", "openai", vec!["chat"], vec![])];
        let registry = ModelRegistry::new(&configs, &providers).unwrap();
        assert_eq!(registry.max_tokens("gpt-4"), Some(4096));
    }

    #[test]
    fn cost_rates_lookup() {
        let providers = build_providers();
        let configs = vec![make_config("gpt-4", "openai", vec!["chat"], vec![])];
        let registry = ModelRegistry::new(&configs, &providers).unwrap();
        let rates = registry.cost_rates("gpt-4");
        assert_eq!(rates, Some((1.0, 2.0)));
    }

    #[test]
    fn cost_rates_unknown_returns_none() {
        let providers = build_providers();
        let configs = vec![make_config("gpt-4", "openai", vec!["chat"], vec![])];
        let registry = ModelRegistry::new(&configs, &providers).unwrap();
        assert!(registry.cost_rates("nonexistent").is_none());
    }

    #[test]
    fn estimate_cost_calculation() {
        let providers = build_providers();
        let configs = vec![make_config("gpt-4", "openai", vec!["chat"], vec![])];
        let registry = ModelRegistry::new(&configs, &providers).unwrap();
        // 1000 input tokens * $1.0/1k = $1.0, 500 output * $2.0/1k = $1.0 → total $2.0
        let cost = registry.estimate_cost("gpt-4", 1000, 500);
        assert!((cost - 2.0).abs() < 0.001);
    }

    #[test]
    fn capabilities_for_model() {
        let providers = build_providers();
        let configs = vec![make_config(
            "gpt-4",
            "openai",
            vec!["chat", "planning"],
            vec![],
        )];
        let registry = ModelRegistry::new(&configs, &providers).unwrap();
        let caps = registry.capabilities_for_model("gpt-4").unwrap();
        assert!(caps.contains(&"chat".to_string()));
        assert!(caps.contains(&"planning".to_string()));
    }

    #[test]
    fn model_config_via_get() {
        let providers = build_providers();
        let configs = vec![make_config("gpt-4", "openai", vec!["chat"], vec![])];
        let registry = ModelRegistry::new(&configs, &providers).unwrap();
        let mc = registry.get("gpt-4").unwrap();
        assert_eq!(mc.id, "gpt-4");
        assert_eq!(mc.provider, "openai");
    }

    // ─── Enumeration ──────────────────────────────────────────────────────

    #[test]
    fn all_models_returns_all() {
        let providers = build_providers();
        let configs = vec![
            make_config("gpt-4", "openai", vec!["chat"], vec![]),
            make_config("claude-3", "anthropic", vec!["chat"], vec![]),
        ];
        let registry = ModelRegistry::new(&configs, &providers).unwrap();
        let all = registry.all_models();
        assert_eq!(all.len(), 2);
        assert!(all.contains(&"gpt-4".to_string()));
    }

    #[test]
    fn all_providers_returns_all() {
        let providers = build_providers();
        let configs = vec![make_config("gpt-4", "openai", vec!["chat"], vec![])];
        let registry = ModelRegistry::new(&configs, &providers).unwrap();
        let all = registry.all_providers();
        assert_eq!(all.len(), 3);
    }

    // ─── Send + Sync ──────────────────────────────────────────────────────

    #[test]
    fn registry_is_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<ModelRegistry>();
        assert_sync::<ModelRegistry>();
    }

    // ─── Empty Config ──────────────────────────────────────────────────────

    #[test]
    fn empty_models_is_valid() {
        let providers = build_providers();
        let registry = ModelRegistry::new(&[], &providers).unwrap();
        assert_eq!(registry.model_count(), 0);
        assert_eq!(registry.all_models().len(), 0);
    }

    #[test]
    fn no_providers_rejected() {
        let configs = vec![make_config("gpt-4", "openai", vec!["chat"], vec![])];
        let err = ModelRegistry::new(&configs, &[]).unwrap_err();
        assert!(format!("{}", err).contains("unknown provider"));
    }
}
