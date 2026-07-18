use crate::command;
use crate::error::{CdpError, CdpResult};
use crate::session::CdpSession;
use browseros_bridge::*;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn rule_url_pattern(pattern: &str) -> Option<&str> {
    if pattern.is_empty() || pattern == "*" {
        None
    } else {
        Some(pattern)
    }
}

fn map_error(e: CdpError) -> BridgeError {
    match e {
        CdpError::Transport(msg) => BridgeError::ConnectionRefused(msg),
        CdpError::ConnectionClosed => BridgeError::ConnectionRefused("Connection closed".into()),
        CdpError::ConnectionTimeout => BridgeError::ConnectionTimedOut,
        CdpError::CommandTimeout(_ms) => BridgeError::Timeout,
        CdpError::Protocol { code, message, .. } => {
            if message.contains("detached") || message.contains("stale") {
                BridgeError::ElementStale(ElementId::new(0))
            } else {
                BridgeError::Internal(format!("CDP error ({code}): {message}"))
            }
        }
        CdpError::Serialization(msg) => BridgeError::Internal(format!("Serialization: {msg}")),
        CdpError::NoResponse(id) => BridgeError::Internal(format!("No response for command {id}")),
        CdpError::CommandFailed(id, msg) => {
            BridgeError::Internal(format!("Command {id} failed: {msg}"))
        }
        CdpError::SessionNotFound(_id) => BridgeError::SessionNotFound(SessionId::default()),
        CdpError::TargetNotFound(t) => BridgeError::Internal(format!("Target not found: {t}")),
        CdpError::InvalidEndpoint(ep) => BridgeError::InvalidEndpoint(ep),
        CdpError::BrowserProcess(msg) => BridgeError::Internal(format!("Browser process: {msg}")),
        CdpError::NotImplemented(feature) => BridgeError::NotImplemented(feature),
    }
}

#[allow(dead_code)]
fn extract_result<T: serde::de::DeserializeOwned>(result: &Value) -> CdpResult<T> {
    serde_json::from_value(result.clone()).map_err(|e| CdpError::Serialization(e.to_string()))
}

