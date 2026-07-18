use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
// use std::sync::Arc;
use std::time::Duration;

use browseros_bridge::*;

// ====================================================================
// Mock implementations for trait object safety verification
// ====================================================================

struct MockBrowser {
    closed: AtomicBool,
}

impl MockBrowser {
    fn new() -> Self {
        Self {
            closed: AtomicBool::new(false),
        }
    }
}

impl BrowserPort for MockBrowser {
    fn info(&self) -> BrowserInfo {
        BrowserInfo {
            executable: PathBuf::from("/usr/bin/chromium"),
            version: "120.0.0".into(),
            user_data_dir: PathBuf::from("/tmp/browseros-test"),
        }
    }
    fn launch(&self, _options: LaunchOptions) -> BridgeResult<()> {
        Ok(())
    }
    fn create_session(&self, _config: SessionConfig) -> BridgeResult<Box<dyn SessionPort>> {
        Ok(Box::new(MockSession::new()))
    }
    fn sessions(&self) -> Vec<Box<dyn SessionPort>> {
        vec![]
    }
    fn close(&self) -> BridgeResult<()> {
        self.closed.store(true, Ordering::SeqCst);
        Ok(())
    }
    fn kill(&self) -> BridgeResult<()> {
        self.closed.store(true, Ordering::SeqCst);
        Ok(())
    }
    fn is_alive(&self) -> bool {
        !self.closed.load(Ordering::SeqCst)
    }
}

struct MockSession {
    config: SessionConfig,
}

impl MockSession {
    fn new() -> Self {
        Self {
            config: SessionConfig::default(),
        }
    }
}

impl SessionPort for MockSession {
    fn pages(&self) -> Vec<Box<dyn PagePort>> {
        vec![]
    }
    fn create_page(&self) -> BridgeResult<Box<dyn PagePort>> {
        Ok(Box::new(MockPage))
    }
    fn close_page(&self, _page_id: &PageId) -> BridgeResult<()> {
        Ok(())
    }
    fn activate_page(&self, _page_id: &PageId) -> BridgeResult<()> {
        Ok(())
    }
    fn close(&self) -> BridgeResult<()> {
        Ok(())
    }
    fn config(&self) -> &SessionConfig {
        &self.config
    }
}

struct MockPage;

impl PagePort for MockPage {
    fn id(&self) -> PageId {
        PageId::new()
    }
    fn url(&self) -> String {
        "about:blank".into()
    }
    fn title(&self) -> String {
        String::new()
    }
    fn navigate(&self, url: &str) -> BridgeResult<NavigationState> {
        Ok(NavigationState {
            navigation_id: NavigationId::new(),
            url: url.into(),
            status: NavigationStatus::Finished,
        })
    }
    fn reload(&self) -> BridgeResult<NavigationState> {
        Ok(NavigationState {
            navigation_id: NavigationId::new(),
            url: "about:blank".into(),
            status: NavigationStatus::Finished,
        })
    }
    fn go_back(&self) -> BridgeResult<NavigationState> {
        Ok(NavigationState {
            navigation_id: NavigationId::new(),
            url: "about:blank".into(),
            status: NavigationStatus::Finished,
        })
    }
    fn go_forward(&self) -> BridgeResult<NavigationState> {
        Ok(NavigationState {
            navigation_id: NavigationId::new(),
            url: "about:blank".into(),
            status: NavigationStatus::Finished,
        })
    }
    fn evaluate(&self, _script: &str, _arg: Option<&serde_json::Value>) -> BridgeResult<JsResult> {
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
        Ok(Box::new(MockElement))
    }
    fn screenshot(&self, _options: ScreenshotOptions) -> BridgeResult<Vec<u8>> {
        Ok(vec![0u8; 100])
    }
    fn pdf(&self, _options: PdfOptions) -> BridgeResult<Vec<u8>> {
        Ok(vec![0u8; 200])
    }
    fn content(&self) -> BridgeResult<String> {
        Ok("<html></html>".into())
    }
    fn set_content(&self, _html: &str) -> BridgeResult<()> {
        Ok(())
    }
    fn set_viewport(&self, _viewport: Viewport) -> BridgeResult<()> {
        Ok(())
    }
    fn wait_for(&self, _condition: WaitCondition) -> BridgeResult<()> {
        Ok(())
    }
    fn frames(&self) -> Vec<Box<dyn FramePort>> {
        vec![]
    }
    fn main_frame(&self) -> Box<dyn FramePort> {
        Box::new(MockFrame)
    }
    fn close(&self) -> BridgeResult<()> {
        Ok(())
    }
    fn locator(&self) -> Box<dyn LocatorPort> {
        Box::new(MockLocator)
    }
    fn network(&self) -> Box<dyn NetworkPort> {
        Box::new(MockNetwork)
    }
    fn input(&self) -> Box<dyn InputPort> {
        Box::new(MockInput)
    }
    fn storage(&self) -> Box<dyn StoragePort> {
        Box::new(MockStorage)
    }
    fn dialog(&self) -> Box<dyn DialogPort> {
        Box::new(MockDialog)
    }
    fn download(&self) -> Box<dyn DownloadPort> {
        Box::new(MockDownload)
    }
}

