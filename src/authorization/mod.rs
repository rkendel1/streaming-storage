use crate::core::{Artifact, ArtifactError, Capability};
use serde::Serialize;
use sha2::Digest;
use std::collections::BTreeSet;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AuthorizationDecision {
    pub pipeline_identity: String,
    pub requested_capabilities: Vec<Capability>,
    pub granted_capabilities: Vec<Capability>,
    pub denied_capabilities: Vec<Capability>,
    pub decision: AuthorizationResult,
    pub policy_identity: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorizationResult {
    Allowed,
    Denied,
}

impl AuthorizationDecision {
    pub fn allowed(
        pipeline_identity: String,
        requested_capabilities: Vec<Capability>,
        policy_identity: String,
    ) -> Self {
        Self {
            pipeline_identity,
            granted_capabilities: requested_capabilities.clone(),
            denied_capabilities: vec![],
            requested_capabilities,
            decision: AuthorizationResult::Allowed,
            policy_identity,
        }
    }

    pub fn denied(
        pipeline_identity: String,
        requested_capabilities: Vec<Capability>,
        granted_capabilities: Vec<Capability>,
        policy_identity: String,
    ) -> Self {
        let granted_set: BTreeSet<_> = granted_capabilities
            .iter()
            .map(|c| (c.name.clone(), c.version.clone()))
            .collect();

        let denied_capabilities: Vec<Capability> = requested_capabilities
            .iter()
            .filter(|c| !granted_set.contains(&(c.name.clone(), c.version.clone())))
            .cloned()
            .collect();

        Self {
            pipeline_identity,
            requested_capabilities: requested_capabilities.clone(),
            granted_capabilities,
            denied_capabilities,
            decision: AuthorizationResult::Denied,
            policy_identity,
        }
    }

    pub fn is_allowed(&self) -> bool {
        self.decision == AuthorizationResult::Allowed
    }

    pub fn to_canonical_bytes(&self) -> Result<Vec<u8>, ArtifactError> {
        let canonical = serde_json::json!({
            "schema": "authorization_decision.v1",
            "pipeline_identity": &self.pipeline_identity,
            "requested_capabilities": &self.requested_capabilities,
            "granted_capabilities": &self.granted_capabilities,
            "denied_capabilities": &self.denied_capabilities,
            "decision": &self.decision,
            "policy_identity": &self.policy_identity,
        });

        Ok(serde_json::to_vec(&canonical)?)
    }

    pub fn identity(&self) -> Result<String, ArtifactError> {
        let canonical_bytes = self.to_canonical_bytes()?;
        let mut hasher = sha2::Sha256::new();
        hasher.update(&canonical_bytes);
        let digest = hasher.finalize();
        let mut encoded = String::from("sha256:");
        for byte in digest {
            use std::fmt::Write as _;
            let _ = write!(encoded, "{byte:02x}");
        }
        Ok(encoded)
    }
}

pub trait CapabilityPolicy: Send + Sync {
    fn authorize(&self, requested: &[Capability]) -> Result<Vec<Capability>, String>;
    fn identity(&self) -> String;
}

pub struct AllowAllPolicy;

impl CapabilityPolicy for AllowAllPolicy {
    fn authorize(&self, requested: &[Capability]) -> Result<Vec<Capability>, String> {
        Ok(requested.to_vec())
    }

    fn identity(&self) -> String {
        "sha256:allow_all_policy".to_string()
    }
}

pub struct AllowListPolicy {
    allowed: Vec<(String, String)>,
}

impl AllowListPolicy {
    pub fn new(allowed: Vec<(String, String)>) -> Self {
        Self { allowed }
    }
}

impl CapabilityPolicy for AllowListPolicy {
    fn authorize(&self, requested: &[Capability]) -> Result<Vec<Capability>, String> {
        let mut granted = Vec::new();
        for cap in requested {
            if self
                .allowed
                .iter()
                .any(|(name, version)| name == &cap.name && version == &cap.version)
            {
                granted.push(cap.clone());
            }
        }

        if granted.len() == requested.len() {
            Ok(granted)
        } else {
            Err(format!(
                "not all capabilities granted: granted {}, requested {}",
                granted.len(),
                requested.len()
            ))
        }
    }

    fn identity(&self) -> String {
        let mut hasher = sha2::Sha256::new();
        for (name, version) in &self.allowed {
            hasher.update(name.as_bytes());
            hasher.update(b":");
            hasher.update(version.as_bytes());
            hasher.update(b"\n");
        }
        let digest = hasher.finalize();
        let mut encoded = String::from("sha256:");
        for byte in digest {
            use std::fmt::Write as _;
            let _ = write!(encoded, "{byte:02x}");
        }
        encoded
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ExecutionEvidence {
    pub pipeline_identity: String,
    pub artifact_identity: String,
    pub authorization_decision: AuthorizationDecision,
    pub stage_trace: Vec<ExecutedStage>,
    pub requested_capabilities: Vec<Capability>,
    pub granted_capabilities: Vec<Capability>,
    pub used_capabilities: Vec<Capability>,
    pub execution_result: ExecutionResult,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionResult {
    Success,
    Failed(String),
}

impl std::fmt::Display for ExecutionResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecutionResult::Success => write!(f, "success"),
            ExecutionResult::Failed(e) => write!(f, "failed: {}", e),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ExecutedStage {
    pub label: String,
    pub stage_identity: String,
}

impl ExecutionEvidence {
    pub fn success(
        pipeline_identity: String,
        artifact: &Artifact,
        authorization_decision: AuthorizationDecision,
        stage_trace: Vec<ExecutedStage>,
        requested_capabilities: Vec<Capability>,
        granted_capabilities: Vec<Capability>,
    ) -> Self {
        Self {
            pipeline_identity,
            artifact_identity: artifact.identity.clone(),
            authorization_decision,
            stage_trace,
            requested_capabilities,
            granted_capabilities: granted_capabilities.clone(),
            used_capabilities: granted_capabilities,
            execution_result: ExecutionResult::Success,
        }
    }

    pub fn failed(
        pipeline_identity: String,
        authorization_decision: AuthorizationDecision,
        stage_trace: Vec<ExecutedStage>,
        requested_capabilities: Vec<Capability>,
        granted_capabilities: Vec<Capability>,
        error: String,
    ) -> Self {
        Self {
            pipeline_identity,
            artifact_identity: String::new(),
            authorization_decision,
            stage_trace,
            requested_capabilities,
            granted_capabilities,
            used_capabilities: vec![],
            execution_result: ExecutionResult::Failed(error),
        }
    }

    pub fn to_canonical_bytes(&self) -> Result<Vec<u8>, ArtifactError> {
        let canonical = serde_json::json!({
            "schema": "execution_evidence.v1",
            "pipeline_identity": &self.pipeline_identity,
            "artifact_identity": &self.artifact_identity,
            "authorization_decision": &self.authorization_decision,
            "stage_trace": &self.stage_trace,
            "requested_capabilities": &self.requested_capabilities,
            "granted_capabilities": &self.granted_capabilities,
            "used_capabilities": &self.used_capabilities,
            "execution_result": &self.execution_result,
        });

        Ok(serde_json::to_vec(&canonical)?)
    }

    pub fn identity(&self) -> Result<String, ArtifactError> {
        let canonical_bytes = self.to_canonical_bytes()?;
        let mut hasher = sha2::Sha256::new();
        hasher.update(&canonical_bytes);
        let digest = hasher.finalize();
        let mut encoded = String::from("sha256:");
        for byte in digest {
            use std::fmt::Write as _;
            let _ = write!(encoded, "{byte:02x}");
        }
        Ok(encoded)
    }
}
