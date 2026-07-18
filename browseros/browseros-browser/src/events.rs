use std::any::Any;

use browseros_bridge::identifiers::{BrowserId, PageId, SessionId};
use browseros_types::event::{DomainEvent, Event, EventCategory, EventMetadata};
use chrono::{DateTime, Utc};

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
    BrowserStarted {
        browser_id: BrowserId,
        version: String,
        executable: String,
        ws_endpoint: String,
    },
    "browser.started"
);

domain_event!(
    BrowserClosed {
        browser_id: BrowserId,
        exit_code: Option<i32>,
        reason: String,
    },
    "browser.closed"
);

domain_event!(
    BrowserCrashed {
        browser_id: BrowserId,
        crash_reason: String,
        dump_path: Option<String>,
        last_known_state: String,
    },
    "browser.crashed"
);

domain_event!(
    BrowserDisconnected {
        browser_id: BrowserId,
        last_known_state: String,
    },
    "browser.disconnected"
);

domain_event!(
    SessionCreated {
        session_id: SessionId,
        browser_id: BrowserId,
        incognito: bool,
        user_agent: Option<String>,
    },
    "session.created"
);

domain_event!(
    SessionClosed {
        session_id: SessionId,
        browser_id: BrowserId,
        page_count: u32,
    },
    "session.closed"
);

domain_event!(
    PageCreated {
        page_id: PageId,
        session_id: SessionId,
        url: String,
        about_blank: bool,
        created_at: DateTime<Utc>,
    },
    "page.created"
);

domain_event!(
    PageClosed {
        page_id: PageId,
        session_id: SessionId,
    },
    "page.closed"
);
