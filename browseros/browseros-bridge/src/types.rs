use crate::identifiers::{ArtifactId, FrameId, NavigationId, NodeId, PageId, SessionId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

// ——— Browser ———

/// Metadata about a browser process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserInfo {
    pub executable: PathBuf,
    pub version: String,
    pub user_data_dir: PathBuf,
}

/// Options for launching or connecting to a browser.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchOptions {
    pub executable: Option<PathBuf>,
    pub headless: bool,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub user_data_dir: Option<PathBuf>,
    pub timeout: Duration,
}

impl Default for LaunchOptions {
    fn default() -> Self {
        Self {
            executable: None,
            headless: true,
            args: Vec::new(),
            env: HashMap::new(),
            user_data_dir: None,
            timeout: Duration::from_secs(30),
        }
    }
}

// ——— Session ———

/// Configuration for a browser session (browsing context).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    pub incognito: bool,
    pub viewport: Option<Viewport>,
    pub user_agent: Option<String>,
    pub locale: Option<String>,
    pub timezone_id: Option<String>,
    pub permissions: HashMap<String, String>,
    pub download_path: Option<PathBuf>,
    pub extra_http_headers: HashMap<String, String>,
    pub offline: bool,
    pub accept_downloads: bool,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            incognito: true,
            viewport: None,
            user_agent: None,
            locale: None,
            timezone_id: None,
            permissions: HashMap::new(),
            download_path: None,
            extra_http_headers: HashMap::new(),
            offline: false,
            accept_downloads: true,
        }
    }
}

/// Viewport dimensions for a page.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
    pub device_scale_factor: Option<f64>,
    pub is_mobile: bool,
}

impl Viewport {
    pub const HD: Viewport = Viewport {
        width: 1280,
        height: 720,
        device_scale_factor: None,
        is_mobile: false,
    };
    pub const FULL_HD: Viewport = Viewport {
        width: 1920,
        height: 1080,
        device_scale_factor: None,
        is_mobile: false,
    };
}

/// Descriptor for a page (tab) in a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageDescriptor {
    pub id: PageId,
    pub session_id: SessionId,
    pub url: String,
    pub title: String,
}

/// Descriptor for a frame within a page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameDescriptor {
    pub id: FrameId,
    pub parent_id: Option<FrameId>,
    pub url: String,
    pub page_id: PageId,
}

// ——— Navigation ———

/// Current state of a navigation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavigationState {
    pub navigation_id: NavigationId,
    pub url: String,
    pub status: NavigationStatus,
}

/// Status of a page navigation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NavigationStatus {
    /// Navigation has started but not yet committed.
    Started,
    /// Navigation has committed (HTTP response received).
    Committed,
    /// Navigation finished successfully.
    Finished,
    /// Navigation failed or was aborted.
    Failed(String),
}

// ——— Screenshot / PDF ———

/// Options for taking a page screenshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenshotOptions {
    pub format: ScreenshotFormat,
    pub quality: Option<u8>,
    pub full_page: bool,
    pub clip: Option<ClipRegion>,
    pub omit_background: bool,
}

impl Default for ScreenshotOptions {
    fn default() -> Self {
        Self {
            format: ScreenshotFormat::Png,
            quality: None,
            full_page: false,
            clip: None,
            omit_background: false,
        }
    }
}

/// Screenshot image format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScreenshotFormat {
    Png,
    Jpeg,
    Webp,
}

/// Region of the page to capture.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ClipRegion {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Options for generating a PDF from a page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PdfOptions {
    pub scale: f64,
    pub print_background: bool,
    pub landscape: bool,
    pub format: PdfPaperFormat,
    pub margin: PdfMargins,
    pub header_template: Option<String>,
    pub footer_template: Option<String>,
    pub prefer_css_page_size: bool,
    pub page_ranges: Option<String>,
}

impl Default for PdfOptions {
    fn default() -> Self {
        Self {
            scale: 1.0,
            print_background: false,
            landscape: false,
            format: PdfPaperFormat::A4,
            margin: PdfMargins::default(),
            header_template: None,
            footer_template: None,
            prefer_css_page_size: false,
            page_ranges: None,
        }
    }
}

