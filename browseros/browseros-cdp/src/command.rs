pub mod page {
    use serde_json::Value;

    pub const NAVIGATE: &str = "Page.navigate";
    pub const RELOAD: &str = "Page.reload";
    pub const GO_BACK: &str = "Page.goBack";
    pub const GO_FORWARD: &str = "Page.goForward";
    pub const CAPTURE_SCREENSHOT: &str = "Page.captureScreenshot";
    pub const PRINT_TO_PDF: &str = "Page.printToPDF";
    pub const GET_CONTENT: &str = "Page.getContent";
    pub const SET_CONTENT: &str = "Page.setContent";
    pub const ENABLE: &str = "Page.enable";
    pub const DISABLE: &str = "Page.disable";
    pub const HANDLE_JAVASCRIPT_DIALOG: &str = "Page.handleJavaScriptDialog";
    pub const SET_VIEWPORT: &str = "Emulation.setDeviceMetricsOverride";
    pub const GET_NAVIGATION_HISTORY: &str = "Page.getNavigationHistory";
    pub const RESET_NAVIGATION_HISTORY: &str = "Page.resetNavigationHistory";
    pub const GET_FRAME_TREE: &str = "Page.getFrameTree";

    pub fn navigate(url: &str) -> Value {
        serde_json::json!({"url": url})
    }

    pub fn capture_screenshot(format: &str, quality: Option<u8>, full_page: bool) -> Value {
        let mut params = serde_json::json!({
            "format": format,
            "captureBeyondViewport": full_page,
        });
        if let Some(q) = quality {
            params["quality"] = serde_json::Value::from(q);
        }
        params
    }

    #[allow(clippy::too_many_arguments)]
    pub fn print_to_pdf(
        landscape: bool,
        print_background: bool,
        scale: f64,
        paper_width: f64,
        paper_height: f64,
        margin_top: f64,
        margin_bottom: f64,
        margin_left: f64,
        margin_right: f64,
    ) -> Value {
        serde_json::json!({
            "landscape": landscape,
            "printBackground": print_background,
            "scale": scale,
            "paperWidth": paper_width,
            "paperHeight": paper_height,
            "marginTop": margin_top,
            "marginBottom": margin_bottom,
            "marginLeft": margin_left,
            "marginRight": margin_right,
        })
    }

    pub fn set_content(html: &str) -> Value {
        serde_json::json!({"html": html})
    }

    pub fn handle_javascript_dialog(accept: bool, prompt_text: Option<&str>) -> Value {
        let mut params = serde_json::json!({
            "accept": accept,
        });
        if let Some(text) = prompt_text {
            params["promptText"] = serde_json::Value::from(text);
        }
        params
    }

    pub fn set_viewport(
        width: u32,
        height: u32,
        device_scale_factor: Option<f64>,
        is_mobile: bool,
    ) -> Value {
        let mut params = serde_json::json!({
            "width": width,
            "height": height,
            "mobile": is_mobile,
        });
        if let Some(dsf) = device_scale_factor {
            params["deviceScaleFactor"] = serde_json::Value::from(dsf);
        }
        params
    }
}

pub mod runtime {
    use serde_json::Value;

    pub const ENABLE: &str = "Runtime.enable";
    pub const EVALUATE: &str = "Runtime.evaluate";
    pub const CALL_FUNCTION_ON: &str = "Runtime.callFunctionOn";
    pub const RUN_IF_WAITING_FOR_DEBUGGER: &str = "Runtime.runIfWaitingForDebugger";

    pub fn evaluate(expression: &str, return_by_value: bool, await_promise: bool) -> Value {
        serde_json::json!({
            "expression": expression,
            "returnByValue": return_by_value,
            "awaitPromise": await_promise,
        })
    }

    pub fn call_function_on(function_declaration: &str, object_id: &str) -> Value {
        serde_json::json!({
            "functionDeclaration": function_declaration,
            "objectId": object_id,
            "returnByValue": true,
        })
    }
}

pub mod dom {
    use serde_json::Value;

    pub const ENABLE: &str = "DOM.enable";
    pub const GET_DOCUMENT: &str = "DOM.getDocument";
    pub const QUERY_SELECTOR: &str = "DOM.querySelector";
    pub const QUERY_SELECTOR_ALL: &str = "DOM.querySelectorAll";
    pub const GET_OUTER_HTML: &str = "DOM.getOuterHTML";
    pub const GET_ATTRIBUTES: &str = "DOM.getAttributes";
    pub const SET_ATTRIBUTE_VALUE: &str = "DOM.setAttributeValue";
    pub const REMOVE_ATTRIBUTE: &str = "DOM.removeAttribute";
    pub const GET_BOX_MODEL: &str = "DOM.getBoxModel";
    pub const FOCUS: &str = "DOM.focus";
    pub const GET_DOCUMENT_ELEMENT: &str = "DOM.getDocument";
    pub const RESOLVE_NODE: &str = "DOM.resolveNode";
    pub const REQUEST_CHILD_NODES: &str = "DOM.requestChildNodes";
    pub const DESCRIBE_NODE: &str = "DOM.describeNode";
    pub const REQUEST_NODE: &str = "DOM.requestNode";
    pub const SET_FILE_INPUT_FILES: &str = "DOM.setFileInputFiles";

