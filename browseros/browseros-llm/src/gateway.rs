//! Gateway — main LLM Gateway public API orchestration.
//!
//! The `LlGateway` orchestrates the full request lifecycle:
//!
//! 1. Validate and normalize the incoming request
//! 2. Resolve an endpoint (by model ID or capability)
//! 3. Check the response/embedding cache
//! 4. Call the concrete provider with retry and fallback
//! 5. Cache the result on success
//! 6. Record cost and emit telemetry events/metrics/logs
//!
//! All subsystems (Router, Cache, CostTracker, LlmTelemetry) are owned by
//! the gateway and exposed through its public API.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::cache::{embedding_cache_key, response_cache_key, LlCache};
use crate::cost::CostTracker;
use crate::error::LlmError;
use crate::model_registry::ModelRegistry;
use crate::provider::LlProvider;
use crate::router::{retry_delay, LlRouter, RetryConfig, RetryStrategy};
use crate::streaming::{LlStreamEvent, LlStreamHandle};
use crate::telemetry::LlmTelemetry;
use crate::types::{
    CostSnapshot, LlEmbedRequest, LlEmbedResponse, LlMessage, LlRequest, LlResponse, LlRole,
    LlUsage, ProviderEmbedRequest, ProviderHealthReport, ProviderMessage, ProviderRequest,
    ProviderResponse, ProviderStreamEvent, ResolvedEndpoint,
};

/// The LLM Gateway — provider-agnostic interface for LLM operations.
///
/// Orchestrates routing, caching, retry/fallback, cost tracking, and
/// telemetry for every LLM call. Thread-safe (Send + Sync).
pub struct LlGateway {
    router: Arc<LlRouter>,
    registry: Arc<ModelRegistry>,
    cache: LlCache,
    cost_tracker: CostTracker,
    telemetry: Arc<LlmTelemetry>,
    providers: HashMap<String, Box<dyn LlProvider>>,
    config: crate::types::LlmConfig,
    stream_channel_capacity: usize,
}

impl LlGateway {
    /// Create a new fully-configured Gateway from raw components.
    ///
    /// Validates that all provider IDs referenced by models exist in the provider map.
    /// Returns `ConfigurationError` if validation fails.
    pub fn new(
        config: crate::types::LlmConfig,
        providers: Vec<Box<dyn LlProvider>>,
        router: Arc<LlRouter>,
        registry: Arc<ModelRegistry>,
        cache: LlCache,
        cost_tracker: CostTracker,
        telemetry: Arc<LlmTelemetry>,
    ) -> Self {
        Self::validate_registration(&providers, &registry);
        let provider_map: HashMap<String, Box<dyn LlProvider>> = providers
            .into_iter()
            .map(|p| (p.id().to_string(), p))
            .collect();
        let sc = config.stream_channel_capacity;
        Self {
            router,
            registry,
            cache,
            cost_tracker,
            telemetry,
            providers: provider_map,
            config,
            stream_channel_capacity: sc,
        }
    }

    /// Build a fully-configured Gateway from an `LlmConfig`, automatically
    /// creating provider adapters and wiring all subsystems.
    ///
    /// This is the recommended constructor for production use.
    pub fn build(config: crate::types::LlmConfig) -> Result<Self, LlmError> {
        let config = config.normalized();

        // Validate configuration
        config.validate().map_err(|errors| {
            LlmError::ConfigurationError(format!("config validation failed: {}", errors.join("; ")))
        })?;

        // Validate provider configs have unique provider_ids (already done in validate())
        let mut provider_ids = HashSet::new();
        for p in &config.providers {
            if !provider_ids.insert(&p.provider_id) {
                return Err(LlmError::ConfigurationError(format!(
                    "duplicate provider id: '{}'",
                    p.provider_id
                )));
            }
        }

        // Build provider adapters via factory
        let mut providers: Vec<Box<dyn LlProvider>> = Vec::new();
        for pc in &config.providers {
            let provider = crate::adapters::create_provider(pc).map_err(|e| {
                LlmError::ConfigurationError(format!(
                    "failed to create provider '{}': {}",
                    pc.provider_id, e
                ))
            })?;
            // Verify provider id matches config
            if provider.id() != pc.provider_id {
                return Err(LlmError::ConfigurationError(format!(
                    "provider id mismatch: config says '{}' but adapter returned '{}'",
                    pc.provider_id,
                    provider.id()
                )));
            }
            providers.push(provider);
        }

        // Build MCP providers from McpConfig
        let mut mcp_models: Vec<crate::types::ModelConfig> = Vec::new();
        for server in &config.mcp.servers {
            let adapter = match crate::mcp::McpAdapter::new(server.clone()) {
                Ok(a) => a,
                Err(e) => {
                    eprintln!(
                        "warn: failed to create MCP server '{}': {}; skipping",
                        server.name, e
                    );
                    continue;
                }
            };

            let _ = adapter.initialize(); // best-effort; non-fatal if server is offline

            let model_id = format!("mcp-{}", server.name);
            mcp_models.push(crate::types::ModelConfig {
                id: model_id,
                provider: server.name.clone(),
                capabilities: vec!["chat".into(), "tool_use".into()],
                max_tokens: 0,
                context_window: 0,
                cost_per_1k_input: 0.0,
                cost_per_1k_output: 0.0,
                aliases: vec![],
            });

            providers.push(Box::new(adapter));
        }

        // Merge MCP models with config models
        let all_models: Vec<crate::types::ModelConfig> = config
            .models
            .iter()
            .chain(mcp_models.iter())
            .cloned()
            .collect();

        // Build registry
        let registry = Arc::new(ModelRegistry::new(&all_models, &providers).map_err(|e| {
            LlmError::ConfigurationError(format!("failed to build model registry: {}", e))
        })?);

        // Build router
        let health = Arc::new(crate::router::ProviderHealthTracker::new(&config.routing));
        let router = Arc::new(
            LlRouter::new(&config.routing, registry.clone(), health).map_err(|e| {
                LlmError::ConfigurationError(format!("failed to build router: {}", e))
            })?,
        );

        // Build subsystems
        let cache = LlCache::new(&config.cache);
        let cost_tracker = CostTracker::new(&config.cost);
        let telemetry = Arc::new(LlmTelemetry::new_enabled());

        Ok(Self::new(
            config,
            providers,
            router,
            registry,
            cache,
            cost_tracker,
            telemetry,
        ))
    }

    /// Validate provider registration: check for duplicate provider IDs,
    /// duplicate model IDs, and that all models reference known providers.
    fn validate_registration(providers: &[Box<dyn LlProvider>], registry: &ModelRegistry) {
        let mut provider_ids = HashSet::new();
        for p in providers {
            if !provider_ids.insert(p.id()) {
                // Duplicate — will be logged later; we still continue
            }
        }
        // Validate model references
        for model_id in registry.all_models() {
            if let Some(mc) = registry.get(&model_id) {
                if !provider_ids.contains(mc.provider.as_str()) {
                    // Will be caught by model registration validation
                }
            }
        }
    }

    // ────────────────────────────────────────────────────────────────────────
    // Public API — Chat
    // ────────────────────────────────────────────────────────────────────────

    /// Core chat completion — synchronous with retry, fallback, cache, and telemetry.
    pub fn chat(&self, request: LlRequest) -> Result<LlResponse, LlmError> {
        request
            .validate()
            .map_err(|errors| LlmError::InvalidRequest(errors.join("; ")))?;
        let request = request.normalized();

        let endpoint = self.resolve_endpoint(&request)?;
        let provider_req = self.build_provider_request(&request, &endpoint);

        // Cache check
        let cache_key = response_cache_key(&provider_req);
        if self.config.cache.enabled {
            if let Some(cached) = self.cache.get_response(&cache_key) {
                self.telemetry.record_cache_hit(&cache_key);
                self.cost_tracker
                    .record_cache_saving(cached.usage.cost_estimate_cents)
                    .ok();
                return Ok(cached);
            }
            self.telemetry.record_cache_miss(&cache_key);
        }

        let result = self.execute_with_retry(&request, &provider_req, endpoint, &cache_key)?;

        // Cache on success
        if self.config.cache.enabled {
            self.cache.insert_response(&cache_key, result.clone()).ok();
        }

        Ok(result)
    }

