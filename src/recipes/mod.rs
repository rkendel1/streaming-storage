use crate::core::{ArtifactError, Capability};
use crate::pipeline::{
    MaterializerSpec, PipelineSpec, SelectStageSpec, SourceSpec, StageSpec,
};
use serde::Serialize;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecipeType {
    DirectoryZip,
    DirectoryTar,
    Wasm,
}

impl RecipeType {
    pub fn label(&self) -> &'static str {
        match self {
            Self::DirectoryZip => "directory_zip",
            Self::DirectoryTar => "directory_tar",
            Self::Wasm => "wasm",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RecipeConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude_exact: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude_prefixes: Option<Vec<String>>,
}

impl Default for RecipeConfig {
    fn default() -> Self {
        Self {
            exclude_exact: Some(vec![".env".to_string()]),
            exclude_prefixes: Some(vec!["node_modules".to_string()]),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RecipeSpec {
    pub recipe_type: RecipeType,
    pub source: SourceSpec,
    pub config: RecipeConfig,
}

impl RecipeSpec {
    pub fn directory_zip() -> Self {
        Self {
            recipe_type: RecipeType::DirectoryZip,
            source: SourceSpec::Directory,
            config: RecipeConfig::default(),
        }
    }

    pub fn directory_tar() -> Self {
        Self {
            recipe_type: RecipeType::DirectoryTar,
            source: SourceSpec::Directory,
            config: RecipeConfig::default(),
        }
    }

    pub fn wasm() -> Self {
        Self {
            recipe_type: RecipeType::Wasm,
            source: SourceSpec::Directory,
            config: RecipeConfig::default(),
        }
    }

    pub fn with_exclusions(mut self, exclude_exact: Vec<String>, exclude_prefixes: Vec<String>) -> Self {
        self.config.exclude_exact = if exclude_exact.is_empty() { None } else { Some(exclude_exact) };
        self.config.exclude_prefixes = if exclude_prefixes.is_empty() { None } else { Some(exclude_prefixes) };
        self
    }

    pub fn compile(&self) -> Result<PipelineSpec, ArtifactError> {
        match self.recipe_type {
            RecipeType::DirectoryZip => self.compile_directory_zip(),
            RecipeType::DirectoryTar => self.compile_directory_tar(),
            RecipeType::Wasm => self.compile_wasm(),
        }
    }

    fn compile_directory_zip(&self) -> Result<PipelineSpec, ArtifactError> {
        let exclude_exact = self.config.exclude_exact.clone().unwrap_or_default();
        let exclude_prefixes = self.config.exclude_prefixes.clone().unwrap_or_default();

        let select_stage = SelectStageSpec::new(exclude_exact, exclude_prefixes)?;

        Ok(PipelineSpec {
            source: self.source.clone(),
            stages: vec![
                StageSpec::Select(select_stage),
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
        })
    }

    fn compile_directory_tar(&self) -> Result<PipelineSpec, ArtifactError> {
        let exclude_exact = self.config.exclude_exact.clone().unwrap_or_default();
        let exclude_prefixes = self.config.exclude_prefixes.clone().unwrap_or_default();

        let select_stage = SelectStageSpec::new(exclude_exact, exclude_prefixes)?;

        Ok(PipelineSpec {
            source: self.source.clone(),
            stages: vec![
                StageSpec::Select(select_stage),
                StageSpec::Manifest,
                StageSpec::Validate,
            ],
            materializer: MaterializerSpec::Tar,
            capabilities: vec![
                Capability::new("filesystem.read", "1"),
                Capability::new("manifest.generate", "1"),
                Capability::new("artifact.validate", "1"),
                Capability::new("package.tar", "1"),
            ],
        })
    }

    fn compile_wasm(&self) -> Result<PipelineSpec, ArtifactError> {
        let exclude_exact = self.config.exclude_exact.clone().unwrap_or_default();
        let exclude_prefixes = self.config.exclude_prefixes.clone().unwrap_or_default();

        let select_stage = SelectStageSpec::new(exclude_exact, exclude_prefixes)?;

        Ok(PipelineSpec {
            source: self.source.clone(),
            stages: vec![
                StageSpec::Select(select_stage),
                StageSpec::Manifest,
                StageSpec::Validate,
            ],
            materializer: MaterializerSpec::Zip,
            capabilities: vec![
                Capability::new("filesystem.read", "1"),
                Capability::new("manifest.generate", "1"),
                Capability::new("artifact.validate", "1"),
                Capability::new("compile.wasm", "1"),
                Capability::new("package.zip", "1"),
            ],
        })
    }
}
