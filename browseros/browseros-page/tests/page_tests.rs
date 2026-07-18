use std::sync::Arc;

use browseros_bridge::identifiers::{FrameId, NavigationId, PageId};
use browseros_event_bus::EventBus;
use browseros_types::event::{DomainEvent, Event, EventMetadata};

use browseros_page::dialog::{DialogAutoHandler, DialogError, DialogStrategy};
use browseros_page::events::{
    DialogClosed, DialogOpened, FrameAttached, FrameDetached, FrameNavigationFinished,
    FrameNavigationStarted, LoadState, NavigationCommitted, NavigationFinished, NavigationStarted,
    PageLoadState, PageUrlChanged, TitleChanged,
};
use browseros_page::extractor::{
    ContentExtractor, DefaultContentExtractor, ExtractionField, ExtractionSchema,
    ExtractionTransform, LinkInfo, PageContent,
};
use browseros_page::frame::{FrameNode, FrameTree};
use browseros_page::lifecycle::{PageState, PageStateError};
use browseros_page::navigation::{NavigationEntry, NavigationHistory};
use browseros_page::waiter::PageWaiter;
use browseros_page::WaitCondition;

// ─── Page Lifecycle Tests ─────────────────────────────────────────────────

#[test]
fn test_lifecycle_full_cycle() {
    let mut state = PageState::Creating;
    state = state.transition_to(PageState::Loading).unwrap();
    state = state.transition_to(PageState::Interactive).unwrap();
    state = state.transition_to(PageState::Complete).unwrap();
    state = state.transition_to(PageState::Closing).unwrap();
    state = state.transition_to(PageState::Closed).unwrap();
    assert_eq!(state, PageState::Closed);
}

#[test]
fn test_lifecycle_recovery() {
    let mut state = PageState::Creating;
    state = state.transition_to(PageState::Loading).unwrap();
    state = state.transition_to(PageState::Crashed).unwrap();
    assert!(state.is_failure());
    state = state.transition_to(PageState::Creating).unwrap();
    assert!(state.is_active());
}

#[test]
fn test_lifecycle_invalid_transition() {
    let result = PageState::Creating.transition_to(PageState::Closed);
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        PageStateError::InvalidTransition { .. }
    ));
}

#[test]
fn test_lifecycle_all_transitions_count() {
    let all_states = [
        PageState::Creating,
        PageState::Loading,
        PageState::Interactive,
        PageState::Complete,
        PageState::Closing,
        PageState::Closed,
        PageState::Crashed,
    ];
    let mut allowed = 0usize;
    let mut disallowed = 0usize;
    for from in &all_states {
        for to in &all_states {
            if from.can_transition_to(*to) {
                allowed += 1;
            } else {
                disallowed += 1;
            }
        }
    }
    assert_eq!(allowed, 13, "must have exactly 13 allowed transitions");
    assert_eq!(
        disallowed,
        49 - 13,
        "must have exactly 36 disallowed transitions"
    );
}

// ─── Navigation History Tests ─────────────────────────────────────────────

#[test]
fn test_navigation_history_basic() {
    let mut hist = NavigationHistory::new(50);
    hist.push(NavigationEntry::new(
        "https://page1.com".into(),
        "Page 1".into(),
        NavigationId::new(),
    ));
    hist.push(NavigationEntry::new(
        "https://page2.com".into(),
        "Page 2".into(),
        NavigationId::new(),
    ));
    assert_eq!(hist.len(), 2);
    assert_eq!(hist.current().unwrap().url, "https://page2.com");
    assert!(hist.can_go_back());
    assert!(!hist.can_go_forward());
}

#[test]
fn test_navigation_back_forward() {
    let mut hist = NavigationHistory::new(50);
    hist.push(NavigationEntry::new(
        "https://a.com".into(),
        "A".into(),
        NavigationId::new(),
    ));
    hist.push(NavigationEntry::new(
        "https://b.com".into(),
        "B".into(),
        NavigationId::new(),
    ));
    hist.push(NavigationEntry::new(
        "https://c.com".into(),
        "C".into(),
        NavigationId::new(),
    ));

    assert_eq!(hist.go_back().unwrap().url, "https://b.com");
    assert_eq!(hist.go_back().unwrap().url, "https://a.com");
    assert!(!hist.can_go_back());

    assert_eq!(hist.go_forward().unwrap().url, "https://b.com");
    assert_eq!(hist.go_forward().unwrap().url, "https://c.com");
    assert!(!hist.can_go_forward());
}

