use std::sync::Arc;

use browseros_bridge::traits::ElementPort;

use crate::element::ElementHandle;
use crate::error::{DomError, DomResult};

pub struct Attributes {
    pub(crate) inner: Arc<dyn ElementPort>,
    pub(crate) handle: ElementHandle,
}

impl std::fmt::Debug for Attributes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Attributes").finish()
    }
}

impl Attributes {
    pub fn new(inner: Arc<dyn ElementPort>, handle: ElementHandle) -> Self {
        Self { inner, handle }
    }

    pub fn get(&self, name: &str) -> DomResult<Option<String>> {
        self.handle.check_stale()?;
        self.inner.get_attribute(name).map_err(DomError::from)
    }

    pub fn set(&self, name: &str, value: &str) -> DomResult<()> {
        self.handle.check_stale()?;
        self.inner
            .set_attribute(name, value)
            .map_err(DomError::from)
    }

    pub fn remove(&self, name: &str) -> DomResult<()> {
        self.handle.check_stale()?;
        self.inner.set_attribute(name, "").map_err(DomError::from)
    }

    pub fn has(&self, name: &str) -> DomResult<bool> {
        self.handle.check_stale()?;
        self.inner.has_attribute(name).map_err(DomError::from)
    }

    pub fn len(&self) -> DomResult<usize> {
        self.handle.check_stale()?;
        self.inner
            .attributes()
            .map_err(DomError::from)
            .map(|m| m.len())
    }

    pub fn is_empty(&self) -> DomResult<bool> {
        self.len().map(|l| l == 0)
    }

    pub fn names(&self) -> DomResult<Vec<String>> {
        self.handle.check_stale()?;
        self.inner
            .attributes()
            .map_err(DomError::from)
            .map(|m| m.into_keys().collect())
    }

    pub fn entries(&self) -> DomResult<Vec<(String, String)>> {
        self.handle.check_stale()?;
        self.inner
            .attributes()
            .map_err(DomError::from)
            .map(|m| m.into_iter().collect())
    }

    pub fn dataset(&self) -> DomResult<Dataset> {
        self.handle.check_stale()?;
        Ok(Dataset(Attributes::new(
            Arc::clone(&self.inner),
            self.handle.clone_handle(),
        )))
    }

    pub fn aria(&self) -> DomResult<AriaAttributes> {
        self.handle.check_stale()?;
        Ok(AriaAttributes::new(
            Arc::clone(&self.inner),
            self.handle.clone_handle(),
        ))
    }
}

pub struct Dataset(pub Attributes);

impl std::fmt::Debug for Dataset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Dataset").finish()
    }
}

impl Dataset {
    pub fn get(&self, key: &str) -> DomResult<Option<String>> {
        let data_key = to_data_attr(key);
        self.0.get(&data_key)
    }

    pub fn set(&self, key: &str, value: &str) -> DomResult<()> {
        let data_key = to_data_attr(key);
        self.0.set(&data_key, value)
    }

    pub fn remove(&self, key: &str) -> DomResult<()> {
        let data_key = to_data_attr(key);
        self.0.remove(&data_key)
    }

    pub fn entries(&self) -> DomResult<Vec<(String, String)>> {
        let all = self.0.entries()?;
        Ok(all
            .into_iter()
            .filter(|(k, _)| k.starts_with("data-"))
            .map(|(k, v)| (from_data_attr(&k), v))
            .collect())
    }
}

fn to_data_attr(key: &str) -> String {
    let underscored = key.replace('-', "_");
    format!("data-{}", underscored)
}

fn from_data_attr(attr: &str) -> String {
    attr.strip_prefix("data-").unwrap_or(attr).replace('_', "-")
}

pub struct AriaAttributes {
    pub(crate) inner: Arc<dyn ElementPort>,
    pub(crate) handle: ElementHandle,
}

impl std::fmt::Debug for AriaAttributes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AriaAttributes").finish()
    }
}

impl AriaAttributes {
    pub fn new(inner: Arc<dyn ElementPort>, handle: ElementHandle) -> Self {
        Self { inner, handle }
    }

    pub fn role(&self) -> DomResult<Option<String>> {
        self.get_str("role")
    }

    pub fn label(&self) -> DomResult<Option<String>> {
        self.get_str("aria-label")
    }

