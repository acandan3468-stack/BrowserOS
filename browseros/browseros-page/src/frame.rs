use std::collections::HashMap;

use browseros_bridge::identifiers::FrameId;

#[derive(Debug, Clone)]
pub struct FrameNode {
    pub frame_id: FrameId,
    pub url: String,
    pub parent_id: Option<FrameId>,
    pub children: Vec<FrameNode>,
    pub is_main_frame: bool,
}

impl FrameNode {
    pub fn new_main(frame_id: FrameId, url: String) -> Self {
        FrameNode {
            frame_id,
            url,
            parent_id: None,
            children: Vec::new(),
            is_main_frame: true,
        }
    }

    pub fn new_child(frame_id: FrameId, url: String, parent_id: FrameId) -> Self {
        FrameNode {
            frame_id,
            url,
            parent_id: Some(parent_id),
            children: Vec::new(),
            is_main_frame: false,
        }
    }

    pub fn add_child(&mut self, child: FrameNode) {
        self.children.push(child);
    }

    pub fn find(&self, frame_id: FrameId) -> Option<&FrameNode> {
        if self.frame_id == frame_id {
            return Some(self);
        }
        for child in &self.children {
            if let Some(found) = child.find(frame_id) {
                return Some(found);
            }
        }
        None
    }

    pub fn find_mut(&mut self, frame_id: FrameId) -> Option<&mut FrameNode> {
        if self.frame_id == frame_id {
            return Some(self);
        }
        for child in &mut self.children {
            if let Some(found) = child.find_mut(frame_id) {
                return Some(found);
            }
        }
        None
    }

    pub fn remove(&mut self, frame_id: FrameId) -> Option<FrameNode> {
        let pos = self.children.iter().position(|c| c.frame_id == frame_id);
        if let Some(idx) = pos {
            return Some(self.children.remove(idx));
        }
        for child in &mut self.children {
            if let Some(found) = child.remove(frame_id) {
                return Some(found);
            }
        }
        None
    }

    pub fn count_descendants(&self) -> usize {
        self.children.len()
            + self
                .children
                .iter()
                .map(|c| c.count_descendants())
                .sum::<usize>()
    }

    pub fn all_frames(&self) -> Vec<&FrameNode> {
        let mut result = vec![self];
        for child in &self.children {
            result.extend(child.all_frames());
        }
        result
    }

    pub fn depth(&self, frame_id: FrameId) -> Option<usize> {
        if self.frame_id == frame_id {
            return Some(0);
        }
        for child in &self.children {
            if let Some(d) = child.depth(frame_id) {
                return Some(d + 1);
            }
        }
        None
    }
}

#[derive(Debug, Clone)]
pub struct FrameTree {
    main_frame: Option<FrameNode>,
    nodes: HashMap<FrameId, FrameNode>,
}

impl FrameTree {
    pub fn new() -> Self {
        FrameTree {
            main_frame: None,
            nodes: HashMap::new(),
        }
    }

    pub fn set_main_frame(&mut self, node: FrameNode) {
        let id = node.frame_id;
        self.main_frame = Some(node);
        if let Some(ref main) = self.main_frame {
            self.nodes.insert(id, main.clone());
        }
    }

    pub fn main_frame(&self) -> Option<&FrameNode> {
        self.main_frame.as_ref()
    }

    pub fn main_frame_id(&self) -> Option<FrameId> {
        self.main_frame.as_ref().map(|n| n.frame_id)
    }

    pub fn attach(&mut self, parent_id: FrameId, child: FrameNode) {
        let has_parent = self
            .main_frame
            .as_ref()
            .and_then(|m| m.find(parent_id))
            .is_some();
        if has_parent {
            if let Some(ref mut main) = self.main_frame {
                if let Some(parent) = main.find_mut(parent_id) {
                    parent.add_child(child.clone());
                }
            }
            self.nodes.insert(child.frame_id, child);
        }
    }

    pub fn detach(&mut self, frame_id: FrameId) -> Option<FrameNode> {
        if let Some(ref mut main) = self.main_frame {
            if main.frame_id == frame_id {
                let removed = self.main_frame.take();
                self.nodes.clear();
                return removed;
            }
            if let Some(removed) = main.remove(frame_id) {
                self.nodes.remove(&frame_id);
                self.remove_descendants_from_map(&removed);
                return Some(removed);
            }
        }
        None
    }