struct MockFrame;

impl FramePort for MockFrame {
    fn id(&self) -> FrameId {
        FrameId::new()
    }
    fn url(&self) -> String {
        "about:blank".into()
    }
    fn title(&self) -> String {
        String::new()
    }
    fn parent_id(&self) -> Option<FrameId> {
        None
    }
    fn content(&self) -> BridgeResult<String> {
        Ok("<html></html>".into())
    }
    fn set_content(&self, _html: &str) -> BridgeResult<()> {
        Ok(())
    }
    fn evaluate(&self, _script: &str, _arg: Option<&serde_json::Value>) -> BridgeResult<JsResult> {
        Ok(JsResult {
            value: serde_json::Value::Null,
            exception_details: None,
        })
    }
    fn child_frames(&self) -> Vec<Box<dyn FramePort>> {
        vec![]
    }
    fn page(&self) -> Box<dyn PagePort> {
        Box::new(MockPage)
    }
}

struct MockElement;

impl ElementPort for MockElement {
    fn id(&self) -> ElementId {
        ElementId::new(42)
    }
    fn tag_name(&self) -> String {
        "div".into()
    }
    fn text_content(&self) -> BridgeResult<String> {
        Ok("hello".into())
    }
    fn inner_html(&self) -> BridgeResult<String> {
        Ok("<span>hello</span>".into())
    }
    fn outer_html(&self) -> BridgeResult<String> {
        Ok("<div><span>hello</span></div>".into())
    }
    fn get_attribute(&self, name: &str) -> BridgeResult<Option<String>> {
        match name {
            "id" => Ok(Some("test".into())),
            _ => Ok(None),
        }
    }
    fn set_attribute(&self, _name: &str, _value: &str) -> BridgeResult<()> {
        Ok(())
    }
    fn has_attribute(&self, name: &str) -> BridgeResult<bool> {
        Ok(name == "id")
    }
    fn attributes(&self) -> BridgeResult<HashMap<String, String>> {
        let mut m = HashMap::new();
        m.insert("id".into(), "test".into());
        Ok(m)
    }
    fn bounding_box(&self) -> BridgeResult<Option<BoxModel>> {
        Ok(Some(BoxModel {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 50.0,
            padding: BoxEdges::default(),
            margin: BoxEdges::default(),
            border: BoxEdges::default(),
        }))
    }
    fn is_visible(&self) -> BridgeResult<bool> {
        Ok(true)
    }
    fn is_enabled(&self) -> BridgeResult<bool> {
        Ok(true)
    }
    fn is_checked(&self) -> BridgeResult<bool> {
        Ok(false)
    }
    fn is_selected(&self) -> BridgeResult<bool> {
        Ok(false)
    }
    fn is_stable(&self) -> BridgeResult<bool> {
        Ok(true)
    }
    fn scroll_into_view(&self) -> BridgeResult<()> {
        Ok(())
    }
    fn click_point(&self) -> BridgeResult<Point> {
        Ok(Point { x: 50.0, y: 25.0 })
    }
    fn query_selector(&self, _selector: &str) -> BridgeResult<Option<Box<dyn ElementPort>>> {
        Ok(None)
    }
    fn query_selector_all(&self, _selector: &str) -> BridgeResult<Vec<Box<dyn ElementPort>>> {
        Ok(vec![])
    }
    fn focus(&self) -> BridgeResult<()> {
        Ok(())
    }
    fn hover(&self) -> BridgeResult<()> {
        Ok(())
    }
    fn snapshot(&self) -> BridgeResult<NodeInfo> {
        Ok(NodeInfo {
            node_id: NodeId::new(42),
            node_type: NodeType::Element,
            tag_name: "div".into(),
            attributes: HashMap::new(),
            text: String::new(),
            children: vec![],
            frame_id: None,
        })
    }
    fn owning_frame(&self) -> Box<dyn FramePort> {
        Box::new(MockFrame)
    }
}

struct MockLocator;

