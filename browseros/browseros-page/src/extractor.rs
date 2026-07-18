use std::collections::HashMap;

use browseros_bridge::error::BridgeResult;
use browseros_bridge::PagePort;

#[derive(Debug, Clone)]
pub struct PageContent {
    pub url: String,
    pub title: String,
    pub text: String,
    pub html: String,
    pub screenshots_taken: Vec<Vec<u8>>,
    pub metadata: HashMap<String, String>,
}

impl PageContent {
    pub fn new(url: String, title: String) -> Self {
        PageContent {
            url,
            title,
            text: String::new(),
            html: String::new(),
            screenshots_taken: Vec::new(),
            metadata: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LinkInfo {
    pub url: String,
    pub text: String,
    pub title: Option<String>,
}

#[derive(Debug, Clone)]
pub enum ExtractionTransform {
    Text,
    InnerHtml,
    OuterHtml,
    Attribute(String),
    Count,
    Exists,
}

#[derive(Debug, Clone)]
pub struct ExtractionField {
    pub name: String,
    pub selector: String,
    pub attribute: Option<String>,
    pub transform: Option<ExtractionTransform>,
}

impl ExtractionField {
    pub fn new(name: &str, selector: &str) -> Self {
        ExtractionField {
            name: name.into(),
            selector: selector.into(),
            attribute: None,
            transform: None,
        }
    }

    pub fn with_attribute(mut self, attr: &str) -> Self {
        self.attribute = Some(attr.into());
        self
    }

    pub fn with_transform(mut self, transform: ExtractionTransform) -> Self {
        self.transform = Some(transform);
        self
    }
}

#[derive(Debug, Clone)]
pub struct ExtractionSchema {
    pub fields: Vec<ExtractionField>,
}

impl ExtractionSchema {
    pub fn new(fields: Vec<ExtractionField>) -> Self {
        ExtractionSchema { fields }
    }
}

pub trait ContentExtractor: Send + Sync {
    fn extract_text(&self, page: &dyn PagePort) -> BridgeResult<String>;

    fn extract_html(&self, page: &dyn PagePort) -> BridgeResult<String>;

    fn extract_structured(
        &self,
        page: &dyn PagePort,
        schema: &ExtractionSchema,
    ) -> BridgeResult<serde_json::Value>;

    fn extract_links(&self, page: &dyn PagePort) -> BridgeResult<Vec<LinkInfo>>;

    fn extract_metadata(&self, page: &dyn PagePort) -> BridgeResult<HashMap<String, String>>;
}

pub struct DefaultContentExtractor;

impl DefaultContentExtractor {
    pub fn new() -> Self {
        DefaultContentExtractor
    }
}

impl ContentExtractor for DefaultContentExtractor {
    fn extract_text(&self, page: &dyn PagePort) -> BridgeResult<String> {
        let html = page.content()?;
        let text = strip_html_tags(&html);
        Ok(text)
    }

    fn extract_html(&self, page: &dyn PagePort) -> BridgeResult<String> {
        page.content()
    }

    fn extract_structured(
        &self,
        page: &dyn PagePort,
        schema: &ExtractionSchema,
    ) -> BridgeResult<serde_json::Value> {
        let html = page.content()?;
        let mut result = serde_json::Map::new();

        for field in &schema.fields {
            let value = extract_field_value(&html, field);
            result.insert(field.name.clone(), value);
        }

        Ok(serde_json::Value::Object(result))
    }

    fn extract_links(&self, page: &dyn PagePort) -> BridgeResult<Vec<LinkInfo>> {
        let html = page.content()?;
        Ok(extract_links_from_html(&html))
    }

    fn extract_metadata(&self, page: &dyn PagePort) -> BridgeResult<HashMap<String, String>> {
        let html = page.content()?;
        Ok(extract_metadata_from_html(&html))
    }
}

impl Default for DefaultContentExtractor {
    fn default() -> Self {
        Self::new()
    }
}

fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut in_entity = false;
    let mut entity_buf = String::new();

    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            '&' if !in_tag => {
                in_entity = true;
                entity_buf.clear();
            }
            ';' if in_entity => {
                in_entity = false;
                let decoded = match entity_buf.as_str() {
                    "amp" => "&",
                    "lt" => "<",
                    "gt" => ">",
                    "quot" => "\"",
                    "nbsp" => " ",
                    "apos" => "'",
                    _ => "",
                };
                result.push_str(decoded);
            }
            _ if in_entity => entity_buf.push(c),
            _ if !in_tag && !in_entity => result.push(c),
            _ => {}
        }
    }
    result
}

fn extract_field_value(html: &str, field: &ExtractionField) -> serde_json::Value {
    let _ = html;
    let _ = field;
    serde_json::Value::Null
}

