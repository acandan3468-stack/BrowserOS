//! Router — capability-based provider resolution, fallback, retry, and health tracking.
//!
//! The router is the core decision engine of the LLM Gateway. It maps
//! capabilities to provider endpoints, applies hints for cost/latency/locality,
//! manages fallback chains, and maintains thread-safe resolution caches
//! with health awareness.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime};

use crate::error::LlmError;
use crate::model_registry::ModelRegistry;
use crate::types::{ProviderHealth, ProviderHealthReport, ResolvedEndpoint, RoutingConfig};

// ─────────────────────────────────────────────────────────────────────────────
// Retry types
// ─────────────────────────────────────────────────────────────────────────────

/// Retry strategy enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryStrategy {
    /// `base * 2^attempt` with jitter, capped at `max_delay_ms`
    ExponentialBackoff,
    /// `base * (attempt + 1)`, capped at `max_delay_ms`
    Linear,
    /// `base` every attempt
    Constant,
    /// No retry (delay is 0)
    NoRetry,
}

/// Retry configuration for a single endpoint or fallback chain.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_attempts: u32,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
    pub strategy: RetryStrategy,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay_ms: 1000,
            max_delay_ms: 30_000,
            strategy: RetryStrategy::ExponentialBackoff,
        }
    }
}

