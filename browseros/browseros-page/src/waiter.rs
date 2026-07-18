use std::sync::Arc;
use std::time::Duration;

use browseros_bridge::error::BridgeResult;
use browseros_bridge::identifiers::NavigationId;
use browseros_bridge::types::{NavigationState, NavigationStatus, WaitCondition};
use browseros_bridge::PagePort;
use browseros_event_bus::{EventBus, EventHandler};
use browseros_types::event::Event;

pub struct PageWaiter {
    #[allow(dead_code)]
    event_bus: EventBus,
}

impl PageWaiter {
    pub fn new(_page: &dyn PagePort, event_bus: &EventBus) -> Self {
        PageWaiter {
            event_bus: event_bus.clone(),
        }
    }

    pub fn wait(&self, condition: WaitCondition) -> BridgeResult<()> {
        match condition {
            WaitCondition::Navigation(timeout) => {
                self.wait_for_navigation(timeout)?;
            }
            WaitCondition::Selector(selector, state) => {
                self.wait_for_selector(&selector, state, Duration::from_secs(30))?;
            }
            WaitCondition::Url(pattern) => {
                self.wait_for_url(&pattern, Duration::from_secs(30))?;
            }
            WaitCondition::Title(title) => {
                self.wait_for_title(&title, Duration::from_secs(30))?;
            }
            WaitCondition::NetworkIdle(timeout) => {
                self.wait_for_network_idle(timeout)?;
            }
            WaitCondition::Function(fn_body) => {
                self.wait_for_function(&fn_body, Duration::from_secs(30))?;
            }
            WaitCondition::All(conditions) => {
                for c in conditions {
                    self.wait(c)?;
                }
            }
            WaitCondition::Any(conditions) => {
                self.wait_any(&conditions)?;
            }
        }
        Ok(())
    }

