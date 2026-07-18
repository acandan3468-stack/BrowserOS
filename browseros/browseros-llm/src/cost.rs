//! Cost tracking — token counting, cost estimation, budget monitoring, and accounting.

use crate::error::LlmError;
use crate::provider::round_to_millicents;
use crate::types::CostConfig;
use std::collections::HashMap;
use std::sync::Mutex;

// ─────────────────────────────────────────────────────────────────────────────
// Data types
// ─────────────────────────────────────────────────────────────────────────────

/// Cost breakdown for a single provider.
#[derive(Debug, Clone, Default)]
pub struct ProviderCostRow {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_cents: f64,
    pub request_count: u64,
}

/// Cost breakdown for a single model.
#[derive(Debug, Clone, Default)]
pub struct ModelCostRow {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_cents: f64,
    pub request_count: u64,
}

/// A point-in-time snapshot of cost tracker state.
#[derive(Debug, Clone)]
pub struct CostTrackerSnapshot {
    pub total_cost_cents: f64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_tokens: u64,
    pub total_requests: u64,
    pub session_cost_cents: f64,
    pub session_input_tokens: u64,
    pub session_output_tokens: u64,
    pub monthly_cost_cents: f64,
    pub monthly_input_tokens: u64,
    pub monthly_output_tokens: u64,
    pub budget_cents: Option<u64>,
    pub budget_remaining_cents: Option<u64>,
    pub budget_exceeded: bool,
    pub alert_threshold: Option<f64>,
    pub alert_triggered: bool,
    pub cache_savings_cents: f64,
    pub fallback_count: u64,
    pub by_provider: HashMap<String, ProviderCostRow>,
    pub by_model: HashMap<String, ModelCostRow>,
}

// ─────────────────────────────────────────────────────────────────────────────
// CostTracker
// ─────────────────────────────────────────────────────────────────────────────

/// Tracks LLM API usage, costs, and budgets.
///
/// Thread-safe via internal Mutex. Accumulates provider-level and model-level
/// accounting with millicent precision to avoid floating-point drift.
pub struct CostTracker {
    inner: Mutex<CostInner>,
    config: CostConfig,
}

struct CostInner {
    provider_costs: HashMap<String, ProviderCostRow>,
    model_costs: HashMap<String, ModelCostRow>,
    // Session-level counters (reset on demand)
    session_input: u64,
    session_output: u64,
    session_cost: f64,
    // Monthly-level counters (monotonically increasing within a session)
    monthly_input: u64,
    monthly_output: u64,
    monthly_cost: f64,
    // Global counters
    total_requests: u64,
    cache_savings: f64,
    fallback_count: u64,
}

impl CostTracker {
    /// Create a new cost tracker with the given configuration.
    pub fn new(config: &CostConfig) -> Self {
        Self {
            inner: Mutex::new(CostInner {
                provider_costs: HashMap::new(),
                model_costs: HashMap::new(),
                session_input: 0,
                session_output: 0,
                session_cost: 0.0,
                monthly_input: 0,
                monthly_output: 0,
                monthly_cost: 0.0,
                total_requests: 0,
                cache_savings: 0.0,
                fallback_count: 0,
            }),
            config: config.clone(),
        }
    }
}

impl Default for CostTracker {
    fn default() -> Self {
        Self::new(&CostConfig::default())
    }
}

impl CostTracker {
    /// Return a copy of the current configuration.
    pub fn config(&self) -> CostConfig {
        self.config.clone()
    }

    /// Estimate the cost in cents for a given token count and model rates.
    ///
    /// Uses millicent rounding to avoid floating-point drift.
    /// Returns (cost_cents, input_tokens, output_tokens).
    pub fn estimate(
        &self,
        input_tokens: u64,
        output_tokens: u64,
        cost_per_1k_input: f64,
        cost_per_1k_output: f64,
    ) -> (f64, u64, u64) {
        let cost = (input_tokens as f64 / 1000.0 * cost_per_1k_input)
            + (output_tokens as f64 / 1000.0 * cost_per_1k_output);
        (round_to_millicents(cost), input_tokens, output_tokens)
    }

