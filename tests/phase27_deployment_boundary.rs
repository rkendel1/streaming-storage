use artifact::{
    EntryContentResolver, LocalArtifactStore, RecipeSpec, RecoveredArtifact, core::sha256_prefixed,
};
use std::fs;
use std::io::Read;
use std::sync::Mutex;
use tempfile::TempDir;

#[path = "../examples/ssh-deployment-boundary/src/lib.rs"]
mod ssh_deployment_boundary;

use ssh_deployment_boundary::runtime_contract::oci_consumer::{OciConsumer, OciMaterialization};
use ssh_deployment_boundary::runtime_contract::{OciRepresentation, RuntimeExecutionInput};
use ssh_deployment_boundary::{
    BoundaryProofStatus, RemoteDeploymentResult, SshDockerDeploymentBoundary, SshRemoteRuntime,
    missing_remote_evidence,
};

static PHASE27_LOCK: Mutex<()> = Mutex::new(());

const ENTRY_POINT: &str = "bin/runtime-fixture";

struct PreparedArtifact {
    artifact_identity: String,
    store_root: TempDir,
    oci_materialization: OciMaterialization,
}

impl Drop for PreparedArtifact {
    fn drop(&mut self) {
        let _ = OciConsumer.remove_image(&self.oci_materialization.image_reference);
    }
}

fn prepare_artifact_fixture() -> PreparedArtifact {
    let source_root = TempDir::new().expect("source root should be created");
    let executable_path = source_root.path().join(ENTRY_POINT);
    if let Some(parent) = executable_path.parent() {
        fs::create_dir_all(parent).expect("fixture executable directory should be created");
    }

    fs::write(
        &executable_path,
        "#!/bin/sh\nif [ \"${PHASE27_RUNTIME_MODE}\" = \"fail\" ]; then\n  echo phase27 failure requested >&2\n  exit 42\nfi\nprintf '%s-remote-runtime\\n' \"${ARTIFACT_RUNTIME_INPUT}\"\necho phase27 stderr >&2\n",
    )
    .expect("fixture executable should be written");

    let pipeline = RecipeSpec::directory_zip()
        .compile()
        .expect("production recipe compilation should succeed");
    let built = pipeline
        .build_from_directory(source_root.path())
        .expect("artifact should build from source directory");
    let artifact = built.artifact().clone();

    let store_root = TempDir::new().expect("store root should be created");
    let store = LocalArtifactStore::open(store_root.path()).expect("store should open");
    store
        .persist(&artifact, &built)
        .expect("artifact should persist before remote runtime execution");

    let oci_materialization = OciConsumer
        .materialize_to_oci_image(&artifact, &built, ENTRY_POINT)
        .expect("artifact should materialize to oci image");

    PreparedArtifact {
        artifact_identity: artifact.identity,
        store_root,
        oci_materialization,
    }
}

fn recover(prepared: &PreparedArtifact) -> RecoveredArtifact {
    LocalArtifactStore::open(prepared.store_root.path())
        .expect("fresh store should open")
        .recover(&prepared.artifact_identity)
        .expect("artifact should recover by identity")
}

fn assert_recovered_content_valid(prepared: &PreparedArtifact) {
    let recovered = recover(prepared);
    let entry = recovered
        .artifact()
        .entries
        .iter()
        .find(|entry| entry.path == ENTRY_POINT)
        .expect("runtime fixture entry should exist");

    let mut content = Vec::new();
    recovered
        .open(ENTRY_POINT)
        .expect("recovered fixture should be readable")
        .read_to_end(&mut content)
        .expect("recovered fixture bytes should be readable");

    assert_eq!(content.len() as u64, entry.size);
    assert_eq!(sha256_prefixed(&content), entry.content_digest);
}

fn oci_representation(prepared: &PreparedArtifact) -> OciRepresentation<'_> {
    OciRepresentation::from_materialization(&prepared.oci_materialization, ENTRY_POINT)
}

fn execute_remote(
    mode: &str,
    runtime_input: &str,
) -> Option<(PreparedArtifact, RemoteDeploymentResult)> {
    let remote = SshRemoteRuntime::from_env()?;
    let prepared = prepare_artifact_fixture();
    let environment = [
        ("PHASE27_RUNTIME_MODE", mode),
        ("ARTIFACT_RUNTIME_INPUT", runtime_input),
    ];
    let input = RuntimeExecutionInput {
        artifact_identity: &prepared.artifact_identity,
        executable_relative_path: ENTRY_POINT,
        arguments: &[],
        environment: &environment,
    };

    let result = SshDockerDeploymentBoundary::new(remote)
        .execute_oci_representation(&oci_representation(&prepared), input)
        .expect("configured ssh remote docker runtime should execute oci representation");

    Some((prepared, result))
}

fn assert_remote_not_configured(criterion: &'static str) {
    let evidence = missing_remote_evidence(criterion);
    assert_eq!(evidence.criterion, criterion);
    assert!(evidence.reason.contains("PHASE27_SSH_TARGET"));
}

#[test]
fn deployment_executes_real_artifact_remotely() {
    let _guard = PHASE27_LOCK
        .lock()
        .expect("phase27 lock should be available");

    if let Some((_prepared, result)) = execute_remote("success", "phase27") {
        assert_eq!(result.exit_code, Some(0));
        assert!(result.stdout.contains("phase27-remote-runtime"));
        assert_eq!(result.lifecycle.execute_observed, true);
    } else {
        assert_remote_not_configured("deployment_executes_real_artifact_remotely");
    }
}

