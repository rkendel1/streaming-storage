pub mod server;

use artifact::{
    AppBundleMaterializer, Artifact, ArtifactError, ContentResolver, DirectoryMaterializer,
    GitTreeMaterializer, LocalArtifactStore, MaterializationResult, PipelineSpec,
    RawFileMaterializer, SourceBackedArtifact, TarCompression, TarMaterializer,
    WasmMaterializer, WasmRepresentationKind, ZipMaterializer,
    default_directory_zip_pipeline,
};
use artifact_runtime_consumer::{RuntimeConsumer, RuntimeExecution};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use zip::ZipArchive;

const MAX_REMOTE_BYTES: u64 = 50 * 1024 * 1024;
const MAX_EXTRACTED_BYTES: u64 = 100 * 1024 * 1024;
const MAX_SOURCE_ENTRIES: usize = 10_000;
const PREVIEW_ENTRY_LIMIT: usize = 512;

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
    pub category: String,
    pub state: CapabilityState,
    pub description: String,
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
    pub display_name: String,
    pub source_kind: String,
    pub detail: String,
    pub entries: Vec<String>,
    pub total_entries: usize,
    pub total_size_bytes: u64,
    pub branch: Option<String>,
    pub detected: Option<String>,
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
    pub output_options: Vec<WorkbenchCapability>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputSelection {
    Zip,
    Tar,
    TarGzip,
    TarZstd,
    Directory,
    RawFile,
    GitTree,
    AppBundle,
    WasmModule,
    WasmComponent,
    OciImage,
    OciLayout,
    Iso,
}

impl OutputSelection {
    pub fn parse(value: &str) -> Result<Self, WorkbenchError> {
        match value {
            "zip" => Ok(Self::Zip),
            "tar" => Ok(Self::Tar),
            "tar-gzip" => Ok(Self::TarGzip),
            "tar-zstd" => Ok(Self::TarZstd),
            "directory" => Ok(Self::Directory),
            "raw-file" => Ok(Self::RawFile),
            "git-tree" => Ok(Self::GitTree),
            "app-bundle" => Ok(Self::AppBundle),
            "wasm-module" => Ok(Self::WasmModule),
            "wasm-component" => Ok(Self::WasmComponent),
            "oci" => Ok(Self::OciImage),
            "oci-layout" => Ok(Self::OciLayout),
            "iso" => Ok(Self::Iso),
            _ => Err(WorkbenchError::InvalidSelection(format!(
                "unknown output selection: {value}"
            ))),
        }
    }

