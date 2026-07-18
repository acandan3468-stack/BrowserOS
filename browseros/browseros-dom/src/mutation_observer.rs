use browseros_types::identifiers::HandleId;

use crate::element::ElementHandle;
use crate::error::DomResult;
use crate::node::NodeHandle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationType {
    ChildList,
    Attributes,
    CharacterData,
    Subtree,
}

#[derive(Debug, Clone)]
pub struct MutationRecord {
    pub target: HandleId,
    pub added_nodes: Vec<NodeHandle>,
    pub removed_nodes: Vec<NodeHandle>,
    pub attribute_name: Option<String>,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub mutation_type: MutationType,
}

pub(crate) type ObserverCallback = Box<dyn Fn(&[MutationRecord]) + Send + Sync>;

#[derive(Debug, Clone, Default)]
pub struct MutationObserverOptions {
    pub child_list: bool,
    pub attributes: bool,
    pub character_data: bool,
    pub subtree: bool,
    pub attribute_filter: Option<Vec<String>>,
    pub attribute_old_value: bool,
    pub character_data_old_value: bool,
}

pub struct MutationObserver {
    #[allow(dead_code)]
    pub(crate) callback: ObserverCallback,
    pub(crate) options: MutationObserverOptions,
}

impl std::fmt::Debug for MutationObserver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MutationObserver")
            .field("options", &self.options)
            .finish()
    }
}

impl MutationObserver {
    pub fn new(
        callback: impl Fn(&[MutationRecord]) + Send + Sync + 'static,
        options: MutationObserverOptions,
    ) -> Self {
        Self {
            callback: Box::new(callback),
            options,
        }
    }

    pub fn observe(&self, _target: &ElementHandle) -> DomResult<crate::state::ObserverHandle> {
        Ok(crate::state::ObserverHandle::new())
    }

    pub fn disconnect(&self, _handle: &crate::state::ObserverHandle) -> DomResult<()> {
        Ok(())
    }

    pub fn take_records(&self, _handle: &crate::state::ObserverHandle) -> Vec<MutationRecord> {
        Vec::new()
    }
}
