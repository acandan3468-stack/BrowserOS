use std::any::Any;

use browseros_bridge::identifiers::{FrameId, NavigationId, PageId};
use browseros_types::event::{DomainEvent, Event, EventCategory, EventMetadata};

macro_rules! domain_event {
    ($name:ident { $($field:ident: $ty:ty),* $(,)? }, $kind:expr) => {
        #[derive(Debug, Clone)]
        pub struct $name {
            pub metadata: EventMetadata,
            $(pub $field: $ty),*
        }

        impl Event for $name {
            fn kind(&self) -> &'static str { $kind }
            fn category(&self) -> EventCategory { EventCategory::Domain }
            fn metadata(&self) -> &EventMetadata { &self.metadata }
            fn as_any(&self) -> &dyn Any { self }
        }

        impl DomainEvent for $name {}
    };
}

domain_event!(
    NavigationStarted {
        page_id: PageId,
        url: String,
        navigation_id: NavigationId,
    },
    "navigation.started"
);

domain_event!(
    NavigationCommitted {
        page_id: PageId,
        url: String,
        navigation_id: NavigationId,
        status_code: u16,
        is_same_document: bool,
    },
    "navigation.committed"
);

domain_event!(
    NavigationFinished {
        page_id: PageId,
        url: String,
        navigation_id: NavigationId,
        status_code: u16,
        load_time_ms: u64,
        dom_content_loaded_ms: u64,
        success: bool,
        error_text: Option<String>,
    },
    "navigation.finished"
);

domain_event!(
    TitleChanged {
        page_id: PageId,
        title: String,
    },
    "page.title_changed"
);

domain_event!(
    PageUrlChanged {
        page_id: PageId,
        url: String,
    },
    "page.url_changed"
);

domain_event!(
    PageLoadState {
        page_id: PageId,
        state: LoadState,
    },
    "page.load_state"
);

domain_event!(
    FrameAttached {
        frame_id: FrameId,
        page_id: PageId,
        parent_frame_id: Option<FrameId>,
        url: String,
    },
    "frame.attached"
);

domain_event!(
    FrameNavigationStarted {
        frame_id: FrameId,
        page_id: PageId,
        url: String,
    },
    "frame.navigation.started"
);

domain_event!(
    FrameNavigationFinished {
        frame_id: FrameId,
        page_id: PageId,
        url: String,
        success: bool,
    },
    "frame.navigation.finished"
);

domain_event!(
    FrameDetached {
        frame_id: FrameId,
        page_id: PageId,
    },
    "frame.detached"
);

domain_event!(
    DialogOpened {
        page_id: PageId,
        dialog_type: String,
        message: String,
        default_value: Option<String>,
    },
    "dialog.opened"
);

domain_event!(
    DialogClosed {
        page_id: PageId,
        dialog_type: String,
        accepted: bool,
    },
    "dialog.closed"
);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LoadState {
    Loading,
    DomContentLoaded,
    Loaded,
    NetworkAlmostIdle,
    NetworkIdle,
}

