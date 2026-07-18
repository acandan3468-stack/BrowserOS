use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagConfig {
    pub max_concurrency: Option<usize>,
    pub default_timeout_ms: Option<u64>,
    pub execution_ttl_secs: Option<u64>,
}

impl Default for DagConfig {
    fn default() -> Self {
        Self {
            max_concurrency: Some(
                std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(4),
            ),
            default_timeout_ms: Some(30_000),
            execution_ttl_secs: Some(3600),
        }
    }
}
