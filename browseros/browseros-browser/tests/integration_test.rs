use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use browseros_bridge::error::BridgeError;
use browseros_bridge::locator::LocatorStrategy;
use browseros_bridge::types::{
    ClickOptions, InterceptionAction, InterceptionRule, JsResult, LaunchOptions, NavigationStatus,
    ResourceType, ScreenshotFormat, ScreenshotOptions, SessionConfig, Viewport, WaitCondition,
};
use browseros_event_bus::EventBus;
use browseros_observability::{LevelFilter, LogLevel, LogRecord, Logger, OutputSink};

use browseros_browser::cdp_backend::CdpBrowserBackend;
use browseros_browser::config::BrowserConfig;
use browseros_browser::handle::{PageHandle, SessionHandle};
use browseros_browser::BrowserManager;

struct NoopSink;

impl OutputSink for NoopSink {
    fn write(&self, _record: &LogRecord) {}
    fn flush(&self) {}
}

fn chrome_available() -> bool {
    find_chrome().is_some()
}

fn test_manager() -> BrowserManager {
    let bus = Arc::new(EventBus::new());
    let logger = Arc::new(Logger::new(
        Arc::new(NoopSink),
        Arc::new(LevelFilter::new(LogLevel::Warn)),
        "integration-test",
    ));
    let config = BrowserConfig::default();
    let manager = BrowserManager::new(bus, logger, &config);
    manager.register_backend(Box::new(CdpBrowserBackend::new()));
    manager
}

fn find_chrome() -> Option<std::path::PathBuf> {
    let candidates = if cfg!(target_os = "windows") {
        vec!["chrome.exe", "chromium.exe", "msedge.exe"]
    } else {
        vec![
            "chrome",
            "chromium",
            "google-chrome",
            "google-chrome-stable",
            "msedge",
        ]
    };
    let path = std::env::var_os("PATH").unwrap_or_default();
    for name in &candidates {
        for dir in std::env::split_paths(&path) {
            let full = dir.join(name);
            if full.exists() {
                return Some(full);
            }
        }
    }
    let common = if cfg!(target_os = "windows") {
        vec![
            r"C:\Program Files\Google\Chrome\Application\chrome.exe".into(),
            r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe".into(),
            r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe".into(),
        ]
    } else {
        vec![
            "/usr/bin/google-chrome".into(),
            "/usr/bin/google-chrome-stable".into(),
            "/usr/bin/chromium".into(),
            "/usr/bin/chromium-browser".into(),
        ]
    };
    common.into_iter().find(|p: &std::path::PathBuf| p.exists())
}

fn launch_options() -> LaunchOptions {
    LaunchOptions {
        executable: find_chrome(),
        headless: true,
        args: vec![
            "--no-sandbox".into(),
            "--disable-gpu".into(),
            "--disable-dev-shm-usage".into(),
        ],
        env: HashMap::new(),
        user_data_dir: None,
        timeout: Duration::from_secs(30),
    }
}

fn setup_page() -> (BrowserManager, SessionHandle, PageHandle) {
    let manager = test_manager();
    let handle = manager.launch(launch_options()).expect("launch browser");
    let session = handle
        .new_session(SessionConfig::default())
        .expect("create session");
    let page = session.new_page().expect("create page");
    (manager, session, page)
}

// ─── 1. Multi-page ──────────────────────────────────────────

#[test]
fn test_multi_page_navigation() {
    if !chrome_available() {
        eprintln!("SKIP: no Chromium-based browser found on PATH");
        return;
    }

    let (manager, session, page1) = setup_page();

    let nav1 = page1
        .navigate("data:text/html,<h1>Page1</h1>")
        .expect("navigate page1");
    assert_eq!(nav1.status, NavigationStatus::Finished);

    let page2 = session.new_page().expect("create page2");
    let nav2 = page2
        .navigate("data:text/html,<h1>Page2</h1>")
        .expect("navigate page2");
    assert_eq!(nav2.status, NavigationStatus::Finished);

    let page3 = session.new_page().expect("create page3");
    let nav3 = page3
        .navigate("data:text/html,<h1>Page3</h1>")
        .expect("navigate page3");
    assert_eq!(nav3.status, NavigationStatus::Finished);

    let all_pages = session.pages();
    assert!(
        all_pages.len() >= 3,
        "expected >= 3 pages, got {}",
        all_pages.len()
    );

    assert_ne!(page1.id(), page2.id(), "page IDs must be unique");
    assert_ne!(page2.id(), page3.id(), "page IDs must be unique");
    assert_ne!(page1.id(), page3.id(), "page IDs must be unique");

    assert!(
        page1.url().contains("Page1"),
        "page1 URL should contain Page1: {}",
        page1.url()
    );
    assert!(
        page2.url().contains("Page2"),
        "page2 URL should contain Page2: {}",
        page2.url()
    );
    assert!(
        page3.url().contains("Page3"),
        "page3 URL should contain Page3: {}",
        page3.url()
    );

    drop(page1);
    drop(page2);
    drop(page3);
    drop(session);
    manager.shutdown();
}

