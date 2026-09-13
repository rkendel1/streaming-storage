use artifact::{
    core::sha256_prefixed, EntryContentResolver, LocalArtifactStore, RecipeSpec, RecoveredArtifact,
    ZipMaterializer,
};
use runtime_contract::oci_consumer::{OciConsumer, OciMaterialization};
use runtime_contract::{
    ExternalRuntimeExecutor, OciRepresentation, OciRuntimeContract, RuntimeExecutionInput,
    RuntimeExecutionResult, ZipRepresentation, ZipRuntimeContract,
};
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Mutex;
use tempfile::TempDir;

#[path = "../examples/runtime-contract/src/lib.rs"]
mod runtime_contract;

static PHASE26B_LOCK: Mutex<()> = Mutex::new(());

const ENTRY_POINT: &str = "bin/runtime-fixture";

struct PreparedArtifact {
    artifact_identity: String,
    store_root: TempDir,
    materialization_root: TempDir,
    materialized_zip: PathBuf,
    zip_representation_digest: String,
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
        "#!/bin/sh\nif [ \"${PHASE26B_RUNTIME_MODE}\" = \"fail\" ]; then\n  echo phase26b failure requested >&2\n  exit 42\nfi\nprintf '%s-runtime\\n' \"${ARTIFACT_RUNTIME_INPUT}\"\necho phase26b stderr >&2\n",
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
        .expect("artifact should persist before runtime execution");

    let materialization_root = TempDir::new().expect("materialization root should be created");
    let materialized_zip = materialization_root.path().join("artifact.zip");
    let zip_result = ZipMaterializer
        .materialize_to_path(&artifact, &built, &materialized_zip)
        .expect("artifact should materialize to zip");

    let oci_materialization = OciConsumer
        .materialize_to_oci_image(&artifact, &built, ENTRY_POINT)
        .expect("artifact should materialize to oci image");

    PreparedArtifact {
        artifact_identity: artifact.identity,
        store_root,
        materialization_root,
        materialized_zip,
        zip_representation_digest: zip_result.output_digest,
        oci_materialization,
    }
}

fn recover(prepared: &PreparedArtifact) -> RecoveredArtifact {
    LocalArtifactStore::open(prepared.store_root.path())
        .expect("fresh store should open")
        .recover(&prepared.artifact_identity)
        .expect("artifact should recover by identity")
}

fn zip_representation(prepared: &PreparedArtifact) -> ZipRepresentation<'_> {
    ZipRepresentation {
        artifact_identity: &prepared.artifact_identity,
        representation_identity: &prepared.zip_representation_digest,
        path: &prepared.materialized_zip,
    }
}

fn oci_representation(prepared: &PreparedArtifact) -> OciRepresentation<'_> {
    OciRepresentation::from_materialization(&prepared.oci_materialization, ENTRY_POINT)
}

fn execute_zip(
    prepared: &PreparedArtifact,
    mode: &str,
    runtime_input: &str,
) -> RuntimeExecutionResult {
    let environment = [
        ("PHASE26B_RUNTIME_MODE", mode),
        ("ARTIFACT_RUNTIME_INPUT", runtime_input),
    ];
    let input = RuntimeExecutionInput {
        artifact_identity: &prepared.artifact_identity,
        executable_relative_path: ENTRY_POINT,
        arguments: &[],
        environment: &environment,
    };

    ZipRuntimeContract
        .execute(&zip_representation(prepared), input)
        .expect("zip runtime contract should execute")
}

