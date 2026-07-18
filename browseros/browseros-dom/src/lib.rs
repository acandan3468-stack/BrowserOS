pub mod attributes;
pub mod collection;
pub mod element;
pub mod error;
pub mod events;
pub mod frame;
pub mod id;
pub mod locator;
pub mod mutation;
pub mod mutation_observer;
pub mod node;
pub mod query;
pub mod selector;
pub mod shadow;
pub mod snapshot;
pub mod state;
pub mod style;
pub mod traversal;

pub use attributes::*;
pub use browseros_bridge::identifiers::{ElementId, FrameId, NodeId, PageId};
pub use browseros_bridge::locator::LocatorStrategy;
pub use browseros_bridge::traits::{ElementPort, FramePort, LocatorEngine, LocatorPort, PagePort};
pub use browseros_bridge::types::{
    BoxEdges, BoxModel, ElementState, JsResult, NodeInfo, NodeType, Point,
};
pub use collection::*;
pub use element::ElementHandle;
pub use error::{DomError, DomResult};
pub use events::*;
pub use frame::FrameHandle;
pub use id::HandleId;
pub use node::NodeHandle;
pub use shadow::ShadowRootHandle;
pub use snapshot::DomSnapshot;
pub use state::*;
