use crate::core::{Artifact, ArtifactError, MaterializationResult, sha256_prefixed, validate_entry_layout};
use crate::pipeline::ContentResolver;
use std::fs::File;
use std::io::Write;
use std::path::Path;

#[derive(Clone, Copy, Debug, Default)]
pub struct TarMaterializer;

impl TarMaterializer {
    pub fn materialize_to_vec<R: ContentResolver>(
        &self,
        artifact: &Artifact,
        resolver: &R,
    ) -> Result<Vec<u8>, ArtifactError> {
        let mut buffer = Vec::new();
        self.materialize_to_writer(artifact, resolver, &mut buffer)?;
        Ok(buffer)
    }

    pub fn materialize_to_path<R: ContentResolver>(
        &self,
        artifact: &Artifact,
        resolver: &R,
        output: impl AsRef<Path>,
    ) -> Result<MaterializationResult, ArtifactError> {
        let output = output.as_ref();
        let bytes = self.materialize_to_vec(artifact, resolver)?;
        let mut file = File::create(output).map_err(|source| ArtifactError::io(output, source))?;
        file.write_all(&bytes)
            .map_err(|source| ArtifactError::io(output, source))?;
        file.sync_all()
            .map_err(|source| ArtifactError::io(output, source))?;
        Ok(MaterializationResult {
            artifact_identity: artifact.identity.clone(),
            materializer_format: "tar".to_string(),
            output_digest: sha256_prefixed(&bytes),
            size_bytes: bytes.len() as u64,
        })
    }

    pub fn materialize_to_writer<W: Write, R: ContentResolver>(
        &self,
        artifact: &Artifact,
        resolver: &R,
        writer: W,
    ) -> Result<(), ArtifactError> {
        validate_entry_layout(&artifact.entries)?;

        let mut tar = tar::Builder::new(writer);

        for entry in &artifact.entries {
            let mut reader = resolver.resolve(&entry.path)?;
            let mut header = tar::Header::new_gnu();
            header.set_size(entry.size);
            header.set_cksum();

            tar.append_data(&mut header, &entry.path, &mut reader)
                .map_err(|error| {
                    ArtifactError::Materialization(format!("{}: {}", entry.path, error))
                })?;
        }

        tar.finish()
            .map_err(|error| ArtifactError::Materialization(error.to_string()))?;

        Ok(())
    }
}
