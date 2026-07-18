use std::sync::Arc;

use browseros_bridge::traits::ElementPort;
use browseros_types::identifiers::HandleId;

use crate::element::ElementHandle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShadowRootMode {
    Open,
    Closed,
}

pub struct ShadowRootHandle {
    pub host: ElementHandle,
    pub mode: ShadowRootMode,
    pub handle_id: HandleId,
    pub element_port: Arc<dyn ElementPort>,
}

impl std::fmt::Debug for ShadowRootHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShadowRootHandle")
            .field("handle_id", &self.handle_id)
            .field("mode", &self.mode)
            .finish()
    }
}

impl Clone for ShadowRootHandle {
    fn clone(&self) -> Self {
        Self {
            host: self.host.clone(),
            mode: self.mode,
            handle_id: self.handle_id,
            element_port: Arc::clone(&self.element_port),
        }
    }
}

impl ShadowRootHandle {
    pub fn new(host: ElementHandle, element_port: Arc<dyn ElementPort>) -> Self {
        Self {
            host,
            mode: ShadowRootMode::Open,
            handle_id: HandleId::new(),
            element_port,
        }
    }

    pub fn id(&self) -> HandleId {
        self.handle_id
    }

    pub fn host_element(&self) -> &ElementHandle {
        &self.host
    }

    pub fn mode(&self) -> ShadowRootMode {
        self.mode
    }

    pub fn is_closed(&self) -> bool {
        self.mode == ShadowRootMode::Closed
    }
}
