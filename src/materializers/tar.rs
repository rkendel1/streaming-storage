use crate::core::{
    Artifact, ArtifactError, MaterializationResult, sha256_prefixed, sha256_reader,
    validate_entry_layout,
};
use crate::pipeline::ContentResolver;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

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
            Self::Gzip => *gzip_available(),
            Self::Zstd => *zstd_available(),
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
        let (output_digest, size_bytes) = match compression {
            TarCompression::None => {
                let bytes = self.materialize_to_vec_with_compression(artifact, resolver, compression)?;
                let mut file =
                    File::create(output).map_err(|source| ArtifactError::io(output, source))?;
                file.write_all(&bytes)
                    .map_err(|source| ArtifactError::io(output, source))?;
                file.sync_all()
                    .map_err(|source| ArtifactError::io(output, source))?;
                (sha256_prefixed(&bytes), bytes.len() as u64)
            }
            TarCompression::Gzip | TarCompression::Zstd => {
                self.materialize_compressed_to_path(artifact, resolver, output, compression)?;
                let mut file =
                    File::open(output).map_err(|source| ArtifactError::io(output, source))?;
                let digest = sha256_reader(&mut file)?;
                let size = fs::metadata(output)
                    .map_err(|source| ArtifactError::io(output, source))?
                    .len();
                (digest, size)
            }
        };
        Ok(MaterializationResult {
            artifact_identity: artifact.identity.clone(),
            materializer_format: "tar".to_string(),
            output_digest,
            size_bytes,
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

    fn materialize_compressed_to_path<R: ContentResolver>(
        &self,
        artifact: &Artifact,
        resolver: &R,
        output: &Path,
        compression: TarCompression,
    ) -> Result<(), ArtifactError> {
        let temp_tar = temp_tar_path(compression);
        let temp_file =
            File::create(&temp_tar).map_err(|source| ArtifactError::io(&temp_tar, source))?;
        self.materialize_to_writer(artifact, resolver, temp_file)?;

        let (command, arguments, label) = match compression {
            TarCompression::Gzip => (
                "gzip",
                vec![OsStr::new("-n"), OsStr::new("-c"), temp_tar.as_os_str()],
                "gzip",
            ),
            TarCompression::Zstd => (
                "zstd",
                vec![
                    OsStr::new("-q"),
                    OsStr::new("--no-progress"),
                    OsStr::new("-c"),
                    temp_tar.as_os_str(),
                ],
                "zstd",
            ),
            TarCompression::None => return Ok(()),
        };

        let mut child = Command::new(command)
            .args(arguments)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|source| ArtifactError::Materialization(format!("{label}: {source}")))?;

        let mut output_file =
            File::create(output).map_err(|source| ArtifactError::io(output, source))?;
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| ArtifactError::Materialization(format!("{label}: missing stdout")))?;
        std::io::copy(&mut stdout, &mut output_file)
            .map_err(|source| ArtifactError::io(output, source))?;
        output_file
            .sync_all()
            .map_err(|source| ArtifactError::io(output, source))?;

        let result = child
            .wait_with_output()
            .map_err(|source| ArtifactError::Materialization(format!("{label}: {source}")))?;
        let _ = fs::remove_file(&temp_tar);
        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr).trim().to_string();
            return Err(ArtifactError::Materialization(format!(
                "{label} compression failed: {stderr}"
            )));
        }
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

fn gzip_available() -> &'static bool {
    static GZIP_AVAILABLE: OnceLock<bool> = OnceLock::new();
    GZIP_AVAILABLE.get_or_init(|| command_available("gzip"))
}

fn zstd_available() -> &'static bool {
    static ZSTD_AVAILABLE: OnceLock<bool> = OnceLock::new();
    ZSTD_AVAILABLE.get_or_init(|| command_available("zstd"))
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

fn temp_tar_path(compression: TarCompression) -> std::path::PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!("artifact-materializer-{suffix}.{}", compression.extension()))
}
