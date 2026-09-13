use crate::core::{Artifact, ArtifactError, MaterializationResult, sha256_prefixed, validate_entry_layout};
use crate::pipeline::ContentResolver;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Cursor, Read, Seek, Write};
use std::path::Path;
use zip::CompressionMethod;
use zip::DateTime;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ZipMaterialization {
    pub output_digest: String,
    pub size_bytes: u64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ZipMaterializer;

impl ZipMaterializer {
    pub fn materialize_to_vec<R: ContentResolver>(
        &self,
        artifact: &Artifact,
        resolver: &R,
    ) -> Result<Vec<u8>, ArtifactError> {
        let cursor = Cursor::new(Vec::new());
        let cursor = self.materialize_to_writer(artifact, resolver, cursor)?;
        Ok(cursor.into_inner())
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
            materializer_format: "zip".to_string(),
            output_digest: sha256_prefixed(&bytes),
            size_bytes: bytes.len() as u64,
        })
    }

    pub fn materialize_to_writer<W: Write + Seek, R: ContentResolver>(
        &self,
        artifact: &Artifact,
        resolver: &R,
        writer: W,
    ) -> Result<W, ArtifactError> {
        validate_entry_layout(&artifact.entries)?;
        let mut zip = ZipWriter::new(writer);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .last_modified_time(DateTime::default())
            .unix_permissions(0o644);

        for entry in &artifact.entries {
            zip.start_file(entry.path.clone(), options)
                .map_err(|error| ArtifactError::Materialization(error.to_string()))?;
            let mut reader = resolver.resolve(&entry.path)?;
            write_and_verify_entry(
                &mut zip,
                &mut reader,
                entry.size,
                &entry.content_digest,
                &entry.path,
            )?;
        }

        zip.finish()
            .map_err(|error| ArtifactError::Materialization(error.to_string()))
    }
}

fn write_and_verify_entry<W: Write, R: Read>(
    writer: &mut W,
    reader: &mut R,
    expected_size: u64,
    expected_digest: &str,
    path: &str,
) -> Result<(), ArtifactError> {
    let mut hasher = Sha256::new();
    let mut size = 0_u64;
    let mut buffer = [0_u8; 8192];

    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|source| ArtifactError::Materialization(format!("{path}: {source}")))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        writer
            .write_all(&buffer[..read])
            .map_err(|source| ArtifactError::Materialization(format!("{path}: {source}")))?;
        size += read as u64;
    }

    let actual_digest = {
        let mut encoded = String::from("sha256:");
        for byte in hasher.finalize() {
            use std::fmt::Write as _;
            let _ = write!(encoded, "{byte:02x}");
        }
        encoded
    };

    if size != expected_size {
        return Err(ArtifactError::Materialization(format!(
            "{path}: expected {expected_size} bytes, wrote {size}"
        )));
    }
    if actual_digest != expected_digest {
        return Err(ArtifactError::Materialization(format!(
            "{path}: content digest drifted during materialization"
        )));
    }

    Ok(())
}