    pub fn query_selector(node_id: u64, selector: &str) -> Value {
        serde_json::json!({
            "nodeId": node_id,
            "selector": selector,
        })
    }

    pub fn get_outer_html(node_id: u64) -> Value {
        serde_json::json!({"nodeId": node_id})
    }

    pub fn get_box_model(node_id: u64) -> Value {
        serde_json::json!({"nodeId": node_id})
    }

    pub fn set_attribute(node_id: u64, name: &str, value: &str) -> Value {
        serde_json::json!({
            "nodeId": node_id,
            "name": name,
            "value": value,
        })
    }

    pub fn resolve_node(node_id: u64, object_group: &str) -> Value {
        serde_json::json!({
            "nodeId": node_id,
            "objectGroup": object_group,
        })
    }

    pub fn describe_node(node_id: u64, depth: u32) -> Value {
        serde_json::json!({
            "nodeId": node_id,
            "depth": depth,
        })
    }

    pub fn focus_node(node_id: u64) -> Value {
        serde_json::json!({"nodeId": node_id})
    }

    pub fn request_node(object_id: &str) -> Value {
        serde_json::json!({"objectId": object_id})
    }

    pub fn set_file_input_files(node_id: u64, files: &[String]) -> Value {
        serde_json::json!({
            "nodeId": node_id,
            "files": files,
        })
    }
}

pub mod target {
    use serde_json::Value;

    pub const CREATE_TARGET: &str = "Target.createTarget";
    pub const CLOSE_TARGET: &str = "Target.closeTarget";
    pub const ATTACH_TO_TARGET: &str = "Target.attachToTarget";
    pub const DETACH_FROM_TARGET: &str = "Target.detachFromTarget";
    pub const GET_TARGETS: &str = "Target.getTargets";
    pub const ACTIVATE_TARGET: &str = "Target.activateTarget";
    pub const SET_AUTO_ATTACH: &str = "Target.setAutoAttach";
    pub const GET_TARGET_INFO: &str = "Target.getTargetInfo";

    pub fn create_target(url: &str) -> Value {
        serde_json::json!({"url": url})
    }

    pub fn close_target(target_id: &str) -> Value {
        serde_json::json!({"targetId": target_id})
    }

    pub fn attach_to_target(target_id: &str, flatten: bool) -> Value {
        serde_json::json!({
            "targetId": target_id,
            "flatten": flatten,
        })
    }

    pub fn activate_target(target_id: &str) -> Value {
        serde_json::json!({"targetId": target_id})
    }
}

pub mod browser {
    use serde_json::Value;

    pub const CLOSE: &str = "Browser.close";
    pub const GET_VERSION: &str = "Browser.getVersion";
    pub const GET_WINDOW_BOUNDS: &str = "Browser.getWindowBounds";
    pub const SET_WINDOW_BOUNDS: &str = "Browser.setWindowBounds";
    pub const SET_DOWNLOAD_BEHAVIOR: &str = "Browser.setDownloadBehavior";

    pub fn set_window_bounds(width: u32, height: u32) -> Value {
        serde_json::json!({
            "windowBounds": {
                "width": width,
                "height": height,
            }
        })
    }

    pub fn set_download_behavior(behavior: &str, download_path: Option<&str>) -> Value {
        let mut params = serde_json::json!({
            "behavior": behavior,
        });
        if let Some(path) = download_path {
            params["downloadPath"] = Value::from(path);
        }
        params
    }
}

pub mod network {
    use serde_json::Value;

    pub const ENABLE: &str = "Network.enable";
    pub const DISABLE: &str = "Network.disable";
    pub const SET_BLOCKED_URLS: &str = "Network.setBlockedURLs";
    pub const SET_CACHE_DISABLED: &str = "Network.setCacheDisabled";
    pub const CLEAR_BROWSER_CACHE: &str = "Network.clearBrowserCache";
    pub const CLEAR_BROWSER_COOKIES: &str = "Network.clearBrowserCookies";
    pub const SET_USER_AGENT_OVERRIDE: &str = "Network.setUserAgentOverride";

