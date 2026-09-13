pub mod core;
pub mod materializers;
pub mod pipeline;

pub use crate::core::{
    Artifact, ArtifactEntry, ArtifactError, Capability, CreationMetadata, EntryType, Manifest,
    MaterializationResult, Provenance, normalize_relative_path, validate_entry_layout,
};
pub use crate::materializers::{TarMaterializer, ZipMaterialization, ZipMaterializer};
pub use crate::pipeline::{
    ContentResolver, EntryContentResolver, MaterializerSpec, MemoryContentResolver, PipelineSpec,
    SelectStageSpec, SourceBackedArtifact, SourceSpec, StageSpec, default_directory_zip_pipeline,
};