impl LocatorPort for MockLocator {
    fn locate(&self, _strategy: &LocatorStrategy) -> BridgeResult<Option<Box<dyn ElementPort>>> {
        Ok(Some(Box::new(MockElement)))
    }
    fn locate_all(&self, _strategy: &LocatorStrategy) -> BridgeResult<Vec<Box<dyn ElementPort>>> {
        Ok(vec![Box::new(MockElement)])
    }
    fn wait_for(
        &self,
        _strategy: &LocatorStrategy,
        _timeout: Duration,
    ) -> BridgeResult<Box<dyn ElementPort>> {
        Ok(Box::new(MockElement))
    }
    fn wait_for_absence(
        &self,
        _strategy: &LocatorStrategy,
        _timeout: Duration,
    ) -> BridgeResult<()> {
        Ok(())
    }
}

impl LocatorEngine for MockLocator {
    fn query_selector(&self, _selector: &str) -> BridgeResult<Option<Box<dyn ElementPort>>> {
        Ok(Some(Box::new(MockElement)))
    }
    fn query_selector_all(&self, _selector: &str) -> BridgeResult<Vec<Box<dyn ElementPort>>> {
        Ok(vec![Box::new(MockElement)])
    }
    fn query_by_text(&self, _text: &str, _exact: bool) -> BridgeResult<Vec<Box<dyn ElementPort>>> {
        Ok(vec![Box::new(MockElement)])
    }
    fn query_by_xpath(&self, _expression: &str) -> BridgeResult<Vec<Box<dyn ElementPort>>> {
        Ok(vec![Box::new(MockElement)])
    }
}

struct MockNetwork;

impl NetworkPort for MockNetwork {
    fn set_offline(&self, _offline: bool) -> BridgeResult<()> {
        Ok(())
    }
    fn set_conditions(&self, _conditions: NetworkConditions) -> BridgeResult<()> {
        Ok(())
    }
    fn add_interception_rule(&self, _rule: InterceptionRule) -> BridgeResult<InterceptionHandle> {
        Ok(InterceptionHandle::new())
    }
    fn remove_interception_rule(&self, _handle: &InterceptionHandle) -> BridgeResult<()> {
        Ok(())
    }
    fn clear_cache(&self) -> BridgeResult<()> {
        Ok(())
    }
    fn clear_cookies(&self) -> BridgeResult<()> {
        Ok(())
    }
}

struct MockInput;

impl InputPort for MockInput {
    fn click(&self, _element: &dyn ElementPort, _options: ClickOptions) -> BridgeResult<()> {
        Ok(())
    }
    fn dblclick(&self, _element: &dyn ElementPort, _options: ClickOptions) -> BridgeResult<()> {
        Ok(())
    }
    fn fill(&self, _element: &dyn ElementPort, _text: &str) -> BridgeResult<()> {
        Ok(())
    }
    fn type_text(
        &self,
        _element: &dyn ElementPort,
        _text: &str,
        _delay: Duration,
    ) -> BridgeResult<()> {
        Ok(())
    }
    fn press_key(&self, _key: &str) -> BridgeResult<()> {
        Ok(())
    }
    fn hover(&self, _element: &dyn ElementPort) -> BridgeResult<()> {
        Ok(())
    }
    fn scroll(&self, _delta_x: f64, _delta_y: f64) -> BridgeResult<()> {
        Ok(())
    }
    fn drag_and_drop(
        &self,
        _source: &dyn ElementPort,
        _target: &dyn ElementPort,
    ) -> BridgeResult<()> {
        Ok(())
    }
    fn upload_file(&self, _element: &dyn ElementPort, _paths: &[PathBuf]) -> BridgeResult<()> {
        Ok(())
    }
    fn select_option(&self, _element: &dyn ElementPort, _values: &[&str]) -> BridgeResult<()> {
        Ok(())
    }
    fn check(&self, _element: &dyn ElementPort) -> BridgeResult<()> {
        Ok(())
    }
    fn uncheck(&self, _element: &dyn ElementPort) -> BridgeResult<()> {
        Ok(())
    }
}

struct MockStorage;

impl StoragePort for MockStorage {
    fn cookies(&self) -> BridgeResult<Vec<Cookie>> {
        Ok(vec![])
    }
    fn set_cookies(&self, _cookies: &[Cookie]) -> BridgeResult<()> {
        Ok(())
    }
    fn delete_cookie(&self, _name: &str, _url: &str) -> BridgeResult<()> {
        Ok(())
    }
    fn delete_all_cookies(&self) -> BridgeResult<()> {
        Ok(())
    }
    fn local_storage(&self) -> BridgeResult<Vec<StorageEntry>> {
        Ok(vec![])
    }
    fn set_local_storage(&self, _entries: &[StorageEntry]) -> BridgeResult<()> {
        Ok(())
    }
    fn clear_local_storage(&self) -> BridgeResult<()> {
        Ok(())
    }
    fn session_storage(&self) -> BridgeResult<Vec<StorageEntry>> {
        Ok(vec![])
    }
    fn clear_session_storage(&self) -> BridgeResult<()> {
        Ok(())
    }
}

