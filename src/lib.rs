pub mod application;
pub mod authorization;
pub mod composition;
pub mod core;
pub mod materializers;
pub mod pipeline;
pub mod public_api;
pub mod recipes;
pub mod storage;
pub mod transforms;
#[cfg(feature = "wasm")]
pub mod wasm;

pub use crate::application::{ApplicationArtifact, ApplicationManifest};
pub use crate::authorization::{
    AllowAllPolicy, AllowListPolicy, AuthorizationDecision, AuthorizationResult, CapabilityPolicy,
    ExecutedStage, ExecutionEvidence, ExecutionResult,
};
pub use crate::composition::{CollisionPolicy, CompositionInput, CompositionOptions};
pub use crate::core::{
    Artifact, ArtifactEntry, ArtifactError, Capability, CreationMetadata, EntryType, Manifest,
    MaterializationResult, Provenance, TransformationRecord, normalize_relative_path,
    validate_entry_layout,
};
pub use crate::materializers::{TarMaterializer, ZipMaterialization, ZipMaterializer};
pub use crate::pipeline::{
    CompileStageSpec, ContentResolver, EntryContentResolver, GenerateStageSpec, InspectedStage,
    MaterializerSpec, MemoryContentResolver, PipelineInspection, PipelineSpec, RedactStageSpec,
    SelectStageSpec, SourceBackedArtifact, SourceSpec, StageSpec, TransformStageSpec,
    TransformedContentResolver, default_directory_zip_pipeline,
};
pub use crate::public_api::{
    ArtifactPipeline, ArtifactRecipe, ArtifactSDK, PublicArtifact, PublicArtifactEntry,
    PublicAuthorizationDecision, PublicExecutedStage, PublicExecutionEvidence,
    PublicPipelineInspection, PublicStage,
};
pub use crate::recipes::{RecipeConfig, RecipeSpec, RecipeType};
pub use crate::storage::{LocalArtifactStore, RecoveredArtifact};
pub use crate::transforms::{
    ArtifactTransform, GenerateTransform, PrefixTransform, RedactTransform, TransformedArtifact,
};