/// Standard paper sizes for PDF generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PdfPaperFormat {
    Letter,
    Legal,
    Tabloid,
    Ledger,
    A0,
    A1,
    A2,
    A3,
    A4,
    A5,
    A6,
}

/// Page margins for PDF generation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PdfMargins {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

impl Default for PdfMargins {
    fn default() -> Self {
        Self {
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            left: 0.0,
        }
    }
}

// ——— JavaScript ———

/// Result of a JavaScript evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsResult {
    pub value: serde_json::Value,
    pub exception_details: Option<ExceptionDetails>,
}

/// Details about a JavaScript exception.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExceptionDetails {
    pub message: String,
    pub stack: Option<String>,
    pub line_number: Option<u32>,
    pub column_number: Option<u32>,
}

/// Options for JavaScript file chooser interaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChooserOptions {
    pub accept_types: Vec<String>,
    pub multi_select: bool,
}

// ——— DOM ———

/// Type of a DOM node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeType {
    Element,
    Text,
    Document,
    DocumentFragment,
    ShadowRoot,
    Comment,
    ProcessingInstruction,
    DocumentType,
    CdataSection,
}

/// Snapshot of a single DOM node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub node_id: NodeId,
    pub node_type: NodeType,
    pub tag_name: String,
    pub attributes: HashMap<String, String>,
    pub text: String,
    pub children: Vec<NodeInfo>,
    pub frame_id: Option<FrameId>,
}

/// Box model of a DOM element.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BoxModel {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub padding: BoxEdges,
    pub margin: BoxEdges,
    pub border: BoxEdges,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct BoxEdges {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

/// Observable state of a DOM element.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ElementState {
    Visible,
    Hidden,
    Stable,
    Enabled,
    Disabled,
    Editable,
    Selected,
}

/// A 2D point.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

// ——— Network ———

/// Information about an HTTP request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestInfo {
    pub url: String,
    pub method: String,
    pub headers: HashMap<String, String>,
    pub post_data: Option<String>,
    pub resource_type: ResourceType,
    pub frame_id: Option<FrameId>,
}

/// Information about an HTTP response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseInfo {
    pub url: String,
    pub status: u16,
    pub status_text: String,
    pub headers: HashMap<String, String>,
    pub mime_type: String,
    pub remote_address: Option<String>,
    pub timing: TimingInfo,
}

/// Timing information for an HTTP request.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TimingInfo {
    pub dns_ms: f64,
    pub connect_ms: f64,
    pub ssl_ms: f64,
    pub send_ms: f64,
    pub wait_ms: f64,
    pub receive_ms: f64,
}

/// Type of a network resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResourceType {
    Document,
    Stylesheet,
    Image,
    Media,
    Font,
    Script,
    Xhr,
    Fetch,
    EventSource,
    WebSocket,
    Manifest,
    Ping,
    Preflight,
    Other,
}

/// A rule for intercepting network requests.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterceptionRule {
    pub url_pattern: String,
    pub resource_types: Vec<ResourceType>,
    pub action: InterceptionAction,
}

/// Action to take when a request is intercepted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InterceptionAction {
    Block,
    Continue,
    Respond {
        status: u16,
        headers: HashMap<String, String>,
        body: Vec<u8>,
    },
}

/// Network conditions for emulation.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct NetworkConditions {
    pub offline: bool,
    pub latency_ms: u64,
    pub download_throughput: Option<f64>,
    pub upload_throughput: Option<f64>,
}

/// A WebSocket message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketMessage {
    pub timestamp: f64,
    pub data: String,
    pub from_server: bool,
}

// ——— Input ———

/// Options for a click action.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ClickOptions {
    pub button: MouseButton,
    pub click_count: u32,
    pub delay: Duration,
    pub force: bool,
    pub no_wait_after: bool,
}

impl Default for ClickOptions {
    fn default() -> Self {
        Self {
            button: MouseButton::Left,
            click_count: 1,
            delay: Duration::ZERO,
            force: false,
            no_wait_after: false,
        }
    }
}

/// Mouse button type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
}