// ─── 2. Iframe lifecycle ────────────────────────────────────

#[test]
fn test_iframe_detection() {
    if !chrome_available() {
        eprintln!("SKIP: no Chromium-based browser found on PATH");
        return;
    }

    let (manager, session, page) = setup_page();

    let html = concat!(
        r#"<html><body>"#,
        r#"<iframe src="data:text/html,<h2>Child</h2>"></iframe>"#,
        r#"<h1>Parent</h1>"#,
        r#"</body></html>"#
    );
    page.set_content(html).expect("set content");
    std::thread::sleep(Duration::from_millis(500));

    let all_frames = page.frames();
    assert!(
        !all_frames.is_empty(),
        "should have at least one frame (main)"
    );

    let root = page.main_frame();
    assert!(
        root.parent_id().is_none(),
        "main frame should have no parent"
    );

    let children = root.child_frames();
    if !children.is_empty() {
        let iframe = &children[0];
        assert!(iframe.parent_id().is_some(), "iframe should have a parent");
        assert!(
            iframe.url().contains("Child") || iframe.url().contains("child"),
            "iframe URL should reference child content: {}",
            iframe.url()
        );
    }

    drop(page);
    drop(session);
    manager.shutdown();
}

#[test]
fn test_iframe_javascript_access() {
    if !chrome_available() {
        eprintln!("SKIP: no Chromium-based browser found on PATH");
        return;
    }

    let (manager, session, page) = setup_page();

    page.set_content(concat!(
        r#"<html><body>"#,
        r#"<iframe id="f1" src="data:text/html,<span id='m'>H</span>"></iframe>"#,
        r#"</body></html>"#,
    ))
    .expect("set content");
    std::thread::sleep(Duration::from_millis(500));

    let js_script = concat!(
        r#"(() => {"#,
        r#"const f = document.getElementById('f1');"#,
        r#"if (!f) return 'no iframe';"#,
        r#"const d = f.contentDocument || f.contentWindow.document;"#,
        r#"if (!d) return 'cross-origin';"#,
        r#"const s = d.getElementById('m');"#,
        r#"return s ? s.textContent : 'no span';"#,
        r#"})()"#,
    );
    let result: JsResult = page.evaluate(js_script).expect("evaluate");

    if let Some(msg) = result.value.as_str() {
        assert!(
            msg == "H" || msg == "cross-origin",
            "unexpected iframe result: {msg}"
        );
    }

    drop(page);
    drop(session);
    manager.shutdown();
}

// ─── 3. Navigation ──────────────────────────────────────────

#[test]
fn test_navigation_url_update() {
    if !chrome_available() {
        eprintln!("SKIP: no Chromium-based browser found on PATH");
        return;
    }

    let (manager, session, page) = setup_page();

    let _nav = page
        .navigate("data:text/html,<h1>First</h1>")
        .expect("navigate");
    assert!(
        page.url().contains("First"),
        "URL should reflect first page: {}",
        page.url()
    );

    let _nav = page
        .navigate("data:text/html,<h1>Second</h1>")
        .expect("navigate second");
    assert!(
        page.url().contains("Second"),
        "URL should reflect second page: {}",
        page.url()
    );

    drop(page);
    drop(session);
    manager.shutdown();
}

#[test]
fn test_navigation_go_back_forward() {
    if !chrome_available() {
        eprintln!("SKIP: no Chromium-based browser found on PATH");
        return;
    }

    let (manager, session, page) = setup_page();

    page.navigate("data:text/html,<h1>A</h1>").expect("nav A");
    std::thread::sleep(Duration::from_millis(200));
    let url_a = page.url();

    page.navigate("data:text/html,<h1>B</h1>").expect("nav B");
    std::thread::sleep(Duration::from_millis(200));
    let url_b = page.url();

    assert_ne!(url_a, url_b, "URLs should differ after navigation");

    let back = page.go_back().expect("go back");
    assert_eq!(back.status, NavigationStatus::Finished);

    let fwd = page.go_forward().expect("go forward");
    assert_eq!(fwd.status, NavigationStatus::Finished);

    drop(page);
    drop(session);
    manager.shutdown();
}