struct MockDialog;

impl DialogPort for MockDialog {
    fn next(&self) -> BridgeResult<Option<DialogInfo>> {
        Ok(None)
    }
    fn accept(&self, _prompt_text: Option<&str>) -> BridgeResult<()> {
        Ok(())
    }
    fn dismiss(&self) -> BridgeResult<()> {
        Ok(())
    }
}

struct MockDownload;

impl DownloadPort for MockDownload {
    fn downloads(&self) -> Vec<DownloadInfo> {
        vec![]
    }
    fn cancel_download(&self, _id: &str) -> BridgeResult<()> {
        Ok(())
    }
    fn set_download_path(&self, _path: PathBuf) -> BridgeResult<()> {
        Ok(())
    }
    fn download_path(&self) -> PathBuf {
        PathBuf::from("/tmp/downloads")
    }
    fn wait_for_completion(&self, _timeout: Duration) -> BridgeResult<Vec<DownloadInfo>> {
        Ok(vec![])
    }
}

struct MockArtifact;

impl ArtifactPort for MockArtifact {
    fn store(
        &self,
        _name: &str,
        _data: Vec<u8>,
        _artifact_type: ArtifactType,
    ) -> BridgeResult<ArtifactId> {
        Ok(ArtifactId::new())
    }
    fn retrieve(&self, _id: &ArtifactId) -> BridgeResult<Vec<u8>> {
        Ok(vec![])
    }
    fn list(&self, _filter: Option<ArtifactFilter>) -> BridgeResult<Vec<ArtifactMeta>> {
        Ok(vec![])
    }
    fn delete(&self, _id: &ArtifactId) -> BridgeResult<()> {
        Ok(())
    }
    fn storage_path(&self) -> PathBuf {
        PathBuf::from("/tmp/artifacts")
    }
}

// ====================================================================
// Tests — Identifiers
// ====================================================================

#[test]
fn test_page_id_creation() {
    let id1 = PageId::new();
    let id2 = PageId::new();
    assert_ne!(id1, id2);
}

#[test]
fn test_page_id_display_and_parse() {
    let id = PageId::new();
    let s = id.to_string();
    let parsed: PageId = s.parse().unwrap();
    assert_eq!(id, parsed);
}

#[test]
fn test_frame_id_creation() {
    let id = FrameId::new();
    assert_ne!(id.to_string(), "");
}

#[test]
fn test_element_id_value() {
    let id = ElementId::new(42);
    assert_eq!(id.get(), 42);
    assert_eq!(id.to_string(), "42");
}

#[test]
fn test_node_id_value() {
    let id = NodeId::new(100);
    assert_eq!(id.get(), 100);
}

#[test]
fn test_session_id_default() {
    let id = SessionId::default();
    assert_ne!(id.to_string(), "");
}

#[test]
fn test_browser_id_unique() {
    let a = BrowserId::new();
    let b = BrowserId::new();
    assert_ne!(a, b);
}

#[test]
fn test_artifact_id_roundtrip() {
    let original = ArtifactId::new();
    let s = original.to_string();
    let parsed: ArtifactId = s.parse().unwrap();
    assert_eq!(original, parsed);
}

#[test]
fn test_navigation_id() {
    let id = NavigationId::new();
    let s = serde_json::to_string(&id).unwrap();
    let _deserialized: NavigationId = serde_json::from_str(&s).unwrap();
}

#[test]
fn test_interception_handle_default() {
    let h = InterceptionHandle::default();
    assert_ne!(h.to_string(), "");
}

// ====================================================================
// Tests — Error construction
// ====================================================================

#[test]
fn test_bridge_error_browser_closed() {
    let info = BrowserClosedInfo {
        browser_id: BrowserId::new(),
        exit_code: Some(0),
        reason: "normal shutdown".into(),
    };
    let err = BridgeError::BrowserClosed(info);
    assert!(err.is_permanent());
    assert!(!err.is_recoverable());
    assert!(err.to_string().contains("browser process closed"));
}

#[test]
fn test_bridge_error_browser_crashed() {
    let info = BrowserCrashedInfo {
        browser_id: BrowserId::new(),
        crash_reason: "out of memory".into(),
        dump_path: None,
    };
    let err = BridgeError::BrowserCrashed(info);
    assert!(err.is_permanent());
}

