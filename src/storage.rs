use crate::core::{
    sha256_reader, validate_entry_layout, Artifact, ArtifactEntry, ArtifactError, MANIFEST_VERSION,
};
use crate::pipeline::ContentResolver;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

const ARTIFACTS_DIR: &str = "artifacts";
const CONTENTS_DIR: &str = "contents";

#[derive(Clone, Debug)]
pub struct LocalArtifactStore {
    root: PathBuf,
}

impl LocalArtifactStore {
    pub fn open(root: impl AsRef<Path>) -> Result<Self, ArtifactError> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(root.join(ARTIFACTS_DIR))
            .map_err(|source| ArtifactError::io(&root, source))?;
        fs::create_dir_all(root.join(CONTENTS_DIR))
            .map_err(|source| ArtifactError::io(&root, source))?;
        Ok(Self { root })
    }

    pub fn persist<R: ContentResolver + ?Sized>(
        &self,
        artifact: &Artifact,
        resolver: &R,
    ) -> Result<(), ArtifactError> {
        validate_recovered_artifact(artifact)?;

        for entry in &artifact.entries {
            self.persist_content(entry, resolver)?;
        }

        let artifact_path = self.artifact_path(&artifact.identity)?;
        write_durable(&artifact_path, &artifact.to_canonical_bytes()?)?;
        Ok(())
    }

    pub fn recover(&self, identity: &str) -> Result<RecoveredArtifact, ArtifactError> {
        let artifact_path = self.artifact_path(identity)?;
        let bytes =
            fs::read(&artifact_path).map_err(|source| ArtifactError::io(&artifact_path, source))?;
        let artifact: Artifact = serde_json::from_slice(&bytes)?;

        if artifact.identity != identity {
            return Err(ArtifactError::Serialization(format!(
                "stored artifact identity {} does not match requested identity {}",
                artifact.identity, identity
            )));
        }
        validate_recovered_artifact(&artifact)?;

        let mut content_paths = BTreeMap::new();
        for entry in &artifact.entries {
            let content_path = self.content_path(&entry.content_digest)?;
            verify_content(&content_path, entry)?;
            content_paths.insert(entry.path.clone(), content_path);
        }

        Ok(RecoveredArtifact {
            artifact,
            content_paths,
        })
    }

    pub fn artifact_path(&self, identity: &str) -> Result<PathBuf, ArtifactError> {
        Ok(self
            .root
            .join(ARTIFACTS_DIR)
            .join(digest_file_name(identity)?))
    }

    pub fn content_path(&self, digest: &str) -> Result<PathBuf, ArtifactError> {
        Ok(self.root.join(CONTENTS_DIR).join(digest_file_name(digest)?))
    }

    fn persist_content<R: ContentResolver + ?Sized>(
        &self,
        entry: &ArtifactEntry,
        resolver: &R,
    ) -> Result<(), ArtifactError> {
        let content_path = self.content_path(&entry.content_digest)?;
        let mut reader = resolver.resolve(&entry.path)?;
        let mut bytes = Vec::new();
        reader
            .read_to_end(&mut bytes)
            .map_err(|source| ArtifactError::io(&entry.path, source))?;

        let actual_digest = crate::core::sha256_prefixed(&bytes);
        if actual_digest != entry.content_digest {
            return Err(ArtifactError::Materialization(format!(
                "{}: content digest mismatch while persisting",
                entry.path
            )));
        }
        if bytes.len() as u64 != entry.size {
            return Err(ArtifactError::Materialization(format!(
                "{}: expected {} bytes, read {}",
                entry.path,
                entry.size,
                bytes.len()
            )));
        }

        if content_path.exists() {
            verify_content(&content_path, entry)?;
            return Ok(());
        }

        write_durable(&content_path, &bytes)?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct RecoveredArtifact {
    artifact: Artifact,
    content_paths: BTreeMap<String, PathBuf>,
}

impl RecoveredArtifact {
    pub fn artifact(&self) -> &Artifact {
        &self.artifact
    }
}

impl ContentResolver for RecoveredArtifact {
    fn resolve(&self, path: &str) -> Result<Box<dyn Read>, ArtifactError> {
        let content_path = self
            .content_paths
            .get(path)
            .ok_or_else(|| ArtifactError::Materialization(format!("missing content for {path}")))?;
        let file =
            File::open(content_path).map_err(|source| ArtifactError::io(content_path, source))?;
        Ok(Box::new(file))
    }
}

fn validate_recovered_artifact(artifact: &Artifact) -> Result<(), ArtifactError> {
    validate_entry_layout(&artifact.entries)?;

    if artifact.manifest.manifest_version != MANIFEST_VERSION {
        return Err(ArtifactError::Serialization(format!(
            "unsupported manifest version {}",
            artifact.manifest.manifest_version
        )));
    }
    if artifact.manifest.artifact_identity != artifact.identity
        || artifact.manifest.entries != artifact.entries
        || artifact.manifest.pipeline_identity != artifact.pipeline_identity
        || artifact.manifest.capabilities != artifact.capabilities
        || artifact.manifest.provenance != artifact.provenance
    {
        return Err(ArtifactError::Serialization(
            "artifact manifest does not match artifact fields".to_string(),
        ));
    }

    let rebuilt = Artifact::from_parts(
        artifact.entries.clone(),
        artifact.pipeline_identity.clone(),
        artifact.capabilities.clone(),
        artifact.provenance.clone(),
    )?;
    if rebuilt.identity != artifact.identity {
        return Err(ArtifactError::Serialization(format!(
            "artifact identity mismatch: expected {}, recomputed {}",
            artifact.identity, rebuilt.identity
        )));
    }

    Ok(())
}

fn verify_content(path: &Path, entry: &ArtifactEntry) -> Result<(), ArtifactError> {
    let metadata = fs::metadata(path).map_err(|source| {
        if source.kind() == io::ErrorKind::NotFound {
            ArtifactError::Materialization(format!("missing content for {}", entry.path))
        } else {
            ArtifactError::io(path, source)
        }
    })?;
    if metadata.len() != entry.size {
        return Err(ArtifactError::Materialization(format!(
            "{}: stored content size mismatch",
            entry.path
        )));
    }

    let mut file = File::open(path).map_err(|source| ArtifactError::io(path, source))?;
    let actual_digest = sha256_reader(&mut file)?;
    if actual_digest != entry.content_digest {
        return Err(ArtifactError::Materialization(format!(
            "{}: stored content digest mismatch",
            entry.path
        )));
    }

    Ok(())
}

fn write_durable(path: &Path, bytes: &[u8]) -> Result<(), ArtifactError> {
    let parent = path
        .parent()
        .ok_or_else(|| ArtifactError::InvalidState("storage path has no parent".to_string()))?;
    fs::create_dir_all(parent).map_err(|source| ArtifactError::io(parent, source))?;

    let temp_path = path.with_extension(format!("tmp.{}", std::process::id()));
    {
        let mut file =
            File::create(&temp_path).map_err(|source| ArtifactError::io(&temp_path, source))?;
        file.write_all(bytes)
            .map_err(|source| ArtifactError::io(&temp_path, source))?;
        file.sync_all()
            .map_err(|source| ArtifactError::io(&temp_path, source))?;
    }
    fs::rename(&temp_path, path).map_err(|source| ArtifactError::io(path, source))?;
    sync_directory(parent)?;
    Ok(())
}

fn sync_directory(path: &Path) -> Result<(), ArtifactError> {
    let directory = File::open(path).map_err(|source| ArtifactError::io(path, source))?;
    directory
        .sync_all()
        .map_err(|source| ArtifactError::io(path, source))
}

fn digest_file_name(digest: &str) -> Result<&str, ArtifactError> {
    let hex = digest.strip_prefix("sha256:").ok_or_else(|| {
        ArtifactError::InvalidState(format!("unsupported digest format: {digest}"))
    })?;
    if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ArtifactError::InvalidState(format!(
            "invalid sha256 digest: {digest}"
        )));
    }
    Ok(hex)
}