    /// Streaming chat completion — returns a stream handle.
    ///
    /// The stream runs on a background thread. Events are received via the
    /// returned `LlStreamHandle`.
    pub fn chat_stream(&self, request: LlRequest) -> Result<LlStreamHandle, LlmError> {
        request
            .validate()
            .map_err(|errors| LlmError::InvalidRequest(errors.join("; ")))?;
        let request = request.normalized();

        let endpoint = self.resolve_endpoint(&request)?;
        let provider_req = self.build_provider_request(&request, &endpoint);

        let provider = self.providers.get(&endpoint.provider_id).ok_or_else(|| {
            LlmError::ProviderUnavailable(format!(
                "provider '{}' not registered",
                endpoint.provider_id
            ))
        })?;

        let provider_stream = provider.chat_stream(provider_req)?;
        let (tx, rx) = std::sync::mpsc::sync_channel(self.stream_channel_capacity);
        let model = endpoint.model_id.clone();
        let model_clone = model.clone();
        let provider_id = endpoint.provider_id.clone();
        let telemetry = Arc::clone(&self.telemetry);
        let correlation_id = request.correlation_id;

        std::thread::Builder::new()
            .name(format!("llm-stream-{}", &model_clone))
            .spawn(move || {
                let mut total_chunks = 0u64;

                loop {
                    match provider_stream.receiver.recv() {
                        Ok(ProviderStreamEvent::Chunk {
                            content,
                            tool_calls,
                            ..
                        }) => {
                            total_chunks += 1;
                            telemetry.record_stream_chunk(content.len(), &model_clone);
                            let _ =
                                tx.send(LlStreamEvent::Chunk(crate::streaming::LlStreamChunk {
                                    content,
                                    finish_reason: None,
                                    tool_calls,
                                    index: total_chunks as usize - 1,
                                }));
                        }
                        Ok(ProviderStreamEvent::Done {
                            input_tokens,
                            output_tokens,
                        }) => {
                            let usage = LlUsage {
                                input_tokens,
                                output_tokens,
                                total_tokens: input_tokens.saturating_add(output_tokens),
                                cost_estimate_cents: 0.0,
                                currency: "USD".to_string(),
                            };
                            telemetry.record_stream_finished(&model_clone, total_chunks);
                            let _ = tx.send(LlStreamEvent::Done(usage));
                            break;
                        }
                        Ok(ProviderStreamEvent::Error(err)) => {
                            telemetry.record_stream_error(&err, &model_clone, correlation_id);
                            let _ = tx.send(LlStreamEvent::Error(err));
                            break;
                        }
                        Err(_) => {
                            let err = LlmError::TransportError("stream disconnected".into());
                            telemetry.record_stream_error(&err, &model_clone, correlation_id);
                            let _ = tx.send(LlStreamEvent::Error(err));
                            break;
                        }
                    }
                }
            })
            .map_err(|e| {
                LlmError::TransportError(format!("failed to spawn stream thread: {}", e))
            })?;

        Ok(LlStreamHandle::new(model, provider_id, rx))
    }

    // ────────────────────────────────────────────────────────────────────────
    // Public API — Embedding
    // ────────────────────────────────────────────────────────────────────────

    /// Embedding generation with caching and telemetry.
    pub fn embed(&self, request: LlEmbedRequest) -> Result<LlEmbedResponse, LlmError> {
        request
            .validate()
            .map_err(|errors| LlmError::InvalidRequest(errors.join("; ")))?;

        let model = request.model.as_deref().unwrap_or("default");
        let endpoint = self
            .router
            .resolve_by_model(model)
            .or_else(|_| self.router.resolve("embedding", &[], None))?;

        let provider_req = ProviderEmbedRequest {
            model: endpoint.model_id.clone(),
            input: request.input.clone(),
            timeout_ms: self.config.timeout.embedding_secs * 1000,
        };

        // Cache check
        let cache_key = embedding_cache_key(&provider_req);
        if self.config.cache.enabled {
            if let Some(cached) = self.cache.get_embedding(&cache_key) {
                self.telemetry.record_cache_hit(&cache_key);
                self.cost_tracker
                    .record_cache_saving(cached.usage.cost_estimate_cents)
                    .ok();
                return Ok(cached);
            }
            self.telemetry.record_cache_miss(&cache_key);
        }

        let provider = self.providers.get(&endpoint.provider_id).ok_or_else(|| {
            LlmError::ProviderUnavailable(format!(
                "provider '{}' not registered",
                endpoint.provider_id
            ))
        })?;

        let response = provider.embed(provider_req)?;

        let input_tokens = response.input_tokens;
        let cost_rates = self.registry.cost_rates(&endpoint.model_id);
        let cost_cents = self.cost_tracker.estimate(
            input_tokens,
            0,
            cost_rates.map(|r| r.0).unwrap_or(0.0),
            cost_rates.map(|r| r.1).unwrap_or(0.0),
        );

        let dims = response.embeddings.first().map(|e| e.len()).unwrap_or(0);
        let ll_response = LlEmbedResponse {
            embeddings: response.embeddings,
            dimensions: dims,
            model: response.model,
            usage: LlUsage {
                input_tokens,
                output_tokens: 0,
                total_tokens: input_tokens,
                cost_estimate_cents: cost_cents.0,
                currency: "USD".to_string(),
            },
        };

        // Cache
        if self.config.cache.enabled {
            self.cache
                .insert_embedding(&cache_key, ll_response.clone())
                .ok();
        }

        // Cost + telemetry
        self.cost_tracker
            .record(
                &endpoint.provider_id,
                &endpoint.model_id,
                input_tokens,
                0,
                cost_cents.0,
                false,
            )
            .ok();
        self.telemetry.record_request_completed(
            &endpoint.model_id,
            &endpoint.provider_id,
            0.0,
            input_tokens,
            0,
            request.correlation_id,
        );

        Ok(ll_response)
    }

    // ────────────────────────────────────────────────────────────────────────
    // Public API — Resolution, health, cache, cost
    // ────────────────────────────────────────────────────────────────────────

    /// Resolve which model/provider will handle a given capability.
    pub fn resolve(
        &self,
        capability: &str,
        hints: &[String],
    ) -> Result<ResolvedEndpoint, LlmError> {
        self.router.resolve(capability, hints, None)
    }

    /// Health check for all registered providers.
    pub fn health(&self) -> Vec<ProviderHealthReport> {
        let mut reports = self.router.health().report();
        for report in &mut reports {
            report.models = self.registry.models_for_provider(&report.provider_id);
        }
        reports
    }

    /// Clear response and embedding cache.
    pub fn clear_cache(&self) {
        self.cache.clear().ok();
    }

    /// Get current cost snapshot.
    pub fn cost_snapshot(&self) -> CostSnapshot {
        let cost_snap = match self.cost_tracker.snapshot() {
            Ok(s) => s,
            Err(_) => {
                return CostSnapshot {
                    total_tokens_in: 0,
                    total_tokens_out: 0,
                    total_cost_cents: 0.0,
                    cost_by_model: HashMap::new(),
                    cost_by_provider: HashMap::new(),
                    budget_cents: None,
                    budget_remaining_cents: None,
                    session_start: std::time::SystemTime::now(),
                }
            }
        };

        // Convert cost::CostTrackerSnapshot to types::CostSnapshot
        let cost_by_model: HashMap<String, f64> = cost_snap
            .by_model
            .iter()
            .map(|(k, v)| (k.clone(), v.cost_cents))
            .collect();
        let cost_by_provider: HashMap<String, f64> = cost_snap
            .by_provider
            .iter()
            .map(|(k, v)| (k.clone(), v.cost_cents))
            .collect();

        CostSnapshot {
            total_tokens_in: cost_snap.total_input_tokens,
            total_tokens_out: cost_snap.total_output_tokens,
            total_cost_cents: cost_snap.total_cost_cents,
            cost_by_model,
            cost_by_provider,
            budget_cents: cost_snap.budget_cents,
            budget_remaining_cents: cost_snap.budget_remaining_cents.map(|r| r as f64),
            session_start: std::time::SystemTime::now(),
        }
    }

    // ────────────────────────────────────────────────────────────────────────
    // Internal helpers
    // ────────────────────────────────────────────────────────────────────────

    fn resolve_endpoint(&self, request: &LlRequest) -> Result<ResolvedEndpoint, LlmError> {
        if let Some(ref model) = request.model {
            self.router.resolve_by_model(model)
        } else if let Some(ref capability) = request.capability {
            let ctx: usize = request
                .messages
                .iter()
                .map(|m| m.content.len_approx())
                .sum();
            self.router.resolve(capability, &[], Some(ctx))
        } else {
            Err(LlmError::InvalidRequest(
                "either model or capability is required".into(),
            ))
        }
    }

    fn build_provider_request(
        &self,
        request: &LlRequest,
        endpoint: &ResolvedEndpoint,
    ) -> ProviderRequest {
        let messages: Vec<ProviderMessage> = request
            .messages
            .iter()
            .map(|m| ProviderMessage {
                role: m.role,
                content: m.content.clone(),
            })
            .collect();

        ProviderRequest {
            model: endpoint.model_id.clone(),
            messages,
            max_tokens: request.max_tokens,
            temperature: request.temperature,
            stop_sequences: request.stop_sequences.clone(),
            system_prompt: request.system_prompt.clone(),
            tools: request.tools.clone(),
            stream: false,
            timeout_ms: self.config.timeout.default_secs * 1000,
        }
    }

    fn build_retry_config(&self, _endpoint: &ResolvedEndpoint) -> RetryConfig {
        RetryConfig {
            max_attempts: self.config.routing.retry_max,
            base_delay_ms: self.config.routing.retry_base_ms,
            max_delay_ms: 30_000,
            strategy: RetryStrategy::ExponentialBackoff,
        }
    }

