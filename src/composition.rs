use crate::core::{Artifact, ArtifactError, Provenance, CreationMetadata};
use std::collections::BTreeMap;

pub struct CompositionInput {
    pub artifacts: Vec<Artifact>,
}

pub struct CompositionOptions {
    pub collision_policy: CollisionPolicy,
}

#[derive(Clone, Debug)]
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

        let identity = compute_composition_identity(&entries, &composition_source);

        let provenance = Provenance {
            source_identity: composition_source.clone(),
            pipeline_identity: identity.clone(),
            creation_metadata: CreationMetadata::default(),
        };

        let manifest = crate::core::Manifest {
            manifest_version: 1,
            artifact_identity: identity.clone(),
            entries: entries.clone(),
            pipeline_identity: identity.clone(),
            capabilities: all_capabilities.clone(),
            provenance: provenance.clone(),
        };

        let identity_for_artifact = identity.clone();

        Ok(Artifact {
            identity: identity_for_artifact.clone(),
            entries,
            manifest,
            pipeline_identity: identity_for_artifact,
            capabilities: all_capabilities,
            provenance,
        })
    }
}

fn compute_composition_identity(entries: &[crate::core::ArtifactEntry], composition_source: &str) -> String {
    let mut hasher = sha2::Sha256::new();

    use sha2::Digest;
    use std::fmt::Write as _;

    for entry in entries {
        hasher.update(entry.path.as_bytes());
        hasher.update(format!("{:?}", entry.entry_type).as_bytes());
        hasher.update(entry.content_digest.as_bytes());
    }

    hasher.update(composition_source.as_bytes());

    let result = hasher.finalize();
    let mut encoded = String::with_capacity(result.len() * 2 + 7);
    encoded.push_str("sha256:");
    for byte in result.iter() {
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}
