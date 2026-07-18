use std::time::Duration;

#[derive(Debug, Clone)]
pub struct CdpConfig {
    pub connect_timeout: Duration,
    pub command_timeout: Duration,
    pub max_retries: u32,
    pub retry_delay: Duration,
    pub reader_buffer_size: usize,
}

impl Default for CdpConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(30),
            command_timeout: Duration::from_secs(60),
            max_retries: 3,
            retry_delay: Duration::from_millis(500),
            reader_buffer_size: 1024,
        }
    }
}

impl CdpConfig {
    pub fn with_connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    pub fn with_command_timeout(mut self, timeout: Duration) -> Self {
        self.command_timeout = timeout;
        self
    }

    pub fn with_max_retries(mut self, retries: u32) -> Self {
        self.max_retries = retries;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = CdpConfig::default();
        assert_eq!(config.connect_timeout, Duration::from_secs(30));
        assert_eq!(config.command_timeout, Duration::from_secs(60));
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.retry_delay, Duration::from_millis(500));
    }

    #[test]
    fn test_custom_config() {
        let config = CdpConfig::default()
            .with_connect_timeout(Duration::from_secs(10))
            .with_command_timeout(Duration::from_secs(30))
            .with_max_retries(5);
        assert_eq!(config.connect_timeout, Duration::from_secs(10));
        assert_eq!(config.command_timeout, Duration::from_secs(30));
        assert_eq!(config.max_retries, 5);
    }

    #[test]
    fn test_config_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<CdpConfig>();
    }
}