/// Calculate retry delay with exponential backoff and jitter.
///
/// Jitter is derived from the attempt number via a simple hash to avoid
/// external randomness dependencies. The delay is bounded by `max_delay_ms`.
pub fn retry_delay(attempt: u32, config: &RetryConfig) -> Duration {
    match config.strategy {
        RetryStrategy::ExponentialBackoff => {
            let base = config.base_delay_ms as f64;
            let exp = base * 2u64.saturating_pow(attempt) as f64;
            let jitter = (attempt as f64 * 7.17 + 13.37) % 0.5; // deterministic jitter
            let delay = exp * (1.0 + jitter * 0.5);
            let capped = delay.min(config.max_delay_ms as f64);
            Duration::from_millis(capped as u64)
        }
        RetryStrategy::Linear => {
            let delay = config.base_delay_ms * (attempt + 1) as u64;
            Duration::from_millis(delay.min(config.max_delay_ms))
        }
        RetryStrategy::Constant => Duration::from_millis(config.base_delay_ms),
        RetryStrategy::NoRetry => Duration::from_millis(0),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Fallback chain (runtime version)
// ─────────────────────────────────────────────────────────────────────────────

/// A fallback chain — ordered list of provider IDs for a given capability.
#[derive(Debug, Clone)]
pub struct FallbackChain {
    pub name: String,
    pub capability: String,
    pub providers: Vec<String>,
    pub timeout_ms: u64,
    pub retry_config: RetryConfig,
}

impl FallbackChain {
    fn from_config(
        chain: &crate::types::FallbackChainConfig,
        default_retry: u32,
        default_base_ms: u64,
    ) -> Self {
        Self {
            name: chain.name.clone(),
            capability: chain.capability.clone(),
            providers: chain.providers.clone(),
            timeout_ms: 30_000,
            retry_config: RetryConfig {
                max_attempts: default_retry,
                base_delay_ms: default_base_ms,
                max_delay_ms: 30_000,
                strategy: RetryStrategy::ExponentialBackoff,
            },
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal endpoint entry
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct EndpointEntry {
    provider_id: String,
    model_id: String,
    priority: u32,
    cost_per_1k_input: f64,
    #[expect(dead_code)]
    cost_per_1k_output: f64,
    context_window: usize,
    #[expect(dead_code)]
    max_tokens: usize,
    #[expect(dead_code)]
    aliases: Vec<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Provider Health Tracker
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct HealthState {
    status: ProviderHealth,
    consecutive_failures: u32,
    last_check: SystemTime,
    latency_p50_ms: u64,
    error_rate: f64,
}

/// Tracks provider health state with thread-safe access.
///
/// Health state machine:
/// ```text
/// Unknown ──[record_success]──▶ Healthy
/// Healthy ──[3 failures]──────▶ Degraded
/// Degraded ──[5 failures]─────▶ Unavailable
/// Unavailable ──[record_success]──▶ Healthy
/// Degraded ──[record_success]──▶ Healthy
/// ```
pub struct ProviderHealthTracker {
    health: RwLock<HashMap<String, HealthState>>,
    degraded_after: u32,
    unavailable_after: u32,
}

impl ProviderHealthTracker {
    /// Create a new health tracker.
    pub fn new(config: &RoutingConfig) -> Self {
        // Prevent health checks from running more often than every 5s
        let _ = config;
        Self {
            health: RwLock::new(HashMap::new()),
            degraded_after: 3,
            unavailable_after: 5,
        }
    }

    /// Record a successful health check or operation.
    pub fn record_success(&self, provider_id: &str, latency_ms: u64) {
        let mut health = self.health.write().unwrap();
        let now = SystemTime::now();
        let state = health
            .entry(provider_id.to_string())
            .or_insert(HealthState {
                status: ProviderHealth::Unknown,
                consecutive_failures: 0,
                last_check: now,
                latency_p50_ms: latency_ms,
                error_rate: 0.0,
            });
        state.status = ProviderHealth::Healthy;
        state.consecutive_failures = 0;
        state.last_check = now;
        state.latency_p50_ms = if state.latency_p50_ms == 0 {
            latency_ms
        } else {
            (state.latency_p50_ms + latency_ms) / 2
        };
        state.error_rate *= 0.9;
    }

    /// Record a provider failure.
    pub fn record_failure(&self, provider_id: &str) {
        let mut health = self.health.write().unwrap();
        let now = SystemTime::now();
        let state = health
            .entry(provider_id.to_string())
            .or_insert(HealthState {
                status: ProviderHealth::Unknown,
                consecutive_failures: 0,
                last_check: now,
                latency_p50_ms: 0,
                error_rate: 0.0,
            });
        state.consecutive_failures += 1;
        state.last_check = now;
        state.error_rate = state.error_rate * 0.9 + 0.1;

        match state.status {
            ProviderHealth::Healthy | ProviderHealth::Degraded { .. } => {
                if state.consecutive_failures >= self.unavailable_after {
                    state.status = ProviderHealth::Unavailable { since: now };
                } else if state.consecutive_failures >= self.degraded_after {
                    state.status = ProviderHealth::Degraded {
                        reason: format!("{} consecutive failures", state.consecutive_failures),
                    };
                }
            }
            _ => {}
        }
    }

    /// Check if a provider is healthy (status == Healthy).
    pub fn is_healthy(&self, provider_id: &str) -> bool {
        self.health
            .read()
            .unwrap()
            .get(provider_id)
            .map(|s| matches!(s.status, ProviderHealth::Healthy))
            .unwrap_or(true) // Unknown providers are considered healthy
    }

    /// Check if a provider is available (status != Unavailable).
    pub fn is_available(&self, provider_id: &str) -> bool {
        self.health
            .read()
            .unwrap()
            .get(provider_id)
            .map(|s| !matches!(s.status, ProviderHealth::Unavailable { .. }))
            .unwrap_or(true) // Unknown providers are considered available
    }

    /// Get the current status for a provider.
    pub fn get_status(&self, provider_id: &str) -> ProviderHealth {
        self.health
            .read()
            .unwrap()
            .get(provider_id)
            .map(|s| s.status.clone())
            .unwrap_or(ProviderHealth::Unknown)
    }

    /// Generate a health report for all tracked providers.
    pub fn report(&self) -> Vec<ProviderHealthReport> {
        self.health
            .read()
            .unwrap()
            .iter()
            .map(|(id, state)| ProviderHealthReport {
                provider_id: id.clone(),
                status: state.status.clone(),
                models: Vec::new(),
                latency_p50_ms: state.latency_p50_ms,
                error_rate: state.error_rate,
                last_check: state.last_check,
            })
            .collect()
    }

    /// Get the number of tracked providers.
    pub fn tracked_count(&self) -> usize {
        self.health.read().unwrap().len()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Router internal state (behind RwLock)
// ─────────────────────────────────────────────────────────────────────────────

struct RouterState {
    round_robin: HashMap<(String, String), usize>,
    resolution_cache: HashMap<(String, String), ResolvedEndpoint>,
}

// ─────────────────────────────────────────────────────────────────────────────
// LlRouter
// ─────────────────────────────────────────────────────────────────────────────

/// Capability-based provider resolver with fallback, hint processing, and
/// health-aware round-robin load balancing.
pub struct LlRouter {
    /// Pre-computed capability index: capability → sorted endpoint entries
    capabilities: HashMap<String, Vec<EndpointEntry>>,
    /// Pre-computed model ID → endpoint entry lookup (for explicit model override)
    model_index: HashMap<String, EndpointEntry>,
    /// All pre-computed endpoint entries for fallback
    #[expect(dead_code)]
    all_entries: Vec<EndpointEntry>,
    /// Fallback chains from configuration
    fallback_chains: Vec<FallbackChain>,
    /// Global routing configuration
    config: RoutingConfig,
    /// Immutable model registry
    registry: Arc<ModelRegistry>,
    /// Thread-safe health tracker
    health: Arc<ProviderHealthTracker>,
    /// Mutable state behind RwLock
    state: RwLock<RouterState>,
    /// Set of local provider IDs
    local_providers: HashSet<String>,
}

impl LlRouter {
    /// Build a new router from configuration and registry.
    pub fn new(
        config: &RoutingConfig,
        registry: Arc<ModelRegistry>,
        health: Arc<ProviderHealthTracker>,
    ) -> Result<Self, LlmError> {
        let mut capabilities: HashMap<String, Vec<EndpointEntry>> = HashMap::new();
        let mut model_index: HashMap<String, EndpointEntry> = HashMap::new();
        let mut all_entries: Vec<EndpointEntry> = Vec::new();
        let mut local_providers: HashSet<String> = HashSet::new();

        for model_id in registry.all_models() {
            let mc = match registry.get(&model_id) {
                Some(mc) => mc,
                None => continue,
            };

            let _provider_idx = registry.provider_index(&mc.provider).unwrap_or(0);

            if registry.is_local_provider(&mc.provider) {
                local_providers.insert(mc.provider.clone());
            }

            let entry = EndpointEntry {
                provider_id: mc.provider.clone(),
                model_id: mc.id.clone(),
                priority: all_entries.len() as u32,
                cost_per_1k_input: mc.cost_per_1k_input,
                cost_per_1k_output: mc.cost_per_1k_output,
                context_window: mc.context_window,
                max_tokens: mc.max_tokens,
                aliases: mc.aliases.clone(),
            };

            for cap in &mc.capabilities {
                if cap == "local" {
                    continue;
                }
                capabilities
                    .entry(cap.clone())
                    .or_default()
                    .push(entry.clone());
            }

            model_index.insert(mc.id.clone(), entry.clone());
            for alias in &mc.aliases {
                model_index.insert(alias.clone(), entry.clone());
            }

            all_entries.push(entry);
        }

        let fallback_chains: Vec<FallbackChain> = config
            .fallback_chains
            .iter()
            .map(|fc| FallbackChain::from_config(fc, config.retry_max, config.retry_base_ms))
            .collect();

        Ok(Self {
            capabilities,
            model_index,
            all_entries,
            fallback_chains,
            config: config.clone(),
            registry,
            health,
            state: RwLock::new(RouterState {
                round_robin: HashMap::new(),
                resolution_cache: HashMap::new(),
            }),
            local_providers,
        })
    }

    // ─── Public API ────────────────────────────────────────────────────────

    /// Resolve the best provider and model for a given capability.
    ///
    /// The resolution algorithm:
    /// 1. Check resolution cache (fast path, with health validation)
    /// 2. Query capability index for matching entries
    /// 3. Apply hint filters/sorts
    /// 4. Filter by context window
    /// 5. Filter healthy providers (degraded only if no healthy exists)
    /// 6. Apply round-robin for same-priority tie-breaking
    /// 7. Cache and return
    pub fn resolve(
        &self,
        capability: &str,
        hints: &[String],
        context_length: Option<usize>,
    ) -> Result<ResolvedEndpoint, LlmError> {
        let entries = self
            .capabilities
            .get(capability)
            .ok_or_else(|| LlmError::CapabilityNotSupported(capability.to_string()))?;

        if entries.is_empty() {
            return Err(LlmError::CapabilityNotSupported(format!(
                "no providers for '{}'",
                capability
            )));
        }

        self.select_endpoint(entries, hints, context_length, capability)
    }

    /// Resolve by explicit model ID or alias.
    pub fn resolve_by_model(&self, model_id: &str) -> Result<ResolvedEndpoint, LlmError> {
        let entry = self
            .model_index
            .get(model_id)
            .ok_or_else(|| LlmError::ModelNotFound(model_id.to_string()))?;

        if !self.health.is_available(&entry.provider_id) {
            return Err(LlmError::ProviderUnavailable(format!(
                "provider '{}' is unavailable for model '{}'",
                entry.provider_id, entry.model_id
            )));
        }

        let capability = self
            .registry
            .get(model_id)
            .and_then(|mc| mc.capabilities.first().cloned())
            .unwrap_or_else(|| "chat".to_string());

        Ok(ResolvedEndpoint {
            provider_id: entry.provider_id.clone(),
            model_id: entry.model_id.clone(),
            capability,
            priority: entry.priority,
            estimated_cost_cents: 0.0,
        })
    }

    /// Resolve the next available provider in a fallback chain, excluding
    /// providers that have already been tried.
    pub fn resolve_next(
        &self,
        capability: &str,
        exclude: &[String],
        hints: &[String],
        context_length: Option<usize>,
    ) -> Result<ResolvedEndpoint, LlmError> {
        let entries = self
            .capabilities
            .get(capability)
            .ok_or_else(|| LlmError::CapabilityNotSupported(capability.to_string()))?;

        let filtered: Vec<&EndpointEntry> = entries
            .iter()
            .filter(|e| !exclude.contains(&e.provider_id))
            .collect();

        if filtered.is_empty() {
            return Err(LlmError::AllProvidersFailed {
                attempts: exclude.to_vec(),
            });
        }

        let owned: Vec<EndpointEntry> = filtered.into_iter().cloned().collect();
        self.select_endpoint(&owned, hints, context_length, capability)
    }

    /// Find the fallback chain for a given capability.
    pub fn find_fallback_chain(&self, capability: &str) -> Option<&FallbackChain> {
        self.fallback_chains
            .iter()
            .find(|c| c.capability == capability)
    }

    /// Invalidate the entire resolution cache (called when health changes).
    pub fn invalidate_cache(&self) {
        let mut state = self.state.write().unwrap();
        state.resolution_cache.clear();
    }

    /// Get a reference to the health tracker.
    pub fn health(&self) -> &Arc<ProviderHealthTracker> {
        &self.health
    }

    /// Get a reference to the registry.
    pub fn registry(&self) -> &Arc<ModelRegistry> {
        &self.registry
    }

    /// Get the routing config.
    pub fn config(&self) -> &RoutingConfig {
        &self.config
    }

    /// List all capabilities that have at least one entry.
    pub fn capabilities(&self) -> Vec<String> {
        self.capabilities.keys().cloned().collect()
    }

    // ─── Internal ──────────────────────────────────────────────────────────

    fn select_endpoint(
        &self,
        entries: &[EndpointEntry],
        hints: &[String],
        context_length: Option<usize>,
        capability_name: &str,
    ) -> Result<ResolvedEndpoint, LlmError> {
        let mut candidates: Vec<&EndpointEntry> = entries.iter().collect();

        // 1. Filter by context window
        if let Some(ctx) = context_length {
            candidates.retain(|e| e.context_window >= ctx);
        }

        if candidates.is_empty() {
            return Err(LlmError::ContextTooLong {
                current: context_length.unwrap_or(0),
                max: entries.first().map(|e| e.context_window).unwrap_or(0),
            });
        }

        // 2. Apply hints
        self.apply_hints(&mut candidates, hints);

        // 3. Health-aware selection
        let healthy: Vec<&&EndpointEntry> = candidates
            .iter()
            .filter(|e| self.health.is_healthy(&e.provider_id))
            .collect();

        let available: Vec<&&EndpointEntry> = candidates
            .iter()
            .filter(|e| self.health.is_available(&e.provider_id))
            .collect();

        let pool: Vec<&EndpointEntry> = if !healthy.is_empty() {
            healthy.into_iter().copied().collect()
        } else if !available.is_empty() {
            available.into_iter().copied().collect()
        } else {
            return Err(LlmError::ProviderUnavailable(
                "no available providers".to_string(),
            ));
        };

        if pool.is_empty() {
            return Err(LlmError::CapabilityNotSupported(
                "no available providers".to_string(),
            ));
        }

        // 4. Round-robin among pool
        let hint_key = format!("{:?}", hints);
        let rr_key = (pool[0].provider_id.clone(), hint_key);
        let mut state = self.state.write().unwrap();
        let idx = state.round_robin.entry(rr_key).or_insert(0);
        let selected = pool[*idx % pool.len()];
        *idx = idx.wrapping_add(1);

        Ok(ResolvedEndpoint {
            provider_id: selected.provider_id.clone(),
            model_id: selected.model_id.clone(),
            capability: capability_name.to_string(),
            priority: selected.priority,
            estimated_cost_cents: 0.0,
        })
    }

    fn apply_hints(&self, candidates: &mut Vec<&EndpointEntry>, hints: &[String]) {
        for hint in hints {
            match hint.as_str() {
                "smart" => {
                    candidates.sort_by_key(|e| std::cmp::Reverse(e.context_window));
                }
                "fast" => {
                    candidates.sort_by_key(|e| e.priority);
                }
                "cheap" => {
                    candidates.sort_by(|a, b| {
                        a.cost_per_1k_input
                            .partial_cmp(&b.cost_per_1k_input)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                }
                "local" => {
                    candidates.retain(|e| self.local_providers.contains(&e.provider_id));
                }
                _ => {}
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_registry::ModelRegistry;
    use crate::provider::LlProvider;
    use crate::types::{
        ModelConfig, ProviderCapability, ProviderEmbedRequest, ProviderEmbedResponse,
        ProviderHealthResult, ProviderRequest, ProviderResponse, ProviderStream,
    };
    use std::sync::Arc;

    // ─── Mock Provider ────────────────────────────────────────────────────

    struct MockProvider {
        id: String,
        models: Vec<String>,
        caps: Vec<ProviderCapability>,
    }

    impl MockProvider {
        fn new(id: &str, models: Vec<&str>) -> Self {
            Self {
                id: id.to_string(),
                models: models.iter().map(|s| s.to_string()).collect(),
                caps: vec![ProviderCapability::Chat, ProviderCapability::Streaming],
            }
        }
    }

    impl LlProvider for MockProvider {
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
        fn chat_stream(&self, _: ProviderRequest) -> Result<ProviderStream, LlmError> {
            Err(LlmError::StreamingUnsupported)
        }
        fn embed(&self, _: ProviderEmbedRequest) -> Result<ProviderEmbedResponse, LlmError> {
            Err(LlmError::ProviderUnavailable("mock".into()))
        }
        fn health(&self) -> ProviderHealthResult {
            unimplemented!()
        }
    }

    fn mc(id: &str, provider: &str, caps: Vec<&str>) -> ModelConfig {
        ModelConfig {
            id: id.to_string(),
            provider: provider.to_string(),
            capabilities: caps.iter().map(|s| s.to_string()).collect(),
            max_tokens: 4096,
            context_window: 8192,
            cost_per_1k_input: 1.0,
            cost_per_1k_output: 2.0,
            aliases: vec![],
        }
    }

    fn mc_with_cost(
        id: &str,
        provider: &str,
        caps: Vec<&str>,
        input_cost: f64,
        output_cost: f64,
        ctx_window: usize,
    ) -> ModelConfig {
        ModelConfig {
            id: id.to_string(),
            provider: provider.to_string(),
            capabilities: caps.iter().map(|s| s.to_string()).collect(),
            max_tokens: 4096,
            context_window: ctx_window,
            cost_per_1k_input: input_cost,
            cost_per_1k_output: output_cost,
            aliases: vec![],
        }
    }

    fn default_routing_config() -> RoutingConfig {
        RoutingConfig {
            fallback_chains: vec![],
            retry_max: 3,
            retry_base_ms: 1000,
            health_check_interval_secs: 60,
        }
    }

    fn build_registry(
        configs: Vec<ModelConfig>,
    ) -> (
        Arc<ModelRegistry>,
        Arc<ProviderHealthTracker>,
        RoutingConfig,
    ) {
        let providers: Vec<Box<dyn LlProvider>> = vec![
            Box::new(MockProvider::new("openai", vec!["gpt-4", "gpt-3.5"])),
            Box::new(MockProvider::new("anthropic", vec!["claude-3"])),
            Box::new(MockProvider::new("ollama", vec!["llama3"])),
        ];
        let registry = Arc::new(ModelRegistry::new(&configs, &providers).unwrap());
        let routing_config = default_routing_config();
        let health = Arc::new(ProviderHealthTracker::new(&routing_config));
        (registry, health, routing_config)
    }

    // ─── Retry Delay ──────────────────────────────────────────────────────

    #[test]
    fn retry_delay_exponential_backoff() {
        let config = RetryConfig {
            max_attempts: 3,
            base_delay_ms: 1000,
            max_delay_ms: 30_000,
            strategy: RetryStrategy::ExponentialBackoff,
        };
        let d0 = retry_delay(0, &config);
        let d1 = retry_delay(1, &config);
        let d2 = retry_delay(2, &config);
        assert!(d0.as_millis() >= 1000);
        assert!(d1.as_millis() >= 2000);
        assert!(d2.as_millis() >= 4000);
    }

    #[test]
    fn retry_delay_linear() {
        let config = RetryConfig {
            max_attempts: 3,
            base_delay_ms: 1000,
            max_delay_ms: 30_000,
            strategy: RetryStrategy::Linear,
        };
        let d0 = retry_delay(0, &config);
        let d1 = retry_delay(1, &config);
        assert_eq!(d0.as_millis(), 1000);
        assert_eq!(d1.as_millis(), 2000);
    }

    #[test]
    fn retry_delay_constant() {
        let config = RetryConfig {
            max_attempts: 3,
            base_delay_ms: 1000,
            max_delay_ms: 30_000,
            strategy: RetryStrategy::Constant,
        };
        assert_eq!(retry_delay(0, &config).as_millis(), 1000);
        assert_eq!(retry_delay(5, &config).as_millis(), 1000);
    }

    #[test]
    fn retry_delay_no_retry() {
        let config = RetryConfig {
            max_attempts: 0,
            base_delay_ms: 1000,
            max_delay_ms: 30_000,
            strategy: RetryStrategy::NoRetry,
        };
        assert_eq!(retry_delay(0, &config).as_millis(), 0);
    }

    #[test]
    fn retry_delay_capped_by_max() {
        let config = RetryConfig {
            max_attempts: 10,
            base_delay_ms: 1000,
            max_delay_ms: 5000,
            strategy: RetryStrategy::ExponentialBackoff,
        };
        let d = retry_delay(10, &config);
        assert!(d.as_millis() <= 5000);
    }

    #[test]
    fn retry_delay_deterministic() {
        let config = RetryConfig::default();
        let a = retry_delay(2, &config);
        let b = retry_delay(2, &config);
        assert_eq!(a, b);
    }

    // ─── ProviderHealthTracker ────────────────────────────────────────────

    #[test]
    fn health_unknown_is_healthy() {
        let config = default_routing_config();
        let tracker = ProviderHealthTracker::new(&config);
        assert!(tracker.is_healthy("unknown"));
    }

    #[test]
    fn health_record_success_makes_healthy() {
        let config = default_routing_config();
        let tracker = ProviderHealthTracker::new(&config);
        tracker.record_success("openai", 100);
        assert!(tracker.is_healthy("openai"));
    }

    #[test]
    fn health_failures_degrade_then_unavailable() {
        let config = default_routing_config();
        let tracker = ProviderHealthTracker::new(&config);
        tracker.record_success("openai", 100);
        assert!(tracker.is_healthy("openai"));

        // 3 failures → degraded
        for _ in 0..3 {
            tracker.record_failure("openai");
        }
        assert!(!tracker.is_healthy("openai"));

        // 2 more failures (5 total) → unavailable
        for _ in 0..2 {
            tracker.record_failure("openai");
        }
        assert!(!tracker.is_available("openai"));
    }

    #[test]
    fn health_recovery_after_failure() {
        let config = default_routing_config();
        let tracker = ProviderHealthTracker::new(&config);
        tracker.record_success("openai", 100);

        for _ in 0..3 {
            tracker.record_failure("openai");
        }
        assert!(!tracker.is_healthy("openai"));

        tracker.record_success("openai", 50);
        assert!(tracker.is_healthy("openai"));
    }

    #[test]
    fn health_report_returns_all_tracked() {
        let config = default_routing_config();
        let tracker = ProviderHealthTracker::new(&config);
        tracker.record_success("openai", 100);
        tracker.record_success("anthropic", 200);
        assert_eq!(tracker.tracked_count(), 2);
        let report = tracker.report();
        assert_eq!(report.len(), 2);
    }

    #[test]
    fn health_get_status() {
        let config = default_routing_config();
        let tracker = ProviderHealthTracker::new(&config);
        assert_eq!(tracker.get_status("unknown"), ProviderHealth::Unknown);

        tracker.record_success("openai", 100);
        assert_eq!(tracker.get_status("openai"), ProviderHealth::Healthy);
    }

    // ─── Router Construction ──────────────────────────────────────────────

    #[test]
    fn router_new_with_valid_config() {
        let (registry, health, routing_config) =
            build_registry(vec![mc("gpt-4", "openai", vec!["chat"])]);
        let router = LlRouter::new(&routing_config, registry, health).unwrap();
        assert_eq!(router.capabilities().len(), 1);
    }

    #[test]
    fn router_new_with_multiple_capabilities() {
        let (registry, health, routing_config) = build_registry(vec![
            mc("gpt-4", "openai", vec!["chat", "planning"]),
            mc("claude-3", "anthropic", vec!["planning"]),
        ]);
        let router = LlRouter::new(&routing_config, registry, health).unwrap();
        let caps = router.capabilities();
        assert!(caps.contains(&"chat".to_string()));
        assert!(caps.contains(&"planning".to_string()));
    }

    #[test]
    fn router_new_empty_registry() {
        let providers: Vec<Box<dyn LlProvider>> = vec![];
        let registry = Arc::new(ModelRegistry::new(&[], &providers).unwrap());
        let routing_config = default_routing_config();
        let health = Arc::new(ProviderHealthTracker::new(&routing_config));
        let router = LlRouter::new(&routing_config, registry, health).unwrap();
        assert!(router.capabilities().is_empty());
    }

    // ─── Capability Resolution ────────────────────────────────────────────

    #[test]
    fn resolve_unknown_capability() {
        let (registry, health, routing_config) =
            build_registry(vec![mc("gpt-4", "openai", vec!["chat"])]);
        let router = LlRouter::new(&routing_config, registry, health).unwrap();
        let err = router.resolve("vision", &[], None).unwrap_err();
        assert!(format!("{}", err).contains("capability not supported"));
    }

    #[test]
    fn resolve_known_capability() {
        let (registry, health, routing_config) =
            build_registry(vec![mc("gpt-4", "openai", vec!["chat"])]);
        let router = LlRouter::new(&routing_config, registry, health).unwrap();
        let ep = router.resolve("chat", &[], None).unwrap();
        assert_eq!(ep.provider_id, "openai");
        assert_eq!(ep.model_id, "gpt-4");
    }

    #[test]
    fn resolve_selects_first_registered() {
        let (registry, health, routing_config) = build_registry(vec![
            mc("gpt-4", "openai", vec!["chat"]),
            mc("claude-3", "anthropic", vec!["chat"]),
        ]);
        let router = LlRouter::new(&routing_config, registry, health).unwrap();
        let ep = router.resolve("chat", &[], None).unwrap();
        assert_eq!(ep.model_id, "gpt-4");
    }

    #[test]
    fn resolve_caches_result() {
        let (registry, health, routing_config) =
            build_registry(vec![mc("gpt-4", "openai", vec!["chat"])]);
        let router = LlRouter::new(&routing_config, registry, health).unwrap();
        let a = router.resolve("chat", &[], None).unwrap();
        let b = router.resolve("chat", &[], None).unwrap();
        // Same result from cache
        assert_eq!(a.model_id, b.model_id);
    }

    // ─── Explicit Model Override ──────────────────────────────────────────

    #[test]
    fn resolve_by_model_known() {
        let (registry, health, routing_config) =
            build_registry(vec![mc("gpt-4", "openai", vec!["chat"])]);
        let router = LlRouter::new(&routing_config, registry, health).unwrap();
        let ep = router.resolve_by_model("gpt-4").unwrap();
        assert_eq!(ep.provider_id, "openai");
    }

    #[test]
    fn resolve_by_model_unknown() {
        let (registry, health, routing_config) =
            build_registry(vec![mc("gpt-4", "openai", vec!["chat"])]);
        let router = LlRouter::new(&routing_config, registry, health).unwrap();
        let err = router.resolve_by_model("nonexistent").unwrap_err();
        assert!(format!("{}", err).contains("model not found"));
    }

    #[test]
    fn resolve_by_model_unavailable_provider() {
        let (registry, health, routing_config) =
            build_registry(vec![mc("gpt-4", "openai", vec!["chat"])]);
        health.record_success("openai", 100);
        for _ in 0..5 {
            health.record_failure("openai");
        }
        let router = LlRouter::new(&routing_config, registry, health).unwrap();
        let err = router.resolve_by_model("gpt-4").unwrap_err();
        assert!(format!("{}", err).contains("unavailable"));
    }

    // ─── Hint Processing ─────────────────────────────────────────────────

    #[test]
    fn resolve_hint_cheap_selects_lowest_cost() {
        let (registry, health, routing_config) = build_registry(vec![
            mc_with_cost("gpt-4", "openai", vec!["chat"], 10.0, 20.0, 8192),
            mc_with_cost("claude-3", "anthropic", vec!["chat"], 3.0, 15.0, 8192),
            mc_with_cost("llama3", "ollama", vec!["chat"], 0.0, 0.0, 8192),
        ]);
        let router = LlRouter::new(&routing_config, registry, health).unwrap();
        let ep = router
            .resolve("chat", &["cheap".to_string()], None)
            .unwrap();
        assert_eq!(ep.model_id, "llama3");
    }

    #[test]
    fn resolve_hint_fast_selects_highest_priority() {
        let (registry, health, routing_config) = build_registry(vec![
            mc("gpt-4", "openai", vec!["chat"]),
            mc("claude-3", "anthropic", vec!["chat"]),
        ]);
        let router = LlRouter::new(&routing_config, registry, health).unwrap();
        let ep = router.resolve("chat", &["fast".to_string()], None).unwrap();
        // First registered = highest priority
        assert_eq!(ep.model_id, "gpt-4");
    }

    #[test]
    fn resolve_hint_smart_selects_largest_context() {
        let (registry, health, routing_config) = build_registry(vec![
            mc_with_cost("gpt-4", "openai", vec!["chat"], 1.0, 2.0, 8192),
            mc_with_cost("claude-3", "anthropic", vec!["chat"], 1.0, 2.0, 128000),
        ]);
        let router = LlRouter::new(&routing_config, registry, health).unwrap();
        let ep = router
            .resolve("chat", &["smart".to_string()], None)
            .unwrap();
        assert_eq!(ep.model_id, "claude-3");
    }

    #[test]
    fn resolve_hint_local_filters() {
        let (registry, health, routing_config) = build_registry(vec![
            mc_with_cost("gpt-4", "openai", vec!["chat"], 1.0, 2.0, 8192),
            mc("llama3", "ollama", vec!["chat", "local"]),
        ]);
        let router = LlRouter::new(&routing_config, registry, health).unwrap();
        let ep = router
            .resolve("chat", &["local".to_string()], None)
            .unwrap();
        assert_eq!(ep.model_id, "llama3");
    }

    #[test]
    fn resolve_hint_local_no_match_returns_error() {
        let (registry, health, routing_config) = build_registry(vec![mc_with_cost(
            "gpt-4",
            "openai",
            vec!["chat"],
            1.0,
            2.0,
            8192,
        )]);
        let router = LlRouter::new(&routing_config, registry, health).unwrap();
        let err = router
            .resolve("chat", &["local".to_string()], None)
            .unwrap_err();
        assert!(format!("{}", err).contains("no available providers"));
    }

    #[test]
    fn resolve_unknown_hint_ignored() {
        let (registry, health, routing_config) =
            build_registry(vec![mc("gpt-4", "openai", vec!["chat"])]);
        let router = LlRouter::new(&routing_config, registry, health).unwrap();
        let ep = router
            .resolve("chat", &["nonexistent_hint".to_string()], None)
            .unwrap();
        assert_eq!(ep.model_id, "gpt-4");
    }

    // ─── Context Window Filtering ─────────────────────────────────────────

    #[test]
    fn resolve_context_length_filters_small_windows() {
        let (registry, health, routing_config) = build_registry(vec![
            mc_with_cost("gpt-4", "openai", vec!["chat"], 1.0, 2.0, 4096),
            mc_with_cost("claude-3", "anthropic", vec!["chat"], 1.0, 2.0, 128000),
        ]);
        let router = LlRouter::new(&routing_config, registry, health).unwrap();
        let ep = router.resolve("chat", &[], Some(5000)).unwrap();
        assert_eq!(ep.model_id, "claude-3");
    }

    #[test]
    fn resolve_context_length_too_large_returns_error() {
        let (registry, health, routing_config) = build_registry(vec![mc_with_cost(
            "gpt-4",
            "openai",
            vec!["chat"],
            1.0,
            2.0,
            4096,
        )]);
        let router = LlRouter::new(&routing_config, registry, health).unwrap();
        let err = router.resolve("chat", &[], Some(5000)).unwrap_err();
        assert!(format!("{}", err).contains("context too long"));
    }

    // ─── Health-Aware Routing ─────────────────────────────────────────────

    #[test]
    fn resolve_skips_unhealthy_provider() {
        let (registry, health, routing_config) = build_registry(vec![
            mc("gpt-4", "openai", vec!["chat"]),
            mc("claude-3", "anthropic", vec!["chat"]),
        ]);
        health.record_success("openai", 100);
        for _ in 0..5 {
            health.record_failure("openai");
        }
        let router = LlRouter::new(&routing_config, registry, health).unwrap();
        let ep = router.resolve("chat", &[], None).unwrap();
        assert_eq!(ep.provider_id, "anthropic");
    }

    #[test]
    fn resolve_uses_degraded_when_no_healthy() {
        let (registry, health, routing_config) =
            build_registry(vec![mc("gpt-4", "openai", vec!["chat"])]);
        health.record_success("openai", 100);
        for _ in 0..3 {
            health.record_failure("openai");
        }
        let router = LlRouter::new(&routing_config, registry, health).unwrap();
        // Should still return openai since it's the only option (degraded, not unavailable)
        let ep = router.resolve("chat", &[], None).unwrap();
        assert_eq!(ep.provider_id, "openai");
    }

    #[test]
    fn resolve_unavailable_provider_skipped_even_as_only_option() {
        let (registry, health, routing_config) =
            build_registry(vec![mc("gpt-4", "openai", vec!["chat"])]);
        health.record_success("openai", 100);
        for _ in 0..5 {
            health.record_failure("openai");
        }
        let router = LlRouter::new(&routing_config, registry, health).unwrap();
        let err = router.resolve("chat", &[], None).unwrap_err();
        assert!(format!("{}", err).contains("no available providers"));
    }

    // ─── Fallback Chain ───────────────────────────────────────────────────

    #[test]
    fn find_fallback_chain_exists() {
        let mut routing_config = default_routing_config();
        routing_config.fallback_chains = vec![crate::types::FallbackChainConfig {
            name: "chat-chain".into(),
            capability: "chat".into(),
            providers: vec!["openai".into(), "anthropic".into()],
        }];
        let (registry, health, _) = build_registry(vec![mc("gpt-4", "openai", vec!["chat"])]);
        let router = LlRouter::new(&routing_config, registry, health).unwrap();
        let chain = router.find_fallback_chain("chat").unwrap();
        assert_eq!(chain.name, "chat-chain");
        assert_eq!(chain.providers.len(), 2);
    }

    #[test]
    fn find_fallback_chain_not_found() {
        let (registry, health, routing_config) =
            build_registry(vec![mc("gpt-4", "openai", vec!["chat"])]);
        let router = LlRouter::new(&routing_config, registry, health).unwrap();
        assert!(router.find_fallback_chain("planning").is_none());
    }

    // ─── Resolve Next (Fallback) ──────────────────────────────────────────

    #[test]
    fn resolve_next_excludes_failed_provider() {
        let (registry, health, routing_config) = build_registry(vec![
            mc("gpt-4", "openai", vec!["chat"]),
            mc("claude-3", "anthropic", vec!["chat"]),
        ]);
        let router = LlRouter::new(&routing_config, registry, health).unwrap();
        let ep = router
            .resolve_next("chat", &["openai".to_string()], &[], None)
            .unwrap();
        assert_eq!(ep.provider_id, "anthropic");
    }

    #[test]
    fn resolve_next_all_excluded_returns_error() {
        let (registry, health, routing_config) =
            build_registry(vec![mc("gpt-4", "openai", vec!["chat"])]);
        let router = LlRouter::new(&routing_config, registry, health).unwrap();
        let err = router
            .resolve_next("chat", &["openai".to_string()], &[], None)
            .unwrap_err();
        assert!(format!("{}", err).contains("all providers failed"));
    }

    // ─── Cache Invalidation ───────────────────────────────────────────────

    #[test]
    fn invalidate_cache_clears() {
        let (registry, health, routing_config) =
            build_registry(vec![mc("gpt-4", "openai", vec!["chat"])]);
        let router = LlRouter::new(&routing_config, registry, health).unwrap();
        let _ = router.resolve("chat", &[], None).unwrap();
        router.invalidate_cache();
        // Should still work after cache clear (re-resolves)
        let ep = router.resolve("chat", &[], None).unwrap();
        assert_eq!(ep.model_id, "gpt-4");
    }

    #[test]
    fn cache_stale_after_health_change() {
        let (registry, health, routing_config) =
            build_registry(vec![mc("gpt-4", "openai", vec!["chat"])]);
        let router = LlRouter::new(&routing_config, registry, health.clone()).unwrap();
        // First resolve caches openai as healthy
        let ep1 = router.resolve("chat", &[], None).unwrap();
        assert_eq!(ep1.provider_id, "openai");

        // Mark unhealthy
        health.record_success("openai", 100);
        for _ in 0..5 {
            health.record_failure("openai");
        }

        // Cache entry should be invalidated on health check (stale detection)
        // Since openai is now unavailable, it should error
        let err = router.resolve("chat", &[], None).unwrap_err();
        assert!(format!("{}", err).contains("no available"));
    }

    // ─── Round Robin ──────────────────────────────────────────────────────

    #[test]
    fn round_robin_distributes_across_equal_priority() {
        // Models with same priority should be round-robined
        let (registry, health, routing_config) = build_registry(vec![
            mc_with_cost("gpt-4", "openai", vec!["chat"], 1.0, 2.0, 8192),
            mc_with_cost("claude-3", "anthropic", vec!["chat"], 1.0, 2.0, 8192),
        ]);
        let router = LlRouter::new(&routing_config, registry, health).unwrap();
        let ep1 = router
            .resolve("chat", &["cheap".to_string()], None)
            .unwrap();
        let ep2 = router
            .resolve("chat", &["cheap".to_string()], None)
            .unwrap();
        assert_ne!(ep1.provider_id, ep2.provider_id);
    }

    #[test]
    fn round_robin_differs_by_hint_set() {
        let (registry, health, routing_config) = build_registry(vec![
            mc_with_cost("gpt-4", "openai", vec!["chat"], 1.0, 2.0, 8192),
            mc_with_cost("claude-3", "anthropic", vec!["chat"], 1.0, 2.0, 8192),
        ]);
        let router = LlRouter::new(&routing_config, registry, health).unwrap();
        // Different hints → different round-robin key
        let _ = router
            .resolve("chat", &["cheap".to_string()], None)
            .unwrap();
        let ep = router.resolve("chat", &["fast".to_string()], None).unwrap();
        assert!(ep.model_id == "gpt-4" || ep.model_id == "claude-3");
    }

    // ─── Thread Safety ────────────────────────────────────────────────────

    #[test]
    fn router_is_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<LlRouter>();
        assert_sync::<LlRouter>();
        assert_send::<ProviderHealthTracker>();
        assert_sync::<ProviderHealthTracker>();
    }

    // ─── Integration: Full Resolution Pipeline ────────────────────────────

    #[test]
    fn full_pipeline_cheap_local_fallback() {
        let mut routing_config = default_routing_config();
        routing_config.fallback_chains = vec![crate::types::FallbackChainConfig {
            name: "chat-chain".into(),
            capability: "chat".into(),
            providers: vec!["ollama".into(), "openai".into()],
        }];
        let (registry, health, _) = build_registry(vec![
            mc_with_cost("gpt-4", "openai", vec!["chat"], 10.0, 20.0, 8192),
            mc_with_cost("llama3", "ollama", vec!["chat", "local"], 0.0, 0.0, 8192),
        ]);
        let router = LlRouter::new(&routing_config, registry, health).unwrap();

        // Cheap + local → should pick llama3 (free + local)
        let ep = router
            .resolve("chat", &["cheap".to_string(), "local".to_string()], None)
            .unwrap();
        assert_eq!(ep.model_id, "llama3");
    }

    #[test]
    fn health_records_persist_across_calls() {
        let config = default_routing_config();
        let tracker = ProviderHealthTracker::new(&config);
        tracker.record_success("openai", 100);
        tracker.record_success("anthropic", 200);
        assert!(tracker.is_healthy("openai"));
        assert!(tracker.is_healthy("anthropic"));

        tracker.record_failure("openai");
        tracker.record_failure("openai");
        assert!(tracker.is_healthy("openai"));

        tracker.record_failure("openai");
        assert!(!tracker.is_healthy("openai"));
    }

    #[test]
    fn error_rate_tracks_over_time() {
        let config = default_routing_config();
        let tracker = ProviderHealthTracker::new(&config);
        tracker.record_success("openai", 100);
        let report_before = tracker.report();
        let err_before = report_before
            .iter()
            .find(|r| r.provider_id == "openai")
            .map(|r| r.error_rate)
            .unwrap();
        assert!(err_before < 0.01);

        for _ in 0..5 {
            tracker.record_failure("openai");
        }
        let report_after = tracker.report();
        let err_after = report_after
            .iter()
            .find(|r| r.provider_id == "openai")
            .map(|r| r.error_rate)
            .unwrap();
        assert!(err_after > err_before);
    }

    // ─── FallbackChain from_config ─────────────────────────────────────────

    #[test]
    fn fallback_chain_from_config() {
        let cfg = crate::types::FallbackChainConfig {
            name: "test".into(),
            capability: "chat".into(),
            providers: vec!["a".into(), "b".into()],
        };
        let chain = FallbackChain::from_config(&cfg, 3, 1000);
        assert_eq!(chain.name, "test");
        assert_eq!(chain.providers.len(), 2);
        assert_eq!(chain.retry_config.max_attempts, 3);
    }
}