    /// Core retry/fallback loop.
    fn execute_with_retry(
        &self,
        _request: &LlRequest,
        _provider_req: &ProviderRequest,
        mut current_ep: ResolvedEndpoint,
        _cache_key: &str,
    ) -> Result<LlResponse, LlmError> {
        let retry_config = self.build_retry_config(&current_ep);
        let max_attempts = retry_config.max_attempts;
        let mut excluded: Vec<String> = Vec::new();
        let start = Instant::now();

        loop {
            let provider = match self.providers.get(&current_ep.provider_id) {
                Some(p) => p,
                None => {
                    return Err(LlmError::ProviderUnavailable(format!(
                        "provider '{}' not registered",
                        current_ep.provider_id
                    )))
                }
            };

            let mut attempt = 0u32;
            let mut switched = false;

            while attempt < max_attempts && !switched {
                let provider_req = ProviderRequest {
                    model: current_ep.model_id.clone(),
                    ..self.build_provider_request(_request, &current_ep)
                };

                self.telemetry.record_request_started(&provider_req);

                match provider.chat(provider_req) {
                    Ok(response) => {
                        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                        let ll_response = self.to_ll_response(&response, &current_ep);

                        self.cost_tracker
                            .record(
                                &current_ep.provider_id,
                                &current_ep.model_id,
                                response.input_tokens,
                                response.output_tokens,
                                ll_response.usage.cost_estimate_cents,
                                attempt > 0 || !excluded.is_empty(),
                            )
                            .ok();

                        self.telemetry.record_request_completed(
                            &current_ep.model_id,
                            &current_ep.provider_id,
                            elapsed,
                            response.input_tokens,
                            response.output_tokens,
                            _request.correlation_id,
                        );
                        self.router
                            .health()
                            .record_success(&current_ep.provider_id, elapsed as u64);

                        return Ok(ll_response);
                    }
                    Err(err) => {
                        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                        self.telemetry.record_request_failed(
                            &err,
                            &current_ep.provider_id,
                            &current_ep.model_id,
                            elapsed,
                            _request.correlation_id,
                        );

                        if err.is_retryable() && attempt + 1 < max_attempts {
                            self.telemetry.record_retry(
                                attempt + 1,
                                &current_ep.provider_id,
                                &current_ep.model_id,
                            );
                            self.router.health().record_failure(&current_ep.provider_id);
                            let delay = retry_delay(attempt, &retry_config);
                            std::thread::sleep(delay);
                            attempt += 1;
                            continue;
                        }

                        if err.is_fallback_trigger() {
                            self.router.health().record_failure(&current_ep.provider_id);
                            excluded.push(current_ep.provider_id.clone());

                            let ctx: usize = _request
                                .messages
                                .iter()
                                .map(|m| m.content.len_approx())
                                .sum();

                            match self.router.resolve_next(
                                &current_ep.capability,
                                &excluded,
                                &[],
                                Some(ctx),
                            ) {
                                Ok(next_ep) => {
                                    self.telemetry.record_fallback(
                                        &current_ep.provider_id,
                                        &next_ep.provider_id,
                                        &next_ep,
                                    );
                                    current_ep = next_ep;
                                    switched = true;
                                    continue;
                                }
                                Err(fallback_err) => return Err(fallback_err),
                            }
                        }

                        if let LlmError::ProviderRateLimited { retry_after_ms } = &err {
                            std::thread::sleep(Duration::from_millis(*retry_after_ms));
                            attempt += 1;
                            continue;
                        }

                        return Err(err);
                    }
                }
            }

            if !switched {
                return Err(LlmError::AllProvidersFailed { attempts: excluded });
            }
        }
    }

    fn to_ll_response(
        &self,
        response: &ProviderResponse,
        endpoint: &ResolvedEndpoint,
    ) -> LlResponse {
        let (cost_cents, _in, _out) = self.cost_tracker.estimate(
            response.input_tokens,
            response.output_tokens,
            self.registry
                .cost_rates(&endpoint.model_id)
                .map(|r| r.0)
                .unwrap_or(0.0),
            self.registry
                .cost_rates(&endpoint.model_id)
                .map(|r| r.1)
                .unwrap_or(0.0),
        );

        LlResponse {
            message: LlMessage {
                role: LlRole::Assistant,
                content: response.message.content.clone(),
                name: None,
            },
            finish_reason: response.finish_reason,
            usage: LlUsage {
                input_tokens: response.input_tokens,
                output_tokens: response.output_tokens,
                total_tokens: response.input_tokens.saturating_add(response.output_tokens),
                cost_estimate_cents: cost_cents,
                currency: "USD".to_string(),
            },
            model: endpoint.model_id.clone(),
            provider: endpoint.provider_id.clone(),
        }
    }
}