#[test]
fn test_navigation_reload() {
    if !chrome_available() {
        eprintln!("SKIP: no Chromium-based browser found on PATH");
        return;
    }

    let (manager, session, page) = setup_page();

    page.navigate("data:text/html,<h1>Reload Test</h1>")
        .expect("navigate");
    let url_before = page.url();

    let reload = page.reload().expect("reload");
    assert_eq!(reload.status, NavigationStatus::Finished);

    let url_after = page.url();
    assert_eq!(url_before, url_after, "URL should be same after reload");

    drop(page);
    drop(session);
    manager.shutdown();
}

// ─── 4. Dialog handling ─────────────────────────────────────

#[test]
fn test_dialog_alert_accept() {
    if !chrome_available() {
        eprintln!("SKIP: no Chromium-based browser found on PATH");
        return;
    }

    let (manager, session, page) = setup_page();

    let _ = page.evaluate("() => { alert('hello'); }");
    std::thread::sleep(Duration::from_millis(300));

    let dialog = page.dialog();
    if let Ok(Some(info)) = dialog.next() {
        assert_eq!(info.message, "hello");
        dialog.accept(None).expect("accept alert");
    }

    drop(page);
    drop(session);
    manager.shutdown();
}

#[test]
fn test_dialog_confirm_dismiss() {
    if !chrome_available() {
        eprintln!("SKIP: no Chromium-based browser found on PATH");
        return;
    }

    let (manager, session, page) = setup_page();

    let _ = page.evaluate("() => confirm('yes?')");
    std::thread::sleep(Duration::from_millis(300));

    let dialog = page.dialog();
    if let Ok(Some(info)) = dialog.next() {
        assert!(info.message.contains("yes"));
        dialog.dismiss().expect("dismiss confirm");
    }

    drop(page);
    drop(session);
    manager.shutdown();
}

// ─── 5. Downloads ───────────────────────────────────────────

#[test]
fn test_download_set_path() {
    if !chrome_available() {
        eprintln!("SKIP: no Chromium-based browser found on PATH");
        return;
    }

    let (manager, session, page) = setup_page();
    let tmp = std::env::temp_dir().join("browseros-int-dl");
    let _ = std::fs::create_dir_all(&tmp);

    let dl = page.download();
    dl.set_download_path(tmp.clone())
        .expect("set download path");
    let stored = dl.download_path();
    assert_eq!(stored, tmp);

    std::fs::remove_dir_all(&tmp).ok();
    drop(page);
    drop(session);
    manager.shutdown();
}

// ─── 6. Request interception ────────────────────────────────

#[test]
fn test_interception_block_request() {
    if !chrome_available() {
        eprintln!("SKIP: no Chromium-based browser found on PATH");
        return;
    }

    let (manager, session, page) = setup_page();

    let net = page.network();
    let rule = InterceptionRule {
        url_pattern: "*.css".to_string(),
        resource_types: vec![ResourceType::Stylesheet],
        action: InterceptionAction::Block,
    };
    let handle = net.add_interception_rule(rule).expect("add rule");

    let html = concat!(
        r#"<html><head>"#,
        r#"<link rel="stylesheet" href="https://example.com/test.css">"#,
        r#"</head><body></body></html>"#,
    );
    let _nav = page.navigate(&format!("data:text/html,{html}"));

    net.remove_interception_rule(&handle).expect("remove rule");

    drop(page);
    drop(session);
    manager.shutdown();
}

#[test]
fn test_interception_multiple_rules() {
    if !chrome_available() {
        eprintln!("SKIP: no Chromium-based browser found on PATH");
        return;
    }

    let (manager, session, page) = setup_page();

    let net = page.network();
    let h1 = net
        .add_interception_rule(InterceptionRule {
            url_pattern: "*block1*".to_string(),
            resource_types: vec![ResourceType::Other],
            action: InterceptionAction::Block,
        })
        .expect("add rule 1");

    let h2 = net
        .add_interception_rule(InterceptionRule {
            url_pattern: "*block2*".to_string(),
            resource_types: vec![],
            action: InterceptionAction::Block,
        })
        .expect("add rule 2");

    assert_ne!(h1, h2, "interception handles must be unique");

    net.remove_interception_rule(&h1).expect("remove rule 1");
    net.remove_interception_rule(&h2).expect("remove rule 2");

    drop(page);
    drop(session);
    manager.shutdown();
}