#[test]
fn test_navigation_push_truncates_forward() {
    let mut hist = NavigationHistory::new(50);
    hist.push(NavigationEntry::new(
        "https://a.com".into(),
        "A".into(),
        NavigationId::new(),
    ));
    hist.push(NavigationEntry::new(
        "https://b.com".into(),
        "B".into(),
        NavigationId::new(),
    ));
    hist.push(NavigationEntry::new(
        "https://c.com".into(),
        "C".into(),
        NavigationId::new(),
    ));
    hist.go_back();
    hist.go_back();
    hist.push(NavigationEntry::new(
        "https://d.com".into(),
        "D".into(),
        NavigationId::new(),
    ));

    assert_eq!(hist.len(), 2);
    assert_eq!(hist.current().unwrap().url, "https://d.com");
}

#[test]
fn test_navigation_clear() {
    let mut hist = NavigationHistory::new(50);
    hist.push(NavigationEntry::new(
        "https://a.com".into(),
        "A".into(),
        NavigationId::new(),
    ));
    hist.clear();
    assert!(hist.is_empty());
    assert!(hist.current().is_none());
}

#[test]
fn test_navigation_max_entries() {
    let mut hist = NavigationHistory::new(2);
    hist.push(NavigationEntry::new(
        "https://a.com".into(),
        "A".into(),
        NavigationId::new(),
    ));
    hist.push(NavigationEntry::new(
        "https://b.com".into(),
        "B".into(),
        NavigationId::new(),
    ));
    hist.push(NavigationEntry::new(
        "https://c.com".into(),
        "C".into(),
        NavigationId::new(),
    ));
    assert_eq!(hist.len(), 2);
    assert_eq!(hist.current().unwrap().url, "https://c.com");
}

// ─── Frame Tree Tests ─────────────────────────────────────────────────────

#[test]
fn test_frame_tree_attach_detach() {
    let mut tree = FrameTree::new();
    let main_id = FrameId::new();
    tree.set_main_frame(FrameNode::new_main(main_id, "https://example.com".into()));

    let child_id = FrameId::new();
    tree.attach(
        main_id,
        FrameNode::new_child(child_id, "https://example.com/iframe".into(), main_id),
    );

    assert!(tree.has(child_id));
    assert_eq!(tree.len(), 2);

    tree.detach(child_id);
    assert!(!tree.has(child_id));
    assert_eq!(tree.len(), 1);
}

#[test]
fn test_frame_tree_depth() {
    let mut tree = FrameTree::new();
    let main_id = FrameId::new();
    tree.set_main_frame(FrameNode::new_main(main_id, "https://example.com".into()));

    let child_id = FrameId::new();
    tree.attach(
        main_id,
        FrameNode::new_child(child_id, "https://example.com/iframe".into(), main_id),
    );

    let grandchild_id = FrameId::new();
    tree.attach(
        child_id,
        FrameNode::new_child(
            grandchild_id,
            "https://example.com/deep-iframe".into(),
            child_id,
        ),
    );

    assert_eq!(tree.depth(main_id), Some(0));
    assert_eq!(tree.depth(child_id), Some(1));
    assert_eq!(tree.depth(grandchild_id), Some(2));
}

#[test]
fn test_frame_tree_get_and_has() {
    let mut tree = FrameTree::new();
    let main_id = FrameId::new();
    tree.set_main_frame(FrameNode::new_main(main_id, "https://example.com".into()));

    let node = tree.get(main_id);
    assert!(node.is_some());
    assert!(node.unwrap().is_main_frame);

    assert!(!tree.has(FrameId::new()));
}

