use std::collections::HashMap;

use browseros_bridge::identifiers::{FrameId, NodeId};
use browseros_bridge::types::{BoxModel, NodeInfo, NodeType, Point};
use browseros_types::identifiers::HandleId;

#[derive(Debug, Clone)]
pub struct BoundingBox {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl BoundingBox {
    pub fn from(model: &BoxModel) -> Self {
        Self {
            x: model.x,
            y: model.y,
            width: model.width,
            height: model.height,
        }
    }

    pub fn center(&self) -> Point {
        Point {
            x: self.x + self.width / 2.0,
            y: self.y + self.height / 2.0,
        }
    }

    pub fn intersects(&self, other: &BoundingBox) -> bool {
        self.x < other.x + other.width
            && self.x + self.width > other.x
            && self.y < other.y + other.height
            && self.y + self.height > other.y
    }

    pub fn contains(&self, point: &Point) -> bool {
        point.x >= self.x
            && point.x <= self.x + self.width
            && point.y >= self.y
            && point.y <= self.y + self.height
    }

    pub fn area(&self) -> f64 {
        self.width * self.height
    }
}

#[derive(Debug, Clone)]
pub struct NodeSnapshot {
    pub node_id: NodeId,
    pub handle_id: HandleId,
    pub node_type: NodeType,
    pub tag_name: String,
    pub attributes: HashMap<String, String>,
    pub text: String,
    pub children: Vec<NodeSnapshot>,
    pub frame_id: Option<FrameId>,
    pub bounding_box: Option<BoundingBox>,
    pub is_visible: bool,
    pub is_enabled: bool,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl NodeSnapshot {
    pub fn from_node_info(info: NodeInfo) -> Self {
        Self::from_node_info_depth(info, u32::MAX)
    }

    pub(crate) fn from_node_info_depth(info: NodeInfo, max_depth: u32) -> Self {
        let children = if max_depth > 0 {
            info.children
                .into_iter()
                .map(|c| Self::from_node_info_depth(c, max_depth - 1))
                .collect()
        } else {
            Vec::new()
        };
        Self {
            node_id: info.node_id,
            handle_id: HandleId::new(),
            node_type: info.node_type,
            tag_name: info.tag_name,
            attributes: info.attributes,
            text: info.text,
            children,
            frame_id: info.frame_id,
            bounding_box: None,
            is_visible: false,
            is_enabled: false,
            timestamp: chrono::Utc::now(),
        }
    }

    pub fn find_id(&self, id: &str) -> Option<&NodeSnapshot> {
        if self.attributes.get("id").is_some_and(|v| v == id) {
            return Some(self);
        }
        for child in &self.children {
            if let Some(found) = child.find_id(id) {
                return Some(found);
            }
        }
        None
    }

    pub fn find_by_tag(&self, tag: &str) -> Vec<&NodeSnapshot> {
        let mut results = Vec::new();
        if self.tag_name.eq_ignore_ascii_case(tag) {
            results.push(self);
        }
        for child in &self.children {
            results.extend(child.find_by_tag(tag));
        }
        results
    }

    pub fn find_by_text(&self, text: &str, exact: bool) -> Vec<&NodeSnapshot> {
        let mut results = Vec::new();
        let matches = if exact {
            self.text == text
        } else {
            self.text.contains(text)
        };
        if matches {
            results.push(self);
        }
        for child in &self.children {
            results.extend(child.find_by_text(text, exact));
        }
        results
    }

    pub fn find_by_attr(&self, name: &str, value: &str) -> Vec<&NodeSnapshot> {
        let mut results = Vec::new();
        if self.attributes.get(name).is_some_and(|v| v == value) {
            results.push(self);
        }
        for child in &self.children {
            results.extend(child.find_by_attr(name, value));
        }
        results
    }

    pub fn path(&self) -> String {
        build_path(self)
    }
}

fn build_path(node: &NodeSnapshot) -> String {
    let mut parts = vec![node.tag_name.clone()];
    if let Some(id_val) = node.attributes.get("id") {
        parts.push(format!("#{id_val}"));
    }
    for (name, value) in &node.attributes {
        if name != "id" && name != "class" {
            parts.push(format!("[{name}=\"{value}\"]"));
            break;
        }
    }
    parts.join("")
}

#[derive(Debug, Clone)]
pub struct DomSnapshot {
    pub root: NodeSnapshot,
    pub url: String,
    pub title: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub frame_id: FrameId,
    pub frame_count: usize,
    pub metadata: HashMap<String, String>,
}

impl DomSnapshot {
    pub fn root(&self) -> &NodeSnapshot {
        &self.root
    }

    pub fn find_id(&self, id: &str) -> Option<&NodeSnapshot> {
        self.root.find_id(id)
    }

    pub fn find_by_tag(&self, tag: &str) -> Vec<&NodeSnapshot> {
        self.root.find_by_tag(tag)
    }

    pub fn find_by_text(&self, text: &str, exact: bool) -> Vec<&NodeSnapshot> {
        self.root.find_by_text(text, exact)
    }

    #[cfg(feature = "snapshot-css-matching")]
    pub fn find_by_selector(&self, selector: &str) -> Vec<&NodeSnapshot> {
        let mut results = Vec::new();
        collect_by_selector(&self.root, selector, &mut results);
        results
    }
}

#[cfg(feature = "snapshot-css-matching")]
fn collect_by_selector<'a>(
    node: &'a NodeSnapshot,
    selector: &str,
    results: &mut Vec<&'a NodeSnapshot>,
) {
    if matches_simple_selector(node, selector) {
        results.push(node);
    }
    for child in &node.children {
        collect_by_selector(child, selector, results);
    }
}

#[cfg(feature = "snapshot-css-matching")]
fn matches_simple_selector(node: &NodeSnapshot, selector: &str) -> bool {
    let selector = selector.trim();
    if let Some(tag) = selector.strip_prefix('.') {
        node.attributes
            .get("class")
            .map_or(false, |c| c.split_whitespace().any(|cl| cl == tag))
    } else if let Some(id) = selector.strip_prefix('#') {
        node.attributes.get("id").map_or(false, |v| v == id)
    } else {
        node.tag_name.eq_ignore_ascii_case(selector)
    }
}
