pub mod server;

use artifact::{
    default_directory_zip_pipeline, Artifact, ArtifactError, ContentResolver, LocalArtifactStore,
    MaterializationResult, PipelineSpec, SourceBackedArtifact, TarMaterializer, ZipMaterializer,
};
use artifact_runtime_consumer::{RuntimeConsumer, RuntimeExecution};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum WorkbenchError {
    Artifact(ArtifactError),
    Io(std::io::Error),
    InvalidSelection(String),
    MissingState(&'static str),
}

impl std::fmt::Display for WorkbenchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Artifact(error) => write!(f, "{error}"),
            Self::Io(error) => write!(f, "{error}"),
            Self::InvalidSelection(message) => write!(f, "{message}"),
            Self::MissingState(name) => write!(f, "missing workbench state: {name}"),
        }
    }
}

impl std::error::Error for WorkbenchError {}

impl From<ArtifactError> for WorkbenchError {
    fn from(value: ArtifactError) -> Self {
        Self::Artifact(value)
    }
}

impl From<std::io::Error> for WorkbenchError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityState {
    Available,
    ExternalConsumer,
    Unavailable,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WorkbenchCapability {
    pub id: String,
    pub label: String,
    pub state: CapabilityState,
    pub detail: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CapabilitiesView {
    pub outputs: Vec<WorkbenchCapability>,
    pub targets: Vec<WorkbenchCapability>,
    pub pipelines: Vec<WorkbenchCapability>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SourceView {
    pub root: String,
    pub source_kind: String,
    pub entries: Vec<String>,
    pub total_entries: usize,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct BuildOptions {
    #[serde(default)]
    pub semantic_declaration: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ArtifactView {
    pub identity: String,
    pub entries: usize,
    pub size_bytes: u64,
    pub pipeline_identity: String,
    pub pipeline: String,
    pub capabilities: Vec<String>,
    pub lineage: String,
    pub semantic_declaration: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputSelection {
    Zip,
    Tar,
    OciImage,
}

impl OutputSelection {
    pub fn parse(value: &str) -> Result<Self, WorkbenchError> {
        match value {
            "zip" => Ok(Self::Zip),
            "tar" => Ok(Self::Tar),
            "oci" => Ok(Self::OciImage),
            "directory" | "wasm" | "oci-layout" | "other" => Err(WorkbenchError::InvalidSelection(
                format!("{value} is visible as a future boundary but is not selectable"),
            )),
            _ => Err(WorkbenchError::InvalidSelection(format!(
                "unknown output selection: {value}"
            ))),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Zip => "ZIP",
            Self::Tar => "TAR",
            Self::OciImage => "OCI Image",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TargetSelection {
    DownloadLocal,
    LocalRuntime,
    Docker,
}

impl TargetSelection {
    pub fn parse(value: &str) -> Result<Self, WorkbenchError> {
        match value {
            "download" => Ok(Self::DownloadLocal),
            "local-runtime" => Ok(Self::LocalRuntime),
            "docker" => Ok(Self::Docker),
            "remote-host" | "other" => Err(WorkbenchError::InvalidSelection(format!(
                "{value} is visible as a future boundary but is not selectable"
            ))),
            _ => Err(WorkbenchError::InvalidSelection(format!(
                "unknown target selection: {value}"
            ))),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::DownloadLocal => "Download / Local",
            Self::LocalRuntime => "Local Runtime",
            Self::Docker => "Docker",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RepresentationView {
    pub artifact_identity: String,
    pub output: String,
    pub representation_identity: String,
    pub size_bytes: u64,
    pub path: Option<String>,
    pub target_ready: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ExecutionView {
    pub identity: String,
    pub target: String,
    pub status: String,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ReceiptView {
    pub artifact: ArtifactView,
    pub representation: RepresentationView,
    pub target: String,
    pub execution: Option<ExecutionView>,
    pub verdict: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct OperationSummary {
    pub artifact_identity: Option<String>,
    pub output: Option<String>,
    pub target: Option<String>,
    pub representation_identity: Option<String>,
    pub execution_identity: Option<String>,
}

pub struct ArtifactWorkbench {
    pipeline: PipelineSpec,
    store: LocalArtifactStore,
    output_root: PathBuf,
    source: Option<PathBuf>,
    source_artifact: Option<SourceBackedArtifact>,
    recovered_artifact: Option<artifact::RecoveredArtifact>,
    selected_output: Option<OutputSelection>,
    selected_target: Option<TargetSelection>,
    representation: Option<RepresentationView>,
    materialized_path: Option<PathBuf>,
    receipt: Option<ReceiptView>,
}

impl ArtifactWorkbench {
    pub fn new(
        store_root: impl AsRef<Path>,
        output_root: impl AsRef<Path>,
    ) -> Result<Self, WorkbenchError> {
        Ok(Self {
            pipeline: default_directory_zip_pipeline(),
            store: LocalArtifactStore::open(store_root)?,
            output_root: output_root.as_ref().to_path_buf(),
            source: None,
            source_artifact: None,
            recovered_artifact: None,
            selected_output: None,
            selected_target: None,
            representation: None,
            materialized_path: None,
            receipt: None,
        })
    }

    pub fn capabilities() -> CapabilitiesView {
        CapabilitiesView {
            pipelines: vec![WorkbenchCapability {
                id: "default".to_string(),
                label: "Default".to_string(),
                state: CapabilityState::Available,
                detail: "Directory source → select → manifest → validate".to_string(),
            }],
            outputs: vec![
                WorkbenchCapability {
                    id: "zip".to_string(),
                    label: "ZIP".to_string(),
                    state: CapabilityState::Available,
                    detail: "Engine ZipMaterializer".to_string(),
                },
                WorkbenchCapability {
                    id: "tar".to_string(),
                    label: "TAR".to_string(),
                    state: CapabilityState::Available,
                    detail: "Engine TarMaterializer".to_string(),
                },
                WorkbenchCapability {
                    id: "oci".to_string(),
                    label: "OCI Image".to_string(),
                    state: CapabilityState::ExternalConsumer,
                    detail: "Available through external OCI consumer when Docker is present"
                        .to_string(),
                },
                WorkbenchCapability {
                    id: "directory".to_string(),
                    label: "Directory".to_string(),
                    state: CapabilityState::Unavailable,
                    detail: "Coming from future materializer".to_string(),
                },
                WorkbenchCapability {
                    id: "wasm".to_string(),
                    label: "WASM".to_string(),
                    state: CapabilityState::Unavailable,
                    detail: "Coming from future materializer".to_string(),
                },
                WorkbenchCapability {
                    id: "oci-layout".to_string(),
                    label: "OCI layout".to_string(),
                    state: CapabilityState::Unavailable,
                    detail: "Coming from future materializer".to_string(),
                },
            ],
            targets: vec![
                WorkbenchCapability {
                    id: "download".to_string(),
                    label: "Download / Local".to_string(),
                    state: CapabilityState::Available,
                    detail: "Materialized file on the workbench host".to_string(),
                },
                WorkbenchCapability {
                    id: "local-runtime".to_string(),
                    label: "Local Runtime".to_string(),
                    state: CapabilityState::Available,
                    detail: "Existing runtime consumer for ZIP representations".to_string(),
                },
                WorkbenchCapability {
                    id: "docker".to_string(),
                    label: "Docker".to_string(),
                    state: CapabilityState::ExternalConsumer,
                    detail: "External OCI consumer boundary".to_string(),
                },
                WorkbenchCapability {
                    id: "remote-host".to_string(),
                    label: "Remote Host".to_string(),
                    state: CapabilityState::Unavailable,
                    detail: "Unavailable until deployment boundary is proven".to_string(),
                },
                WorkbenchCapability {
                    id: "other".to_string(),
                    label: "Other".to_string(),
                    state: CapabilityState::Unavailable,
                    detail: "Unavailable until a real target boundary exists".to_string(),
                },
            ],
        }
    }

    pub fn import_source(
        &mut self,
        source: impl AsRef<Path>,
    ) -> Result<SourceView, WorkbenchError> {
        let source = source.as_ref();
        let metadata = fs::metadata(source)?;
        if !metadata.is_dir() {
            return Err(WorkbenchError::InvalidSelection(format!(
                "{} is not a directory source",
                source.display()
            )));
        }

        self.source = Some(source.to_path_buf());
        self.source_artifact = None;
        self.recovered_artifact = None;
        self.selected_output = None;
        self.selected_target = None;
        self.representation = None;
        self.materialized_path = None;
        self.receipt = None;

        self.describe_source()
    }

    pub fn build_artifact(
        &mut self,
        options: BuildOptions,
    ) -> Result<ArtifactView, WorkbenchError> {
        let source = self
            .source
            .clone()
            .ok_or(WorkbenchError::MissingState("source"))?;
        if options
            .semantic_declaration
            .as_ref()
            .is_some_and(|value| !value.trim().is_empty())
        {
            return Err(WorkbenchError::InvalidSelection(
                "semantic declarations are visible but unavailable in the default pipeline"
                    .to_string(),
            ));
        }
        let source_artifact = self.pipeline.build_from_directory(&source)?;
        self.store
            .persist(source_artifact.artifact(), &source_artifact)?;
        let view = artifact_view(source_artifact.artifact(), &self.pipeline)?;
        self.source_artifact = Some(source_artifact);
        self.recovered_artifact = None;
        self.representation = None;
        self.materialized_path = None;
        self.receipt = None;
        Ok(view)
    }

    pub fn recover_artifact(&mut self, identity: &str) -> Result<ArtifactView, WorkbenchError> {
        let recovered = self.store.recover(identity)?;
        let view = artifact_view(recovered.artifact(), &self.pipeline)?;
        self.recovered_artifact = Some(recovered);
        self.source_artifact = None;
        self.representation = None;
        self.materialized_path = None;
        self.receipt = None;
        Ok(view)
    }

    pub fn select_output(&mut self, value: &str) -> Result<RepresentationView, WorkbenchError> {
        let selection = OutputSelection::parse(value)?;
        let output_file = self.materialize(selection)?;
        self.selected_output = Some(selection);
        self.materialized_path = output_file.path.clone().map(PathBuf::from);
        self.representation = Some(output_file.clone());
        self.receipt = None;
        Ok(output_file)
    }

    pub fn select_target(&mut self, value: &str) -> Result<OperationSummary, WorkbenchError> {
        let target = TargetSelection::parse(value)?;
        self.selected_target = Some(target);
        Ok(self.summary())
    }

    pub fn execute(
        &mut self,
        executable_relative_path: &str,
    ) -> Result<ReceiptView, WorkbenchError> {
        let target = self
            .selected_target
            .ok_or(WorkbenchError::MissingState("target"))?;
        let output = self
            .selected_output
            .ok_or(WorkbenchError::MissingState("output"))?;
        let representation = self
            .representation
            .clone()
            .ok_or(WorkbenchError::MissingState("representation"))?;
        let artifact = self.current_artifact()?;
        let execution = match (output, target) {
            (OutputSelection::Zip, TargetSelection::LocalRuntime) => {
                let path = self
                    .materialized_path
                    .as_ref()
                    .ok_or(WorkbenchError::MissingState("materialized_path"))?;
                Some(execution_view(RuntimeConsumer.execute_materialized_zip(
                    &artifact.identity,
                    path,
                    executable_relative_path,
                    &[],
                    &[],
                )?))
            }
            (_, TargetSelection::DownloadLocal) => None,
            (OutputSelection::OciImage, TargetSelection::Docker) => {
                return Err(WorkbenchError::InvalidSelection(
                    "Docker execution remains in the external OCI consumer and is unavailable unless that boundary is run directly".to_string(),
                ));
            }
            _ => {
                return Err(WorkbenchError::InvalidSelection(format!(
                    "{} cannot execute on {} in this workbench",
                    output.label(),
                    target.label()
                )));
            }
        };

        let receipt = ReceiptView {
            artifact: artifact_view(artifact, &self.pipeline)?,
            representation,
            target: target.label().to_string(),
            execution,
            verdict: "WORKBENCH PROVEN".to_string(),
        };
        self.receipt = Some(receipt.clone());
        Ok(receipt)
    }

    pub fn reset_operation(&mut self) {
        self.source = None;
        self.source_artifact = None;
        self.recovered_artifact = None;
        self.selected_output = None;
        self.selected_target = None;
        self.representation = None;
        self.materialized_path = None;
        self.receipt = None;
    }

    pub fn summary(&self) -> OperationSummary {
        OperationSummary {
            artifact_identity: self
                .current_artifact()
                .ok()
                .map(|artifact| artifact.identity.clone()),
            output: self
                .selected_output
                .map(|output| output.label().to_string()),
            target: self
                .selected_target
                .map(|target| target.label().to_string()),
            representation_identity: self
                .representation
                .as_ref()
                .map(|representation| representation.representation_identity.clone()),
            execution_identity: self
                .receipt
                .as_ref()
                .and_then(|receipt| receipt.execution.as_ref())
                .map(|execution| execution.identity.clone()),
        }
    }

    fn describe_source(&self) -> Result<SourceView, WorkbenchError> {
        let root = self
            .source
            .as_ref()
            .ok_or(WorkbenchError::MissingState("source"))?;
        let mut entries = Vec::new();
        collect_preview(root, root, &mut entries)?;
        entries.sort();
        let total_entries = entries.len();
        entries.truncate(12);
        Ok(SourceView {
            root: root.display().to_string(),
            source_kind: "directory".to_string(),
            entries,
            total_entries,
        })
    }

    fn materialize(
        &mut self,
        selection: OutputSelection,
    ) -> Result<RepresentationView, WorkbenchError> {
        fs::create_dir_all(&self.output_root)?;
        if let Some(source_artifact) = &self.source_artifact {
            return materialize_with(
                selection,
                source_artifact.artifact(),
                source_artifact,
                &self.output_root,
            );
        }
        if let Some(recovered) = &self.recovered_artifact {
            return materialize_with(
                selection,
                recovered.artifact(),
                recovered,
                &self.output_root,
            );
        }
        Err(WorkbenchError::MissingState("artifact"))
    }

    fn current_artifact(&self) -> Result<&Artifact, WorkbenchError> {
        if let Some(source_artifact) = &self.source_artifact {
            return Ok(source_artifact.artifact());
        }
        if let Some(recovered) = &self.recovered_artifact {
            return Ok(recovered.artifact());
        }
        Err(WorkbenchError::MissingState("artifact"))
    }
}

fn materialize_with<R: ContentResolver>(
    selection: OutputSelection,
    artifact: &Artifact,
    resolver: &R,
    output_root: &Path,
) -> Result<RepresentationView, WorkbenchError> {
    match selection {
            OutputSelection::Zip => {
            let path = output_root.join(file_stem(&artifact.identity, "zip"));
                let result = ZipMaterializer.materialize_to_path(artifact, resolver, &path)?;
                Ok(representation_view(result, selection, path, vec!["download", "local-runtime"]))
            }
            OutputSelection::Tar => {
            let path = output_root.join(file_stem(&artifact.identity, "tar"));
                let result = TarMaterializer.materialize_to_path(artifact, resolver, &path)?;
                Ok(representation_view(result, selection, path, vec!["download"]))
            }
            OutputSelection::OciImage => Err(WorkbenchError::InvalidSelection(
                "OCI image materialization is available through the external OCI consumer; run that boundary where Docker is available".to_string(),
            )),
        }
}

fn artifact_view(
    artifact: &Artifact,
    pipeline: &PipelineSpec,
) -> Result<ArtifactView, WorkbenchError> {
    Ok(ArtifactView {
        identity: artifact.identity.clone(),
        entries: artifact.entries.len(),
        size_bytes: artifact.total_size(),
        pipeline_identity: artifact.pipeline_identity.clone(),
        pipeline: pipeline.identity()?,
        capabilities: artifact
            .capabilities
            .iter()
            .map(|capability| format!("{}@{}", capability.name, capability.version))
            .collect(),
        lineage: if artifact.lineage().is_empty() {
            "None".to_string()
        } else {
            format!("{} transformation(s)", artifact.lineage().len())
        },
        semantic_declaration: artifact.semantic_type().map(ToString::to_string),
    })
}

fn representation_view(
    result: MaterializationResult,
    output: OutputSelection,
    path: PathBuf,
    target_ready: Vec<&str>,
) -> RepresentationView {
    RepresentationView {
        artifact_identity: result.artifact_identity,
        output: output.label().to_string(),
        representation_identity: result.output_digest,
        size_bytes: result.size_bytes,
        path: Some(path.display().to_string()),
        target_ready: target_ready.into_iter().map(ToString::to_string).collect(),
    }
}

fn execution_view(execution: RuntimeExecution) -> ExecutionView {
    ExecutionView {
        identity: execution.runtime_execution_id.to_string(),
        target: "Local Runtime".to_string(),
        status: if execution.exit_code == Some(0) {
            "Completed".to_string()
        } else {
            "Failed".to_string()
        },
        exit_code: execution.exit_code,
        stdout: execution.stdout,
        stderr: execution.stderr,
    }
}

fn collect_preview(
    root: &Path,
    directory: &Path,
    entries: &mut Vec<String>,
) -> Result<(), WorkbenchError> {
    for child in fs::read_dir(directory)? {
        let child = child?;
        let path = child.path();
        let relative = path.strip_prefix(root).map_err(|_| {
            WorkbenchError::InvalidSelection("failed to derive source-relative path".to_string())
        })?;
        let mut display = relative.to_string_lossy().replace('\\', "/");
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.is_dir() {
            display.push('/');
            entries.push(display);
            collect_preview(root, &path, entries)?;
        } else if metadata.is_file() {
            entries.push(display);
        }
    }
    Ok(())
}

fn file_stem(identity: &str, extension: &str) -> String {
    let stem = identity.strip_prefix("sha256:").unwrap_or(identity);
    format!("artifact-{stem}.{extension}")
}
