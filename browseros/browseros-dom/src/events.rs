use std::time::Duration;

use browseros_bridge::identifiers::{ArtifactId, FrameId, PageId};
use browseros_bridge::types::NodeType;
use browseros_types::identifiers::HandleId;

use crate::mutation_observer::{MutationRecord, MutationType};
use crate::shadow::ShadowRootMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomOperation {
    QuerySelector,
    QuerySelectorAll,
    XPathEvaluation,
    TextQuery,
    AttributeGet,
    AttributeSet,
    AttributeRemove,
    TextContent,
    InnerHtml,
    OuterHtml,
    Snapshot,
    ComputeStyle,
    BoundingBox,
    ScrollIntoView,
    Focus,
    MutationApply,
    TreeWalk,
    Evaluate,
    Locate,
}

#[derive(Debug, Clone)]
pub struct ElementCreatedPayload {
    pub page_id: PageId,
    pub frame_id: FrameId,
    pub handle_id: HandleId,
    pub tag_name: String,
    pub node_type: NodeType,
}

#[derive(Debug, Clone)]
pub struct ElementRemovedPayload {
    pub page_id: PageId,
    pub frame_id: FrameId,
    pub handle_id: HandleId,
    pub tag_name: String,
}

#[derive(Debug, Clone)]
pub struct ElementStalePayload {
    pub page_id: PageId,
    pub frame_id: FrameId,
    pub handle_id: HandleId,
    pub generation: u64,
}

#[derive(Debug, Clone)]
pub struct ElementReacquiredPayload {
    pub page_id: PageId,
    pub frame_id: FrameId,
    pub old_handle_id: HandleId,
    pub new_handle_id: HandleId,
}

