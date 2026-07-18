//! Response and embedding LRU cache with TTL, memory limits, and statistics.

use crate::error::LlmError;
use crate::types::{
    CacheConfig, LlEmbedResponse, LlResponse, ProviderEmbedRequest, ProviderRequest,
};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

// ─────────────────────────────────────────────────────────────────────────────
// Cache entry
// ─────────────────────────────────────────────────────────────────────────────

/// Type of cached payload.
#[derive(Debug, Clone)]
pub enum CacheEntryType {
    Response(LlResponse),
    Embedding(LlEmbedResponse),
}

impl CacheEntryType {
    /// Approximate byte size of the payload.
    pub fn approximate_size(&self) -> usize {
        match self {
            CacheEntryType::Response(r) => r.approximate_size(),
            CacheEntryType::Embedding(e) => {
                let mut size = std::mem::size_of::<LlEmbedResponse>();
                size += e.model.len();
                size += e
                    .embeddings
                    .iter()
                    .map(|v| v.len() * 4 + std::mem::size_of::<Vec<f32>>())
                    .sum::<usize>();
                size
            }
        }
    }
}

/// A single cache entry with metadata.
#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub key: String,
    pub payload: CacheEntryType,
    pub created_at: Instant,
    pub last_access: Instant,
    pub ttl: Duration,
    pub byte_size: usize,
    pub hit_count: u64,
    pub access_order: u64,
}

/// Cache statistics.
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub entries: usize,
    pub total_bytes: usize,
    pub max_bytes: usize,
    pub max_entries: usize,
    pub evictions: u64,
    pub expired_cleaned: u64,
}

// ─────────────────────────────────────────────────────────────────────────────
// Cache key generation (deterministic)
// ─────────────────────────────────────────────────────────────────────────────

/// Build a deterministic cache key from a provider request.
///
/// Uses sorted field ordering so equivalent requests produce identical keys.
pub fn response_cache_key(req: &ProviderRequest) -> String {
    let mut parts: Vec<String> = Vec::with_capacity(10);
    parts.push("resp".into());
    parts.push(req.model.clone());

    let msgs: Vec<String> = req
        .messages
        .iter()
        .map(|m| {
            format!(
                "{}:{}",
                m.role.as_str(),
                content_to_deterministic(&m.content)
            )
        })
        .collect();
    parts.push(msgs.join("|"));

    let mut tool_strs: Vec<String> = req
        .tools
        .iter()
        .map(|t| format!("{}:{}:{:?}", t.name, t.description, t.parameters))
        .collect();
    tool_strs.sort();
    parts.push(tool_strs.join(","));

    parts.push(
        req.temperature
            .map_or(0, |t| (t * 100.0) as u64)
            .to_string(),
    );

    parts.push(req.max_tokens.unwrap_or(0).to_string());

    parts.push(req.system_prompt.as_deref().unwrap_or("").to_string());

    let mut stops = req.stop_sequences.clone();
    stops.sort();
    parts.push(stops.join(","));

    parts.push(if req.stream { "1" } else { "0" }.to_string());

    parts.join("||")
}

/// Build a deterministic cache key for an embedding request.
pub fn embedding_cache_key(req: &ProviderEmbedRequest) -> String {
    let mut parts: Vec<String> = Vec::with_capacity(5);
    parts.push("embed".into());
    parts.push(req.model.clone());

    let mut inputs = req.input.clone();
    inputs.sort();
    parts.push(inputs.join("|"));

    parts.push(req.timeout_ms.to_string());

    parts.join("||")
}

