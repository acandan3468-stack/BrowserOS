use crate::element::ElementHandle;
use crate::error::{DomError, DomResult};

#[allow(dead_code)]
pub struct WhatToShow;

impl WhatToShow {
    pub const ELEMENT: u32 = 1;
    pub const TEXT: u32 = 2;
    pub const COMMENT: u32 = 4;
    pub const DOCUMENT_FRAGMENT: u32 = 8;
    pub const ALL: u32 = !0u32;
}

pub struct TreeWalker {
    pub(crate) current: Option<ElementHandle>,
    pub(crate) root: ElementHandle,
    pub(crate) what_to_show: u32,
}

impl std::fmt::Debug for TreeWalker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TreeWalker").finish()
    }
}

impl TreeWalker {
    pub fn new(root: &ElementHandle, what_to_show: u32) -> Self {
        Self {
            current: Some(root.clone()),
            root: root.clone(),
            what_to_show,
        }
    }

    fn check_current(&self) -> DomResult<&ElementHandle> {
        self.current
            .as_ref()
            .ok_or_else(|| DomError::InvalidHandle("TreeWalker has no current node".to_string()))
    }

    fn accept_node(&self, _el: &ElementHandle) -> bool {
        if self.what_to_show & WhatToShow::ELEMENT != 0 {
            return true;
        }
        false
    }

    pub fn parent_node(&mut self) -> DomResult<Option<ElementHandle>> {
        let current = self.check_current()?.clone();
        let parent = current.parent()?;
        if let Some(ref p) = parent {
            if self.accept_node(p) {
                self.current = Some(p.clone());
            }
        }
        Ok(parent)
    }

    pub fn first_child(&mut self) -> DomResult<Option<ElementHandle>> {
        let current = self.check_current()?.clone();
        let children = current.children()?;
        let first = children.first()?;
        if let Some(ref f) = first {
            if self.accept_node(f) {
                self.current = Some(f.clone());
            }
        }
        Ok(first)
    }

    pub fn last_child(&mut self) -> DomResult<Option<ElementHandle>> {
        let current = self.check_current()?.clone();
        let children = current.children()?;
        let last = children.last()?;
        if let Some(ref l) = last {
            if self.accept_node(l) {
                self.current = Some(l.clone());
            }
        }
        Ok(last)
    }

    pub fn next_sibling(&mut self) -> DomResult<Option<ElementHandle>> {
        let current = self.check_current()?.clone();
        let next = current.next_sibling()?;
        if let Some(ref n) = next {
            if self.accept_node(n) {
                self.current = Some(n.clone());
            }
        }
        Ok(next)
    }

    pub fn previous_sibling(&mut self) -> DomResult<Option<ElementHandle>> {
        let current = self.check_current()?.clone();
        let prev = current.previous_sibling()?;
        if let Some(ref p) = prev {
            if self.accept_node(p) {
                self.current = Some(p.clone());
            }
        }
        Ok(prev)
    }

    pub fn next_node(&mut self) -> DomResult<Option<ElementHandle>> {
        let current = self.check_current()?.clone();

        let first_child = current.children()?.first()?;
        if let Some(child) = first_child {
            if self.accept_node(&child) {
                self.current = Some(child.clone());
                return Ok(Some(child));
            }
        }

        let mut node = current;
        loop {
            let next = node.next_sibling()?;
            if let Some(sib) = next {
                if self.accept_node(&sib) {
                    self.current = Some(sib.clone());
                    return Ok(Some(sib));
                }
                node = sib;
                continue;
            }

            let parent = node.parent()?;
            match parent {
                Some(p) => {
                    if p.id() == self.root.id() {
                        return Ok(None);
                    }
                    node = p;
                }
                None => return Ok(None),
            }
        }
    }

    pub fn previous_node(&mut self) -> DomResult<Option<ElementHandle>> {
        let current = self.check_current()?.clone();

        let prev = current.previous_sibling()?;
        if let Some(sib) = prev {
            let mut node = sib;
            loop {
                let children = node.children()?;
                let last = children.last()?;
                match last {
                    Some(child) => {
                        node = child;
                    }
                    None => break,
                }
            }
            if self.accept_node(&node) {
                self.current = Some(node.clone());
                return Ok(Some(node));
            }
        }

        let parent = current.parent()?;
        if let Some(ref p) = parent {
            if p.id() == self.root.id() {
                self.current = None;
                return Ok(None);
            }
            if self.accept_node(p) {
                self.current = Some(p.clone());
            }
        }
        Ok(parent)
    }

    pub fn current_node(&self) -> Option<&ElementHandle> {
        self.current.as_ref()
    }

    pub fn reset(&mut self) {
        self.current = Some(self.root.clone());
    }
}

pub struct AncestorIterator {
    current: Option<ElementHandle>,
}

impl AncestorIterator {
    pub fn new(start: ElementHandle) -> Self {
        Self {
            current: Some(start),
        }
    }
}

impl Iterator for AncestorIterator {
    type Item = DomResult<ElementHandle>;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.current.take()?;
        match current.parent() {
            Ok(Some(parent)) => {
                self.current = Some(parent.clone());
                Some(Ok(parent))
            }
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        }
    }
}

pub struct DescendantIterator {
    stack: Vec<ElementHandle>,
}

impl DescendantIterator {
    pub fn new(start: ElementHandle) -> Self {
        Self { stack: vec![start] }
    }
}

impl Iterator for DescendantIterator {
    type Item = DomResult<ElementHandle>;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.stack.pop()?;
        match current.children() {
            Ok(children) => {
                let mut child_handles = match children.to_vec() {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                child_handles.reverse();
                self.stack.extend(child_handles);
                Some(Ok(current))
            }
            Err(e) => Some(Err(e)),
        }
    }
}

pub struct SiblingIterator {
    current: Option<ElementHandle>,
    direction: i32,
}

impl SiblingIterator {
    pub fn forward(start: ElementHandle) -> Self {
        Self {
            current: Some(start),
            direction: 1,
        }
    }

    pub fn backward(start: ElementHandle) -> Self {
        Self {
            current: Some(start),
            direction: -1,
        }
    }
}

impl Iterator for SiblingIterator {
    type Item = DomResult<ElementHandle>;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.current.take()?;
        if self.direction > 0 {
            match current.next_sibling() {
                Ok(Some(sib)) => {
                    self.current = Some(sib.clone());
                    Some(Ok(sib))
                }
                Ok(None) => None,
                Err(e) => Some(Err(e)),
            }
        } else {
            match current.previous_sibling() {
                Ok(Some(sib)) => {
                    self.current = Some(sib.clone());
                    Some(Ok(sib))
                }
                Ok(None) => None,
                Err(e) => Some(Err(e)),
            }
        }
    }
}