#[test]
fn test_frame_tree_detach_main_clears_all() {
    let mut tree = FrameTree::new();
    let main_id = FrameId::new();
    tree.set_main_frame(FrameNode::new_main(main_id, "https://example.com".into()));
    let child_id = FrameId::new();
    tree.attach(
        main_id,
        FrameNode::new_child(child_id, "https://example.com/iframe".into(), main_id),
    );

    tree.detach(main_id);
    assert!(tree.is_empty());
    assert!(!tree.has(main_id));
    assert!(!tree.has(child_id));
}

#[test]
fn test_frame_tree_traverse() {
    let mut tree = FrameTree::new();
    let main_id = FrameId::new();
    tree.set_main_frame(FrameNode::new_main(main_id, "https://example.com".into()));
    let child_id = FrameId::new();
    tree.attach(
        main_id,
        FrameNode::new_child(child_id, "https://example.com/iframe".into(), main_id),
    );

    let mut visited = Vec::new();
    tree.traverse(&mut |n: &FrameNode| visited.push(n.frame_id));
    assert_eq!(visited.len(), 2);
    assert!(visited.contains(&main_id));
    assert!(visited.contains(&child_id));
}

#[test]
fn test_frame_node_find_mut() {
    let mut main = FrameNode::new_main(FrameId::new(), "https://example.com".into());
    let child_id = FrameId::new();
    main.add_child(FrameNode::new_child(
        child_id,
        "https://example.com/iframe".into(),
        main.frame_id,
    ));

    let found = main.find_mut(child_id);
    assert!(found.is_some());
    found.unwrap().url = "https://updated.com".into();
    assert_eq!(main.find(child_id).unwrap().url, "https://updated.com");
}

// ─── Event Tests ──────────────────────────────────────────────────────────

#[test]
fn test_navigation_events_kind() {
    let meta = make_meta();
    let ev = NavigationStarted {
        metadata: meta.clone(),
        page_id: PageId::new(),
        url: "https://example.com".into(),
        navigation_id: NavigationId::new(),
    };
    assert_eq!(ev.kind(), "navigation.started");

    let ev = NavigationCommitted {
        metadata: meta.clone(),
        page_id: PageId::new(),
        url: "https://example.com".into(),
        navigation_id: NavigationId::new(),
        status_code: 200,
        is_same_document: false,
    };
    assert_eq!(ev.kind(), "navigation.committed");

    let ev = NavigationFinished {
        metadata: meta.clone(),
        page_id: PageId::new(),
        url: "https://example.com".into(),
        navigation_id: NavigationId::new(),
        status_code: 200,
        load_time_ms: 1000,
        dom_content_loaded_ms: 500,
        success: true,
        error_text: None,
    };
    assert_eq!(ev.kind(), "navigation.finished");
}

#[test]
fn test_frame_events_kind() {
    let meta = make_meta();
    let ev = FrameAttached {
        metadata: meta.clone(),
        frame_id: FrameId::new(),
        page_id: PageId::new(),
        parent_frame_id: None,
        url: "https://example.com".into(),
    };
    assert_eq!(ev.kind(), "frame.attached");

    let ev = FrameDetached {
        metadata: meta.clone(),
        frame_id: FrameId::new(),
        page_id: PageId::new(),
    };
    assert_eq!(ev.kind(), "frame.detached");
}

#[test]
fn test_dialog_events_kind() {
    let meta = make_meta();
    let ev = DialogOpened {
        metadata: meta.clone(),
        page_id: PageId::new(),
        dialog_type: "alert".into(),
        message: "Hello".into(),
        default_value: None,
    };
    assert_eq!(ev.kind(), "dialog.opened");

    let ev = DialogClosed {
        metadata: meta.clone(),
        page_id: PageId::new(),
        dialog_type: "confirm".into(),
        accepted: true,
    };
    assert_eq!(ev.kind(), "dialog.closed");
}

#[test]
fn test_page_state_event() {
    let meta = make_meta();
    let ev = PageLoadState {
        metadata: meta.clone(),
        page_id: PageId::new(),
        state: LoadState::NetworkIdle,
    };
    assert_eq!(ev.kind(), "page.load_state");
    assert!(matches!(ev.state, LoadState::NetworkIdle));
}

