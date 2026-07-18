use std::sync::Arc;

use browseros_bridge::identifiers::NodeId;
use browseros_bridge::traits::ElementPort;
use browseros_bridge::types::NodeType;
use browseros_types::identifiers::HandleId;

use crate::collection::NodeCollection;
use crate::element::ElementHandle;
use crate::error::{DomError, DomResult};
use crate::shadow::ShadowRootHandle;
use crate::snapshot::NodeSnapshot;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) enum NodeHandleKind {
    Element(ElementHandle),
    Text(TextNode),
    Comment(CommentNode),
    DocumentFragment(DocumentFragment),
    ShadowRoot(ShadowRootHandle),
}

#[derive(Debug, Clone)]
pub struct NodeHandle {
    pub(crate) kind: NodeHandleKind,
    pub(crate) node_type: NodeType,
    pub(crate) handle_id: HandleId,
}

impl NodeHandle {
    pub fn node_type(&self) -> NodeType {
        self.node_type
    }

    pub fn id(&self) -> HandleId {
        self.handle_id
    }

    pub fn as_element(&self) -> Option<ElementHandle> {
        match &self.kind {
            NodeHandleKind::Element(el) => Some(el.clone()),
            _ => None,
        }
    }

    pub fn as_text(&self) -> Option<&TextNode> {
        match &self.kind {
            NodeHandleKind::Text(t) => Some(t),
            _ => None,
        }
    }

    pub fn as_comment(&self) -> Option<&CommentNode> {
        match &self.kind {
            NodeHandleKind::Comment(c) => Some(c),
            _ => None,
        }
    }

    pub fn as_document_fragment(&self) -> Option<&DocumentFragment> {
        match &self.kind {
            NodeHandleKind::DocumentFragment(df) => Some(df),
            _ => None,
        }
    }

    pub fn as_shadow_root(&self) -> Option<&ShadowRootHandle> {
        match &self.kind {
            NodeHandleKind::ShadowRoot(sr) => Some(sr),
            _ => None,
        }
    }

    pub fn parent_node(&self) -> DomResult<Option<NodeHandle>> {
        match &self.kind {
            NodeHandleKind::Element(el) => {
                let parent = el.parent()?;
                Ok(parent.map(|p| NodeHandle {
                    kind: NodeHandleKind::Element(p),
                    node_type: NodeType::Element,
                    handle_id: HandleId::new(),
                }))
            }
            _ => Err(DomError::NotSupported(
                "parent_node not supported for this node type".to_string(),
            )),
        }
    }

    pub fn child_nodes(&self) -> DomResult<NodeCollection> {
        match &self.kind {
            NodeHandleKind::Element(el) => {
                let children = el.children()?;
                let handles: Vec<NodeHandle> = children
                    .to_vec()?
                    .into_iter()
                    .map(|e| NodeHandle {
                        kind: NodeHandleKind::Element(e),
                        node_type: NodeType::Element,
                        handle_id: HandleId::new(),
                    })
                    .collect();
                Ok(NodeCollection::from_vec(handles))
            }
            _ => Err(DomError::NotSupported(
                "child_nodes not supported for this node type".to_string(),
            )),
        }
    }

    pub fn text_content(&self) -> DomResult<String> {
        match &self.kind {
            NodeHandleKind::Element(el) => el.text_content(),
            NodeHandleKind::Text(t) => Ok(t.text.clone()),
            NodeHandleKind::Comment(c) => Ok(c.data.clone()),
            NodeHandleKind::DocumentFragment(_) => Ok(String::new()),
            NodeHandleKind::ShadowRoot(_) => Ok(String::new()),
        }
    }

    pub fn snapshot(&self) -> DomResult<NodeSnapshot> {
        match &self.kind {
            NodeHandleKind::Element(el) => el.snapshot(),
            NodeHandleKind::Text(t) => Ok(NodeSnapshot {
                node_id: NodeId::new(0),
                handle_id: self.handle_id,
                node_type: NodeType::Text,
                tag_name: String::new(),
                attributes: std::collections::HashMap::new(),
                text: t.text.clone(),
                children: Vec::new(),
                frame_id: None,
                bounding_box: None,
                is_visible: false,
                is_enabled: false,
                timestamp: chrono::Utc::now(),
            }),
            NodeHandleKind::Comment(c) => Ok(NodeSnapshot {
                node_id: NodeId::new(0),
                handle_id: self.handle_id,
                node_type: NodeType::Comment,
                tag_name: String::new(),
                attributes: std::collections::HashMap::new(),
                text: c.data.clone(),
                children: Vec::new(),
                frame_id: None,
                bounding_box: None,
                is_visible: false,
                is_enabled: false,
                timestamp: chrono::Utc::now(),
            }),
            NodeHandleKind::DocumentFragment(_) => Ok(NodeSnapshot {
                node_id: NodeId::new(0),
                handle_id: self.handle_id,
                node_type: NodeType::DocumentFragment,
                tag_name: String::new(),
                attributes: std::collections::HashMap::new(),
                text: String::new(),
                children: Vec::new(),
                frame_id: None,
                bounding_box: None,
                is_visible: false,
                is_enabled: false,
                timestamp: chrono::Utc::now(),
            }),
            NodeHandleKind::ShadowRoot(sr) => sr.host.snapshot(),
        }
    }
}

pub struct TextNode {
    pub(crate) handle_id: HandleId,
    pub(crate) element_port: Arc<dyn ElementPort>,
    pub(crate) text: String,
}

impl std::fmt::Debug for TextNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextNode")
            .field("handle_id", &self.handle_id)
            .field("text", &self.text)
            .finish()
    }
}

impl Clone for TextNode {
    fn clone(&self) -> Self {
        Self {
            handle_id: self.handle_id,
            element_port: Arc::clone(&self.element_port),
            text: self.text.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CommentNode {
    #[allow(dead_code)]
    pub(crate) handle_id: HandleId,
    pub(crate) data: String,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DocumentFragment {
    pub(crate) handle_id: HandleId,
    pub(crate) nodes: Vec<ElementHandle>,
}
