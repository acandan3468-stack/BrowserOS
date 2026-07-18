use browseros_bridge::error::BridgeError;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum StorageError {
    #[error("bridge error: {0}")]
    BridgeError(#[from] BridgeError),
}

pub type StorageResult<T> = Result<T, StorageError>;