    pub fn set_blocked_urls(urls: Vec<String>) -> Value {
        serde_json::json!({"urls": urls})
    }

    pub fn set_cache_disabled(disabled: bool) -> Value {
        serde_json::json!({"cacheDisabled": disabled})
    }
}

pub mod input {
    use serde_json::Value;

    pub const DISPATCH_MOUSE_EVENT: &str = "Input.dispatchMouseEvent";
    pub const DISPATCH_KEY_EVENT: &str = "Input.dispatchKeyEvent";
    pub const INSERT_TEXT: &str = "Input.insertText";

    pub fn dispatch_mouse_click(x: f64, y: f64, button: &str, click_count: u32) -> Value {
        serde_json::json!({
            "type": "mousePressed",
            "x": x,
            "y": y,
            "button": button,
            "clickCount": click_count,
        })
    }

    pub fn dispatch_key_event(key: &str) -> Value {
        serde_json::json!({
            "type": "keyDown",
            "key": key,
        })
    }

    pub fn key_down(key: &str) -> Value {
        serde_json::json!({
            "type": "keyDown",
            "key": key,
            "windowsVirtualKeyCode": 0,
        })
    }

    pub fn key_up(key: &str) -> Value {
        serde_json::json!({
            "type": "keyUp",
            "key": key,
            "windowsVirtualKeyCode": 0,
        })
    }

    pub fn char_event(char: char) -> Value {
        serde_json::json!({
            "type": "char",
            "text": char.to_string(),
            "key": char.to_string(),
        })
    }

    pub fn insert_text(text: &str) -> Value {
        serde_json::json!({"text": text})
    }
}

pub mod storage {
    pub const GET_COOKIES: &str = "Storage.getCookies";
    pub const SET_COOKIES: &str = "Storage.setCookies";
    pub const CLEAR_COOKIES: &str = "Storage.clearCookies";
}

pub mod css {
    use serde_json::Value;

    pub const GET_COMPUTED_STYLE_FOR_NODE: &str = "CSS.getComputedStyleForNode";

    pub fn get_computed_style(node_id: u64) -> Value {
        serde_json::json!({"nodeId": node_id})
    }
}

pub mod fetch {
    use serde_json::Value;

    pub const ENABLE: &str = "Fetch.enable";
    pub const DISABLE: &str = "Fetch.disable";
    pub const CONTINUE_REQUEST: &str = "Fetch.continueRequest";
    pub const FULFILL_REQUEST: &str = "Fetch.fulfillRequest";
    pub const FAIL_REQUEST: &str = "Fetch.failRequest";

    pub fn enable(patterns: Vec<Value>) -> Value {
        serde_json::json!({"patterns": patterns})
    }

    pub fn request_pattern(url_pattern: Option<&str>, resource_type: Option<&str>) -> Value {
        let mut pattern = serde_json::json!({"requestStage": "Request"});
        if let Some(url) = url_pattern {
            pattern["urlPattern"] = Value::from(url);
        }
        if let Some(rt) = resource_type {
            pattern["resourceType"] = Value::from(rt);
        }
        pattern
    }

    pub fn continue_request(request_id: &str) -> Value {
        serde_json::json!({"requestId": request_id})
    }

    pub fn fulfill_request(
        request_id: &str,
        status: u16,
        headers: Vec<Value>,
        body: &str,
    ) -> Value {
        serde_json::json!({
            "requestId": request_id,
            "responseCode": status,
            "responseHeaders": headers,
            "body": body,
        })
    }

    pub fn fail_request(request_id: &str, error_reason: &str) -> Value {
        serde_json::json!({
            "requestId": request_id,
            "errorReason": error_reason,
        })
    }

