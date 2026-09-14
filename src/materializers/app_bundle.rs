use crate::core::{Artifact, ArtifactError, MaterializationResult, sha256_prefixed, validate_entry_layout};
use crate::pipeline::ContentResolver;
use serde::Serialize;
use std::fs;
use std::io::Read;
use std::path::Path;

#[derive(Clone, Copy, Debug, Default)]
pub struct AppBundleMaterializer;

impl AppBundleMaterializer {
    pub fn materialize_to_path<R: ContentResolver>(
        &self,
        artifact: &Artifact,
        resolver: &R,
        output: impl AsRef<Path>,
    ) -> Result<MaterializationResult, ArtifactError> {
        validate_entry_layout(&artifact.entries)?;
        let entrypoint = detect_entrypoint(artifact).ok_or_else(|| {
            ArtifactError::Materialization(
                "App Bundle requires an entrypoint such as application.wasm, bin/app, or a single file"
                    .to_string(),
            )
        })?;

        let output = output.as_ref();
        if output.exists() {
            fs::remove_dir_all(output).map_err(|source| ArtifactError::io(output, source))?;
        }
        fs::create_dir_all(output.join("payload")).map_err(|source| ArtifactError::io(output, source))?;
        fs::create_dir_all(output.join("resources"))
            .map_err(|source| ArtifactError::io(output, source))?;

        let manifest = BundleManifest::new(artifact, &entrypoint);
        let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
        fs::write(output.join("manifest.json"), &manifest_bytes)
            .map_err(|source| ArtifactError::io(output.join("manifest.json"), source))?;

        for entry in &artifact.entries {
            let path = output.join("payload").join(&entry.path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|source| ArtifactError::io(parent, source))?;
            }
            let bytes = read_entry_bytes(resolver, &entry.path, entry.size, &entry.content_digest)?;
            fs::write(&path, &bytes).map_err(|source| ArtifactError::io(&path, source))?;
        }

        Ok(MaterializationResult {
            artifact_identity: artifact.identity.clone(),
            materializer_format: "app-bundle".to_string(),
            output_digest: sha256_prefixed(&serde_json::to_vec(&manifest)?),
            size_bytes: artifact.total_size() + manifest_bytes.len() as u64,
        })
    }
}

pub fn detect_entrypoint(artifact: &Artifact) -> Option<String> {
    if artifact.entries.len() == 1 {
        return artifact.entries.first().map(|entry| entry.path.clone());
    }
    ["application.wasm", "bin/app"]
        .into_iter()
        .find(|candidate| artifact.entries.iter().any(|entry| entry.path == *candidate))
        .map(ToString::to_string)
}

#[derive(Serialize)]
struct BundleManifest<'a> {
    format_version: u32,
    artifact_identity: &'a str,
    entrypoint: String,
    payload_root: &'a str,
    resources_root: &'a str,
    entries: Vec<BundleManifestEntry<'a>>,
}

impl<'a> BundleManifest<'a> {
    fn new(artifact: &'a Artifact, entrypoint: &str) -> Self {
        Self {
            format_version: 1,
            artifact_identity: &artifact.identity,
            entrypoint: format!("payload/{entrypoint}"),
            payload_root: "payload",
            resources_root: "resources",
            entries: artifact
                .entries
                .iter()
                .map(|entry| BundleManifestEntry {
                    source_path: &entry.path,
                    bundle_path: format!("payload/{}", entry.path),
                    size: entry.size,
                    content_digest: &entry.content_digest,
                })
                .collect(),
        }
    }
}

#[derive(Serialize)]
struct BundleManifestEntry<'a> {
    source_path: &'a str,
    bundle_path: String,
    size: u64,
    content_digest: &'a str,
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