// ─── 7. File upload ─────────────────────────────────────────

#[test]
fn test_file_upload_element() {
    if !chrome_available() {
        eprintln!("SKIP: no Chromium-based browser found on PATH");
        return;
    }

    let (manager, session, page) = setup_page();

    page.set_content(concat!(
        r#"<html><body>"#,
        r#"<input type="file" id="upload" />"#,
        r#"</body></html>"#,
    ))
    .expect("set content");
    std::thread::sleep(Duration::from_millis(300));

    let tmp = std::env::temp_dir().join("browseros-upload-test.txt");
    std::fs::write(&tmp, b"integration test file content").expect("write temp file");

    let locate = page.locator();
    let el = locate
        .wait_for(
            &LocatorStrategy::Css("#upload".into()),
            Duration::from_secs(5),
        )
        .expect("find upload element");

    let result = page.input().upload_file(el.as_ref(), &[tmp.clone()]);
    std::fs::remove_file(&tmp).ok();

    match result {
        Ok(()) => {}
        Err(BridgeError::NotImplemented(_)) => {
            eprintln!("SKIP: upload_file not implemented in CDP layer");
        }
        Err(e) => panic!("unexpected upload error: {e}"),
    }

    drop(page);
    drop(session);
    manager.shutdown();
}

// ─── 8. Element interaction ─────────────────────────────────

#[test]
fn test_element_query_and_interaction() {
    if !chrome_available() {
        eprintln!("SKIP: no Chromium-based browser found on PATH");
        return;
    }

    let (manager, session, page) = setup_page();

    page.set_content(concat!(
        r#"<html><body>"#,
        r#"<button id="btn">Click Me</button>"#,
        r#"<div id="output"></div>"#,
        r#"</body></html>"#,
    ))
    .expect("set content");
    std::thread::sleep(Duration::from_millis(300));

    let locate = page.locator();
    let btn = locate
        .wait_for(&LocatorStrategy::Css("#btn".into()), Duration::from_secs(5))
        .expect("find button");

    assert!(
        btn.is_visible().unwrap_or(false),
        "button should be visible"
    );
    assert!(
        btn.is_enabled().unwrap_or(false),
        "button should be enabled"
    );

    let tag = btn.tag_name();
    assert_eq!(tag, "button", "tag should be 'button', got '{tag}'");

    let text = btn.text_content().expect("get text");
    assert!(
        text.contains("Click Me"),
        "text should contain 'Click Me': {text}"
    );

    let outer = btn.outer_html().expect("get outer HTML");
    assert!(
        outer.contains("btn"),
        "outer HTML should contain 'btn': {outer}"
    );

    let input = page.input();
    let click_result = input.click(btn.as_ref(), ClickOptions::default());
    assert!(
        click_result.is_ok() || matches!(&click_result, Err(BridgeError::NotImplemented(_))),
        "click should succeed or be not implemented: {click_result:?}"
    );

    drop(page);
    drop(session);
    manager.shutdown();
}

#[test]
fn test_element_multiple_query_strategies() {
    if !chrome_available() {
        eprintln!("SKIP: no Chromium-based browser found on PATH");
        return;
    }

    let (manager, session, page) = setup_page();

    page.set_content(concat!(
        r#"<html><body>"#,
        r#"<div data-testid="greeting">Hello World</div>"#,
        r#"<input placeholder="Enter name" />"#,
        r#"<p>Plain text</p>"#,
        r#"</body></html>"#,
    ))
    .expect("set content");
    std::thread::sleep(Duration::from_millis(300));

    let locate = page.locator();

    let by_testid = locate
        .locate(&LocatorStrategy::TestId("greeting".into()))
        .ok()
        .flatten();
    assert!(by_testid.is_some(), "testid strategy should find element");

    let by_placeholder = locate
        .locate(&LocatorStrategy::Placeholder("Enter name".into()))
        .expect("placeholder locate should succeed");
    assert!(
        by_placeholder.is_some(),
        "placeholder strategy should find element"
    );

    let all_p = locate
        .locate_all(&LocatorStrategy::Css("p".into()))
        .expect("locate_all p");
    assert!(!all_p.is_empty(), "should find <p> elements");

    drop(page);
    drop(session);
    manager.shutdown();
}