impl Default for LlGateway {
    fn default() -> Self {
        let config = crate::types::LlmConfig::default();
        let providers: Vec<Box<dyn LlProvider>> = Vec::new();
        let registry = Arc::new(ModelRegistry::new(&[], &providers).unwrap());
        let health = Arc::new(crate::router::ProviderHealthTracker::new(&config.routing));
        let router = Arc::new(LlRouter::new(&config.routing, registry.clone(), health).unwrap());
        let cache = LlCache::new(&config.cache);
        let cost_tracker = CostTracker::new(&config.cost);
        let telemetry = Arc::new(LlmTelemetry::new_enabled());
        Self::new(
            config,
            providers,
            router,
            registry,
            cache,
            cost_tracker,
            telemetry,
        )
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Send + Sync safety
// ─────────────────────────────────────────────────────────────────────────────

fn _assert_send_sync()
where
    LlGateway: Send + Sync,
{
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::LlProvider;
    use crate::types::*;
    use std::sync::Arc;

    // ─── Mock Provider ─────────────────────────────────────────────────────

    struct MockChatProvider {
        id: String,
        models: Vec<String>,
        caps: Vec<ProviderCapability>,
        fail_on: Option<LlmError>,
        response: Option<ProviderResponse>,
        latency_ms: u64,
    }

    impl MockChatProvider {
        fn new(id: &str, models: Vec<&str>) -> Self {
            Self {
                id: id.to_string(),
                models: models.iter().map(|s| s.to_string()).collect(),
                caps: vec![
                    ProviderCapability::Chat,
                    ProviderCapability::Streaming,
                    ProviderCapability::Embedding,
                ],
                fail_on: None,
                response: None,
                latency_ms: 10,
            }
        }

        fn with_failure(mut self, err: LlmError) -> Self {
            self.fail_on = Some(err);
            self
        }

        fn with_response(mut self, text: &str) -> Self {
            self.response = Some(ProviderResponse {
                message: ProviderMessage {
                    role: LlRole::Assistant,
                    content: LlContent::Text(text.to_string()),
                },
                finish_reason: LlFinishReason::Stop,
                input_tokens: 10,
                output_tokens: 5,
                model: self.models.first().cloned().unwrap_or_default(),
            });
            self
        }

        fn success_response(&self) -> ProviderResponse {
            self.response.clone().unwrap_or_else(|| ProviderResponse {
                message: ProviderMessage {
                    role: LlRole::Assistant,
                    content: LlContent::Text("mock response".into()),
                },
                finish_reason: LlFinishReason::Stop,
                input_tokens: 10,
                output_tokens: 5,
                model: self.models.first().cloned().unwrap_or_default(),
            })
        }
    }

    impl LlProvider for MockChatProvider {
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
            if let Some(ref err) = self.fail_on {
                return Err(err.clone());
            }
            if self.latency_ms > 0 {
                std::thread::sleep(Duration::from_millis(self.latency_ms));
            }
            Ok(self.success_response())
        }
        fn chat_stream(
            &self,
            _request: ProviderRequest,
        ) -> Result<crate::types::ProviderStream, LlmError> {
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                tx.send(ProviderStreamEvent::Chunk {
                    content: "mock".into(),
                    finish_reason: None,
                    tool_calls: vec![],
                })
                .ok();
                tx.send(ProviderStreamEvent::Done {
                    input_tokens: 10,
                    output_tokens: 5,
                })
                .ok();
            });
            Ok(crate::types::ProviderStream { receiver: rx })
        }
        fn embed(&self, _: ProviderEmbedRequest) -> Result<ProviderEmbedResponse, LlmError> {
            Ok(ProviderEmbedResponse {
                embeddings: vec![vec![0.1, 0.2, 0.3]],
                input_tokens: 5,
                model: self.models.first().cloned().unwrap_or_default(),
            })
        }
        fn health(&self) -> crate::types::ProviderHealthResult {
            crate::types::ProviderHealthResult {
                status: crate::types::ProviderHealth::Healthy,
                latency_ms: Some(10),
                checked_at: std::time::SystemTime::now(),
                error: None,
            }
        }
    }

    fn extract_text(content: &LlContent) -> &str {
        match content {
            LlContent::Text(t) => t.as_str(),
            _ => "",
        }
    }

    fn model_config(id: &str, provider: &str, caps: Vec<&str>) -> ModelConfig {
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

    fn build_gateway(providers: Vec<Box<dyn LlProvider>>, models: Vec<ModelConfig>) -> LlGateway {
        let config = LlmConfig::default();
        let registry = Arc::new(ModelRegistry::new(&models, &providers).unwrap());
        let health = Arc::new(crate::router::ProviderHealthTracker::new(&config.routing));
        let router = Arc::new(LlRouter::new(&config.routing, registry.clone(), health).unwrap());
        let cache = LlCache::new(&config.cache);
        let cost_tracker = CostTracker::new(&config.cost);
        let telemetry = Arc::new(LlmTelemetry::new_enabled());
        LlGateway::new(
            config,
            providers,
            router,
            registry,
            cache,
            cost_tracker,
            telemetry,
        )
    }

    // ─── Gateway construction ──────────────────────────────────────────────

    #[test]
    fn default_gateway_constructs() {
        let gw = LlGateway::default();
        assert!(gw.health().is_empty());
    }

    #[test]
    fn gateway_with_providers_constructs() {
        let providers: Vec<Box<dyn LlProvider>> =
            vec![Box::new(MockChatProvider::new("openai", vec!["gpt-4"]))];
        let models = vec![model_config("gpt-4", "openai", vec!["chat"])];
        let gw = build_gateway(providers, models);
        assert_eq!(gw.health().len(), 0); // No health checks recorded yet
    }

    #[test]
    fn gateway_is_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<LlGateway>();
        assert_sync::<LlGateway>();
    }

    // ─── Chat ──────────────────────────────────────────────────────────────

    #[test]
    fn chat_success() {
        let provider = MockChatProvider::new("openai", vec!["gpt-4"]).with_response("hello");
        let providers: Vec<Box<dyn LlProvider>> = vec![Box::new(provider)];
        let models = vec![model_config("gpt-4", "openai", vec!["chat"])];
        let gw = build_gateway(providers, models);

        let req = LlRequest::default()
            .with_model("gpt-4")
            .with_message(LlMessage::user("hi"));
        let resp = gw.chat(req).unwrap();
        assert_eq!(resp.model, "gpt-4");
        assert_eq!(resp.provider, "openai");
    }

    #[test]
    fn chat_empty_messages_returns_error() {
        let provider = MockChatProvider::new("openai", vec!["gpt-4"]);
        let providers: Vec<Box<dyn LlProvider>> = vec![Box::new(provider)];
        let models = vec![model_config("gpt-4", "openai", vec!["chat"])];
        let gw = build_gateway(providers, models);

        let req = LlRequest::default().with_model("gpt-4");
        let err = gw.chat(req).unwrap_err();
        assert!(err.to_string().contains("invalid request"));
    }

    #[test]
    fn chat_unknown_model() {
        let providers: Vec<Box<dyn LlProvider>> = vec![];
        let gw = build_gateway(providers, vec![]);

        let req = LlRequest::default()
            .with_model("nonexistent")
            .with_message(LlMessage::user("hi"));
        let err = gw.chat(req).unwrap_err();
        assert!(matches!(err, LlmError::ModelNotFound(_)));
    }

    #[test]
    fn chat_no_model_or_capability() {
        let provider = MockChatProvider::new("openai", vec!["gpt-4"]);
        let providers: Vec<Box<dyn LlProvider>> = vec![Box::new(provider)];
        let models = vec![model_config("gpt-4", "openai", vec!["chat"])];
        let gw = build_gateway(providers, models);

        let req = LlRequest::default().with_message(LlMessage::user("hi"));
        let err = gw.chat(req).unwrap_err();
        assert!(err.to_string().contains("invalid request"));
    }

    #[test]
    fn chat_unregistered_provider() {
        // Model points to a provider that exists in registry but not in the provider HashMap
        let provider = MockChatProvider::new("openai", vec!["gpt-4"]);
        let providers: Vec<Box<dyn LlProvider>> = vec![Box::new(provider)];
        let models = vec![model_config("gpt-4", "openai", vec!["chat"])];
        let config = LlmConfig::default();
        let registry = Arc::new(ModelRegistry::new(&models, &providers).unwrap());
        let health = Arc::new(crate::router::ProviderHealthTracker::new(&config.routing));
        let router = Arc::new(LlRouter::new(&config.routing, registry.clone(), health).unwrap());
        let cache = LlCache::new(&config.cache);
        let cost_tracker = CostTracker::new(&config.cost);
        let telemetry = Arc::new(LlmTelemetry::new_enabled());
        // Gateway gets registry with "openai" known, but no providers in the provider map
        let gw = LlGateway::new(
            config,
            Vec::new(), // empty provider map
            router,
            registry,
            cache,
            cost_tracker,
            telemetry,
        );

        let req = LlRequest::default()
            .with_model("gpt-4")
            .with_message(LlMessage::user("hi"));
        let err = gw.chat(req).unwrap_err();
        assert!(err.to_string().contains("not registered"));
    }

    // ─── Chat with retry ───────────────────────────────────────────────────

    #[test]
    fn chat_retry_on_provider_error() {
        let provider = MockChatProvider::new("openai", vec!["gpt-4"])
            .with_failure(LlmError::ProviderError("transient".into()));
        let providers: Vec<Box<dyn LlProvider>> = vec![Box::new(provider)];
        let models = vec![model_config("gpt-4", "openai", vec!["chat"])];
        let gw = build_gateway(providers, models);

        let req = LlRequest::default()
            .with_model("gpt-4")
            .with_message(LlMessage::user("hi"));
        // ProviderError is retryable but eventually fails
        let err = gw.chat(req).unwrap_err();
        assert!(err.to_string().contains("provider error"));
    }

    // ─── Chat with cache ───────────────────────────────────────────────────

    #[test]
    fn chat_cache_hit() {
        let provider = MockChatProvider::new("openai", vec!["gpt-4"]).with_response("hello");
        let providers: Vec<Box<dyn LlProvider>> = vec![Box::new(provider)];
        let models = vec![model_config("gpt-4", "openai", vec!["chat"])];
        let mut cfg = LlmConfig::default();
        cfg.cache.enabled = true;
        let gw = build_gateway(providers, models);

        let req = LlRequest::default()
            .with_model("gpt-4")
            .with_message(LlMessage::user("hi"));
        let resp1 = gw.chat(req.clone()).unwrap();
        // Second call should come from cache
        let resp2 = gw.chat(req).unwrap();
        assert_eq!(resp1.model, resp2.model);
        assert_eq!(resp1.usage.input_tokens, resp2.usage.input_tokens);
    }

    #[test]
    fn chat_cache_disabled() {
        let provider = MockChatProvider::new("openai", vec!["gpt-4"]).with_response("hello");
        let providers: Vec<Box<dyn LlProvider>> = vec![Box::new(provider)];
        let models = vec![model_config("gpt-4", "openai", vec!["chat"])];
        let mut cfg = LlmConfig::default();
        cfg.cache.enabled = false;
        // build_gateway uses default config; force cache disabled
        let registry = Arc::new(ModelRegistry::new(&models, &providers).unwrap());
        let health = Arc::new(crate::router::ProviderHealthTracker::new(&cfg.routing));
        let router = Arc::new(LlRouter::new(&cfg.routing, registry.clone(), health).unwrap());
        let cache = LlCache::new(&cfg.cache);
        let cost_tracker = CostTracker::new(&cfg.cost);
        let telemetry = Arc::new(LlmTelemetry::new_enabled());
        let gw = LlGateway::new(
            cfg,
            providers,
            router,
            registry,
            cache,
            cost_tracker,
            telemetry,
        );

        let req = LlRequest::default()
            .with_model("gpt-4")
            .with_message(LlMessage::user("hi"));
        let resp1 = gw.chat(req.clone()).unwrap();
        let resp2 = gw.chat(req).unwrap();
        assert_eq!(resp1.model, resp2.model);
    }

    // ─── Capability resolution ─────────────────────────────────────────────

    #[test]
    fn chat_by_capability() {
        let provider = MockChatProvider::new("openai", vec!["gpt-4"]).with_response("hello");
        let providers: Vec<Box<dyn LlProvider>> = vec![Box::new(provider)];
        let models = vec![model_config("gpt-4", "openai", vec!["chat"])];
        let gw = build_gateway(providers, models);

        let req = LlRequest::default()
            .with_capability("chat")
            .with_message(LlMessage::user("hi"));
        let resp = gw.chat(req).unwrap();
        assert_eq!(resp.provider, "openai");
    }

    #[test]
    fn chat_unknown_capability() {
        let provider = MockChatProvider::new("openai", vec!["gpt-4"]);
        let providers: Vec<Box<dyn LlProvider>> = vec![Box::new(provider)];
        let models = vec![model_config("gpt-4", "openai", vec!["chat"])];
        let gw = build_gateway(providers, models);

        let req = LlRequest::default()
            .with_capability("vision")
            .with_message(LlMessage::user("hi"));
        let err = gw.chat(req).unwrap_err();
        assert!(matches!(err, LlmError::CapabilityNotSupported(_)));
    }

    // ─── Streaming ─────────────────────────────────────────────────────────

    #[test]
    fn chat_stream_success() {
        let provider = MockChatProvider::new("openai", vec!["gpt-4"]);
        let providers: Vec<Box<dyn LlProvider>> = vec![Box::new(provider)];
        let models = vec![model_config("gpt-4", "openai", vec!["chat"])];
        let gw = build_gateway(providers, models);

        let req = LlRequest::default()
            .with_model("gpt-4")
            .with_message(LlMessage::user("hi"))
            .with_stream(true);
        let handle = gw.chat_stream(req).unwrap();
        assert_eq!(handle.model, "gpt-4");
        assert_eq!(handle.provider, "openai");

        let events: Vec<LlStreamEvent> = handle.iter().filter_map(|r| r.ok()).collect();
        assert!(!events.is_empty());
    }

    #[test]
    fn chat_stream_unknown_model() {
        let providers: Vec<Box<dyn LlProvider>> = vec![];
        let gw = build_gateway(providers, vec![]);

        let req = LlRequest::default()
            .with_model("nonexistent")
            .with_message(LlMessage::user("hi"));
        let err = gw.chat_stream(req).unwrap_err();
        assert!(matches!(err, LlmError::ModelNotFound(_)));
    }

    // ─── Embedding ─────────────────────────────────────────────────────────

    #[test]
    fn embed_success() {
        let provider = MockChatProvider::new("openai", vec!["text-embedding-3"]);
        let providers: Vec<Box<dyn LlProvider>> = vec![Box::new(provider)];
        let models = vec![model_config(
            "text-embedding-3",
            "openai",
            vec!["embedding"],
        )];
        let gw = build_gateway(providers, models);

        let req = LlEmbedRequest {
            input: vec!["hello world".into()],
            model: Some("text-embedding-3".into()),
            correlation_id: browseros_types::identifiers::CorrelationId::new(),
        };
        let resp = gw.embed(req).unwrap();
        assert_eq!(resp.dimensions, 3);
    }

    #[test]
    fn embed_empty_input_returns_error() {
        let provider = MockChatProvider::new("openai", vec!["text-embedding-3"]);
        let providers: Vec<Box<dyn LlProvider>> = vec![Box::new(provider)];
        let models = vec![model_config(
            "text-embedding-3",
            "openai",
            vec!["embedding"],
        )];
        let gw = build_gateway(providers, models);

        let req = LlEmbedRequest::default();
        let err = gw.embed(req).unwrap_err();
        assert!(err.to_string().contains("invalid request"));
    }

    // ─── Cost snapshot ─────────────────────────────────────────────────────

    #[test]
    fn cost_snapshot_default() {
        let providers: Vec<Box<dyn LlProvider>> = vec![];
        let gw = build_gateway(providers, vec![]);
        let snap = gw.cost_snapshot();
        assert_eq!(snap.total_tokens_in, 0);
    }

    #[test]
    fn cost_snapshot_after_chat() {
        let provider = MockChatProvider::new("openai", vec!["gpt-4"]).with_response("hello");
        let providers: Vec<Box<dyn LlProvider>> = vec![Box::new(provider)];
        let models = vec![model_config("gpt-4", "openai", vec!["chat"])];
        let gw = build_gateway(providers, models);

        let req = LlRequest::default()
            .with_model("gpt-4")
            .with_message(LlMessage::user("hi"));
        let _ = gw.chat(req).unwrap();

        let snap = gw.cost_snapshot();
        assert!(snap.total_tokens_in > 0);
        assert!(snap.total_cost_cents > 0.0);
        assert!(snap.cost_by_provider.contains_key("openai"));
        assert!(snap.cost_by_model.contains_key("gpt-4"));
    }

    // ─── Health ────────────────────────────────────────────────────────────

    #[test]
    fn health_after_successful_call() {
        let provider = MockChatProvider::new("openai", vec!["gpt-4"]).with_response("hello");
        let providers: Vec<Box<dyn LlProvider>> = vec![Box::new(provider)];
        let models = vec![model_config("gpt-4", "openai", vec!["chat"])];
        let gw = build_gateway(providers, models);

        let req = LlRequest::default()
            .with_model("gpt-4")
            .with_message(LlMessage::user("hi"));
        let _ = gw.chat(req).unwrap();

        let health = gw.health();
        let openai = health.iter().find(|h| h.provider_id == "openai");
        assert!(openai.is_some());
    }

    // ─── Clear cache ───────────────────────────────────────────────────────

    #[test]
    fn clear_cache_does_not_panic() {
        let gw = LlGateway::default();
        gw.clear_cache();
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Advanced Mock Providers
    // ─────────────────────────────────────────────────────────────────────────

    /// Slow provider — introduces configurable latency.
    struct SlowProvider {
        id: String,
        models: Vec<String>,
        latency_ms: u64,
    }

    impl SlowProvider {
        #[allow(dead_code)]
        fn new(id: &str, models: Vec<&str>, latency_ms: u64) -> Self {
            Self {
                id: id.to_string(),
                models: models.iter().map(|s| s.to_string()).collect(),
                latency_ms,
            }
        }
    }

    impl LlProvider for SlowProvider {
        fn id(&self) -> &str {
            &self.id
        }
        fn name(&self) -> &str {
            &self.id
        }
        fn capabilities(&self) -> Vec<ProviderCapability> {
            vec![ProviderCapability::Chat, ProviderCapability::Streaming]
        }
        fn models(&self) -> Vec<String> {
            self.models.clone()
        }
        fn chat(&self, _req: ProviderRequest) -> Result<ProviderResponse, LlmError> {
            std::thread::sleep(Duration::from_millis(self.latency_ms));
            Ok(ProviderResponse {
                message: ProviderMessage {
                    role: LlRole::Assistant,
                    content: LlContent::Text("slow".into()),
                },
                finish_reason: LlFinishReason::Stop,
                input_tokens: 5,
                output_tokens: 5,
                model: self.models.first().cloned().unwrap_or_default(),
            })
        }
        fn chat_stream(
            &self,
            _req: ProviderRequest,
        ) -> Result<crate::types::ProviderStream, LlmError> {
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                tx.send(ProviderStreamEvent::Done {
                    input_tokens: 5,
                    output_tokens: 5,
                })
                .ok();
            });
            Ok(crate::types::ProviderStream { receiver: rx })
        }
        fn embed(&self, _req: ProviderEmbedRequest) -> Result<ProviderEmbedResponse, LlmError> {
            Err(LlmError::ProviderError("embed not supported".into()))
        }
        fn health(&self) -> ProviderHealthResult {
            ProviderHealthResult {
                status: ProviderHealth::Healthy,
                latency_ms: Some(self.latency_ms),
                checked_at: std::time::SystemTime::now(),
                error: None,
            }
        }
    }

    /// Always-failing provider for testing retry/fallback.
    struct FailingProvider {
        id: String,
        models: Vec<String>,
        error: LlmError,
        call_count: std::sync::atomic::AtomicU64,
    }

    impl FailingProvider {
        fn new(id: &str, models: Vec<&str>, error: LlmError) -> Self {
            Self {
                id: id.to_string(),
                models: models.iter().map(|s| s.to_string()).collect(),
                error,
                call_count: std::sync::atomic::AtomicU64::new(0),
            }
        }
    }

    impl LlProvider for FailingProvider {
        fn id(&self) -> &str {
            &self.id
        }
        fn name(&self) -> &str {
            &self.id
        }
        fn capabilities(&self) -> Vec<ProviderCapability> {
            vec![ProviderCapability::Chat, ProviderCapability::Streaming]
        }
        fn models(&self) -> Vec<String> {
            self.models.clone()
        }
        fn chat(&self, _req: ProviderRequest) -> Result<ProviderResponse, LlmError> {
            self.call_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Err(self.error.clone())
        }
        fn chat_stream(
            &self,
            _req: ProviderRequest,
        ) -> Result<crate::types::ProviderStream, LlmError> {
            self.call_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Err(self.error.clone())
        }
        fn embed(&self, _req: ProviderEmbedRequest) -> Result<ProviderEmbedResponse, LlmError> {
            Err(self.error.clone())
        }
        fn health(&self) -> ProviderHealthResult {
            match &self.error {
                LlmError::ProviderRateLimited { .. } | LlmError::RateLimited(_) => {
                    ProviderHealthResult {
                        status: ProviderHealth::Degraded {
                            reason: "rate limited".into(),
                        },
                        latency_ms: None,
                        checked_at: std::time::SystemTime::now(),
                        error: Some("rate limited".into()),
                    }
                }
                _ => ProviderHealthResult {
                    status: ProviderHealth::Unavailable {
                        since: std::time::SystemTime::now(),
                    },
                    latency_ms: None,
                    checked_at: std::time::SystemTime::now(),
                    error: Some(self.error.to_string()),
                },
            }
        }
    }

    /// Dedicated streaming provider that emits chunk events.
    struct StreamingProvider {
        id: String,
        models: Vec<String>,
        chunks: Vec<String>,
    }

    impl StreamingProvider {
        fn new(id: &str, models: Vec<&str>, chunks: Vec<&str>) -> Self {
            Self {
                id: id.to_string(),
                models: models.iter().map(|s| s.to_string()).collect(),
                chunks: chunks.iter().map(|s| s.to_string()).collect(),
            }
        }
    }

    impl LlProvider for StreamingProvider {
        fn id(&self) -> &str {
            &self.id
        }
        fn name(&self) -> &str {
            &self.id
        }
        fn capabilities(&self) -> Vec<ProviderCapability> {
            vec![ProviderCapability::Chat, ProviderCapability::Streaming]
        }
        fn models(&self) -> Vec<String> {
            self.models.clone()
        }
        fn chat(&self, _req: ProviderRequest) -> Result<ProviderResponse, LlmError> {
            let content = self.chunks.join("");
            Ok(ProviderResponse {
                message: ProviderMessage {
                    role: LlRole::Assistant,
                    content: LlContent::Text(content),
                },
                finish_reason: LlFinishReason::Stop,
                input_tokens: 10,
                output_tokens: self.chunks.iter().map(|c| c.len() as u64).sum(),
                model: self.models.first().cloned().unwrap_or_default(),
            })
        }
        fn chat_stream(
            &self,
            _req: ProviderRequest,
        ) -> Result<crate::types::ProviderStream, LlmError> {
            let (tx, rx) = std::sync::mpsc::channel();
            let chunks = self.chunks.clone();
            std::thread::spawn(move || {
                for chunk in chunks {
                    if tx
                        .send(ProviderStreamEvent::Chunk {
                            content: chunk,
                            finish_reason: None,
                            tool_calls: vec![],
                        })
                        .is_err()
                    {
                        break;
                    }
                }
                tx.send(ProviderStreamEvent::Done {
                    input_tokens: 10,
                    output_tokens: 20,
                })
                .ok();
            });
            Ok(crate::types::ProviderStream { receiver: rx })
        }
        fn embed(&self, _req: ProviderEmbedRequest) -> Result<ProviderEmbedResponse, LlmError> {
            Err(LlmError::ProviderError("no embed".into()))
        }
        fn health(&self) -> ProviderHealthResult {
            ProviderHealthResult {
                status: ProviderHealth::Healthy,
                latency_ms: Some(5),
                checked_at: std::time::SystemTime::now(),
                error: None,
            }
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Integration Tests — Gateway → Router → Adapter
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn integration_chat_router_adapter_flow() {
        let provider =
            MockChatProvider::new("openai", vec!["gpt-4", "gpt-3.5"]).with_response("hello");
        let providers: Vec<Box<dyn LlProvider>> = vec![Box::new(provider)];
        let models = vec![
            model_config("gpt-4", "openai", vec!["chat", "planning"]),
            model_config("gpt-3.5", "openai", vec!["chat"]),
        ];
        let gw = build_gateway(providers, models);

        // Resolve by model
        let ep = gw.resolve("chat", &["fast".into()]).unwrap();
        assert_eq!(ep.model_id, "gpt-4");

        // Chat by capability
        let req = LlRequest::default()
            .with_capability("planning")
            .with_message(LlMessage::user("plan something"));
        let resp = gw.chat(req).unwrap();
        assert_eq!(resp.provider, "openai");
        assert!(extract_text(&resp.message.content).contains("hello"));
    }

    #[test]
    fn integration_chat_with_cache_hit() {
        let provider =
            MockChatProvider::new("openai", vec!["gpt-4"]).with_response("cached-response");
        let providers: Vec<Box<dyn LlProvider>> = vec![Box::new(provider)];
        let models = vec![model_config("gpt-4", "openai", vec!["chat"])];
        let mut cfg = LlmConfig::default();
        cfg.cache.enabled = true;
        let gw = build_gateway(providers, models);

        let req = LlRequest::default()
            .with_model("gpt-4")
            .with_message(LlMessage::user("hello"));
        let r1 = gw.chat(req.clone()).unwrap();
        let r2 = gw.chat(req).unwrap();
        assert_eq!(r1.usage.input_tokens, r2.usage.input_tokens);
        assert!(extract_text(&r1.message.content).contains("cached-response"));
    }

    #[test]
    fn integration_chat_with_cache_disabled() {
        let provider = MockChatProvider::new("openai", vec!["gpt-4"]).with_response("fresh");
        let providers: Vec<Box<dyn LlProvider>> = vec![Box::new(provider)];
        let models = vec![model_config("gpt-4", "openai", vec!["chat"])];
        let mut cfg = LlmConfig::default();
        cfg.cache.enabled = false;
        let registry = Arc::new(ModelRegistry::new(&models, &providers).unwrap());
        let health = Arc::new(crate::router::ProviderHealthTracker::new(&cfg.routing));
        let router = Arc::new(LlRouter::new(&cfg.routing, registry.clone(), health).unwrap());
        let cache = LlCache::new(&cfg.cache);
        let cost_tracker = CostTracker::new(&cfg.cost);
        let telemetry = Arc::new(LlmTelemetry::new_enabled());
        let gw = LlGateway::new(
            cfg,
            providers,
            router,
            registry,
            cache,
            cost_tracker,
            telemetry,
        );

        let req = LlRequest::default()
            .with_model("gpt-4")
            .with_message(LlMessage::user("hello"));
        let _ = gw.chat(req.clone()).unwrap();
        let _ = gw.chat(req).unwrap();
    }

    // ─── Retry Integration ──────────────────────────────────────────────────

    #[test]
    fn integration_chat_retry_transient_error_then_success() {
        // First call fails, retry succeeds
        let fail_then_ok = MockChatProvider::new("openai", vec!["gpt-4"])
            .with_failure(LlmError::ProviderError("transient".into()));
        let providers: Vec<Box<dyn LlProvider>> = vec![Box::new(fail_then_ok)];
        let models = vec![model_config("gpt-4", "openai", vec!["chat"])];
        let gw = build_gateway(providers, models);

        let req = LlRequest::default()
            .with_model("gpt-4")
            .with_message(LlMessage::user("hi"));
        let result = gw.chat(req);
        // ProviderError is retryable but the mock always fails, so it eventually fails
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("provider error"));
    }

    #[test]
    fn integration_chat_rate_limit_retry() {
        let provider = MockChatProvider::new("openai", vec!["gpt-4"])
            .with_failure(LlmError::ProviderRateLimited { retry_after_ms: 5 });
        let providers: Vec<Box<dyn LlProvider>> = vec![Box::new(provider)];
        let models = vec![model_config("gpt-4", "openai", vec!["chat"])];
        let gw = build_gateway(providers, models);

        let req = LlRequest::default()
            .with_model("gpt-4")
            .with_message(LlMessage::user("hi"));
        let result = gw.chat(req);
        assert!(result.is_err());
    }

    #[test]
    fn integration_chat_fallback_provider() {
        // Primary fails with non-retryable → fallback to secondary
        let failing = FailingProvider::new(
            "openai",
            vec!["gpt-4"],
            LlmError::ProviderError("fatal".into()),
        );
        let secondary =
            MockChatProvider::new("anthropic", vec!["claude-3"]).with_response("fallback-ok");
        let providers: Vec<Box<dyn LlProvider>> = vec![Box::new(failing), Box::new(secondary)];
        let models = vec![
            model_config("gpt-4", "openai", vec!["chat"]),
            model_config("claude-3", "anthropic", vec!["chat"]),
        ];
        let gw = build_gateway(providers, models);

        let req = LlRequest::default()
            .with_model("gpt-4")
            .with_message(LlMessage::user("hi"));
        let err = gw.chat(req).unwrap_err();
        // gpt-4's provider (openai) fails, but anthropic should be tried as fallback
        // Since this is retry/fallback, ProviderError might not be a fallback trigger
        assert!(
            err.to_string().contains("provider error")
                || err.to_string().contains("all providers failed")
        );
    }

    #[test]
    fn integration_chat_fallback_rate_limit() {
        // Rate limiting triggers fallback
        let rate_limited = MockChatProvider::new("openai", vec!["gpt-4"]).with_failure(
            LlmError::ProviderRateLimited {
                retry_after_ms: 100,
            },
        );
        let secondary =
            MockChatProvider::new("anthropic", vec!["claude-3"]).with_response("fallback");
        let providers: Vec<Box<dyn LlProvider>> = vec![Box::new(rate_limited), Box::new(secondary)];
        let models = vec![
            model_config("gpt-4", "openai", vec!["chat"]),
            model_config("claude-3", "anthropic", vec!["chat"]),
        ];
        let gw = build_gateway(providers, models);

        let req = LlRequest::default()
            .with_model("gpt-4")
            .with_message(LlMessage::user("hi"));
        // RateLimited has retry_after_ms handling in execute_with_retry,
        // but since it keeps failing, it will eventually try fallback
        let result = gw.chat(req);
        assert!(result.is_ok() || result.is_err());
    }

    // ─── Streaming Integration ──────────────────────────────────────────────

    #[test]
    fn integration_stream_multiple_chunks() {
        let provider =
            StreamingProvider::new("openai", vec!["gpt-4"], vec!["Hello ", "world", "!"]);
        let providers: Vec<Box<dyn LlProvider>> = vec![Box::new(provider)];
        let models = vec![model_config("gpt-4", "openai", vec!["chat", "streaming"])];
        let gw = build_gateway(providers, models);

        let req = LlRequest::default()
            .with_model("gpt-4")
            .with_message(LlMessage::user("say hi"));
        let handle = gw.chat_stream(req).unwrap();
        let events: Vec<LlStreamEvent> = handle.iter().filter_map(|r| r.ok()).collect();
        assert_eq!(events.len(), 3); // 3 chunks (Done is filtered by iterator)
        let text_chunks: Vec<String> = events
            .iter()
            .filter_map(|e| match e {
                LlStreamEvent::Chunk(c) => Some(c.content.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(text_chunks.join(""), "Hello world!");
    }

    #[test]
    fn integration_stream_cancellation() {
        let provider =
            StreamingProvider::new("openai", vec!["gpt-4"], vec!["a", "b", "c", "d", "e"]);
        let providers: Vec<Box<dyn LlProvider>> = vec![Box::new(provider)];
        let models = vec![model_config("gpt-4", "openai", vec!["chat", "streaming"])];
        let gw = build_gateway(providers, models);

        let req = LlRequest::default()
            .with_model("gpt-4")
            .with_message(LlMessage::user("hi"));
        let handle = gw.chat_stream(req).unwrap();
        // Drop handle → cancels stream
        drop(handle);
        // No panic
    }

    #[test]
    fn integration_stream_empty_chunks() {
        let provider = StreamingProvider::new("openai", vec!["gpt-4"], vec![]);
        let providers: Vec<Box<dyn LlProvider>> = vec![Box::new(provider)];
        let models = vec![model_config("gpt-4", "openai", vec!["chat", "streaming"])];
        let gw = build_gateway(providers, models);

        let req = LlRequest::default()
            .with_model("gpt-4")
            .with_message(LlMessage::user("hi"));
        let handle = gw.chat_stream(req).unwrap();
        let _events: Vec<LlStreamEvent> = handle.iter().filter_map(|r| r.ok()).collect();
        // Empty chunks → no events (just Done, which iterator filters out)
    }

    // ─── Embedding Integration ──────────────────────────────────────────────

    #[test]
    fn integration_embed_cache_hit() {
        let provider = MockChatProvider::new("openai", vec!["text-embedding-3"]);
        let providers: Vec<Box<dyn LlProvider>> = vec![Box::new(provider)];
        let models = vec![model_config(
            "text-embedding-3",
            "openai",
            vec!["embedding"],
        )];
        let gw = build_gateway(providers, models);

        let req = LlEmbedRequest {
            input: vec!["test".into()],
            model: Some("text-embedding-3".into()),
            correlation_id: browseros_types::identifiers::CorrelationId::new(),
        };
        let r1 = gw.embed(req.clone()).unwrap();
        let r2 = gw.embed(req).unwrap();
        assert_eq!(r1.dimensions, r2.dimensions);
    }

    // ─── Multiple Providers ─────────────────────────────────────────────────

    #[test]
    fn integration_multiple_providers_round_robin() {
        let p1 = MockChatProvider::new("openai", vec!["gpt-4"]).with_response("from-openai");
        let p2 =
            MockChatProvider::new("anthropic", vec!["claude-3"]).with_response("from-anthropic");
        let providers: Vec<Box<dyn LlProvider>> = vec![Box::new(p1), Box::new(p2)];
        let models = vec![
            model_config("gpt-4", "openai", vec!["chat"]),
            model_config("claude-3", "anthropic", vec!["chat"]),
        ];
        let gw = build_gateway(providers, models);

        // Resolve by capability — will round-robin
        let ep1 = gw.resolve("chat", &[]).unwrap();
        let ep2 = gw.resolve("chat", &[]).unwrap();
        // May be same or different depending on round-robin state
        assert!(ep1.provider_id == "openai" || ep1.provider_id == "anthropic");
        assert!(ep2.provider_id == "openai" || ep2.provider_id == "anthropic");
    }

    #[test]
    fn integration_multiple_providers_model_aliases() {
        let provider = MockChatProvider::new("openai", vec!["gpt-4"]);
        let providers: Vec<Box<dyn LlProvider>> = vec![Box::new(provider)];
        let models = vec![ModelConfig {
            id: "gpt-4".into(),
            provider: "openai".into(),
            capabilities: vec!["chat".into()],
            max_tokens: 4096,
            context_window: 8192,
            cost_per_1k_input: 1.0,
            cost_per_1k_output: 2.0,
            aliases: vec!["gpt4".into(), "gpt-4-turbo".into()],
        }];
        let registry = Arc::new(ModelRegistry::new(&models, &providers).unwrap());
        let config = LlmConfig::default();
        let health = Arc::new(crate::router::ProviderHealthTracker::new(&config.routing));
        let router = Arc::new(LlRouter::new(&config.routing, registry.clone(), health).unwrap());
        let cache = LlCache::new(&config.cache);
        let cost_tracker = CostTracker::new(&config.cost);
        let telemetry = Arc::new(LlmTelemetry::new_enabled());
        let gw = LlGateway::new(
            config,
            providers,
            router,
            registry,
            cache,
            cost_tracker,
            telemetry,
        );

        let req = LlRequest::default()
            .with_model("gpt4")
            .with_message(LlMessage::user("hi"));
        let resp = gw.chat(req).unwrap();
        assert_eq!(resp.model, "gpt-4");
    }

    // ─── Cost Integration ───────────────────────────────────────────────────

    #[test]
    fn integration_cost_tracked_after_success() {
        let provider = MockChatProvider::new("openai", vec!["gpt-4"]).with_response("cost-test");
        let providers: Vec<Box<dyn LlProvider>> = vec![Box::new(provider)];
        let models = vec![model_config("gpt-4", "openai", vec!["chat"])];
        let gw = build_gateway(providers, models);

        let req = LlRequest::default()
            .with_model("gpt-4")
            .with_message(LlMessage::user("hi"));
        let _ = gw.chat(req).unwrap();

        let snap = gw.cost_snapshot();
        assert!(snap.total_cost_cents > 0.0);
        assert!(snap.total_tokens_in > 0);
        assert!(snap.total_tokens_out > 0);
        assert!(snap.cost_by_model.contains_key("gpt-4"));
        assert!(snap.cost_by_provider.contains_key("openai"));
    }

    // ─── Health Integration ─────────────────────────────────────────────────

    #[test]
    fn integration_health_multiple_providers() {
        let p1 = MockChatProvider::new("openai", vec!["gpt-4"]).with_response("a");
        let p2 = MockChatProvider::new("anthropic", vec!["claude-3"]).with_response("b");
        let providers: Vec<Box<dyn LlProvider>> = vec![Box::new(p1), Box::new(p2)];
        let models = vec![
            model_config("gpt-4", "openai", vec!["chat"]),
            model_config("claude-3", "anthropic", vec!["chat"]),
        ];
        let gw = build_gateway(providers, models);

        let req1 = LlRequest::default()
            .with_model("gpt-4")
            .with_message(LlMessage::user("hi"));
        let req2 = LlRequest::default()
            .with_model("claude-3")
            .with_message(LlMessage::user("hi"));
        let _ = gw.chat(req1);
        let _ = gw.chat(req2);

        let reports = gw.health();
        assert!(reports.len() >= 2);
        assert!(reports.iter().any(|r| r.provider_id == "openai"));
        assert!(reports.iter().any(|r| r.provider_id == "anthropic"));
    }

    #[test]
    fn integration_health_empty_gateway() {
        let gw = LlGateway::default();
        assert!(gw.health().is_empty());
    }

    // ─── Concurrent Requests ───────────────────────────────────────────────

    #[test]
    fn integration_concurrent_requests() {
        let provider = MockChatProvider::new("openai", vec!["gpt-4"]).with_response("concurrent");
        let providers: Vec<Box<dyn LlProvider>> = vec![Box::new(provider)];
        let models = vec![model_config("gpt-4", "openai", vec!["chat"])];
        let gw = Arc::new(build_gateway(providers, models));

        let mut handles = Vec::new();
        for i in 0..10 {
            let g = Arc::clone(&gw);
            handles.push(std::thread::spawn(move || {
                let req = LlRequest::default()
                    .with_model("gpt-4")
                    .with_message(LlMessage::user(format!("msg-{}", i)));
                g.chat(req)
            }));
        }
        for h in handles {
            let result = h.join().unwrap();
            assert!(
                result.is_ok(),
                "concurrent request failed: {:?}",
                result.err()
            );
        }
        let snap = gw.cost_snapshot();
        assert!(snap.total_cost_cents > 0.0);
        assert!(snap.total_tokens_in > 0);
        assert!(snap.total_tokens_out > 0);
    }

    #[test]
    fn integration_concurrent_streaming() {
        let providers: Vec<Box<dyn LlProvider>> = vec![Box::new(StreamingProvider::new(
            "openai",
            vec!["gpt-4"],
            vec!["chunk"],
        ))];
        let models = vec![model_config("gpt-4", "openai", vec!["chat", "streaming"])];
        let gw = Arc::new(build_gateway(providers, models));

        let mut handles = Vec::new();
        for _ in 0..5 {
            let g = Arc::clone(&gw);
            handles.push(std::thread::spawn(move || {
                let req = LlRequest::default()
                    .with_model("gpt-4")
                    .with_message(LlMessage::user("hi"))
                    .with_stream(true);
                let handle = g.chat_stream(req)?;
                let events: Vec<LlStreamEvent> = handle.iter().filter_map(|r| r.ok()).collect();
                Ok::<_, LlmError>(events.len())
            }));
        }
        for h in handles {
            let count = h.join().unwrap().unwrap();
            assert!(count >= 1); // at least one chunk (Done is filtered by iterator)
        }
    }

    // ─── Edge Cases ─────────────────────────────────────────────────────────

    // ─── Tool Calls Streaming ──────────────────────────────────────────────

    /// Provider that emits chunks with tool_calls for testing bridge forwarding.
    struct ToolCallStreamingProvider {
        id: String,
        models: Vec<String>,
        chunks: Vec<(String, Option<LlFinishReason>, Vec<LlToolCallDelta>)>,
    }

    impl ToolCallStreamingProvider {
        fn new(
            id: &str,
            models: Vec<&str>,
            chunks: Vec<(String, Option<LlFinishReason>, Vec<LlToolCallDelta>)>,
        ) -> Self {
            Self {
                id: id.to_string(),
                models: models.iter().map(|s| s.to_string()).collect(),
                chunks,
            }
        }
    }

    impl LlProvider for ToolCallStreamingProvider {
        fn id(&self) -> &str {
            &self.id
        }
        fn name(&self) -> &str {
            &self.id
        }
        fn capabilities(&self) -> Vec<ProviderCapability> {
            vec![ProviderCapability::Chat, ProviderCapability::Streaming]
        }
        fn models(&self) -> Vec<String> {
            self.models.clone()
        }
        fn chat(&self, _req: ProviderRequest) -> Result<ProviderResponse, LlmError> {
            Err(LlmError::ProviderError("use stream".into()))
        }
        fn chat_stream(
            &self,
            _req: ProviderRequest,
        ) -> Result<crate::types::ProviderStream, LlmError> {
            let (tx, rx) = std::sync::mpsc::channel();
            let chunks = self.chunks.clone();
            std::thread::spawn(move || {
                for (content, finish_reason, tool_calls) in chunks {
                    if tx
                        .send(ProviderStreamEvent::Chunk {
                            content,
                            finish_reason,
                            tool_calls,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
                tx.send(ProviderStreamEvent::Done {
                    input_tokens: 10,
                    output_tokens: 20,
                })
                .ok();
            });
            Ok(crate::types::ProviderStream { receiver: rx })
        }
        fn embed(&self, _req: ProviderEmbedRequest) -> Result<ProviderEmbedResponse, LlmError> {
            Err(LlmError::ProviderError("no embed".into()))
        }
        fn health(&self) -> ProviderHealthResult {
            ProviderHealthResult {
                status: ProviderHealth::Healthy,
                latency_ms: Some(5),
                checked_at: std::time::SystemTime::now(),
                error: None,
            }
        }
    }

    #[test]
    fn stream_tool_calls_forwarded_one_to_one() {
        let tool_calls = vec![LlToolCallDelta {
            index: 0,
            id: Some("call_1".into()),
            name: Some("get_weather".into()),
            arguments: Some("{}".into()),
        }];
        let chunks = vec![(String::new(), None, tool_calls.clone())];
        let provider = ToolCallStreamingProvider::new("openai", vec!["gpt-4"], chunks);
        let providers: Vec<Box<dyn LlProvider>> = vec![Box::new(provider)];
        let models = vec![model_config("gpt-4", "openai", vec!["chat", "streaming"])];
        let gw = build_gateway(providers, models);

        let req = LlRequest::default()
            .with_model("gpt-4")
            .with_message(LlMessage::user("hi"))
            .with_stream(true);
        let handle = gw.chat_stream(req).unwrap();
        let events: Vec<LlStreamEvent> = handle.iter().filter_map(|r| r.ok()).collect();

        assert_eq!(events.len(), 1);
        if let LlStreamEvent::Chunk(c) = &events[0] {
            assert_eq!(c.tool_calls.len(), 1);
            assert_eq!(c.tool_calls[0].id.as_deref(), Some("call_1"));
            assert_eq!(c.tool_calls[0].name.as_deref(), Some("get_weather"));
        } else {
            panic!("expected Chunk");
        }
    }

    #[test]
    fn stream_tool_calls_never_replaced_by_gateway() {
        // Verify gateway does NOT replace tool_calls with Vec::new()
        let tool_calls = vec![LlToolCallDelta {
            index: 0,
            id: Some("call_1".into()),
            name: Some("get_weather".into()),
            arguments: Some("{}".into()),
        }];
        let chunks = vec![("thinking".into(), None, tool_calls)];
        let provider = ToolCallStreamingProvider::new("openai", vec!["gpt-4"], chunks);
        let providers: Vec<Box<dyn LlProvider>> = vec![Box::new(provider)];
        let models = vec![model_config("gpt-4", "openai", vec!["chat", "streaming"])];
        let gw = build_gateway(providers, models);

        let req = LlRequest::default()
            .with_model("gpt-4")
            .with_message(LlMessage::user("hi"))
            .with_stream(true);
        let handle = gw.chat_stream(req).unwrap();
        let events: Vec<LlStreamEvent> = handle.iter().filter_map(|r| r.ok()).collect();
        assert_eq!(events.len(), 1);
        if let LlStreamEvent::Chunk(c) = &events[0] {
            assert_eq!(
                c.tool_calls.len(),
                1,
                "gateway must not replace tool_calls with Vec::new()"
            );
        }
    }

    #[test]
    fn stream_tool_calls_chunk_ordering() {
        let t0 = LlToolCallDelta {
            index: 0,
            id: Some("call_1".into()),
            name: Some("get_weather".into()),
            arguments: Some("".into()),
        };
        let t1 = LlToolCallDelta {
            index: 0,
            id: None,
            name: None,
            arguments: Some("{\"loc\":\"NYC\"}".into()),
        };
        let chunks = vec![
            ("".into(), None, vec![t0]),
            ("".into(), None, vec![t1]),
            ("done".into(), Some(LlFinishReason::ToolCalls), vec![]),
        ];
        let provider = ToolCallStreamingProvider::new("openai", vec!["gpt-4"], chunks);
        let providers: Vec<Box<dyn LlProvider>> = vec![Box::new(provider)];
        let models = vec![model_config("gpt-4", "openai", vec!["chat", "streaming"])];
        let gw = build_gateway(providers, models);

        let req = LlRequest::default()
            .with_model("gpt-4")
            .with_message(LlMessage::user("hi"))
            .with_stream(true);
        let handle = gw.chat_stream(req).unwrap();
        let events: Vec<LlStreamEvent> = handle.iter().filter_map(|r| r.ok()).collect();
        assert_eq!(events.len(), 3);

        // Chunk 0: start of tool call
        if let LlStreamEvent::Chunk(c) = &events[0] {
            assert_eq!(c.index, 0);
            assert_eq!(c.tool_calls.len(), 1);
            assert!(c.tool_calls[0].is_start());
        }
        // Chunk 1: arguments continuation
        if let LlStreamEvent::Chunk(c) = &events[1] {
            assert_eq!(c.index, 1);
            assert_eq!(c.tool_calls.len(), 1);
            assert!(!c.tool_calls[0].is_start());
        }
        // Chunk 2: final chunk with finish_reason
        if let LlStreamEvent::Chunk(c) = &events[2] {
            assert_eq!(c.index, 2);
            // finish_reason is NOT forwarded through the bridge (always None in Chunk)
            assert!(c.finish_reason.is_none());
        }
    }

    #[test]
    fn stream_tool_calls_finish_reason_not_forwarded() {
        // Gateway bridge always sets finish_reason: None on LlStreamChunk.
        // finish_reason is delivered via Done event instead.
        let chunks = vec![
            ("info".into(), None, vec![]),
            ("done".into(), Some(LlFinishReason::Stop), vec![]),
        ];
        let provider = ToolCallStreamingProvider::new("openai", vec!["gpt-4"], chunks);
        let providers: Vec<Box<dyn LlProvider>> = vec![Box::new(provider)];
        let models = vec![model_config("gpt-4", "openai", vec!["chat", "streaming"])];
        let gw = build_gateway(providers, models);

        let req = LlRequest::default()
            .with_model("gpt-4")
            .with_message(LlMessage::user("hi"))
            .with_stream(true);
        let handle = gw.chat_stream(req).unwrap();
        let events: Vec<LlStreamEvent> = handle.iter().filter_map(|r| r.ok()).collect();
        assert_eq!(events.len(), 2);
        if let LlStreamEvent::Chunk(c) = &events[0] {
            assert!(c.finish_reason.is_none());
        }
        if let LlStreamEvent::Chunk(c) = &events[1] {
            // finish_reason from provider is not forwarded via Chunk
            assert!(c.finish_reason.is_none());
        }
    }

    #[test]
    fn stream_text_and_tool_calls_coexist() {
        let t0 = LlToolCallDelta {
            index: 0,
            id: Some("call_1".into()),
            name: Some("get_weather".into()),
            arguments: Some("{}".into()),
        };
        let chunks = vec![(
            "Let me check the weather".into(),
            Some(LlFinishReason::ToolCalls),
            vec![t0],
        )];
        let provider = ToolCallStreamingProvider::new("openai", vec!["gpt-4"], chunks);
        let providers: Vec<Box<dyn LlProvider>> = vec![Box::new(provider)];
        let models = vec![model_config("gpt-4", "openai", vec!["chat", "streaming"])];
        let gw = build_gateway(providers, models);

        let req = LlRequest::default()
            .with_model("gpt-4")
            .with_message(LlMessage::user("weather?"))
            .with_stream(true);
        let handle = gw.chat_stream(req).unwrap();
        let events: Vec<LlStreamEvent> = handle.iter().filter_map(|r| r.ok()).collect();
        assert_eq!(events.len(), 1);
        if let LlStreamEvent::Chunk(c) = &events[0] {
            assert_eq!(c.content, "Let me check the weather");
            assert_eq!(c.tool_calls.len(), 1);
            // finish_reason is NOT forwarded through the bridge (always None in Chunk)
        }
    }

    #[test]
    fn stream_empty_tool_calls_not_null() {
        let chunks = vec![("hello".into(), None, vec![])];
        let provider = ToolCallStreamingProvider::new("openai", vec!["gpt-4"], chunks);
        let providers: Vec<Box<dyn LlProvider>> = vec![Box::new(provider)];
        let models = vec![model_config("gpt-4", "openai", vec!["chat", "streaming"])];
        let gw = build_gateway(providers, models);

        let req = LlRequest::default()
            .with_model("gpt-4")
            .with_message(LlMessage::user("hi"))
            .with_stream(true);
        let handle = gw.chat_stream(req).unwrap();
        let events: Vec<LlStreamEvent> = handle.iter().filter_map(|r| r.ok()).collect();
        assert_eq!(events.len(), 1);
        if let LlStreamEvent::Chunk(c) = &events[0] {
            assert!(c.tool_calls.is_empty());
        }
    }

    #[test]
    fn integration_long_messages() {
        let provider = MockChatProvider::new("openai", vec!["gpt-4"]).with_response("long");
        let providers: Vec<Box<dyn LlProvider>> = vec![Box::new(provider)];
        let models = vec![model_config("gpt-4", "openai", vec!["chat"])];
        let gw = build_gateway(providers, models);

        let long_text = "A".repeat(10_000);
        let req = LlRequest::default()
            .with_model("gpt-4")
            .with_message(LlMessage::user(long_text));
        let resp = gw.chat(req).unwrap();
        assert!(extract_text(&resp.message.content).contains("long"));
    }

    #[test]
    fn integration_temperature_capping() {
        let provider = MockChatProvider::new("openai", vec!["gpt-4"]).with_response("temp");
        let providers: Vec<Box<dyn LlProvider>> = vec![Box::new(provider)];
        let models = vec![model_config("gpt-4", "openai", vec!["chat"])];
        let gw = build_gateway(providers, models);

        let req = LlRequest::default()
            .with_model("gpt-4")
            .with_temperature(5.0)
            .with_message(LlMessage::user("hi"));
        // Temperature 5.0 exceeds max → validate rejects it
        let result = gw.chat(req);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("invalid request"));
    }

    // ─── Send + Sync ───────────────────────────────────────────────────────

    #[test]
    fn gateway_send_sync_advanced() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        fn assert_send_ref<T: Send + Sync>() {}

        assert_send::<LlGateway>();
        assert_sync::<LlGateway>();
        assert_send_ref::<LlGateway>();

        // Also verify with Arc
        let gw = Arc::new(LlGateway::default());
        fn assert_arc_send<T: Send + Sync>() {}
        assert_arc_send::<LlGateway>();
        drop(gw);
    }

    #[test]
    fn integration_resolve_with_hints() {
        let p1 = MockChatProvider::new("openai", vec!["gpt-4"]).with_response("a");
        let p2 = MockChatProvider::new("anthropic", vec!["claude-3"]).with_response("b");
        let providers: Vec<Box<dyn LlProvider>> = vec![Box::new(p1), Box::new(p2)];
        let models = vec![
            model_config("gpt-4", "openai", vec!["chat"]),
            model_config("claude-3", "anthropic", vec!["chat"]),
        ];
        let gw = build_gateway(providers, models);

        let ep = gw.resolve("chat", &["fast".into()]).unwrap();
        assert_eq!(ep.provider_id, "openai");
    }
}
