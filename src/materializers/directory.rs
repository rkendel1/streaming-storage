use crate::core::{Artifact, ArtifactError, MaterializationResult, sha256_prefixed, validate_entry_layout};
use crate::pipeline::ContentResolver;
use serde::Serialize;
use std::fs;
use std::io::Read;
use std::path::Path;

#[derive(Clone, Copy, Debug, Default)]
pub struct DirectoryMaterializer;

impl DirectoryMaterializer {
    pub fn materialize_to_path<R: ContentResolver>(
        &self,
        artifact: &Artifact,
        resolver: &R,
        output: impl AsRef<Path>,
    ) -> Result<MaterializationResult, ArtifactError> {
        validate_entry_layout(&artifact.entries)?;
        let output = output.as_ref();
        if output.exists() {
            fs::remove_dir_all(output).map_err(|source| ArtifactError::io(output, source))?;
        }
        fs::create_dir_all(output).map_err(|source| ArtifactError::io(output, source))?;

        for entry in &artifact.entries {
            let path = output.join(&entry.path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|source| ArtifactError::io(parent, source))?;
            }
            let bytes = read_entry_bytes(resolver, entry.path.as_str(), entry.size, &entry.content_digest)?;
            fs::write(&path, &bytes).map_err(|source| ArtifactError::io(&path, source))?;
        }

        Ok(MaterializationResult {
            artifact_identity: artifact.identity.clone(),
            materializer_format: "directory".to_string(),
            output_digest: directory_digest(artifact)?,
            size_bytes: artifact.total_size(),
        })
    }
}

#[derive(Serialize)]
struct DirectoryDescriptor<'a> {
    format: &'a str,
    entries: &'a [crate::core::ArtifactEntry],
}

fn directory_digest(artifact: &Artifact) -> Result<String, ArtifactError> {
    Ok(sha256_prefixed(&serde_json::to_vec(&DirectoryDescriptor {
        format: "directory",
        entries: &artifact.entries,
    })?))
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
