use std::sync::atomic::AtomicU64;
use std::sync::{Arc, OnceLock};

use browseros_bridge::identifiers::{ElementId, FrameId, PageId};
use browseros_bridge::traits::{ElementPort, LocatorEngine};
use browseros_bridge::types::{NodeInfo, Point};
use browseros_types::identifiers::HandleId;

use crate::attributes::Attributes;
use crate::collection::ElementCollection;
use crate::error::{DomError, DomResult};
use crate::frame::FrameHandle;
use crate::query;
use crate::shadow::ShadowRootHandle;
use crate::snapshot::{BoundingBox, NodeSnapshot};
use crate::state::ElementStateHelper;
use crate::style::{ComputedStyle, StyleDeclaration};
use crate::traversal::{AncestorIterator, DescendantIterator};

pub struct ElementHandle {
    pub(crate) inner: Arc<dyn ElementPort>,
    pub(crate) locator_engine: Arc<dyn LocatorEngine>,
    pub(crate) handle_id: HandleId,
    pub(crate) generation: Arc<AtomicU64>,
    pub(crate) known_generation: u64,
    pub(crate) frame_id: FrameId,
    pub(crate) page_id: PageId,
    #[allow(dead_code)]
    pub(crate) node_info: OnceLock<Option<NodeInfo>>,
}

impl std::fmt::Debug for ElementHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ElementHandle")
            .field("handle_id", &self.handle_id)
            .field("frame_id", &self.frame_id)
            .field("page_id", &self.page_id)
            .field("backend_id", &self.inner.id())
            .finish()
    }
}

impl ElementHandle {
    pub fn new(
        inner: Box<dyn ElementPort>,
        locator_engine: Arc<dyn LocatorEngine>,
        handle_id: HandleId,
        generation: Arc<AtomicU64>,
        frame_id: FrameId,
        page_id: PageId,
    ) -> Self {
        let known_generation = generation.load(std::sync::atomic::Ordering::Relaxed);
        Self {
            inner: inner.into(),
            locator_engine,
            handle_id,
            generation,
            known_generation,
            frame_id,
            page_id,
            node_info: OnceLock::new(),
        }
    }

    pub fn id(&self) -> HandleId {
        self.handle_id
    }

    pub fn backend_id(&self) -> ElementId {
        self.inner.id()
    }

    pub fn tag_name(&self) -> DomResult<String> {
        self.check_stale()?;
        Ok(self.inner.tag_name())
    }

    pub fn text_content(&self) -> DomResult<String> {
        self.check_stale()?;
        self.inner.text_content().map_err(DomError::from)
    }

    pub fn inner_html(&self) -> DomResult<String> {
        self.check_stale()?;
        self.inner.inner_html().map_err(DomError::from)
    }

    pub fn outer_html(&self) -> DomResult<String> {
        self.check_stale()?;
        self.inner.outer_html().map_err(DomError::from)
    }

    pub fn attributes(&self) -> DomResult<Attributes> {
        self.check_stale()?;
        Ok(Attributes::new(
            Arc::clone(&self.inner),
            self.clone_handle(),
        ))
    }

    pub fn attribute(&self, name: &str) -> DomResult<Option<String>> {
        self.check_stale()?;
        self.inner.get_attribute(name).map_err(DomError::from)
    }

    pub fn set_attribute(&self, name: &str, value: &str) -> DomResult<()> {
        self.check_stale()?;
        self.inner
            .set_attribute(name, value)
            .map_err(DomError::from)
    }

    pub fn remove_attribute(&self, name: &str) -> DomResult<()> {
        self.check_stale()?;
        self.inner.set_attribute(name, "").map_err(DomError::from)
    }

    pub fn has_attribute(&self, name: &str) -> DomResult<bool> {
        self.check_stale()?;
        self.inner.has_attribute(name).map_err(DomError::from)
    }

