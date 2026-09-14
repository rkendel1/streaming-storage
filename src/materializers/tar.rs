use crate::core::{
    Artifact, ArtifactError, MaterializationResult, sha256_prefixed, validate_entry_layout,
};
use crate::pipeline::ContentResolver;
use std::ffi::OsStr;
use std::fs::File;
use std::io::Write;
use std::process::{Command, Stdio};
use std::path::Path;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TarCompression {
    None,
    Gzip,
    Zstd,
}

impl TarCompression {
    pub fn extension(self) -> &'static str {
        match self {
            Self::None => "tar",
            Self::Gzip => "tar.gz",
            Self::Zstd => "tar.zst",
        }
    }

    pub fn is_supported(self) -> bool {
        match self {
            Self::None => true,
            Self::Gzip => command_available("gzip"),
            Self::Zstd => command_available("zstd"),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TarMaterializer;

impl TarMaterializer {
    pub fn materialize_to_vec<R: ContentResolver>(
        &self,
        artifact: &Artifact,
        resolver: &R,
    ) -> Result<Vec<u8>, ArtifactError> {
        self.materialize_to_vec_with_compression(artifact, resolver, TarCompression::None)
    }

    pub fn materialize_to_vec_with_compression<R: ContentResolver>(
        &self,
        artifact: &Artifact,
        resolver: &R,
        compression: TarCompression,
    ) -> Result<Vec<u8>, ArtifactError> {
        let mut buffer = Vec::new();
        self.materialize_to_writer(artifact, resolver, &mut buffer)?;
        match compression {
            TarCompression::None => Ok(buffer),
            TarCompression::Gzip => compress_with_command(
                &buffer,
                "gzip",
                &[OsStr::new("-n"), OsStr::new("-c")],
                "gzip",
            ),
            TarCompression::Zstd => compress_with_command(
                &buffer,
                "zstd",
                &[OsStr::new("-q"), OsStr::new("--no-progress"), OsStr::new("-c")],
                "zstd",
            ),
        }
    }

    pub fn materialize_to_path<R: ContentResolver>(
        &self,
        artifact: &Artifact,
        resolver: &R,
        output: impl AsRef<Path>,
    ) -> Result<MaterializationResult, ArtifactError> {
        self.materialize_to_path_with_compression(artifact, resolver, output, TarCompression::None)
    }

    pub fn materialize_to_path_with_compression<R: ContentResolver>(
        &self,
        artifact: &Artifact,
        resolver: &R,
        output: impl AsRef<Path>,
        compression: TarCompression,
    ) -> Result<MaterializationResult, ArtifactError> {
        let output = output.as_ref();
        let bytes = self.materialize_to_vec_with_compression(artifact, resolver, compression)?;
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

fn command_available(name: &str) -> bool {
    Command::new(name)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

fn compress_with_command(
    bytes: &[u8],
    command: &str,
    arguments: &[&OsStr],
    label: &str,
) -> Result<Vec<u8>, ArtifactError> {
    if !command_available(command) {
        return Err(ArtifactError::Materialization(format!(
            "{label} compression is unavailable in this environment"
        )));
    }

    let mut child = Command::new(command)
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| ArtifactError::Materialization(format!("{label}: {source}")))?;

    child
        .stdin
        .take()
        .ok_or_else(|| ArtifactError::Materialization(format!("{label}: missing stdin")))?
        .write_all(bytes)
        .map_err(|source| ArtifactError::Materialization(format!("{label}: {source}")))?;

    let output = child
        .wait_with_output()
        .map_err(|source| ArtifactError::Materialization(format!("{label}: {source}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(ArtifactError::Materialization(format!(
            "{label} compression failed: {stderr}"
        )));
    }
    Ok(output.stdout)
}
