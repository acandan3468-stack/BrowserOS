use browseros_bridge::types::ElementState;
use browseros_types::identifiers::HandleId;

use crate::element::ElementHandle;
use crate::error::DomResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ElementStateHelper {
    pub visible: bool,
    pub enabled: bool,
    pub checked: bool,
    pub selected: bool,
    pub stable: bool,
}

impl ElementStateHelper {
    pub(crate) fn from_handle(handle: &ElementHandle) -> DomResult<Self> {
        Ok(Self {
            visible: handle.is_visible()?,
            enabled: handle.is_enabled()?,
            checked: handle.is_checked()?,
            selected: handle.is_selected()?,
            stable: handle.is_stable()?,
        })
    }

    pub fn is(&self, state: ElementState) -> bool {
        match state {
            ElementState::Visible => self.visible,
            ElementState::Hidden => !self.visible,
            ElementState::Enabled => self.enabled,
            ElementState::Disabled => !self.enabled,
            ElementState::Selected => self.selected,
            ElementState::Editable => self.enabled && self.visible,
            ElementState::Stable => self.stable,
        }
    }

    pub fn all(&self, states: &[ElementState]) -> bool {
        states.iter().all(|&s| self.is(s))
    }

    pub fn any(&self, states: &[ElementState]) -> bool {
        states.iter().any(|&s| self.is(s))
    }
}

#[derive(Debug)]
pub struct ObserverHandle {
    handle_id: HandleId,
}

impl ObserverHandle {
    pub fn new() -> Self {
        Self {
            handle_id: HandleId::new(),
        }
    }

    pub fn id(&self) -> HandleId {
        self.handle_id
    }
}

impl Default for ObserverHandle {
    fn default() -> Self {
        Self::new()
    }
}