    pub fn class_list(&self) -> DomResult<crate::attributes::ClassList> {
        self.check_stale()?;
        Ok(crate::attributes::ClassList::new(
            Arc::clone(&self.inner),
            self.clone_handle(),
        ))
    }

    pub fn style(&self) -> DomResult<StyleDeclaration> {
        self.check_stale()?;
        Ok(StyleDeclaration::new(
            Arc::clone(&self.inner),
            self.clone_handle(),
        ))
    }

    pub fn computed_style(&self, pseudo_el: Option<&str>) -> DomResult<ComputedStyle> {
        self.check_stale()?;
        Ok(ComputedStyle::new(
            Arc::clone(&self.inner),
            self.clone_handle(),
            pseudo_el.map(|s| s.to_string()),
        ))
    }

    pub fn bounding_box(&self) -> DomResult<Option<BoundingBox>> {
        self.check_stale()?;
        self.inner
            .bounding_box()
            .map_err(DomError::from)
            .map(|opt| opt.as_ref().map(BoundingBox::from))
    }

    pub fn is_visible(&self) -> DomResult<bool> {
        self.check_stale()?;
        self.inner.is_visible().map_err(DomError::from)
    }

    pub fn is_enabled(&self) -> DomResult<bool> {
        self.check_stale()?;
        self.inner.is_enabled().map_err(DomError::from)
    }

    pub fn is_checked(&self) -> DomResult<bool> {
        self.check_stale()?;
        self.inner.is_checked().map_err(DomError::from)
    }

    pub fn is_selected(&self) -> DomResult<bool> {
        self.check_stale()?;
        self.inner.is_selected().map_err(DomError::from)
    }

    pub fn is_stable(&self) -> DomResult<bool> {
        self.check_stale()?;
        self.inner.is_stable().map_err(DomError::from)
    }

    pub fn state(&self) -> DomResult<ElementStateHelper> {
        self.check_stale()?;
        ElementStateHelper::from_handle(self)
    }

    pub fn focus(&self) -> DomResult<()> {
        self.check_stale()?;
        self.inner.focus().map_err(DomError::from)
    }

    pub fn scroll_into_view(&self) -> DomResult<()> {
        self.check_stale()?;
        self.inner.scroll_into_view().map_err(DomError::from)
    }

    pub fn click_point(&self) -> DomResult<Point> {
        self.check_stale()?;
        self.inner.click_point().map_err(DomError::from)
    }

    pub fn snapshot(&self) -> DomResult<NodeSnapshot> {
        self.snapshot_with_depth(u32::MAX)
    }

    pub(crate) fn snapshot_with_depth(&self, max_depth: u32) -> DomResult<NodeSnapshot> {
        self.check_stale()?;
        let node_info = self.inner.snapshot().map_err(DomError::from)?;
        let bounding_box = self.inner.bounding_box().map_err(DomError::from)?;
        let is_visible = self.inner.is_visible().map_err(DomError::from)?;
        let is_enabled = self.inner.is_enabled().map_err(DomError::from)?;
        let children = if max_depth > 0 {
            node_info
                .children
                .into_iter()
                .map(|c| NodeSnapshot::from_node_info_depth(c, max_depth - 1))
                .collect()
        } else {
            Vec::new()
        };
        Ok(NodeSnapshot {
            node_id: node_info.node_id,
            handle_id: self.handle_id,
            node_type: node_info.node_type,
            tag_name: node_info.tag_name,
            attributes: node_info.attributes,
            text: node_info.text,
            children,
            frame_id: node_info.frame_id,
            bounding_box: bounding_box.as_ref().map(BoundingBox::from),
            is_visible,
            is_enabled,
            timestamp: chrono::Utc::now(),
        })
    }

    pub fn matches(&self, selector: &str) -> DomResult<bool> {
        self.check_stale()?;
        self.inner.is_matches(selector).map_err(DomError::from)
    }

