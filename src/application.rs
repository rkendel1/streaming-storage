use crate::core::{Artifact, ArtifactError, Capability};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct ApplicationManifest {
    pub name: String,
    pub entrypoint: String,
    pub declared_capabilities: Vec<Capability>,
}

impl ApplicationManifest {
    pub fn compute_from_wasm_artifact(
        artifact: &Artifact,
        declared_capabilities: Vec<Capability>,
    ) -> Result<ApplicationManifest, ArtifactError> {
        if !artifact.entries.iter().any(|e| e.path == "application.wasm") {
            return Err(ArtifactError::InvalidState(
                "Not a WASM application artifact: no application.wasm entry".to_string(),
            ));
        }

        let name = artifact
            .entries
            .iter()
            .find(|e| e.path.ends_with(".rs") || e.path == "main.rs")
            .map(|_| "application".to_string())
            .unwrap_or_else(|| "application".to_string());

        Ok(ApplicationManifest {
            name,
            entrypoint: "application.wasm".to_string(),
            declared_capabilities,
        })
    }

    pub fn to_json(&self) -> Result<Vec<u8>, ArtifactError> {
        serde_json::to_vec_pretty(self)
            .map_err(|e| ArtifactError::Serialization(format!("Manifest serialization failed: {}", e)))
    }
}

pub struct ApplicationArtifact {
    artifact: Artifact,
    manifest: ApplicationManifest,
}

impl ApplicationArtifact {
    pub fn from_wasm_artifact(artifact: Artifact, declared_capabilities: Vec<Capability>) -> Result<ApplicationArtifact, ArtifactError> {
        let manifest = ApplicationManifest::compute_from_wasm_artifact(&artifact, declared_capabilities)?;
        Ok(ApplicationArtifact { artifact, manifest })
    }

    pub fn artifact(&self) -> &Artifact {
        &self.artifact
    }

    pub fn manifest(&self) -> &ApplicationManifest {
        &self.manifest
    }

    pub fn identity(&self) -> &str {
        &self.artifact.identity
    }

    pub fn has_executable(&self) -> bool {
        self.artifact
            .entries
            .iter()
            .any(|e| e.path == self.manifest.entrypoint)
    }

    pub fn declared_capabilities(&self) -> Vec<Capability> {
        self.manifest.declared_capabilities.clone()
    }
}