    fn remove_descendants_from_map(&mut self, node: &FrameNode) {
        for child in &node.children {
            self.nodes.remove(&child.frame_id);
            self.remove_descendants_from_map(child);
        }
    }

    pub fn get(&self, frame_id: FrameId) -> Option<&FrameNode> {
        self.nodes.get(&frame_id)
    }

    pub fn has(&self, frame_id: FrameId) -> bool {
        self.nodes.contains_key(&frame_id)
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn replace(&mut self, frame_id: FrameId, mut new_node: FrameNode) -> Option<FrameNode> {
        let old = self.get(frame_id)?.clone();
        if old.is_main_frame {
            self.main_frame = Some(new_node);
            self.nodes.clear();
            if let Some(ref main) = self.main_frame {
                self.nodes.insert(main.frame_id, main.clone());
                let mut stack: Vec<&FrameNode> = main.children.iter().collect();
                while let Some(node) = stack.pop() {
                    self.nodes.insert(node.frame_id, node.clone());
                    stack.extend(&node.children);
                }
            }
        } else {
            let _ = self.detach(frame_id);
            new_node.children = old.children.clone();
            self.nodes.insert(frame_id, new_node);
        }
        Some(old)
    }

    pub fn traverse<F>(&self, f: &mut F)
    where
        F: FnMut(&FrameNode),
    {
        if let Some(ref main) = self.main_frame {
            traverse_node(main, f);
        }
    }

    pub fn depth(&self, frame_id: FrameId) -> Option<usize> {
        self.main_frame.as_ref().and_then(|m| m.depth(frame_id))
    }

    pub fn ancestor_ids(&self, frame_id: FrameId) -> Vec<FrameId> {
        let mut ancestors = Vec::new();
        if let Some(mut current) = self.get(frame_id).cloned() {
            while let Some(pid) = current.parent_id {
                ancestors.push(pid);
                if let Some(parent) = self.get(pid) {
                    current = parent.clone();
                } else {
                    break;
                }
            }
        }
        ancestors
    }
}

fn traverse_node<F>(node: &FrameNode, f: &mut F)
where
    F: FnMut(&FrameNode),
{
    f(node);
    for child in &node.children {
        traverse_node(child, f);
    }
}

impl Default for FrameTree {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_id(_n: u64) -> FrameId {
        FrameId::new()
    }

    #[test]
    fn test_empty_tree() {
        let tree = FrameTree::new();
        assert!(tree.is_empty());
        assert_eq!(tree.len(), 0);
        assert!(tree.main_frame().is_none());
    }

    #[test]
    fn test_set_main_frame() {
        let mut tree = FrameTree::new();
        let id = FrameId::new();
        tree.set_main_frame(FrameNode::new_main(id, "https://example.com".into()));
        assert_eq!(tree.len(), 1);
        assert!(tree.main_frame().is_some());
        assert_eq!(tree.main_frame_id(), Some(id));
    }

    #[test]
    fn test_attach_child() {
        let mut tree = FrameTree::new();
        let main_id = FrameId::new();
        tree.set_main_frame(FrameNode::new_main(main_id, "https://example.com".into()));
        let child_id = FrameId::new();
        tree.attach(
            main_id,
            FrameNode::new_child(child_id, "https://example.com/iframe".into(), main_id),
        );
        assert_eq!(tree.len(), 2);
        assert!(tree.has(child_id));
        assert_eq!(tree.main_frame().unwrap().children.len(), 1);
    }

    #[test]
    fn test_detach_main_frame_clears_tree() {
        let mut tree = FrameTree::new();
        let main_id = FrameId::new();
        tree.set_main_frame(FrameNode::new_main(main_id, "https://example.com".into()));
        let child_id = FrameId::new();
        tree.attach(
            main_id,
            FrameNode::new_child(child_id, "https://example.com/iframe".into(), main_id),
        );
        let removed = tree.detach(main_id);
        assert!(removed.is_some());
        assert!(tree.is_empty());
    }

    #[test]
    fn test_detach_child() {
        let mut tree = FrameTree::new();
        let main_id = FrameId::new();
        tree.set_main_frame(FrameNode::new_main(main_id, "https://example.com".into()));
        let child_id = FrameId::new();
        tree.attach(
            main_id,
            FrameNode::new_child(child_id, "https://example.com/iframe".into(), main_id),
        );
        let removed = tree.detach(child_id);
        assert!(removed.is_some());
        assert!(!tree.has(child_id));
        assert!(tree.main_frame().is_some());
    }