#[test]
fn deployment_preserves_artifact_identity() {
    let _guard = PHASE27_LOCK
        .lock()
        .expect("phase27 lock should be available");

    if let Some((prepared, result)) = execute_remote("success", "identity") {
        assert_eq!(result.artifact_identity, prepared.artifact_identity);
        assert_eq!(
            recover(&prepared).artifact().identity,
            prepared.artifact_identity
        );
    } else {
        assert_remote_not_configured("deployment_preserves_artifact_identity");
    }
}

#[test]
fn deployment_preserves_representation_identity() {
    let _guard = PHASE27_LOCK
        .lock()
        .expect("phase27 lock should be available");

    if let Some((prepared, result)) = execute_remote("success", "representation") {
        assert_eq!(
            result.representation_identity,
            prepared.oci_materialization.oci_representation_digest
        );
        assert_eq!(
            result.transport.remote_representation_identity,
            prepared.oci_materialization.oci_representation_digest
        );
        assert!(result.transport.digest_survived_transport);
        assert!(!result.transport.remote_rebuilt);
        assert!(!result.transport.registry_required);
    } else {
        assert_remote_not_configured("deployment_preserves_representation_identity");
    }
}

#[test]
fn deployment_produces_remote_execution_result() {
    let _guard = PHASE27_LOCK
        .lock()
        .expect("phase27 lock should be available");

    if let Some((_prepared, result)) = execute_remote("success", "result") {
        assert!(!result.remote_execution_identifier.is_empty());
        assert_eq!(result.exit_code, Some(0));
        assert!(result.stdout.contains("result-remote-runtime"));
        assert!(result.stderr.contains("phase27 stderr"));
    } else {
        assert_remote_not_configured("deployment_produces_remote_execution_result");
    }
}

#[test]
fn deployment_failure_does_not_mutate_artifact() {
    let _guard = PHASE27_LOCK
        .lock()
        .expect("phase27 lock should be available");

    if let Some((prepared, result)) = execute_remote("fail", "ignored") {
        assert_eq!(result.exit_code, Some(42));
        assert!(result.stderr.contains("phase27 failure requested"));
        let recovered = recover(&prepared);
        assert_eq!(recovered.artifact().identity, prepared.artifact_identity);
        assert_eq!(recovered.artifact().lineage().len(), 0);
        assert_recovered_content_valid(&prepared);
    } else {
        assert_remote_not_configured("deployment_failure_does_not_mutate_artifact");
    }
}

#[test]
fn artifact_recovers_after_remote_execution() {
    let _guard = PHASE27_LOCK
        .lock()
        .expect("phase27 lock should be available");

    if let Some((prepared, result)) = execute_remote("success", "recover") {
        assert_eq!(result.exit_code, Some(0));
        assert_eq!(
            recover(&prepared).artifact().identity,
            prepared.artifact_identity
        );
        assert_recovered_content_valid(&prepared);
    } else {
        assert_remote_not_configured("artifact_recovers_after_remote_execution");
    }
}

#[test]
fn deployment_identity_is_separate_if_provider_exposes_one() {
    let _guard = PHASE27_LOCK
        .lock()
        .expect("phase27 lock should be available");

    if let Some((_prepared, result)) = execute_remote("success", "deployment") {
        let deployment_identity = result
            .deployment_identity
            .as_ref()
            .expect("ssh docker provider exposes container id as deployment identity");
        assert_ne!(deployment_identity, &result.artifact_identity);
        assert_ne!(deployment_identity, &result.representation_identity);
        assert!(result.deployment_identity_is_distinct.is_proven());
    } else {
        assert_remote_not_configured("deployment_identity_is_separate_if_provider_exposes_one");
    }
}

#[test]
fn remote_execution_identity_is_separate_from_artifact_identity() {
    let _guard = PHASE27_LOCK
        .lock()
        .expect("phase27 lock should be available");

    if let Some((_prepared, result)) = execute_remote("success", "execution") {
        assert_ne!(result.remote_execution_identifier, result.artifact_identity);
        assert_ne!(
            result.remote_execution_identifier,
            result.representation_identity
        );
        assert_eq!(
            result.execution_identity_is_distinct,
            BoundaryProofStatus::NotProven(
                "ssh docker create/start uses the container id as both deployment handle and execution handle"
                    .to_string()
            )
        );
    } else {
        assert_remote_not_configured(
            "remote_execution_identity_is_separate_from_artifact_identity",
        );
    }
}

#[test]
fn remote_execution_can_be_observed() {
    let _guard = PHASE27_LOCK
        .lock()
        .expect("phase27 lock should be available");

    if let Some((_prepared, result)) = execute_remote("success", "observe") {
        assert!(result.lifecycle.observe_observed);
        assert!(result.stdout.contains("observe-remote-runtime"));
        assert!(result.stderr.contains("phase27 stderr"));
    } else {
        assert_remote_not_configured("remote_execution_can_be_observed");
    }
}

#[test]
fn deployment_lifecycle_is_observed_not_assumed() {
    let _guard = PHASE27_LOCK
        .lock()
        .expect("phase27 lock should be available");

    if let Some((_prepared, result)) = execute_remote("success", "lifecycle") {
        assert!(result.lifecycle.create_observed);
        assert!(result.lifecycle.execute_observed);
        assert!(result.lifecycle.observe_observed);
        assert!(result.lifecycle.delete_observed);
        assert!(result.lifecycle.deployment_persistent_until_delete);
        assert!(
            !result
                .lifecycle
                .execution_implicitly_creates_deployment_state
        );
        assert!(matches!(
            result.lifecycle.multiple_executions_reuse_same_deployment,
            BoundaryProofStatus::NotProven(_)
        ));
    } else {
        assert_remote_not_configured("deployment_lifecycle_is_observed_not_assumed");
    }
}
