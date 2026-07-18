use std::collections::HashMap;
use std::sync::Arc;

use browseros_bridge::traits::ElementPort;

use crate::element::ElementHandle;
use crate::error::{DomError, DomResult};

pub struct StyleDeclaration {
    pub(crate) inner: Arc<dyn ElementPort>,
    pub(crate) handle: ElementHandle,
}

impl std::fmt::Debug for StyleDeclaration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StyleDeclaration").finish()
    }
}

impl StyleDeclaration {
    pub fn new(inner: Arc<dyn ElementPort>, handle: ElementHandle) -> Self {
        Self { inner, handle }
    }

    pub fn get(&self, property: &str) -> DomResult<Option<String>> {
        self.handle.check_stale()?;
        let frame = self.inner.owning_frame();
        let id = self.inner.id().get();
        let prop = property.replace('\'', "\\'");
        let js = format!(
            r#"document.querySelector('[cdp_node_id="{id}"]').style.getPropertyValue('{prop}')"#
        );
        let result = frame.evaluate(&js, None).map_err(DomError::from)?;
        let val = result
            .value
            .as_str()
            .map(|s| s.to_string())
            .unwrap_or_default();
        if val.is_empty() {
            Ok(None)
        } else {
            Ok(Some(val))
        }
    }

    pub fn set(&self, property: &str, value: &str) -> DomResult<()> {
        self.handle.check_stale()?;
        let frame = self.inner.owning_frame();
        let id = self.inner.id().get();
        let prop = property.replace('\'', "\\'");
        let val = value.replace('\'', "\\'");
        let js = format!(
            r#"document.querySelector('[cdp_node_id="{id}"]').style.setProperty('{prop}', '{val}')"#
        );
        frame.evaluate(&js, None).map_err(DomError::from)?;
        Ok(())
    }

    pub fn remove(&self, property: &str) -> DomResult<()> {
        self.handle.check_stale()?;
        let frame = self.inner.owning_frame();
        let id = self.inner.id().get();
        let prop = property.replace('\'', "\\'");
        let js = format!(
            r#"document.querySelector('[cdp_node_id="{id}"]').style.removeProperty('{prop}')"#
        );
        frame.evaluate(&js, None).map_err(DomError::from)?;
        Ok(())
    }

    pub fn css_text(&self) -> DomResult<String> {
        self.handle.check_stale()?;
        let frame = self.inner.owning_frame();
        let id = self.inner.id().get();
        let js = format!(
            r#"document.querySelector('[cdp_node_id="{id}"]')?.getAttribute('style') || ''"#
        );
        let result = frame.evaluate(&js, None).map_err(DomError::from)?;
        Ok(result
            .value
            .as_str()
            .map(|s| s.to_string())
            .unwrap_or_default())
    }

    pub fn set_css_text(&self, css: &str) -> DomResult<()> {
        self.handle.check_stale()?;
        let frame = self.inner.owning_frame();
        let id = self.inner.id().get();
        let escaped = css.replace('\'', "\\'");
        let js = format!(
            r#"document.querySelector('[cdp_node_id="{id}"]').setAttribute('style', '{escaped}')"#
        );
        frame.evaluate(&js, None).map_err(DomError::from)?;
        Ok(())
    }

    pub fn len(&self) -> DomResult<usize> {
        let text = self.css_text()?;
        Ok(text.split(';').filter(|s| !s.trim().is_empty()).count())
    }

    pub fn is_empty(&self) -> DomResult<bool> {
        self.len().map(|l| l == 0)
    }

    pub fn entries(&self) -> DomResult<Vec<(String, String)>> {
        let text = self.css_text()?;
        Ok(text
            .split(';')
            .filter_map(|pair| {
                let pair = pair.trim();
                if pair.is_empty() {
                    return None;
                }
                let mut parts = pair.splitn(2, ':');
                let key = parts.next()?.trim().to_string();
                let val = parts.next()?.trim().to_string();
                Some((key, val))
            })
            .collect())
    }
}

pub struct ComputedStyle {
    pub(crate) inner: Arc<dyn ElementPort>,
    pub(crate) handle: ElementHandle,
    pub(crate) pseudo_element: Option<String>,
}

impl std::fmt::Debug for ComputedStyle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ComputedStyle")
            .field("pseudo_element", &self.pseudo_element)
            .finish()
    }
}

impl ComputedStyle {
    pub fn new(
        inner: Arc<dyn ElementPort>,
        handle: ElementHandle,
        pseudo_element: Option<String>,
    ) -> Self {
        Self {
            inner,
            handle,
            pseudo_element,
        }
    }

    pub fn get(&self, property: &str) -> DomResult<Option<String>> {
        self.handle.check_stale()?;
        let frame = self.inner.owning_frame();
        let id = self.inner.id().get();
        let prop = property.replace('\'', "\\'");
        let pseudo = self
            .pseudo_element
            .as_ref()
            .map(|p| format!(", '{p}'"))
            .unwrap_or_default();
        let js = format!(
            r#"window.getComputedStyle(document.querySelector('[cdp_node_id="{id}"]'){pseudo}).getPropertyValue('{prop}')"#
        );
        let result = frame.evaluate(&js, None).map_err(DomError::from)?;
        let val = result
            .value
            .as_str()
            .map(|s| s.to_string())
            .unwrap_or_default();
        if val.is_empty() {
            Ok(None)
        } else {
            Ok(Some(val))
        }
    }

    pub fn all(&self) -> DomResult<HashMap<String, String>> {
        self.handle.check_stale()?;
        let frame = self.inner.owning_frame();
        let id = self.inner.id().get();
        let pseudo = self
            .pseudo_element
            .as_ref()
            .map(|p| format!(", '{p}'"))
            .unwrap_or_default();
        let js = format!(
            r#"(function() {{ const cs = window.getComputedStyle(document.querySelector('[cdp_node_id="{id}"]'){pseudo}); const m = {{}}; for (let i = 0; i < cs.length; i++) {{ const prop = cs[i]; m[prop] = cs.getPropertyValue(prop); }} return m; }})()"#
        );
        let result = frame.evaluate(&js, None).map_err(DomError::from)?;
        Ok(result
            .value
            .as_object()
            .map(|obj| {
                obj.iter()
                    .map(|(k, v)| {
                        (
                            k.clone(),
                            v.as_str().map(|s| s.to_string()).unwrap_or_default(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default())
    }
}
