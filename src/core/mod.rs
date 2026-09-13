use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

pub const MANIFEST_VERSION: u32 = 1;
pub const MAX_PATH_LENGTH: usize = 4096;

#[derive(Debug)]
pub enum ArtifactError {
    Io {
        path: PathBuf,
        source: io::Error,
    },
    InvalidPath(String),
    PathTooLong(String),
    DuplicatePath(String),
    ConflictingPath {
        existing: String,
        conflicting: String,
    },
    SymlinkNotSupported(PathBuf),
    UnsupportedFileType(PathBuf),
    MissingStageOutput(&'static str),
    InvalidState(String),
    Serialization(String),
    Materialization(String),
}

impl ArtifactError {
    pub fn io(path: impl AsRef<Path>, source: io::Error) -> Self {
        Self::Io {
            path: path.as_ref().to_path_buf(),
            source,
        }
    }
}

impl fmt::Display for ArtifactError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "{}: {}", path.display(), source),
            Self::InvalidPath(path) => write!(f, "invalid artifact path: {path}"),
            Self::PathTooLong(path) => {
                write!(f, "artifact path exceeds {MAX_PATH_LENGTH} bytes: {path}")
            }
            Self::DuplicatePath(path) => write!(f, "duplicate artifact path: {path}"),
            Self::ConflictingPath {
                existing,
                conflicting,
            } => {
                write!(
                    f,
                    "conflicting artifact paths: {existing} conflicts with {conflicting}"
                )
            }
            Self::SymlinkNotSupported(path) => {
                write!(
                    f,
                    "symlinks are rejected for security reasons: {}",
                    path.display()
                )
            }
            Self::UnsupportedFileType(path) => {
                write!(f, "unsupported filesystem object: {}", path.display())
            }
            Self::MissingStageOutput(name) => write!(f, "pipeline stage output missing: {name}"),
            Self::InvalidState(message) => write!(f, "invalid pipeline state: {message}"),
            Self::Serialization(message) => write!(f, "serialization error: {message}"),
            Self::Materialization(message) => write!(f, "materialization error: {message}"),
        }
    }
}

impl std::error::Error for ArtifactError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<serde_json::Error> for ArtifactError {
    fn from(value: serde_json::Error) -> Self {
        Self::Serialization(value.to_string())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Ord, PartialOrd, Serialize)]
pub struct Capability {
    pub name: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub parameters: BTreeMap<String, String>,
}

impl Capability {
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            parameters: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryType {
    File,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Ord, PartialOrd, Serialize)]