    fn id(self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::Tar => "tar",
            Self::TarGzip => "tar-gzip",
            Self::TarZstd => "tar-zstd",
            Self::Directory => "directory",
            Self::RawFile => "raw-file",
            Self::GitTree => "git-tree",
            Self::AppBundle => "app-bundle",
            Self::WasmModule => "wasm-module",
            Self::WasmComponent => "wasm-component",
            Self::OciImage => "oci",
            Self::OciLayout => "oci-layout",
            Self::Iso => "iso",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Zip => "ZIP",
            Self::Tar => "TAR",
            Self::TarGzip => "TAR + gzip",
            Self::TarZstd => "TAR + zstd",
            Self::Directory => "Directory",
            Self::RawFile => "Raw File",
            Self::GitTree => "Git Tree",
            Self::AppBundle => "App Bundle",
            Self::WasmModule => "WASM Module",
            Self::WasmComponent => "WASM Component",
            Self::OciImage => "OCI Image",
            Self::OciLayout => "OCI Layout",
            Self::Iso => "ISO",
        }
    }

    fn category(self) -> &'static str {
        match self {
            Self::Zip
            | Self::Tar
            | Self::TarGzip
            | Self::TarZstd
            | Self::Directory
            | Self::RawFile => "Portable",
            Self::OciImage
            | Self::OciLayout
            | Self::WasmModule
            | Self::WasmComponent => "Runtime",
            Self::GitTree => "Development",
            Self::AppBundle => "Application",
            Self::Iso => "Distribution",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::Zip => "Portable ZIP archive",
            Self::Tar => "Portable TAR archive",
            Self::TarGzip => "TAR archive compressed with gzip",
            Self::TarZstd => "TAR archive compressed with zstd",
            Self::Directory => "Materialize as a filesystem directory",
            Self::RawFile => "Single-file materialization",
            Self::GitTree => "Materialize as a Git-compatible working tree",
            Self::AppBundle => "Self-contained application bundle",
            Self::WasmModule => "WebAssembly core module representation",
            Self::WasmComponent => "WebAssembly Component Model representation",
            Self::OciImage => "Container image representation",
            Self::OciLayout => "OCI image layout directory",
            Self::Iso => "Portable ISO filesystem image",
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
            "docker" | "remote-host" | "other" => Err(WorkbenchError::InvalidSelection(format!(
                "{value} is visible as an unintegrated boundary but is not selectable"
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
    pub output_id: String,
    pub output: String,
    pub representation_identity_label: String,
    pub representation_identity: String,
    pub size_bytes: u64,
    pub path: Option<String>,
    pub target_ready: Vec<String>,
    pub details: Vec<RepresentationDetail>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RepresentationDetail {
    pub label: String,
    pub value: String,
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
    source_view: Option<SourceView>,
    source_artifact: Option<SourceBackedArtifact>,
    recovered_artifact: Option<artifact::RecoveredArtifact>,
    selected_output: Option<OutputSelection>,
    selected_target: Option<TargetSelection>,
    representation: Option<RepresentationView>,
    materialized_path: Option<PathBuf>,
    receipt: Option<ReceiptView>,
}

struct AcquiredSource {
    root: PathBuf,
    view: SourceView,
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
            source_view: None,
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
                category: "Pipeline".to_string(),
                state: CapabilityState::Available,
                description: "Directory source → select → manifest → validate".to_string(),
                detail: "Directory source → select → manifest → validate".to_string(),
            }],
            outputs: catalog_output_options(None),
            targets: vec![
                WorkbenchCapability {
                    id: "download".to_string(),
                    label: "Download / Local".to_string(),
                    category: "Target".to_string(),
                    state: CapabilityState::Available,
                    description: "Materialized output on the workbench host".to_string(),
                    detail: "Materialized file on the workbench host".to_string(),
                },
                WorkbenchCapability {
                    id: "local-runtime".to_string(),
                    label: "Local Runtime".to_string(),
                    category: "Target".to_string(),
                    state: CapabilityState::Available,
                    description: "Execute ZIP representations with the runtime consumer"
                        .to_string(),
                    detail: "Existing runtime consumer for ZIP representations".to_string(),
                },
                WorkbenchCapability {
                    id: "docker".to_string(),
                    label: "Docker".to_string(),
                    category: "Target".to_string(),
                    state: CapabilityState::ExternalConsumer,
                    description: "External OCI consumer boundary".to_string(),
                    detail: "External OCI consumer boundary".to_string(),
                },
                WorkbenchCapability {
                    id: "remote-host".to_string(),
                    label: "Remote Host".to_string(),
                    category: "Target".to_string(),
                    state: CapabilityState::Unavailable,
                    description: "External deployment boundary".to_string(),
                    detail: "Unavailable until deployment boundary is proven".to_string(),
                },
                WorkbenchCapability {
                    id: "other".to_string(),
                    label: "Other".to_string(),
                    category: "Target".to_string(),
                    state: CapabilityState::Unavailable,
                    description: "Future consumer boundary".to_string(),
                    detail: "Unavailable until a real target boundary exists".to_string(),
                },
            ],
        }
    }

    pub fn import_source(
        &mut self,
        source: impl AsRef<Path>,
    ) -> Result<SourceView, WorkbenchError> {
        let input = source.as_ref().to_string_lossy().trim().to_string();
        let acquired = acquire_source(&input, &self.output_root)?;

        self.source = Some(acquired.root.clone());
        self.source_view = Some(acquired.view.clone());
        self.source_artifact = None;
        self.recovered_artifact = None;
        self.selected_output = None;
        self.selected_target = None;
        self.representation = None;
        self.materialized_path = None;
        self.receipt = None;

        Ok(acquired.view)
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
        let view = artifact_view(source_artifact.artifact(), &source_artifact, &self.pipeline)?;
        self.source_artifact = Some(source_artifact);
        self.recovered_artifact = None;
        self.representation = None;
        self.materialized_path = None;
        self.receipt = None;
        Ok(view)
    }

    pub fn recover_artifact(&mut self, identity: &str) -> Result<ArtifactView, WorkbenchError> {
        let recovered = self.store.recover(identity)?;
        let view = artifact_view(recovered.artifact(), &recovered, &self.pipeline)?;
        self.recovered_artifact = Some(recovered);
        self.source_artifact = None;
        self.representation = None;
        self.materialized_path = None;
        self.receipt = None;
        Ok(view)
    }

    pub fn select_output(&mut self, value: &str) -> Result<RepresentationView, WorkbenchError> {
        let selection = OutputSelection::parse(value)?;
        let capability = self.output_capability(selection)?;
        if capability.state != CapabilityState::Available {
            return Err(WorkbenchError::InvalidSelection(capability.detail));
        }
        let output_file = self.materialize(selection)?;
        self.selected_output = Some(selection);
        self.materialized_path = output_file.path.clone().map(PathBuf::from);
        self.representation = Some(output_file.clone());
        self.receipt = None;
        Ok(output_file)
    }

    pub fn select_target(&mut self, value: &str) -> Result<OperationSummary, WorkbenchError> {
        let target = TargetSelection::parse(value)?;
        let representation = self
            .representation
            .as_ref()
            .ok_or(WorkbenchError::MissingState("representation"))?;
        if !representation.target_ready.iter().any(|ready| ready == value) {
            return Err(WorkbenchError::InvalidSelection(format!(
                "{} is unavailable for {}",
                target.label(),
                representation.output
            )));
        }
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
            artifact: self.current_artifact_view()?,
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
        self.source_view = None;
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

    fn current_artifact_view(&self) -> Result<ArtifactView, WorkbenchError> {
        if let Some(source_artifact) = &self.source_artifact {
            return artifact_view(source_artifact.artifact(), source_artifact, &self.pipeline);
        }
        if let Some(recovered) = &self.recovered_artifact {
            return artifact_view(recovered.artifact(), recovered, &self.pipeline);
        }
        Err(WorkbenchError::MissingState("artifact"))
    }

    fn output_capability(&self, selection: OutputSelection) -> Result<WorkbenchCapability, WorkbenchError> {
        if let Some(source_artifact) = &self.source_artifact {
            return output_capability(selection, source_artifact.artifact(), source_artifact);
        }
        if let Some(recovered) = &self.recovered_artifact {
            return output_capability(selection, recovered.artifact(), recovered);
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
        OutputSelection::TarGzip => {
            let path = output_root.join(file_name(&artifact.identity, "tar.gz"));
            let result = TarMaterializer.materialize_to_path_with_compression(
                artifact,
                resolver,
                &path,
                TarCompression::Gzip,
            )?;
            Ok(representation_view(result, selection, path, vec!["download"]))
        }
        OutputSelection::TarZstd => {
            let path = output_root.join(file_name(&artifact.identity, "tar.zst"));
            let result = TarMaterializer.materialize_to_path_with_compression(
                artifact,
                resolver,
                &path,
                TarCompression::Zstd,
            )?;
            Ok(representation_view(result, selection, path, vec!["download"]))
        }
        OutputSelection::Directory => {
            let path = output_root.join(directory_name(&artifact.identity, "directory"));
            let result = DirectoryMaterializer.materialize_to_path(artifact, resolver, &path)?;
            Ok(representation_view(result, selection, path, vec!["download"]))
        }
        OutputSelection::RawFile => {
            let entry = artifact
                .entries
                .first()
                .ok_or_else(|| WorkbenchError::InvalidSelection("artifact is empty".to_string()))?;
            let path = output_root.join(raw_output_name(&artifact.identity, &entry.path));
            let result = RawFileMaterializer.materialize_to_path(artifact, resolver, &path)?;
            Ok(representation_view(result, selection, path, vec!["download"]))
        }
        OutputSelection::GitTree => {
            let path = output_root.join(directory_name(&artifact.identity, "git-tree"));
            let result = GitTreeMaterializer.materialize_to_path(artifact, resolver, &path)?;
            Ok(representation_view(result, selection, path, vec!["download"]))
        }
        OutputSelection::AppBundle => {
            let path = output_root.join(directory_name(&artifact.identity, "app-bundle"));
            let result = AppBundleMaterializer.materialize_to_path(artifact, resolver, &path)?;
            Ok(representation_view(result, selection, path, vec!["download"]))
        }
        OutputSelection::WasmModule => {
            let path = output_root.join(file_stem(&artifact.identity, "wasm"));
            let result = WasmMaterializer.materialize_to_path(
                artifact,
                resolver,
                &path,
                WasmRepresentationKind::Module,
            )?;
            Ok(representation_view(result, selection, path, vec!["download"]))
        }
        OutputSelection::WasmComponent => {
            let path = output_root.join(file_name(&artifact.identity, "component.wasm"));
            let result = WasmMaterializer.materialize_to_path(
                artifact,
                resolver,
                &path,
                WasmRepresentationKind::Component,
            )?;
            Ok(representation_view(result, selection, path, vec!["download"]))
        }
        OutputSelection::OciImage => Err(WorkbenchError::InvalidSelection(
            "OCI image materialization remains an external consumer concern".to_string(),
        )),
        OutputSelection::OciLayout => Err(WorkbenchError::InvalidSelection(
            "OCI layout is not proven in this workbench yet".to_string(),
        )),
        OutputSelection::Iso => Err(WorkbenchError::InvalidSelection(
            "ISO materialization is unavailable because required tooling is not present".to_string(),
        )),
    }
}

fn artifact_view<R: ContentResolver>(
    artifact: &Artifact,
    resolver: &R,
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
        output_options: artifact_output_options(artifact, resolver)?,
    })
}