    /// Record actual usage after a successful API call.
    ///
    /// Updates provider, model, session, and monthly counters.
    pub fn record(
        &self,
        provider: &str,
        model: &str,
        input_tokens: u64,
        output_tokens: u64,
        cost_cents: f64,
        is_fallback: bool,
    ) -> Result<(), LlmError> {
        let cost = round_to_millicents(cost_cents);
        let mut inner = self
            .inner
            .lock()
            .map_err(|e| LlmError::CacheError(format!("cost tracker lock poisoned: {}", e)))?;

        // Provider-level
        let p_row = inner
            .provider_costs
            .entry(provider.to_string())
            .or_default();
        p_row.input_tokens = p_row.input_tokens.saturating_add(input_tokens);
        p_row.output_tokens = p_row.output_tokens.saturating_add(output_tokens);
        p_row.cost_cents = round_to_millicents(p_row.cost_cents + cost);
        p_row.request_count += 1;

        // Model-level
        let m_row = inner.model_costs.entry(model.to_string()).or_default();
        m_row.input_tokens = m_row.input_tokens.saturating_add(input_tokens);
        m_row.output_tokens = m_row.output_tokens.saturating_add(output_tokens);
        m_row.cost_cents = round_to_millicents(m_row.cost_cents + cost);
        m_row.request_count += 1;

        // Session
        inner.session_input = inner.session_input.saturating_add(input_tokens);
        inner.session_output = inner.session_output.saturating_add(output_tokens);
        inner.session_cost = round_to_millicents(inner.session_cost + cost);

        // Monthly
        inner.monthly_input = inner.monthly_input.saturating_add(input_tokens);
        inner.monthly_output = inner.monthly_output.saturating_add(output_tokens);
        inner.monthly_cost = round_to_millicents(inner.monthly_cost + cost);

        // Global
        inner.total_requests += 1;
        if is_fallback {
            inner.fallback_count += 1;
        }

        Ok(())
    }

    /// Record a cache saving (cost that was avoided by a cache hit).
    pub fn record_cache_saving(&self, cost_cents: f64) -> Result<(), LlmError> {
        let cost = round_to_millicents(cost_cents);
        let mut inner = self
            .inner
            .lock()
            .map_err(|e| LlmError::CacheError(format!("cost tracker lock poisoned: {}", e)))?;
        inner.cache_savings = round_to_millicents(inner.cache_savings + cost);
        Ok(())
    }

    /// Generate a point-in-time snapshot of all tracked costs.
    pub fn snapshot(&self) -> Result<CostTrackerSnapshot, LlmError> {
        let inner = self
            .inner
            .lock()
            .map_err(|e| LlmError::CacheError(format!("cost tracker lock poisoned: {}", e)))?;
        let total_cost = inner.session_cost; // Use session as primary total
        let budget = self.config.budget_monthly_cents;
        let remaining = budget.map(|b| {
            let spent = (inner.monthly_cost * 1000.0).round() as u64;
            b.saturating_sub(spent / 1000)
        });
        let threshold = self.config.alert_threshold;
        let exceeded = budget.is_some_and(|b| {
            let monthly_millicents = (inner.monthly_cost * 1000.0).round() as u64;
            let budget_millicents = b * 1000;
            monthly_millicents >= budget_millicents
        });
        let alerted = threshold
            .and_then(|t| {
                budget.map(|b| {
                    let monthly_millicents = (inner.monthly_cost * 1000.0).round() as u64;
                    let budget_millicents = b * 1000;
                    let alert_millicents = (budget_millicents as f64 * t) as u64;
                    monthly_millicents >= alert_millicents
                })
            })
            .unwrap_or(false);

        Ok(CostTrackerSnapshot {
            total_cost_cents: round_to_millicents(total_cost),
            total_input_tokens: inner.session_input,
            total_output_tokens: inner.session_output,
            total_tokens: inner.session_input.saturating_add(inner.session_output),
            total_requests: inner.total_requests,
            session_cost_cents: round_to_millicents(inner.session_cost),
            session_input_tokens: inner.session_input,
            session_output_tokens: inner.session_output,
            monthly_cost_cents: round_to_millicents(inner.monthly_cost),
            monthly_input_tokens: inner.monthly_input,
            monthly_output_tokens: inner.monthly_output,
            budget_cents: budget,
            budget_remaining_cents: remaining,
            budget_exceeded: exceeded,
            alert_threshold: threshold,
            alert_triggered: alerted,
            cache_savings_cents: round_to_millicents(inner.cache_savings),
            fallback_count: inner.fallback_count,
            by_provider: inner.provider_costs.clone(),
            by_model: inner.model_costs.clone(),
        })
    }