#[test]
fn test_title_changed_event() {
    let meta = make_meta();
    let ev = TitleChanged {
        metadata: meta.clone(),
        page_id: PageId::new(),
        title: "New Title".into(),
    };
    assert_eq!(ev.kind(), "page.title_changed");
    assert_eq!(ev.title, "New Title");
}

#[test]
fn test_page_url_changed_event() {
    let meta = make_meta();
    let ev = PageUrlChanged {
        metadata: meta.clone(),
        page_id: PageId::new(),
        url: "https://example.com/new".into(),
    };
    assert_eq!(ev.kind(), "page.url_changed");
    assert_eq!(ev.url, "https://example.com/new");
}

#[test]
fn test_all_events_are_domain_events() {
    fn assert_domain<E: DomainEvent>() {}
    assert_domain::<NavigationStarted>();
    assert_domain::<NavigationCommitted>();
    assert_domain::<NavigationFinished>();
    assert_domain::<TitleChanged>();
    assert_domain::<PageUrlChanged>();
    assert_domain::<PageLoadState>();
    assert_domain::<FrameAttached>();
    assert_domain::<FrameDetached>();
    assert_domain::<FrameNavigationStarted>();
    assert_domain::<FrameNavigationFinished>();
    assert_domain::<DialogOpened>();
    assert_domain::<DialogClosed>();
}

#[test]
fn test_event_bus_publish_receive() {
    use std::sync::Mutex;

    let bus = EventBus::new();
    let received = Arc::new(Mutex::new(Vec::new()));
    let r = received.clone();

    let handler: Arc<dyn browseros_event_bus::EventHandler> = Arc::new(move |ev: &dyn Event| {
        r.lock().unwrap().push(ev.kind().to_owned());
    });
    bus.subscribe("navigation.finished", handler);

    let meta = make_meta();
    bus.publish(Box::new(NavigationFinished {
        metadata: meta,
        page_id: PageId::new(),
        url: "https://example.com".into(),
        navigation_id: NavigationId::new(),
        status_code: 200,
        load_time_ms: 1000,
        dom_content_loaded_ms: 500,
        success: true,
        error_text: None,
    }));

    let kinds = received.lock().unwrap();
    assert_eq!(kinds.len(), 1);
    assert_eq!(kinds[0], "navigation.finished");
}

// ─── Content Extractor Tests ──────────────────────────────────────────────

#[test]
fn test_extract_links_with_attrs() {
    let link = LinkInfo {
        url: "https://example.com".into(),
        text: "Example".into(),
        title: Some("Example Title".into()),
    };
    assert_eq!(link.url, "https://example.com");
    assert_eq!(link.text, "Example");
    assert_eq!(link.title.as_deref(), Some("Example Title"));
}

#[test]
fn test_extraction_field_defaults() {
    let field = ExtractionField::new("price", ".price");
    assert_eq!(field.name, "price");
    assert_eq!(field.selector, ".price");
    assert!(field.attribute.is_none());
    assert!(field.transform.is_none());
}

#[test]
fn test_extraction_field_chaining() {
    let field = ExtractionField::new("link", "a")
        .with_attribute("href")
        .with_transform(ExtractionTransform::Attribute("href".into()));
    assert_eq!(field.attribute.as_deref(), Some("href"));
    assert!(matches!(
        field.transform,
        Some(ExtractionTransform::Attribute(ref a)) if a == "href"
    ));
}

#[test]
fn test_extraction_schema_new() {
    let fields = vec![ExtractionField::new("title", "h1")];
    let schema = ExtractionSchema::new(fields);
    assert_eq!(schema.fields.len(), 1);
}

#[test]
fn test_page_content_builder() {
    let content = PageContent::new("https://example.com".into(), "Test".into());
    assert_eq!(content.url, "https://example.com");
    assert_eq!(content.title, "Test");
    assert!(content.text.is_empty());
    assert!(content.html.is_empty());
    assert!(content.screenshots_taken.is_empty());
    assert!(content.metadata.is_empty());
}

#[test]
fn test_content_extractor_trait_object() {
    let extractor: Box<dyn ContentExtractor> = Box::new(DefaultContentExtractor::new());
    assert!(extractor.extract_text(&make_test_page()).is_ok());
}