fn representation_view(
    result: MaterializationResult,
    output: OutputSelection,
    path: PathBuf,
    target_ready: Vec<&str>,
) -> RepresentationView {
    let mut details = vec![RepresentationDetail {
        label: "Format".to_string(),
        value: result.materializer_format.clone(),
    }];
    match output {
        OutputSelection::TarGzip => details.push(RepresentationDetail {
            label: "Compression".to_string(),
            value: "gzip".to_string(),
        }),
        OutputSelection::TarZstd => details.push(RepresentationDetail {
            label: "Compression".to_string(),
            value: "zstd".to_string(),
        }),
        _ => {}
    }
    RepresentationView {
        artifact_identity: result.artifact_identity,
        output_id: output.id().to_string(),
        output: output.label().to_string(),
        representation_identity_label: representation_identity_label(output).to_string(),
        representation_identity: result.output_digest,
        size_bytes: result.size_bytes,
        path: Some(path.display().to_string()),
        target_ready: target_ready.into_iter().map(ToString::to_string).collect(),
        details,
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

fn catalog_output_options(artifact: Option<(&Artifact, &dyn ContentResolver)>) -> Vec<WorkbenchCapability> {
    output_selections()
        .into_iter()
        .map(|selection| match artifact {
            Some((artifact, resolver)) => {
                output_capability(selection, artifact, resolver).unwrap_or_else(|_| {
                    unavailable_output(selection, "artifact capability evaluation failed")
                })
            }
            None => catalog_output(selection),
        })
        .collect()
}

fn artifact_output_options<R: ContentResolver + ?Sized>(
    artifact: &Artifact,
    resolver: &R,
) -> Result<Vec<WorkbenchCapability>, WorkbenchError> {
    output_selections()
        .into_iter()
        .map(|selection| output_capability(selection, artifact, resolver))
        .collect()
}

fn output_capability<R: ContentResolver + ?Sized>(
    selection: OutputSelection,
    artifact: &Artifact,
    resolver: &R,
) -> Result<WorkbenchCapability, WorkbenchError> {
    let mut capability = catalog_output(selection);
    capability.state = CapabilityState::Available;
    capability.detail.clear();

    match selection {
        OutputSelection::Zip | OutputSelection::Tar | OutputSelection::Directory | OutputSelection::GitTree => {}
        OutputSelection::TarGzip => {
            if !TarCompression::Gzip.is_supported() {
                capability.state = CapabilityState::Unavailable;
                capability.detail = "gzip tooling is unavailable in this environment".to_string();
            }
        }
        OutputSelection::TarZstd => {
            if !TarCompression::Zstd.is_supported() {
                capability.state = CapabilityState::Unavailable;
                capability.detail = "zstd tooling is unavailable in this environment".to_string();
            }
        }
        OutputSelection::RawFile => {
            if artifact.entries.len() != 1 {
                capability.state = CapabilityState::Unavailable;
                capability.detail =
                    "Raw File requires an artifact containing exactly one file".to_string();
            } else if let Some(entry) = artifact.entries.first() {
                capability.detail = format!("Direct materialization of {}", entry.path);
            }
        }
        OutputSelection::AppBundle => {
            if let Some(entrypoint) = app_bundle_entrypoint(artifact) {
                capability.detail = format!("Entrypoint {entrypoint}");
            } else {
                capability.state = CapabilityState::Unavailable;
                capability.detail =
                    "Requires an entrypoint such as application.wasm, bin/app, or a single file"
                        .to_string();
            }
        }
        OutputSelection::WasmModule => match WasmMaterializer.detect_kind(artifact, resolver) {
            Ok(WasmRepresentationKind::Module) => {
                capability.detail = "Valid core WebAssembly module".to_string();
            }
            Ok(WasmRepresentationKind::Component) => {
                capability.state = CapabilityState::Unavailable;
                capability.detail = "Artifact is a WASM component, not a core module".to_string();
            }
            Err(error) => {
                capability.state = CapabilityState::Unavailable;
                capability.detail = error.to_string();
            }
        },
        OutputSelection::WasmComponent => match WasmMaterializer.detect_kind(artifact, resolver) {
            Ok(WasmRepresentationKind::Component) => {
                capability.detail = "Valid WebAssembly Component Model binary".to_string();
            }
            Ok(WasmRepresentationKind::Module) => {
                capability.state = CapabilityState::Unavailable;
                capability.detail = "Artifact is a WASM module, not a component".to_string();
            }
            Err(error) => {
                capability.state = CapabilityState::Unavailable;
                capability.detail = error.to_string();
            }
        },
        OutputSelection::OciImage => {
            capability.state = CapabilityState::ExternalConsumer;
            capability.detail = "Materialize through the external OCI consumer when Docker is present"
                .to_string();
        }
        OutputSelection::OciLayout => {
            capability.state = CapabilityState::Unavailable;
            capability.detail = "OCI layout is not proven in this workbench yet".to_string();
        }
        OutputSelection::Iso => {
            capability.state = CapabilityState::Unavailable;
            capability.detail = "ISO materialization requires dedicated tooling that is not present"
                .to_string();
        }
    }

    if capability.detail.is_empty() {
        capability.detail = capability.description.clone();
    }
    Ok(capability)
}

fn catalog_output(selection: OutputSelection) -> WorkbenchCapability {
    let (state, detail) = match selection {
        OutputSelection::TarGzip if !TarCompression::Gzip.is_supported() => (
            CapabilityState::Unavailable,
            "gzip tooling is unavailable in this environment".to_string(),
        ),
        OutputSelection::TarZstd if !TarCompression::Zstd.is_supported() => (
            CapabilityState::Unavailable,
            "zstd tooling is unavailable in this environment".to_string(),
        ),
        OutputSelection::OciImage => (
            CapabilityState::ExternalConsumer,
            "Materialize through the external OCI consumer when Docker is present".to_string(),
        ),
        OutputSelection::OciLayout => (
            CapabilityState::Unavailable,
            "OCI layout is not proven in this workbench yet".to_string(),
        ),
        OutputSelection::Iso => (
            CapabilityState::Unavailable,
            "ISO materialization requires dedicated tooling that is not present".to_string(),
        ),
        OutputSelection::RawFile => (
            CapabilityState::Unavailable,
            "Requires exactly one file".to_string(),
        ),
        OutputSelection::WasmModule => (
            CapabilityState::Unavailable,
            "Requires a valid core WASM module artifact".to_string(),
        ),
        OutputSelection::WasmComponent => (
            CapabilityState::Unavailable,
            "Requires a valid WASM component artifact".to_string(),
        ),
        OutputSelection::AppBundle => (
            CapabilityState::Unavailable,
            "Requires an entrypoint such as application.wasm, bin/app, or a single file".to_string(),
        ),
        _ => (CapabilityState::Available, selection.description().to_string()),
    };
    WorkbenchCapability {
        id: selection.id().to_string(),
        label: selection.label().to_string(),
        category: selection.category().to_string(),
        state,
        description: selection.description().to_string(),
        detail,
    }
}

fn unavailable_output(selection: OutputSelection, detail: &str) -> WorkbenchCapability {
    WorkbenchCapability {
        id: selection.id().to_string(),
        label: selection.label().to_string(),
        category: selection.category().to_string(),
        state: CapabilityState::Unavailable,
        description: selection.description().to_string(),
        detail: detail.to_string(),
    }
}

fn output_selections() -> Vec<OutputSelection> {
    vec![
        OutputSelection::Zip,
        OutputSelection::Tar,
        OutputSelection::TarGzip,
        OutputSelection::TarZstd,
        OutputSelection::Directory,
        OutputSelection::RawFile,
        OutputSelection::OciImage,
        OutputSelection::OciLayout,
        OutputSelection::WasmModule,
        OutputSelection::WasmComponent,
        OutputSelection::GitTree,
        OutputSelection::AppBundle,
        OutputSelection::Iso,
    ]
}

fn representation_identity_label(selection: OutputSelection) -> &'static str {
    match selection {
        OutputSelection::GitTree => "Git Tree Identity",
        OutputSelection::RawFile => "Output Digest",
        OutputSelection::WasmModule => "Module Digest",
        OutputSelection::WasmComponent => "Component Digest",
        _ => "Representation Identity",
    }
}

fn acquire_source(input: &str, staging_root: &Path) -> Result<AcquiredSource, WorkbenchError> {
    if input.starts_with("https://github.com/") {
        return if is_github_repository_url(input) {
            acquire_github_source(input, staging_root)
        } else if looks_downloadable(input) {
            acquire_direct_url(input, staging_root)
        } else {
            Err(WorkbenchError::InvalidSelection(
                "GitHub source must be a repository URL such as https://github.com/owner/repo"
                    .to_string(),
            ))
        };
    }
    if input.starts_with("http://") {
        return Err(WorkbenchError::InvalidSelection(
            "remote source URLs must use HTTPS".to_string(),
        ));
    }
    if input.starts_with("https://") {
        return acquire_direct_url(input, staging_root);
    }
    if input.contains("://") {
        return Err(WorkbenchError::InvalidSelection(format!(
            "unsupported source URL: {input}"
        )));
    }
    acquire_local_source(Path::new(input), staging_root)
}

fn acquire_local_source(
    path: &Path,
    staging_root: &Path,
) -> Result<AcquiredSource, WorkbenchError> {
    let metadata = fs::metadata(path)?;
    let root = if metadata.is_dir() {
        path.to_path_buf()
    } else if metadata.is_file() {
        let staging = staging_root.join(format!("local-file-{}", unique_suffix()));
        fs::create_dir_all(&staging)?;
        let file_name = path.file_name().ok_or_else(|| {
            WorkbenchError::InvalidSelection("local file must have a file name".to_string())
        })?;
        fs::copy(path, staging.join(file_name))?;
        staging
    } else {
        return Err(WorkbenchError::InvalidSelection(format!(
            "{} is not a file or directory source",
            path.display()
        )));
    };
    let display_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Local Source");
    let mut view = source_view(&root, "Local", display_name, None, None)?;
    if metadata.is_file() {
        view.display_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("local-file")
            .to_string();
        view.detail = "Local file staged from its containing folder".to_string();
    }
    Ok(AcquiredSource { root, view })
}

fn acquire_github_source(
    input: &str,
    staging_root: &Path,
) -> Result<AcquiredSource, WorkbenchError> {
    let (owner, repo) = parse_github_repository(input)?;
    let branches = ["main", "master"];
    let mut last_error = None;
    for branch in branches {
        let url = format!("https://codeload.github.com/{owner}/{repo}/zip/refs/heads/{branch}");
        match download_and_extract_zip(&url, staging_root) {
            Ok(root) => {
                let source_root = single_child_directory(&root).unwrap_or(root);
                let mut view = source_view(
                    &source_root,
                    "GitHub",
                    &format!("{owner}/{repo}"),
                    Some(branch.to_string()),
                    Some("ZIP archive".to_string()),
                )?;
                view.detail = format!("GitHub repository {owner}/{repo}");
                return Ok(AcquiredSource {
                    root: source_root,
                    view,
                });
            }
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.unwrap_or_else(|| {
        WorkbenchError::InvalidSelection("GitHub repository could not be acquired".to_string())
    }))
}

fn acquire_direct_url(input: &str, staging_root: &Path) -> Result<AcquiredSource, WorkbenchError> {
    if !looks_downloadable(input) {
        return Err(WorkbenchError::InvalidSelection(
            "unsupported URL: provide a direct downloadable .zip archive".to_string(),
        ));
    }
    let root = download_and_extract_zip(input, staging_root)?;
    let mut view = source_view(
        &root,
        "Direct URL",
        input,
        None,
        Some("ZIP archive".to_string()),
    )?;
    view.detail = "Direct downloadable source archive".to_string();
    Ok(AcquiredSource { root, view })
}

fn is_github_repository_url(input: &str) -> bool {
    parse_github_repository(input).is_ok()
}

fn parse_github_repository(input: &str) -> Result<(String, String), WorkbenchError> {
    let without_scheme = input.strip_prefix("https://github.com/").ok_or_else(|| {
        WorkbenchError::InvalidSelection("GitHub URLs must use HTTPS".to_string())
    })?;
    let mut parts = without_scheme.trim_end_matches('/').split('/');
    let owner = parts.next().unwrap_or_default();
    let repo = parts.next().unwrap_or_default().trim_end_matches(".git");
    if owner.is_empty() || repo.is_empty() || parts.next().is_some() {
        return Err(WorkbenchError::InvalidSelection(
            "GitHub source must be a repository URL such as https://github.com/owner/repo"
                .to_string(),
        ));
    }
    if !safe_github_segment(owner) || !safe_github_segment(repo) {
        return Err(WorkbenchError::InvalidSelection(
            "GitHub owner and repository names contain unsupported characters".to_string(),
        ));
    }
    Ok((owner.to_string(), repo.to_string()))
}

fn safe_github_segment(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn looks_downloadable(input: &str) -> bool {
    input
        .split('?')
        .next()
        .is_some_and(|path| path.to_ascii_lowercase().ends_with(".zip"))
}

fn download_and_extract_zip(url: &str, staging_root: &Path) -> Result<PathBuf, WorkbenchError> {
    fs::create_dir_all(staging_root)?;
    let destination = staging_root.join(format!("source-{}", unique_suffix()));
    fs::create_dir_all(&destination)?;
    let archive_path = destination.with_extension("zip");
    let status = Command::new("curl")
        .arg("--fail")
        .arg("--location")
        .arg("--silent")
        .arg("--show-error")
        .arg("--max-time")
        .arg("30")
        .arg("--max-filesize")
        .arg(MAX_REMOTE_BYTES.to_string())
        .arg("--output")
        .arg(&archive_path)
        .arg(url)
        .status()?;
    if !status.success() {
        return Err(WorkbenchError::InvalidSelection(format!(
            "failed to download source archive from {url}"
        )));
    }
    let size = fs::metadata(&archive_path)?.len();
    if size == 0 || size > MAX_REMOTE_BYTES {
        return Err(WorkbenchError::InvalidSelection(
            "downloaded source archive is empty or too large".to_string(),
        ));
    }
    extract_zip_safely(&archive_path, &destination)?;
    let _ = fs::remove_file(&archive_path);
    Ok(destination)
}

fn extract_zip_safely(archive_path: &Path, destination: &Path) -> Result<(), WorkbenchError> {
    let file = fs::File::open(archive_path)?;
    let mut archive = ZipArchive::new(file).map_err(|error| {
        WorkbenchError::InvalidSelection(format!("invalid ZIP archive: {error}"))
    })?;
    if archive.len() > MAX_SOURCE_ENTRIES {
        return Err(WorkbenchError::InvalidSelection(
            "source archive contains too many entries".to_string(),
        ));
    }
    let mut total = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| {
            WorkbenchError::InvalidSelection(format!("invalid ZIP entry: {error}"))
        })?;
        let Some(enclosed) = entry.enclosed_name() else {
            return Err(WorkbenchError::InvalidSelection(
                "source archive contains a path traversal entry".to_string(),
            ));
        };
        if entry
            .unix_mode()
            .is_some_and(|mode| (mode & 0o170000) == 0o120000)
        {
            return Err(WorkbenchError::InvalidSelection(
                "source archive contains unsupported symlinks".to_string(),
            ));
        }
        total = total.saturating_add(entry.size());
        if total > MAX_EXTRACTED_BYTES {
            return Err(WorkbenchError::InvalidSelection(
                "source archive expands beyond the workbench limit".to_string(),
            ));
        }
        let outpath = destination.join(enclosed);
        if !outpath.starts_with(destination) {
            return Err(WorkbenchError::InvalidSelection(
                "source archive escapes the extraction directory".to_string(),
            ));
        }
        if entry.is_dir() {
            fs::create_dir_all(&outpath)?;
        } else {
            if let Some(parent) = outpath.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut outfile = fs::File::create(&outpath)?;
            io::copy(&mut entry, &mut outfile)?;
        }
    }
    Ok(())
}

fn single_child_directory(root: &Path) -> Option<PathBuf> {
    let mut children = fs::read_dir(root).ok()?.filter_map(Result::ok);
    let child = children.next()?.path();
    if children.next().is_none() && child.is_dir() {
        Some(child)
    } else {
        None
    }
}

fn source_view(
    root: &Path,
    kind: &str,
    display_name: &str,
    branch: Option<String>,
    detected: Option<String>,
) -> Result<SourceView, WorkbenchError> {
    let mut entries = Vec::new();
    let mut total_entries = 0;
    let mut total_size_bytes = 0;
    collect_preview(
        root,
        root,
        &mut entries,
        &mut total_entries,
        &mut total_size_bytes,
    )?;
    entries.sort();
    entries.truncate(PREVIEW_ENTRY_LIMIT);
    Ok(SourceView {
        root: root.display().to_string(),
        display_name: display_name.to_string(),
        source_kind: kind.to_string(),
        detail: kind.to_string(),
        entries,
        total_entries,
        total_size_bytes,
        branch,
        detected,
    })
}

fn unique_suffix() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .to_string()
}