#[derive(Debug, Clone)]
pub struct AttributeChangedPayload {
    pub page_id: PageId,
    pub frame_id: FrameId,
    pub handle_id: HandleId,
    pub name: String,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TextContentChangedPayload {
    pub page_id: PageId,
    pub frame_id: FrameId,
    pub handle_id: HandleId,
    pub old_text: String,
    pub new_text: String,
}

#[derive(Debug, Clone)]
pub struct InnerHtmlChangedPayload {
    pub page_id: PageId,
    pub frame_id: FrameId,
    pub handle_id: HandleId,
}

#[derive(Debug, Clone)]
pub struct ClassListChangedPayload {
    pub page_id: PageId,
    pub frame_id: FrameId,
    pub handle_id: HandleId,
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct StyleChangedPayload {
    pub page_id: PageId,
    pub frame_id: FrameId,
    pub handle_id: HandleId,
    pub property: String,
    pub old_value: String,
    pub new_value: String,
}

#[derive(Debug, Clone)]
pub struct ChildNodeInsertedPayload {
    pub page_id: PageId,
    pub frame_id: FrameId,
    pub parent_handle_id: HandleId,
    pub child_handle_id: HandleId,
    pub child_tag_name: String,
}

#[derive(Debug, Clone)]
pub struct ChildNodeRemovedPayload {
    pub page_id: PageId,
    pub frame_id: FrameId,
    pub parent_handle_id: HandleId,
    pub child_handle_id: HandleId,
    pub child_tag_name: String,
}

#[derive(Debug, Clone)]
pub struct SubtreeModifiedPayload {
    pub page_id: PageId,
    pub frame_id: FrameId,
    pub root_handle_id: HandleId,
}

#[derive(Debug, Clone)]
pub struct NodeReplacedPayload {
    pub page_id: PageId,
    pub frame_id: FrameId,
    pub parent_handle_id: HandleId,
    pub old_handle_id: HandleId,
    pub new_handle_id: HandleId,
}

#[derive(Debug, Clone)]
pub struct FrameAttachedPayload {
    pub page_id: PageId,
    pub frame_id: FrameId,
    pub parent_frame_id: Option<FrameId>,
    pub url: String,
}

#[derive(Debug, Clone)]
pub struct FrameDetachedPayload {
    pub page_id: PageId,
    pub frame_id: FrameId,
    pub parent_frame_id: Option<FrameId>,
}

#[derive(Debug, Clone)]
pub struct FrameNavigatedPayload {
    pub page_id: PageId,
    pub frame_id: FrameId,
    pub url: String,
}

#[derive(Debug, Clone)]
pub struct FrameClearedForNavigationPayload {
    pub page_id: PageId,
    pub frame_id: FrameId,
}

#[derive(Debug, Clone)]
pub struct DocumentUpdatedPayload {
    pub page_id: PageId,
    pub frame_id: FrameId,
}

#[derive(Debug, Clone)]
pub struct ElementFocusedPayload {
    pub page_id: PageId,
    pub frame_id: FrameId,
    pub handle_id: HandleId,
    pub tag_name: String,
}

#[derive(Debug, Clone)]
pub struct ElementBlurredPayload {
    pub page_id: PageId,
    pub frame_id: FrameId,
    pub handle_id: HandleId,
    pub tag_name: String,
}

#[derive(Debug, Clone)]
pub struct SelectionChangedPayload {
    pub page_id: PageId,
    pub frame_id: FrameId,
    pub selected_text: String,
    pub handle_id: Option<HandleId>,
}

#[derive(Debug, Clone)]
pub struct ScrollPositionChangedPayload {
    pub page_id: PageId,
    pub frame_id: FrameId,
    pub scroll_x: f64,
    pub scroll_y: f64,
}

#[derive(Debug, Clone)]
pub struct ShadowRootAttachedPayload {
    pub page_id: PageId,
    pub frame_id: FrameId,
    pub host_handle_id: HandleId,
    pub shadow_root_mode: ShadowRootMode,
}

#[derive(Debug, Clone)]
pub struct ShadowRootDetachedPayload {
    pub page_id: PageId,
    pub frame_id: FrameId,
    pub host_handle_id: HandleId,
}

#[derive(Debug, Clone)]
pub struct ShadowDomSlotChangedPayload {
    pub page_id: PageId,
    pub frame_id: FrameId,
    pub host_handle_id: HandleId,
    pub slot_name: String,
    pub assigned_nodes: Vec<HandleId>,
}

#[derive(Debug, Clone)]
pub struct MutationObservedPayload {
    pub page_id: PageId,
    pub frame_id: FrameId,
    pub target_handle_id: HandleId,
    pub mutation_type: MutationType,
    pub records: Vec<MutationRecord>,
}

#[derive(Debug, Clone)]
pub struct SnapshotCreatedPayload {
    pub page_id: PageId,
    pub frame_id: FrameId,
    pub node_count: usize,
    pub depth: usize,
}

#[derive(Debug, Clone)]
pub struct SnapshotStoredPayload {
    pub page_id: PageId,
    pub artifact_id: ArtifactId,
}

#[derive(Debug, Clone)]
pub struct DomOperationFailedPayload {
    pub page_id: PageId,
    pub frame_id: FrameId,
    pub handle_id: Option<HandleId>,
    pub operation: DomOperation,
    pub error: String,
}

#[derive(Debug, Clone)]
pub struct DomTimeoutPayload {
    pub page_id: PageId,
    pub frame_id: FrameId,
    pub operation: DomOperation,
    pub timeout: Duration,
}

#[derive(Debug, Clone)]
pub enum DomEvent {
    ElementCreated(ElementCreatedPayload),
    ElementRemoved(ElementRemovedPayload),
    ElementStale(ElementStalePayload),
    ElementReacquired(ElementReacquiredPayload),
    AttributeChanged(AttributeChangedPayload),
    TextContentChanged(TextContentChangedPayload),
    InnerHtmlChanged(InnerHtmlChangedPayload),
    ClassListChanged(ClassListChangedPayload),
    StyleChanged(StyleChangedPayload),
    ChildNodeInserted(ChildNodeInsertedPayload),
    ChildNodeRemoved(ChildNodeRemovedPayload),
    SubtreeModified(SubtreeModifiedPayload),
    NodeReplaced(NodeReplacedPayload),
    FrameAttached(FrameAttachedPayload),
    FrameDetached(FrameDetachedPayload),
    FrameNavigated(FrameNavigatedPayload),
    FrameClearedForNavigation(FrameClearedForNavigationPayload),
    DocumentUpdated(DocumentUpdatedPayload),
    ElementFocused(ElementFocusedPayload),
    ElementBlurred(ElementBlurredPayload),
    SelectionChanged(SelectionChangedPayload),
    ScrollPositionChanged(ScrollPositionChangedPayload),
    ShadowRootAttached(ShadowRootAttachedPayload),
    ShadowRootDetached(ShadowRootDetachedPayload),
    ShadowDomSlotChanged(ShadowDomSlotChangedPayload),
    MutationObserved(MutationObservedPayload),
    SnapshotCreated(SnapshotCreatedPayload),
    SnapshotStored(SnapshotStoredPayload),
    DomOperationFailed(DomOperationFailedPayload),
    DomTimeout(DomTimeoutPayload),
}