/// A mouse action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MouseAction {
    Click { button: MouseButton, count: u32 },
    DblClick,
    Down { button: MouseButton },
    Up { button: MouseButton },
    Move { x: f64, y: f64 },
    Wheel { delta_x: f64, delta_y: f64 },
}

/// A keyboard action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum KeyboardAction {
    Press { key: String },
    Down { key: String },
    Up { key: String },
    Type { text: String },
}

/// A touch action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TouchAction {
    Tap { x: f64, y: f64 },
    Press { x: f64, y: f64 },
    Move { x: f64, y: f64 },
    Release,
    Cancel,
}

/// A single action in an action sequence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Action {
    Mouse(MouseAction),
    Keyboard(KeyboardAction),
    Touch(TouchAction),
    Wait(Duration),
}

/// A sequence of input actions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionSequence {
    pub actions: Vec<Action>,
}

// ——— Storage ———

/// A single HTTP cookie.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
    pub same_site: SameSitePolicy,
    pub expires: Option<chrono::DateTime<chrono::Utc>>,
}

/// SameSite cookie policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SameSitePolicy {
    Strict,
    Lax,
    None,
}

/// A key-value entry in web storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageEntry {
    pub key: String,
    pub value: String,
}

// ——— Dialog ———

/// Type of a JavaScript dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DialogType {
    Alert,
    Confirm,
    Prompt,
    BeforeUnload,
}

/// Information about a JavaScript dialog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogInfo {
    pub dialog_type: DialogType,
    pub message: String,
    pub default_value: Option<String>,
    pub page_id: PageId,
}

// ——— Download ———

/// State of a file download.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DownloadState {
    InProgress,
    Completed,
    Cancelled,
    Failed(String),
}

/// Information about a file download.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadInfo {
    pub url: String,
    pub suggested_filename: String,
    pub file_path: Option<PathBuf>,
    pub mime_type: String,
    pub total_bytes: f64,
    pub received_bytes: f64,
    pub state: DownloadState,
    pub page_id: PageId,
    pub download_id: String,
}

// ——— Wait Conditions ———

/// A condition to wait for on a page.
#[derive(Debug, Clone)]
pub enum WaitCondition {
    /// Wait for navigation to complete.
    Navigation(Duration),
    /// Wait for a selector to match an element in the given state.
    Selector(String, ElementState),
    /// Wait for the page URL to match.
    Url(String),
    /// Wait for the page title to match.
    Title(String),
    /// Wait for network to become idle.
    NetworkIdle(Duration),
    /// Wait for a JavaScript function to return truthy.
    Function(String),
    /// Wait for all conditions to be met.
    All(Vec<WaitCondition>),
    /// Wait for any condition to be met.
    Any(Vec<WaitCondition>),
}

// ——— Browser port state ———

/// Operational state of a port/handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HandleState {
    Active,
    Closed,
    Detached,
    Error,
}

// ——— Permission ———

/// Browser permission type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Permission {
    Geolocation,
    Camera,
    Microphone,
    Notifications,
    Midi,
    ClipboardRead,
    ClipboardWrite,
    BackgroundSync,
    BackgroundFetch,
    PersistentStorage,
    PushAndMessaging,
    Sensors,
    AccessibilityEvents,
    PaymentHandler,
    IdleDetection,
    WindowManagement,
    LocalFonts,
    StorageAccess,
    TopLevelStorageAccess,
}

// ——— Artifact ———

/// Type of a stored artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArtifactType {
    Screenshot,
    Pdf,
    Trace,
    Har,
    Download,
    Recording,
    Snapshot,
    Other(String),
}

/// Metadata about a stored artifact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactMeta {
    pub id: ArtifactId,
    pub name: String,
    pub artifact_type: ArtifactType,
    pub size: u64,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub mime_type: String,
    pub source_url: Option<String>,
}

/// Filter for querying artifacts.
#[derive(Debug, Clone, Default)]
pub struct ArtifactFilter {
    pub types: Option<Vec<ArtifactType>>,
    pub before: Option<chrono::DateTime<chrono::Utc>>,
    pub after: Option<chrono::DateTime<chrono::Utc>>,
    pub name_contains: Option<String>,
}