fn collect_preview(
    root: &Path,
    directory: &Path,
    entries: &mut Vec<String>,
    total_entries: &mut usize,
    total_size_bytes: &mut u64,
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
            *total_entries += 1;
            entries.push(display);
            collect_preview(root, &path, entries, total_entries, total_size_bytes)?;
        } else if metadata.is_file() {
            *total_entries += 1;
            *total_size_bytes += metadata.len();
            entries.push(display);
        }
    }
    Ok(())
}

fn file_stem(identity: &str, extension: &str) -> String {
    file_name(identity, extension)
}

fn file_name(identity: &str, extension: &str) -> String {
    let stem = identity.strip_prefix("sha256:").unwrap_or(identity);
    format!("artifact-{stem}.{extension}")
}

fn directory_name(identity: &str, suffix: &str) -> String {
    let stem = identity.strip_prefix("sha256:").unwrap_or(identity);
    format!("artifact-{stem}-{suffix}")
}

fn raw_output_name(identity: &str, source_path: &str) -> String {
    let name = Path::new(source_path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("artifact.bin");
    let stem = identity.strip_prefix("sha256:").unwrap_or(identity);
    format!("artifact-{stem}-{name}")
}

fn app_bundle_entrypoint(artifact: &Artifact) -> Option<String> {
    if artifact.entries.len() == 1 {
        return artifact.entries.first().map(|entry| entry.path.clone());
    }
    ["application.wasm", "bin/app"]
        .into_iter()
        .find(|candidate| artifact.entries.iter().any(|entry| entry.path == *candidate))
        .map(ToString::to_string)
}
