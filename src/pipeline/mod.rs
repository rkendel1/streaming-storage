use crate::core::{
    Artifact, ArtifactEntry, ArtifactError, Capability, CreationMetadata, EntryType, Provenance,
    canonical_capabilities, normalize_relative_path, sha256_prefixed,
};
use serde::Serialize;
use sha2::Digest;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceSpec {
    Directory,
}

impl SourceSpec {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Directory => "directory",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SelectStageSpec {
    pub exclude_exact: Vec<String>,
    pub exclude_prefixes: Vec<String>,
}

impl SelectStageSpec {
    pub fn new(
        exclude_exact: Vec<String>,
        exclude_prefixes: Vec<String>,
    ) -> Result<Self, ArtifactError> {
        Ok(Self {
            exclude_exact: canonical_path_list(exclude_exact)?,
            exclude_prefixes: canonical_path_list(exclude_prefixes)?,
        })
    }

    fn allows(&self, path: &str) -> bool {
        if self.exclude_exact.iter().any(|candidate| candidate == path) {
            return false;
        }

        !self.exclude_prefixes.iter().any(|prefix| {
            path == prefix
                || path
                    .strip_prefix(prefix)
                    .is_some_and(|suffix| suffix.starts_with('/'))
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StageSpec {
    Select(SelectStageSpec),
    Manifest,
    Validate,
}

impl StageSpec {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Select(_) => "select",
            Self::Manifest => "manifest",
            Self::Validate => "validate",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MaterializerSpec {
    Zip,
    Tar,
}

impl MaterializerSpec {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::Tar => "tar",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PipelineSpec {
    pub source: SourceSpec,
    pub stages: Vec<StageSpec>,
    pub materializer: MaterializerSpec,
    pub capabilities: Vec<Capability>,
}

impl PipelineSpec {
    pub fn identity(&self) -> Result<String, ArtifactError> {
        self.validate_stage_sequence()?;
        Ok(sha256_prefixed(&self.to_canonical_bytes()?))
    }

    pub fn to_canonical_bytes(&self) -> Result<Vec<u8>, ArtifactError> {
        #[derive(Serialize)]
        struct CanonicalPipeline<'a> {
            schema: &'static str,
            source: &'a SourceSpec,
            stages: Vec<CanonicalStage>,
            materializer: &'a MaterializerSpec,
            capabilities: Vec<Capability>,
        }

        #[derive(Serialize)]
        struct CanonicalStage {
            kind: &'static str,
            #[serde(skip_serializing_if = "Option::is_none")]
            exclude_exact: Option<Vec<String>>,
            #[serde(skip_serializing_if = "Option::is_none")]
            exclude_prefixes: Option<Vec<String>>,
        }

        let stages = self
            .stages
            .iter()
            .map(|stage| match stage {
                StageSpec::Select(spec) => CanonicalStage {
                    kind: "select",
                    exclude_exact: Some(spec.exclude_exact.clone()),
                    exclude_prefixes: Some(spec.exclude_prefixes.clone()),
                },
                StageSpec::Manifest => CanonicalStage {
                    kind: "manifest",
                    exclude_exact: None,
                    exclude_prefixes: None,
                },
                StageSpec::Validate => CanonicalStage {
                    kind: "validate",
                    exclude_exact: None,
                    exclude_prefixes: None,
                },
            })
            .collect();

        let canonical = CanonicalPipeline {
            schema: "pipeline.v1",
            source: &self.source,
            stages,
            materializer: &self.materializer,
            capabilities: canonical_capabilities(self.capabilities.clone()),
        };

        Ok(serde_json::to_vec(&canonical)?)
    }

    pub fn build_from_directory(
        &self,
        root: impl AsRef<Path>,
    ) -> Result<SourceBackedArtifact, ArtifactError> {
        if self.source != SourceSpec::Directory {
            return Err(ArtifactError::InvalidState(
                "Phase 1 only supports directory sources".to_string(),
            ));
        }

        self.validate_stage_sequence()?;
        let root = root.as_ref();
        let pipeline_identity = self.identity()?;
        let discovered = DirectorySource::new(root)?.discover()?;
        let mut state = PipelineState::new(discovered);

        for stage in &self.stages {
            state.stage_trace.push(stage.label().to_string());
            match stage {
                StageSpec::Select(spec) => state.apply_select(spec),
                StageSpec::Manifest => {
                    state.generate_manifest_seed(&pipeline_identity, &self.capabilities)?
                }
                StageSpec::Validate => state.validate()?,
            }
        }

        let seed = state
            .manifest_seed
            .take()
            .ok_or(ArtifactError::MissingStageOutput("manifest"))?;
        let artifact = Artifact::from_parts(
            seed.entries,
            pipeline_identity,
            seed.capabilities,
            seed.provenance,
        )?;

        Ok(SourceBackedArtifact {
            artifact,
            contents: seed.contents,
            stage_trace: state.stage_trace,
        })
    }

    fn validate_stage_sequence(&self) -> Result<(), ArtifactError> {
        let mut seen_manifest = false;
        let mut seen_validate = false;

        for stage in &self.stages {
            if seen_validate {
                return Err(ArtifactError::InvalidState(
                    "validate must be the final stage in Phase 1 pipelines".to_string(),
                ));
            }

            match stage {
                StageSpec::Select(_) if seen_manifest => {
                    return Err(ArtifactError::InvalidState(
                        "select stages must appear before manifest".to_string(),
                    ));
                }
                StageSpec::Select(_) => {}
                StageSpec::Manifest if seen_manifest => {
                    return Err(ArtifactError::InvalidState(
                        "manifest stage may only appear once".to_string(),
                    ));
                }
                StageSpec::Manifest => seen_manifest = true,
                StageSpec::Validate if !seen_manifest => {
                    return Err(ArtifactError::InvalidState(
                        "validate requires a preceding manifest stage".to_string(),
                    ));
                }
                StageSpec::Validate => seen_validate = true,
            }
        }

        if !seen_manifest {
            return Err(ArtifactError::MissingStageOutput("manifest"));
        }
        if !seen_validate {
            return Err(ArtifactError::MissingStageOutput("validate"));
        }

        Ok(())
    }
}

pub fn default_directory_zip_pipeline() -> PipelineSpec {
    PipelineSpec {
        source: SourceSpec::Directory,
        stages: vec![
            StageSpec::Select(
                SelectStageSpec::new(vec![".env".to_string()], vec!["node_modules".to_string()])
                    .expect("default selection is valid"),
            ),
            StageSpec::Manifest,
            StageSpec::Validate,
        ],
        materializer: MaterializerSpec::Zip,
        capabilities: vec![
            Capability::new("filesystem.read", "1"),
            Capability::new("manifest.generate", "1"),
            Capability::new("artifact.validate", "1"),
            Capability::new("package.zip", "1"),
        ],
    }
}

pub trait ContentResolver: Send + Sync {
    fn resolve(&self, path: &str) -> Result<Box<dyn Read>, ArtifactError>;
}

pub trait EntryContentResolver {
    fn open(&self, path: &str) -> Result<Box<dyn Read>, ArtifactError>;
}

impl<T: ContentResolver + ?Sized> EntryContentResolver for T {
    fn open(&self, path: &str) -> Result<Box<dyn Read>, ArtifactError> {
        self.resolve(path)
    }
}

#[derive(Debug)]
pub struct SourceBackedArtifact {
    artifact: Artifact,
    contents: BTreeMap<String, PathBuf>,
    stage_trace: Vec<String>,
}

impl SourceBackedArtifact {
    pub fn artifact(&self) -> &Artifact {
        &self.artifact
    }

    pub fn stage_trace(&self) -> &[String] {
        &self.stage_trace
    }
}

impl ContentResolver for SourceBackedArtifact {
    fn resolve(&self, path: &str) -> Result<Box<dyn Read>, ArtifactError> {
        let source_path = self
            .contents
            .get(path)
            .ok_or_else(|| ArtifactError::Materialization(format!("missing content for {path}")))?;
        let file =
            File::open(source_path).map_err(|source| ArtifactError::io(source_path, source))?;
        Ok(Box::new(file))
    }
}

#[derive(Debug)]
struct DirectorySource {
    root: PathBuf,
}

impl DirectorySource {
    fn new(root: impl AsRef<Path>) -> Result<Self, ArtifactError> {
        let root = root.as_ref();
        let metadata = fs::metadata(root).map_err(|source| ArtifactError::io(root, source))?;
        if !metadata.is_dir() {
            return Err(ArtifactError::InvalidState(format!(
                "{} is not a directory source",
                root.display()
            )));
        }
        Ok(Self {
            root: root.to_path_buf(),
        })
    }

    fn discover(&self) -> Result<Vec<DiscoveredFile>, ArtifactError> {
        let mut files = Vec::new();
        self.walk(&self.root, &mut files)?;
        files.sort_by(|left, right| left.normalized_path.cmp(&right.normalized_path));
        Ok(files)
    }

    fn walk(&self, directory: &Path, files: &mut Vec<DiscoveredFile>) -> Result<(), ArtifactError> {
        let mut children = fs::read_dir(directory)
            .map_err(|source| ArtifactError::io(directory, source))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| ArtifactError::io(directory, source))?;
        children.sort_by_key(|entry| entry.file_name());

        for child in children {
            let path = child.path();
            let metadata =
                fs::symlink_metadata(&path).map_err(|source| ArtifactError::io(&path, source))?;
            let file_type = metadata.file_type();

            if file_type.is_symlink() {
                return Err(ArtifactError::SymlinkNotSupported(path));
            }

            if file_type.is_dir() {
                self.walk(&path, files)?;
                continue;
            }

            if !file_type.is_file() {
                return Err(ArtifactError::UnsupportedFileType(path));
            }

            let relative = path.strip_prefix(&self.root).map_err(|_| {
                ArtifactError::InvalidState("failed to derive relative source path".to_string())
            })?;
            let normalized_path = normalize_fs_path(relative)?;
            let size = metadata.len();
            let content_digest = hash_file(&path)?;

            files.push(DiscoveredFile {
                normalized_path,
                source_path: path,
                size,
                content_digest,
            });
        }

        Ok(())
    }
}

#[derive(Clone, Debug)]
struct DiscoveredFile {
    normalized_path: String,
    source_path: PathBuf,
    size: u64,
    content_digest: String,
}

#[derive(Debug)]
struct PipelineState {
    discovered: Vec<DiscoveredFile>,
    selected: Vec<DiscoveredFile>,
    manifest_seed: Option<ManifestSeed>,
    stage_trace: Vec<String>,
}

impl PipelineState {
    fn new(discovered: Vec<DiscoveredFile>) -> Self {
        Self {
            selected: discovered.clone(),
            discovered,
            manifest_seed: None,
            stage_trace: Vec::new(),
        }
    }

    fn apply_select(&mut self, selection: &SelectStageSpec) {
        self.selected = self
            .discovered
            .iter()
            .filter(|entry| selection.allows(&entry.normalized_path))
            .cloned()
            .collect();
    }

    fn generate_manifest_seed(
        &mut self,
        pipeline_identity: &str,
        capabilities: &[Capability],
    ) -> Result<(), ArtifactError> {
        let entries = self
            .selected
            .iter()
            .map(|file| ArtifactEntry {
                path: file.normalized_path.clone(),
                entry_type: EntryType::File,
                size: file.size,
                content_digest: file.content_digest.clone(),
            })
            .collect::<Vec<_>>();
        let source_identity = compute_source_identity(&entries)?;
        let contents = self
            .selected
            .iter()
            .map(|file| (file.normalized_path.clone(), file.source_path.clone()))
            .collect();

        self.manifest_seed = Some(ManifestSeed {
            entries,
            capabilities: canonical_capabilities(capabilities.to_vec()),
            provenance: Provenance {
                source_identity,
                pipeline_identity: pipeline_identity.to_string(),
                creation_metadata: CreationMetadata::default(),
            },
            contents,
        });
        Ok(())
    }

    fn validate(&self) -> Result<(), ArtifactError> {
        let seed = self
            .manifest_seed
            .as_ref()
            .ok_or(ArtifactError::MissingStageOutput("manifest"))?;
        crate::core::validate_entry_layout(&seed.entries)?;
        Ok(())
    }
}

#[derive(Debug)]
struct ManifestSeed {
    entries: Vec<ArtifactEntry>,
    capabilities: Vec<Capability>,
    provenance: Provenance,
    contents: BTreeMap<String, PathBuf>,
}

fn compute_source_identity(entries: &[ArtifactEntry]) -> Result<String, ArtifactError> {
    #[derive(Serialize)]
    struct CanonicalSource<'a> {
        schema: &'static str,
        entries: &'a [ArtifactEntry],
    }

    Ok(sha256_prefixed(&serde_json::to_vec(&CanonicalSource {
        schema: "source.directory.v1",
        entries,
    })?))
}

fn canonical_path_list(values: Vec<String>) -> Result<Vec<String>, ArtifactError> {
    let mut paths = BTreeSet::new();
    for value in values {
        paths.insert(normalize_relative_path(&value)?);
    }
    Ok(paths.into_iter().collect())
}

fn normalize_fs_path(path: &Path) -> Result<String, ArtifactError> {
    let raw = path
        .to_str()
        .ok_or_else(|| ArtifactError::InvalidState("source path is not valid UTF-8".to_string()))?;
    normalize_relative_path(raw)
}

fn hash_file(path: &Path) -> Result<String, ArtifactError> {
    let mut file = File::open(path).map_err(|source| ArtifactError::io(path, source))?;
    let mut buffer = [0_u8; 8192];
    let mut hasher = sha2::Sha256::new();

    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|source| ArtifactError::io(path, source))?;
        if read == 0 {
            break;
        }
        sha2::Digest::update(&mut hasher, &buffer[..read]);
    }

    let digest = sha2::Digest::finalize(hasher);
    let mut encoded = String::from("sha256:");
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(encoded, "{byte:02x}");
    }
    Ok(encoded)
}

#[derive(Debug)]
pub struct MemoryContentResolver {
    entries: BTreeMap<String, Vec<u8>>,
}

impl MemoryContentResolver {
    pub fn new(entries: BTreeMap<String, Vec<u8>>) -> Self {
        Self { entries }
    }
}

impl ContentResolver for MemoryContentResolver {
    fn resolve(&self, path: &str) -> Result<Box<dyn Read>, ArtifactError> {
        let bytes = self
            .entries
            .get(path)
            .cloned()
            .ok_or_else(|| ArtifactError::Materialization(format!("missing content for {path}")))?;
        Ok(Box::new(std::io::Cursor::new(bytes)))
    }
}