// ─── Dialog Auto-Handler Tests ────────────────────────────────────────────

#[test]
fn test_dialog_handler_lifecycle() {
    let page = Arc::new(TestDialogPage::new());
    let bus = EventBus::new();
    let mut handler = DialogAutoHandler::new(page, &bus);
    assert!(!handler.is_active());

    handler.auto_handle(DialogStrategy::Accept).unwrap();
    assert!(handler.is_active());

    handler.stop_auto_handle().unwrap();
    assert!(!handler.is_active());
}

#[test]
fn test_dialog_double_start_error() {
    let page = Arc::new(TestDialogPage::new());
    let bus = EventBus::new();
    let mut handler = DialogAutoHandler::new(page, &bus);
    handler.auto_handle(DialogStrategy::Accept).unwrap();
    let result = handler.auto_handle(DialogStrategy::Dismiss);
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), DialogError::AlreadyHandling));
}

#[test]
fn test_dialog_stop_without_start_error() {
    let page = Arc::new(TestDialogPage::new());
    let bus = EventBus::new();
    let mut handler = DialogAutoHandler::new(page, &bus);
    let result = handler.stop_auto_handle();
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), DialogError::NotHandling));
}

// ─── Frame Event Tests ────────────────────────────────────────────────────

#[test]
fn test_frame_navigation_events() {
    let meta = make_meta();
    let ev = FrameNavigationStarted {
        metadata: meta.clone(),
        frame_id: FrameId::new(),
        page_id: PageId::new(),
        url: "https://example.com".into(),
    };
    assert_eq!(ev.kind(), "frame.navigation.started");

    let ev = FrameNavigationFinished {
        metadata: meta.clone(),
        frame_id: FrameId::new(),
        page_id: PageId::new(),
        url: "https://example.com".into(),
        success: true,
    };
    assert_eq!(ev.kind(), "frame.navigation.finished");
    assert!(ev.success);
}

#[test]
fn test_frame_navigation_failed() {
    let meta = make_meta();
    let ev = FrameNavigationFinished {
        metadata: meta,
        frame_id: FrameId::new(),
        page_id: PageId::new(),
        url: "https://example.com".into(),
        success: false,
    };
    assert!(!ev.success);
}

// ─── Load State Tests ─────────────────────────────────────────────────────

#[test]
fn test_load_state_variants() {
    assert_eq!(LoadState::Loading.as_str(), "loading");
    assert_eq!(LoadState::DomContentLoaded.as_str(), "domcontentloaded");
    assert_eq!(LoadState::Loaded.as_str(), "loaded");
    assert_eq!(LoadState::NetworkAlmostIdle.as_str(), "networkalmostidle");
    assert_eq!(LoadState::NetworkIdle.as_str(), "networkidle");
}

#[test]
fn test_load_state_clone_eq() {
    assert_eq!(LoadState::Loading, LoadState::Loading);
    assert_ne!(LoadState::Loading, LoadState::Loaded);
}

// ─── Navigation Idempotency ───────────────────────────────────────────────

#[test]
fn test_navigation_empty_go_back_forward() {
    let mut hist = NavigationHistory::new(50);
    assert!(hist.go_back().is_none());
    assert!(hist.go_forward().is_none());
}

#[test]
fn test_navigation_current_mut() {
    let mut hist = NavigationHistory::new(50);
    hist.push(NavigationEntry::new(
        "https://a.com".into(),
        "A".into(),
        NavigationId::new(),
    ));
    assert!(hist.current_mut().is_some());
    hist.current_mut().unwrap().title = "Modified".into();
    assert_eq!(hist.current().unwrap().title, "Modified");
}

#[test]
fn test_navigation_current_mut_empty() {
    let mut hist = NavigationHistory::new(50);
    assert!(hist.current_mut().is_none());
}