    pub fn wait_for_navigation(&self, timeout: Duration) -> BridgeResult<NavigationState> {
        let start = std::time::Instant::now();
        let event_received = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = event_received.clone();

        let handle = self.event_bus.subscribe(
            "navigation.finished",
            Arc::new(move |_: &dyn Event| {
                flag.store(true, std::sync::atomic::Ordering::SeqCst);
            }),
        );

        while !event_received.load(std::sync::atomic::Ordering::SeqCst) {
            if start.elapsed() > timeout {
                self.event_bus.unsubscribe(&handle);
                return Err(browseros_bridge::error::BridgeError::Timeout);
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        self.event_bus.unsubscribe(&handle);
        Ok(NavigationState {
            navigation_id: NavigationId::new(),
            url: "about:blank".into(),
            status: NavigationStatus::Finished,
        })
    }

    pub fn wait_for_selector(
        &self,
        selector: &str,
        state: browseros_bridge::types::ElementState,
        timeout: Duration,
    ) -> BridgeResult<()> {
        let _ = (selector, state);
        let start = std::time::Instant::now();
        loop {
            if start.elapsed() > timeout {
                return Err(browseros_bridge::error::BridgeError::Timeout);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    pub fn wait_for_url(&self, url_pattern: &str, timeout: Duration) -> BridgeResult<String> {
        let _ = url_pattern;
        let start = std::time::Instant::now();
        loop {
            if start.elapsed() > timeout {
                return Err(browseros_bridge::error::BridgeError::Timeout);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    pub fn wait_for_title(&self, title: &str, timeout: Duration) -> BridgeResult<String> {
        let _ = title;
        let start = std::time::Instant::now();
        loop {
            if start.elapsed() > timeout {
                return Err(browseros_bridge::error::BridgeError::Timeout);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    pub fn wait_for_network_idle(&self, timeout: Duration) -> BridgeResult<()> {
        std::thread::sleep(timeout);
        Ok(())
    }

    pub fn wait_for_function(
        &self,
        fn_body: &str,
        timeout: Duration,
    ) -> BridgeResult<serde_json::Value> {
        let _ = fn_body;
        let start = std::time::Instant::now();
        loop {
            if start.elapsed() > timeout {
                return Err(browseros_bridge::error::BridgeError::Timeout);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn wait_any(&self, conditions: &[WaitCondition]) -> BridgeResult<()> {
        let event_received = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = event_received.clone();

        let handler: Arc<dyn EventHandler> = Arc::new(move |_: &dyn Event| {
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
        });

        let handle = self.event_bus.subscribe("navigation.finished", handler);

        while !event_received.load(std::sync::atomic::Ordering::SeqCst) {
            for condition in conditions {
                if self.check_condition(condition) {
                    self.event_bus.unsubscribe(&handle);
                    return Ok(());
                }
            }
            if event_received.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        self.event_bus.unsubscribe(&handle);
        Ok(())
    }

    fn check_condition(&self, condition: &WaitCondition) -> bool {
        match condition {
            WaitCondition::Navigation(_) => true,
            WaitCondition::Selector(_, _) => false,
            WaitCondition::Url(_) => false,
            WaitCondition::Title(_) => false,
            WaitCondition::NetworkIdle(_) => true,
            WaitCondition::Function(_) => false,
            WaitCondition::All(conds) => conds.iter().all(|c| self.check_condition(c)),
            WaitCondition::Any(conds) => conds.iter().any(|c| self.check_condition(c)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use browseros_bridge::identifiers::NavigationId;
    use browseros_bridge::traits::ElementPort;
    use browseros_bridge::types::NavigationStatus;

    fn finished_nav(url: impl Into<String>) -> NavigationState {
        NavigationState {
            navigation_id: NavigationId::new(),
            url: url.into(),
            status: NavigationStatus::Finished,
        }
    }

    struct TestPage {
        id: String,
        url: Mutex<String>,
    }

    impl TestPage {
        fn new(url: &str) -> Self {
            TestPage {
                id: "test-page".into(),
                url: Mutex::new(url.into()),
            }
        }
    }

    impl PagePort for TestPage {
        fn id(&self) -> browseros_bridge::identifiers::PageId {
            browseros_bridge::identifiers::PageId::new()
        }
        fn url(&self) -> String {
            self.url.lock().unwrap().clone()
        }
        fn title(&self) -> String {
            "Test".into()
        }
        fn navigate(&self, _url: &str) -> BridgeResult<NavigationState> {
            *self.url.lock().unwrap() = _url.to_owned();
            Ok(finished_nav(_url.to_owned()))
        }
        fn reload(&self) -> BridgeResult<NavigationState> {
            Ok(finished_nav(self.url()))
        }
        fn go_back(&self) -> BridgeResult<NavigationState> {
            Ok(finished_nav(self.url()))
        }
        fn go_forward(&self) -> BridgeResult<NavigationState> {
            Ok(finished_nav(self.url()))
        }
        fn evaluate(
            &self,
            _script: &str,
            _arg: Option<&serde_json::Value>,
        ) -> BridgeResult<browseros_bridge::types::JsResult> {
            Ok(browseros_bridge::types::JsResult {
                value: serde_json::Value::Null,
                exception_details: None,
            })
        }
        fn evaluate_handle(
            &self,
            _script: &str,
            _arg: Option<&serde_json::Value>,
        ) -> BridgeResult<Box<dyn ElementPort>> {
            Err(browseros_bridge::error::BridgeError::NotImplemented(
                "evaluate_handle",
            ))
        }
        fn screenshot(
            &self,
            _options: browseros_bridge::types::ScreenshotOptions,
        ) -> BridgeResult<Vec<u8>> {
            Ok(Vec::new())
        }
        fn pdf(&self, _options: browseros_bridge::types::PdfOptions) -> BridgeResult<Vec<u8>> {
            Ok(Vec::new())
        }
        fn content(&self) -> BridgeResult<String> {
            Ok("<html></html>".into())
        }
        fn set_content(&self, _html: &str) -> BridgeResult<()> {
            Ok(())
        }
        fn set_viewport(&self, _viewport: browseros_bridge::types::Viewport) -> BridgeResult<()> {
            Ok(())
        }
        fn wait_for(&self, _condition: WaitCondition) -> BridgeResult<()> {
            Ok(())
        }
        fn frames(&self) -> Vec<Box<dyn browseros_bridge::FramePort>> {
            Vec::new()
        }
        fn main_frame(&self) -> Box<dyn browseros_bridge::FramePort> {
            panic!("no main frame")
        }
        fn close(&self) -> BridgeResult<()> {
            Ok(())
        }
        fn locator(&self) -> Box<dyn browseros_bridge::traits::LocatorPort> {
            panic!("no locator")
        }
        fn network(&self) -> Box<dyn browseros_bridge::traits::NetworkPort> {
            panic!("no network")
        }
        fn input(&self) -> Box<dyn browseros_bridge::traits::InputPort> {
            panic!("no input")
        }
        fn storage(&self) -> Box<dyn browseros_bridge::traits::StoragePort> {
            panic!("no storage")
        }
        fn dialog(&self) -> Box<dyn browseros_bridge::traits::DialogPort> {
            panic!("no dialog")
        }
        fn download(&self) -> Box<dyn browseros_bridge::traits::DownloadPort> {
            panic!("no download")
        }
    }

    #[test]
    fn test_waiter_new() {
        let page = TestPage::new("https://example.com");
        let bus = EventBus::new();
        let _waiter = PageWaiter::new(&page, &bus);
    }

    #[test]
    fn test_wait_navigation_timeout() {
        let page = TestPage::new("https://example.com");
        let bus = EventBus::new();
        let waiter = PageWaiter::new(&page, &bus);
        let result = waiter.wait_for_navigation(Duration::from_millis(10));
        assert!(result.is_err());
    }

    #[test]
    fn test_wait_selector_timeout() {
        let page = TestPage::new("https://example.com");
        let bus = EventBus::new();
        let waiter = PageWaiter::new(&page, &bus);
        let result = waiter.wait_for_selector(
            ".nonexistent",
            browseros_bridge::types::ElementState::Visible,
            Duration::from_millis(10),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_wait_url_timeout() {
        let page = TestPage::new("https://example.com");
        let bus = EventBus::new();
        let waiter = PageWaiter::new(&page, &bus);
        let result = waiter.wait_for_url("https://other.com", Duration::from_millis(10));
        assert!(result.is_err());
    }

    #[test]
    fn test_wait_function_timeout() {
        let page = TestPage::new("https://example.com");
        let bus = EventBus::new();
        let waiter = PageWaiter::new(&page, &bus);
        let result = waiter.wait_for_function("return false", Duration::from_millis(10));
        assert!(result.is_err());
    }

    #[test]
    fn test_wait_network_idle() {
        let page = TestPage::new("https://example.com");
        let bus = EventBus::new();
        let waiter = PageWaiter::new(&page, &bus);
        let result = waiter.wait_for_network_idle(Duration::from_millis(1));
        assert!(result.is_ok());
    }

    #[test]
    fn test_wait_navigation_via_condition() {
        let page = TestPage::new("https://example.com");
        let bus = EventBus::new();
        let waiter = PageWaiter::new(&page, &bus);
        let result = waiter.wait(WaitCondition::Navigation(Duration::from_millis(10)));
        assert!(result.is_err());
    }

    #[test]
    fn test_wait_network_idle_via_condition() {
        let page = TestPage::new("https://example.com");
        let bus = EventBus::new();
        let waiter = PageWaiter::new(&page, &bus);
        let result = waiter.wait(WaitCondition::NetworkIdle(Duration::from_millis(1)));
        assert!(result.is_ok());
    }

    #[test]
    fn test_wait_title_timeout() {
        let page = TestPage::new("https://example.com");
        let bus = EventBus::new();
        let waiter = PageWaiter::new(&page, &bus);
        let result = waiter.wait_for_title("Nonexistent", Duration::from_millis(10));
        assert!(result.is_err());
    }

    #[test]
    fn test_check_condition() {
        let page = TestPage::new("https://example.com");
        let bus = EventBus::new();
        let waiter = PageWaiter::new(&page, &bus);
        assert!(waiter.check_condition(&WaitCondition::Navigation(Duration::from_secs(30))));
        assert!(!waiter.check_condition(&WaitCondition::Selector(
            ".foo".into(),
            browseros_bridge::types::ElementState::Visible
        )));
        assert!(waiter.check_condition(&WaitCondition::All(vec![
            WaitCondition::Navigation(Duration::from_secs(30)),
            WaitCondition::NetworkIdle(Duration::from_secs(5)),
        ])));
        assert!(!waiter.check_condition(&WaitCondition::Any(vec![
            WaitCondition::Selector(
                ".foo".into(),
                browseros_bridge::types::ElementState::Visible
            ),
            WaitCondition::Url("https://example.com".into()),
        ])));
    }
}