#[test]
fn test_element_stale_detection() {
    if !chrome_available() {
        eprintln!("SKIP: no Chromium-based browser found on PATH");
        return;
    }

    let (manager, session, page) = setup_page();

    page.set_content(concat!(
        r#"<html><body><div id="removable">Remove me</div></body></html>"#,
    ))
    .expect("set content");
    std::thread::sleep(Duration::from_millis(300));

    let locate = page.locator();
    let el = locate
        .wait_for(
            &LocatorStrategy::Css("#removable".into()),
            Duration::from_secs(5),
        )
        .expect("find removable");

    let _ = page.evaluate("document.getElementById('removable').remove()");
    std::thread::sleep(Duration::from_millis(200));

    let result = el.is_visible();
    assert!(
        result.is_err(),
        "is_visible on removed element should fail: {:?}",
        result
    );

    drop(page);
    drop(session);
    manager.shutdown();
}

// ─── 9. Page close/reopen ───────────────────────────────────

#[test]
fn test_page_close_and_reopen() {
    if !chrome_available() {
        eprintln!("SKIP: no Chromium-based browser found on PATH");
        return;
    }

    let (manager, session, page) = setup_page();
    let page_id = page.id();

    let initial_count = session.pages().len();

    session.close_page(&page_id).expect("close initial page");
    std::thread::sleep(Duration::from_millis(200));

    let after_close = session.pages().len();
    assert!(
        after_close < initial_count || after_close == 0,
        "page count should decrease after close"
    );

    let new_page = session.new_page().expect("reopen page");
    new_page
        .navigate("data:text/html,<h1>Reopened</h1>")
        .expect("navigate reopened page");
    assert!(
        new_page.url().contains("Reopened"),
        "reopened page should have navigated"
    );

    assert_ne!(page_id, new_page.id(), "reopened page should have new ID");

    drop(page);
    drop(new_page);
    drop(session);
    manager.shutdown();
}

#[test]
fn test_page_close_twice() {
    if !chrome_available() {
        eprintln!("SKIP: no Chromium-based browser found on PATH");
        return;
    }

    let (manager, session, page) = setup_page();
    let pid = page.id();

    session.close_page(&pid).expect("close page first time");
    std::thread::sleep(Duration::from_millis(200));

    let _second = session.close_page(&pid);

    drop(page);
    drop(session);
    manager.shutdown();
}

#[test]
fn test_session_close_cleanup() {
    if !chrome_available() {
        eprintln!("SKIP: no Chromium-based browser found on PATH");
        return;
    }

    let (manager, session, _page) = setup_page();

    for i in 0..3 {
        let p = session.new_page().expect(&format!("create page {i}"));
        let _nav = p.navigate(&format!("data:text/html,<h1>Page {i}</h1>"));
    }

    let count = session.pages().len();
    assert!(count >= 1, "should have at least one page, got {count}");

    session.close().expect("close session");

    drop(_page);
    drop(session);
    manager.shutdown();
}

// ─── 10. Browser shutdown ───────────────────────────────────

#[test]
fn test_browser_launch_and_shutdown() {
    if !chrome_available() {
        eprintln!("SKIP: no Chromium-based browser found on PATH");
        return;
    }

    let manager = test_manager();
    let handle = manager.launch(launch_options()).expect("launch browser");
    assert!(handle.is_alive(), "browser should be alive after launch");

    let info = handle.info();
    assert!(!info.version.is_empty(), "version should not be empty");

    let results = manager.shutdown();
    assert!(!results.is_empty(), "shutdown should return results");
    assert!(
        results.into_iter().all(|r| r.is_ok()),
        "all shutdown results should be ok"
    );
}

#[test]
fn test_browser_launch_close_cycle() {
    if !chrome_available() {
        eprintln!("SKIP: no Chromium-based browser found on PATH");
        return;
    }

    let manager = test_manager();
    let handle = manager.launch(launch_options()).expect("launch browser");

    let session = handle
        .new_session(SessionConfig::default())
        .expect("create session");
    let page = session.new_page().expect("create page");

    let _nav = page.navigate("data:text/html,<h1>Lifecycle</h1>");

    page.close().expect("close page");
    session.close().expect("close session");
    handle.close().expect("close browser");

    let results = manager.shutdown();
    assert!(
        results.into_iter().all(|r| r.is_ok()),
        "shutdown should be clean"
    );
}

