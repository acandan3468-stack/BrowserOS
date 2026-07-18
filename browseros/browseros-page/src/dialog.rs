use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use browseros_bridge::PagePort;
use browseros_event_bus::EventBus;
use browseros_types::event::Event;
use browseros_types::identifiers::SubscriptionHandle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogStrategy {
    Accept,
    Dismiss,
    AcceptWithText(&'static str),
    AutoDismiss,
}

pub struct DialogAutoHandler {
    page: Arc<dyn PagePort>,
    event_bus: EventBus,
    active: AtomicBool,
    subscription: Option<SubscriptionHandle>,
}

impl DialogAutoHandler {
    pub fn new(page: Arc<dyn PagePort>, event_bus: &EventBus) -> Self {
        DialogAutoHandler {
            page,
            event_bus: event_bus.clone(),
            active: AtomicBool::new(false),
            subscription: None,
        }
    }

    pub fn auto_handle(&mut self, strategy: DialogStrategy) -> Result<(), DialogError> {
        if self.active.load(Ordering::SeqCst) {
            return Err(DialogError::AlreadyHandling);
        }

        let handle = match strategy {
            DialogStrategy::Accept => self.event_bus.subscribe("dialog.opened", {
                let p = self.page.clone();
                Arc::new(move |_: &dyn Event| {
                    let dialog = p.dialog();
                    let _ = dialog.accept(None);
                })
            }),
            DialogStrategy::Dismiss => self.event_bus.subscribe("dialog.opened", {
                let p = self.page.clone();
                Arc::new(move |_: &dyn Event| {
                    let dialog = p.dialog();
                    let _ = dialog.dismiss();
                })
            }),
            DialogStrategy::AcceptWithText(text) => {
                let text_static: &'static str = text;
                self.event_bus.subscribe("dialog.opened", {
                    let p = self.page.clone();
                    Arc::new(move |_: &dyn Event| {
                        let dialog = p.dialog();
                        let _ = dialog.accept(Some(text_static));
                    })
                })
            }
            DialogStrategy::AutoDismiss => self.event_bus.subscribe("dialog.opened", {
                let p = self.page.clone();
                Arc::new(move |_: &dyn Event| {
                    let dialog = p.dialog();
                    let _ = dialog.dismiss();
                })
            }),
        };

        self.active.store(true, Ordering::SeqCst);
        self.subscription = Some(handle);
        Ok(())
    }

    pub fn stop_auto_handle(&mut self) -> Result<(), DialogError> {
        if !self.active.load(Ordering::SeqCst) {
            return Err(DialogError::NotHandling);
        }
        if let Some(handle) = self.subscription.take() {
            self.event_bus.unsubscribe(&handle);
        }
        self.active.store(false, Ordering::SeqCst);
        Ok(())
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }
}

impl std::fmt::Debug for DialogAutoHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DialogAutoHandler")
            .field("active", &self.active.load(Ordering::SeqCst))
            .finish()
    }
}

#[derive(Debug, Clone)]
pub enum DialogError {
    AlreadyHandling,
    NotHandling,
}

impl std::fmt::Display for DialogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DialogError::AlreadyHandling => write!(f, "already auto-handling dialogs"),
            DialogError::NotHandling => write!(f, "not currently auto-handling dialogs"),
        }
    }
}

impl std::error::Error for DialogError {}

#[cfg(test)]
mod tests {
    use super::*;
    use browseros_bridge::error::BridgeResult;
    use browseros_bridge::identifiers::NavigationId;
    use browseros_bridge::identifiers::PageId;
    use browseros_bridge::types::NavigationStatus;

    fn nav_finished(url: impl Into<String>) -> NavigationState {
        NavigationState {
            navigation_id: NavigationId::new(),
            url: url.into(),
            status: NavigationStatus::Finished,
        }
    }

    use browseros_bridge::traits::{
        DialogPort, DownloadPort, ElementPort, FramePort, InputPort, LocatorPort, NetworkPort,
        StoragePort,
    };
    use browseros_bridge::types::{
        DialogInfo, JsResult, NavigationState, PdfOptions, ScreenshotOptions, Viewport,
        WaitCondition,
    };
    use std::sync::Mutex;

    struct TestDialogPort {
        accepted: Mutex<bool>,
        dismissed: Mutex<bool>,
        prompt_text: Mutex<Option<String>>,
    }

    impl TestDialogPort {
        fn new() -> Self {
            TestDialogPort {
                accepted: Mutex::new(false),
                dismissed: Mutex::new(false),
                prompt_text: Mutex::new(None),
            }
        }
    }

    impl DialogPort for TestDialogPort {
        fn next(&self) -> BridgeResult<Option<DialogInfo>> {
            Ok(None)
        }
        fn accept(&self, prompt_text: Option<&str>) -> BridgeResult<()> {
            *self.accepted.lock().unwrap() = true;
            *self.prompt_text.lock().unwrap() = prompt_text.map(|s| s.to_owned());
            Ok(())
        }
        fn dismiss(&self) -> BridgeResult<()> {
            *self.dismissed.lock().unwrap() = true;
            Ok(())
        }
    }

    struct TestDialogPage {
        dialog_port: TestDialogPort,
        page_id: PageId,
    }

    impl TestDialogPage {
        fn new() -> Self {
            TestDialogPage {
                dialog_port: TestDialogPort::new(),
                page_id: PageId::new(),
            }
        }
    }

