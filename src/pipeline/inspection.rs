use crate::core::{ArtifactError, Capability};
use crate::pipeline::{MaterializerSpec, PipelineSpec, StageSpec};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct PipelineInspection {
    pub pipeline_identity: String,
    pub stages: Vec<InspectedStage>,
    pub required_capabilities: Vec<Capability>,
    pub materializer: String,
    pub canonical_pipeline_json: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct InspectedStage {
    pub label: String,
    pub stage_identity: String,
}

impl PipelineSpec {
    pub fn inspect(&self) -> Result<PipelineInspection, ArtifactError> {
        self.validate_stage_sequence()?;

        let pipeline_identity = self.identity()?;
        let canonical_pipeline_json =
            String::from_utf8(self.to_canonical_bytes()?).map_err(|_| {
                ArtifactError::InvalidState("pipeline JSON contains invalid UTF-8".to_string())
            })?;

        let mut stages = Vec::new();
        for stage in &self.stages {
            stages.push(InspectedStage {
                label: stage.label().to_string(),
                stage_identity: stage.identity()?,
            });
        }

        let mut required_capabilities = self.required_capabilities();

        match self.materializer {
            MaterializerSpec::Zip => {
                required_capabilities.push(Capability::new("package.zip", "1"));
            }
            MaterializerSpec::Tar => {
                required_capabilities.push(Capability::new("package.tar", "1"));
            }
        }

        Ok(PipelineInspection {
            pipeline_identity,
            stages,
            required_capabilities: crate::core::canonical_capabilities(required_capabilities),
            materializer: self.materializer.label().to_string(),
            canonical_pipeline_json,
        })
    }
}
