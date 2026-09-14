use crate::authorization::{AllowAllPolicy, CapabilityPolicy};
use crate::core::Artifact;
use crate::pipeline::PipelineSpec;
use crate::recipes::{RecipeSpec, RecipeType};
use serde_json::json;
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub struct WasmArtifactSDK;

#[cfg(feature = "wasm")]
#[wasm_bindgen]
impl WasmArtifactSDK {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmArtifactSDK {
        WasmArtifactSDK
    }

    #[wasm_bindgen]
    pub fn recipe_from_spec_json(recipe_json: &str) -> Result<WasmArtifactRecipe, String> {
        let recipe: RecipeSpec = serde_json::from_str(recipe_json)
            .map_err(|e| format!("Failed to parse recipe: {}", e))?;
        Ok(WasmArtifactRecipe { recipe })
    }

    #[wasm_bindgen]
    pub fn pipeline_from_spec_json(pipeline_json: &str) -> Result<WasmArtifactPipeline, String> {
        let pipeline: PipelineSpec = serde_json::from_str(pipeline_json)
            .map_err(|e| format!("Failed to parse pipeline: {}", e))?;
        Ok(WasmArtifactPipeline { pipeline })
    }
}

#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub struct WasmArtifactRecipe {
    recipe: RecipeSpec,
}

#[cfg(feature = "wasm")]
#[wasm_bindgen]
impl WasmArtifactRecipe {
    #[wasm_bindgen]
    pub fn compile(&self) -> Result<WasmArtifactPipeline, String> {
        let pipeline = self
            .recipe
            .compile()
            .map_err(|e| format!("Compilation failed: {}", e))?;
        Ok(WasmArtifactPipeline { pipeline })
    }

    #[wasm_bindgen]
    pub fn spec_json(&self) -> String {
        serde_json::to_string(&self.recipe).unwrap_or_default()
    }
}

#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub struct WasmArtifactPipeline {
    pipeline: PipelineSpec,
}

#[cfg(feature = "wasm")]
#[wasm_bindgen]
impl WasmArtifactPipeline {
    #[wasm_bindgen]
    pub fn inspect(&self) -> Result<String, String> {
        let inspection = self
            .pipeline
            .inspect()
            .map_err(|e| format!("Inspection failed: {}", e))?;

        let result = json!({
            "pipeline_identity": inspection.pipeline_identity,
            "stages": inspection.stages.iter().map(|s| {
                json!({
                    "label": s.label,
                    "identity": s.stage_identity,
                })
            }).collect::<Vec<_>>(),
            "required_capabilities": inspection.required_capabilities.iter().map(|c| {
                json!({
                    "name": c.name,
                    "version": c.version,
                })
            }).collect::<Vec<_>>(),
            "materializer": inspection.materializer,
        });

        Ok(serde_json::to_string(&result)
            .map_err(|e| format!("Failed to serialize inspection: {}", e))?)
    }

    #[wasm_bindgen]
    pub fn required_capabilities(&self) -> String {
        let caps = self.pipeline.required_capabilities();
        let result = caps
            .iter()
            .map(|c| {
                json!({
                    "name": c.name,
                    "version": c.version,
                })
            })
            .collect::<Vec<_>>();
        serde_json::to_string(&result).unwrap_or_default()
    }

    #[wasm_bindgen]
    pub fn build_from_directory(&self, root: &str) -> Result<String, String> {
        let built = self
            .pipeline
            .build_from_directory(root)
            .map_err(|e| format!("Build failed: {}", e))?;

        let artifact = built.artifact();
        let result = json!({
            "identity": artifact.identity,
            "entries_count": artifact.entries.len(),
            "entries": artifact.entries.iter().map(|e| {
                json!({
                    "path": e.path,
                    "entry_type": format!("{:?}", e.entry_type),
                })
            }).collect::<Vec<_>>(),
        });

        Ok(serde_json::to_string(&result)
            .map_err(|e| format!("Failed to serialize artifact: {}", e))?)
    }

    #[wasm_bindgen]
    pub fn build_with_authorization(&self, root: &str) -> Result<String, String> {
        let policy = AllowAllPolicy;
        let (built, evidence) = self
            .pipeline
            .build_with_authorization(root, &policy)
            .map_err(|e| format!("Build with authorization failed: {}", e))?;

        let artifact = built.artifact();
        let auth_decision = &evidence.authorization_decision;

        let result = json!({
            "artifact": {
                "identity": artifact.identity,
                "entries_count": artifact.entries.len(),
                "entries": artifact.entries.iter().map(|e| {
                    json!({
                        "path": e.path,
                        "entry_type": format!("{:?}", e.entry_type),
                    })
                }).collect::<Vec<_>>(),
            },
            "evidence": {
                "is_successful": evidence.authorization_decision.is_allowed() &&
                    matches!(evidence.execution_result, crate::authorization::ExecutionResult::Success),
                "authorization_decision": {
                    "allowed": auth_decision.is_allowed(),
                    "requested_capabilities": evidence.requested_capabilities.iter().map(|c| {
                        json!({"name": c.name, "version": c.version})
                    }).collect::<Vec<_>>(),
                    "granted_capabilities": evidence.granted_capabilities.iter().map(|c| {
                        json!({"name": c.name, "version": c.version})
                    }).collect::<Vec<_>>(),
                    "denied_capabilities": auth_decision.denied_capabilities.iter().map(|c| {
                        json!({"name": c.name, "version": c.version})
                    }).collect::<Vec<_>>(),
                },
                "stage_trace": evidence.stage_trace.iter().map(|s| {
                    json!({
                        "label": s.label,
                        "identity": s.stage_identity,
                    })
                }).collect::<Vec<_>>(),
                "used_capabilities": evidence.used_capabilities.iter().map(|c| {
                    json!({"name": c.name, "version": c.version})
                }).collect::<Vec<_>>(),
            },
        });

        Ok(serde_json::to_string(&result)
            .map_err(|e| format!("Failed to serialize result: {}", e))?)
    }

    #[wasm_bindgen]
    pub fn spec_json(&self) -> String {
        serde_json::to_string(&self.pipeline).unwrap_or_default()
    }
}