#[test]
fn test_bridge_error_element_stale() {
    let id = ElementId::new(42);
    let err = BridgeError::ElementStale(id);
    assert!(err.is_permanent());
    assert!(err.to_string().contains("42"));
}

#[test]
fn test_bridge_error_timeout() {
    let err = BridgeError::Timeout;
    assert!(err.is_recoverable());
}

#[test]
fn test_bridge_error_navigation_timeout() {
    let err = BridgeError::NavigationTimeout {
        page_id: PageId::new(),
        url: "https://example.com".into(),
        timeout: Duration::from_secs(30),
    };
    assert!(!err.is_permanent());
    assert!(err.to_string().contains("timed out"));
}

#[test]
fn test_bridge_error_dialog() {
    let err = BridgeError::NoDialogOpen;
    assert_eq!(err.to_string(), "no dialog open");
}

#[test]
fn test_bridge_error_locator() {
    let strategy = LocatorStrategy::Css(".btn".into());
    let err = BridgeError::LocatorTimeout {
        strategy: Box::new(strategy),
        timeout: Duration::from_secs(5),
    };
    assert!(err.to_string().contains("locator timed out"));
}

#[test]
fn test_bridge_error_not_implemented() {
    let err = BridgeError::NotImplemented("screenshot");
    assert!(err.to_string().contains("not supported"));
}

#[test]
fn test_bridge_error_recoverability() {
    assert!(BridgeError::ConnectionTimedOut.is_recoverable());
    assert!(BridgeError::ConnectionRefused("refused".into()).is_recoverable());
    assert!(BridgeError::NetworkUnreachable.is_recoverable());
    assert!(BridgeError::Timeout.is_recoverable());
}

#[test]
fn test_bridge_error_permanence() {
    assert!(BridgeError::PageClosed(PageId::new()).is_permanent());
    assert!(BridgeError::FrameDetached(FrameId::new()).is_permanent());
    assert!(BridgeError::SessionClosed(SessionId::new()).is_permanent());
    assert!(BridgeError::ElementStale(ElementId::new(1)).is_permanent());
}

// ====================================================================
// Tests — Locator Strategy
// ====================================================================

#[test]
fn test_locator_strategy_css() {
    let s = LocatorStrategy::Css(".my-class".into());
    assert_eq!(s.to_string(), "css=.my-class");
}

#[test]
fn test_locator_strategy_xpath() {
    let s = LocatorStrategy::XPath("//div[@id='x']".into());
    assert_eq!(s.to_string(), "xpath=//div[@id='x']");
}

#[test]
fn test_locator_strategy_text() {
    let exact = LocatorStrategy::Text {
        text: "Submit".into(),
        exact: true,
    };
    assert_eq!(exact.to_string(), "text=\"Submit\"");
    let fuzzy = LocatorStrategy::Text {
        text: "Submit".into(),
        exact: false,
    };
    assert_eq!(fuzzy.to_string(), "text~=\"Submit\"");
}

#[test]
fn test_locator_strategy_role() {
    let with_name = LocatorStrategy::Role {
        role: "button".into(),
        name: Some("Click me".into()),
    };
    assert_eq!(with_name.to_string(), "role=button[name=\"Click me\"]");
    let no_name = LocatorStrategy::Role {
        role: "button".into(),
        name: None,
    };
    assert_eq!(no_name.to_string(), "role=button");
}

#[test]
fn test_locator_strategy_test_id() {
    let s = LocatorStrategy::TestId("login-btn".into());
    assert_eq!(s.to_string(), "testid=login-btn");
}

#[test]
fn test_locator_strategy_nested() {
    let parent = LocatorStrategy::Role {
        role: "form".into(),
        name: None,
    };
    let child = LocatorStrategy::Css("button".into());
    let s = LocatorStrategy::Nested(Box::new(parent), Box::new(child));
    assert_eq!(s.to_string(), "role=form >> css=button");
}

#[test]
fn test_locator_strategy_and() {
    let strategies = vec![
        LocatorStrategy::Css(".btn".into()),
        LocatorStrategy::Text {
            text: "Submit".into(),
            exact: true,
        },
    ];
    let s = LocatorStrategy::And(strategies);
    assert_eq!(s.to_string(), "(css=.btn && text=\"Submit\")");
}

#[test]
fn test_locator_strategy_or() {
    let strategies = vec![
        LocatorStrategy::Css(".btn".into()),
        LocatorStrategy::Css(".submit".into()),
    ];
    let s = LocatorStrategy::Or(strategies);
    assert_eq!(s.to_string(), "(css=.btn || css=.submit)");
}

