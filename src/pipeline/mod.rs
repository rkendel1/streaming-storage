use crate::core::{
    Artifact, ArtifactEntry, ArtifactError, Capability, CreationMetadata, EntryType, Provenance,
    canonical_capabilities, normalize_relative_path, sha256_prefixed,
};
use crate::transforms::ArtifactTransform;
use serde::Serialize;
use sha2::Digest;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

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
pub struct TransformStageSpec {
    pub prefix: String,
}

impl TransformStageSpec {
    pub fn new(prefix: impl Into<String>) -> Result<Self, ArtifactError> {
        let prefix = prefix.into();
        normalize_relative_path(&prefix)?;
        Ok(Self { prefix })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RedactStageSpec {
    pub paths: Vec<String>,
}

impl RedactStageSpec {
    pub fn new(paths: Vec<String>) -> Result<Self, ArtifactError> {
        let paths = canonical_path_list(paths)?;
        Ok(Self { paths })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GenerateStageSpec {
    pub path: String,
    pub content: String,
}

impl GenerateStageSpec {
    pub fn new(path: impl Into<String>, content: impl Into<String>) -> Result<Self, ArtifactError> {
        let path = path.into();
        normalize_relative_path(&path)?;
        Ok(Self {
            path,
            content: content.into(),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StageSpec {
    Select(SelectStageSpec),
    Transform(TransformStageSpec),
    Redact(RedactStageSpec),
    Generate(GenerateStageSpec),
    Manifest,
    Validate,
}

impl StageSpec {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Select(_) => "select",
            Self::Transform(_) => "transform",
            Self::Redact(_) => "redact",
            Self::Generate(_) => "generate",
            Self::Manifest => "manifest",
            Self::Validate => "validate",
        }
    }

    pub fn identity(&self) -> Result<String, ArtifactError> {
        let canonical = serde_json::json!({
            "kind": self.label(),
            "spec": self,
        });
        Ok(sha256_prefixed(&serde_json::to_vec(&canonical)?))
    }
}

pub struct StageOutput {
    pub artifact: Artifact,
    pub resolver: Arc<dyn ContentResolver>,
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

pub struct PipelineExecutor;

impl PipelineExecutor {
    pub fn execute_stage(
        artifact: &Artifact,
        resolver: Arc<dyn ContentResolver>,
        stage: &StageSpec,
    ) -> Result<StageOutput, ArtifactError> {
        match stage {
            StageSpec::Select(spec) => Self::execute_select(artifact, resolver, spec),
            StageSpec::Transform(spec) => Self::execute_transform(artifact, resolver, spec),
            StageSpec::Redact(spec) => Self::execute_redact(artifact, resolver, spec),
            StageSpec::Generate(spec) => Self::execute_generate(artifact, resolver, spec),
            StageSpec::Manifest => Self::execute_manifest(artifact, resolver),
            StageSpec::Validate => Self::execute_validate(artifact, resolver),
        }
    }

    fn execute_select(
        artifact: &Artifact,
        resolver: Arc<dyn ContentResolver>,
        spec: &SelectStageSpec,
    ) -> Result<StageOutput, ArtifactError> {
        let filtered_entries: Vec<ArtifactEntry> = artifact
            .entries
            .iter()
            .filter(|entry| spec.allows(&entry.path))
            .cloned()
            .collect();

        let new_artifact = Artifact::from_parts(
            filtered_entries,
            artifact.pipeline_identity.clone(),
            artifact.capabilities.clone(),
            artifact.provenance.clone(),
        )?;

        Ok(StageOutput {
            artifact: new_artifact,
            resolver,
        })
    }

    fn execute_transform(
        artifact: &Artifact,
        resolver: Arc<dyn ContentResolver>,
        spec: &TransformStageSpec,
    ) -> Result<StageOutput, ArtifactError> {
        use crate::transforms::PrefixTransform;

        let transform = PrefixTransform::new(&spec.prefix);
        let result = transform.apply(artifact, resolver.as_ref())?;

        Ok(StageOutput {
            artifact: result.artifact,
            resolver: Arc::new(TransformedContentResolver::new(result.content_updates)),
        })
    }

    fn execute_redact(
        artifact: &Artifact,
        resolver: Arc<dyn ContentResolver>,
        spec: &RedactStageSpec,
    ) -> Result<StageOutput, ArtifactError> {
        use crate::transforms::RedactTransform;

        let transform = RedactTransform::new(spec.paths.clone());
        let result = transform.apply(artifact, resolver.as_ref())?;

        Ok(StageOutput {
            artifact: result.artifact,
            resolver: Arc::new(RedactedContentResolver {
                removed_paths: spec.paths.clone(),
                inner: resolver,
            }),
        })
    }

    fn execute_generate(
        artifact: &Artifact,
        resolver: Arc<dyn ContentResolver>,
        spec: &GenerateStageSpec,
    ) -> Result<StageOutput, ArtifactError> {
        use crate::transforms::GenerateTransform;

        let transform = GenerateTransform::new(&spec.path, spec.content.as_bytes());
        let result = transform.apply(artifact, resolver.as_ref())?;

        Ok(StageOutput {
            artifact: result.artifact,
            resolver: Arc::new(TransformedContentResolver::new(result.content_updates)),
        })
    }

    fn execute_manifest(
        artifact: &Artifact,
        resolver: Arc<dyn ContentResolver>,
    ) -> Result<StageOutput, ArtifactError> {
        Ok(StageOutput {
            artifact: artifact.clone(),
            resolver,
        })
    }

    fn execute_validate(
        artifact: &Artifact,
        resolver: Arc<dyn ContentResolver>,
    ) -> Result<StageOutput, ArtifactError> {
        crate::core::validate_entry_layout(&artifact.entries)?;
        Ok(StageOutput {
            artifact: artifact.clone(),
            resolver,
        })
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
        let canonical = serde_json::json!({
            "schema": "pipeline.v1",
            "source": &self.source,
            "stages": &self.stages,
            "materializer": &self.materializer,
            "capabilities": canonical_capabilities(self.capabilities.clone()),
        });

        Ok(serde_json::to_vec(&canonical)?)
    }

    pub fn required_capabilities(&self) -> Vec<Capability> {
        canonical_capabilities(self.capabilities.clone())
    }

    pub fn build_with_authorization(
        &self,
        root: impl AsRef<Path>,
        policy: &dyn crate::authorization::CapabilityPolicy,
    ) -> Result<(SourceBackedArtifact, crate::authorization::ExecutionEvidence), ArtifactError> {
        let pipeline_identity = self.identity()?;
        let requested_capabilities = self.required_capabilities();
        let policy_identity = policy.identity();

        let granted_capabilities = match policy.authorize(&requested_capabilities) {
            Ok(granted) => granted,
            Err(_) => {
                let decision = crate::authorization::AuthorizationDecision::denied(
                    pipeline_identity.clone(),
                    requested_capabilities.clone(),
                    vec![],
                    policy_identity,
                );
                let _evidence = crate::authorization::ExecutionEvidence::failed(
                    pipeline_identity,
                    decision,
                    vec![],
                    requested_capabilities,
                    vec![],
                    "authorization denied: not all capabilities granted".to_string(),
                );
                return Err(ArtifactError::InvalidState(
                    "authorization denied: not all capabilities granted".to_string(),
                ));
            }
        };

        let decision = crate::authorization::AuthorizationDecision::allowed(
            pipeline_identity.clone(),
            requested_capabilities.clone(),
            policy_identity,
        );

        match self.build_from_directory(root) {
            Ok(artifact) => {
                let stage_trace: Vec<crate::authorization::ExecutedStage> = artifact
                    .stage_trace()
                    .iter()
                    .map(|label| crate::authorization::ExecutedStage {
                        label: label.clone(),
                        stage_identity: String::new(),
                    })
                    .collect();

                let evidence = crate::authorization::ExecutionEvidence::success(
                    pipeline_identity,
                    artifact.artifact(),
                    decision,
                    stage_trace,
                    requested_capabilities,
                    granted_capabilities,
                );

                Ok((artifact, evidence))
            }
            Err(e) => {
                let _evidence = crate::authorization::ExecutionEvidence::failed(
                    pipeline_identity,
                    decision,
                    vec![],
                    requested_capabilities,
                    granted_capabilities,
                    e.to_string(),
                );
                Err(e)
            }
        }
    }

    pub fn build_from_directory(
        &self,
        root: impl AsRef<Path>,
    ) -> Result<SourceBackedArtifact, ArtifactError> {
        if self.source != SourceSpec::Directory {
            return Err(ArtifactError::InvalidState(
                "only directory sources are supported".to_string(),
            ));
        }

        self.validate_stage_sequence()?;
        let root = root.as_ref();
        let pipeline_identity = self.identity()?;

        let directory = DirectorySource::new(root)?;
        let discovered = directory.discover()?;

        let mut state = PipelineState::new(discovered);
        state.generate_manifest_seed(&pipeline_identity, &self.capabilities)?;

        let seed = state
            .manifest_seed
            .take()
            .ok_or(ArtifactError::MissingStageOutput("manifest"))?;

        let initial_artifact = Artifact::from_parts(
            seed.entries,
            pipeline_identity.clone(),
            seed.capabilities.clone(),
            seed.provenance.clone(),
        )?;

        let source_backed = SourceBackedArtifact {
            artifact: initial_artifact.clone(),
            contents: seed.contents.clone(),
            stage_trace: vec!["source".to_string()],
            resolver: None,
        };

        let mut stage_trace = vec!["source".to_string()];
        let mut current = StageOutput {
            artifact: initial_artifact,
            resolver: Arc::new(source_backed),
        };

        for stage in &self.stages {
            stage_trace.push(stage.label().to_string());
            current = PipelineExecutor::execute_stage(&current.artifact, current.resolver.clone(), stage)?;
        }

        Ok(SourceBackedArtifact {
            artifact: current.artifact,
            contents: seed.contents,
            stage_trace,
            resolver: Some(current.resolver),
        })
    }

    fn validate_stage_sequence(&self) -> Result<(), ArtifactError> {
        let mut seen_manifest = false;
        let mut seen_validate = false;

        for stage in &self.stages {
            if seen_validate {
                return Err(ArtifactError::InvalidState(
                    "validate must be the final stage".to_string(),
                ));
            }

            match stage {
                StageSpec::Select(_) | StageSpec::Transform(_) | StageSpec::Redact(_)
                | StageSpec::Generate(_) if seen_manifest => {
                    return Err(ArtifactError::InvalidState(
                        "source and transformation stages must appear before manifest".to_string(),
                    ));
                }
                StageSpec::Select(_) | StageSpec::Transform(_) | StageSpec::Redact(_)
                | StageSpec::Generate(_) => {}
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

pub struct SourceBackedArtifact {
    artifact: Artifact,
    contents: BTreeMap<String, PathBuf>,
    stage_trace: Vec<String>,
    resolver: Option<Arc<dyn ContentResolver>>,
}

impl std::fmt::Debug for SourceBackedArtifact {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourceBackedArtifact")
            .field("artifact", &self.artifact)
            .field("contents", &self.contents)
            .field("stage_trace", &self.stage_trace)
            .field("resolver", &"<ContentResolver>")
            .finish()
    }
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
        if let Some(resolver) = &self.resolver {
            return resolver.resolve(path);
        }

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

struct RedactedContentResolver {
    removed_paths: Vec<String>,
    inner: Arc<dyn ContentResolver>,
}

impl ContentResolver for RedactedContentResolver {
    fn resolve(&self, path: &str) -> Result<Box<dyn Read>, ArtifactError> {
        if self.removed_paths.contains(&path.to_string()) {
            return Err(ArtifactError::Materialization(format!(
                "cannot resolve redacted path: {path}"
            )));
        }
        self.inner.resolve(path)
    }
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

#[derive(Debug)]
pub struct TransformedContentResolver {
    entries: BTreeMap<String, Vec<u8>>,
}

impl TransformedContentResolver {
    pub fn new(entries: BTreeMap<String, Vec<u8>>) -> Self {
        Self { entries }
    }
}

impl ContentResolver for TransformedContentResolver {
    fn resolve(&self, path: &str) -> Result<Box<dyn Read>, ArtifactError> {
        let bytes = self
            .entries
            .get(path)
            .cloned()
            .ok_or_else(|| ArtifactError::Materialization(format!("missing content for {path}")))?;
        Ok(Box::new(std::io::Cursor::new(bytes)))
    }
}