fn result_field(result: &Value, field: &str) -> Option<String> {
    result
        .get(field)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn collect_frames(frame: &CdpFrame, out: &mut Vec<Box<dyn FramePort>>) {
    out.push(Box::new(CdpFrame {
        session: frame.session.clone(),
        frame_id: frame.frame_id,
        url_cache: frame.url_cache.clone(),
        title_cache: frame.title_cache.clone(),
        parent_frame_id: frame.parent_frame_id,
        children: vec![],
    }));
    for child in &frame.children {
        collect_frames(child, out);
    }
}

fn get_document_node_id(session: &CdpSession) -> BridgeResult<u64> {
    let doc: Value = session
        .send(command::dom::GET_DOCUMENT, None)
        .map_err(map_error)?;
    let node_id = doc["root"]["nodeId"].as_u64().unwrap_or(1);
    Ok(node_id)
}

fn evaluate_js(session: &CdpSession, expression: &str) -> BridgeResult<Value> {
    session
        .send::<Value>(
            command::runtime::EVALUATE,
            Some(serde_json::json!({
                "expression": expression,
                "returnByValue": true,
                "awaitPromise": true,
            })),
        )
        .map_err(map_error)
}

fn wait_for_condition(
    session: &CdpSession,
    condition: &WaitCondition,
    deadline: std::time::Instant,
) -> BridgeResult<()> {
    match condition {
        WaitCondition::Navigation(timeout) => {
            let nav_deadline = std::time::Instant::now() + *timeout;
            loop {
                if std::time::Instant::now() > nav_deadline {
                    return Ok(());
                }
                let ready = evaluate_js(session, "document.readyState")
                    .ok()
                    .and_then(|v| v["result"]["value"].as_str().map(|s| s.to_string()))
                    .unwrap_or_default();
                if ready == "complete" {
                    return Ok(());
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        WaitCondition::Selector(selector, _state) => loop {
            if std::time::Instant::now() > deadline {
                return Err(BridgeError::Timeout);
            }
            let doc: Value = session
                .send(command::dom::GET_DOCUMENT, None)
                .map_err(map_error)?;
            let doc_node_id = doc["result"]["nodeId"].as_u64().unwrap_or(1);
            let result: Value = session
                .send(
                    command::dom::QUERY_SELECTOR,
                    Some(command::dom::query_selector(doc_node_id, selector)),
                )
                .map_err(map_error)?;
            if result["nodeId"].as_u64().is_some() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(100));
        },
        WaitCondition::Url(pattern) => loop {
            if std::time::Instant::now() > deadline {
                return Err(BridgeError::Timeout);
            }
            let current = evaluate_js(session, "document.URL")
                .ok()
                .and_then(|v| v["result"]["value"].as_str().map(|s| s.to_string()))
                .unwrap_or_default();
            if current.contains(pattern) || current == *pattern {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(100));
        },
        WaitCondition::Title(pattern) => loop {
            if std::time::Instant::now() > deadline {
                return Err(BridgeError::Timeout);
            }
            let current = evaluate_js(session, "document.title")
                .ok()
                .and_then(|v| v["result"]["value"].as_str().map(|s| s.to_string()))
                .unwrap_or_default();
            if current.contains(pattern) || current == *pattern {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(100));
        },
        WaitCondition::NetworkIdle(timeout) => {
            let idle_deadline = std::time::Instant::now() + *timeout;
            while std::time::Instant::now() <= idle_deadline {
                std::thread::sleep(Duration::from_millis(200));
            }
            Ok(())
        }
        WaitCondition::Function(script) => loop {
            if std::time::Instant::now() > deadline {
                return Err(BridgeError::Timeout);
            }
            let result = evaluate_js(session, script)?;
            let truthy = result["result"]["value"].as_bool().unwrap_or(false)
                || result["result"]["value"].is_string();
            if truthy {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(100));
        },
        WaitCondition::All(conditions) => {
            for c in conditions {
                wait_for_condition(session, c, deadline)?;
            }
            Ok(())
        }
        WaitCondition::Any(conditions) => {
            let mut last_err = BridgeError::Timeout;
            for c in conditions {
                match wait_for_condition(session, c, deadline) {
                    Ok(()) => return Ok(()),
                    Err(e) => last_err = e,
                }
            }
            Err(last_err)
        }
    }
}

// ─── SessionPort ───────────────────────────────────────────

pub struct CdpSessionPort {
    session: CdpSession,
    config: SessionConfig,
    page_map: Arc<Mutex<HashMap<PageId, (String, CdpSession)>>>,
}

impl CdpSessionPort {
    pub fn new(session: CdpSession, config: SessionConfig) -> Self {
        Self {
            session,
            config,
            page_map: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn build_page(&self, page_id: PageId, child_session: CdpSession, target_id: String) -> CdpPage {
        CdpPage::with_id(page_id, child_session, target_id)
    }
}

impl SessionPort for CdpSessionPort {
    fn pages(&self) -> Vec<Box<dyn PagePort>> {
        let map = self.page_map.lock().unwrap_or_else(|e| e.into_inner());
        map.iter()
            .map(|(page_id, (target_id, child_session))| {
                Box::new(CdpPage::with_id(
                    *page_id,
                    child_session.clone(),
                    target_id.clone(),
                )) as Box<dyn PagePort>
            })
            .collect()
    }

    fn create_page(&self) -> BridgeResult<Box<dyn PagePort>> {
        let result: Value = self
            .session
            .send(
                command::target::CREATE_TARGET,
                Some(serde_json::json!({
                    "url": "about:blank",
                })),
            )
            .map_err(map_error)?;
        let target_id = result["targetId"]
            .as_str()
            .ok_or_else(|| BridgeError::Internal("Missing targetId".into()))?
            .to_string();

        let attach_result: Value = self
            .session
            .send(
                command::target::ATTACH_TO_TARGET,
                Some(serde_json::json!({
                    "targetId": target_id,
                    "flatten": true,
                })),
            )
            .map_err(map_error)?;
        let child_session_id = attach_result["sessionId"]
            .as_str()
            .ok_or_else(|| BridgeError::Internal("Missing sessionId".into()))?
            .to_string();

        let child_session = CdpSession::new(
            self.session.connection().clone(),
            &target_id,
            &child_session_id,
        );

        child_session
            .send::<Value>(command::page::ENABLE, None)
            .map_err(map_error)?;
        child_session
            .send::<Value>(command::runtime::ENABLE, None)
            .map_err(map_error)?;
        child_session
            .send::<Value>(command::dom::ENABLE, None)
            .map_err(map_error)?;
        let _ = child_session.send::<Value>(command::runtime::RUN_IF_WAITING_FOR_DEBUGGER, None);

        let page_id = PageId::new();
        let page = self.build_page(page_id, child_session.clone(), target_id.clone());
        self.page_map
            .lock()
            .map_err(|e| BridgeError::Internal(format!("page_map lock poisoned: {e}")))?
            .insert(page_id, (target_id, child_session));
        Ok(Box::new(page))
    }

    fn close_page(&self, page_id: &PageId) -> BridgeResult<()> {
        let target_id = self
            .page_map
            .lock()
            .map_err(|e| BridgeError::Internal(format!("page_map lock poisoned: {e}")))?
            .remove(page_id)
            .ok_or(BridgeError::PageNotFound(*page_id))?
            .0;
        self.session
            .send::<Value>(
                command::target::CLOSE_TARGET,
                Some(command::target::close_target(&target_id)),
            )
            .map_err(map_error)?;
        Ok(())
    }

    fn activate_page(&self, page_id: &PageId) -> BridgeResult<()> {
        let target_id = self
            .page_map
            .lock()
            .map_err(|e| BridgeError::Internal(format!("page_map lock poisoned: {e}")))?
            .get(page_id)
            .ok_or(BridgeError::PageNotFound(*page_id))?
            .0
            .clone();
        self.session
            .send::<Value>(
                command::target::ACTIVATE_TARGET,
                Some(command::target::activate_target(&target_id)),
            )
            .map_err(map_error)?;
        Ok(())
    }

    fn close(&self) -> BridgeResult<()> {
        Ok(())
    }

    fn config(&self) -> &SessionConfig {
        &self.config
    }
}

// ─── PagePort ──────────────────────────────────────────────

pub struct CdpPage {
    session: CdpSession,
    page_id: PageId,
    target_id: String,
    url_cache: Arc<Mutex<String>>,
    title_cache: Arc<Mutex<String>>,
    frame_tree_cache: Arc<Mutex<Option<Value>>>,
}

impl CdpPage {
    pub fn new(session: CdpSession, target_id: String) -> Self {
        Self {
            page_id: PageId::new(),
            target_id,
            session,
            url_cache: Arc::new(Mutex::new(String::new())),
            title_cache: Arc::new(Mutex::new(String::new())),
            frame_tree_cache: Arc::new(Mutex::new(None)),
        }
    }

    pub fn with_id(page_id: PageId, session: CdpSession, target_id: String) -> Self {
        Self {
            page_id,
            target_id,
            session,
            url_cache: Arc::new(Mutex::new(String::new())),
            title_cache: Arc::new(Mutex::new(String::new())),
            frame_tree_cache: Arc::new(Mutex::new(None)),
        }
    }

    fn refresh_frame_tree(&self) {
        let result: Value = match self
            .session
            .send(command::page::GET_FRAME_TREE, None)
            .map_err(map_error)
        {
            Ok(v) => v,
            Err(_) => return,
        };
        if let Some(tree) = result.get("frameTree") {
            *self
                .frame_tree_cache
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = Some(tree.clone());
        }
    }

    fn send<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        params: Option<Value>,
    ) -> BridgeResult<T> {
        self.session.send(method, params).map_err(map_error)
    }

    fn update_url_title(&self) {
        let url: Value = match self
            .session
            .send(
                "Runtime.evaluate",
                Some(serde_json::json!({
                    "expression": "document.URL",
                    "returnByValue": true,
                    "awaitPromise": true,
                })),
            )
            .map_err(map_error)
        {
            Ok(v) => v,
            Err(_) => return,
        };
        if let Ok(mut cached) = self.url_cache.lock() {
            if let Some(s) = url["result"]["value"].as_str() {
                *cached = s.to_string();
            }
        }

        let title: Value = match self
            .session
            .send(
                "Runtime.evaluate",
                Some(serde_json::json!({
                    "expression": "document.title",
                    "returnByValue": true,
                    "awaitPromise": true,
                })),
            )
            .map_err(map_error)
        {
            Ok(v) => v,
            Err(_) => return,
        };
        if let Ok(mut cached) = self.title_cache.lock() {
            if let Some(s) = title["result"]["value"].as_str() {
                *cached = s.to_string();
            }
        }
    }
}

impl CdpPage {
    fn wait_for_load(&self, timeout: Duration) -> BridgeResult<()> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            if std::time::Instant::now() > deadline {
                return Ok(());
            }
            let result: Value = match self.session.send(
                "Runtime.evaluate",
                Some(serde_json::json!({
                    "expression": "document.readyState",
                    "returnByValue": true,
                    "awaitPromise": true,
                })),
            ) {
                Ok(v) => v,
                Err(_) => {
                    std::thread::sleep(Duration::from_millis(100));
                    continue;
                }
            };
            let ready = result["result"]["value"].as_str().unwrap_or("");
            if ready == "complete" {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

impl PagePort for CdpPage {
    fn id(&self) -> PageId {
        self.page_id
    }

    fn url(&self) -> String {
        self.url_cache.lock().map(|g| g.clone()).unwrap_or_default()
    }

    fn title(&self) -> String {
        self.title_cache
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default()
    }

    fn navigate(&self, url: &str) -> BridgeResult<NavigationState> {
        let params = command::page::navigate(url);
        let result: Value = self.send(command::page::NAVIGATE, Some(params))?;

        let error_text = result
            .get("errorText")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        if let Some(err) = error_text {
            return Ok(NavigationState {
                navigation_id: NavigationId::default(),
                url: url.to_string(),
                status: NavigationStatus::Failed(err),
            });
        }

        let _ = self.wait_for_load(Duration::from_secs(30));
        self.update_url_title();
        self.refresh_frame_tree();
        let current_url = self.url();

        Ok(NavigationState {
            navigation_id: NavigationId::default(),
            url: current_url,
            status: NavigationStatus::Finished,
        })
    }

    fn reload(&self) -> BridgeResult<NavigationState> {
        let _result: Value = self.send(command::page::RELOAD, None)?;
        let _ = self.wait_for_load(Duration::from_secs(30));
        self.update_url_title();
        self.refresh_frame_tree();
        Ok(NavigationState {
            navigation_id: NavigationId::default(),
            url: self.url(),
            status: NavigationStatus::Finished,
        })
    }

    fn go_back(&self) -> BridgeResult<NavigationState> {
        let params = serde_json::json!({
            "expression": "window.history.back()",
            "returnByValue": true,
            "awaitPromise": true,
        });
        let _: Value = self.send("Runtime.evaluate", Some(params))?;
        let _ = self.wait_for_load(Duration::from_secs(30));
        self.update_url_title();
        self.refresh_frame_tree();
        Ok(NavigationState {
            navigation_id: NavigationId::default(),
            url: self.url(),
            status: NavigationStatus::Finished,
        })
    }

    fn go_forward(&self) -> BridgeResult<NavigationState> {
        let params = serde_json::json!({
            "expression": "window.history.forward()",
            "returnByValue": true,
            "awaitPromise": true,
        });
        let _: Value = self.send("Runtime.evaluate", Some(params))?;
        let _ = self.wait_for_load(Duration::from_secs(30));
        self.update_url_title();
        self.refresh_frame_tree();
        Ok(NavigationState {
            navigation_id: NavigationId::default(),
            url: self.url(),
            status: NavigationStatus::Finished,
        })
    }

    fn evaluate(&self, script: &str, arg: Option<&Value>) -> BridgeResult<JsResult> {
        let expression = if let Some(a) = arg {
            format!(
                "({})({})",
                script,
                serde_json::to_string(a).unwrap_or_default()
            )
        } else {
            script.to_string()
        };
        let params = command::runtime::evaluate(&expression, true, true);
        let result: Value = self.send(command::runtime::EVALUATE, Some(params))?;

        Ok(JsResult {
            value: result
                .get("result")
                .and_then(|r| r.get("value"))
                .cloned()
                .unwrap_or(Value::Null),
            exception_details: result.get("exceptionDetails").map(|ed| ExceptionDetails {
                message: ed["exception"]["description"]
                    .as_str()
                    .unwrap_or("")
                    .to_string(),
                stack: ed.get("stackTrace").map(|s| format!("{s:?}")),
                line_number: ed["lineNumber"].as_u64().map(|n| n as u32),
                column_number: ed["columnNumber"].as_u64().map(|n| n as u32),
            }),
        })
    }

    fn evaluate_handle(
        &self,
        script: &str,
        arg: Option<&Value>,
    ) -> BridgeResult<Box<dyn ElementPort>> {
        let expression = if let Some(a) = arg {
            format!(
                "({})({})",
                script,
                serde_json::to_string(a).unwrap_or_default()
            )
        } else {
            script.to_string()
        };
        let result: Value = self
            .session
            .send(
                command::runtime::EVALUATE,
                Some(serde_json::json!({
                    "expression": expression,
                    "returnByValue": false,
                    "awaitPromise": true,
                })),
            )
            .map_err(map_error)?;
        let object_id = result["result"]["objectId"]
            .as_str()
            .ok_or_else(|| BridgeError::Internal("Expression did not return a DOM object".into()))?
            .to_string();
        // Ensure DOM tree is populated so requestNode returns a valid nodeId.
        let _: Value = self
            .session
            .send(
                command::dom::GET_DOCUMENT,
                Some(serde_json::json!({"depth": -1})),
            )
            .unwrap_or(Value::Null);
        let node_result: Value = self
            .session
            .send(
                command::dom::REQUEST_NODE,
                Some(command::dom::request_node(&object_id)),
            )
            .map_err(map_error)?;
        let node_id = node_result["nodeId"]
            .as_u64()
            .ok_or_else(|| BridgeError::Internal("Missing nodeId from DOM.requestNode".into()))?;
        Ok(Box::new(CdpElement::new(self.session.clone(), node_id)))
    }

    fn screenshot(&self, options: ScreenshotOptions) -> BridgeResult<Vec<u8>> {
        let format = match options.format {
            ScreenshotFormat::Png => "png",
            ScreenshotFormat::Jpeg => "jpeg",
            ScreenshotFormat::Webp => "webp",
        };
        let params = command::page::capture_screenshot(format, options.quality, options.full_page);
        let result: Value = self.send(command::page::CAPTURE_SCREENSHOT, Some(params))?;
        let data = result["data"]
            .as_str()
            .ok_or_else(|| BridgeError::Internal("Missing screenshot data".into()))?;
        use base64::Engine;
        let engine = base64::engine::general_purpose::STANDARD;
        engine
            .decode(data)
            .map_err(|e| BridgeError::Internal(format!("Base64 decode: {e}")))
    }

    fn pdf(&self, options: PdfOptions) -> BridgeResult<Vec<u8>> {
        let paper_sizes = paper_size(&options.format);
        let params = command::page::print_to_pdf(
            options.landscape,
            options.print_background,
            options.scale,
            paper_sizes.0,
            paper_sizes.1,
            options.margin.top,
            options.margin.bottom,
            options.margin.left,
            options.margin.right,
        );
        let result: Value = self.send(command::page::PRINT_TO_PDF, Some(params))?;
        let data = result["data"]
            .as_str()
            .ok_or_else(|| BridgeError::Internal("Missing PDF data".into()))?;
        use base64::Engine;
        let engine = base64::engine::general_purpose::STANDARD;
        engine
            .decode(data)
            .map_err(|e| BridgeError::Internal(format!("Base64 decode: {e}")))
    }

    fn content(&self) -> BridgeResult<String> {
        let params = serde_json::json!({
            "expression": "document.documentElement.outerHTML",
            "returnByValue": true,
            "awaitPromise": true,
        });
        let result: Value = self.send("Runtime.evaluate", Some(params))?;
        Ok(result["result"]["value"].as_str().unwrap_or("").to_string())
    }

    fn set_content(&self, html: &str) -> BridgeResult<()> {
        let escaped = serde_json::Value::String(html.to_string());
        let script = format!(
            "document.open(); document.write({}); document.close();",
            escaped
        );
        let params = serde_json::json!({
            "expression": script,
            "returnByValue": true,
            "awaitPromise": true,
        });
        self.send::<Value>("Runtime.evaluate", Some(params))?;
        self.refresh_frame_tree();
        Ok(())
    }

    fn set_viewport(&self, viewport: Viewport) -> BridgeResult<()> {
        let params = command::page::set_viewport(
            viewport.width,
            viewport.height,
            viewport.device_scale_factor,
            viewport.is_mobile,
        );
        self.send::<Value>(command::page::SET_VIEWPORT, Some(params))?;
        Ok(())
    }

    fn wait_for(&self, condition: WaitCondition) -> BridgeResult<()> {
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        wait_for_condition(&self.session, &condition, deadline)
    }

    fn frames(&self) -> Vec<Box<dyn FramePort>> {
        let tree = self
            .frame_tree_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let tree = match tree {
            Some(t) => t,
            None => return vec![],
        };
        let main = CdpFrame::from_frame_tree(self.session.clone(), &tree, None);
        let mut all: Vec<Box<dyn FramePort>> = vec![];
        collect_frames(&main, &mut all);
        all
    }

    fn main_frame(&self) -> Box<dyn FramePort> {
        let tree = self
            .frame_tree_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        match tree {
            Some(t) => Box::new(CdpFrame::from_frame_tree(self.session.clone(), &t, None)),
            None => Box::new(CdpFrame::new(self.session.clone())),
        }
    }

    fn close(&self) -> BridgeResult<()> {
        self.send::<Value>(
            command::target::CLOSE_TARGET,
            Some(serde_json::json!({
                "targetId": self.target_id,
            })),
        )?;
        Ok(())
    }

    fn locator(&self) -> Box<dyn LocatorPort> {
        Box::new(CdpLocatorEngine::new(self.session.clone()))
    }

    fn network(&self) -> Box<dyn NetworkPort> {
        Box::new(CdpNetworkPort::new(self.session.clone()))
    }

    fn input(&self) -> Box<dyn InputPort> {
        Box::new(CdpInputPort::new(self.session.clone()))
    }

    fn storage(&self) -> Box<dyn StoragePort> {
        Box::new(CdpStoragePort::new(self.session.clone()))
    }

    fn dialog(&self) -> Box<dyn DialogPort> {
        Box::new(CdpDialogPort::new(self.session.clone(), self.page_id))
    }

    fn download(&self) -> Box<dyn DownloadPort> {
        Box::new(CdpDownloadPort::new(self.session.clone()))
    }
}

// ─── FramePort ─────────────────────────────────────────────

#[derive(Clone)]
pub struct CdpFrame {
    session: CdpSession,
    frame_id: FrameId,
    url_cache: Arc<Mutex<String>>,
    title_cache: Arc<Mutex<String>>,
    parent_frame_id: Option<FrameId>,
    children: Vec<CdpFrame>,
}

impl CdpFrame {
    pub fn new(session: CdpSession) -> Self {
        Self {
            session,
            frame_id: FrameId::default(),
            url_cache: Arc::new(Mutex::new(String::new())),
            title_cache: Arc::new(Mutex::new(String::new())),
            parent_frame_id: None,
            children: vec![],
        }
    }

    pub fn from_frame_tree(session: CdpSession, value: &Value, parent: Option<FrameId>) -> Self {
        let frame = &value["frame"];
        let frame_id = frame["id"]
            .as_str()
            .and_then(|s| uuid::Uuid::parse_str(s).ok())
            .map(FrameId::from_uuid)
            .unwrap_or_default();
        let url = frame["url"].as_str().unwrap_or("").to_string();
        let parent_id = parent.or_else(|| {
            frame["parentId"]
                .as_str()
                .and_then(|s| uuid::Uuid::parse_str(s).ok())
                .map(FrameId::from_uuid)
        });
        let children: Vec<CdpFrame> = value["childFrames"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .map(|child| CdpFrame::from_frame_tree(session.clone(), child, Some(frame_id)))
                    .collect()
            })
            .unwrap_or_default();
        Self {
            session,
            frame_id,
            url_cache: Arc::new(Mutex::new(url)),
            title_cache: Arc::new(Mutex::new(String::new())),
            parent_frame_id: parent_id,
            children,
        }
    }

    #[allow(dead_code)]
    fn update_url_title(&self) {
        let url: Value = match self
            .session
            .send(
                "Runtime.evaluate",
                Some(serde_json::json!({
                    "expression": "document.URL",
                    "returnByValue": true,
                    "awaitPromise": true,
                })),
            )
            .map_err(map_error)
        {
            Ok(v) => v,
            Err(_) => return,
        };
        if let Ok(mut cached) = self.url_cache.lock() {
            if let Some(s) = url["result"]["value"].as_str() {
                *cached = s.to_string();
            }
        }
        let title: Value = match self
            .session
            .send(
                "Runtime.evaluate",
                Some(serde_json::json!({
                    "expression": "document.title",
                    "returnByValue": true,
                    "awaitPromise": true,
                })),
            )
            .map_err(map_error)
        {
            Ok(v) => v,
            Err(_) => return,
        };
        if let Ok(mut cached) = self.title_cache.lock() {
            if let Some(s) = title["result"]["value"].as_str() {
                *cached = s.to_string();
            }
        }
    }
}

impl FramePort for CdpFrame {
    fn id(&self) -> FrameId {
        self.frame_id
    }

    fn url(&self) -> String {
        self.url_cache.lock().map(|g| g.clone()).unwrap_or_default()
    }

    fn title(&self) -> String {
        self.title_cache
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default()
    }

    fn parent_id(&self) -> Option<FrameId> {
        self.parent_frame_id
    }

    fn content(&self) -> BridgeResult<String> {
        let params = serde_json::json!({
            "expression": "document.documentElement.outerHTML",
            "returnByValue": true,
            "awaitPromise": true,
        });
        let result: Value = self
            .session
            .send("Runtime.evaluate", Some(params))
            .map_err(map_error)?;
        Ok(result["result"]["value"].as_str().unwrap_or("").to_string())
    }

    fn set_content(&self, html: &str) -> BridgeResult<()> {
        let escaped = serde_json::Value::String(html.to_string());
        let script = format!(
            "document.open(); document.write({}); document.close();",
            escaped
        );
        let params = serde_json::json!({
            "expression": script,
            "returnByValue": true,
            "awaitPromise": true,
        });
        self.session
            .send::<Value>("Runtime.evaluate", Some(params))
            .map_err(map_error)?;
        Ok(())
    }

    fn evaluate(&self, script: &str, arg: Option<&Value>) -> BridgeResult<JsResult> {
        let expression = if let Some(a) = arg {
            format!(
                "({})({})",
                script,
                serde_json::to_string(a).unwrap_or_default()
            )
        } else {
            script.to_string()
        };
        let params = command::runtime::evaluate(&expression, true, true);
        let result: Value = self
            .session
            .send(command::runtime::EVALUATE, Some(params))
            .map_err(map_error)?;
        Ok(JsResult {
            value: result
                .get("result")
                .and_then(|r| r.get("value"))
                .cloned()
                .unwrap_or(Value::Null),
            exception_details: result.get("exceptionDetails").map(|ed| ExceptionDetails {
                message: ed["exception"]["description"]
                    .as_str()
                    .unwrap_or("")
                    .to_string(),
                stack: ed.get("stackTrace").map(|s| format!("{s:?}")),
                line_number: ed["lineNumber"].as_u64().map(|n| n as u32),
                column_number: ed["columnNumber"].as_u64().map(|n| n as u32),
            }),
        })
    }

    fn child_frames(&self) -> Vec<Box<dyn FramePort>> {
        self.children
            .iter()
            .map(|child| {
                Box::new(CdpFrame {
                    session: child.session.clone(),
                    frame_id: child.frame_id,
                    url_cache: child.url_cache.clone(),
                    title_cache: child.title_cache.clone(),
                    parent_frame_id: child.parent_frame_id,
                    children: child.children.clone(),
                }) as Box<dyn FramePort>
            })
            .collect()
    }

    fn page(&self) -> Box<dyn PagePort> {
        Box::new(CdpPage::new(
            self.session.clone(),
            self.session.target_id().to_string(),
        ))
    }
}

// ─── ElementPort ───────────────────────────────────────────

#[allow(dead_code)]
pub struct CdpElement {
    session: CdpSession,
    node_id: u64,
    backend_node_id: Option<u64>,
}

impl CdpElement {
    pub fn new(session: CdpSession, node_id: u64) -> Self {
        Self {
            session,
            node_id,
            backend_node_id: None,
        }
    }

    fn call_on_element(&self, function: &str) -> BridgeResult<Value> {
        let resolve: Value = self
            .session
            .send(
                command::dom::RESOLVE_NODE,
                Some(serde_json::json!({
                    "nodeId": self.node_id,
                    "objectGroup": "element-state",
                })),
            )
            .map_err(map_error)?;
        let object_id = resolve["object"]["objectId"]
            .as_str()
            .ok_or_else(|| BridgeError::Internal("Missing objectId".into()))?
            .to_string();
        self.session
            .send(
                command::runtime::CALL_FUNCTION_ON,
                Some(serde_json::json!({
                    "functionDeclaration": function,
                    "objectId": object_id,
                    "returnByValue": true,
                    "awaitPromise": true,
                })),
            )
            .map_err(map_error)
    }
}

impl ElementPort for CdpElement {
    fn id(&self) -> ElementId {
        ElementId::new(self.node_id)
    }

    fn tag_name(&self) -> String {
        let params = serde_json::json!({ "nodeId": self.node_id });
        if let Ok(result) = self
            .session
            .send::<Value>(command::dom::DESCRIBE_NODE, Some(params))
        {
            if let Some(name) = result["node"]["localName"].as_str() {
                return name.to_string();
            }
        }
        String::new()
    }

    fn text_content(&self) -> BridgeResult<String> {
        let resolve: Value = self
            .session
            .send(
                command::dom::RESOLVE_NODE,
                Some(serde_json::json!({
                    "nodeId": self.node_id,
                    "objectGroup": "element-state",
                })),
            )
            .map_err(map_error)?;
        let object_id = resolve["object"]["objectId"]
            .as_str()
            .ok_or_else(|| BridgeError::Internal("Missing objectId in text_content".into()))?
            .to_string();
        let result: Value = self
            .session
            .send(
                command::runtime::CALL_FUNCTION_ON,
                Some(serde_json::json!({
                    "functionDeclaration": "function() { return this.textContent || ''; }",
                    "objectId": object_id,
                    "returnByValue": true,
                    "awaitPromise": true,
                })),
            )
            .map_err(map_error)?;
        Ok(result["result"]["value"].as_str().unwrap_or("").to_string())
    }

    fn inner_html(&self) -> BridgeResult<String> {
        let result: Value = self
            .session
            .send(
                command::dom::GET_OUTER_HTML,
                Some(command::dom::get_outer_html(self.node_id)),
            )
            .map_err(map_error)?;
        Ok(result_field(&result, "outerHTML").unwrap_or_default())
    }

    fn outer_html(&self) -> BridgeResult<String> {
        let result: Value = self
            .session
            .send(
                command::dom::GET_OUTER_HTML,
                Some(command::dom::get_outer_html(self.node_id)),
            )
            .map_err(map_error)?;
        Ok(result_field(&result, "outerHTML").unwrap_or_default())
    }

    fn get_attribute(&self, name: &str) -> BridgeResult<Option<String>> {
        let result: Value = self.session.send(command::runtime::EVALUATE, Some(serde_json::json!({
            "expression": format!("document.querySelector('[cdp_node_id=\"{}\"]')?.getAttribute('{name}') || ''", self.node_id),
            "returnByValue": true,
        }))).map_err(map_error)?;
        let value = result_field(&result, "value").unwrap_or_default();
        if value.is_empty() {
            Ok(None)
        } else {
            Ok(Some(value))
        }
    }

    fn set_attribute(&self, name: &str, value: &str) -> BridgeResult<()> {
        self.session
            .send::<Value>(
                command::dom::SET_ATTRIBUTE_VALUE,
                Some(command::dom::set_attribute(self.node_id, name, value)),
            )
            .map_err(map_error)?;
        Ok(())
    }

    fn has_attribute(&self, name: &str) -> BridgeResult<bool> {
        let result: Value = self
            .session
            .send(
                command::dom::GET_ATTRIBUTES,
                Some(command::dom::get_outer_html(self.node_id)),
            )
            .map_err(map_error)?;
        let attrs = result["attributes"].as_array().cloned().unwrap_or_default();
        Ok(attrs.iter().any(|a| a.as_str() == Some(name)))
    }

    fn attributes(&self) -> BridgeResult<std::collections::HashMap<String, String>> {
        let result: Value = self
            .session
            .send(
                command::dom::GET_ATTRIBUTES,
                Some(command::dom::get_outer_html(self.node_id)),
            )
            .map_err(map_error)?;
        let attrs = result["attributes"].as_array().cloned().unwrap_or_default();
        let mut map = std::collections::HashMap::new();
        for chunk in attrs.chunks(2) {
            if let (Some(k), Some(v)) = (
                chunk.first().and_then(|c| c.as_str()),
                chunk.get(1).and_then(|c| c.as_str()),
            ) {
                map.insert(k.to_string(), v.to_string());
            }
        }
        Ok(map)
    }

    fn bounding_box(&self) -> BridgeResult<Option<BoxModel>> {
        let result = self
            .session
            .send::<Value>(
                command::dom::GET_BOX_MODEL,
                Some(command::dom::get_box_model(self.node_id)),
            )
            .map_err(map_error);
        match result {
            Ok(val) => {
                let model = val["model"].clone();
                let content = &model["content"];
                Ok(Some(BoxModel {
                    x: content[0].as_f64().unwrap_or(0.0),
                    y: content[1].as_f64().unwrap_or(0.0),
                    width: (content[2].as_f64().unwrap_or(0.0)
                        - content[0].as_f64().unwrap_or(0.0))
                    .abs(),
                    height: (content[3].as_f64().unwrap_or(0.0)
                        - content[1].as_f64().unwrap_or(0.0))
                    .abs(),
                    padding: BoxEdges::default(),
                    margin: BoxEdges::default(),
                    border: BoxEdges::default(),
                }))
            }
            Err(_) => Ok(None),
        }
    }

    fn is_visible(&self) -> BridgeResult<bool> {
        let result = self.call_on_element(
            "function() {
                const s = window.getComputedStyle(this);
                return this.offsetWidth > 0 && this.offsetHeight > 0
                    && s.visibility !== 'hidden'
                    && s.display !== 'none'
                    && !this.hidden;
            }",
        )?;
        Ok(result["result"]["value"].as_bool().unwrap_or(false))
    }

    fn is_enabled(&self) -> BridgeResult<bool> {
        let result = self.call_on_element("function() { return !this.disabled; }")?;
        Ok(result["result"]["value"].as_bool().unwrap_or(true))
    }

    fn is_checked(&self) -> BridgeResult<bool> {
        let result = self.call_on_element("function() { return !!this.checked; }")?;
        Ok(result["result"]["value"].as_bool().unwrap_or(false))
    }

    fn is_selected(&self) -> BridgeResult<bool> {
        let result = self.call_on_element("function() { return !!this.selected; }")?;
        Ok(result["result"]["value"].as_bool().unwrap_or(false))
    }

    fn is_stable(&self) -> BridgeResult<bool> {
        let pos1: Value = match self
            .session
            .send(
                command::dom::GET_BOX_MODEL,
                Some(command::dom::get_box_model(self.node_id)),
            )
            .map_err(map_error)
        {
            Ok(v) => v,
            Err(_) => return Ok(false),
        };
        std::thread::sleep(Duration::from_millis(50));
        let pos2: Value = match self
            .session
            .send(
                command::dom::GET_BOX_MODEL,
                Some(command::dom::get_box_model(self.node_id)),
            )
            .map_err(map_error)
        {
            Ok(v) => v,
            Err(_) => return Ok(false),
        };
        let c1 = pos1["model"]["content"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let c2 = pos2["model"]["content"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if c1.len() != c2.len() {
            return Ok(false);
        }
        Ok(c1.iter().zip(c2.iter()).all(|(a, b)| {
            let x1 = a[0].as_f64().unwrap_or(0.0);
            let y1 = a[1].as_f64().unwrap_or(0.0);
            let x2 = b[0].as_f64().unwrap_or(0.0);
            let y2 = b[1].as_f64().unwrap_or(0.0);
            (x1 - x2).abs() < 0.5 && (y1 - y2).abs() < 0.5
        }))
    }

    fn scroll_into_view(&self) -> BridgeResult<()> {
        let _: Value = self.session.send(command::runtime::EVALUATE, Some(serde_json::json!({
            "expression": format!("document.querySelector('[cdp_node_id=\"{}\"]')?.scrollIntoView()", self.node_id),
            "returnByValue": true,
        }))).map_err(map_error)?;
        Ok(())
    }

    fn click_point(&self) -> BridgeResult<Point> {
        let result = self
            .session
            .send::<Value>(
                command::dom::GET_BOX_MODEL,
                Some(command::dom::get_box_model(self.node_id)),
            )
            .map_err(map_error)?;
        let content = &result["model"]["content"];
        let arr = content.as_array().map(|a| a.as_slice()).unwrap_or(&[]);
        let len = arr.len().max(1);
        let x = arr
            .iter()
            .map(|p| p[0].as_f64().unwrap_or(0.0))
            .sum::<f64>()
            / len as f64;
        let y = arr
            .iter()
            .map(|p| p[1].as_f64().unwrap_or(0.0))
            .sum::<f64>()
            / len as f64;
        Ok(Point { x, y })
    }

    fn query_selector(&self, selector: &str) -> BridgeResult<Option<Box<dyn ElementPort>>> {
        let result: Value = self
            .session
            .send(
                command::dom::QUERY_SELECTOR,
                Some(command::dom::query_selector(self.node_id, selector)),
            )
            .map_err(map_error)?;
        let node_id = result["nodeId"].as_u64();
        Ok(node_id.map(|nid| {
            Box::new(CdpElement::new(self.session.clone(), nid)) as Box<dyn ElementPort>
        }))
    }

    fn query_selector_all(&self, selector: &str) -> BridgeResult<Vec<Box<dyn ElementPort>>> {
        let result: Value = self
            .session
            .send(
                command::dom::QUERY_SELECTOR_ALL,
                Some(command::dom::query_selector(self.node_id, selector)),
            )
            .map_err(map_error)?;
        let node_ids = result["nodeIds"].as_array().cloned().unwrap_or_default();
        Ok(node_ids
            .iter()
            .filter_map(|v| v.as_u64())
            .map(|nid| Box::new(CdpElement::new(self.session.clone(), nid)) as Box<dyn ElementPort>)
            .collect())
    }

    fn focus(&self) -> BridgeResult<()> {
        self.session
            .send::<Value>(
                command::dom::FOCUS,
                Some(command::dom::focus_node(self.node_id)),
            )
            .map_err(map_error)?;
        Ok(())
    }

    fn hover(&self) -> BridgeResult<()> {
        let point = self.click_point()?;
        let _: Value = self
            .session
            .send(
                command::input::DISPATCH_MOUSE_EVENT,
                Some(serde_json::json!({
                    "type": "mouseMoved",
                    "x": point.x,
                    "y": point.y,
                })),
            )
            .map_err(map_error)?;
        Ok(())
    }

    fn snapshot(&self) -> BridgeResult<NodeInfo> {
        let result: Value = self
            .session
            .send(
                command::dom::DESCRIBE_NODE,
                Some(command::dom::describe_node(self.node_id, 1)),
            )
            .map_err(map_error)?;
        let node = &result["node"];
        Ok(NodeInfo {
            node_id: NodeId::new(self.node_id),
            node_type: browseros_bridge::NodeType::Element,
            tag_name: node["localName"].as_str().unwrap_or("").to_string(),
            attributes: std::collections::HashMap::new(),
            text: node["nodeValue"].as_str().unwrap_or("").to_string(),
            children: vec![],
            frame_id: node["frameId"].as_str().map(|_| FrameId::default()),
        })
    }

    fn owning_frame(&self) -> Box<dyn FramePort> {
        Box::new(CdpFrame::new(self.session.clone()))
    }
}

// ─── LocatorPort + LocatorEngine ───────────────────────────

pub struct CdpLocatorEngine {
    session: CdpSession,
}

impl CdpLocatorEngine {
    pub fn new(session: CdpSession) -> Self {
        Self { session }
    }
}

impl LocatorPort for CdpLocatorEngine {
    fn locate(&self, strategy: &LocatorStrategy) -> BridgeResult<Option<Box<dyn ElementPort>>> {
        let selector = locator_to_selector(strategy)?;
        let doc_node_id = get_document_node_id(&self.session)?;
        let result: Value = self
            .session
            .send(
                command::dom::QUERY_SELECTOR,
                Some(command::dom::query_selector(doc_node_id, &selector)),
            )
            .map_err(map_error)?;
        Ok(result["nodeId"].as_u64().map(|nid| {
            Box::new(CdpElement::new(self.session.clone(), nid)) as Box<dyn ElementPort>
        }))
    }

    fn locate_all(&self, strategy: &LocatorStrategy) -> BridgeResult<Vec<Box<dyn ElementPort>>> {
        let selector = locator_to_selector(strategy)?;
        let doc_node_id = get_document_node_id(&self.session)?;
        let result: Value = self
            .session
            .send(
                command::dom::QUERY_SELECTOR_ALL,
                Some(command::dom::query_selector(doc_node_id, &selector)),
            )
            .map_err(map_error)?;
        let node_ids = result["nodeIds"].as_array().cloned().unwrap_or_default();
        Ok(node_ids
            .iter()
            .filter_map(|v| v.as_u64())
            .map(|nid| Box::new(CdpElement::new(self.session.clone(), nid)) as Box<dyn ElementPort>)
            .collect())
    }

    fn wait_for(
        &self,
        strategy: &LocatorStrategy,
        timeout: Duration,
    ) -> BridgeResult<Box<dyn ElementPort>> {
        let selector = locator_to_selector(strategy)?;
        let deadline = std::time::Instant::now() + timeout;
        loop {
            if std::time::Instant::now() > deadline {
                return Err(BridgeError::Timeout);
            }
            let doc_node_id = match get_document_node_id(&self.session) {
                Ok(id) => id,
                Err(_) => {
                    std::thread::sleep(Duration::from_millis(100));
                    continue;
                }
            };
            let result: Value = self
                .session
                .send(
                    command::dom::QUERY_SELECTOR,
                    Some(command::dom::query_selector(doc_node_id, &selector)),
                )
                .map_err(map_error)?;
            if let Some(node_id) = result["nodeId"].as_u64() {
                return Ok(Box::new(CdpElement::new(self.session.clone(), node_id)));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn wait_for_absence(&self, strategy: &LocatorStrategy, timeout: Duration) -> BridgeResult<()> {
        let selector = locator_to_selector(strategy)?;
        let deadline = std::time::Instant::now() + timeout;
        loop {
            if std::time::Instant::now() > deadline {
                return Ok(());
            }
            let result: Value = self
                .session
                .send(
                    command::dom::QUERY_SELECTOR,
                    Some(command::dom::query_selector(1, &selector)),
                )
                .map_err(map_error)?;
            if result["nodeId"].as_u64().is_none() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

impl LocatorEngine for CdpLocatorEngine {
    fn query_selector(&self, selector: &str) -> BridgeResult<Option<Box<dyn ElementPort>>> {
        let result: Value = self
            .session
            .send(
                command::dom::QUERY_SELECTOR,
                Some(command::dom::query_selector(1, selector)),
            )
            .map_err(map_error)?;
        Ok(result["nodeId"].as_u64().map(|nid| {
            Box::new(CdpElement::new(self.session.clone(), nid)) as Box<dyn ElementPort>
        }))
    }

    fn query_selector_all(&self, selector: &str) -> BridgeResult<Vec<Box<dyn ElementPort>>> {
        let result: Value = self
            .session
            .send(
                command::dom::QUERY_SELECTOR_ALL,
                Some(command::dom::query_selector(1, selector)),
            )
            .map_err(map_error)?;
        let node_ids = result["nodeIds"].as_array().cloned().unwrap_or_default();
        Ok(node_ids
            .iter()
            .filter_map(|v| v.as_u64())
            .map(|nid| Box::new(CdpElement::new(self.session.clone(), nid)) as Box<dyn ElementPort>)
            .collect())
    }

    fn query_by_text(&self, text: &str, exact: bool) -> BridgeResult<Vec<Box<dyn ElementPort>>> {
        let xpath = if exact {
            format!("//text()='{text}'")
        } else {
            format!("//text()[contains(., '{text}')]")
        };
        let result: Value = self
            .session
            .send(
                command::dom::QUERY_SELECTOR_ALL,
                Some(command::dom::query_selector(1, &xpath)),
            )
            .map_err(map_error)?;
        let node_ids = result["nodeIds"].as_array().cloned().unwrap_or_default();
        Ok(node_ids
            .iter()
            .filter_map(|v| v.as_u64())
            .map(|nid| Box::new(CdpElement::new(self.session.clone(), nid)) as Box<dyn ElementPort>)
            .collect())
    }

    fn query_by_xpath(&self, expression: &str) -> BridgeResult<Vec<Box<dyn ElementPort>>> {
        let result: Value = self
            .session
            .send(
                command::dom::QUERY_SELECTOR_ALL,
                Some(command::dom::query_selector(1, expression)),
            )
            .map_err(map_error)?;
        let node_ids = result["nodeIds"].as_array().cloned().unwrap_or_default();
        Ok(node_ids
            .iter()
            .filter_map(|v| v.as_u64())
            .map(|nid| Box::new(CdpElement::new(self.session.clone(), nid)) as Box<dyn ElementPort>)
            .collect())
    }
}

fn locator_to_selector(strategy: &LocatorStrategy) -> BridgeResult<String> {
    match strategy {
        LocatorStrategy::Css(s) => Ok(s.clone()),
        LocatorStrategy::XPath(s) => Ok(s.clone()),
        LocatorStrategy::Text { text, exact } => {
            if *exact {
                Ok(format!("//text()='{text}'"))
            } else {
                Ok(format!("//text()[contains(., '{text}')]"))
            }
        }
        LocatorStrategy::Role { role, name } => {
            if let Some(n) = name {
                Ok(format!("[role=\"{role}\"][aria-label=\"{n}\"]"))
            } else {
                Ok(format!("[role=\"{role}\"]"))
            }
        }
        LocatorStrategy::TestId(id) => Ok(format!("[data-testid=\"{id}\"]")),
        LocatorStrategy::Placeholder(text) => Ok(format!("[placeholder=\"{text}\"]")),
        LocatorStrategy::Label(text) => Ok(format!("[aria-label=\"{text}\"]")),
        LocatorStrategy::AltText(text) => Ok(format!("img[alt=\"{text}\"]")),
        LocatorStrategy::Title(text) => Ok(format!("[title=\"{text}\"]")),
        LocatorStrategy::Nested(parent, child) => {
            let parent_sel = locator_to_selector(parent)?;
            let child_sel = locator_to_selector(child)?;
            Ok(format!("{parent_sel} {child_sel}"))
        }
        LocatorStrategy::And(strategies) => {
            let selectors: BridgeResult<Vec<String>> =
                strategies.iter().map(locator_to_selector).collect();
            Ok(selectors?.join(""))
        }
        LocatorStrategy::Or(strategies) => {
            let selectors: BridgeResult<Vec<String>> =
                strategies.iter().map(locator_to_selector).collect();
            Ok(selectors?.join(", "))
        }
    }
}

// ─── NetworkPort ────────────────────────────────────────────

fn resource_type_to_cdp(rt: &ResourceType) -> &'static str {
    match rt {
        ResourceType::Document => "Document",
        ResourceType::Stylesheet => "Stylesheet",
        ResourceType::Image => "Image",
        ResourceType::Media => "Media",
        ResourceType::Font => "Font",
        ResourceType::Script => "Script",
        ResourceType::Xhr => "XHR",
        ResourceType::Fetch => "Fetch",
        ResourceType::EventSource => "EventSource",
        ResourceType::WebSocket => "WebSocket",
        ResourceType::Manifest => "Manifest",
        ResourceType::Ping => "Ping",
        ResourceType::Preflight => "Preflight",
        ResourceType::Other => "Other",
    }
}

fn url_pattern_matches(pattern: &str, url: &str) -> bool {
    if pattern.is_empty() || pattern == "*" {
        return true;
    }
    if let Some(suffix) = pattern.strip_prefix('*') {
        url.ends_with(suffix)
    } else if let Some(prefix) = pattern.strip_suffix('*') {
        url.starts_with(prefix)
    } else {
        pattern == url
    }
}

fn rule_matches(rule: &InterceptionRule, url: &str, resource_type: &str) -> bool {
    if !rule.resource_types.is_empty()
        && !rule
            .resource_types
            .iter()
            .any(|rt| resource_type_to_cdp(rt) == resource_type)
    {
        return false;
    }
    if !url_pattern_matches(&rule.url_pattern, url) {
        return false;
    }
    true
}

pub struct CdpNetworkPort {
    session: CdpSession,
    rules: Arc<Mutex<Vec<(InterceptionHandle, InterceptionRule)>>>,
    listener_registered: AtomicBool,
}

impl CdpNetworkPort {
    pub fn new(session: CdpSession) -> Self {
        Self {
            session,
            rules: Arc::new(Mutex::new(Vec::new())),
            listener_registered: AtomicBool::new(false),
        }
    }

    fn ensure_listener(&self) {
        if !self.listener_registered.swap(true, Ordering::SeqCst) {
            let session = self.session.clone();
            let rules = self.rules.clone();
            self.session
                .on_event("Fetch.requestPaused", move |_method, params| {
                    let request_id = match params["requestId"].as_str() {
                        Some(id) => id.to_string(),
                        None => return,
                    };
                    let url = params["request"]["url"].as_str().unwrap_or("");
                    let resource_type = params["resourceType"].as_str().unwrap_or("");

                    let action = {
                        let r = rules.lock().unwrap_or_else(|e| e.into_inner());
                        r.iter()
                            .find(|(_, rule)| rule_matches(rule, url, resource_type))
                            .map(|(_, rule)| rule.action.clone())
                    };

                    match action {
                        Some(InterceptionAction::Block) => {
                            let _ = session.send::<Value>(
                                command::fetch::FAIL_REQUEST,
                                Some(command::fetch::fail_request(&request_id, "blockedByClient")),
                            );
                        }
                        Some(InterceptionAction::Continue) | None => {
                            let _ = session.send::<Value>(
                                command::fetch::CONTINUE_REQUEST,
                                Some(command::fetch::continue_request(&request_id)),
                            );
                        }
                        Some(InterceptionAction::Respond {
                            status,
                            ref headers,
                            ref body,
                        }) => {
                            let header_values: Vec<Value> = headers
                                .iter()
                                .map(|(k, v)| {
                                    command::fetch::response_header(k.as_str(), v.as_str())
                                })
                                .collect();
                            let body_b64 = base64::Engine::encode(
                                &base64::engine::general_purpose::STANDARD,
                                body,
                            );
                            let _ = session.send::<Value>(
                                command::fetch::FULFILL_REQUEST,
                                Some(command::fetch::fulfill_request(
                                    &request_id,
                                    status,
                                    header_values,
                                    &body_b64,
                                )),
                            );
                        }
                    }
                });
        }
    }

    fn sync_patterns(&self) -> BridgeResult<()> {
        let rules = self
            .rules
            .lock()
            .map_err(|e| BridgeError::Internal(format!("rules lock poisoned: {e}")))?;
        if rules.is_empty() {
            let _ = self.session.send::<Value>(command::fetch::DISABLE, None);
        } else {
            let mut patterns = Vec::new();
            for (_, rule) in rules.iter() {
                if rule.resource_types.is_empty() {
                    let url_pattern = rule_url_pattern(&rule.url_pattern);
                    patterns.push(command::fetch::request_pattern(url_pattern, None));
                } else {
                    for rt in &rule.resource_types {
                        let url_pattern = rule_url_pattern(&rule.url_pattern);
                        patterns.push(command::fetch::request_pattern(
                            url_pattern,
                            Some(resource_type_to_cdp(rt)),
                        ));
                    }
                }
            }
            self.session
                .send::<Value>(
                    command::fetch::ENABLE,
                    Some(command::fetch::enable(patterns)),
                )
                .map_err(map_error)?;
        }
        Ok(())
    }
}

impl NetworkPort for CdpNetworkPort {
    fn set_offline(&self, offline: bool) -> BridgeResult<()> {
        self.session
            .send::<Value>(command::network::ENABLE, None)
            .map_err(map_error)?;
        self.session
            .send::<Value>(
                command::network::SET_CACHE_DISABLED,
                Some(command::network::set_cache_disabled(offline)),
            )
            .map_err(map_error)?;
        Ok(())
    }

    fn set_conditions(&self, conditions: NetworkConditions) -> BridgeResult<()> {
        self.session
            .send::<Value>(command::network::ENABLE, None)
            .map_err(map_error)?;
        if conditions.offline {
            self.session
                .send::<Value>(
                    command::network::SET_BLOCKED_URLS,
                    Some(command::network::set_blocked_urls(vec!["*".into()])),
                )
                .map_err(map_error)?;
        }
        Ok(())
    }

    fn add_interception_rule(&self, rule: InterceptionRule) -> BridgeResult<InterceptionHandle> {
        let handle = InterceptionHandle::new();
        {
            let mut rules = self
                .rules
                .lock()
                .map_err(|e| BridgeError::Internal(format!("rules lock poisoned: {e}")))?;
            rules.push((handle, rule));
        }
        self.ensure_listener();
        self.sync_patterns()?;
        Ok(handle)
    }

    fn remove_interception_rule(&self, handle: &InterceptionHandle) -> BridgeResult<()> {
        let mut rules = self
            .rules
            .lock()
            .map_err(|e| BridgeError::Internal(format!("rules lock poisoned: {e}")))?;
        let len_before = rules.len();
        rules.retain(|(h, _)| h != handle);
        if rules.len() == len_before {
            return Err(BridgeError::InterceptionRuleNotFound);
        }
        drop(rules);
        self.sync_patterns()?;
        Ok(())
    }

    fn clear_cache(&self) -> BridgeResult<()> {
        self.session
            .send::<Value>(command::network::CLEAR_BROWSER_CACHE, None)
            .map_err(map_error)?;
        Ok(())
    }

    fn clear_cookies(&self) -> BridgeResult<()> {
        self.session
            .send::<Value>(command::network::CLEAR_BROWSER_COOKIES, None)
            .map_err(map_error)?;
        Ok(())
    }
}

// ─── InputPort ──────────────────────────────────────────────

pub struct CdpInputPort {
    session: CdpSession,
}

impl CdpInputPort {
    pub fn new(session: CdpSession) -> Self {
        Self { session }
    }

    fn point_for_element(&self, element: &dyn ElementPort) -> BridgeResult<Point> {
        element.click_point()
    }
}

impl InputPort for CdpInputPort {
    fn click(&self, element: &dyn ElementPort, options: ClickOptions) -> BridgeResult<()> {
        let point = self.point_for_element(element)?;
        let btn = match options.button {
            MouseButton::Left => "left",
            MouseButton::Right => "right",
            MouseButton::Middle => "middle",
            MouseButton::Back => "back",
            MouseButton::Forward => "forward",
        };
        let params =
            command::input::dispatch_mouse_click(point.x, point.y, btn, options.click_count);
        self.session
            .send::<Value>(command::input::DISPATCH_MOUSE_EVENT, Some(params))
            .map_err(map_error)?;
        Ok(())
    }

    fn dblclick(&self, element: &dyn ElementPort, options: ClickOptions) -> BridgeResult<()> {
        self.click(
            element,
            ClickOptions {
                click_count: 2,
                ..options
            },
        )
    }

    fn fill(&self, element: &dyn ElementPort, text: &str) -> BridgeResult<()> {
        element.focus()?;
        let _: Value = self
            .session
            .send(
                command::runtime::EVALUATE,
                Some(serde_json::json!({
                    "expression": "document.activeElement ? document.activeElement.value = '' : ''",
                    "returnByValue": true,
                })),
            )
            .map_err(map_error)?;
        self.session
            .send::<Value>(
                command::input::INSERT_TEXT,
                Some(command::input::insert_text(text)),
            )
            .map_err(map_error)?;
        Ok(())
    }

    fn type_text(
        &self,
        element: &dyn ElementPort,
        text: &str,
        delay: Duration,
    ) -> BridgeResult<()> {
        element.focus()?;
        for ch in text.chars() {
            let down = command::input::key_down(&ch.to_string());
            self.session
                .send::<Value>(command::input::DISPATCH_KEY_EVENT, Some(down))
                .map_err(map_error)?;
            let char_evt = command::input::char_event(ch);
            self.session
                .send::<Value>(command::input::DISPATCH_KEY_EVENT, Some(char_evt))
                .map_err(map_error)?;
            let up = command::input::key_up(&ch.to_string());
            self.session
                .send::<Value>(command::input::DISPATCH_KEY_EVENT, Some(up))
                .map_err(map_error)?;
            if !delay.is_zero() {
                std::thread::sleep(delay);
            }
        }
        Ok(())
    }

    fn press_key(&self, key: &str) -> BridgeResult<()> {
        let down = command::input::key_down(key);
        self.session
            .send::<Value>(command::input::DISPATCH_KEY_EVENT, Some(down))
            .map_err(map_error)?;
        let up = command::input::key_up(key);
        self.session
            .send::<Value>(command::input::DISPATCH_KEY_EVENT, Some(up))
            .map_err(map_error)?;
        Ok(())
    }

    fn hover(&self, element: &dyn ElementPort) -> BridgeResult<()> {
        let point = self.point_for_element(element)?;
        let _: Value = self
            .session
            .send(
                command::input::DISPATCH_MOUSE_EVENT,
                Some(serde_json::json!({
                    "type": "mouseMoved",
                    "x": point.x,
                    "y": point.y,
                })),
            )
            .map_err(map_error)?;
        Ok(())
    }

    fn scroll(&self, delta_x: f64, delta_y: f64) -> BridgeResult<()> {
        let _: Value = self
            .session
            .send(
                command::input::DISPATCH_MOUSE_EVENT,
                Some(serde_json::json!({
                    "type": "mouseWheel",
                    "deltaX": delta_x,
                    "deltaY": delta_y,
                })),
            )
            .map_err(map_error)?;
        Ok(())
    }

    fn drag_and_drop(
        &self,
        source: &dyn ElementPort,
        target: &dyn ElementPort,
    ) -> BridgeResult<()> {
        let src_point = self.point_for_element(source)?;
        let tgt_point = self.point_for_element(target)?;
        let press = command::input::dispatch_mouse_click(src_point.x, src_point.y, "left", 1);
        self.session
            .send::<Value>(command::input::DISPATCH_MOUSE_EVENT, Some(press))
            .map_err(map_error)?;
        let _: Value = self
            .session
            .send(
                command::input::DISPATCH_MOUSE_EVENT,
                Some(serde_json::json!({
                    "type": "mouseMoved",
                    "x": tgt_point.x,
                    "y": tgt_point.y,
                })),
            )
            .map_err(map_error)?;
        let _: Value = self
            .session
            .send(
                command::input::DISPATCH_MOUSE_EVENT,
                Some(serde_json::json!({
                    "type": "mouseReleased",
                    "x": tgt_point.x,
                    "y": tgt_point.y,
                    "button": "left",
                })),
            )
            .map_err(map_error)?;
        Ok(())
    }

    fn upload_file(&self, element: &dyn ElementPort, paths: &[PathBuf]) -> BridgeResult<()> {
        let node_id = element.id().get();
        let files: Vec<String> = paths
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();
        let params = command::dom::set_file_input_files(node_id, &files);
        self.session
            .send::<Value>(command::dom::SET_FILE_INPUT_FILES, Some(params))
            .map_err(map_error)?;
        Ok(())
    }

    fn select_option(&self, element: &dyn ElementPort, values: &[&str]) -> BridgeResult<()> {
        element.focus()?;
        let values_json =
            serde_json::to_string(&values.iter().map(|v| v.to_string()).collect::<Vec<_>>())
                .map_err(|e| BridgeError::Internal(format!("Serialization: {e}")))?;
        let expression = format!(
            r#"(function() {{
                const el = document.activeElement;
                if (!el || el.tagName !== 'SELECT') return false;
                const vals = {};
                if (el.multiple) {{
                    for (const opt of el.options) opt.selected = vals.includes(opt.value);
                }} else if (vals.length > 0) {{
                    el.value = vals[0];
                }}
                el.dispatchEvent(new Event('change', {{ bubbles: true }}));
                return true;
            }})()"#,
            values_json
        );
        let _: Value = self
            .session
            .send(
                command::runtime::EVALUATE,
                Some(serde_json::json!({
                    "expression": expression,
                    "returnByValue": true,
                    "awaitPromise": true,
                })),
            )
            .map_err(map_error)?;
        Ok(())
    }

    fn check(&self, element: &dyn ElementPort) -> BridgeResult<()> {
        element.focus()?;
        self.click(element, ClickOptions::default())
    }

    fn uncheck(&self, element: &dyn ElementPort) -> BridgeResult<()> {
        element.focus()?;
        self.click(element, ClickOptions::default())
    }
}

// ─── StoragePort ────────────────────────────────────────────

pub struct CdpStoragePort {
    session: CdpSession,
}

impl CdpStoragePort {
    pub fn new(session: CdpSession) -> Self {
        Self { session }
    }
}

impl StoragePort for CdpStoragePort {
    fn cookies(&self) -> BridgeResult<Vec<Cookie>> {
        let result: Value = self
            .session
            .send(command::storage::GET_COOKIES, None)
            .map_err(map_error)?;
        let cookies = result["cookies"].as_array().cloned().unwrap_or_default();
        cookies
            .iter()
            .map(|c| {
                Ok(Cookie {
                    name: c["name"].as_str().unwrap_or("").to_string(),
                    value: c["value"].as_str().unwrap_or("").to_string(),
                    domain: c["domain"].as_str().unwrap_or("").to_string(),
                    path: c["path"].as_str().unwrap_or("").to_string(),
                    secure: c["secure"].as_bool().unwrap_or(false),
                    http_only: c["httpOnly"].as_bool().unwrap_or(false),
                    same_site: match c["sameSite"].as_str() {
                        Some("Strict") => SameSitePolicy::Strict,
                        Some("Lax") => SameSitePolicy::Lax,
                        _ => SameSitePolicy::None,
                    },
                    expires: c["expires"].as_f64().map(|ts| {
                        chrono::DateTime::from_timestamp(ts as i64, 0).unwrap_or_default()
                    }),
                })
            })
            .collect()
    }

    fn set_cookies(&self, cookies: &[Cookie]) -> BridgeResult<()> {
        let params = serde_json::json!({
            "cookies": cookies.iter().map(|c| serde_json::json!({
                "name": c.name,
                "value": c.value,
                "domain": c.domain,
                "path": c.path,
                "secure": c.secure,
                "httpOnly": c.http_only,
                "sameSite": match c.same_site {
                    SameSitePolicy::Strict => "Strict",
                    SameSitePolicy::Lax => "Lax",
                    SameSitePolicy::None => "None",
                },
            })).collect::<Vec<_>>(),
        });
        self.session
            .send::<Value>(command::storage::SET_COOKIES, Some(params))
            .map_err(map_error)?;
        Ok(())
    }

    fn delete_cookie(&self, name: &str, _url: &str) -> BridgeResult<()> {
        let _: Value = self.session.send(command::runtime::EVALUATE, Some(serde_json::json!({
            "expression": format!("document.cookie='{name}=; expires=Thu, 01 Jan 1970 00:00:00 UTC; path=/'"),
            "returnByValue": true,
        }))).map_err(map_error)?;
        Ok(())
    }

    fn delete_all_cookies(&self) -> BridgeResult<()> {
        self.session
            .send::<Value>(command::storage::CLEAR_COOKIES, None)
            .map_err(map_error)?;
        Ok(())
    }

    fn local_storage(&self) -> BridgeResult<Vec<StorageEntry>> {
        let result: Value = self
            .session
            .send(
                command::runtime::EVALUATE,
                Some(serde_json::json!({
                    "expression": "JSON.stringify(Object.entries(localStorage))",
                    "returnByValue": true,
                })),
            )
            .map_err(map_error)?;
        let entries_str = result["result"]["value"].as_str().unwrap_or("[]");
        let entries: Vec<Vec<String>> = serde_json::from_str(entries_str).unwrap_or_default();
        Ok(entries
            .iter()
            .filter_map(|e| {
                if e.len() >= 2 {
                    Some(StorageEntry {
                        key: e[0].clone(),
                        value: e[1].clone(),
                    })
                } else {
                    None
                }
            })
            .collect())
    }

    fn set_local_storage(&self, entries: &[StorageEntry]) -> BridgeResult<()> {
        for entry in entries {
            let _: Value = self.session.send(command::runtime::EVALUATE, Some(serde_json::json!({
                "expression": format!("localStorage.setItem('{}', '{}')", entry.key.replace('\'', "\\'"), entry.value.replace('\'', "\\'")),
            }))).map_err(map_error)?;
        }
        Ok(())
    }

    fn clear_local_storage(&self) -> BridgeResult<()> {
        let _: Value = self
            .session
            .send(
                command::runtime::EVALUATE,
                Some(serde_json::json!({
                    "expression": "localStorage.clear()",
                })),
            )
            .map_err(map_error)?;
        Ok(())
    }

    fn session_storage(&self) -> BridgeResult<Vec<StorageEntry>> {
        let result: Value = self
            .session
            .send(
                command::runtime::EVALUATE,
                Some(serde_json::json!({
                    "expression": "JSON.stringify(Object.entries(sessionStorage))",
                    "returnByValue": true,
                })),
            )
            .map_err(map_error)?;
        let entries_str = result["result"]["value"].as_str().unwrap_or("[]");
        let entries: Vec<Vec<String>> = serde_json::from_str(entries_str).unwrap_or_default();
        Ok(entries
            .iter()
            .filter_map(|e| {
                if e.len() >= 2 {
                    Some(StorageEntry {
                        key: e[0].clone(),
                        value: e[1].clone(),
                    })
                } else {
                    None
                }
            })
            .collect())
    }

    fn clear_session_storage(&self) -> BridgeResult<()> {
        let _: Value = self
            .session
            .send(
                command::runtime::EVALUATE,
                Some(serde_json::json!({
                    "expression": "sessionStorage.clear()",
                })),
            )
            .map_err(map_error)?;
        Ok(())
    }
}

// ─── DialogPort ─────────────────────────────────────────────

pub struct CdpDialogPort {
    session: CdpSession,
    page_id: PageId,
    current: Arc<Mutex<Option<DialogInfo>>>,
    initialized: AtomicBool,
}

impl CdpDialogPort {
    pub fn new(session: CdpSession, page_id: PageId) -> Self {
        Self {
            session,
            page_id,
            current: Arc::new(Mutex::new(None)),
            initialized: AtomicBool::new(false),
        }
    }

    fn ensure_listening(&self) {
        if !self.initialized.swap(true, Ordering::SeqCst) {
            let queue = self.current.clone();
            let page_id = self.page_id;
            self.session
                .on_event("Page.javascriptDialogOpening", move |_method, params| {
                    let dialog_type = match params["type"].as_str() {
                        Some("alert") => DialogType::Alert,
                        Some("confirm") => DialogType::Confirm,
                        Some("prompt") => DialogType::Prompt,
                        Some("beforeunload") => DialogType::BeforeUnload,
                        _ => return,
                    };
                    let message = params["message"].as_str().unwrap_or("").to_string();
                    let default_prompt = params["defaultPrompt"].as_str();
                    let info = DialogInfo {
                        dialog_type,
                        message,
                        default_value: default_prompt.map(|s| s.to_string()),
                        page_id,
                    };
                    if let Ok(mut cur) = queue.lock() {
                        *cur = Some(info);
                    }
                });
        }
    }
}

impl DialogPort for CdpDialogPort {
    fn next(&self) -> BridgeResult<Option<DialogInfo>> {
        self.ensure_listening();
        let mut cur = self
            .current
            .lock()
            .map_err(|e| BridgeError::Internal(format!("dialog port mutex poisoned: {e}")))?;
        Ok(cur.take())
    }

    fn accept(&self, prompt_text: Option<&str>) -> BridgeResult<()> {
        let params = command::page::handle_javascript_dialog(true, prompt_text);
        self.session
            .send::<Value>(command::page::HANDLE_JAVASCRIPT_DIALOG, Some(params))
            .map_err(map_error)?;
        Ok(())
    }

    fn dismiss(&self) -> BridgeResult<()> {
        let params = command::page::handle_javascript_dialog(false, None);
        self.session
            .send::<Value>(command::page::HANDLE_JAVASCRIPT_DIALOG, Some(params))
            .map_err(map_error)?;
        Ok(())
    }
}

// ─── DownloadPort ───────────────────────────────────────────

pub struct CdpDownloadPort {
    session: CdpSession,
    inner: Arc<DownloadInner>,
}

struct DownloadInner {
    downloads: Mutex<Vec<DownloadInfo>>,
    download_path: Mutex<PathBuf>,
    listener_registered: AtomicBool,
}

impl CdpDownloadPort {
    pub fn new(session: CdpSession) -> Self {
        Self {
            session,
            inner: Arc::new(DownloadInner {
                downloads: Mutex::new(Vec::new()),
                download_path: Mutex::new(PathBuf::from(".")),
                listener_registered: AtomicBool::new(false),
            }),
        }
    }

    fn ensure_listener(&self) {
        if !self.inner.listener_registered.swap(true, Ordering::SeqCst) {
            let inner = self.inner.clone();
            self.session
                .on_event("Browser.downloadWillBegin", move |_method, params| {
                    let url = params["url"].as_str().unwrap_or("").to_string();
                    let suggested_filename = params["suggestedFilename"]
                        .as_str()
                        .unwrap_or("download")
                        .to_string();
                    let download_id = params["guid"].as_str().unwrap_or("").to_string();
                    let total_bytes = params["totalBytes"].as_f64().unwrap_or(0.0);
                    let page_id = PageId::new();

                    let info = DownloadInfo {
                        url,
                        suggested_filename,
                        file_path: None,
                        mime_type: String::new(),
                        total_bytes,
                        received_bytes: 0.0,
                        state: DownloadState::InProgress,
                        page_id,
                        download_id,
                    };

                    if let Ok(mut d) = inner.downloads.lock() {
                        d.push(info);
                    }
                });

            let inner2 = self.inner.clone();
            self.session
                .on_event("Browser.downloadProgress", move |_method, params| {
                    let download_id = params["guid"].as_str().unwrap_or("");
                    let state = match params["state"].as_str() {
                        Some("inProgress") => DownloadState::InProgress,
                        Some("completed") => DownloadState::Completed,
                        Some("canceled") => DownloadState::Cancelled,
                        _ => return,
                    };
                    let received_bytes = params["receivedBytes"].as_f64().unwrap_or(0.0);
                    let total_bytes = params["totalBytes"].as_f64().unwrap_or(0.0);

                    if let Ok(mut d) = inner2.downloads.lock() {
                        if let Some(info) = d.iter_mut().find(|i| i.download_id == download_id) {
                            info.state = state;
                            info.received_bytes = received_bytes;
                            info.total_bytes = total_bytes;
                        }
                    }
                });
        }
    }
}

impl DownloadPort for CdpDownloadPort {
    fn downloads(&self) -> Vec<DownloadInfo> {
        self.ensure_listener();
        self.inner
            .downloads
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn cancel_download(&self, id: &str) -> BridgeResult<()> {
        self.ensure_listener();
        let mut downloads = self
            .inner
            .downloads
            .lock()
            .map_err(|e| BridgeError::Internal(format!("downloads lock poisoned: {e}")))?;
        if let Some(info) = downloads.iter_mut().find(|i| i.download_id == id) {
            if info.state == DownloadState::InProgress {
                info.state = DownloadState::Cancelled;
            }
            Ok(())
        } else {
            Err(BridgeError::DownloadNotFound {
                download_id: id.to_string(),
            })
        }
    }

    fn set_download_path(&self, path: PathBuf) -> BridgeResult<()> {
        let path_str = path.to_str().unwrap_or(".").to_string();
        {
            let mut dp =
                self.inner.download_path.lock().map_err(|e| {
                    BridgeError::Internal(format!("download_path lock poisoned: {e}"))
                })?;
            *dp = path.clone();
        }
        let result = self.session.send::<Value>(
            command::browser::SET_DOWNLOAD_BEHAVIOR,
            Some(command::browser::set_download_behavior(
                "allow",
                Some(&path_str),
            )),
        );
        match result {
            Ok(_) => Ok(()),
            Err(_) => {
                // Browser.setDownloadBehavior may not be available in all
                // CDP implementations; fall back to just storing the path.
                Ok(())
            }
        }
    }

    fn download_path(&self) -> PathBuf {
        self.inner
            .download_path
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn wait_for_completion(&self, timeout: Duration) -> BridgeResult<Vec<DownloadInfo>> {
        self.ensure_listener();
        let deadline = std::time::Instant::now() + timeout;
        loop {
            {
                let downloads =
                    self.inner.downloads.lock().map_err(|e| {
                        BridgeError::Internal(format!("downloads lock poisoned: {e}"))
                    })?;
                let in_progress = downloads
                    .iter()
                    .any(|d| d.state == DownloadState::InProgress);
                if !in_progress {
                    return Ok(downloads.clone());
                }
            }
            if std::time::Instant::now() > deadline {
                return Ok(self
                    .inner
                    .downloads
                    .lock()
                    .map_err(|e| BridgeError::Internal(format!("downloads lock poisoned: {e}")))?
                    .clone());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

// ─── Helpers ────────────────────────────────────────────────

fn paper_size(format: &PdfPaperFormat) -> (f64, f64) {
    match format {
        PdfPaperFormat::Letter => (8.5, 11.0),
        PdfPaperFormat::Legal => (8.5, 14.0),
        PdfPaperFormat::Tabloid => (11.0, 17.0),
        PdfPaperFormat::Ledger => (17.0, 11.0),
        PdfPaperFormat::A0 => (33.1, 46.8),
        PdfPaperFormat::A1 => (23.4, 33.1),
        PdfPaperFormat::A2 => (16.5, 23.4),
        PdfPaperFormat::A3 => (11.7, 16.5),
        PdfPaperFormat::A4 => (8.27, 11.7),
        PdfPaperFormat::A5 => (5.83, 8.27),
        PdfPaperFormat::A6 => (4.13, 5.83),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::CdpConfig;
    use crate::connection::CdpConnection;
    use crate::transport::mock::MockTransport;
    use std::sync::Arc;

    fn make_transport() -> MockTransport {
        MockTransport::new()
    }

    fn make_connection_with(transport: MockTransport) -> (Arc<CdpConnection>, MockTransport) {
        let t2 = transport.clone();
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        (Arc::new(conn), t2)
    }

    fn make_connection() -> (Arc<CdpConnection>, MockTransport) {
        make_connection_with(MockTransport::new())
    }

    fn make_session() -> (CdpSession, MockTransport) {
        let (conn, transport) = make_connection();
        let session = CdpSession::new(conn, "test-target", "test-session");
        (session, transport)
    }

    #[test]
    fn test_session_port_create_page() {
        let transport = MockTransport::with_responses(vec![
            r#"{"id":1,"result":{"targetId":"new-target"}}"#.into(),
            r#"{"id":2,"result":{"sessionId":"new-session"}}"#.into(),
            r#"{"id":3,"result":{}}"#.into(),
            r#"{"id":4,"result":{}}"#.into(),
            r#"{"id":5,"result":{}}"#.into(),
            r#"{"id":6,"result":{}}"#.into(),
        ]);
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "parent-target", "parent-session");

        let session_port = CdpSessionPort::new(session, SessionConfig::default());
        let page = session_port.create_page().unwrap();
        assert!(page.url().is_empty());
    }

    #[test]
    fn test_session_port_pages_after_create() {
        let transport = MockTransport::with_responses(vec![
            r#"{"id":1,"result":{"targetId":"t1"}}"#.into(),
            r#"{"id":2,"result":{"sessionId":"s1"}}"#.into(),
            r#"{"id":3,"result":{}}"#.into(),
            r#"{"id":4,"result":{}}"#.into(),
            r#"{"id":5,"result":{}}"#.into(),
            r#"{"id":6,"result":{}}"#.into(),
        ]);
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "parent-target", "parent-session");
        let session_port = CdpSessionPort::new(session, SessionConfig::default());
        assert!(session_port.pages().is_empty());
        let page = session_port.create_page().unwrap();
        let page_id = page.id();
        assert_eq!(session_port.pages().len(), 1);
        assert_eq!(session_port.pages()[0].id(), page_id);
    }

    #[test]
    fn test_session_port_close_page() {
        let transport = MockTransport::with_responses(vec![
            r#"{"id":1,"result":{"targetId":"t1"}}"#.into(),
            r#"{"id":2,"result":{"sessionId":"s1"}}"#.into(),
            r#"{"id":3,"result":{}}"#.into(),
            r#"{"id":4,"result":{}}"#.into(),
            r#"{"id":5,"result":{}}"#.into(),
            r#"{"id":6,"result":{}}"#.into(),
            r#"{"id":7,"result":{}}"#.into(),
        ]);
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "parent-target", "parent-session");
        let session_port = CdpSessionPort::new(session, SessionConfig::default());
        let page = session_port.create_page().unwrap();
        let page_id = page.id();
        assert_eq!(session_port.pages().len(), 1);
        session_port.close_page(&page_id).unwrap();
        assert!(session_port.pages().is_empty());
    }

    #[test]
    fn test_session_port_close_page_not_found() {
        let (session, _) = make_session();
        let session_port = CdpSessionPort::new(session, SessionConfig::default());
        let unknown_id = PageId::new();
        match session_port.close_page(&unknown_id) {
            Err(BridgeError::PageNotFound(id)) => assert_eq!(id, unknown_id),
            _ => panic!("Expected PageNotFound"),
        }
    }

    #[test]
    fn test_session_port_activate_page() {
        let transport = MockTransport::with_responses(vec![
            r#"{"id":1,"result":{"targetId":"t1"}}"#.into(),
            r#"{"id":2,"result":{"sessionId":"s1"}}"#.into(),
            r#"{"id":3,"result":{}}"#.into(),
            r#"{"id":4,"result":{}}"#.into(),
            r#"{"id":5,"result":{}}"#.into(),
            r#"{"id":6,"result":{}}"#.into(),
            r#"{"id":7,"result":{}}"#.into(),
        ]);
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "parent-target", "parent-session");
        let session_port = CdpSessionPort::new(session, SessionConfig::default());
        let page = session_port.create_page().unwrap();
        let page_id = page.id();
        session_port.activate_page(&page_id).unwrap();
        // Page should still be in the map after activate
        assert_eq!(session_port.pages().len(), 1);
    }

    #[test]
    fn test_session_port_activate_page_not_found() {
        let (session, _) = make_session();
        let session_port = CdpSessionPort::new(session, SessionConfig::default());
        let unknown_id = PageId::new();
        match session_port.activate_page(&unknown_id) {
            Err(BridgeError::PageNotFound(id)) => assert_eq!(id, unknown_id),
            _ => panic!("Expected PageNotFound"),
        }
    }

    #[test]
    fn test_page_port_url_title() {
        let (session, _) = make_session();
        let page = CdpPage::new(session, "target".into());
        assert!(page.url().is_empty());
        assert!(page.title().is_empty());
    }

    #[test]
    fn test_page_port_navigate() {
        let transport = MockTransport::with_responses(vec![
            r#"{"id":1,"result":{}}"#.into(),
            r#"{"id":2,"result":{"result":{"value":"complete","type":"string"}}}"#.into(),
            r#"{"id":3,"result":{"result":{"value":"https://example.com/","type":"string"}}}"#
                .into(),
            r#"{"id":4,"result":{"result":{"value":"Example","type":"string"}}}"#.into(),
        ]);
        let (conn, _) = make_connection_with(transport);
        let session = CdpSession::new(conn, "t", "s");
        let page = CdpPage::new(session, "target".into());
        let result = page.navigate("https://example.com").unwrap();
        assert_eq!(result.status, NavigationStatus::Finished);
        assert_eq!(result.url, "https://example.com/");
        assert_eq!(page.url(), "https://example.com/");
        assert_eq!(page.title(), "Example");
    }

    #[test]
    fn test_page_port_go_back_go_forward() {
        let transport = MockTransport::with_responses(vec![
            r#"{"id":1,"result":{}}"#.into(),
            r#"{"id":2,"result":{"result":{"value":"complete","type":"string"}}}"#.into(),
            r#"{"id":3,"result":{"result":{"value":"https://back.example.com/","type":"string"}}}"#.into(),
            r#"{"id":4,"result":{"result":{"value":"Back","type":"string"}}}"#.into(),
            r#"{"id":5,"result":{}}"#.into(),
            r#"{"id":6,"result":{"result":{"value":"complete","type":"string"}}}"#.into(),
            r#"{"id":7,"result":{"result":{"value":"https://forward.example.com/","type":"string"}}}"#.into(),
            r#"{"id":8,"result":{"result":{"value":"Forward","type":"string"}}}"#.into(),
        ]);
        let (conn, _) = make_connection_with(transport);
        let session = CdpSession::new(conn, "t", "s");
        let page = CdpPage::new(session, "target".into());
        assert!(page.go_back().is_ok());
        assert!(page.go_forward().is_ok());
    }

    #[test]
    fn test_page_port_evaluate() {
        let transport = MockTransport::with_responses(vec![
            r#"{"id":1,"result":{"result":{"value":42,"type":"number"}}}"#.into(),
        ]);
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "t", "s");
        let page = CdpPage::new(session, "target".into());
        let result = page.evaluate("1+1", None).unwrap();
        assert_eq!(result.value, 42);
    }

    #[test]
    fn test_page_port_content() {
        let transport = MockTransport::with_responses(vec![
            r#"{"id":1,"result":{"result":{"value":"<html>test</html>","type":"string"}}}"#.into(),
        ]);
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "t", "s");
        let page = CdpPage::new(session, "target".into());
        let content = page.content().unwrap();
        assert_eq!(content, "<html>test</html>");
    }

    #[test]
    fn test_page_port_set_content() {
        let transport = MockTransport::with_responses(vec![r#"{"id":1,"result":{}}"#.into()]);
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "t", "s");
        let page = CdpPage::new(session, "target".into());
        assert!(page.set_content("<html>test</html>").is_ok());
    }

    #[test]
    fn test_page_port_frames() {
        let (session, _) = make_session();
        let page = CdpPage::new(session, "target".into());
        let frames = page.frames();
        assert!(frames.is_empty());
    }

    #[test]
    fn test_page_port_wait_for_selector_found() {
        let transport = MockTransport::with_responses(vec![
            // DOM.getDocument returns root node
            r#"{"id":1,"result":{"root":{"nodeId":1}}}"#.into(),
            // DOM.querySelector returns matching node
            r#"{"id":2,"result":{"nodeId":5}}"#.into(),
        ]);
        let (conn, _) = make_connection_with(transport);
        let session = CdpSession::new(conn, "t", "s");
        let page = CdpPage::new(session, "target".into());
        page.wait_for(WaitCondition::Selector("div".into(), ElementState::Visible))
            .unwrap();
    }

    #[test]
    fn test_page_port_wait_for_url_match() {
        let transport = MockTransport::with_responses(vec![
            r#"{"id":1,"result":{"result":{"value":"https://example.com/page","type":"string"}}}"#
                .into(),
        ]);
        let (conn, _) = make_connection_with(transport);
        let session = CdpSession::new(conn, "t", "s");
        let page = CdpPage::new(session, "target".into());
        page.wait_for(WaitCondition::Url("example.com".into()))
            .unwrap();
    }

    #[test]
    fn test_page_port_wait_for_title_match() {
        let transport = MockTransport::with_responses(vec![
            r#"{"id":1,"result":{"result":{"value":"My Page Title","type":"string"}}}"#.into(),
        ]);
        let (conn, _) = make_connection_with(transport);
        let session = CdpSession::new(conn, "t", "s");
        let page = CdpPage::new(session, "target".into());
        page.wait_for(WaitCondition::Title("Page".into())).unwrap();
    }

    #[test]
    fn test_page_port_wait_for_function_truthy() {
        let transport = MockTransport::with_responses(vec![
            r#"{"id":1,"result":{"result":{"value":true,"type":"boolean"}}}"#.into(),
        ]);
        let (conn, _) = make_connection_with(transport);
        let session = CdpSession::new(conn, "t", "s");
        let page = CdpPage::new(session, "target".into());
        page.wait_for(WaitCondition::Function("() => true".into()))
            .unwrap();
    }

    #[test]
    fn test_page_port_wait_for_navigation() {
        let transport = MockTransport::with_responses(vec![
            r#"{"id":1,"result":{"result":{"value":"complete","type":"string"}}}"#.into(),
        ]);
        let (conn, _) = make_connection_with(transport);
        let session = CdpSession::new(conn, "t", "s");
        let page = CdpPage::new(session, "target".into());
        page.wait_for(WaitCondition::Navigation(Duration::from_secs(5)))
            .unwrap();
    }

    #[test]
    fn test_page_port_evaluate_handle() {
        let transport = MockTransport::with_responses(vec![
            // Runtime.evaluate returns an object with objectId
            r#"{"id":1,"result":{"result":{"type":"object","subtype":"node","objectId":"obj-1","className":"HTMLDivElement"}}}"#.into(),
            // DOM.getDocument returns root node (populates DOM tree)
            r#"{"id":2,"result":{"root":{"nodeId":1}}}"#.into(),
            // DOM.requestNode returns nodeId
            r#"{"id":3,"result":{"nodeId":42}}"#.into(),
        ]);
        let (conn, _) = make_connection_with(transport);
        let session = CdpSession::new(conn, "t", "s");
        let page = CdpPage::new(session, "target".into());
        let element = page
            .evaluate_handle("document.querySelector('div')", None)
            .unwrap();
        assert_eq!(element.id(), ElementId::new(42));
    }

    #[test]
    fn test_page_port_evaluate_handle_no_object_errors() {
        let transport = MockTransport::with_responses(vec![
            // Runtime.evaluate returns a value without objectId (not a DOM node)
            r#"{"id":1,"result":{"result":{"type":"string","value":"hello"}}}"#.into(),
        ]);
        let (conn, _) = make_connection_with(transport);
        let session = CdpSession::new(conn, "t", "s");
        let page = CdpPage::new(session, "target".into());
        match page.evaluate_handle("'hello'", None) {
            Err(BridgeError::Internal(_)) => {}
            _ => panic!("Expected Internal error for non-object result"),
        }
    }

    #[test]
    fn test_frame_port_methods() {
        let (session, _) = make_session();
        let frame = CdpFrame::new(session);
        assert!(frame.child_frames().is_empty());
        assert!(frame.parent_id().is_none());
    }

    #[test]
    fn test_element_port_id() {
        let (session, _) = make_session();
        let element = CdpElement::new(session, 42);
        assert_eq!(element.id(), ElementId::new(42));
    }

    #[test]
    fn test_element_is_visible_true() {
        let transport = MockTransport::with_responses(vec![
            r#"{"id":1,"result":{"object":{"objectId":"o1"}}}"#.into(),
            r#"{"id":2,"result":{"result":{"value":true}}}"#.into(),
        ]);
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "t", "s");
        let element = CdpElement::new(session, 1);
        assert!(element.is_visible().unwrap());
    }

    #[test]
    fn test_element_is_visible_false() {
        let transport = MockTransport::with_responses(vec![
            r#"{"id":1,"result":{"object":{"objectId":"o1"}}}"#.into(),
            r#"{"id":2,"result":{"result":{"value":false}}}"#.into(),
        ]);
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "t", "s");
        let element = CdpElement::new(session, 1);
        assert!(!element.is_visible().unwrap());
    }

    #[test]
    fn test_element_is_enabled_true() {
        let transport = MockTransport::with_responses(vec![
            r#"{"id":1,"result":{"object":{"objectId":"o1"}}}"#.into(),
            r#"{"id":2,"result":{"result":{"value":true}}}"#.into(),
        ]);
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "t", "s");
        let element = CdpElement::new(session, 1);
        assert!(element.is_enabled().unwrap());
    }

    #[test]
    fn test_element_is_enabled_false() {
        let transport = MockTransport::with_responses(vec![
            r#"{"id":1,"result":{"object":{"objectId":"o1"}}}"#.into(),
            r#"{"id":2,"result":{"result":{"value":false}}}"#.into(),
        ]);
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "t", "s");
        let element = CdpElement::new(session, 1);
        assert!(!element.is_enabled().unwrap());
    }

    #[test]
    fn test_element_is_checked_true() {
        let transport = MockTransport::with_responses(vec![
            r#"{"id":1,"result":{"object":{"objectId":"o1"}}}"#.into(),
            r#"{"id":2,"result":{"result":{"value":true}}}"#.into(),
        ]);
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "t", "s");
        let element = CdpElement::new(session, 1);
        assert!(element.is_checked().unwrap());
    }

    #[test]
    fn test_element_is_checked_false() {
        let transport = MockTransport::with_responses(vec![
            r#"{"id":1,"result":{"object":{"objectId":"o1"}}}"#.into(),
            r#"{"id":2,"result":{"result":{"value":false}}}"#.into(),
        ]);
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "t", "s");
        let element = CdpElement::new(session, 1);
        assert!(!element.is_checked().unwrap());
    }

    #[test]
    fn test_element_is_selected_true() {
        let transport = MockTransport::with_responses(vec![
            r#"{"id":1,"result":{"object":{"objectId":"o1"}}}"#.into(),
            r#"{"id":2,"result":{"result":{"value":true}}}"#.into(),
        ]);
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "t", "s");
        let element = CdpElement::new(session, 1);
        assert!(element.is_selected().unwrap());
    }

    #[test]
    fn test_element_is_selected_false() {
        let transport = MockTransport::with_responses(vec![
            r#"{"id":1,"result":{"object":{"objectId":"o1"}}}"#.into(),
            r#"{"id":2,"result":{"result":{"value":false}}}"#.into(),
        ]);
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "t", "s");
        let element = CdpElement::new(session, 1);
        assert!(!element.is_selected().unwrap());
    }

    #[test]
    fn test_element_is_stable_true() {
        let transport = MockTransport::with_responses(vec![
            r#"{"id":1,"result":{"model":{"content":[[0,0],[100,0],[100,100],[0,100]]}}}"#.into(),
            r#"{"id":2,"result":{"model":{"content":[[0,0],[100,0],[100,100],[0,100]]}}}"#.into(),
        ]);
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "t", "s");
        let element = CdpElement::new(session, 1);
        assert!(element.is_stable().unwrap());
    }

    #[test]
    fn test_element_is_stable_false_moved() {
        let transport = MockTransport::with_responses(vec![
            r#"{"id":1,"result":{"model":{"content":[[0,0],[100,0],[100,100],[0,100]]}}}"#.into(),
            r#"{"id":2,"result":{"model":{"content":[[10,10],[110,10],[110,110],[10,110]]}}}"#
                .into(),
        ]);
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "t", "s");
        let element = CdpElement::new(session, 1);
        assert!(!element.is_stable().unwrap());
    }

    #[test]
    fn test_locator_engine_query() {
        let transport = MockTransport::with_responses(vec![
            r#"{"id":1,"result":{"nodeId":10}}"#.into(),
            r#"{"id":2,"result":{"nodeIds":[20,30]}}"#.into(),
        ]);
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "t", "s");
        let engine = CdpLocatorEngine::new(session);
        let result = engine.query_selector(".my-class").unwrap();
        assert!(result.is_some());
        let results = engine.query_selector_all("div").unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_resource_type_to_cdp() {
        assert_eq!(resource_type_to_cdp(&ResourceType::Document), "Document");
        assert_eq!(
            resource_type_to_cdp(&ResourceType::Stylesheet),
            "Stylesheet"
        );
        assert_eq!(resource_type_to_cdp(&ResourceType::Image), "Image");
        assert_eq!(resource_type_to_cdp(&ResourceType::Script), "Script");
        assert_eq!(resource_type_to_cdp(&ResourceType::Xhr), "XHR");
        assert_eq!(resource_type_to_cdp(&ResourceType::Fetch), "Fetch");
        assert_eq!(resource_type_to_cdp(&ResourceType::WebSocket), "WebSocket");
        assert_eq!(resource_type_to_cdp(&ResourceType::Other), "Other");
    }

    #[test]
    fn test_url_pattern_matches() {
        assert!(url_pattern_matches("", "https://example.com"));
        assert!(url_pattern_matches("*", "https://example.com"));
        assert!(url_pattern_matches("*.js", "https://example.com/app.js"));
        assert!(url_pattern_matches(
            "https://example.com/*",
            "https://example.com/index.html"
        ));
        assert!(!url_pattern_matches(
            "*.js",
            "https://example.com/style.css"
        ));
        assert!(url_pattern_matches(
            "https://example.com/index.html",
            "https://example.com/index.html"
        ));
        assert!(!url_pattern_matches(
            "https://example.com/index.html",
            "https://example.com/other.html"
        ));
    }

    #[test]
    fn test_rule_matches() {
        let rule = InterceptionRule {
            url_pattern: "*.js".into(),
            resource_types: vec![ResourceType::Script],
            action: InterceptionAction::Block,
        };
        assert!(rule_matches(&rule, "https://example.com/app.js", "Script"));
        assert!(!rule_matches(
            &rule,
            "https://example.com/style.css",
            "Stylesheet"
        ));
        assert!(!rule_matches(
            &rule,
            "https://example.com/app.js",
            "Stylesheet"
        ));

        let wildcard_rule = InterceptionRule {
            url_pattern: "*".into(),
            resource_types: vec![],
            action: InterceptionAction::Continue,
        };
        assert!(rule_matches(
            &wildcard_rule,
            "https://anything.com",
            "Image"
        ));
    }

    #[test]
    fn test_network_port_default() {
        let transport = MockTransport::with_responses(vec![
            r#"{"id":1,"result":{}}"#.into(),
            r#"{"id":2,"result":{}}"#.into(),
        ]);
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "t", "s");
        let net = CdpNetworkPort::new(session);
        assert!(net.clear_cache().is_ok());
        assert!(net.clear_cookies().is_ok());
    }

    #[test]
    fn test_add_interception_rule_sends_fetch_enable() {
        let transport = MockTransport::with_responses(vec![r#"{"id":1,"result":{}}"#.into()]);
        let transport_clone = transport.clone();
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "t", "s");
        let net = CdpNetworkPort::new(session);

        let rule = InterceptionRule {
            url_pattern: "*.js".into(),
            resource_types: vec![ResourceType::Script],
            action: InterceptionAction::Block,
        };

        let handle = net.add_interception_rule(rule).unwrap();
        let _ = handle; // suppress unused warning

        let sent = transport_clone.sent_messages();
        let fetch_enable_msgs: Vec<&String> =
            sent.iter().filter(|m| m.contains("Fetch.enable")).collect();
        assert!(
            !fetch_enable_msgs.is_empty(),
            "Expected Fetch.enable to be sent"
        );
        assert!(fetch_enable_msgs[0].contains("*.js"));
        assert!(fetch_enable_msgs[0].contains("Script"));
    }

    #[test]
    fn test_add_interception_rule_returns_handle() {
        let transport = MockTransport::with_responses(vec![r#"{"id":1,"result":{}}"#.into()]);
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "t", "s");
        let net = CdpNetworkPort::new(session);

        let rule = InterceptionRule {
            url_pattern: "*".into(),
            resource_types: vec![],
            action: InterceptionAction::Continue,
        };

        let handle = net.add_interception_rule(rule).unwrap();
        assert_ne!(
            handle.to_string(),
            InterceptionHandle::default().to_string()
        );
    }

    #[test]
    fn test_remove_interception_rule_disables_when_empty() {
        let transport = MockTransport::with_responses(vec![
            r#"{"id":1,"result":{}}"#.into(), // Fetch.enable from add
            r#"{"id":2,"result":{}}"#.into(), // Fetch.disable from remove
        ]);
        let transport_clone = transport.clone();
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "t", "s");
        let net = CdpNetworkPort::new(session);

        let rule = InterceptionRule {
            url_pattern: "*.css".into(),
            resource_types: vec![ResourceType::Stylesheet],
            action: InterceptionAction::Block,
        };

        let handle = net.add_interception_rule(rule).unwrap();
        net.remove_interception_rule(&handle).unwrap();

        let sent = transport_clone.sent_messages();
        assert!(sent.iter().any(|m| m.contains("Fetch.enable")));
        assert!(sent.iter().any(|m| m.contains("Fetch.disable")));
    }

    #[test]
    fn test_remove_interception_rule_not_found() {
        let (session, _) = make_session();
        let net = CdpNetworkPort::new(session);

        let handle = InterceptionHandle::new();
        let result = net.remove_interception_rule(&handle);
        match result {
            Err(BridgeError::InterceptionRuleNotFound) => {}
            _ => panic!("Expected InterceptionRuleNotFound"),
        }
    }

    #[test]
    fn test_fetch_request_paused_block_action() {
        let transport = MockTransport::with_responses(vec![
            r#"{"id":1,"result":{}}"#.into(), // Fetch.enable
            r#"{"id":2,"result":{}}"#.into(), // Fetch.failRequest from handler
        ]);
        let transport_clone = transport.clone();
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "t", "s");
        let net = CdpNetworkPort::new(session);

        // Add block rule for JS files
        let rule = InterceptionRule {
            url_pattern: "*.js".into(),
            resource_types: vec![ResourceType::Script],
            action: InterceptionAction::Block,
        };
        let _handle = net.add_interception_rule(rule).unwrap();

        // Push a matching requestPaused event
        transport_clone.push_event(
            r#"{"method":"Fetch.requestPaused","params":{"requestId":"req-1","request":{"url":"https://example.com/app.js"},"resourceType":"Script"}}"#.into(),
        );

        std::thread::sleep(Duration::from_millis(100));

        let sent = transport_clone.sent_messages();
        let fail_msgs: Vec<&String> = sent
            .iter()
            .filter(|m| m.contains("Fetch.failRequest"))
            .collect();
        assert!(
            !fail_msgs.is_empty(),
            "Expected Fetch.failRequest to be sent"
        );
        assert!(fail_msgs.last().unwrap().contains("blockedByClient"));
    }

    #[test]
    fn test_fetch_request_paused_continue_action() {
        let transport = MockTransport::with_responses(vec![
            r#"{"id":1,"result":{}}"#.into(), // Fetch.enable
            r#"{"id":2,"result":{}}"#.into(), // Fetch.continueRequest from handler
        ]);
        let transport_clone = transport.clone();
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "t", "s");
        let net = CdpNetworkPort::new(session);

        // Add continue rule for all requests
        let rule = InterceptionRule {
            url_pattern: "*".into(),
            resource_types: vec![],
            action: InterceptionAction::Continue,
        };
        let _handle = net.add_interception_rule(rule).unwrap();

        transport_clone.push_event(
            r#"{"method":"Fetch.requestPaused","params":{"requestId":"req-2","request":{"url":"https://example.com/data.json"},"resourceType":"Fetch"}}"#.into(),
        );

        std::thread::sleep(Duration::from_millis(100));

        let sent = transport_clone.sent_messages();
        assert!(sent.iter().any(|m| m.contains("Fetch.continueRequest")));
    }

    #[test]
    fn test_fetch_request_paused_respond_action() {
        let transport = MockTransport::with_responses(vec![
            r#"{"id":1,"result":{}}"#.into(), // Fetch.enable
            r#"{"id":2,"result":{}}"#.into(), // Fetch.fulfillRequest from handler
        ]);
        let transport_clone = transport.clone();
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "t", "s");
        let net = CdpNetworkPort::new(session);

        let mut headers = std::collections::HashMap::new();
        headers.insert("Content-Type".into(), "application/json".into());

        let rule = InterceptionRule {
            url_pattern: "*.json".into(),
            resource_types: vec![],
            action: InterceptionAction::Respond {
                status: 200,
                headers,
                body: b"{\"status\":\"ok\"}".to_vec(),
            },
        };
        let _handle = net.add_interception_rule(rule).unwrap();

        transport_clone.push_event(
            r#"{"method":"Fetch.requestPaused","params":{"requestId":"req-3","request":{"url":"https://example.com/data.json"},"resourceType":"XHR"}}"#.into(),
        );

        std::thread::sleep(Duration::from_millis(100));

        let sent = transport_clone.sent_messages();
        let fulfill_msgs: Vec<&String> = sent
            .iter()
            .filter(|m| m.contains("Fetch.fulfillRequest"))
            .collect();
        assert!(
            !fulfill_msgs.is_empty(),
            "Expected Fetch.fulfillRequest to be sent"
        );
        assert!(fulfill_msgs
            .last()
            .unwrap()
            .contains(r#""responseCode":200"#));
    }

    #[test]
    fn test_fetch_request_paused_no_match_continues() {
        let transport = MockTransport::with_responses(vec![
            r#"{"id":1,"result":{}}"#.into(), // Fetch.enable
            r#"{"id":2,"result":{}}"#.into(), // Fetch.continueRequest from handler (default for no match)
        ]);
        let transport_clone = transport.clone();
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "t", "s");
        let net = CdpNetworkPort::new(session);

        // Only intercept scripts, but the event is for a stylesheet
        let rule = InterceptionRule {
            url_pattern: "*.js".into(),
            resource_types: vec![ResourceType::Script],
            action: InterceptionAction::Block,
        };
        let _handle = net.add_interception_rule(rule).unwrap();

        transport_clone.push_event(
            r#"{"method":"Fetch.requestPaused","params":{"requestId":"req-4","request":{"url":"https://example.com/style.css"},"resourceType":"Stylesheet"}}"#.into(),
        );

        std::thread::sleep(Duration::from_millis(100));

        // No rule matches, so the request should be continued (not blocked)
        let sent = transport_clone.sent_messages();
        assert!(sent.iter().any(|m| m.contains("Fetch.continueRequest")));
        // Should NOT contain Fetch.failRequest
        assert!(!sent.iter().any(|m| m.contains("Fetch.failRequest")));
    }

    #[test]
    fn test_input_port_click() {
        let transport = MockTransport::with_responses(vec![
            r#"{"id":1,"result":{"model":{"content":[[100,200],[300,200],[300,400],[100,400]]}}}"#
                .into(),
            r#"{"id":2,"result":{}}"#.into(),
        ]);
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "t", "s");
        let element = CdpElement::new(session.clone(), 1);
        let input = CdpInputPort::new(session);
        assert!(input.click(&element, ClickOptions::default()).is_ok());
    }

    #[test]
    fn test_input_port_press_key() {
        let transport = MockTransport::with_responses(vec![
            r#"{"id":1,"result":{}}"#.into(),
            r#"{"id":2,"result":{}}"#.into(),
        ]);
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "t", "s");
        let input = CdpInputPort::new(session);
        assert!(input.press_key("Enter").is_ok());
    }

    #[test]
    fn test_input_port_hover() {
        let transport = MockTransport::with_responses(vec![
            r#"{"id":1,"result":{"model":{"content":[[0,0],[100,0],[100,100],[0,100]]}}}"#.into(),
            r#"{"id":2,"result":{}}"#.into(),
        ]);
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "t", "s");
        let element = CdpElement::new(session.clone(), 1);
        let input = CdpInputPort::new(session);
        assert!(input.hover(&element).is_ok());
    }

    #[test]
    fn test_input_port_scroll() {
        let transport = MockTransport::with_responses(vec![r#"{"id":1,"result":{}}"#.into()]);
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "t", "s");
        let input = CdpInputPort::new(session);
        assert!(input.scroll(0.0, 100.0).is_ok());
    }

    #[test]
    fn test_input_port_fill() {
        let transport = MockTransport::with_responses(vec![
            r#"{"id":1,"result":{}}"#.into(),
            r#"{"id":2,"result":{"result":{"value":"test","type":"string"}}}"#.into(),
            r#"{"id":3,"result":{}}"#.into(),
        ]);
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "t", "s");
        let element = CdpElement::new(session.clone(), 1);
        let input = CdpInputPort::new(session);
        assert!(input.fill(&element, "hello").is_ok());
    }

    #[test]
    fn test_input_port_check() {
        let transport = MockTransport::with_responses(vec![
            r#"{"id":1,"result":{}}"#.into(),
            r#"{"id":2,"result":{"model":{"content":[[0,0],[20,0],[20,20],[0,20]]}}}"#.into(),
            r#"{"id":3,"result":{}}"#.into(),
        ]);
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "t", "s");
        let element = CdpElement::new(session.clone(), 1);
        let input = CdpInputPort::new(session);
        assert!(input.check(&element).is_ok());
    }

    #[test]
    fn test_input_port_uncheck() {
        let transport = MockTransport::with_responses(vec![
            r#"{"id":1,"result":{}}"#.into(),
            r#"{"id":2,"result":{"model":{"content":[[0,0],[20,0],[20,20],[0,20]]}}}"#.into(),
            r#"{"id":3,"result":{}}"#.into(),
        ]);
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "t", "s");
        let element = CdpElement::new(session.clone(), 1);
        let input = CdpInputPort::new(session);
        assert!(input.uncheck(&element).is_ok());
    }

    #[test]
    fn test_input_port_dblclick() {
        let transport = MockTransport::with_responses(vec![
            r#"{"id":1,"result":{"model":{"content":[[0,0],[20,0],[20,20],[0,20]]}}}"#.into(),
            r#"{"id":2,"result":{}}"#.into(),
        ]);
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "t", "s");
        let element = CdpElement::new(session.clone(), 1);
        let input = CdpInputPort::new(session);
        assert!(input.dblclick(&element, ClickOptions::default()).is_ok());
    }

    #[test]
    fn test_input_port_type_text() {
        let transport = MockTransport::with_responses(vec![
            r#"{"id":1,"result":{}}"#.into(),
            r#"{"id":2,"result":{}}"#.into(),
            r#"{"id":3,"result":{}}"#.into(),
            r#"{"id":4,"result":{}}"#.into(),
        ]);
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "t", "s");
        let element = CdpElement::new(session.clone(), 1);
        let input = CdpInputPort::new(session);
        assert!(input.type_text(&element, "x", Duration::ZERO).is_ok());
    }

    #[test]
    fn test_input_port_drag_and_drop() {
        let transport = MockTransport::with_responses(vec![
            r#"{"id":1,"result":{"model":{"content":[[0,0],[50,0],[50,50],[0,50]]}}}"#.into(),
            r#"{"id":2,"result":{}}"#.into(),
            r#"{"id":3,"result":{"model":{"content":[[100,100],[150,100],[150,150],[100,150]]}}}"#
                .into(),
            r#"{"id":4,"result":{}}"#.into(),
            r#"{"id":5,"result":{}}"#.into(),
        ]);
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "t", "s");
        let src = CdpElement::new(session.clone(), 1);
        let tgt = CdpElement::new(session.clone(), 2);
        let input = CdpInputPort::new(session);
        assert!(input.drag_and_drop(&src, &tgt).is_ok());
    }

    #[test]
    fn test_input_port_select_option() {
        let transport = MockTransport::with_responses(vec![
            r#"{"id":1,"result":{}}"#.into(),
            r#"{"id":2,"result":{"result":{"value":true}}}"#.into(),
        ]);
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "t", "s");
        let element = CdpElement::new(session.clone(), 1);
        let input = CdpInputPort::new(session);
        assert!(input.select_option(&element, &["option2"]).is_ok());
    }

    #[test]
    fn test_storage_port_empty() {
        let transport =
            MockTransport::with_responses(vec![r#"{"id":1,"result":{"cookies":[]}}"#.into()]);
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "t", "s");
        let storage = CdpStoragePort::new(session);
        let cookies = storage.cookies().unwrap();
        assert!(cookies.is_empty());
    }

    #[test]
    fn test_dialog_port_methods() {
        let transport = MockTransport::with_responses(vec![
            r#"{"id":1,"result":{}}"#.into(),
            r#"{"id":2,"result":{}}"#.into(),
        ]);
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "t", "s");
        let dialog = CdpDialogPort::new(session, PageId::new());
        assert!(dialog.accept(None).is_ok());
        assert!(dialog.dismiss().is_ok());
    }

    #[test]
    fn test_dialog_port_next_empty_initially() {
        let (session, _) = make_session();
        let dialog = CdpDialogPort::new(session, PageId::new());
        assert!(dialog.next().unwrap().is_none());
    }

    #[test]
    fn test_dialog_port_event_dispatch() {
        let transport = MockTransport::with_responses(vec![]);
        let transport_clone = transport.clone();
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "t", "s");
        let dialog = CdpDialogPort::new(session, PageId::new());

        // First call registers event listener, returns None
        assert!(dialog.next().unwrap().is_none());

        // Push a Page.javascriptDialogOpening event via the shared transport
        transport_clone.push_event(
            r#"{"method":"Page.javascriptDialogOpening","params":{"url":"https://example.com","message":"Hello world","type":"alert","hasBrowserHandler":false,"defaultPrompt":"default"}}"#.into(),
        );

        // Wait for reader thread to dispatch event
        std::thread::sleep(Duration::from_millis(100));

        // next() should now return the dialog info
        let info = dialog.next().unwrap();
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.dialog_type, DialogType::Alert);
        assert_eq!(info.message, "Hello world");
        assert_eq!(info.default_value, Some("default".to_string()));

        // After take(), next() returns None again
        assert!(dialog.next().unwrap().is_none());
    }

    #[test]
    fn test_dialog_port_event_types() {
        let transport = MockTransport::with_responses(vec![]);
        let transport_clone = transport.clone();
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "t", "s");
        let dialog = CdpDialogPort::new(session, PageId::new());

        assert!(dialog.next().unwrap().is_none());

        // Push a confirm dialog event
        transport_clone.push_event(
            r#"{"method":"Page.javascriptDialogOpening","params":{"url":"https://example.com","message":"Are you sure?","type":"confirm","hasBrowserHandler":false}}"#.into(),
        );

        std::thread::sleep(Duration::from_millis(100));

        let info = dialog.next().unwrap().unwrap();
        assert_eq!(info.dialog_type, DialogType::Confirm);
        assert_eq!(info.message, "Are you sure?");
        assert!(info.default_value.is_none());
    }

    #[test]
    fn test_download_port_empty() {
        let (session, _) = make_session();
        let download = CdpDownloadPort::new(session);
        assert!(download.downloads().is_empty());
    }

    #[test]
    fn test_download_path_default() {
        let (session, _) = make_session();
        let download = CdpDownloadPort::new(session);
        assert_eq!(download.download_path(), PathBuf::from("."));
    }

    #[test]
    fn test_download_set_path_stored() {
        let (session, _) = make_session();
        let download = CdpDownloadPort::new(session);
        let path = PathBuf::from(r#"C:\Downloads"#);
        // set_download_path doesn't need a real Browser.setDownloadBehavior response;
        // it gracefully handles errors by storing the path locally
        let _ = download.set_download_path(path.clone());
        assert_eq!(download.download_path(), path);
    }

    #[test]
    fn test_download_cancel_not_found() {
        let (session, _) = make_session();
        let download = CdpDownloadPort::new(session);
        match download.cancel_download("nonexistent") {
            Err(BridgeError::DownloadNotFound { .. }) => {}
            _ => panic!("Expected DownloadNotFound"),
        }
    }

    #[test]
    fn test_download_wait_completion_empty() {
        let (session, _) = make_session();
        let download = CdpDownloadPort::new(session);
        let result = download
            .wait_for_completion(Duration::from_millis(100))
            .unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_download_progress_event_tracks_state() {
        let transport = MockTransport::with_responses(vec![]);
        let transport_clone = transport.clone();
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "t", "s");
        let download = CdpDownloadPort::new(session);

        // First call registers listener
        assert!(download.downloads().is_empty());

        // Push Browser.downloadWillBegin event
        transport_clone.push_event(
            r#"{"method":"Browser.downloadWillBegin","params":{"guid":"d1","url":"https://example.com/file.zip","suggestedFilename":"file.zip","totalBytes":1000}}"#.into(),
        );

        std::thread::sleep(Duration::from_millis(100));

        let downloads = download.downloads();
        assert_eq!(downloads.len(), 1);
        assert_eq!(downloads[0].download_id, "d1");
        assert_eq!(downloads[0].state, DownloadState::InProgress);

        // Push Browser.downloadProgress completed event
        transport_clone.push_event(
            r#"{"method":"Browser.downloadProgress","params":{"guid":"d1","state":"completed","receivedBytes":1000,"totalBytes":1000}}"#.into(),
        );

        std::thread::sleep(Duration::from_millis(100));

        let downloads = download.downloads();
        assert_eq!(downloads[0].state, DownloadState::Completed);
    }

    #[test]
    fn test_download_progress_cancel_updates_state() {
        let transport = MockTransport::with_responses(vec![]);
        let transport_clone = transport.clone();
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "t", "s");
        let download = CdpDownloadPort::new(session);

        download.downloads(); // registers listener

        transport_clone.push_event(
            r#"{"method":"Browser.downloadWillBegin","params":{"guid":"d2","url":"https://example.com/doc.pdf","suggestedFilename":"doc.pdf","totalBytes":500}}"#.into(),
        );
        std::thread::sleep(Duration::from_millis(100));

        assert_eq!(download.downloads().len(), 1);

        // Cancel via API
        download.cancel_download("d2").unwrap();

        let downloads = download.downloads();
        assert_eq!(downloads[0].state, DownloadState::Cancelled);
    }

    #[test]
    fn test_download_wait_for_completion_blocks() {
        let transport = MockTransport::with_responses(vec![]);
        let transport_clone = transport.clone();
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "t", "s");
        let download = CdpDownloadPort::new(session);

        download.downloads(); // registers listener

        // Start a download
        transport_clone.push_event(
            r#"{"method":"Browser.downloadWillBegin","params":{"guid":"d3","url":"https://example.com/big.zip","suggestedFilename":"big.zip","totalBytes":10000}}"#.into(),
        );
        std::thread::sleep(Duration::from_millis(50));

        // Wait for completion with a short timeout — should time out since download is in progress
        let result = download
            .wait_for_completion(Duration::from_millis(100))
            .unwrap();
        // The download is still InProgress, so wait_for_completion returns current state after timeout
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].state, DownloadState::InProgress);

        // Now complete it
        transport_clone.push_event(
            r#"{"method":"Browser.downloadProgress","params":{"guid":"d3","state":"completed","receivedBytes":10000,"totalBytes":10000}}"#.into(),
        );
        std::thread::sleep(Duration::from_millis(100));

        // Now wait should return immediately
        let result = download
            .wait_for_completion(Duration::from_millis(100))
            .unwrap();
        assert_eq!(result[0].state, DownloadState::Completed);
    }

    #[test]
    fn test_locator_to_selector_variants() {
        assert_eq!(
            locator_to_selector(&LocatorStrategy::Css("div".into())).unwrap(),
            "div"
        );
        assert_eq!(
            locator_to_selector(&LocatorStrategy::TestId("submit".into())).unwrap(),
            "[data-testid=\"submit\"]"
        );
        assert_eq!(
            locator_to_selector(&LocatorStrategy::Placeholder("Search".into())).unwrap(),
            "[placeholder=\"Search\"]"
        );
        assert_eq!(
            locator_to_selector(&LocatorStrategy::Label("Name".into())).unwrap(),
            "[aria-label=\"Name\"]"
        );
        assert_eq!(
            locator_to_selector(&LocatorStrategy::AltText("Logo".into())).unwrap(),
            "img[alt=\"Logo\"]"
        );
    }

    #[test]
    fn test_paper_sizes() {
        assert_eq!(paper_size(&PdfPaperFormat::A4), (8.27, 11.7));
        assert_eq!(paper_size(&PdfPaperFormat::Letter), (8.5, 11.0));
        assert_eq!(paper_size(&PdfPaperFormat::Legal), (8.5, 14.0));
    }

    #[test]
    fn test_locate_port_locate() {
        let transport = MockTransport::with_responses(vec![
            // DOM.getDocument returns root node
            r#"{"id":1,"result":{"root":{"nodeId":1}}}"#.into(),
            // DOM.querySelector returns matching node
            r#"{"id":2,"result":{"nodeId":7}}"#.into(),
        ]);
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "t", "s");
        let locator = CdpLocatorEngine::new(session);
        let result = locator
            .locate(&LocatorStrategy::Css("button".into()))
            .unwrap();
        assert!(result.is_some());
    }

    #[test]
    fn test_locator_wait_for_found() {
        let transport = MockTransport::with_responses(vec![
            // DOM.getDocument returns root node
            r#"{"id":1,"result":{"root":{"nodeId":1}}}"#.into(),
            // DOM.querySelector returns matching node
            r#"{"id":2,"result":{"nodeId":10}}"#.into(),
        ]);
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "t", "s");
        let locator = CdpLocatorEngine::new(session);
        let element = locator
            .wait_for(
                &LocatorStrategy::Css("div.container".into()),
                Duration::from_secs(5),
            )
            .unwrap();
        assert_eq!(element.id(), ElementId::new(10));
    }

    #[test]
    fn test_locator_wait_for_timeout() {
        let transport = MockTransport::with_responses(vec![
            // DOM.querySelector returns empty (no nodeId) — triggers timeout
            r#"{"id":1,"result":{}}"#.into(),
        ]);
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "t", "s");
        let locator = CdpLocatorEngine::new(session);
        // With MockTransport, each send gets the same response — no nodeId means never found
        match locator.wait_for(
            &LocatorStrategy::Css(".nonexistent".into()),
            Duration::from_millis(50),
        ) {
            Err(BridgeError::Timeout) => {}
            _ => panic!("Expected Timeout"),
        }
    }

    #[test]
    fn test_locator_wait_for_absence_present_then_gone() {
        // First response has nodeId (element still present), second has none (element gone)
        let transport = MockTransport::with_responses(vec![
            r#"{"id":1,"result":{"nodeId":5}}"#.into(),
            r#"{"id":2,"result":{}}"#.into(),
        ]);
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "t", "s");
        let locator = CdpLocatorEngine::new(session);
        locator
            .wait_for_absence(
                &LocatorStrategy::Css(".removing".into()),
                Duration::from_secs(5),
            )
            .unwrap();
    }

    #[test]
    fn test_locator_wait_for_absence_already_gone() {
        let transport = MockTransport::with_responses(vec![r#"{"id":1,"result":{}}"#.into()]);
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        let session = CdpSession::new(Arc::new(conn), "t", "s");
        let locator = CdpLocatorEngine::new(session);
        locator
            .wait_for_absence(
                &LocatorStrategy::Css(".never-there".into()),
                Duration::from_millis(50),
            )
            .unwrap();
    }

    #[test]
    fn test_input_port_upload_file() {
        let transport = MockTransport::with_responses(vec![r#"{"id":1,"result":{}}"#.into()]);
        let (conn, _) = make_connection_with(transport);
        let session = CdpSession::new(conn, "t", "s");
        let element = CdpElement::new(session.clone(), 15);
        let input = CdpInputPort::new(session);
        input
            .upload_file(&element, &[PathBuf::from(r#"C:\file.txt"#)])
            .unwrap();
    }

    #[test]
    fn test_trait_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<CdpPage>();
        assert_send_sync::<CdpFrame>();
        assert_send_sync::<CdpElement>();
        assert_send_sync::<CdpLocatorEngine>();
        assert_send_sync::<CdpNetworkPort>();
        assert_send_sync::<CdpInputPort>();
        assert_send_sync::<CdpStoragePort>();
        assert_send_sync::<CdpDialogPort>();
        assert_send_sync::<CdpDownloadPort>();
    }
}
