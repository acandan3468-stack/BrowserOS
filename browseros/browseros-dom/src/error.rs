use browseros_bridge::error::BridgeError;
use browseros_bridge::identifiers::FrameId;
use browseros_types::identifiers::HandleId;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum DomError {
    #[error("element handle {0} is stale — DOM node no longer attached")]
    StaleElement(HandleId),

    #[error("frame {0} is detached — no longer part of the page tree")]
    FrameDetached(FrameId),

    #[error("element not found: {0}")]
    NotFound(String),

    #[error("element is not visible")]
    NotVisible,

    #[error("element is not interactable: {0}")]
    NotInteractable(String),

    #[error("closed shadow root cannot be traversed: {0}")]
    ClosedShadowRoot(String),

    #[error("cross-origin frame access denied")]
    CrossOriginFrame,

    #[error("operation not supported by backend: {0}")]
    NotSupported(String),

    #[error("invalid handle state: {0}")]
    InvalidHandle(String),

    #[error("bridge error: {0}")]
    BridgeError(#[from] BridgeError),
}

impl DomError {
    pub fn is_stale(&self) -> bool {
        matches!(self, Self::StaleElement(_))
    }

    pub fn is_permanent(&self) -> bool {
        matches!(
            self,
            Self::StaleElement(_) | Self::FrameDetached(_) | Self::InvalidHandle(_)
        )
    }

    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            Self::NotFound(_) | Self::NotVisible | Self::NotInteractable(_)
        ) || self.as_bridge().is_some_and(BridgeError::is_recoverable)
    }

    fn as_bridge(&self) -> Option<&BridgeError> {
        match self {
            Self::BridgeError(e) => Some(e),
            _ => None,
        }
    }
}

pub type DomResult<T> = Result<T, DomError>;