    pub fn describedby(&self) -> DomResult<Option<String>> {
        self.get_str("aria-describedby")
    }

    pub fn expanded(&self) -> DomResult<Option<bool>> {
        self.get_bool("aria-expanded")
    }

    pub fn hidden(&self) -> DomResult<Option<bool>> {
        self.get_bool("aria-hidden")
    }

    pub fn pressed(&self) -> DomResult<Option<bool>> {
        self.get_bool("aria-pressed")
    }

    pub fn selected(&self) -> DomResult<Option<bool>> {
        self.get_bool("aria-selected")
    }

    pub fn checked(&self) -> DomResult<Option<bool>> {
        self.get_bool("aria-checked")
    }

    fn get_str(&self, name: &str) -> DomResult<Option<String>> {
        self.handle.check_stale()?;
        self.inner.get_attribute(name).map_err(DomError::from)
    }

    fn get_bool(&self, name: &str) -> DomResult<Option<bool>> {
        self.handle.check_stale()?;
        self.inner
            .get_attribute(name)
            .map_err(DomError::from)
            .map(|opt| opt.map(|v| v == "true" || v.is_empty()))
    }
}

pub struct ClassList {
    pub(crate) inner: Arc<dyn ElementPort>,
    pub(crate) handle: ElementHandle,
}

impl std::fmt::Debug for ClassList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClassList").finish()
    }
}

impl ClassList {
    pub fn new(inner: Arc<dyn ElementPort>, handle: ElementHandle) -> Self {
        Self { inner, handle }
    }

    pub fn contains(&self, class: &str) -> DomResult<bool> {
        self.handle.check_stale()?;
        let attr = self.inner.get_attribute("class").map_err(DomError::from)?;
        Ok(attr
            .map(|c| c.split_whitespace().any(|cl| cl == class))
            .unwrap_or(false))
    }

    pub fn add(&self, class: &str) -> DomResult<()> {
        self.handle.check_stale()?;
        let existing = self
            .inner
            .get_attribute("class")
            .map_err(DomError::from)?
            .unwrap_or_default();
        let mut classes: Vec<&str> = existing.split_whitespace().collect();
        if !classes.contains(&class) {
            classes.push(class);
        }
        self.inner
            .set_attribute("class", &classes.join(" "))
            .map_err(DomError::from)
    }

    pub fn remove(&self, class: &str) -> DomResult<()> {
        self.handle.check_stale()?;
        let existing = self
            .inner
            .get_attribute("class")
            .map_err(DomError::from)?
            .unwrap_or_default();
        let classes: Vec<&str> = existing
            .split_whitespace()
            .filter(|c| *c != class)
            .collect();
        self.inner
            .set_attribute("class", &classes.join(" "))
            .map_err(DomError::from)
    }

    pub fn toggle(&self, class: &str) -> DomResult<bool> {
        self.handle.check_stale()?;
        let present = self.contains(class)?;
        if present {
            self.remove(class)?;
            Ok(false)
        } else {
            self.add(class)?;
            Ok(true)
        }
    }

    pub fn replace(&self, old: &str, new: &str) -> DomResult<()> {
        self.handle.check_stale()?;
        let existing = self
            .inner
            .get_attribute("class")
            .map_err(DomError::from)?
            .unwrap_or_default();
        let classes: Vec<&str> = existing
            .split_whitespace()
            .map(|c| if c == old { new } else { c })
            .collect();
        self.inner
            .set_attribute("class", &classes.join(" "))
            .map_err(DomError::from)
    }

    pub fn len(&self) -> DomResult<usize> {
        self.handle.check_stale()?;
        let attr = self.inner.get_attribute("class").map_err(DomError::from)?;
        Ok(attr.map(|c| c.split_whitespace().count()).unwrap_or(0))
    }

    pub fn is_empty(&self) -> DomResult<bool> {
        self.len().map(|l| l == 0)
    }

    pub fn values(&self) -> DomResult<Vec<String>> {
        self.handle.check_stale()?;
        let attr = self.inner.get_attribute("class").map_err(DomError::from)?;
        Ok(attr
            .map(|c| c.split_whitespace().map(String::from).collect())
            .unwrap_or_default())
    }
}