fn extract_links_from_html(html: &str) -> Vec<LinkInfo> {
    let mut links = Vec::new();
    let mut pos = 0;

    while pos < html.len() {
        if let Some(start) = html[pos..].find("<a ") {
            let abs_start = pos + start;
            if let Some(end) = html[abs_start..].find('>') {
                let tag = &html[abs_start..abs_start + end];
                let url = extract_attr(tag, "href").unwrap_or_default();
                let title = extract_attr(tag, "title");
                pos = abs_start + end + 1;
                if let Some(text_end) = html[pos..].find("</a>") {
                    let text = html[pos..pos + text_end].to_string();
                    let text_clean = strip_html_tags(&text).trim().to_string();
                    if !url.is_empty() {
                        links.push(LinkInfo {
                            url,
                            text: text_clean,
                            title,
                        });
                    }
                    pos += text_end + 4;
                }
            } else {
                pos += 1;
            }
        } else {
            break;
        }
    }
    links
}

fn extract_metadata_from_html(html: &str) -> HashMap<String, String> {
    let mut meta = HashMap::new();
    let mut pos = 0;

    while pos < html.len() {
        if let Some(start) = html[pos..].find("<meta ") {
            let abs_start = pos + start;
            if let Some(end) = html[abs_start..].find('>') {
                let tag = &html[abs_start..abs_start + end];
                if let Some(name) =
                    extract_attr(tag, "name").or_else(|| extract_attr(tag, "property"))
                {
                    if let Some(content) = extract_attr(tag, "content") {
                        meta.insert(name.to_lowercase(), content);
                    }
                }
                if let Some(charset) = extract_attr(tag, "charset") {
                    meta.insert("charset".into(), charset);
                }
                pos = abs_start + end + 1;
            } else {
                pos += 1;
            }
        } else {
            break;
        }
    }

    if let Some(title_start) = html.find("<title>") {
        if let Some(title_end) = html[title_start + 7..].find("</title>") {
            let title = &html[title_start + 7..title_start + 7 + title_end];
            meta.insert("title".into(), title.trim().to_string());
        }
    }

    meta
}