    pub fn response_header(name: &str, value: &str) -> Value {
        serde_json::json!({"name": name, "value": value})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_navigate_params() {
        let params = page::navigate("https://example.com");
        assert_eq!(params["url"], "https://example.com");
    }

    #[test]
    fn test_page_capture_screenshot() {
        let params = page::capture_screenshot("png", Some(80), true);
        assert_eq!(params["format"], "png");
        assert_eq!(params["quality"], 80);
        assert_eq!(params["captureBeyondViewport"], true);
    }

    #[test]
    fn test_page_capture_screenshot_no_quality() {
        let params = page::capture_screenshot("jpeg", None, false);
        assert_eq!(params["format"], "jpeg");
        assert!(params.get("quality").is_none());
    }

    #[test]
    fn test_runtime_evaluate_params() {
        let params = runtime::evaluate("1+1", true, true);
        assert_eq!(params["expression"], "1+1");
        assert_eq!(params["returnByValue"], true);
    }

    #[test]
    fn test_dom_query_selector() {
        let params = dom::query_selector(1, ".my-class");
        assert_eq!(params["nodeId"], 1);
        assert_eq!(params["selector"], ".my-class");
    }

    #[test]
    fn test_target_create() {
        let params = target::create_target("about:blank");
        assert_eq!(params["url"], "about:blank");
    }

    #[test]
    fn test_browser_version_command() {
        assert_eq!(browser::GET_VERSION, "Browser.getVersion");
    }

    #[test]
    fn test_network_set_blocked_urls() {
        let params = network::set_blocked_urls(vec!["https://ads.com".into()]);
        assert_eq!(params["urls"][0], "https://ads.com");
    }

    #[test]
    fn test_input_mouse_click() {
        let params = input::dispatch_mouse_click(100.0, 200.0, "left", 1);
        assert_eq!(params["x"], 100.0);
        assert_eq!(params["type"], "mousePressed");
        assert_eq!(params["clickCount"], 1);
    }

    #[test]
    fn test_input_key_down_up() {
        let down = input::key_down("Enter");
        assert_eq!(down["type"], "keyDown");
        assert_eq!(down["key"], "Enter");
        let up = input::key_up("Enter");
        assert_eq!(up["type"], "keyUp");
        assert_eq!(up["key"], "Enter");
    }

    #[test]
    fn test_input_char_event() {
        let params = input::char_event('a');
        assert_eq!(params["type"], "char");
        assert_eq!(params["text"], "a");
    }

    #[test]
    fn test_dom_get_box_model() {
        let params = dom::get_box_model(5);
        assert_eq!(params["nodeId"], 5);
    }

    #[test]
    fn test_page_handle_dialog() {
        let params = page::handle_javascript_dialog(true, Some("hello"));
        assert_eq!(params["accept"], true);
        assert_eq!(params["promptText"], "hello");
    }

    #[test]
    fn test_page_handle_dialog_no_text() {
        let params = page::handle_javascript_dialog(false, None);
        assert_eq!(params["accept"], false);
        assert!(params.get("promptText").is_none());
    }

    #[test]
    fn test_fetch_enable_no_patterns() {
        let params = fetch::enable(vec![]);
        assert_eq!(params["patterns"], serde_json::json!([]));
    }

    #[test]
    fn test_fetch_enable_with_patterns() {
        let patterns = vec![fetch::request_pattern(Some("*.js"), Some("Script"))];
        let params = fetch::enable(patterns);
        assert_eq!(params["patterns"][0]["urlPattern"], "*.js");
        assert_eq!(params["patterns"][0]["resourceType"], "Script");
        assert_eq!(params["patterns"][0]["requestStage"], "Request");
    }

    #[test]
    fn test_fetch_request_pattern_no_resource_type() {
        let p = fetch::request_pattern(Some("https://example.com/*"), None);
        assert_eq!(p["urlPattern"], "https://example.com/*");
        assert!(p.get("resourceType").is_none());
        assert_eq!(p["requestStage"], "Request");
    }

    #[test]
    fn test_fetch_continue_request() {
        let params = fetch::continue_request("request-1");
        assert_eq!(params["requestId"], "request-1");
    }

    #[test]
    fn test_fetch_fail_request() {
        let params = fetch::fail_request("req-1", "blockedByClient");
        assert_eq!(params["requestId"], "req-1");
        assert_eq!(params["errorReason"], "blockedByClient");
    }

    #[test]
    fn test_fetch_fulfill_request() {
        let headers = vec![fetch::response_header("Content-Type", "text/plain")];
        let params = fetch::fulfill_request("req-1", 200, headers, "OK");
        assert_eq!(params["requestId"], "req-1");
        assert_eq!(params["responseCode"], 200);
        assert_eq!(params["responseHeaders"][0]["name"], "Content-Type");
        assert_eq!(params["responseHeaders"][0]["value"], "text/plain");
        assert_eq!(params["body"], "OK");
    }

    #[test]
    fn test_fetch_response_header() {
        let h = fetch::response_header("X-Custom", "value");
        assert_eq!(h["name"], "X-Custom");
        assert_eq!(h["value"], "value");
    }

    #[test]
    fn test_dom_request_node() {
        let params = dom::request_node("obj-1");
        assert_eq!(params["objectId"], "obj-1");
    }

    #[test]
    fn test_dom_set_file_input_files() {
        let files = vec!["/path/a.txt".to_string(), "/path/b.txt".to_string()];
        let params = dom::set_file_input_files(42, &files);
        assert_eq!(params["nodeId"], 42);
        assert_eq!(params["files"][0], "/path/a.txt");
        assert_eq!(params["files"][1], "/path/b.txt");
    }
}