#[test]
fn test_navigation_set_max_entries_truncates() {
    let mut hist = NavigationHistory::new(50);
    hist.push(NavigationEntry::new(
        "https://a.com".into(),
        "A".into(),
        NavigationId::new(),
    ));
    hist.push(NavigationEntry::new(
        "https://b.com".into(),
        "B".into(),
        NavigationId::new(),
    ));
    hist.push(NavigationEntry::new(
        "https://c.com".into(),
        "C".into(),
        NavigationId::new(),
    ));
    hist.set_max_entries(1);
    assert_eq!(hist.len(), 1);
    assert_eq!(hist.current().unwrap().url, "https://c.com");
}

#[test]
fn test_navigation_navigate_to_index() {
    let mut hist = NavigationHistory::new(50);
    hist.push(NavigationEntry::new(
        "https://a.com".into(),
        "A".into(),
        NavigationId::new(),
    ));
    hist.push(NavigationEntry::new(
        "https://b.com".into(),
        "B".into(),
        NavigationId::new(),
    ));
    let entry = hist.navigate_to_index(0);
    assert_eq!(entry.unwrap().url, "https://a.com");
    assert_eq!(hist.current_index(), 0);
}

#[test]
fn test_navigation_navigate_to_invalid_index() {
    let mut hist = NavigationHistory::new(50);
    hist.push(NavigationEntry::new(
        "https://a.com".into(),
        "A".into(),
        NavigationId::new(),
    ));
    assert!(hist.navigate_to_index(5).is_none());
}

#[test]
fn test_navigation_entries_slice() {
    let mut hist = NavigationHistory::new(50);
    hist.push(NavigationEntry::new(
        "https://a.com".into(),
        "A".into(),
        NavigationId::new(),
    ));
    hist.push(NavigationEntry::new(
        "https://b.com".into(),
        "B".into(),
        NavigationId::new(),
    ));
    assert_eq!(hist.entries().len(), 2);
}

#[test]
fn test_navigation_default_max_50() {
    let hist = NavigationHistory::default();
    assert_eq!(hist.max_entries(), 50);
}

// ─── Frame Tree Empty/Default ─────────────────────────────────────────────

#[test]
fn test_frame_tree_default_is_empty() {
    let tree = FrameTree::default();
    assert!(tree.is_empty());
    assert!(tree.main_frame().is_none());
    assert!(tree.main_frame_id().is_none());
}

#[test]
fn test_detach_nonexistent_frame() {
    let mut tree = FrameTree::new();
    tree.set_main_frame(FrameNode::new_main(
        FrameId::new(),
        "https://example.com".into(),
    ));
    assert!(tree.detach(FrameId::new()).is_none());
}

#[test]
fn test_attach_to_nonexistent_parent() {
    let mut tree = FrameTree::new();
    tree.set_main_frame(FrameNode::new_main(
        FrameId::new(),
        "https://example.com".into(),
    ));
    tree.attach(
        FrameId::new(),
        FrameNode::new_child(FrameId::new(), "https://example.com".into(), FrameId::new()),
    );
    assert_eq!(tree.len(), 1);
}

#[test]
fn test_frame_node_all_frames() {
    let mut main = FrameNode::new_main(FrameId::new(), "https://example.com".into());
    main.add_child(FrameNode::new_child(
        FrameId::new(),
        "https://child.com".into(),
        main.frame_id,
    ));
    main.add_child(FrameNode::new_child(
        FrameId::new(),
        "https://child2.com".into(),
        main.frame_id,
    ));
    assert_eq!(main.all_frames().len(), 3);
}

#[test]
fn test_frame_node_count_descendants() {
    let mut main = FrameNode::new_main(FrameId::new(), "https://example.com".into());
    main.add_child(FrameNode::new_child(
        FrameId::new(),
        "https://child.com".into(),
        main.frame_id,
    ));
    assert_eq!(main.count_descendants(), 1);
}

#[test]
fn test_wait_condition_navigation() {
    let cond = WaitCondition::Navigation(std::time::Duration::from_secs(30));
    assert!(matches!(cond, WaitCondition::Navigation(_)));
}

#[test]
fn test_wait_condition_compound() {
    let cond = WaitCondition::All(vec![
        WaitCondition::Navigation(std::time::Duration::from_secs(30)),
        WaitCondition::NetworkIdle(std::time::Duration::from_secs(5)),
    ]);
    assert!(matches!(cond, WaitCondition::All(_)));
}