fn extract_attr(tag: &str, name: &str) -> Option<String> {
    let pattern = format!(" {}=\"", name);
    if let Some(start) = tag.find(&pattern) {
        let val_start = start + pattern.len();
        if let Some(end) = tag[val_start..].find('"') {
            return Some(tag[val_start..val_start + end].to_string());
        }
    }
    let pattern_single = format!(" {}='", name);
    if let Some(start) = tag.find(&pattern_single) {
        let val_start = start + pattern_single.len();
        if let Some(end) = tag[val_start..].find('\'') {
            return Some(tag[val_start..val_start + end].to_string());
        }
    }
    if tag.contains(&format!(" {}", name)) && !tag.contains(&format!("{}=\"", name)) {
        return Some(String::new());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use browseros_bridge::error::BridgeResult;
    use browseros_bridge::identifiers::{NavigationId, PageId};
    use browseros_bridge::types::NavigationStatus;
    fn nav_finished(url: impl Into<String>) -> browseros_bridge::types::NavigationState {
        browseros_bridge::types::NavigationState {
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
        JsResult, NavigationState, PdfOptions, ScreenshotOptions, Viewport, WaitCondition,
    };
    use std::sync::Mutex;

    struct TestContentPage {
        html: Mutex<String>,
        title: Mutex<String>,
        page_id: PageId,
    }

    impl TestContentPage {
        fn new(html: &str, title: &str) -> Self {
            TestContentPage {
                html: Mutex::new(html.into()),
                title: Mutex::new(title.into()),
                page_id: PageId::new(),
            }
        }
    }

    impl PagePort for TestContentPage {
        fn id(&self) -> PageId {
            self.page_id
        }
        fn url(&self) -> String {
            "https://example.com".into()
        }
        fn title(&self) -> String {
            self.title.lock().unwrap().clone()
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
            Ok(self.html.lock().unwrap().clone())
        }
        fn set_content(&self, _html: &str) -> BridgeResult<()> {
            *self.html.lock().unwrap() = _html.to_owned();
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
            panic!("no dialog")
        }
        fn download(&self) -> Box<dyn DownloadPort> {
            panic!("no download")
        }
    }

    #[test]
    fn test_page_content_new() {
        let content = PageContent::new("https://example.com".into(), "Example".into());
        assert_eq!(content.url, "https://example.com");
        assert_eq!(content.title, "Example");
        assert!(content.text.is_empty());
        assert!(content.metadata.is_empty());
    }

    #[test]
    fn test_extract_text_basic() {
        let page = TestContentPage::new("<html><body><p>Hello World</p></body></html>", "Test");
        let extractor = DefaultContentExtractor::new();
        let text = extractor.extract_text(&page).unwrap();
        assert!(text.contains("Hello World"));
    }

    #[test]
    fn test_extract_text_strips_tags() {
        let page = TestContentPage::new(
            "<html><body><div><h1>Title</h1><p>Paragraph</p></div></body></html>",
            "Test",
        );
        let extractor = DefaultContentExtractor::new();
        let text = extractor.extract_text(&page).unwrap();
        assert!(text.contains("Title"));
        assert!(text.contains("Paragraph"));
        assert!(!text.contains("<h1>"));
    }

    #[test]
    fn test_extract_html() {
        let html = "<html><body><p>Content</p></body></html>";
        let page = TestContentPage::new(html, "Test");
        let extractor = DefaultContentExtractor::new();
        let result = extractor.extract_html(&page).unwrap();
        assert_eq!(result, html);
    }

    #[test]
    fn test_extract_links_empty() {
        let page = TestContentPage::new("<html><body><p>No links</p></body></html>", "Test");
        let extractor = DefaultContentExtractor::new();
        let links = extractor.extract_links(&page).unwrap();
        assert!(links.is_empty());
    }

    #[test]
    fn test_extract_links_single() {
        let html = r#"<html><body><a href="https://example.com">Example</a></body></html>"#;
        let page = TestContentPage::new(html, "Test");
        let extractor = DefaultContentExtractor::new();
        let links = extractor.extract_links(&page).unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].url, "https://example.com");
        assert_eq!(links[0].text, "Example");
    }

    #[test]
    fn test_extract_links_multiple() {
        let html = r#"<html><body>
            <a href="https://page1.com">Page 1</a>
            <a href="https://page2.com">Page 2</a>
        </body></html>"#;
        let page = TestContentPage::new(html, "Test");
        let extractor = DefaultContentExtractor::new();
        let links = extractor.extract_links(&page).unwrap();
        assert_eq!(links.len(), 2);
    }

    #[test]
    fn test_extract_links_with_title() {
        let html = r#"<a href="https://example.com" title="Example Site">Example</a>"#;
        let page = TestContentPage::new(html, "Test");
        let extractor = DefaultContentExtractor::new();
        let links = extractor.extract_links(&page).unwrap();
        assert_eq!(links[0].title.as_deref(), Some("Example Site"));
    }

    #[test]
    fn test_extract_metadata_title() {
        let html = "<html><head><title>My Page</title></head><body></body></html>";
        let page = TestContentPage::new(html, "My Page");
        let extractor = DefaultContentExtractor::new();
        let meta = extractor.extract_metadata(&page).unwrap();
        assert_eq!(meta.get("title").map(|s| s.as_str()), Some("My Page"));
    }

    #[test]
    fn test_extract_metadata_meta_tags() {
        let html = r#"<html><head>
            <meta name="description" content="A test page">
            <meta name="keywords" content="test, rust">
            <meta charset="utf-8">
        </head><body></body></html>"#;
        let page = TestContentPage::new(html, "Test");
        let extractor = DefaultContentExtractor::new();
        let meta = extractor.extract_metadata(&page).unwrap();
        assert_eq!(
            meta.get("description").map(|s| s.as_str()),
            Some("A test page")
        );
        assert_eq!(meta.get("keywords").map(|s| s.as_str()), Some("test, rust"));
        assert_eq!(meta.get("charset").map(|s| s.as_str()), Some("utf-8"));
    }

    #[test]
    fn test_extract_structured_empty_schema() {
        let page = TestContentPage::new("<html></html>", "Test");
        let extractor = DefaultContentExtractor::new();
        let schema = ExtractionSchema::new(vec![]);
        let result = extractor.extract_structured(&page, &schema).unwrap();
        assert_eq!(result, serde_json::Value::Object(serde_json::Map::new()));
    }

    #[test]
    fn test_extraction_field_builder() {
        let field = ExtractionField::new("title", "h1")
            .with_attribute("class")
            .with_transform(ExtractionTransform::Text);
        assert_eq!(field.name, "title");
        assert_eq!(field.selector, "h1");
        assert_eq!(field.attribute.as_deref(), Some("class"));
        assert!(matches!(field.transform, Some(ExtractionTransform::Text)));
    }

    #[test]
    fn test_link_info_struct() {
        let link = LinkInfo {
            url: "https://example.com".into(),
            text: "Example".into(),
            title: Some("Example Site".into()),
        };
        assert_eq!(link.url, "https://example.com");
        assert_eq!(link.text, "Example");
        assert_eq!(link.title.as_deref(), Some("Example Site"));
    }

    #[test]
    fn test_strip_html_tags_entities() {
        let text = strip_html_tags("Hello &amp; World");
        assert_eq!(text, "Hello & World");
    }

    #[test]
    fn test_strip_html_tags_nested() {
        let text = strip_html_tags("<div><p>Hello <b>World</b></p></div>");
        assert_eq!(text, "Hello World");
    }

    #[test]
    fn test_content_extractor_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<DefaultContentExtractor>();
    }

    #[test]
    fn test_schema_clone() {
        let schema = ExtractionSchema::new(vec![ExtractionField::new("title", "h1")]);
        let cloned = schema.clone();
        assert_eq!(cloned.fields.len(), 1);
    }
}