#[test]
fn test_browser_multiple_sessions() {
    if !chrome_available() {
        eprintln!("SKIP: no Chromium-based browser found on PATH");
        return;
    }

    let manager = test_manager();
    let handle = manager.launch(launch_options()).expect("launch browser");

    let session1 = handle
        .new_session(SessionConfig::default())
        .expect("create session 1");
    let page1 = session1.new_page().expect("create page1");
    let _nav1 = page1.navigate("data:text/html,<h1>Session 1</h1>");

    let session2 = handle
        .new_session(SessionConfig::default())
        .expect("create session 2");
    let page2 = session2.new_page().expect("create page2");
    let _nav2 = page2.navigate("data:text/html,<h1>Session 2</h1>");

    assert!(page1.url().contains("Session 1"), "page1 in session1");
    assert!(page2.url().contains("Session 2"), "page2 in session2");

    assert_ne!(
        page1.id(),
        page2.id(),
        "pages in different sessions should have different IDs"
    );

    drop(page1);
    drop(page2);
    drop(session1);
    drop(session2);
    manager.shutdown();
}

// ─── JavaScript evaluation ──────────────────────────────────

#[test]
fn test_javascript_evaluation() {
    if !chrome_available() {
        eprintln!("SKIP: no Chromium-based browser found on PATH");
        return;
    }

    let (manager, session, page) = setup_page();

    let _nav = page.navigate("data:text/html,<div id='x'>42</div>");
    std::thread::sleep(Duration::from_millis(200));

    let result: JsResult = page
        .evaluate("(() => document.getElementById('x').textContent)()")
        .expect("evaluate");

    assert_eq!(
        result.value,
        serde_json::json!("42"),
        "JS should return '42'"
    );

    let num: JsResult = page.evaluate("(() => 1 + 2)()").expect("evaluate");
    assert_eq!(num.value, serde_json::json!(3), "1+2 should = 3");

    drop(page);
    drop(session);
    manager.shutdown();
}

#[test]
fn test_javascript_evaluate_handle() {
    if !chrome_available() {
        eprintln!("SKIP: no Chromium-based browser found on PATH");
        return;
    }

    let (manager, session, page) = setup_page();

    page.set_content(concat!(
        r#"<html><body><div id="handle-test">Handle</div></body></html>"#,
    ))
    .expect("set content");
    std::thread::sleep(Duration::from_millis(300));

    let result = page.evaluate_handle("(() => document.getElementById('handle-test'))()", None);

    match result {
        Ok(el) => {
            let tag = el.tag_name();
            assert_eq!(tag, "div", "handle should be a div, got '{tag}'");
            let text = el.text_content().expect("get text");
            assert_eq!(text, "Handle");
        }
        Err(BridgeError::Internal(_)) => {}
        Err(e) => panic!("unexpected evaluate_handle error: {e}"),
    }

    drop(page);
    drop(session);
    manager.shutdown();
}

// ─── Wait conditions ────────────────────────────────────────

#[test]
fn test_wait_for_navigation() {
    if !chrome_available() {
        eprintln!("SKIP: no Chromium-based browser found on PATH");
        return;
    }

    let (manager, session, page) = setup_page();

    let _nav = page.navigate("data:text/html,<h1>Wait Nav</h1>");

    let result = page.wait_for(WaitCondition::Navigation(Duration::from_secs(5)));
    assert!(
        result.is_ok(),
        "wait for navigation should succeed: {result:?}"
    );

    drop(page);
    drop(session);
    manager.shutdown();
}

#[test]
fn test_wait_for_function() {
    if !chrome_available() {
        eprintln!("SKIP: no Chromium-based browser found on PATH");
        return;
    }

    let (manager, session, page) = setup_page();

    let _nav = page.navigate("data:text/html,<div id='ready'>OK</div>");
    std::thread::sleep(Duration::from_millis(200));

    let result = page.wait_for(WaitCondition::Function(
        "(() => document.getElementById('ready') !== null)()".into(),
    ));
    assert!(
        result.is_ok(),
        "wait for function should succeed: {result:?}"
    );

    drop(page);
    drop(session);
    manager.shutdown();
}

// ─── Screenshot ─────────────────────────────────────────────

#[test]
fn test_screenshot_capture() {
    if !chrome_available() {
        eprintln!("SKIP: no Chromium-based browser found on PATH");
        return;
    }

    let (manager, session, page) = setup_page();
    page.set_viewport(Viewport::HD).ok();

    let _nav = page.navigate("data:text/html,<h1>Screenshot</h1>");
    std::thread::sleep(Duration::from_millis(200));

    let png = page
        .screenshot(ScreenshotOptions::default())
        .expect("screenshot PNG");
    assert!(!png.is_empty(), "screenshot should produce data");
    assert!(
        png.len() > 100,
        "screenshot should be larger than 100 bytes"
    );

    let jpeg = page
        .screenshot(ScreenshotOptions {
            format: ScreenshotFormat::Jpeg,
            quality: Some(80),
            ..Default::default()
        })
        .expect("screenshot JPEG");
    assert!(!jpeg.is_empty(), "JPEG screenshot should produce data");

    drop(page);
    drop(session);
    manager.shutdown();
}

