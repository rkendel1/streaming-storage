use crate::core::{Artifact, ArtifactError, MaterializationResult, sha256_prefixed, validate_entry_layout};
use crate::pipeline::ContentResolver;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

#[derive(Clone, Copy, Debug, Default)]
pub struct RawFileMaterializer;

impl RawFileMaterializer {
    pub fn materialize_to_path<R: ContentResolver>(
        &self,
        artifact: &Artifact,
        resolver: &R,
        output: impl AsRef<Path>,
    ) -> Result<MaterializationResult, ArtifactError> {
        validate_entry_layout(&artifact.entries)?;
        let entry = single_entry(artifact)?;
        let output = output.as_ref();
        let bytes = read_entry_bytes(resolver, &entry.path, entry.size, &entry.content_digest)?;
        let mut file = File::create(output).map_err(|source| ArtifactError::io(output, source))?;
        file.write_all(&bytes)
            .map_err(|source| ArtifactError::io(output, source))?;
        file.sync_all()
            .map_err(|source| ArtifactError::io(output, source))?;

        Ok(MaterializationResult {
            artifact_identity: artifact.identity.clone(),
            materializer_format: "raw-file".to_string(),
            output_digest: sha256_prefixed(&bytes),
            size_bytes: bytes.len() as u64,
        })
    }
}

fn single_entry(artifact: &Artifact) -> Result<&crate::core::ArtifactEntry, ArtifactError> {
    if artifact.entries.len() != 1 {
        return Err(ArtifactError::Materialization(
            "Raw File requires an artifact containing exactly one file".to_string(),
        ));
    }
    Ok(&artifact.entries[0])
}

fn read_entry_bytes<R: ContentResolver>(
    resolver: &R,
    path: &str,
    expected_size: u64,
    expected_digest: &str,
) -> Result<Vec<u8>, ArtifactError> {
    let mut reader = resolver.resolve(path)?;
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .map_err(|source| ArtifactError::Materialization(format!("{path}: {source}")))?;
    if bytes.len() as u64 != expected_size {
        return Err(ArtifactError::Materialization(format!(
            "{path}: expected {expected_size} bytes, wrote {}",
            bytes.len()
        )));
    }
    if sha256_prefixed(&bytes) != expected_digest {
        return Err(ArtifactError::Materialization(format!(
            "{path}: content digest drifted during materialization"
        )));
    }
    Ok(bytes)
}