#[test]
fn test_locator_strategy_placeholder() {
    let s = LocatorStrategy::Placeholder("Email".into());
    assert_eq!(s.to_string(), "placeholder=Email");
}

#[test]
fn test_locator_strategy_label() {
    let s = LocatorStrategy::Label("Username".into());
    assert_eq!(s.to_string(), "label=Username");
}

// ====================================================================
// Tests — Value types
// ====================================================================

#[test]
fn test_viewport_constants() {
    assert_eq!(Viewport::HD.width, 1280);
    assert_eq!(Viewport::HD.height, 720);
    assert_eq!(Viewport::FULL_HD.width, 1920);
    assert_eq!(Viewport::FULL_HD.height, 1080);
}

#[test]
fn test_screenshot_options_default() {
    let opts = ScreenshotOptions::default();
    assert_eq!(opts.format, ScreenshotFormat::Png);
    assert!(!opts.full_page);
}

#[test]
fn test_pdf_options_default() {
    let opts = PdfOptions::default();
    assert_eq!(opts.format, PdfPaperFormat::A4);
    assert_eq!(opts.scale, 1.0);
}

#[test]
fn test_launch_options_default() {
    let opts = LaunchOptions::default();
    assert!(opts.headless);
    assert_eq!(opts.timeout, Duration::from_secs(30));
}

#[test]
fn test_session_config_default() {
    let cfg = SessionConfig::default();
    assert!(cfg.incognito);
    assert!(cfg.accept_downloads);
}

#[test]
fn test_click_options_default() {
    let opts = ClickOptions::default();
    assert_eq!(opts.button, MouseButton::Left);
    assert_eq!(opts.click_count, 1);
}

#[test]
fn test_network_conditions_default() {
    let nc = NetworkConditions::default();
    assert!(!nc.offline);
    assert_eq!(nc.latency_ms, 0);
}

#[test]
fn test_cookie_creation() {
    let cookie = Cookie {
        name: "session".into(),
        value: "abc123".into(),
        domain: ".example.com".into(),
        path: "/".into(),
        secure: true,
        http_only: true,
        same_site: SameSitePolicy::Lax,
        expires: None,
    };
    assert_eq!(cookie.name, "session");
    assert!(cookie.secure);
}

#[test]
fn test_dialog_info() {
    let info = DialogInfo {
        dialog_type: DialogType::Confirm,
        message: "Are you sure?".into(),
        default_value: None,
        page_id: PageId::new(),
    };
    assert_eq!(info.dialog_type, DialogType::Confirm);
}

#[test]
fn test_download_state() {
    assert_eq!(format!("{:?}", DownloadState::InProgress), "InProgress");
    assert_eq!(format!("{:?}", DownloadState::Completed), "Completed");
    assert!(matches!(
        DownloadState::Failed("err".into()),
        DownloadState::Failed(_)
    ));
}

#[test]
fn test_resource_type_variants() {
    assert_eq!(format!("{:?}", ResourceType::Document), "Document");
    assert_eq!(format!("{:?}", ResourceType::WebSocket), "WebSocket");
}

#[test]
fn test_mouse_button_variants() {
    assert_eq!(MouseButton::Left as u8, 0);
    assert_eq!(MouseButton::Right as u8, 1);
}

#[test]
fn test_element_state_variants() {
    assert_eq!(ElementState::Visible as u8, 0);
    assert_eq!(ElementState::Hidden as u8, 1);
}

#[test]
fn test_dialog_type_variants() {
    assert_eq!(DialogType::Alert as u8, 0);
    assert_eq!(DialogType::BeforeUnload as u8, 3);
}

#[test]
fn test_same_site_policy_variants() {
    assert_eq!(SameSitePolicy::Strict as u8, 0);
    assert_eq!(SameSitePolicy::Lax as u8, 1);
    assert_eq!(SameSitePolicy::None as u8, 2);
}

#[test]
fn test_node_type_variants() {
    assert_eq!(NodeType::Element as u8, 0);
    assert_eq!(NodeType::ShadowRoot as u8, 4);
}

#[test]
fn test_permission_variants() {
    assert_eq!(Permission::Geolocation as u8, 0);
    assert_eq!(Permission::Notifications as u8, 3);
}

#[test]
fn test_handle_state() {
    assert_eq!(HandleState::Active as u8, 0);
    assert_eq!(HandleState::Detached as u8, 2);
}

// ====================================================================
// Tests — Trait Object Safety
// ====================================================================