#[test]
fn test_wait_condition_any() {
    let cond = WaitCondition::Any(vec![
        WaitCondition::Url("https://example.com".into()),
        WaitCondition::Title("Example".into()),
    ]);
    assert!(matches!(cond, WaitCondition::Any(_)));
}

// ─── Event Metadata Tests ─────────────────────────────────────────────────

#[test]
fn test_event_metadata_correlation_preserved() {
    use browseros_types::identifiers::{CorrelationId, ModuleId};
    use browseros_types::value::{ContentType, SemVer};
    use chrono::Utc;

    let corr_id = CorrelationId::new();
    let meta = EventMetadata::new(
        ModuleId::new("browseros-page", SemVer::new(0, 1, 0)),
        corr_id,
        None,
        ContentType::new("app/json"),
        Utc::now(),
    );
    let ev = NavigationStarted {
        metadata: meta.clone(),
        page_id: PageId::new(),
        url: "https://example.com".into(),
        navigation_id: NavigationId::new(),
    };
    assert_eq!(ev.metadata().correlation_id, corr_id);
}

// ─── Dialog Error Display ─────────────────────────────────────────────────

#[test]
fn test_dialog_error_display() {
    assert_eq!(
        DialogError::AlreadyHandling.to_string(),
        "already auto-handling dialogs"
    );
    assert_eq!(
        DialogError::NotHandling.to_string(),
        "not currently auto-handling dialogs"
    );
}

// ─── PageState Display ────────────────────────────────────────────────────

#[test]
fn test_page_state_display_all() {
    assert_eq!(PageState::Creating.to_string(), "creating");
    assert_eq!(PageState::Loading.to_string(), "loading");
    assert_eq!(PageState::Interactive.to_string(), "interactive");
    assert_eq!(PageState::Complete.to_string(), "complete");
    assert_eq!(PageState::Closing.to_string(), "closing");
    assert_eq!(PageState::Closed.to_string(), "closed");
    assert_eq!(PageState::Crashed.to_string(), "crashed");
}

// ─── NavigationEntry Construction ─────────────────────────────────────────

#[test]
fn test_navigation_entry_new() {
    let nav_id = NavigationId::new();
    let entry = NavigationEntry::new("https://example.com".into(), "Example".into(), nav_id);
    assert_eq!(entry.url, "https://example.com");
    assert_eq!(entry.title, "Example");
    assert_eq!(entry.navigation_id, nav_id);
}

// ─── FrameNode Construction ───────────────────────────────────────────────

#[test]
fn test_frame_node_new_main() {
    let id = FrameId::new();
    let node = FrameNode::new_main(id, "https://example.com".into());
    assert!(node.is_main_frame);
    assert!(node.parent_id.is_none());
    assert!(node.children.is_empty());
}

#[test]
fn test_frame_node_new_child() {
    let id = FrameId::new();
    let parent_id = FrameId::new();
    let node = FrameNode::new_child(id, "https://child.com".into(), parent_id);
    assert!(!node.is_main_frame);
    assert_eq!(node.parent_id, Some(parent_id));
}

// ─── Replace Frame ────────────────────────────────────────────────────────

#[test]
fn test_replace_main_frame_updates_url() {
    let mut tree = FrameTree::new();
    let main_id = FrameId::new();
    tree.set_main_frame(FrameNode::new_main(main_id, "https://old.com".into()));

    let new_node = FrameNode::new_main(main_id, "https://new.com".into());
    tree.replace(main_id, new_node);
    assert_eq!(tree.get(main_id).unwrap().url, "https://new.com");
}

#[test]
fn test_replace_nonexistent_returns_none() {
    let mut tree = FrameTree::new();
    tree.set_main_frame(FrameNode::new_main(
        FrameId::new(),
        "https://example.com".into(),
    ));
    let result = tree.replace(
        FrameId::new(),
        FrameNode::new_main(FrameId::new(), "https://new.com".into()),
    );
    assert!(result.is_none());
}

// ─── PageWaiter Compile-check ─────────────────────────────────────────────

#[test]
fn test_page_waiter_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<PageWaiter>();
}

// ─── DefaultContentExtractor Traits ───────────────────────────────────────

