use crate::error::BridgeResult;
use crate::identifiers::ArtifactId;
use crate::types::{ArtifactFilter, ArtifactMeta, ArtifactType};
use std::path::PathBuf;

/// Interface for persistent artifact storage.
///
/// `ArtifactPort` provides CRUD operations for browser artifacts
/// such as screenshots, PDFs, traces, and HAR files.
pub trait ArtifactPort: Send + Sync {
    /// Store a new artifact. Returns a unique identifier.
    fn store(
        &self,
        name: &str,
        data: Vec<u8>,
        artifact_type: ArtifactType,
    ) -> BridgeResult<ArtifactId>;

    /// Retrieve artifact data by ID.
    fn retrieve(&self, id: &ArtifactId) -> BridgeResult<Vec<u8>>;

    /// List stored artifacts, optionally filtered.
    fn list(&self, filter: Option<ArtifactFilter>) -> BridgeResult<Vec<ArtifactMeta>>;

    /// Delete an artifact by ID.
    fn delete(&self, id: &ArtifactId) -> BridgeResult<()>;

    /// Returns the base storage path.
    fn storage_path(&self) -> PathBuf;
}
