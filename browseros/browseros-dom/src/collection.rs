use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use browseros_bridge::identifiers::{FrameId, PageId};
use browseros_bridge::traits::{ElementPort, LocatorEngine};
use browseros_bridge::types::NodeType;
use browseros_types::identifiers::HandleId;

use crate::element::ElementHandle;
use crate::error::{DomError, DomResult};
use crate::node::{NodeHandle, NodeHandleKind};
use crate::snapshot::NodeSnapshot;

#[allow(dead_code)]
pub(crate) enum CollectionKind {
    Live(Arc<dyn LiveQuery>),
    Static(Vec<NodeHandle>),
}

pub struct NodeCollection {
    pub(crate) kind: CollectionKind,
    pub(crate) length: usize,
}

impl std::fmt::Debug for NodeCollection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeCollection")
            .field("length", &self.length)
            .finish()
    }
}

impl NodeCollection {
    pub fn from_vec(handles: Vec<NodeHandle>) -> Self {
        let len = handles.len();
        Self {
            kind: CollectionKind::Static(handles),
            length: len,
        }
    }

    pub fn get(&self, index: usize) -> DomResult<Option<NodeHandle>> {
        match &self.kind {
            CollectionKind::Static(handles) => Ok(handles.get(index).cloned()),
            CollectionKind::Live(lq) => {
                let all = lq.refresh().map_err(DomError::BridgeError)?;
                Ok(all.into_iter().nth(index))
            }
        }
    }

    pub fn iter(&self) -> NodeIterator<'_> {
        NodeIterator::new(self)
    }

    pub fn filter(&self, predicate: impl Fn(&NodeHandle) -> bool) -> DomResult<NodeCollection> {
        let all = self.to_vec()?;
        let filtered: Vec<NodeHandle> = all.into_iter().filter(|n| predicate(n)).collect();
        Ok(NodeCollection::from_vec(filtered))
    }

    pub fn to_vec(&self) -> DomResult<Vec<NodeHandle>> {
        match &self.kind {
            CollectionKind::Static(handles) => Ok(handles.clone()),
            CollectionKind::Live(lq) => {
                let all = lq.refresh().map_err(DomError::BridgeError)?;
                Ok(all)
            }
        }
    }

    pub fn is_live(&self) -> bool {
        matches!(self.kind, CollectionKind::Live(_))
    }

    pub fn snapshot(&self) -> DomResult<Vec<NodeSnapshot>> {
        let handles = self.to_vec()?;
        let mut snaps = Vec::new();
        for handle in handles {
            snaps.push(handle.snapshot()?);
        }
        Ok(snaps)
    }
}

pub struct ElementCollection {
    pub(crate) inner: NodeCollection,
}

impl std::fmt::Debug for ElementCollection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ElementCollection").finish()
    }
}

impl ElementCollection {
    pub fn from_vec(handles: Vec<ElementHandle>) -> Self {
        let nodes: Vec<NodeHandle> = handles
            .into_iter()
            .map(|el| NodeHandle {
                kind: NodeHandleKind::Element(el),
                node_type: NodeType::Element,
                handle_id: HandleId::new(),
            })
            .collect();
        Self {
            inner: NodeCollection::from_vec(nodes),
        }
    }

    pub fn from_ports(
        ports: Vec<Box<dyn ElementPort>>,
        locator_engine: Arc<dyn LocatorEngine>,
        generation: Arc<AtomicU64>,
        frame_id: FrameId,
        page_id: PageId,
    ) -> Self {
        let handles: Vec<ElementHandle> = ports
            .into_iter()
            .map(|port| {
                ElementHandle::new(
                    port,
                    Arc::clone(&locator_engine),
                    HandleId::new(),
                    Arc::clone(&generation),
                    frame_id,
                    page_id,
                )
            })
            .collect();
        Self::from_vec(handles)
    }

    pub fn get(&self, index: usize) -> DomResult<Option<ElementHandle>> {
        match self.inner.get(index)? {
            Some(n) => {
                if let Some(el) = n.as_element() {
                    Ok(Some(el))
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }

    pub fn iter(&self) -> ElementIterator<'_> {
        ElementIterator::new(self)
    }

    pub fn first(&self) -> DomResult<Option<ElementHandle>> {
        self.get(0)
    }

    pub fn last(&self) -> DomResult<Option<ElementHandle>> {
        let nodes = self.inner.to_vec()?;
        if nodes.is_empty() {
            return Ok(None);
        }
        self.get(nodes.len() - 1)
    }

    pub fn count(&self) -> DomResult<usize> {
        self.inner.to_vec().map(|v| v.len())
    }

    pub fn nth(&self, n: usize) -> DomResult<Option<ElementHandle>> {
        self.get(n)
    }

    pub fn to_vec(&self) -> DomResult<Vec<ElementHandle>> {
        let nodes = self.inner.to_vec()?;
        let mut els = Vec::new();
        for node in nodes {
            if let Some(el) = node.as_element() {
                els.push(el);
            }
        }
        Ok(els)
    }

    pub fn snapshot(&self) -> DomResult<Vec<NodeSnapshot>> {
        self.inner.snapshot()
    }
}

pub trait LiveQuery: Send + Sync {
    fn refresh(&self) -> Result<Vec<NodeHandle>, browseros_bridge::error::BridgeError>;
}

pub struct NodeIterator<'a> {
    collection: &'a NodeCollection,
    index: usize,
}

impl<'a> NodeIterator<'a> {
    pub fn new(collection: &'a NodeCollection) -> Self {
        Self {
            collection,
            index: 0,
        }
    }
}

impl<'a> Iterator for NodeIterator<'a> {
    type Item = DomResult<NodeHandle>;

    fn next(&mut self) -> Option<Self::Item> {
        let result = self.collection.get(self.index);
        self.index += 1;
        match result {
            Ok(Some(handle)) => Some(Ok(handle)),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        }
    }
}

pub struct ElementIterator<'a> {
    collection: &'a ElementCollection,
    index: usize,
}

impl<'a> ElementIterator<'a> {
    pub fn new(collection: &'a ElementCollection) -> Self {
        Self {
            collection,
            index: 0,
        }
    }
}

impl<'a> Iterator for ElementIterator<'a> {
    type Item = DomResult<ElementHandle>;

    fn next(&mut self) -> Option<Self::Item> {
        let result = self.collection.get(self.index);
        self.index += 1;
        match result {
            Ok(Some(handle)) => Some(Ok(handle)),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        }
    }
}
