use std::sync::Arc;

use browseros_bridge::error::BridgeResult;
use browseros_bridge::identifiers::PageId;
use browseros_bridge::traits::{
    BrowserPort, DialogPort, DownloadPort, FramePort, InputPort, LocatorPort, NetworkPort,
    PagePort, SessionPort, StoragePort,
};
use browseros_bridge::types::{
    BrowserInfo, JsResult, NavigationState, PdfOptions, ScreenshotOptions, SessionConfig,
};

/// A handle to a managed browser instance.
#[derive(Clone)]
pub struct BrowserHandle {
    inner: Arc<dyn BrowserPort>,
}

impl BrowserHandle {
    pub fn new(port: Arc<dyn BrowserPort>) -> Self {
        Self { inner: port }
    }

    pub fn info(&self) -> BrowserInfo {
        self.inner.info()
    }

    pub fn new_session(&self, config: SessionConfig) -> BridgeResult<SessionHandle> {
        let session = self.inner.create_session(config)?;
        Ok(SessionHandle::new(session))
    }

    pub fn sessions(&self) -> Vec<SessionHandle> {
        self.inner
            .sessions()
            .into_iter()
            .map(SessionHandle::new)
            .collect()
    }

    pub fn close(&self) -> BridgeResult<()> {
        self.inner.close()
    }
}

/// A handle to a browser session (browsing context).
#[derive(Clone)]
pub struct SessionHandle {
    inner: Arc<dyn SessionPort>,
}

impl SessionHandle {
    pub fn new(port: Box<dyn SessionPort>) -> Self {
        Self {
            inner: Arc::from(port),
        }
    }

    pub fn pages(&self) -> Vec<PageHandle> {
        self.inner
            .pages()
            .into_iter()
            .map(PageHandle::new)
            .collect()
    }

    pub fn new_page(&self) -> BridgeResult<PageHandle> {
        let page = self.inner.create_page()?;
        Ok(PageHandle::new(page))
    }

    pub fn close(&self) -> BridgeResult<()> {
        self.inner.close()
    }
}

/// A handle to a page (tab).
#[derive(Clone)]
pub struct PageHandle {
    inner: Arc<dyn PagePort>,
}

impl PageHandle {
    pub fn new(port: Box<dyn PagePort>) -> Self {
        Self {
            inner: Arc::from(port),
        }
    }

    pub fn id(&self) -> PageId {
        self.inner.id()
    }

    pub fn url(&self) -> String {
        self.inner.url()
    }

    pub fn title(&self) -> String {
        self.inner.title()
    }

    pub fn navigate(&self, url: &str) -> BridgeResult<NavigationState> {
        self.inner.navigate(url)
    }

    pub fn evaluate(&self, script: &str) -> BridgeResult<JsResult> {
        self.inner.evaluate(script, None)
    }

    pub fn screenshot(&self, options: ScreenshotOptions) -> BridgeResult<Vec<u8>> {
        self.inner.screenshot(options)
    }

    pub fn pdf(&self, options: PdfOptions) -> BridgeResult<Vec<u8>> {
        self.inner.pdf(options)
    }

    pub fn locator(&self) -> Box<dyn LocatorPort> {
        self.inner.locator()
    }

    pub fn network(&self) -> Box<dyn NetworkPort> {
        self.inner.network()
    }

    pub fn input(&self) -> Box<dyn InputPort> {
        self.inner.input()
    }

    pub fn storage(&self) -> Box<dyn StoragePort> {
        self.inner.storage()
    }

    pub fn dialog(&self) -> Box<dyn DialogPort> {
        self.inner.dialog()
    }

    pub fn download(&self) -> Box<dyn DownloadPort> {
        self.inner.download()
    }

    pub fn frames(&self) -> Vec<FrameHandle> {
        self.inner
            .frames()
            .into_iter()
            .map(FrameHandle::new)
            .collect()
    }

    pub fn close(&self) -> BridgeResult<()> {
        self.inner.close()
    }
}

/// A handle to a frame.
#[derive(Clone)]
pub struct FrameHandle {
    inner: Arc<dyn FramePort>,
}

impl FrameHandle {
    pub fn new(port: Box<dyn FramePort>) -> Self {
        Self {
            inner: Arc::from(port),
        }
    }
}

impl std::ops::Deref for BrowserHandle {
    type Target = dyn BrowserPort;
    fn deref(&self) -> &Self::Target {
        &*self.inner
    }
}

impl std::ops::Deref for SessionHandle {
    type Target = dyn SessionPort;
    fn deref(&self) -> &Self::Target {
        &*self.inner
    }
}

impl std::ops::Deref for PageHandle {
    type Target = dyn PagePort;
    fn deref(&self) -> &Self::Target {
        &*self.inner
    }
}

impl std::ops::Deref for FrameHandle {
    type Target = dyn FramePort;
    fn deref(&self) -> &Self::Target {
        &*self.inner
    }
}