#[test]
fn test_browser_trait_object() {
    let browser: Box<dyn BrowserPort> = Box::new(MockBrowser::new());
    let info = browser.info();
    assert_eq!(info.version, "120.0.0");
    assert!(browser.is_alive());
    browser.close().unwrap();
    assert!(!browser.is_alive());
}

#[test]
fn test_session_trait_object() {
    let browser: Box<dyn BrowserPort> = Box::new(MockBrowser::new());
    let session = browser.create_session(SessionConfig::default()).unwrap();
    let page = session.create_page().unwrap();
    assert_eq!(page.url(), "about:blank");
    session.close().unwrap();
}

#[test]
fn test_page_trait_object() {
    let page: Box<dyn PagePort> = Box::new(MockPage);
    assert_eq!(page.title(), "");
    let nav = page.navigate("https://example.com").unwrap();
    assert_eq!(nav.status, NavigationStatus::Finished);
    let content = page.content().unwrap();
    assert_eq!(content, "<html></html>");
    let _ss = page.screenshot(ScreenshotOptions::default()).unwrap();
    let _pdf = page.pdf(PdfOptions::default()).unwrap();
}

#[test]
fn test_frame_trait_object() {
    let frame: Box<dyn FramePort> = Box::new(MockFrame);
    assert!(frame.parent_id().is_none());
    let content = frame.content().unwrap();
    assert_eq!(content, "<html></html>");
    let _page = frame.page();
}

#[test]
fn test_element_trait_object() {
    let el: Box<dyn ElementPort> = Box::new(MockElement);
    assert_eq!(el.tag_name(), "div");
    assert!(el.is_visible().unwrap());
    assert!(el.is_enabled().unwrap());
    assert_eq!(el.id().get(), 42);
    let attr = el.get_attribute("id").unwrap();
    assert_eq!(attr, Some("test".into()));
}

#[test]
fn test_locator_trait_object() {
    let loc: Box<dyn LocatorPort> = Box::new(MockLocator);
    let strategy = LocatorStrategy::Css(".btn".into());
    let el = loc.locate(&strategy).unwrap();
    assert!(el.is_some());
    let all = loc.locate_all(&strategy).unwrap();
    assert_eq!(all.len(), 1);
}

#[test]
fn test_network_trait_object() {
    let net: Box<dyn NetworkPort> = Box::new(MockNetwork);
    net.set_offline(true).unwrap();
    net.clear_cache().unwrap();
    let rule = InterceptionRule {
        url_pattern: "*".into(),
        resource_types: vec![ResourceType::Document],
        action: InterceptionAction::Block,
    };
    let handle = net.add_interception_rule(rule).unwrap();
    net.remove_interception_rule(&handle).unwrap();
}

#[test]
fn test_input_trait_object() {
    let input: Box<dyn InputPort> = Box::new(MockInput);
    let el: Box<dyn ElementPort> = Box::new(MockElement);
    input.click(el.as_ref(), ClickOptions::default()).unwrap();
    input.fill(el.as_ref(), "text").unwrap();
    input.press_key("Enter").unwrap();
}

#[test]
fn test_storage_trait_object() {
    let storage: Box<dyn StoragePort> = Box::new(MockStorage);
    let cookies = storage.cookies().unwrap();
    assert!(cookies.is_empty());
    storage.delete_all_cookies().unwrap();
    storage.clear_local_storage().unwrap();
}

#[test]
fn test_dialog_trait_object() {
    let dialog: Box<dyn DialogPort> = Box::new(MockDialog);
    let next = dialog.next().unwrap();
    assert!(next.is_none());
    dialog.accept(None).unwrap();
    dialog.dismiss().unwrap();
}

#[test]
fn test_download_trait_object() {
    let dl: Box<dyn DownloadPort> = Box::new(MockDownload);
    assert!(dl.downloads().is_empty());
    let completed = dl.wait_for_completion(Duration::from_secs(5)).unwrap();
    assert!(completed.is_empty());
}

#[test]
fn test_artifact_trait_object() {
    let art: Box<dyn ArtifactPort> = Box::new(MockArtifact);
    let id = art
        .store("test", vec![1, 2, 3], ArtifactType::Screenshot)
        .unwrap();
    let data = art.retrieve(&id).unwrap();
    assert!(data.is_empty());
    let list = art.list(None).unwrap();
    assert!(list.is_empty());
}

#[test]
fn test_locator_engine_dyn() {
    let engine: Box<dyn LocatorEngine> = Box::new(MockLocator);
    let result = engine.query_selector("div").unwrap();
    assert!(result.is_some());
}