#[test]
fn test_default_content_extractor_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<DefaultContentExtractor>();
}

// ─── DialogStrategy Traits ────────────────────────────────────────────────

#[test]
fn test_dialog_strategy_clone_copy() {
    let s1 = DialogStrategy::Accept;
    let s2 = s1;
    assert_eq!(s1, s2);
    assert!(!matches!((s1, s2), (DialogStrategy::Dismiss, _)));
}

// ─── Helpers ──────────────────────────────────────────────────────────────

fn make_meta() -> EventMetadata {
    use browseros_types::identifiers::{CorrelationId, ModuleId};
    use browseros_types::value::{ContentType, SemVer};
    use chrono::Utc;
    EventMetadata::new(
        ModuleId::new("browseros-page", SemVer::new(0, 1, 0)),
        CorrelationId::new(),
        None,
        ContentType::new("application/x-browseros-event"),
        Utc::now(),
    )
}

fn make_test_page() -> TestContentPage {
    TestContentPage::new("<html><body><p>Hello</p></body></html>", "Test Page")
}

// ─── Test Page Implementations (used by integration tests) ────────────────

use std::sync::Mutex;

use browseros_bridge::error::BridgeResult;
use browseros_bridge::traits::{
    DialogPort, DownloadPort, ElementPort, FramePort, InputPort, LocatorPort, NetworkPort,
    StoragePort,
};
use browseros_bridge::types::NavigationStatus;
use browseros_bridge::types::{
    DialogInfo, JsResult, NavigationState, PdfOptions, ScreenshotOptions, Viewport,
};
use browseros_bridge::PagePort;

fn nav_finished(url: impl Into<String>) -> NavigationState {
    NavigationState {
        navigation_id: NavigationId::new(),
        url: url.into(),
        status: NavigationStatus::Finished,
    }
}

struct TestDialogPage {
    page_id: PageId,
    accepted: Mutex<bool>,
    dismissed: Mutex<bool>,
}

impl TestDialogPage {
    fn new() -> Self {
        TestDialogPage {
            page_id: PageId::new(),
            accepted: Mutex::new(false),
            dismissed: Mutex::new(false),
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
    fn navigate(&self, url: &str) -> BridgeResult<NavigationState> {
        Ok(nav_finished(url))
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
    fn evaluate(&self, _script: &str, _arg: Option<&serde_json::Value>) -> BridgeResult<JsResult> {
        Ok(JsResult {
            value: serde_json::Value::Null,
            exception_details: None,
        })
    }
    fn evaluate_handle(
        &self,
        _s: &str,
        _a: Option<&serde_json::Value>,
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
        Box::new(TestDialogPortImpl {
            accepted: Mutex::new(*self.accepted.lock().unwrap()),
            dismissed: Mutex::new(*self.dismissed.lock().unwrap()),
        })
    }
    fn download(&self) -> Box<dyn DownloadPort> {
        panic!("no download")
    }
}

struct TestDialogPortImpl {
    accepted: Mutex<bool>,
    dismissed: Mutex<bool>,
}

impl DialogPort for TestDialogPortImpl {
    fn next(&self) -> BridgeResult<Option<DialogInfo>> {
        Ok(None)
    }
    fn accept(&self, _prompt_text: Option<&str>) -> BridgeResult<()> {
        *self.accepted.lock().unwrap() = true;
        Ok(())
    }
    fn dismiss(&self) -> BridgeResult<()> {
        *self.dismissed.lock().unwrap() = true;
        Ok(())
    }
}

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
    fn navigate(&self, url: &str) -> BridgeResult<NavigationState> {
        Ok(nav_finished(url))
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
    fn evaluate(&self, _script: &str, _arg: Option<&serde_json::Value>) -> BridgeResult<JsResult> {
        Ok(JsResult {
            value: serde_json::Value::Null,
            exception_details: None,
        })
    }
    fn evaluate_handle(
        &self,
        _s: &str,
        _a: Option<&serde_json::Value>,
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
    fn set_content(&self, html: &str) -> BridgeResult<()> {
        *self.html.lock().unwrap() = html.to_owned();
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