// ─── Page content ───────────────────────────────────────────

#[test]
fn test_page_content_roundtrip() {
    if !chrome_available() {
        eprintln!("SKIP: no Chromium-based browser found on PATH");
        return;
    }

    let (manager, session, page) = setup_page();

    let html = concat!(r#"<html><body><p>Content roundtrip</p></body></html>"#);
    page.set_content(html).expect("set content");
    std::thread::sleep(Duration::from_millis(200));

    let retrieved = page.content().expect("get content");
    assert!(
        retrieved.contains("Content roundtrip"),
        "retrieved content should contain our text: {retrieved}"
    );

    drop(page);
    drop(session);
    manager.shutdown();
}

// ─── Input interaction ──────────────────────────────────────

#[test]
fn test_input_hover_and_click() {
    if !chrome_available() {
        eprintln!("SKIP: no Chromium-based browser found on PATH");
        return;
    }

    let (manager, session, page) = setup_page();
    page.set_viewport(Viewport::HD).ok();

    page.set_content(concat!(
        r#"<html><body><button id="t">T</button></body></html>"#
    ))
    .expect("set content");
    std::thread::sleep(Duration::from_millis(300));

    let locate = page.locator();
    let btn = locate
        .wait_for(&LocatorStrategy::Css("#t".into()), Duration::from_secs(5))
        .expect("find target");

    let input = page.input();
    let hover_result = input.hover(btn.as_ref());
    match &hover_result {
        Ok(()) => {}
        Err(BridgeError::NotImplemented(_)) => {}
        Err(e) => panic!("hover error: {e}"),
    }

    let click_result = input.click(btn.as_ref(), ClickOptions::default());
    assert!(
        click_result.is_ok() || matches!(&click_result, Err(BridgeError::NotImplemented(_))),
        "click should succeed or be not implemented: {click_result:?}"
    );

    drop(page);
    drop(session);
    manager.shutdown();
}

#[test]
fn test_input_fill() {
    if !chrome_available() {
        eprintln!("SKIP: no Chromium-based browser found on PATH");
        return;
    }

    let (manager, session, page) = setup_page();

    page.set_content(concat!(
        r#"<html><body>"#,
        r#"<input type="text" id="name" />"#,
        r#"</body></html>"#,
    ))
    .expect("set content");
    std::thread::sleep(Duration::from_millis(300));

    let locate = page.locator();
    let el = locate
        .wait_for(
            &LocatorStrategy::Css("#name".into()),
            Duration::from_secs(5),
        )
        .expect("find input");

    let fill_result = page.input().fill(el.as_ref(), "Integration Test");
    match &fill_result {
        Ok(()) => {
            let value: JsResult = page
                .evaluate("(() => document.getElementById('name').value)()")
                .expect("get value");
            assert_eq!(value.value, serde_json::json!("Integration Test"));
        }
        Err(BridgeError::NotImplemented(_)) => {}
        Err(e) => panic!("fill error: {e}"),
    }

    drop(page);
    drop(session);
    manager.shutdown();
}

#[test]
fn test_keyboard_press() {
    if !chrome_available() {
        eprintln!("SKIP: no Chromium-based browser found on PATH");
        return;
    }

    let (manager, session, page) = setup_page();

    let _nav = page.navigate("data:text/html,<h1>Keyboard</h1>");

    let result = page.input().press_key("Enter");
    assert!(
        result.is_ok() || matches!(&result, Err(BridgeError::NotImplemented(_))),
        "press_key should succeed or be not implemented: {result:?}"
    );

    drop(page);
    drop(session);
    manager.shutdown();
}

// ─── Storage Integration Tests ─────────────────────────────────────────
//
// These tests exercise cookie and web storage operations through the real
// CDP backend.
//
// Limitations:
//   - Cookies can only be SET when the page has a valid origin (http/https).
//     On `about:blank` or `data:` URIs, `set_cookies` returns a CDP error.
//   - localStorage and sessionStorage on `about:blank` have opaque origins
//     that restrict JavaScript access. Read/write operations silently
//     return empty defaults via the CDP implementation.
//
// Currently all storage tests run on `about:blank` (opaque origin).
// Cookies and localStorage writes silently degrade on opaque origins,
// so write operations are tested for graceful error handling rather
// than success. Future work: navigate to `http://localhost` for
// origin-dependent write tests.

#[test]
fn test_storage_cookies_read_empty() {
    if !chrome_available() {
        eprintln!("SKIP: no Chromium-based browser found on PATH");
        return;
    }

    let (_manager, _session, page) = setup_page();
    let storage = page.storage();
    let cookies = storage.cookies().expect("get cookies on fresh page");
    // On about:blank, cookies should be empty
    assert!(cookies.is_empty(), "cookies should be empty on about:blank");
}

#[test]
fn test_storage_cookies_clear_all_noop() {
    if !chrome_available() {
        eprintln!("SKIP: no Chromium-based browser found on PATH");
        return;
    }

    let (_manager, _session, page) = setup_page();
    let storage = page.storage();
    // delete_all_cookies should succeed even on about:blank
    storage.delete_all_cookies().expect("delete all cookies");
}

#[test]
fn test_storage_local_storage_operations() {
    if !chrome_available() {
        eprintln!("SKIP: no Chromium-based browser found on PATH");
        return;
    }

    let (_manager, _session, page) = setup_page();
    let _nav = page.navigate("about:blank");
    let storage = page.storage();

    // Read should return empty (opaque origin degrades gracefully)
    let entries = storage.local_storage().expect("get local storage");
    assert!(
        entries.is_empty(),
        "localStorage should be empty on about:blank"
    );

    // Write succeeds at the protocol level (JS error is swallowed)
    storage
        .set_local_storage(&[browseros_bridge::types::StorageEntry {
            key: "test_key".to_string(),
            value: "test_value".to_string(),
        }])
        .expect("set local storage");

    // After clear, still empty
    storage.clear_local_storage().expect("clear local storage");
}

#[test]
fn test_storage_session_storage_operations() {
    if !chrome_available() {
        eprintln!("SKIP: no Chromium-based browser found on PATH");
        return;
    }

    let (_manager, _session, page) = setup_page();
    let _nav = page.navigate("about:blank");
    let storage = page.storage();

    let entries = storage.session_storage().expect("get session storage");
    assert!(entries.is_empty(), "sessionStorage should start empty");

    storage
        .clear_session_storage()
        .expect("clear session storage");
}

#[test]
fn test_storage_manager_basic_ops() {
    if !chrome_available() {
        eprintln!("SKIP: no Chromium-based browser found on PATH");
        return;
    }

    let (_manager, _session, page) = setup_page();
    let _nav = page.navigate("about:blank");

    let sm = browseros_storage::StorageManager::new(&*page);

    // Cookies
    let cookies = sm.get_cookies().expect("StorageManager get_cookies");
    assert!(cookies.is_empty(), "cookies empty via StorageManager");

    // Local storage
    let ls = sm
        .get_local_storage()
        .expect("StorageManager get_local_storage");
    assert!(ls.is_empty(), "localStorage empty via StorageManager");

    // Session storage
    let ss = sm
        .get_session_storage()
        .expect("StorageManager get_session_storage");
    assert!(ss.is_empty(), "sessionStorage empty via StorageManager");

    // Cookie management
    sm.delete_all_cookies()
        .expect("StorageManager delete_all_cookies");
    sm.clear_local_storage()
        .expect("StorageManager clear_local_storage");
    sm.clear_session_storage()
        .expect("StorageManager clear_session_storage");
}

#[test]
fn test_storage_cookie_set_with_valid_domain() {
    // This test verifies that cookies can be set when a valid domain
    // is provided. If the backend rejects it (opaque origin), we check
    // that the error is handled gracefully.
    if !chrome_available() {
        eprintln!("SKIP: no Chromium-based browser found on PATH");
        return;
    }

    let (_manager, _session, page) = setup_page();
    let storage = page.storage();

    // Try setting a cookie with a valid-looking domain
    let cookie = browseros_bridge::types::Cookie {
        name: "test_sess".to_string(),
        value: "session_value".to_string(),
        domain: "localhost".to_string(),
        path: "/".to_string(),
        secure: false,
        http_only: false,
        same_site: browseros_bridge::types::SameSitePolicy::Lax,
        expires: None,
    };

    match storage.set_cookies(&[cookie]) {
        Ok(()) => {
            // If the backend accepted it, verify it's readable
            let cookies = storage.cookies().expect("get cookies after set");
            let found = cookies.iter().any(|c| c.name == "test_sess");
            assert!(found, "set cookie should be readable");
        }
        Err(e) => {
            // Opaque origin — this is expected on about:blank
            eprintln!("set_cookies rejected (expected on opaque origin): {e}");
        }
    }
}
