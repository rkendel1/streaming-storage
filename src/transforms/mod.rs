use crate::core::{
    sha256_prefixed, Artifact, ArtifactEntry, ArtifactError, CreationMetadata, EntryType,
    Provenance, TransformationRecord,
};
use crate::pipeline::ContentResolver;
use serde::Serialize;
use std::collections::BTreeMap;

pub trait ArtifactTransform {
    fn transform_kind(&self) -> &'static str;
    fn transform_identity(&self) -> Result<String, ArtifactError>;

    fn apply(
        &self,
        artifact: &Artifact,
        resolver: &dyn ContentResolver,
    ) -> Result<TransformedArtifact, ArtifactError>;
}

pub struct TransformedArtifact {
    pub artifact: Artifact,
    pub content_updates: BTreeMap<String, Vec<u8>>,
}

pub struct PrefixTransform {
    pub prefix: String,
}

impl PrefixTransform {
    pub fn new(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
        }
    }
}

impl ArtifactTransform for PrefixTransform {
    fn transform_kind(&self) -> &'static str {
        "prefix"
    }

    fn transform_identity(&self) -> Result<String, ArtifactError> {
        transform_identity(
            self.transform_kind(),
            &serde_json::json!({
                "prefix": &self.prefix,
            }),
        )
    }

    fn apply(
        &self,
        artifact: &Artifact,
        resolver: &dyn ContentResolver,
    ) -> Result<TransformedArtifact, ArtifactError> {
        let mut new_entries = Vec::new();
        let mut content_updates = BTreeMap::new();

        for entry in &artifact.entries {
            let new_path = format!("{}/{}", self.prefix, entry.path);
            new_entries.push(ArtifactEntry {
                path: new_path.clone(),
                entry_type: entry.entry_type,
                size: entry.size,
                content_digest: entry.content_digest.clone(),
            });

            let mut reader = resolver.resolve(&entry.path)?;
            let mut content = Vec::new();
            std::io::Read::read_to_end(&mut reader, &mut content)
                .map_err(|source| ArtifactError::io("<transformed>", source))?;
            content_updates.insert(new_path, content);
        }

        let new_artifact = Artifact::from_parts(
            new_entries,
            artifact.pipeline_identity.clone(),
            artifact.capabilities.clone(),
            Provenance {
                source_identity: artifact.provenance.source_identity.clone(),
                pipeline_identity: artifact.pipeline_identity.clone(),
                creation_metadata: CreationMetadata::default(),
            },
        )?
        .with_transformation_record(TransformationRecord {
            input_artifact_identity: artifact.identity.clone(),
            transform_identity: self.transform_identity()?,
            transform_kind: self.transform_kind().to_string(),
        });

        Ok(TransformedArtifact {
            artifact: new_artifact,
            content_updates,
        })
    }
}

pub struct RedactTransform {
    pub paths_to_remove: Vec<String>,
}

impl RedactTransform {
    pub fn new(paths: Vec<String>) -> Self {
        Self {
            paths_to_remove: paths,
        }
    }
}

impl ArtifactTransform for RedactTransform {
    fn transform_kind(&self) -> &'static str {
        "redact"
    }

    fn transform_identity(&self) -> Result<String, ArtifactError> {
        let mut paths = self.paths_to_remove.clone();
        paths.sort();
        paths.dedup();
        transform_identity(
            self.transform_kind(),
            &serde_json::json!({
                "paths": paths,
            }),
        )
    }

    fn apply(
        &self,
        artifact: &Artifact,
        _resolver: &dyn ContentResolver,
    ) -> Result<TransformedArtifact, ArtifactError> {
        let new_entries: Vec<ArtifactEntry> = artifact
            .entries
            .iter()
            .filter(|entry| !self.paths_to_remove.contains(&entry.path))
            .cloned()
            .collect();

        let new_artifact = Artifact::from_parts(
            new_entries,
            artifact.pipeline_identity.clone(),
            artifact.capabilities.clone(),
            Provenance {
                source_identity: artifact.provenance.source_identity.clone(),
                pipeline_identity: artifact.pipeline_identity.clone(),
                creation_metadata: CreationMetadata::default(),
            },
        )?
        .with_transformation_record(TransformationRecord {
            input_artifact_identity: artifact.identity.clone(),
            transform_identity: self.transform_identity()?,
            transform_kind: self.transform_kind().to_string(),
        });

        Ok(TransformedArtifact {
            artifact: new_artifact,
            content_updates: BTreeMap::new(),
        })
    }
}

pub struct GenerateTransform {
    pub path: String,
    pub content: Vec<u8>,
}

impl GenerateTransform {
    pub fn new(path: impl Into<String>, content: impl Into<Vec<u8>>) -> Self {
        Self {
            path: path.into(),
            content: content.into(),
        }
    }
}

impl ArtifactTransform for GenerateTransform {
    fn transform_kind(&self) -> &'static str {
        "generate"
    }

    fn transform_identity(&self) -> Result<String, ArtifactError> {
        transform_identity(
            self.transform_kind(),
            &serde_json::json!({
                "path": &self.path,
                "content_digest": sha256_prefixed(&self.content),
                "size": self.content.len() as u64,
            }),
        )
    }

    fn apply(
        &self,
        artifact: &Artifact,
        resolver: &dyn ContentResolver,
    ) -> Result<TransformedArtifact, ArtifactError> {
        use sha2::{Digest, Sha256};

        let mut new_entries = artifact.entries.clone();
        let mut content_updates = BTreeMap::new();

        let mut hasher = Sha256::new();
        hasher.update(&self.content);
        let digest = {
            let mut encoded = String::from("sha256:");
            for byte in hasher.finalize() {
                use std::fmt::Write as _;
                let _ = write!(encoded, "{byte:02x}");
            }
            encoded
        };

        new_entries.push(ArtifactEntry {
            path: self.path.clone(),
            entry_type: EntryType::File,
            size: self.content.len() as u64,
            content_digest: digest,
        });

        new_entries.sort_by(|a, b| a.path.cmp(&b.path));

        for entry in &artifact.entries {
            let mut reader = resolver.resolve(&entry.path)?;
            let mut content = Vec::new();
            std::io::Read::read_to_end(&mut reader, &mut content)
                .map_err(|source| ArtifactError::io("<transformed>", source))?;
            content_updates.insert(entry.path.clone(), content);
        }

        content_updates.insert(self.path.clone(), self.content.clone());

        let new_artifact = Artifact::from_parts(
            new_entries,
            artifact.pipeline_identity.clone(),
            artifact.capabilities.clone(),
            Provenance {
                source_identity: artifact.provenance.source_identity.clone(),
                pipeline_identity: artifact.pipeline_identity.clone(),
                creation_metadata: CreationMetadata::default(),
            },
        )?
        .with_transformation_record(TransformationRecord {
            input_artifact_identity: artifact.identity.clone(),
            transform_identity: self.transform_identity()?,
            transform_kind: self.transform_kind().to_string(),
        });

        Ok(TransformedArtifact {
            artifact: new_artifact,
            content_updates,
        })
    }
}

fn transform_identity<T: Serialize>(kind: &str, spec: &T) -> Result<String, ArtifactError> {
    let canonical = serde_json::json!({
        "schema": "artifact_transform.v1",
        "kind": kind,
        "spec": spec,
    });
    Ok(sha256_prefixed(&serde_json::to_vec(&canonical)?))
}
