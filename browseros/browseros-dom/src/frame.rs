use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use browseros_bridge::identifiers::{FrameId, PageId};
use browseros_bridge::traits::{FramePort, LocatorEngine};
use browseros_bridge::types::JsResult;
use browseros_types::identifiers::HandleId;

use crate::collection::ElementCollection;
use crate::element::ElementHandle;
use crate::error::{DomError, DomResult};
use crate::locator::LocatorBuilder;
use crate::snapshot::DomSnapshot;

pub struct FrameHandle {
    pub(crate) frame_id: FrameId,
    pub(crate) locator_engine: Arc<dyn LocatorEngine>,
    pub(crate) page_id: PageId,
    pub(crate) parent_frame_id: Option<FrameId>,
    pub(crate) generation: Arc<AtomicU64>,
    pub(crate) frame_port: Box<dyn FramePort>,
}

impl std::fmt::Debug for FrameHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FrameHandle")
            .field("frame_id", &self.frame_id)
            .field("page_id", &self.page_id)
            .finish()
    }
}

impl FrameHandle {
    pub fn id(&self) -> FrameId {
        self.frame_id
    }

    pub fn parent_id(&self) -> Option<FrameId> {
        self.parent_frame_id
    }

    pub fn is_main_frame(&self) -> bool {
        self.parent_frame_id.is_none()
    }

    pub fn is_cross_origin(&self) -> bool {
        false
    }

    pub fn url(&self) -> DomResult<String> {
        Ok(self.frame_port.url())
    }

    pub fn title(&self) -> DomResult<String> {
        Ok(self.frame_port.title())
    }

    pub fn query(&self, selector: &str) -> DomResult<Option<ElementHandle>> {
        self.locator_engine
            .query_selector(selector)
            .map_err(DomError::from)
            .map(|opt| {
                opt.map(|port| {
                    ElementHandle::new(
                        port,
                        Arc::clone(&self.locator_engine),
                        HandleId::new(),
                        Arc::clone(&self.generation),
                        self.frame_id,
                        self.page_id,
                    )
                })
            })
    }

    pub fn query_all(&self, selector: &str) -> DomResult<ElementCollection> {
        let ports = self
            .locator_engine
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
        let ports = self
            .locator_engine
            .query_by_xpath(expr)
            .map_err(DomError::from)?;
        Ok(ElementCollection::from_ports(
            ports,
            Arc::clone(&self.locator_engine),
            Arc::clone(&self.generation),
            self.frame_id,
            self.page_id,
        ))
    }

    pub fn query_by_text(&self, text: &str, exact: bool) -> DomResult<ElementCollection> {
        let ports = self
            .locator_engine
            .query_by_text(text, exact)
            .map_err(DomError::from)?;
        Ok(ElementCollection::from_ports(
            ports,
            Arc::clone(&self.locator_engine),
            Arc::clone(&self.generation),
            self.frame_id,
            self.page_id,
        ))
    }

    pub fn locator(&self) -> LocatorBuilder {
        LocatorBuilder::new(Arc::clone(&self.locator_engine), self.frame_id)
    }

    pub fn snapshot(
        &self,
        max_depth: Option<u32>,
        _selector_filter: Option<&str>,
    ) -> DomResult<DomSnapshot> {
        let root_element = self
            .locator_engine
            .query_selector(":root")
            .map_err(DomError::from)?;
        let root_handle = root_element.map(|port| {
            ElementHandle::new(
                port,
                Arc::clone(&self.locator_engine),
                HandleId::new(),
                Arc::clone(&self.generation),
                self.frame_id,
                self.page_id,
            )
        });
        match root_handle {
            Some(el) => {
                let node_snap = el.snapshot_with_depth(max_depth.unwrap_or(10))?;
                let url = self.frame_port.url();
                let title = self.frame_port.title();
                Ok(DomSnapshot {
                    root: node_snap,
                    url,
                    title,
                    timestamp: chrono::Utc::now(),
                    frame_id: self.frame_id,
                    frame_count: 0,
                    metadata: std::collections::HashMap::new(),
                })
            }
            None => Err(DomError::NotFound("document root not found".to_string())),
        }
    }

    pub fn snapshot_all(&self) -> DomResult<DomSnapshot> {
        self.snapshot(None, None)
    }

    pub fn evaluate(&self, script: &str) -> DomResult<JsResult> {
        self.frame_port
            .evaluate(script, None)
            .map_err(DomError::from)
    }

    pub fn child_frames(&self) -> DomResult<Vec<FrameHandle>> {
        let ports = self.frame_port.child_frames();
        let handles = ports
            .into_iter()
            .map(|port| {
                let id = port.id();
                let parent = port.parent_id();
                FrameHandle {
                    frame_id: id,
                    locator_engine: Arc::clone(&self.locator_engine),
                    page_id: self.page_id,
                    parent_frame_id: parent,
                    generation: Arc::clone(&self.generation),
                    frame_port: port,
                }
            })
            .collect();
        Ok(handles)
    }
}
