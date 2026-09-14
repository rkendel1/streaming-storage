use crate::core::{Artifact, ArtifactError, MaterializationResult, sha256_prefixed, validate_entry_layout};
use crate::pipeline::ContentResolver;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

const MODULE_HEADER: [u8; 8] = [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
const COMPONENT_HEADER: [u8; 8] = [0x00, 0x61, 0x73, 0x6d, 0x0a, 0x00, 0x01, 0x00];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WasmRepresentationKind {
    Module,
    Component,
}

impl WasmRepresentationKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Module => "wasm-module",
            Self::Component => "wasm-component",
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct WasmMaterializer;

impl WasmMaterializer {
    pub fn detect_kind<R: ContentResolver + ?Sized>(
        &self,
        artifact: &Artifact,
        resolver: &R,
    ) -> Result<WasmRepresentationKind, ArtifactError> {
        validate_entry_layout(&artifact.entries)?;
        let entry = single_entry(artifact)?;
        let bytes = read_entry_bytes(resolver, &entry.path, entry.size, &entry.content_digest)?;
        if bytes.starts_with(&MODULE_HEADER) {
            return Ok(WasmRepresentationKind::Module);
        }
        if bytes.starts_with(&COMPONENT_HEADER) {
            return Ok(WasmRepresentationKind::Component);
        }
        Err(ArtifactError::Materialization(
            "WASM representation requires a valid core module or component binary".to_string(),
        ))
    }

    pub fn materialize_to_path<R: ContentResolver + ?Sized>(
        &self,
        artifact: &Artifact,
        resolver: &R,
        output: impl AsRef<Path>,
        kind: WasmRepresentationKind,
    ) -> Result<MaterializationResult, ArtifactError> {
        let detected = self.detect_kind(artifact, resolver)?;
        if detected != kind {
            return Err(ArtifactError::Materialization(format!(
                "{} requires a matching {} artifact",
                kind.label(),
                kind.label()
            )));
        }
        let entry = single_entry(artifact)?;
        let bytes = read_entry_bytes(resolver, &entry.path, entry.size, &entry.content_digest)?;
        let output = output.as_ref();
        let mut file = File::create(output).map_err(|source| ArtifactError::io(output, source))?;
        file.write_all(&bytes)
            .map_err(|source| ArtifactError::io(output, source))?;
        file.sync_all()
            .map_err(|source| ArtifactError::io(output, source))?;
        Ok(MaterializationResult {
            artifact_identity: artifact.identity.clone(),
            materializer_format: kind.label().to_string(),
            output_digest: sha256_prefixed(&bytes),
            size_bytes: bytes.len() as u64,
        })
    }
}

fn single_entry(artifact: &Artifact) -> Result<&crate::core::ArtifactEntry, ArtifactError> {
    if artifact.entries.len() != 1 {
        return Err(ArtifactError::Materialization(
            "WASM representation requires an artifact containing exactly one file".to_string(),
        ));
    }
    Ok(&artifact.entries[0])
}

fn read_entry_bytes<R: ContentResolver + ?Sized>(
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
