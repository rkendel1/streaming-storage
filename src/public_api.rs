use crate::authorization::{AuthorizationDecision, CapabilityPolicy, ExecutionEvidence};
use crate::core::{Artifact, ArtifactError, Capability};
use crate::pipeline::{PipelineInspection, PipelineSpec};
use crate::recipes::RecipeSpec;
use std::path::Path;

pub struct ArtifactSDK;

impl ArtifactSDK {
    pub fn recipe_from_spec(recipe: RecipeSpec) -> ArtifactRecipe {
        ArtifactRecipe { recipe }
    }

    pub fn pipeline_from_spec(pipeline: PipelineSpec) -> ArtifactPipeline {
        ArtifactPipeline { pipeline }
    }
}

pub struct ArtifactRecipe {
    recipe: RecipeSpec,
}

impl ArtifactRecipe {
    pub fn compile(self) -> Result<ArtifactPipeline, ArtifactError> {
        let pipeline = self.recipe.compile()?;
        Ok(ArtifactPipeline { pipeline })
    }
}

pub struct ArtifactPipeline {
    pipeline: PipelineSpec,
}

impl ArtifactPipeline {
    pub fn inspect(&self) -> Result<PublicPipelineInspection, ArtifactError> {
        let inspection = self.pipeline.inspect()?;
        Ok(PublicPipelineInspection {
            pipeline_identity: inspection.pipeline_identity,
            stages: inspection
                .stages
                .iter()
                .map(|s| PublicStage {
                    label: s.label.clone(),
                    identity: s.stage_identity.clone(),
                })
                .collect(),
            required_capabilities: inspection.required_capabilities,
            materializer: inspection.materializer,
        })
    }

    pub fn required_capabilities(&self) -> Vec<Capability> {
        self.pipeline.required_capabilities()
    }

    pub fn build_from_directory(
        &self,
        root: impl AsRef<Path>,
    ) -> Result<PublicArtifact, ArtifactError> {
        let built = self.pipeline.build_from_directory(root)?;
        Ok(PublicArtifact {
            artifact: built.artifact().clone(),
        })
    }

    pub fn build_with_authorization(
        &self,
        root: impl AsRef<Path>,
        policy: &dyn CapabilityPolicy,
    ) -> Result<(PublicArtifact, PublicExecutionEvidence), ArtifactError> {
        let (built, evidence) = self.pipeline.build_with_authorization(root, policy)?;
        Ok((
            PublicArtifact {
                artifact: built.artifact().clone(),
            },
            PublicExecutionEvidence { evidence },
        ))
    }
}

#[derive(Clone, Debug)]
pub struct PublicPipelineInspection {
    pub pipeline_identity: String,
    pub stages: Vec<PublicStage>,
    pub required_capabilities: Vec<Capability>,
    pub materializer: String,
}

#[derive(Clone, Debug)]
pub struct PublicStage {
    pub label: String,
    pub identity: String,
}

#[derive(Clone)]
pub struct PublicArtifact {
    artifact: Artifact,
}

impl PublicArtifact {
    pub fn identity(&self) -> &str {
        &self.artifact.identity
    }

    pub fn entries_count(&self) -> usize {
        self.artifact.entries.len()
    }

    pub fn entries(&self) -> Vec<PublicArtifactEntry> {
        self.artifact
            .entries
            .iter()
            .map(|e| PublicArtifactEntry {
                path: e.path.clone(),
                entry_type: format!("{:?}", e.entry_type),
            })
            .collect()
    }

    pub fn as_artifact(&self) -> &Artifact {
        &self.artifact
    }
}

#[derive(Clone, Debug)]
pub struct PublicArtifactEntry {
    pub path: String,
    pub entry_type: String,
}

pub struct PublicExecutionEvidence {
    evidence: ExecutionEvidence,
}

impl PublicExecutionEvidence {
    pub fn is_successful(&self) -> bool {
        self.evidence.authorization_decision.is_allowed()
            && matches!(
                self.evidence.execution_result,
                crate::authorization::ExecutionResult::Success
            )
    }

    pub fn authorization_decision(&self) -> PublicAuthorizationDecision {
        PublicAuthorizationDecision {
            allowed: self.evidence.authorization_decision.is_allowed(),
            requested_capabilities: self.evidence.requested_capabilities.clone(),
            granted_capabilities: self.evidence.granted_capabilities.clone(),
            denied_capabilities: self
                .evidence
                .authorization_decision
                .denied_capabilities
                .clone(),
        }
    }

    pub fn stage_trace(&self) -> Vec<PublicExecutedStage> {
        self.evidence
            .stage_trace
            .iter()
            .map(|s| PublicExecutedStage {
                label: s.label.clone(),
                identity: s.stage_identity.clone(),
            })
            .collect()
    }

    pub fn used_capabilities(&self) -> Vec<Capability> {
        self.evidence.used_capabilities.clone()
    }

    pub fn as_evidence(&self) -> &ExecutionEvidence {
        &self.evidence
    }
}

#[derive(Clone, Debug)]
pub struct PublicAuthorizationDecision {
    pub allowed: bool,
    pub requested_capabilities: Vec<Capability>,
    pub granted_capabilities: Vec<Capability>,
    pub denied_capabilities: Vec<Capability>,
}

#[derive(Clone, Debug)]
pub struct PublicExecutedStage {
    pub label: String,
    pub identity: String,
}
