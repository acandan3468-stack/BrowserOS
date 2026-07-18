#![allow(dead_code)]

pub mod bridge;
pub mod config;
pub mod engine;
pub mod error;
pub mod events;
pub mod exec;
pub mod loader;
pub mod manager;
pub mod node;
pub mod planner;
pub mod plugin;
pub mod runtime;

mod executor;
mod graph;
mod scheduler;

pub use crate::bridge::bridge_node;
pub use crate::engine::DagEngine;
pub use crate::error::DagError;
pub use crate::events::{
    DagExecutionCancelled, DagExecutionCompleted, DagExecutionFailed, DagExecutionStarted,
    DagNodeCompleted, DagNodeFailed, DagNodeRetrying, DagNodeSkipped, DagNodeStarted,
};
pub use crate::exec::{
    CapabilityMetadata, ContextPropagator, ExecutableNode, ExecutionContext, ExecutionError,
    ExecutionInput, ExecutionMetadata, ExecutionOutput, NodeFactory, NodeRegistry, VariableStore,
};
pub use crate::loader::{
    ManifestCapability, ManifestDependency, ManifestHeader, ManifestHooks, ManifestPermission,
    PluginLoader, PluginManifestFile,
};
pub use crate::manager::{PluginManager, PluginStatistics};
pub use crate::node::{DagDefinition, DagExecutionState, DagNode, DagResult, NodeKind, NodeState};
pub use crate::planner::{
    ExecutionConstraints, ExecutionHints, ExecutionIntent, ExecutionMode, ExecutionPlan,
    ExecutionPriority, ExecutionTarget, PlanValidationResult, PlannerBridge, PlannerMetadata,
    PlannerRequest, PlannerResponse, PlannerType, PlanningContext, PlanningError, PlanningResult,
    StepValidationResult,
};
pub use crate::plugin::{
    Plugin, PluginAuthor, PluginCapability, PluginCapabilityId, PluginContext, PluginDependency,
    PluginError, PluginExecutionMetadata, PluginHooks, PluginId, PluginLifecycle, PluginManifest,
    PluginMetadata, PluginPermission, PluginRegistration, PluginRegistry, PluginState,
    PluginStatus, PluginValidationResult, PluginVersion,
};
pub use crate::runtime::{
    CapabilityMatch, CapabilityMatcher, CapabilityRequest, CapabilityResolver, CapabilitySelection,
    MatchCriterion, PluginCapabilityNode, PluginExecutionBridge, PluginRuntime,
};
