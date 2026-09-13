use crate::core::{sha256_prefixed, Artifact, ArtifactError, CreationMetadata, Provenance};
use serde::Serialize;
use std::collections::BTreeMap;

pub struct CompositionInput {
    pub artifacts: Vec<Artifact>,
}

pub struct CompositionOptions {
    pub collision_policy: CollisionPolicy,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub enum CollisionPolicy {
    Reject,
}

impl Default for CompositionOptions {
    fn default() -> Self {
        CompositionOptions {
            collision_policy: CollisionPolicy::Reject,
        }
    }
}

impl CompositionInput {
    pub fn new(artifacts: Vec<Artifact>) -> Self {
        CompositionInput { artifacts }
    }

    pub fn compose(&self, options: CompositionOptions) -> Result<Artifact, ArtifactError> {
        if self.artifacts.is_empty() {
            return Err(ArtifactError::InvalidState(
                "composition requires at least one artifact".to_string(),
            ));
        }

        if self.artifacts.len() == 1 {
            return Ok(self.artifacts[0].clone());
        }

        let mut composed_entries = BTreeMap::new();
        let mut provenance_sources = Vec::new();
        let mut all_capabilities = Vec::new();

        for artifact in self.artifacts.iter() {
            provenance_sources.push(artifact.identity.clone());

            for entry in &artifact.entries {
                if composed_entries.contains_key(&entry.path) {
                    match options.collision_policy {
                        CollisionPolicy::Reject => {
                            return Err(ArtifactError::InvalidState(format!(
                                "collision in composition: {} appears in multiple artifacts",
                                entry.path
                            )));
                        }
                    }
                }
                composed_entries.insert(entry.path.clone(), entry.clone());
            }

            for cap in &artifact.capabilities {
                if !all_capabilities.iter().any(|c| c == cap) {
                    all_capabilities.push(cap.clone());
                }
            }
        }

        let entries: Vec<_> = composed_entries.into_values().collect();

        let composition_source = format!("composed[{}]", provenance_sources.join(","));
        let pipeline_identity =
            compute_composition_identity(&provenance_sources, &options.collision_policy)?;

        let provenance = Provenance {
            source_identity: composition_source.clone(),
            pipeline_identity: pipeline_identity.clone(),
            creation_metadata: CreationMetadata::default(),
        };

        Artifact::from_parts(entries, pipeline_identity, all_capabilities, provenance)
    }
}

fn compute_composition_identity(
    input_artifact_identities: &[String],
    collision_policy: &CollisionPolicy,
) -> Result<String, ArtifactError> {
    let canonical = serde_json::json!({
        "schema": "artifact_composition.v1",
        "input_artifact_identities": input_artifact_identities,
        "collision_policy": collision_policy,
    });
    Ok(sha256_prefixed(&serde_json::to_vec(&canonical)?))
}