    /// Reset session-level counters.
    ///
    /// Monthly counters persist across session resets.
    pub fn reset_session(&self) -> Result<(), LlmError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|e| LlmError::CacheError(format!("cost tracker lock poisoned: {}", e)))?;
        inner.session_input = 0;
        inner.session_output = 0;
        inner.session_cost = 0.0;
        inner.provider_costs.clear();
        inner.model_costs.clear();
        inner.total_requests = 0;
        inner.cache_savings = 0.0;
        inner.fallback_count = 0;
        Ok(())
    }

    /// Check whether the budget has been exceeded or the alert threshold triggered.
    ///
    /// Returns (budget_exceeded, alert_triggered, budget_remaining_cents).
    pub fn check_budget(&self) -> Result<(bool, bool, Option<u64>), LlmError> {
        let inner = self
            .inner
            .lock()
            .map_err(|e| LlmError::CacheError(format!("cost tracker lock poisoned: {}", e)))?;
        let budget = self.config.budget_monthly_cents;
        let monthly_millicents = (inner.monthly_cost * 1000.0).round() as u64;
        let budget_millicents = budget.map(|b| b * 1000);

        let exceeded = budget_millicents
            .map(|bm| monthly_millicents >= bm)
            .unwrap_or(false);

        let triggered = self
            .config
            .alert_threshold
            .and_then(|t| {
                budget_millicents.map(|bm| {
                    let alert_at = (bm as f64 * t) as u64;
                    monthly_millicents >= alert_at
                })
            })
            .unwrap_or(false);

        let remaining = budget.map(|b| {
            let spent_millicents = (inner.monthly_cost * 1000.0).round() as u64;
            let budget_millicents = b * 1000;
            let rem = budget_millicents.saturating_sub(spent_millicents);
            rem / 1000 // Convert back to cents
        });

        Ok((exceeded, triggered, remaining))
    }

    /// Return the current total monthly cost in cents.
    pub fn monthly_cost_cents(&self) -> Result<f64, LlmError> {
        let inner = self
            .inner
            .lock()
            .map_err(|e| LlmError::CacheError(format!("cost tracker lock poisoned: {}", e)))?;
        Ok(round_to_millicents(inner.monthly_cost))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Send + Sync safety
// ─────────────────────────────────────────────────────────────────────────────

fn _assert_send_sync()
where
    CostTracker: Send + Sync,
{
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> CostConfig {
        CostConfig {
            budget_monthly_cents: None,
            alert_threshold: None,
        }
    }

    fn budget_config(budget: u64, threshold: Option<f64>) -> CostConfig {
        CostConfig {
            budget_monthly_cents: Some(budget),
            alert_threshold: threshold,
        }
    }

    #[test]
    fn estimate_zero_tokens() {
        let tracker = CostTracker::default();
        let (cost, input, output) = tracker.estimate(0, 0, 1.0, 2.0);
        assert_eq!(cost, 0.0);
        assert_eq!(input, 0);
        assert_eq!(output, 0);
    }

    #[test]
    fn estimate_basic() {
        let tracker = CostTracker::default();
        let (cost, input, output) = tracker.estimate(1000, 500, 1.0, 2.0);
        // 1000/1000 * 1.0 + 500/1000 * 2.0 = 1.0 + 1.0 = 2.0
        assert!((cost - 2.0).abs() < 0.001);
        assert_eq!(input, 1000);
        assert_eq!(output, 500);
    }

    #[test]
    fn estimate_rounding() {
        let tracker = CostTracker::default();
        let (cost, _, _) = tracker.estimate(1, 1, 0.333, 0.666);
        assert!((cost * 1000.0).fract() < 0.001 || (cost * 1000.0).fract() > 0.999);
    }

    #[test]
    fn record_increases_counter() {
        let tracker = CostTracker::new(&default_config());
        tracker
            .record("openai", "gpt-4", 100, 50, 0.15, false)
            .unwrap();
        let snap = tracker.snapshot().unwrap();
        assert_eq!(snap.total_requests, 1);
        assert_eq!(snap.total_input_tokens, 100);
        assert_eq!(snap.total_output_tokens, 50);
        assert!((snap.total_cost_cents - 0.15).abs() < 0.001);
    }

    #[test]
    fn record_multiple_providers() {
        let tracker = CostTracker::new(&default_config());
        tracker
            .record("openai", "gpt-4", 100, 50, 0.15, false)
            .unwrap();
        tracker
            .record("anthropic", "claude-3", 200, 100, 0.30, false)
            .unwrap();
        let snap = tracker.snapshot().unwrap();
        assert_eq!(snap.by_provider.len(), 2);
        assert_eq!(snap.by_provider.get("openai").unwrap().request_count, 1);
        assert_eq!(snap.by_provider.get("anthropic").unwrap().request_count, 1);
    }

    #[test]
    fn record_model_accounting() {
        let tracker = CostTracker::new(&default_config());
        tracker
            .record("openai", "gpt-4", 100, 50, 0.15, false)
            .unwrap();
        let snap = tracker.snapshot().unwrap();
        let m_row = snap.by_model.get("gpt-4").unwrap();
        assert_eq!(m_row.input_tokens, 100);
        assert_eq!(m_row.output_tokens, 50);
    }

    #[test]
    fn record_fallback_counted() {
        let tracker = CostTracker::new(&default_config());
        tracker
            .record("openai", "gpt-4", 10, 5, 0.01, true)
            .unwrap();
        let snap = tracker.snapshot().unwrap();
        assert_eq!(snap.fallback_count, 1);
    }

    #[test]
    fn record_non_fallback_not_counted() {
        let tracker = CostTracker::new(&default_config());
        tracker
            .record("openai", "gpt-4", 10, 5, 0.01, false)
            .unwrap();
        let snap = tracker.snapshot().unwrap();
        assert_eq!(snap.fallback_count, 0);
    }

    #[test]
    fn session_reset_clears_session() {
        let tracker = CostTracker::new(&default_config());
        tracker
            .record("openai", "gpt-4", 100, 50, 0.15, false)
            .unwrap();
        tracker.reset_session().unwrap();
        let snap = tracker.snapshot().unwrap();
        assert_eq!(snap.total_requests, 0);
        assert_eq!(snap.total_input_tokens, 0);
        assert!(snap.by_provider.is_empty());
        assert!(snap.by_model.is_empty());
    }

    #[test]
    fn monthly_persists_across_session_reset() {
        let tracker = CostTracker::new(&default_config());
        tracker
            .record("openai", "gpt-4", 100, 50, 0.15, false)
            .unwrap();
        tracker.reset_session().unwrap();
        let monthly = tracker.monthly_cost_cents().unwrap();
        assert!((monthly - 0.15).abs() < 0.001);
    }

    #[test]
    fn snapshot_contains_provider_totals() {
        let tracker = CostTracker::new(&default_config());
        tracker
            .record("openai", "gpt-4", 100, 50, 0.15, false)
            .unwrap();
        tracker
            .record("openai", "gpt-4", 200, 100, 0.30, false)
            .unwrap();
        let snap = tracker.snapshot().unwrap();
        let p_row = snap.by_provider.get("openai").unwrap();
        assert_eq!(p_row.input_tokens, 300);
        assert_eq!(p_row.output_tokens, 150);
        assert_eq!(p_row.request_count, 2);
    }

    #[test]
    fn budget_not_exceeded_when_under() {
        let tracker = CostTracker::new(&budget_config(1000, None));
        tracker
            .record("openai", "gpt-4", 10, 5, 0.01, false)
            .unwrap();
        let (exceeded, _, remaining) = tracker.check_budget().unwrap();
        assert!(!exceeded);
        assert!(remaining.unwrap() > 0);
    }

    #[test]
    fn budget_exceeded_when_over() {
        let tracker = CostTracker::new(&budget_config(1, None));
        tracker
            .record("openai", "gpt-4", 10000, 5000, 15.0, false)
            .unwrap();
        let (exceeded, _, _) = tracker.check_budget().unwrap();
        assert!(exceeded);
    }

    #[test]
    fn budget_remaining_is_never_negative() {
        let tracker = CostTracker::new(&budget_config(1, None));
        tracker
            .record("openai", "gpt-4", 100000, 50000, 150.0, false)
            .unwrap();
        let (_, _, remaining) = tracker.check_budget().unwrap();
        assert_eq!(remaining.unwrap(), 0);
    }

    #[test]
    fn alert_threshold_triggered() {
        let tracker = CostTracker::new(&budget_config(100, Some(0.5)));
        // Spend 60 cents (over 50% of 100)
        tracker
            .record("openai", "gpt-4", 40000, 20000, 60.0, false)
            .unwrap();
        let (_, triggered, _) = tracker.check_budget().unwrap();
        assert!(triggered);
    }

    #[test]
    fn alert_threshold_not_triggered_below() {
        let tracker = CostTracker::new(&budget_config(100, Some(0.5)));
        tracker
            .record("openai", "gpt-4", 10000, 5000, 15.0, false)
            .unwrap();
        let (_, triggered, _) = tracker.check_budget().unwrap();
        assert!(!triggered);
    }

    #[test]
    fn no_budget_no_alert() {
        let tracker = CostTracker::new(&default_config());
        tracker
            .record("openai", "gpt-4", 100000, 50000, 150.0, false)
            .unwrap();
        let (exceeded, triggered, remaining) = tracker.check_budget().unwrap();
        assert!(!exceeded);
        assert!(!triggered);
        assert!(remaining.is_none());
    }

    #[test]
    fn cache_savings_tracked() {
        let tracker = CostTracker::new(&default_config());
        tracker.record_cache_saving(0.50).unwrap();
        tracker.record_cache_saving(0.25).unwrap();
        let snap = tracker.snapshot().unwrap();
        assert!((snap.cache_savings_cents - 0.75).abs() < 0.001);
    }

    #[test]
    fn rounding_no_drift() {
        let tracker = CostTracker::new(&default_config());
        // Many small records
        for _ in 0..100 {
            tracker
                .record("openai", "gpt-4", 1, 1, 0.001, false)
                .unwrap();
        }
        let snap = tracker.snapshot().unwrap();
        let expected = round_to_millicents(0.001 * 100.0);
        assert!((snap.total_cost_cents - expected).abs() < 0.0001);
    }

    #[test]
    fn overflow_protection() {
        let tracker = CostTracker::new(&default_config());
        // Large numbers should saturate, not panic
        tracker
            .record("openai", "gpt-4", u64::MAX, u64::MAX, 1.0, false)
            .unwrap();
        let snap = tracker.snapshot().unwrap();
        assert_eq!(snap.total_input_tokens, u64::MAX);
        assert_eq!(snap.total_output_tokens, u64::MAX);
    }

    #[test]
    fn snapshot_contains_budget_info() {
        let tracker = CostTracker::new(&budget_config(500, Some(0.8)));
        tracker
            .record("openai", "gpt-4", 1000, 500, 1.5, false)
            .unwrap();
        let snap = tracker.snapshot().unwrap();
        assert_eq!(snap.budget_cents, Some(500));
        assert!(snap.budget_remaining_cents.unwrap() <= 500);
        assert!(!snap.budget_exceeded);
    }

    #[test]
    fn concurrent_records() {
        let tracker = std::sync::Arc::new(CostTracker::new(&default_config()));
        let mut handles = Vec::new();
        for _ in 0..10 {
            let t = tracker.clone();
            handles.push(std::thread::spawn(move || {
                t.record("openai", "gpt-4", 10, 5, 0.01, false).unwrap();
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        let snap = tracker.snapshot().unwrap();
        assert_eq!(snap.total_requests, 10);
        assert_eq!(snap.total_input_tokens, 100);
        assert_eq!(snap.total_output_tokens, 50);
    }

    #[test]
    fn estimate_with_zero_rates() {
        let tracker = CostTracker::default();
        let (cost, _, _) = tracker.estimate(1000, 500, 0.0, 0.0);
        assert_eq!(cost, 0.0);
    }

    #[test]
    fn empty_tracker_values() {
        let tracker = CostTracker::new(&default_config());
        let snap = tracker.snapshot().unwrap();
        assert_eq!(snap.total_tokens, 0);
        assert_eq!(snap.total_requests, 0);
        assert_eq!(snap.fallback_count, 0);
        assert_eq!(snap.cache_savings_cents, 0.0);
    }

    #[test]
    fn snapshot_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<CostTrackerSnapshot>();
        assert_sync::<CostTrackerSnapshot>();
    }
}
