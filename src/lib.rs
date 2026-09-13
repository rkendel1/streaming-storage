pub mod core;
pub mod materializers;
pub mod pipeline;
pub mod transforms;

pub use crate::core::{
    Artifact, ArtifactEntry, ArtifactError, Capability, CreationMetadata, EntryType, Manifest,
    MaterializationResult, Provenance, normalize_relative_path, validate_entry_layout,
};
pub use crate::materializers::{TarMaterializer, ZipMaterialization, ZipMaterializer};
pub use crate::transforms::{
    ArtifactTransform, GenerateTransform, PrefixTransform, RedactTransform, TransformedArtifact,
};
pub use crate::pipeline::{
    ContentResolver, EntryContentResolver, GenerateStageSpec, MaterializerSpec,
    MemoryContentResolver, PipelineSpec, RedactStageSpec, SelectStageSpec, SourceBackedArtifact,
    SourceSpec, StageSpec, TransformedContentResolver, TransformStageSpec,
    default_directory_zip_pipeline,
};
