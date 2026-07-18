pub mod dialog;
pub mod events;
pub mod extractor;
pub mod frame;
pub mod lifecycle;
pub mod navigation;
pub mod waiter;

pub use browseros_bridge::types::WaitCondition;

pub use dialog::{DialogAutoHandler, DialogError, DialogStrategy};
pub use events::{
    DialogClosed, DialogOpened, FrameAttached, FrameDetached, FrameNavigationFinished,
    FrameNavigationStarted, LoadState, NavigationCommitted, NavigationFinished, NavigationStarted,
    PageLoadState, PageUrlChanged, TitleChanged,
};
pub use extractor::{
    ContentExtractor, DefaultContentExtractor, ExtractionField, ExtractionSchema,
    ExtractionTransform, LinkInfo, PageContent,
};
pub use frame::{FrameNode, FrameTree};
pub use lifecycle::{PageState, PageStateError};
pub use navigation::{NavigationEntry, NavigationHistory};
pub use waiter::PageWaiter;