    #[test]
    fn test_get_node() {
        let mut tree = FrameTree::new();
        let main_id = FrameId::new();
        tree.set_main_frame(FrameNode::new_main(main_id, "https://example.com".into()));
        let node = tree.get(main_id);
        assert!(node.is_some());
        assert_eq!(node.unwrap().url, "https://example.com");
    }

    #[test]
    fn test_get_nonexistent() {
        let tree = FrameTree::new();
        assert!(tree.get(FrameId::new()).is_none());
    }

    #[test]
    fn test_attach_nonexistent_parent_does_nothing() {
        let mut tree = FrameTree::new();
        let main_id = FrameId::new();
        tree.set_main_frame(FrameNode::new_main(main_id, "https://example.com".into()));
        let child_id = FrameId::new();
        tree.attach(
            FrameId::new(),
            FrameNode::new_child(
                child_id,
                "https://example.com/iframe".into(),
                FrameId::new(),
            ),
        );
        assert!(!tree.has(child_id));
    }

    #[test]
    fn test_detach_nonexistent_returns_none() {
        let mut tree = FrameTree::new();
        tree.set_main_frame(FrameNode::new_main(
            FrameId::new(),
            "https://example.com".into(),
        ));
        assert!(tree.detach(FrameId::new()).is_none());
    }

    #[test]
    fn test_replace_main_frame() {
        let mut tree = FrameTree::new();
        let main_id = FrameId::new();
        tree.set_main_frame(FrameNode::new_main(main_id, "https://example.com".into()));
        let new_main = FrameNode::new_main(main_id, "https://example.org".into());
        let old = tree.replace(main_id, new_main);
        assert!(old.is_some());
        assert_eq!(tree.get(main_id).unwrap().url, "https://example.org");
    }

    #[test]
    fn test_traverse() {
        let mut tree = FrameTree::new();
        let main_id = FrameId::new();
        tree.set_main_frame(FrameNode::new_main(main_id, "https://example.com".into()));
        let child_id = FrameId::new();
        tree.attach(
            main_id,
            FrameNode::new_child(child_id, "https://example.com/iframe".into(), main_id),
        );
        let mut visited = Vec::new();
        tree.traverse(&mut |n: &FrameNode| visited.push(n.frame_id));
        assert_eq!(visited.len(), 2);
    }

    #[test]
    fn test_depth() {
        let mut tree = FrameTree::new();
        let main_id = FrameId::new();
        tree.set_main_frame(FrameNode::new_main(main_id, "https://example.com".into()));
        let child_id = FrameId::new();
        tree.attach(
            main_id,
            FrameNode::new_child(child_id, "https://example.com/iframe".into(), main_id),
        );
        assert_eq!(tree.depth(main_id), Some(0));
        assert_eq!(tree.depth(child_id), Some(1));
        assert!(tree.depth(FrameId::new()).is_none());
    }

    #[test]
    fn test_frame_node_all_frames() {
        let main_id = FrameId::new();
        let mut main = FrameNode::new_main(main_id, "https://example.com".into());
        let child_id = FrameId::new();
        main.add_child(FrameNode::new_child(
            child_id,
            "https://example.com/iframe".into(),
            main_id,
        ));
        let all = main.all_frames();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_frame_node_count_descendants() {
        let main_id = FrameId::new();
        let mut main = FrameNode::new_main(main_id, "https://example.com".into());
        main.add_child(FrameNode::new_child(
            FrameId::new(),
            "https://example.com/iframe".into(),
            main_id,
        ));
        assert_eq!(main.count_descendants(), 1);
    }

    #[test]
    fn test_frame_node_find() {
        let main_id = FrameId::new();
        let mut main = FrameNode::new_main(main_id, "https://example.com".into());
        let child_id = FrameId::new();
        main.add_child(FrameNode::new_child(
            child_id,
            "https://example.com/iframe".into(),
            main_id,
        ));
        assert!(main.find(child_id).is_some());
        assert!(main.find(FrameId::new()).is_none());
    }

    #[test]
    fn test_frame_node_remove() {
        let main_id = FrameId::new();
        let mut main = FrameNode::new_main(main_id, "https://example.com".into());
        let child_id = FrameId::new();
        main.add_child(FrameNode::new_child(
            child_id,
            "https://example.com/iframe".into(),
            main_id,
        ));
        let removed = main.remove(child_id);
        assert!(removed.is_some());
        assert!(main.children.is_empty());
    }
}