    pub fn query(&self, selector: &str) -> DomResult<Option<ElementHandle>> {
        self.check_stale()?;
        let opt = self
            .inner
            .query_selector(selector)
            .map_err(DomError::from)?;
        Ok(opt.map(|port| ElementHandle {
            inner: port.into(),
            locator_engine: Arc::clone(&self.locator_engine),
            handle_id: HandleId::new(),
            generation: Arc::clone(&self.generation),
            known_generation: self.known_generation,
            frame_id: self.frame_id,
            page_id: self.page_id,
            node_info: OnceLock::new(),
        }))
    }

    pub fn query_all(&self, selector: &str) -> DomResult<ElementCollection> {
        self.check_stale()?;
        let ports = self
            .inner
            .query_selector_all(selector)
            .map_err(DomError::from)?;
        Ok(ElementCollection::from_ports(
            ports,
            Arc::clone(&self.locator_engine),
            Arc::clone(&self.generation),
            self.frame_id,
            self.page_id,
        ))
    }

    pub fn query_xpath(&self, expr: &str) -> DomResult<ElementCollection> {
        self.check_stale()?;
        let frame_port = self.inner.owning_frame();
        let ports = query::query_xpath_on_frame(&*frame_port, &*self.locator_engine, expr)?;
        Ok(ElementCollection::from_ports(
            ports,
            Arc::clone(&self.locator_engine),
            Arc::clone(&self.generation),
            self.frame_id,
            self.page_id,
        ))
    }

    pub fn query_by_text(&self, text: &str, exact: bool) -> DomResult<ElementCollection> {
        self.check_stale()?;
        let frame_port = self.inner.owning_frame();
        let ports =
            query::query_by_text_on_frame(&*frame_port, &*self.locator_engine, text, exact)?;
        Ok(ElementCollection::from_ports(
            ports,
            Arc::clone(&self.locator_engine),
            Arc::clone(&self.generation),
            self.frame_id,
            self.page_id,
        ))
    }

    pub fn parent(&self) -> DomResult<Option<ElementHandle>> {
        self.check_stale()?;
        let opt = query::find_parent(&*self.inner, &*self.locator_engine)?;
        Ok(opt.map(|port| ElementHandle {
            inner: port.into(),
            locator_engine: Arc::clone(&self.locator_engine),
            handle_id: HandleId::new(),
            generation: Arc::clone(&self.generation),
            known_generation: self.known_generation,
            frame_id: self.frame_id,
            page_id: self.page_id,
            node_info: OnceLock::new(),
        }))
    }

    pub fn children(&self) -> DomResult<ElementCollection> {
        self.check_stale()?;
        let ports = query::children_of(&*self.inner, &*self.locator_engine)?;
        Ok(ElementCollection::from_ports(
            ports,
            Arc::clone(&self.locator_engine),
            Arc::clone(&self.generation),
            self.frame_id,
            self.page_id,
        ))
    }

    pub fn next_sibling(&self) -> DomResult<Option<ElementHandle>> {
        self.check_stale()?;
        let opt = query::sibling_by_offset(&*self.inner, &*self.locator_engine, 1)?;
        Ok(opt.map(|port| ElementHandle {
            inner: port.into(),
            locator_engine: Arc::clone(&self.locator_engine),
            handle_id: HandleId::new(),
            generation: Arc::clone(&self.generation),
            known_generation: self.known_generation,
            frame_id: self.frame_id,
            page_id: self.page_id,
            node_info: OnceLock::new(),
        }))
    }

    pub fn previous_sibling(&self) -> DomResult<Option<ElementHandle>> {
        self.check_stale()?;
        let opt = query::sibling_by_offset(&*self.inner, &*self.locator_engine, -1)?;
        Ok(opt.map(|port| ElementHandle {
            inner: port.into(),
            locator_engine: Arc::clone(&self.locator_engine),
            handle_id: HandleId::new(),
            generation: Arc::clone(&self.generation),
            known_generation: self.known_generation,
            frame_id: self.frame_id,
            page_id: self.page_id,
            node_info: OnceLock::new(),
        }))
    }

