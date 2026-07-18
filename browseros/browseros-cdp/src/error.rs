use thiserror::Error;

pub type CdpResult<T> = Result<T, CdpError>;

#[derive(Error, Debug, Clone)]
pub enum CdpError {
    #[error("Transport error: {0}")]
    Transport(String),

    #[error("Protocol error (code={code}): {message}")]
    Protocol {
        code: i64,
        message: String,
        data: Option<String>,
    },

    #[error("Connection closed")]
    ConnectionClosed,

    #[error("Connection timeout")]
    ConnectionTimeout,

    #[error("Command timed out after {0}ms")]
    CommandTimeout(u64),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("No response for command {0}")]
    NoResponse(u64),

    #[error("Command {0} failed: {1}")]
    CommandFailed(u64, String),

    #[error("Session not found: {0}")]
    SessionNotFound(String),

    #[error("Target not found: {0}")]
    TargetNotFound(String),

    #[error("Invalid endpoint: {0}")]
    InvalidEndpoint(String),

    #[error("Browser process error: {0}")]
    BrowserProcess(String),

    #[error("Not implemented: {0}")]
    NotImplemented(&'static str),
}

impl CdpError {
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::ConnectionClosed
                | Self::ConnectionTimeout
                | Self::CommandTimeout(_)
                | Self::Transport(_)
        )
    }

    pub fn is_protocol(&self) -> bool {
        matches!(self, Self::Protocol { .. })
    }
}

impl From<serde_json::Error> for CdpError {
    fn from(e: serde_json::Error) -> Self {
        Self::Serialization(e.to_string())
    }
}