// ====================================================================
// Tests — Serialization
// ====================================================================

#[test]
fn test_browser_info_serialization() {
    let info = BrowserInfo {
        executable: PathBuf::from("/usr/bin/chromium"),
        version: "120.0".into(),
        user_data_dir: PathBuf::from("/tmp/test"),
    };
    let json = serde_json::to_string(&info).unwrap();
    let deserialized: BrowserInfo = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.version, "120.0");
}

#[test]
fn test_navigation_status_serialization() {
    let status = NavigationStatus::Finished;
    let json = serde_json::to_string(&status).unwrap();
    assert_eq!(json, "\"Finished\"");
    let deserialized: NavigationStatus = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, NavigationStatus::Finished);
}

#[test]
fn test_cookie_serialization() {
    let cookie = Cookie {
        name: "session".into(),
        value: "abc".into(),
        domain: ".example.com".into(),
        path: "/".into(),
        secure: true,
        http_only: false,
        same_site: SameSitePolicy::Lax,
        expires: None,
    };
    let json = serde_json::to_string(&cookie).unwrap();
    let deserialized: Cookie = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.name, "session");
    assert_eq!(deserialized.same_site, SameSitePolicy::Lax);
}

#[test]
fn test_page_id_serialization() {
    let id = PageId::new();
    let json = serde_json::to_string(&id).unwrap();
    let deserialized: PageId = serde_json::from_str(&json).unwrap();
    assert_eq!(id, deserialized);
}

#[test]
fn test_dialog_info_serialization() {
    let info = DialogInfo {
        dialog_type: DialogType::Prompt,
        message: "Enter name:".into(),
        default_value: Some("John".into()),
        page_id: PageId::new(),
    };
    let json = serde_json::to_string(&info).unwrap();
    let deserialized: DialogInfo = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.dialog_type, DialogType::Prompt);
    assert_eq!(deserialized.default_value, Some("John".into()));
}

#[test]
fn test_screenshot_format_serialization() {
    assert_eq!(
        serde_json::to_string(&ScreenshotFormat::Png).unwrap(),
        "\"Png\""
    );
    assert_eq!(
        serde_json::to_string(&ScreenshotFormat::Jpeg).unwrap(),
        "\"Jpeg\""
    );
}

// ====================================================================
// Tests — Edge cases
// ====================================================================

#[test]
fn test_empty_browser_sessions() {
    let browser: Box<dyn BrowserPort> = Box::new(MockBrowser::new());
    assert!(browser.sessions().is_empty());
}

#[test]
fn test_page_no_frames() {
    let page: Box<dyn PagePort> = Box::new(MockPage);
    assert!(page.frames().is_empty());
}

#[test]
fn test_element_stale_error_recovery_hint() {
    let err = BridgeError::ElementStale(ElementId::new(1));
    assert!(err.is_permanent());
    let msg = err.to_string();
    assert!(msg.contains("stale"));
}

#[test]
fn test_locator_ambiguous_error() {
    let strategy = LocatorStrategy::Css(".btn".into());
    let err = BridgeError::LocatorAmbiguous {
        strategy: Box::new(strategy),
        count: 5,
    };
    assert!(err.to_string().contains("5"));
}

#[test]
fn test_javascript_error() {
    let err = BridgeError::JavascriptError {
        page_id: PageId::new(),
        message: "x is not defined".into(),
        stack: Some("at <anonymous>:1:1".into()),
    };
    assert!(err.to_string().contains("x is not defined"));
}

#[test]
fn test_download_failed_error() {
    let err = BridgeError::DownloadFailed {
        download_id: "dl_1".into(),
        reason: "connection lost".into(),
    };
    assert!(err.to_string().contains("connection lost"));
}

#[test]
fn test_invalid_handle_error() {
    let err = BridgeError::InvalidHandle {
        handle_type: "ElementPort",
    };
    assert!(err.to_string().contains("ElementPort"));
}

// ====================================================================
// Tests — Send + Sync verification (compile-time)
// ====================================================================

#[test]
fn verify_send_sync() {
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}

    assert_send::<PageId>();
    assert_sync::<PageId>();
    assert_send::<BridgeError>();
    assert_sync::<BridgeError>();
    assert_send::<LocatorStrategy>();
    assert_sync::<LocatorStrategy>();
    assert_send::<Box<dyn BrowserPort>>();
    assert_sync::<Box<dyn BrowserPort>>();
    assert_send::<Box<dyn PagePort>>();
    assert_sync::<Box<dyn PagePort>>();
    assert_send::<Box<dyn ElementPort>>();
    assert_sync::<Box<dyn ElementPort>>();
}