fn execute_oci(
    prepared: &PreparedArtifact,
    mode: &str,
    runtime_input: &str,
) -> RuntimeExecutionResult {
    let environment = [
        ("PHASE26B_RUNTIME_MODE", mode),
        ("ARTIFACT_RUNTIME_INPUT", runtime_input),
    ];
    let input = RuntimeExecutionInput {
        artifact_identity: &prepared.artifact_identity,
        executable_relative_path: ENTRY_POINT,
        arguments: &[],
        environment: &environment,
    };

    OciRuntimeContract
        .execute(&oci_representation(prepared), input)
        .expect("oci runtime contract should execute")
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

#[test]
fn runtime_contract_executes_zip_representation() {
    let _guard = PHASE26B_LOCK
        .lock()
        .expect("phase26b lock should be available");
    let prepared = prepare_artifact_fixture();

    let execution = execute_zip(&prepared, "success", "zip");

    assert_eq!(execution.artifact_identity, prepared.artifact_identity);
    assert_eq!(
        execution.representation_identity,
        prepared.zip_representation_digest
    );
    assert_eq!(execution.exit_code, Some(0));
}

#[test]
fn runtime_contract_executes_oci_representation() {
    let _guard = PHASE26B_LOCK
        .lock()
        .expect("phase26b lock should be available");
    let prepared = prepare_artifact_fixture();

    let execution = execute_oci(&prepared, "success", "oci");

    assert_eq!(execution.artifact_identity, prepared.artifact_identity);
    assert_eq!(
        execution.representation_identity,
        prepared.oci_materialization.oci_representation_digest
    );
    assert_eq!(execution.exit_code, Some(0));
}

#[test]
fn runtime_contract_preserves_artifact_identity() {
    let _guard = PHASE26B_LOCK
        .lock()
        .expect("phase26b lock should be available");
    let prepared = prepare_artifact_fixture();
    let identity_before = prepared.artifact_identity.clone();

    let zip_execution = execute_zip(&prepared, "success", "zip");
    let identity_after_zip = recover(&prepared).artifact().identity.clone();
    let oci_execution = execute_oci(&prepared, "success", "oci");
    let identity_after_oci = recover(&prepared).artifact().identity.clone();

    assert_eq!(zip_execution.artifact_identity, identity_before);
    assert_eq!(oci_execution.artifact_identity, identity_before);
    assert_eq!(identity_after_zip, identity_before);
    assert_eq!(identity_after_oci, identity_before);
    assert_ne!(
        zip_execution.representation_identity,
        oci_execution.representation_identity
    );
    assert_ne!(
        zip_execution.runtime_execution_identifier,
        oci_execution.runtime_execution_identifier
    );
}

#[test]
fn runtime_contract_captures_exit_status() {
    let _guard = PHASE26B_LOCK
        .lock()
        .expect("phase26b lock should be available");
    let prepared = prepare_artifact_fixture();

    let zip_failure = execute_zip(&prepared, "fail", "ignored");
    let oci_failure = execute_oci(&prepared, "fail", "ignored");

    assert_eq!(zip_failure.exit_code, Some(42));
    assert_eq!(oci_failure.exit_code, Some(42));
}

#[test]
fn runtime_contract_captures_stdout_and_stderr() {
    let _guard = PHASE26B_LOCK
        .lock()
        .expect("phase26b lock should be available");
    let prepared = prepare_artifact_fixture();

    let zip_execution = execute_zip(&prepared, "success", "zip");
    let oci_execution = execute_oci(&prepared, "success", "oci");

    assert!(zip_execution
        .stdout
        .lines()
        .any(|line| line.trim() == "zip-runtime"));
    assert!(oci_execution
        .stdout
        .lines()
        .any(|line| line.trim() == "oci-runtime"));
    assert!(zip_execution.stderr.contains("phase26b stderr"));
    assert!(oci_execution.stderr.contains("phase26b stderr"));
}

#[test]
fn runtime_contract_produces_distinct_execution_ids() {
    let _guard = PHASE26B_LOCK
        .lock()
        .expect("phase26b lock should be available");
    let prepared = prepare_artifact_fixture();

    let first = execute_zip(&prepared, "success", "first");
    let second = execute_zip(&prepared, "success", "second");
    let third = execute_oci(&prepared, "success", "third");

    assert_ne!(
        first.runtime_execution_identifier,
        second.runtime_execution_identifier
    );
    assert_ne!(
        second.runtime_execution_identifier,
        third.runtime_execution_identifier
    );
}

#[test]
fn runtime_contract_supports_repeat_execution() {
    let _guard = PHASE26B_LOCK
        .lock()
        .expect("phase26b lock should be available");
    let prepared = prepare_artifact_fixture();

    let first_zip = execute_zip(&prepared, "success", "hello");
    let second_zip = execute_zip(&prepared, "success", "hello");
    let first_oci = execute_oci(&prepared, "success", "hello");
    let second_oci = execute_oci(&prepared, "success", "hello");

    assert_eq!(first_zip.exit_code, Some(0));
    assert_eq!(second_zip.exit_code, Some(0));
    assert_eq!(first_oci.exit_code, Some(0));
    assert_eq!(second_oci.exit_code, Some(0));
    assert_eq!(first_zip.artifact_identity, second_zip.artifact_identity);
    assert_eq!(first_oci.artifact_identity, second_oci.artifact_identity);
}

#[test]
fn runtime_contract_does_not_mutate_artifact() {
    let _guard = PHASE26B_LOCK
        .lock()
        .expect("phase26b lock should be available");
    let prepared = prepare_artifact_fixture();
    let identity_before = prepared.artifact_identity.clone();

    let zip_failure = execute_zip(&prepared, "fail", "ignored");
    let oci_failure = execute_oci(&prepared, "fail", "ignored");
    assert_eq!(zip_failure.exit_code, Some(42));
    assert_eq!(oci_failure.exit_code, Some(42));

    let recovered = recover(&prepared);
    assert_eq!(recovered.artifact().identity, identity_before);
    assert_eq!(recovered.artifact().lineage().len(), 0);
    assert_recovered_content_valid(&prepared);

    assert_eq!(execute_zip(&prepared, "success", "zip").exit_code, Some(0));
    assert_eq!(execute_oci(&prepared, "success", "oci").exit_code, Some(0));
}

#[test]
fn runtime_contract_supports_recovered_artifact() {
    let _guard = PHASE26B_LOCK
        .lock()
        .expect("phase26b lock should be available");
    let prepared = prepare_artifact_fixture();
    let identity_before = prepared.artifact_identity.clone();

    let recovered_before = recover(&prepared);
    let recovered_zip_path = prepared.materialization_root.path().join("recovered.zip");
    let recovered_zip = ZipMaterializer
        .materialize_to_path(
            recovered_before.artifact(),
            &recovered_before,
            &recovered_zip_path,
        )
        .expect("recovered artifact should materialize to zip");
    let recovered_oci = OciConsumer
        .materialize_to_oci_image(recovered_before.artifact(), &recovered_before, ENTRY_POINT)
        .expect("recovered artifact should materialize to oci");

    let recovered_zip_representation = ZipRepresentation {
        artifact_identity: recovered_before.artifact().identity.as_str(),
        representation_identity: recovered_zip.output_digest.as_str(),
        path: &recovered_zip_path,
    };
    let recovered_oci_representation =
        OciRepresentation::from_materialization(&recovered_oci, ENTRY_POINT);
    let input = RuntimeExecutionInput {
        artifact_identity: recovered_before.artifact().identity.as_str(),
        executable_relative_path: ENTRY_POINT,
        arguments: &[],
        environment: &[
            ("PHASE26B_RUNTIME_MODE", "success"),
            ("ARTIFACT_RUNTIME_INPUT", "recovered"),
        ],
    };

    let zip_execution = ZipRuntimeContract
        .execute(&recovered_zip_representation, input)
        .expect("recovered zip should execute");
    let oci_execution = OciRuntimeContract
        .execute(&recovered_oci_representation, input)
        .expect("recovered oci should execute");

    assert_eq!(zip_execution.artifact_identity, identity_before);
    assert_eq!(oci_execution.artifact_identity, identity_before);
    assert_eq!(recover(&prepared).artifact().identity, identity_before);
    assert_recovered_content_valid(&prepared);

    OciConsumer
        .remove_image(&recovered_oci.image_reference)
        .expect("recovered oci image should be removable");
}

#[test]
fn runtime_contract_has_no_deployment_dependency() {
    let source = include_str!("../examples/runtime-contract/src/lib.rs").to_lowercase();
    let runtime_source = include_str!("../examples/runtime-consumer/src/lib.rs").to_lowercase();
    let oci_source = include_str!("../examples/oci-consumer/src/lib.rs").to_lowercase();
    let manifest = include_str!("../examples/runtime-contract/Cargo.toml").to_lowercase();
    let forbidden = [
        "fly",
        "render",
        "kubernetes",
        "aws_sdk",
        "google_cloud",
        "azure_",
        "gcp",
        "registry credential",
        "cloud credential",
        "remote deployment",
        "rollout",
        "provider lifecycle",
        "remote health",
        "scaling",
    ];

    for token in forbidden {
        assert!(
            !source.contains(token)
                && !runtime_source.contains(token)
                && !oci_source.contains(token)
                && !manifest.contains(token),
            "runtime contract must not depend on deployment term: {token}"
        );
    }
}
