use std::sync::Arc;
use std::time::Duration;

use browseros_bridge::identifiers::{FrameId, PageId};
use browseros_bridge::locator::LocatorStrategy;
use browseros_bridge::traits::{LocatorEngine, LocatorPort};
use browseros_types::identifiers::HandleId;

use crate::collection::ElementCollection;
use crate::element::ElementHandle;
use crate::error::{DomError, DomResult};
use crate::snapshot::NodeSnapshot;

pub struct LocatorBuilder {
    strategy: LocatorStrategy,
    engine: Option<Arc<dyn LocatorEngine>>,
    locator_port: Option<Arc<dyn LocatorPort>>,
    frame_id: Option<FrameId>,
}

impl std::fmt::Debug for LocatorBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocatorBuilder")
            .field("strategy", &self.strategy)
            .finish()
    }
}

impl LocatorBuilder {
    pub fn new(engine: Arc<dyn LocatorEngine>, frame_id: FrameId) -> Self {
        Self {
            strategy: LocatorStrategy::Css("*".to_string()),
            engine: Some(engine),
            locator_port: None,
            frame_id: Some(frame_id),
        }
    }

    pub fn css(selector: &str) -> Self {
        Self {
            strategy: LocatorStrategy::Css(selector.to_string()),
            engine: None,
            locator_port: None,
            frame_id: None,
        }
    }

    pub fn xpath(expr: &str) -> Self {
        Self {
            strategy: LocatorStrategy::XPath(expr.to_string()),
            engine: None,
            locator_port: None,
            frame_id: None,
        }
    }

    pub fn text(text: &str) -> Self {
        Self {
            strategy: LocatorStrategy::Text {
                text: text.to_string(),
                exact: false,
            },
            engine: None,
            locator_port: None,
            frame_id: None,
        }
    }

    pub fn exact_text(text: &str) -> Self {
        Self {
            strategy: LocatorStrategy::Text {
                text: text.to_string(),
                exact: true,
            },
            engine: None,
            locator_port: None,
            frame_id: None,
        }
    }

    pub fn role(role: &str) -> Self {
        Self {
            strategy: LocatorStrategy::Role {
                role: role.to_string(),
                name: None,
            },
            engine: None,
            locator_port: None,
            frame_id: None,
        }
    }

    pub fn label(text: &str) -> Self {
        Self {
            strategy: LocatorStrategy::Label(text.to_string()),
            engine: None,
            locator_port: None,
            frame_id: None,
        }
    }

    pub fn placeholder(text: &str) -> Self {
        Self {
            strategy: LocatorStrategy::Placeholder(text.to_string()),
            engine: None,
            locator_port: None,
            frame_id: None,
        }
    }

    pub fn test_id(id: &str) -> Self {
        Self {
            strategy: LocatorStrategy::TestId(id.to_string()),
            engine: None,
            locator_port: None,
            frame_id: None,
        }
    }

    pub fn alt_text(text: &str) -> Self {
        Self {
            strategy: LocatorStrategy::AltText(text.to_string()),
            engine: None,
            locator_port: None,
            frame_id: None,
        }
    }

    pub fn title(text: &str) -> Self {
        Self {
            strategy: LocatorStrategy::Title(text.to_string()),
            engine: None,
            locator_port: None,
            frame_id: None,
        }
    }

    pub fn ai(_description: &str) -> Self {
        Self {
            strategy: LocatorStrategy::Css("*".to_string()),
            engine: None,
            locator_port: None,
            frame_id: None,
        }
    }

    pub fn and(self, other: Self) -> Self {
        Self {
            strategy: LocatorStrategy::And(vec![self.strategy, other.strategy]),
            engine: self.engine,
            locator_port: self.locator_port,
            frame_id: self.frame_id,
        }
    }

    pub fn or(self, other: Self) -> Self {
        Self {
            strategy: LocatorStrategy::Or(vec![self.strategy, other.strategy]),
            engine: self.engine,
            locator_port: self.locator_port,
            frame_id: self.frame_id,
        }
    }