pub struct ArtifactEntry {
    pub path: String,
    pub entry_type: EntryType,
    pub size: u64,
    pub content_digest: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CreationMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Provenance {
    pub source_identity: String,
    pub pipeline_identity: String,
    pub creation_metadata: CreationMetadata,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TransformationRecord {
    pub input_artifact_identity: String,
    pub transform_identity: String,
    pub transform_kind: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Manifest {
    pub manifest_version: u32,
    pub artifact_identity: String,
    pub entries: Vec<ArtifactEntry>,
    pub pipeline_identity: String,
    pub capabilities: Vec<Capability>,
    pub provenance: Provenance,
}

impl Manifest {
    pub fn to_canonical_bytes(&self) -> Result<Vec<u8>, ArtifactError> {
        Ok(serde_json::to_vec(self)?)
    }

    pub fn to_canonical_json(&self) -> Result<String, ArtifactError> {
        Ok(String::from_utf8(self.to_canonical_bytes()?).expect("canonical manifest is utf-8"))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Artifact {
    pub identity: String,
    pub entries: Vec<ArtifactEntry>,
    pub manifest: Manifest,
    pub pipeline_identity: String,
    pub capabilities: Vec<Capability>,
    pub provenance: Provenance,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lineage: Vec<TransformationRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic_declaration: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MaterializationResult {
    pub artifact_identity: String,
    pub materializer_format: String,
    pub output_digest: String,
    pub size_bytes: u64,
}

impl Artifact {
    pub fn from_parts(
        entries: Vec<ArtifactEntry>,
        pipeline_identity: String,
        capabilities: Vec<Capability>,
        provenance: Provenance,
    ) -> Result<Self, ArtifactError> {
        let entries = sort_entries(entries);
        validate_entry_layout(&entries)?;
        let capabilities = canonical_capabilities(capabilities);
        let identity =
            compute_artifact_identity(&entries, &pipeline_identity, &capabilities, &provenance)?;
        let manifest = Manifest {
            manifest_version: MANIFEST_VERSION,
            artifact_identity: identity.clone(),
            entries: entries.clone(),
            pipeline_identity: pipeline_identity.clone(),
            capabilities: capabilities.clone(),
            provenance: provenance.clone(),
        };

        Ok(Self {
            identity,
            entries,
            manifest,
            pipeline_identity,
            capabilities,
            provenance,
            lineage: Vec::new(),
            semantic_declaration: None,
        })
    }

    pub fn with_transformation_record(mut self, record: TransformationRecord) -> Self {
        self.lineage.push(record);
        self
    }

    pub fn lineage(&self) -> &[TransformationRecord] {
        &self.lineage
    }

    pub fn with_semantic_declaration(mut self, declaration: impl Into<String>) -> Self {
        self.semantic_declaration = Some(declaration.into());
        self
    }

    pub fn semantic_type(&self) -> Option<&str> {
        self.semantic_declaration.as_deref()
    }

    pub fn total_size(&self) -> u64 {
        self.entries.iter().map(|entry| entry.size).sum()
    }

    pub fn to_canonical_bytes(&self) -> Result<Vec<u8>, ArtifactError> {
        Ok(serde_json::to_vec(self)?)
    }
}

pub fn normalize_relative_path(raw: &str) -> Result<String, ArtifactError> {
    let candidate = raw.replace('\\', "/");

    if candidate.starts_with('/') || is_windows_absolute_path(&candidate) {
        return Err(ArtifactError::InvalidPath(raw.to_string()));
    }

    let mut segments = Vec::new();
    for segment in candidate.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." {
            return Err(ArtifactError::InvalidPath(raw.to_string()));
        }
        if segment.contains('\0') {
            return Err(ArtifactError::InvalidPath(raw.to_string()));
        }
        segments.push(segment);
    }

    if segments.is_empty() {
        return Err(ArtifactError::InvalidPath(raw.to_string()));
    }

    let normalized = segments.join("/");
    if normalized.len() > MAX_PATH_LENGTH {
        return Err(ArtifactError::PathTooLong(normalized));
    }

    Ok(normalized)
}

pub fn validate_entry_layout(entries: &[ArtifactEntry]) -> Result<(), ArtifactError> {
    let mut seen = BTreeSet::new();
    let mut sorted_paths = entries
        .iter()
        .map(|entry| entry.path.clone())
        .collect::<Vec<_>>();
    sorted_paths.sort();

    for entry in entries {
        let normalized = normalize_relative_path(&entry.path)?;
        if normalized != entry.path {
            return Err(ArtifactError::InvalidPath(entry.path.clone()));
        }
        if !seen.insert(entry.path.clone()) {
            return Err(ArtifactError::DuplicatePath(entry.path.clone()));
        }
    }

    for pair in sorted_paths.windows(2) {
        let current = &pair[0];
        let next = &pair[1];
        let prefix = format!("{current}/");
        if next.starts_with(&prefix) {
            return Err(ArtifactError::ConflictingPath {
                existing: current.clone(),
                conflicting: next.clone(),
            });
        }
    }

    Ok(())
}

pub fn sha256_prefixed(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format_digest(hasher.finalize())
}

pub fn sha256_reader<R: Read>(reader: &mut R) -> Result<String, ArtifactError> {
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 8192];

    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|source| ArtifactError::io("<stream>", source))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok(format_digest(hasher.finalize()))
}

pub fn canonical_capabilities(capabilities: Vec<Capability>) -> Vec<Capability> {
    let mut capabilities = capabilities;
    capabilities.sort();
    capabilities.dedup();
    capabilities
}

fn compute_artifact_identity(
    entries: &[ArtifactEntry],
    pipeline_identity: &str,
    capabilities: &[Capability],
    provenance: &Provenance,
) -> Result<String, ArtifactError> {
    #[derive(Serialize)]
    struct CanonicalProvenance<'a> {
        source_identity: &'a str,
    }

    #[derive(Serialize)]
    struct CanonicalArtifact<'a> {
        schema: &'static str,
        entries: &'a [ArtifactEntry],
        pipeline_identity: &'a str,
        capabilities: &'a [Capability],
        provenance: CanonicalProvenance<'a>,
    }

    let canonical = CanonicalArtifact {
        schema: "artifact.v1",
        entries,
        pipeline_identity,
        capabilities,
        provenance: CanonicalProvenance {
            source_identity: &provenance.source_identity,
        },
    };

    Ok(sha256_prefixed(&serde_json::to_vec(&canonical)?))
}

fn sort_entries(mut entries: Vec<ArtifactEntry>) -> Vec<ArtifactEntry> {
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    entries
}

fn is_windows_absolute_path(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 3 && bytes[1] == b':' && bytes[2] == b'/' && bytes[0].is_ascii_alphabetic()
}

fn format_digest(digest: impl AsRef<[u8]>) -> String {
    let mut encoded = String::with_capacity(digest.as_ref().len() * 2 + 7);
    encoded.push_str("sha256:");
    for byte in digest.as_ref() {
        use std::fmt::Write as _;
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}