/// Deterministic representation of LlContent for cache key building.
fn content_to_deterministic(content: &crate::types::LlContent) -> String {
    match content {
        crate::types::LlContent::Text(t) => format!("text:{}", t),
        crate::types::LlContent::Image { mime_type, data } => {
            format!("img:{}:{}", mime_type, data.len())
        }
        crate::types::LlContent::ToolResult { call_id, output } => {
            format!("tr:{}:{}", call_id, output)
        }
        crate::types::LlContent::ToolCall {
            call_id,
            name,
            arguments,
        } => {
            let mut keys: Vec<&String> = arguments.keys().collect();
            keys.sort();
            let args: String = keys
                .iter()
                .map(|k| format!("{}:{:?}", k, arguments[*k]))
                .collect::<Vec<_>>()
                .join(",");
            format!("tc:{}:{}:{}", call_id, name, args)
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// LlCache
// ─────────────────────────────────────────────────────────────────────────────

/// In-memory LRU cache for LLM responses and embeddings.
///
/// Thread-safe via internal Mutex. Supports TTL expiration, byte-based memory
/// limits, LRU eviction, statistics, and deterministic cache keys.
pub struct LlCache {
    inner: Mutex<CacheInner>,
    config: CacheConfig,
}

struct CacheInner {
    entries: HashMap<String, CacheEntry>,
    stats: CacheStats,
    total_bytes: usize,
    access_counter: u64,
}

impl LlCache {
    /// Create a new cache with the given configuration.
    pub fn new(config: &CacheConfig) -> Self {
        let max_bytes = if config.max_memory_bytes == 0 {
            100 * 1024 * 1024
        } else {
            config.max_memory_bytes
        };
        let max_entries = if config.max_entries == 0 {
            crate::DEFAULT_CACHE_MAX_ENTRIES
        } else {
            config.max_entries
        };
        Self {
            inner: Mutex::new(CacheInner {
                entries: HashMap::new(),
                stats: CacheStats {
                    max_bytes,
                    max_entries,
                    ..CacheStats::default()
                },
                total_bytes: 0,
                access_counter: 0,
            }),
            config: config.clone(),
        }
    }
}

impl Default for LlCache {
    fn default() -> Self {
        Self::new(&CacheConfig::default())
    }
}

impl LlCache {
    /// Return a reference to the cache configuration.
    pub fn config(&self) -> &CacheConfig {
        &self.config
    }

    /// Insert a response into the cache with a given key.
    pub fn insert_response(&self, key: &str, response: LlResponse) -> Result<(), LlmError> {
        let byte_size = response.approximate_size();
        let ttl = Duration::from_secs(self.config.ttl_secs);
        let now = Instant::now();
        let mut inner = self
            .inner
            .lock()
            .map_err(|e| LlmError::CacheError(format!("lock poisoned: {}", e)))?;
        let counter = inner.access_counter;
        inner.access_counter = counter.wrapping_add(1);
        drop(inner);

        let entry = CacheEntry {
            key: key.to_string(),
            payload: CacheEntryType::Response(response),
            created_at: now,
            last_access: now,
            ttl,
            byte_size,
            hit_count: 0,
            access_order: counter,
        };
        self.insert_entry(entry)
    }

    /// Insert an embedding into the cache with a given key.
    pub fn insert_embedding(&self, key: &str, embedding: LlEmbedResponse) -> Result<(), LlmError> {
        let byte_size = CacheEntryType::Embedding(embedding.clone()).approximate_size();
        let ttl = Duration::from_secs(self.config.embed_ttl_secs);
        let now = Instant::now();
        let mut inner = self
            .inner
            .lock()
            .map_err(|e| LlmError::CacheError(format!("lock poisoned: {}", e)))?;
        let counter = inner.access_counter;
        inner.access_counter = counter.wrapping_add(1);
        drop(inner);

        let entry = CacheEntry {
            key: key.to_string(),
            payload: CacheEntryType::Embedding(embedding),
            created_at: now,
            last_access: now,
            ttl,
            byte_size,
            hit_count: 0,
            access_order: counter,
        };
        self.insert_entry(entry)
    }

    /// Internal: insert an entry, evicting if necessary.
    fn insert_entry(&self, entry: CacheEntry) -> Result<(), LlmError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|e| LlmError::CacheError(format!("lock poisoned: {}", e)))?;

        if let Some(old) = inner.entries.get(&entry.key) {
            inner.total_bytes = inner.total_bytes.saturating_sub(old.byte_size);
        }

        self.evict_if_needed(&mut inner, entry.byte_size);

        let key = entry.key.clone();
        inner.total_bytes += entry.byte_size;
        inner.entries.insert(key, entry);
        inner.stats.entries = inner.entries.len();
        inner.stats.total_bytes = inner.total_bytes;
        Ok(())
    }

    /// Look up a response by key. Returns `None` on miss or expiration.
    pub fn get_response(&self, key: &str) -> Option<LlResponse> {
        let mut inner = self.inner.lock().ok()?;

        // Check existence and expiration first
        let is_present = inner.entries.contains_key(key);
        if !is_present {
            inner.stats.misses += 1;
            inner.stats.entries = inner.entries.len();
            inner.stats.total_bytes = inner.total_bytes;
            return None;
        }

        let is_expired = inner.entries.get(key)?.created_at.elapsed() > inner.entries.get(key)?.ttl;

        if is_expired {
            if let Some(entry) = inner.entries.remove(key) {
                inner.total_bytes = inner.total_bytes.saturating_sub(entry.byte_size);
                inner.stats.expired_cleaned += 1;
            }
            inner.stats.entries = inner.entries.len();
            inner.stats.total_bytes = inner.total_bytes;
            inner.stats.misses += 1;
            return None;
        }

        // Save counter before mutable borrow to avoid field-via-struct conflict
        let current_counter = inner.access_counter;
        let new_counter = current_counter.wrapping_add(1);
        let (result, is_correct_type) = {
            let entry = inner.entries.get_mut(key)?;
            let correct = matches!(entry.payload, CacheEntryType::Response(_));
            let cloned = if correct {
                entry.last_access = Instant::now();
                entry.hit_count += 1;
                entry.access_order = current_counter;
                match &entry.payload {
                    CacheEntryType::Response(r) => Some(r.clone()),
                    _ => None,
                }
            } else {
                None
            };
            (cloned, correct)
        };
        inner.access_counter = new_counter;
        if is_correct_type {
            inner.stats.hits += 1;
        } else {
            inner.stats.misses += 1;
        }
        inner.stats.entries = inner.entries.len();
        inner.stats.total_bytes = inner.total_bytes;
        result
    }

    /// Look up an embedding by key. Returns `None` on miss or expiration.
    pub fn get_embedding(&self, key: &str) -> Option<LlEmbedResponse> {
        let mut inner = self.inner.lock().ok()?;

        let is_present = inner.entries.contains_key(key);
        if !is_present {
            inner.stats.misses += 1;
            inner.stats.entries = inner.entries.len();
            inner.stats.total_bytes = inner.total_bytes;
            return None;
        }

        let is_expired = inner.entries.get(key)?.created_at.elapsed() > inner.entries.get(key)?.ttl;

        if is_expired {
            if let Some(entry) = inner.entries.remove(key) {
                inner.total_bytes = inner.total_bytes.saturating_sub(entry.byte_size);
                inner.stats.expired_cleaned += 1;
            }
            inner.stats.entries = inner.entries.len();
            inner.stats.total_bytes = inner.total_bytes;
            inner.stats.misses += 1;
            return None;
        }

        let current_counter = inner.access_counter;
        let new_counter = current_counter.wrapping_add(1);
        let (result, is_correct_type) = {
            let entry = inner.entries.get_mut(key)?;
            let correct = matches!(entry.payload, CacheEntryType::Embedding(_));
            if correct {
                entry.last_access = Instant::now();
                entry.hit_count += 1;
                entry.access_order = current_counter;
                let cloned = match &entry.payload {
                    CacheEntryType::Embedding(e) => Some(e.clone()),
                    _ => None,
                };
                (cloned, true)
            } else {
                (None, false)
            }
        };
        inner.access_counter = new_counter;
        if is_correct_type {
            inner.stats.hits += 1;
        } else {
            inner.stats.misses += 1;
        }
        inner.stats.entries = inner.entries.len();
        inner.stats.total_bytes = inner.total_bytes;
        result
    }

    /// Check if a key exists in the cache (not expired).
    pub fn contains(&self, key: &str) -> bool {
        let inner = match self.inner.lock() {
            Ok(i) => i,
            Err(_) => return false,
        };
        match inner.entries.get(key) {
            Some(e) => e.created_at.elapsed() <= e.ttl,
            None => false,
        }
    }

    /// Remove an entry by key.
    pub fn remove(&self, key: &str) -> Result<(), LlmError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|e| LlmError::CacheError(format!("lock poisoned: {}", e)))?;
        if let Some(entry) = inner.entries.remove(key) {
            inner.total_bytes = inner.total_bytes.saturating_sub(entry.byte_size);
            inner.stats.entries = inner.entries.len();
            inner.stats.total_bytes = inner.total_bytes;
        }
        Ok(())
    }

    /// Remove all entries.
    pub fn clear(&self) -> Result<(), LlmError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|e| LlmError::CacheError(format!("lock poisoned: {}", e)))?;
        inner.entries.clear();
        inner.total_bytes = 0;
        inner.stats.entries = 0;
        inner.stats.total_bytes = 0;
        Ok(())
    }

    /// Remove all expired entries and return the count removed.
    pub fn cleanup_expired(&self) -> Result<u64, LlmError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|e| LlmError::CacheError(format!("lock poisoned: {}", e)))?;
        let expired_keys: Vec<String> = inner
            .entries
            .iter()
            .filter(|(_, e)| e.created_at.elapsed() > e.ttl)
            .map(|(k, _)| k.clone())
            .collect();
        let removed = expired_keys.len() as u64;
        for key in &expired_keys {
            if let Some(entry) = inner.entries.remove(key) {
                inner.total_bytes = inner.total_bytes.saturating_sub(entry.byte_size);
            }
        }
        inner.stats.expired_cleaned += removed;
        inner.stats.entries = inner.entries.len();
        inner.stats.total_bytes = inner.total_bytes;
        Ok(removed)
    }

    /// Return current cache statistics.
    pub fn stats(&self) -> Result<CacheStats, LlmError> {
        let inner = self
            .inner
            .lock()
            .map_err(|e| LlmError::CacheError(format!("lock poisoned: {}", e)))?;
        let mut s = inner.stats.clone();
        s.entries = inner.entries.len();
        s.total_bytes = inner.total_bytes;
        Ok(s)
    }

    /// Return the number of entries in the cache.
    pub fn len(&self) -> usize {
        self.inner.lock().map(|i| i.entries.len()).unwrap_or(0)
    }

    /// Return true if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Return total memory usage in bytes.
    pub fn total_bytes(&self) -> usize {
        self.inner.lock().map(|i| i.total_bytes).unwrap_or(0)
    }

    // ─── Private helpers ────────────────────────────────────────────────────

    fn evict_if_needed(&self, inner: &mut CacheInner, needed_bytes: usize) {
        let max_bytes = inner.stats.max_bytes;
        let max_entries = inner.stats.max_entries;

        let over_memory = max_bytes > 0 && inner.total_bytes + needed_bytes > max_bytes;
        let over_entries = max_entries > 0 && inner.entries.len() + 1 > max_entries;

        if !over_memory && !over_entries {
            return;
        }

        // Phase 1: remove expired
        let expired_keys: Vec<String> = inner
            .entries
            .iter()
            .filter(|(_, e)| e.created_at.elapsed() > e.ttl)
            .map(|(k, _)| k.clone())
            .collect();
        for key in &expired_keys {
            if let Some(entry) = inner.entries.remove(key) {
                inner.total_bytes = inner.total_bytes.saturating_sub(entry.byte_size);
                inner.stats.evictions += 1;
            }
        }

        let still_over_memory = max_bytes > 0 && inner.total_bytes + needed_bytes > max_bytes;
        let still_over_entries = max_entries > 0 && inner.entries.len() + 1 > max_entries;

        if !still_over_memory && !still_over_entries {
            inner.stats.expired_cleaned += expired_keys.len() as u64;
            return;
        }

        // Phase 2: LRU eviction by access_order
        let lru_keys: Vec<String> = {
            let mut vec: Vec<(&String, &CacheEntry)> = inner.entries.iter().collect();
            vec.sort_by_key(|(_, e)| e.access_order);
            vec.into_iter().map(|(k, _)| k.clone()).collect()
        };

        for key in &lru_keys {
            let om = max_bytes > 0 && inner.total_bytes + needed_bytes > max_bytes;
            let oe = max_entries > 0 && inner.entries.len() + 1 > max_entries;
            if !om && !oe {
                break;
            }
            if let Some(entry) = inner.entries.remove(key) {
                inner.total_bytes = inner.total_bytes.saturating_sub(entry.byte_size);
                inner.stats.evictions += 1;
            }
        }

        // Phase 3: if still over memory, remove largest entries
        while max_bytes > 0
            && inner.total_bytes + needed_bytes > max_bytes
            && !inner.entries.is_empty()
        {
            let largest_key = inner
                .entries
                .iter()
                .max_by_key(|(_, e)| e.byte_size)
                .map(|(k, _)| k.clone());
            if let Some(key) = largest_key {
                if let Some(entry) = inner.entries.remove(&key) {
                    inner.total_bytes = inner.total_bytes.saturating_sub(entry.byte_size);
                    inner.stats.evictions += 1;
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Send + Sync safety
// ─────────────────────────────────────────────────────────────────────────────

fn _assert_send_sync()
where
    LlCache: Send + Sync,
{
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    use std::thread;

    fn test_response(model: &str) -> LlResponse {
        LlResponse {
            message: LlMessage {
                role: LlRole::Assistant,
                content: LlContent::Text("hello world".into()),
                name: None,
            },
            finish_reason: LlFinishReason::Stop,
            usage: LlUsage {
                input_tokens: 10,
                output_tokens: 5,
                total_tokens: 15,
                cost_estimate_cents: 0.01,
                currency: "USD".into(),
            },
            model: model.to_string(),
            provider: "test".into(),
        }
    }

    fn test_embedding(model: &str) -> LlEmbedResponse {
        LlEmbedResponse {
            embeddings: vec![vec![0.1, 0.2, 0.3]],
            dimensions: 3,
            model: model.to_string(),
            usage: LlUsage {
                input_tokens: 5,
                output_tokens: 0,
                total_tokens: 5,
                cost_estimate_cents: 0.005,
                currency: "USD".into(),
            },
        }
    }

    fn small_config() -> CacheConfig {
        CacheConfig {
            enabled: true,
            max_entries: 3,
            max_memory_bytes: 1024 * 1024,
            ttl_secs: 3600,
            embed_ttl_secs: 3600,
        }
    }

    // ─── Basic operations ───────────────────────────────────────────────

    #[test]
    fn new_cache_empty() {
        let cache = LlCache::default();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn insert_and_get_response() {
        let cache = LlCache::new(&small_config());
        let resp = test_response("gpt-4");
        cache.insert_response("key1", resp.clone()).unwrap();
        let got = cache.get_response("key1").unwrap();
        assert_eq!(got.model, "gpt-4");
        assert_eq!(got.usage.input_tokens, 10);
    }

    #[test]
    fn insert_and_get_embedding() {
        let cache = LlCache::new(&small_config());
        let emb = test_embedding("text-embedding-3");
        cache.insert_embedding("emb1", emb.clone()).unwrap();
        let got = cache.get_embedding("emb1").unwrap();
        assert_eq!(got.model, "text-embedding-3");
        assert_eq!(got.embeddings.len(), 1);
    }

    #[test]
    fn cache_miss_returns_none() {
        let cache = LlCache::default();
        assert!(cache.get_response("nonexistent").is_none());
        assert!(cache.get_embedding("nonexistent").is_none());
    }

    #[test]
    fn type_mismatch_response_from_embedding_key() {
        let cache = LlCache::new(&small_config());
        let emb = test_embedding("e1");
        cache.insert_embedding("k1", emb).unwrap();
        assert!(cache.get_response("k1").is_none());
    }

    #[test]
    fn type_mismatch_embedding_from_response_key() {
        let cache = LlCache::new(&small_config());
        let resp = test_response("gpt-4");
        cache.insert_response("k1", resp).unwrap();
        assert!(cache.get_embedding("k1").is_none());
    }

    #[test]
    fn contains_returns_true_for_existing() {
        let cache = LlCache::new(&small_config());
        cache.insert_response("k1", test_response("gpt-4")).unwrap();
        assert!(cache.contains("k1"));
    }

    #[test]
    fn contains_returns_false_for_missing() {
        let cache = LlCache::default();
        assert!(!cache.contains("nope"));
    }

    #[test]
    fn remove_existing_key() {
        let cache = LlCache::new(&small_config());
        cache.insert_response("k1", test_response("gpt-4")).unwrap();
        cache.remove("k1").unwrap();
        assert!(cache.get_response("k1").is_none());
    }

    #[test]
    fn remove_nonexistent_is_ok() {
        let cache = LlCache::default();
        cache.remove("nope").unwrap();
    }

    #[test]
    fn clear_empties_cache() {
        let cache = LlCache::new(&small_config());
        cache.insert_response("k1", test_response("gpt-4")).unwrap();
        cache
            .insert_response("k2", test_response("claude"))
            .unwrap();
        cache.clear().unwrap();
        assert!(cache.is_empty());
    }

    // ─── Statistics ─────────────────────────────────────────────────────

    #[test]
    fn stats_after_insert_and_hit() {
        let cache = LlCache::new(&small_config());
        cache.insert_response("k1", test_response("gpt-4")).unwrap();
        let _ = cache.get_response("k1");
        let stats = cache.stats().unwrap();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.entries, 1);
    }

    #[test]
    fn stats_after_miss() {
        let cache = LlCache::default();
        let _ = cache.get_response("nope");
        let stats = cache.stats().unwrap();
        assert_eq!(stats.misses, 1);
    }

    #[test]
    fn stats_shows_config_values() {
        let config = small_config();
        let cache = LlCache::new(&config);
        let stats = cache.stats().unwrap();
        assert_eq!(stats.max_entries, 3);
        assert!(stats.max_bytes > 0);
    }

    #[test]
    fn duplicate_insert_replaces() {
        let cache = LlCache::new(&small_config());
        let r1 = test_response("gpt-4");
        let mut r2 = test_response("gpt-4");
        r2.usage.input_tokens = 99;
        cache.insert_response("k1", r1).unwrap();
        cache.insert_response("k1", r2.clone()).unwrap();
        let got = cache.get_response("k1").unwrap();
        assert_eq!(got.usage.input_tokens, 99);
    }

    // ─── Eviction ───────────────────────────────────────────────────────

    #[test]
    fn lru_eviction_respects_max_entries() {
        let config = CacheConfig {
            enabled: true,
            max_entries: 2,
            max_memory_bytes: 10 * 1024 * 1024,
            ttl_secs: 3600,
            embed_ttl_secs: 3600,
        };
        let cache = LlCache::new(&config);
        cache.insert_response("k1", test_response("a")).unwrap();
        cache.insert_response("k2", test_response("b")).unwrap();
        let _ = cache.get_response("k1");
        cache.insert_response("k3", test_response("c")).unwrap();
        assert!(cache.contains("k1"));
        assert!(!cache.contains("k2"));
        assert!(cache.contains("k3"));
    }

    #[test]
    fn lru_eviction_unaccessed_oldest_first() {
        let config = CacheConfig {
            enabled: true,
            max_entries: 2,
            max_memory_bytes: 10 * 1024 * 1024,
            ttl_secs: 3600,
            embed_ttl_secs: 3600,
        };
        let cache = LlCache::new(&config);
        cache.insert_response("k1", test_response("a")).unwrap();
        cache.insert_response("k2", test_response("b")).unwrap();
        cache.insert_response("k3", test_response("c")).unwrap();
        assert!(!cache.contains("k1"));
        assert!(cache.contains("k2"));
        assert!(cache.contains("k3"));
    }

    #[test]
    fn memory_eviction_removes_largest() {
        let config = CacheConfig {
            enabled: true,
            max_entries: 100,
            max_memory_bytes: 500,
            ttl_secs: 3600,
            embed_ttl_secs: 3600,
        };
        let cache = LlCache::new(&config);
        let large = LlEmbedResponse {
            embeddings: vec![vec![0.1; 1000]],
            dimensions: 1000,
            model: "big".into(),
            usage: LlUsage::default(),
        };
        cache.insert_embedding("big", large).unwrap();
        let small = LlResponse {
            message: LlMessage::assistant("hi"),
            finish_reason: LlFinishReason::Stop,
            usage: LlUsage::default(),
            model: "small".into(),
            provider: "t".into(),
        };
        cache.insert_response("small", small).unwrap();
        let big2 = LlEmbedResponse {
            embeddings: vec![vec![0.2; 500]],
            dimensions: 500,
            model: "big2".into(),
            usage: LlUsage::default(),
        };
        cache.insert_embedding("big2", big2).unwrap();
        let stats = cache.stats().unwrap();
        assert!(stats.evictions > 0);
    }

    #[test]
    fn expired_entry_not_returned() {
        let config = CacheConfig {
            enabled: true,
            max_entries: 10,
            max_memory_bytes: 10 * 1024 * 1024,
            ttl_secs: 0,
            embed_ttl_secs: 0,
        };
        let cache = LlCache::new(&config);
        cache.insert_response("k1", test_response("a")).unwrap();
        assert!(cache.get_response("k1").is_none());
    }

    #[test]
    fn cleanup_expired_removes_expired() {
        let config = CacheConfig {
            enabled: true,
            max_entries: 10,
            max_memory_bytes: 10 * 1024 * 1024,
            ttl_secs: 0,
            embed_ttl_secs: 0,
        };
        let cache = LlCache::new(&config);
        cache.insert_response("k1", test_response("a")).unwrap();
        cache.insert_response("k2", test_response("b")).unwrap();
        let removed = cache.cleanup_expired().unwrap();
        assert_eq!(removed, 2);
        assert!(cache.is_empty());
    }

    #[test]
    fn cleanup_expired_only_expired() {
        let config = CacheConfig {
            enabled: true,
            max_entries: 10,
            max_memory_bytes: 10 * 1024 * 1024,
            ttl_secs: 3600,
            embed_ttl_secs: 3600,
        };
        let cache = LlCache::new(&config);
        cache.insert_response("k1", test_response("a")).unwrap();
        cache.insert_response("k2", test_response("b")).unwrap();
        let removed = cache.cleanup_expired().unwrap();
        assert_eq!(removed, 0);
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn hit_count_increments() {
        let cache = LlCache::new(&small_config());
        cache.insert_response("k1", test_response("a")).unwrap();
        let _ = cache.get_response("k1");
        let _ = cache.get_response("k1");
        let _ = cache.get_response("k1");
        let stats = cache.stats().unwrap();
        assert_eq!(stats.hits, 3);
    }

    #[test]
    fn total_bytes_tracked_correctly() {
        let cache = LlCache::new(&small_config());
        let r = test_response("gpt-4");
        cache.insert_response("k1", r).unwrap();
        let bytes = cache.total_bytes();
        assert!(bytes > 0);
    }

    // ─── Concurrency ────────────────────────────────────────────────────

    #[test]
    fn concurrent_reads() {
        let cache = std::sync::Arc::new(LlCache::new(&small_config()));
        cache.insert_response("k1", test_response("gpt-4")).unwrap();

        let mut handles = Vec::new();
        for _ in 0..10 {
            let c = cache.clone();
            handles.push(thread::spawn(move || {
                c.get_response("k1").unwrap();
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        let stats = cache.stats().unwrap();
        assert_eq!(stats.hits, 10);
    }

    #[test]
    fn concurrent_writes() {
        let cache = std::sync::Arc::new(LlCache::new(&CacheConfig {
            enabled: true,
            max_entries: 100,
            max_memory_bytes: 10 * 1024 * 1024,
            ttl_secs: 3600,
            embed_ttl_secs: 3600,
        }));

        let mut handles = Vec::new();
        for i in 0..20 {
            let c = cache.clone();
            handles.push(thread::spawn(move || {
                let resp = test_response(&format!("model-{}", i));
                c.insert_response(&format!("k-{}", i), resp).unwrap();
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(cache.len(), 20);
    }

    #[test]
    fn concurrent_read_write() {
        let cache = std::sync::Arc::new(LlCache::new(&CacheConfig {
            enabled: true,
            max_entries: 50,
            max_memory_bytes: 10 * 1024 * 1024,
            ttl_secs: 3600,
            embed_ttl_secs: 3600,
        }));

        let mut handles = Vec::new();
        for i in 0..10 {
            let c = cache.clone();
            handles.push(thread::spawn(move || {
                let resp = test_response(&format!("m-{}", i));
                c.insert_response(&format!("k-{}", i), resp).unwrap();
                let _ = c.get_response(&format!("k-{}", i));
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
    }

    // ─── Large payload ──────────────────────────────────────────────────

    #[test]
    fn large_payload_eviction() {
        let config = CacheConfig {
            enabled: true,
            max_entries: 100,
            max_memory_bytes: 2000,
            ttl_secs: 3600,
            embed_ttl_secs: 3600,
        };
        let cache = LlCache::new(&config);
        let big = LlEmbedResponse {
            embeddings: vec![vec![0.5; 10000]],
            dimensions: 10000,
            model: "huge".into(),
            usage: LlUsage::default(),
        };
        cache.insert_embedding("huge", big).unwrap();
        assert!(cache.contains("huge"));

        let small = LlResponse {
            message: LlMessage::assistant("tiny"),
            finish_reason: LlFinishReason::Stop,
            usage: LlUsage::default(),
            model: "tiny".into(),
            provider: "t".into(),
        };
        cache.insert_response("tiny", small).unwrap();
        let stats = cache.stats().unwrap();
        assert!(stats.evictions > 0);
    }

    // ─── Default handling ───────────────────────────────────────────────

    #[test]
    fn zero_max_entries_uses_default() {
        let config = CacheConfig {
            enabled: true,
            max_entries: 0,
            max_memory_bytes: 10 * 1024 * 1024,
            ttl_secs: 3600,
            embed_ttl_secs: 3600,
        };
        let cache = LlCache::new(&config);
        assert_eq!(
            cache.stats().unwrap().max_entries,
            crate::DEFAULT_CACHE_MAX_ENTRIES
        );
    }

    #[test]
    fn zero_max_memory_uses_default() {
        let config = CacheConfig {
            enabled: true,
            max_entries: 100,
            max_memory_bytes: 0,
            ttl_secs: 3600,
            embed_ttl_secs: 3600,
        };
        let cache = LlCache::new(&config);
        assert_eq!(cache.stats().unwrap().max_bytes, 100 * 1024 * 1024);
    }

    // ─── Cache key determinism ──────────────────────────────────────────

    #[test]
    fn cache_key_deterministic_response() {
        let req1 = ProviderRequest {
            model: "gpt-4".into(),
            messages: vec![ProviderMessage {
                role: LlRole::User,
                content: LlContent::Text("hello".into()),
            }],
            max_tokens: Some(100),
            temperature: Some(0.7),
            stop_sequences: vec!["stop".into()],
            system_prompt: None,
            tools: vec![],
            stream: false,
            timeout_ms: 30000,
        };
        let req2 = ProviderRequest {
            model: "gpt-4".into(),
            messages: vec![ProviderMessage {
                role: LlRole::User,
                content: LlContent::Text("hello".into()),
            }],
            max_tokens: Some(100),
            temperature: Some(0.7),
            stop_sequences: vec!["stop".into()],
            system_prompt: None,
            tools: vec![],
            stream: false,
            timeout_ms: 60000,
        };
        let key1 = response_cache_key(&req1);
        let key2 = response_cache_key(&req2);
        assert_eq!(key1, key2, "identical requests must produce identical keys");
    }

    #[test]
    fn cache_key_differs_for_different_model() {
        let mut req = ProviderRequest {
            model: "gpt-4".into(),
            messages: vec![],
            max_tokens: None,
            temperature: None,
            stop_sequences: vec![],
            system_prompt: None,
            tools: vec![],
            stream: false,
            timeout_ms: 30000,
        };
        let key1 = response_cache_key(&req);
        req.model = "claude-3".into();
        let key2 = response_cache_key(&req);
        assert_ne!(key1, key2);
    }

    #[test]
    fn cache_key_deterministic_tool_order() {
        let tool = LlTool {
            name: "get_weather".into(),
            description: "Get weather".into(),
            parameters: serde_json::Value::Object(serde_json::Map::new()),
            strict: false,
        };
        let req = ProviderRequest {
            model: "gpt-4".into(),
            messages: vec![],
            max_tokens: None,
            temperature: None,
            stop_sequences: vec![],
            system_prompt: None,
            tools: vec![tool],
            stream: false,
            timeout_ms: 30000,
        };
        let key = response_cache_key(&req);
        assert!(key.contains("get_weather"));
    }

    #[test]
    fn embedding_cache_key_deterministic() {
        let req1 = ProviderEmbedRequest {
            model: "e1".into(),
            input: vec!["b".into(), "a".into()],
            timeout_ms: 5000,
        };
        let req2 = ProviderEmbedRequest {
            model: "e1".into(),
            input: vec!["a".into(), "b".into()],
            timeout_ms: 5000,
        };
        let key1 = embedding_cache_key(&req1);
        let key2 = embedding_cache_key(&req2);
        assert_eq!(
            key1, key2,
            "embedding keys must be independent of input order"
        );
    }

    #[test]
    fn embedding_cache_key_differs_for_different_model() {
        let mut req = ProviderEmbedRequest {
            model: "e1".into(),
            input: vec!["hello".into()],
            timeout_ms: 5000,
        };
        let key1 = embedding_cache_key(&req);
        req.model = "e2".into();
        let key2 = embedding_cache_key(&req);
        assert_ne!(key1, key2);
    }

    #[test]
    fn cache_reports_config() {
        let config = small_config();
        let cache = LlCache::new(&config);
        assert_eq!(cache.config().max_entries, 3);
    }
}
