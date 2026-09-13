pub mod core;
pub mod materializers;
pub mod pipeline;

pub use crate::core::{
    Artifact, ArtifactEntry, ArtifactError, Capability, CreationMetadata, EntryType, Manifest,
    Provenance, normalize_relative_path, validate_entry_layout,
};
pub use crate::materializers::{ZipMaterialization, ZipMaterializer};
pub use crate::pipeline::{
    EntryContentResolver, MaterializerSpec, PipelineSpec, SelectStageSpec, SourceBackedArtifact,
    SourceSpec, StageSpec, default_directory_zip_pipeline,
};