    pub fn closest(&self, selector: &str) -> DomResult<Option<ElementHandle>> {
        self.check_stale()?;
        let opt = query::closest_by_traversal(&*self.inner, &*self.locator_engine, selector)?;
        Ok(opt.map(|port| ElementHandle {
            inner: port.into(),
            locator_engine: Arc::clone(&self.locator_engine),
            handle_id: HandleId::new(),
            generation: Arc::clone(&self.generation),
            known_generation: self.known_generation,
            frame_id: self.frame_id,
            page_id: self.page_id,
            node_info: OnceLock::new(),
        }))
    }

    pub fn ancestors(&self) -> AncestorIterator {
        AncestorIterator::new(self.clone_handle())
    }

    pub fn descendants(&self) -> DescendantIterator {
        DescendantIterator::new(self.clone_handle())
    }

    pub fn shadow_root(&self) -> DomResult<Option<ShadowRootHandle>> {
        self.check_stale()?;
        Ok(Some(ShadowRootHandle::new(
            self.clone_handle(),
            Arc::clone(&self.inner),
        )))
    }

    pub fn owning_frame(&self) -> FrameHandle {
        FrameHandle {
            frame_id: self.frame_id,
            locator_engine: Arc::clone(&self.locator_engine),
            page_id: self.page_id,
            parent_frame_id: None,
            generation: Arc::clone(&self.generation),
            frame_port: self.inner.owning_frame(),
        }
    }

    pub fn set_inner_html(&self, html: &str) -> DomResult<()> {
        self.check_stale()?;
        let frame_port = self.inner.owning_frame();
        let id = self.inner.id().get();
        let escaped = html.replace('\'', "\\'");
        let js =
            format!(r#"document.querySelector('[cdp_node_id="{id}"]').innerHTML = '{escaped}'"#);
        frame_port.evaluate(&js, None).map_err(DomError::from)?;
        Ok(())
    }

    pub fn set_text_content(&self, text: &str) -> DomResult<()> {
        self.check_stale()?;
        let frame_port = self.inner.owning_frame();
        let id = self.inner.id().get();
        let escaped = text.replace('\'', "\\'");
        let js =
            format!(r#"document.querySelector('[cdp_node_id="{id}"]').textContent = '{escaped}'"#);
        frame_port.evaluate(&js, None).map_err(DomError::from)?;
        Ok(())
    }

    pub fn remove(&self) -> DomResult<()> {
        self.check_stale()?;
        let frame_port = self.inner.owning_frame();
        let id = self.inner.id().get();
        let js = format!(r#"document.querySelector('[cdp_node_id="{id}"]')?.remove()"#);
        frame_port.evaluate(&js, None).map_err(DomError::from)?;
        Ok(())
    }

    pub fn is_stale(&self) -> bool {
        let current = self.generation.load(std::sync::atomic::Ordering::Relaxed);
        self.known_generation != current
    }

    pub fn backend_element_port(&self) -> &dyn ElementPort {
        &*self.inner
    }

    pub(crate) fn check_stale(&self) -> DomResult<()> {
        let current = self.generation.load(std::sync::atomic::Ordering::Relaxed);
        if self.known_generation != current {
            return Err(DomError::StaleElement(self.handle_id));
        }
        Ok(())
    }

    pub(crate) fn clone_handle(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            locator_engine: Arc::clone(&self.locator_engine),
            handle_id: self.handle_id,
            generation: Arc::clone(&self.generation),
            known_generation: self.known_generation,
            frame_id: self.frame_id,
            page_id: self.page_id,
            node_info: OnceLock::new(),
        }
    }
}

impl Clone for ElementHandle {
    fn clone(&self) -> Self {
        self.clone_handle()
    }
}