    pub fn child(self, parent: Self) -> Self {
        Self {
            strategy: LocatorStrategy::Nested(Box::new(parent.strategy), Box::new(self.strategy)),
            engine: self.engine,
            locator_port: self.locator_port,
            frame_id: self.frame_id,
        }
    }

    pub fn nth(self, _index: usize) -> Self {
        self
    }

    pub fn visible(self) -> Self {
        self
    }

    pub fn build(&self) -> LocatorStrategy {
        self.strategy.clone()
    }
}

pub struct Locator {
    pub strategy: LocatorStrategy,
    pub engine: Arc<dyn LocatorEngine>,
    pub locator_port: Arc<dyn LocatorPort>,
    pub frame_id: FrameId,
    pub timeout: Duration,
}

impl std::fmt::Debug for Locator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Locator")
            .field("strategy", &self.strategy)
            .field("frame_id", &self.frame_id)
            .field("timeout", &self.timeout)
            .finish()
    }
}

impl Locator {
    pub fn resolve(&self) -> DomResult<ElementHandle> {
        let opt = self
            .locator_port
            .locate(&self.strategy)
            .map_err(DomError::from)?;
        opt.map(|port| {
            ElementHandle::new(
                port,
                Arc::clone(&self.engine),
                HandleId::new(),
                Arc::new(std::sync::atomic::AtomicU64::new(0)),
                self.frame_id,
                PageId::new(),
            )
        })
        .ok_or_else(|| DomError::NotFound("locator did not match any element".to_string()))
    }

    pub fn resolve_all(&self) -> DomResult<ElementCollection> {
        let ports = self
            .locator_port
            .locate_all(&self.strategy)
            .map_err(DomError::from)?;
        Ok(ElementCollection::from_ports(
            ports,
            Arc::clone(&self.engine),
            Arc::new(std::sync::atomic::AtomicU64::new(0)),
            self.frame_id,
            PageId::new(),
        ))
    }

    pub fn wait(&self, timeout: Option<Duration>) -> DomResult<ElementHandle> {
        let timeout = timeout.unwrap_or(self.timeout);
        let port = self
            .locator_port
            .wait_for(&self.strategy, timeout)
            .map_err(DomError::from)?;
        Ok(ElementHandle::new(
            port,
            Arc::clone(&self.engine),
            HandleId::new(),
            Arc::new(std::sync::atomic::AtomicU64::new(0)),
            self.frame_id,
            PageId::new(),
        ))
    }

    pub fn wait_for_absence(&self, timeout: Option<Duration>) -> DomResult<()> {
        let timeout = timeout.unwrap_or(self.timeout);
        self.locator_port
            .wait_for_absence(&self.strategy, timeout)
            .map_err(DomError::from)
    }

    pub fn is_visible(&self) -> DomResult<bool> {
        let el = self.resolve()?;
        el.is_visible()
    }

    pub fn is_enabled(&self) -> DomResult<bool> {
        let el = self.resolve()?;
        el.is_enabled()
    }

    pub fn count(&self) -> DomResult<usize> {
        let all = self.resolve_all()?;
        all.count()
    }

    pub fn snapshot(&self) -> DomResult<Vec<NodeSnapshot>> {
        let all = self.resolve_all()?;
        all.snapshot()
    }

    pub fn filter(&self, additional: &LocatorStrategy) -> Self {
        let combined = LocatorStrategy::And(vec![self.strategy.clone(), additional.clone()]);
        Self {
            strategy: combined,
            engine: Arc::clone(&self.engine),
            locator_port: Arc::clone(&self.locator_port),
            frame_id: self.frame_id,
            timeout: self.timeout,
        }
    }

    pub fn first(&self) -> Self {
        self.nth(0)
    }

    pub fn last(&self) -> Self {
        self.nth(usize::MAX)
    }

    pub fn nth(&self, _index: usize) -> Self {
        Self {
            strategy: self.strategy.clone(),
            engine: Arc::clone(&self.engine),
            locator_port: Arc::clone(&self.locator_port),
            frame_id: self.frame_id,
            timeout: self.timeout,
        }
    }
}