    impl PagePort for TestDialogPage {
        fn id(&self) -> PageId {
            self.page_id
        }
        fn url(&self) -> String {
            "https://example.com".into()
        }
        fn title(&self) -> String {
            "Test".into()
        }
        fn navigate(&self, _url: &str) -> BridgeResult<NavigationState> {
            Ok(nav_finished(_url))
        }
        fn reload(&self) -> BridgeResult<NavigationState> {
            Ok(nav_finished(self.url()))
        }
        fn go_back(&self) -> BridgeResult<NavigationState> {
            Ok(nav_finished(self.url()))
        }
        fn go_forward(&self) -> BridgeResult<NavigationState> {
            Ok(nav_finished(self.url()))
        }
        fn evaluate(
            &self,
            _script: &str,
            _arg: Option<&serde_json::Value>,
        ) -> BridgeResult<JsResult> {
            Ok(JsResult {
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
        fn screenshot(&self, _opts: ScreenshotOptions) -> BridgeResult<Vec<u8>> {
            Ok(Vec::new())
        }
        fn pdf(&self, _opts: PdfOptions) -> BridgeResult<Vec<u8>> {
            Ok(Vec::new())
        }
        fn content(&self) -> BridgeResult<String> {
            Ok("<html></html>".into())
        }
        fn set_content(&self, _html: &str) -> BridgeResult<()> {
            Ok(())
        }
        fn set_viewport(&self, _vp: Viewport) -> BridgeResult<()> {
            Ok(())
        }
        fn wait_for(&self, _cond: WaitCondition) -> BridgeResult<()> {
            Ok(())
        }
        fn frames(&self) -> Vec<Box<dyn FramePort>> {
            Vec::new()
        }
        fn main_frame(&self) -> Box<dyn FramePort> {
            panic!("no main frame")
        }
        fn close(&self) -> BridgeResult<()> {
            Ok(())
        }
        fn locator(&self) -> Box<dyn LocatorPort> {
            panic!("no locator")
        }
        fn network(&self) -> Box<dyn NetworkPort> {
            panic!("no network")
        }
        fn input(&self) -> Box<dyn InputPort> {
            panic!("no input")
        }
        fn storage(&self) -> Box<dyn StoragePort> {
            panic!("no storage")
        }
        fn dialog(&self) -> Box<dyn DialogPort> {
            Box::new(TestDialogPort::new())
        }
        fn download(&self) -> Box<dyn DownloadPort> {
            panic!("no download")
        }
    }

    fn make_page() -> Arc<dyn PagePort> {
        Arc::new(TestDialogPage::new())
    }

    #[test]
    fn test_dialog_strategy_accept() {
        let page = make_page();
        let bus = EventBus::new();
        let mut handler = DialogAutoHandler::new(page, &bus);
        assert!(!handler.is_active());
        assert!(handler.auto_handle(DialogStrategy::Accept).is_ok());
        assert!(handler.is_active());
    }

    #[test]
    fn test_dialog_strategy_dismiss() {
        let page = make_page();
        let bus = EventBus::new();
        let mut handler = DialogAutoHandler::new(page, &bus);
        assert!(handler.auto_handle(DialogStrategy::Dismiss).is_ok());
    }

    #[test]
    fn test_dialog_strategy_auto_dismiss() {
        let page = make_page();
        let bus = EventBus::new();
        let mut handler = DialogAutoHandler::new(page, &bus);
        assert!(handler.auto_handle(DialogStrategy::AutoDismiss).is_ok());
    }

    #[test]
    fn test_dialog_already_handling_error() {
        let page = make_page();
        let bus = EventBus::new();
        let mut handler = DialogAutoHandler::new(page, &bus);
        handler.auto_handle(DialogStrategy::Accept).unwrap();
        let result = handler.auto_handle(DialogStrategy::Accept);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), DialogError::AlreadyHandling));
    }

    #[test]
    fn test_dialog_stop_auto_handle() {
        let page = make_page();
        let bus = EventBus::new();
        let mut handler = DialogAutoHandler::new(page, &bus);
        handler.auto_handle(DialogStrategy::Accept).unwrap();
        assert!(handler.stop_auto_handle().is_ok());
        assert!(!handler.is_active());
    }

    #[test]
    fn test_dialog_stop_not_handling_error() {
        let page = make_page();
        let bus = EventBus::new();
        let mut handler = DialogAutoHandler::new(page, &bus);
        let result = handler.stop_auto_handle();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), DialogError::NotHandling));
    }

    #[test]
    fn test_dialog_auto_handler_debug() {
        let page = make_page();
        let bus = EventBus::new();
        let handler = DialogAutoHandler::new(page, &bus);
        let debug = format!("{:?}", handler);
        assert!(debug.contains("DialogAutoHandler"));
        assert!(debug.contains("active"));
    }

    #[test]
    fn test_dialog_error_display() {
        let err = DialogError::AlreadyHandling;
        assert_eq!(err.to_string(), "already auto-handling dialogs");
        let err = DialogError::NotHandling;
        assert_eq!(err.to_string(), "not currently auto-handling dialogs");
    }

    #[test]
    fn test_accept_with_text_strategy() {
        let page = make_page();
        let bus = EventBus::new();
        let mut handler = DialogAutoHandler::new(page, &bus);
        let result = handler.auto_handle(DialogStrategy::AcceptWithText("response"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_dialog_strategy_clone_copy_eq() {
        assert_eq!(DialogStrategy::Accept, DialogStrategy::Accept);
        assert_eq!(DialogStrategy::Dismiss, DialogStrategy::Dismiss);
        assert_ne!(DialogStrategy::Accept, DialogStrategy::Dismiss);
    }
}