impl LoadState {
    pub fn as_str(&self) -> &'static str {
        match self {
            LoadState::Loading => "loading",
            LoadState::DomContentLoaded => "domcontentloaded",
            LoadState::Loaded => "loaded",
            LoadState::NetworkAlmostIdle => "networkalmostidle",
            LoadState::NetworkIdle => "networkidle",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use browseros_types::identifiers::{CorrelationId, ModuleId};
    use browseros_types::value::{ContentType, SemVer};
    use chrono::Utc;

    fn test_metadata() -> EventMetadata {
        EventMetadata::new(
            ModuleId::new("browseros-page", SemVer::new(0, 1, 0)),
            CorrelationId::new(),
            None,
            ContentType::new("application/x-browseros-event"),
            Utc::now(),
        )
    }

    #[test]
    fn test_navigation_started_kind() {
        let ev = NavigationStarted {
            metadata: test_metadata(),
            page_id: PageId::new(),
            url: "https://example.com".into(),
            navigation_id: NavigationId::new(),
        };
        assert_eq!(ev.kind(), "navigation.started");
        assert_eq!(ev.category(), EventCategory::Domain);
    }

    #[test]
    fn test_navigation_committed_kind() {
        let ev = NavigationCommitted {
            metadata: test_metadata(),
            page_id: PageId::new(),
            url: "https://example.com".into(),
            navigation_id: NavigationId::new(),
            status_code: 200,
            is_same_document: false,
        };
        assert_eq!(ev.kind(), "navigation.committed");
    }

    #[test]
    fn test_navigation_finished_kind() {
        let ev = NavigationFinished {
            metadata: test_metadata(),
            page_id: PageId::new(),
            url: "https://example.com".into(),
            navigation_id: NavigationId::new(),
            status_code: 200,
            load_time_ms: 1500,
            dom_content_loaded_ms: 800,
            success: true,
            error_text: None,
        };
        assert_eq!(ev.kind(), "navigation.finished");
    }

    #[test]
    fn test_title_changed_kind() {
        let ev = TitleChanged {
            metadata: test_metadata(),
            page_id: PageId::new(),
            title: "New Title".into(),
        };
        assert_eq!(ev.kind(), "page.title_changed");
    }

    #[test]
    fn test_page_url_changed_kind() {
        let ev = PageUrlChanged {
            metadata: test_metadata(),
            page_id: PageId::new(),
            url: "https://example.com".into(),
        };
        assert_eq!(ev.kind(), "page.url_changed");
    }

    #[test]
    fn test_page_load_state_kind() {
        let ev = PageLoadState {
            metadata: test_metadata(),
            page_id: PageId::new(),
            state: LoadState::Loaded,
        };
        assert_eq!(ev.kind(), "page.load_state");
    }

    #[test]
    fn test_frame_attached_kind() {
        let ev = FrameAttached {
            metadata: test_metadata(),
            frame_id: FrameId::new(),
            page_id: PageId::new(),
            parent_frame_id: None,
            url: "https://example.com".into(),
        };
        assert_eq!(ev.kind(), "frame.attached");
    }

    #[test]
    fn test_frame_navigation_started_kind() {
        let ev = FrameNavigationStarted {
            metadata: test_metadata(),
            frame_id: FrameId::new(),
            page_id: PageId::new(),
            url: "https://example.com".into(),
        };
        assert_eq!(ev.kind(), "frame.navigation.started");
    }

    #[test]
    fn test_frame_navigation_finished_kind() {
        let ev = FrameNavigationFinished {
            metadata: test_metadata(),
            frame_id: FrameId::new(),
            page_id: PageId::new(),
            url: "https://example.com".into(),
            success: true,
        };
        assert_eq!(ev.kind(), "frame.navigation.finished");
    }

    #[test]
    fn test_frame_detached_kind() {
        let ev = FrameDetached {
            metadata: test_metadata(),
            frame_id: FrameId::new(),
            page_id: PageId::new(),
        };
        assert_eq!(ev.kind(), "frame.detached");
    }

    #[test]
    fn test_dialog_opened_kind() {
        let ev = DialogOpened {
            metadata: test_metadata(),
            page_id: PageId::new(),
            dialog_type: "alert".into(),
            message: "Hello".into(),
            default_value: None,
        };
        assert_eq!(ev.kind(), "dialog.opened");
    }

    #[test]
    fn test_dialog_closed_kind() {
        let ev = DialogClosed {
            metadata: test_metadata(),
            page_id: PageId::new(),
            dialog_type: "confirm".into(),
            accepted: true,
        };
        assert_eq!(ev.kind(), "dialog.closed");
    }

    #[test]
    fn test_domain_event_marker() {
        fn assert_domain<E: DomainEvent>() {}
        assert_domain::<NavigationStarted>();
        assert_domain::<NavigationCommitted>();
        assert_domain::<NavigationFinished>();
        assert_domain::<TitleChanged>();
        assert_domain::<PageUrlChanged>();
        assert_domain::<PageLoadState>();
        assert_domain::<FrameAttached>();
        assert_domain::<FrameNavigationStarted>();
        assert_domain::<FrameNavigationFinished>();
        assert_domain::<FrameDetached>();
        assert_domain::<DialogOpened>();
        assert_domain::<DialogClosed>();
    }

    #[test]
    fn test_all_events_send_sync_static() {
        fn assert_bounds<E: Event>() {}
        assert_bounds::<NavigationStarted>();
        assert_bounds::<NavigationCommitted>();
        assert_bounds::<NavigationFinished>();
        assert_bounds::<TitleChanged>();
        assert_bounds::<PageUrlChanged>();
        assert_bounds::<PageLoadState>();
        assert_bounds::<FrameAttached>();
        assert_bounds::<FrameNavigationStarted>();
        assert_bounds::<FrameNavigationFinished>();
        assert_bounds::<FrameDetached>();
        assert_bounds::<DialogOpened>();
        assert_bounds::<DialogClosed>();
    }

    #[test]
    fn test_load_state_as_str() {
        assert_eq!(LoadState::Loading.as_str(), "loading");
        assert_eq!(LoadState::DomContentLoaded.as_str(), "domcontentloaded");
        assert_eq!(LoadState::Loaded.as_str(), "loaded");
        assert_eq!(LoadState::NetworkAlmostIdle.as_str(), "networkalmostidle");
        assert_eq!(LoadState::NetworkIdle.as_str(), "networkidle");
    }

    #[test]
    fn test_navigation_finished_error_text() {
        let ev = NavigationFinished {
            metadata: test_metadata(),
            page_id: PageId::new(),
            url: "https://example.com".into(),
            navigation_id: NavigationId::new(),
            status_code: 0,
            load_time_ms: 0,
            dom_content_loaded_ms: 0,
            success: false,
            error_text: Some("net::ERR_CONNECTION_REFUSED".into()),
        };
        assert_eq!(
            ev.error_text.as_deref(),
            Some("net::ERR_CONNECTION_REFUSED")
        );
    }

    #[test]
    fn test_navigation_committed_same_document() {
        let ev = NavigationCommitted {
            metadata: test_metadata(),
            page_id: PageId::new(),
            url: "https://example.com#section".into(),
            navigation_id: NavigationId::new(),
            status_code: 200,
            is_same_document: true,
        };
        assert!(ev.is_same_document);
    }

    #[test]
    fn test_frame_attached_with_parent() {
        let parent_id = FrameId::new();
        let ev = FrameAttached {
            metadata: test_metadata(),
            frame_id: FrameId::new(),
            page_id: PageId::new(),
            parent_frame_id: Some(parent_id),
            url: "https://example.com/iframe".into(),
        };
        assert!(ev.parent_frame_id.is_some());
    }

    #[test]
    fn test_dialog_opened_with_default() {
        let ev = DialogOpened {
            metadata: test_metadata(),
            page_id: PageId::new(),
            dialog_type: "prompt".into(),
            message: "Enter name:".into(),
            default_value: Some("John".into()),
        };
        assert_eq!(ev.default_value.as_deref(), Some("John"));
    }

    #[test]
    fn test_metadata_preserved() {
        let meta = test_metadata();
        let ev = NavigationStarted {
            metadata: meta.clone(),
            page_id: PageId::new(),
            url: "https://example.com".into(),
            navigation_id: NavigationId::new(),
        };
        assert_eq!(ev.metadata().id, meta.id);
        assert_eq!(ev.metadata().correlation_id, meta.correlation_id);
        assert_eq!(ev.metadata().source, meta.source);
        assert_eq!(
            ev.metadata().content_type,
            ContentType::new("application/x-browseros-event")
        );
    }
}
