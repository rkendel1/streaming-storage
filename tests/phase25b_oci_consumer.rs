use artifact::{
    core::sha256_prefixed, EntryContentResolver, LocalArtifactStore, RecipeSpec, RecoveredArtifact,
    ZipMaterializer,
};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;
use std::sync::Mutex;
use tempfile::TempDir;

#[path = "../examples/oci-consumer/src/lib.rs"]
mod oci_consumer;
#[path = "../examples/runtime-consumer/src/lib.rs"]
mod runtime_consumer;

use oci_consumer::{OciConsumer, OciMaterialization, OciRuntimeExecution};
use runtime_consumer::{RuntimeConsumer, RuntimeExecution};

static OCI_EXECUTION_LOCK: Mutex<()> = Mutex::new(());

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
    let executable_path = source_root.path().join("bin/runtime-fixture");
    if let Some(parent) = executable_path.parent() {
        fs::create_dir_all(parent).expect("fixture executable directory should be created");
    }

    fs::write(
        &executable_path,
        "#!/bin/sh\nif [ \"${PHASE25B_RUNTIME_MODE}\" = \"fail\" ]; then\n  echo runtime failure requested >&2\n  exit 42\nfi\nprintf '%s-runtime\\n' \"${ARTIFACT_RUNTIME_INPUT}\"\n",
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

    let oci_materialization =
        expect_oci(OciConsumer.materialize_to_oci_image(&artifact, &built, "bin/runtime-fixture"));

    PreparedArtifact {
        artifact_identity: artifact.identity,
        store_root,
        materialization_root,
        materialized_zip,
        zip_representation_digest: zip_result.output_digest,
        oci_materialization,
    }
}

fn execute_zip(prepared: &PreparedArtifact, mode: &str, runtime_input: &str) -> RuntimeExecution {
    RuntimeConsumer
        .execute_materialized_zip(
            &prepared.artifact_identity,
            &prepared.materialized_zip,
            "bin/runtime-fixture",
            &[],
            &[
                ("PHASE25B_RUNTIME_MODE", mode),
                ("ARTIFACT_RUNTIME_INPUT", runtime_input),
            ],
        )
        .expect("zip runtime should execute materialized artifact")
}

fn execute_oci(
    prepared: &PreparedArtifact,
    mode: &str,
    runtime_input: &str,
) -> OciRuntimeExecution {
    let _runtime_guard = OCI_EXECUTION_LOCK
        .lock()
        .expect("runtime execution lock should be available");
    expect_oci(OciConsumer.execute(
        &prepared.oci_materialization,
        &[],
        &[
            ("PHASE25B_RUNTIME_MODE", mode),
            ("ARTIFACT_RUNTIME_INPUT", runtime_input),
        ],
    ))
}

fn recover(prepared: &PreparedArtifact) -> RecoveredArtifact {
    LocalArtifactStore::open(prepared.store_root.path())
        .expect("fresh store should open")
        .recover(&prepared.artifact_identity)
        .expect("artifact should recover by identity")
}

fn expect_oci<T>(result: io::Result<T>) -> T {
    result.unwrap_or_else(|error| panic!("{error}"))
}

fn sha256_local(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let mut encoded = String::from("sha256:");
    for byte in hasher.finalize() {
        use std::fmt::Write as _;
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

#[test]
fn oci_materializes_real_artifact() {
    let prepared = prepare_artifact_fixture();

    assert_eq!(
        prepared.oci_materialization.artifact_identity,
        prepared.artifact_identity
    );
    assert!(prepared
        .oci_materialization
        .oci_representation_digest
        .starts_with("sha256:"));
    assert_ne!(
        prepared.oci_materialization.oci_representation_digest,
        prepared.artifact_identity
    );
}

#[test]
fn oci_representation_is_deterministic() {
    let prepared = prepare_artifact_fixture();
    let recovered = recover(&prepared);
    let second = expect_oci(OciConsumer.materialize_to_oci_image(
        recovered.artifact(),
        &recovered,
        "bin/runtime-fixture",
    ));

    assert_eq!(
        prepared.oci_materialization.artifact_identity,
        second.artifact_identity
    );
    assert_eq!(
        prepared.oci_materialization.oci_representation_digest,
        second.oci_representation_digest
    );

    OciConsumer
        .remove_image(&second.image_reference)
        .expect("second oci image should be removable");
}

#[test]
fn oci_runtime_executes_artifact() {
    let prepared = prepare_artifact_fixture();

    let execution = execute_oci(&prepared, "success", "hello");

    assert_eq!(execution.artifact_identity, prepared.artifact_identity);
    assert_eq!(execution.exit_code, Some(0));
    assert!(execution
        .stdout
        .lines()
        .any(|line| line.trim() == "hello-runtime"));
}

#[test]
fn oci_does_not_change_artifact_identity() {
    let prepared = prepare_artifact_fixture();
    let identity_before_execution = prepared.artifact_identity.clone();

    let execution = execute_oci(&prepared, "success", "hello");
    let recovered_after_execution = recover(&prepared);
    let identity_after_execution = recovered_after_execution.artifact().identity.clone();

    assert_eq!(execution.artifact_identity, identity_before_execution);
    assert_eq!(identity_after_execution, identity_before_execution);
}

#[test]
fn same_artifact_has_stable_identity_across_zip_and_oci() {
    let prepared = prepare_artifact_fixture();
    let identity_before_zip = prepared.artifact_identity.clone();

    let zip_execution = execute_zip(&prepared, "success", "hello");
    let identity_after_zip = recover(&prepared).artifact().identity.clone();

    let identity_before_oci = recover(&prepared).artifact().identity.clone();
    let oci_execution = execute_oci(&prepared, "success", "hello");
    let identity_after_oci = recover(&prepared).artifact().identity.clone();

    assert_eq!(zip_execution.exit_code, Some(0));
    assert_eq!(oci_execution.exit_code, Some(0));

    assert_eq!(identity_before_zip, identity_after_zip);
    assert_eq!(identity_after_zip, identity_before_oci);
    assert_eq!(identity_before_oci, identity_after_oci);

    assert_ne!(
        prepared.zip_representation_digest,
        prepared.oci_materialization.oci_representation_digest
    );
    assert_ne!(
        prepared.oci_materialization.oci_representation_digest,
        prepared.artifact_identity
    );
}

#[test]
fn oci_runtime_failure_does_not_corrupt_artifact() {
    let prepared = prepare_artifact_fixture();
    let identity_before_failure = prepared.artifact_identity.clone();

    let failure_execution = execute_oci(&prepared, "fail", "ignored");
    assert_eq!(failure_execution.exit_code, Some(42));
    assert_eq!(failure_execution.artifact_identity, identity_before_failure);
    assert!(failure_execution
        .stderr
        .contains("runtime failure requested"));

    let recovered = recover(&prepared);
    assert_eq!(recovered.artifact().identity, identity_before_failure);

    let mut content = Vec::new();
    recovered
        .open("bin/runtime-fixture")
        .expect("recovered fixture should be readable")
        .read_to_end(&mut content)
        .expect("recovered fixture bytes should be readable");
    let entry = recovered
        .artifact()
        .entries
        .iter()
        .find(|entry| entry.path == "bin/runtime-fixture")
        .expect("runtime fixture entry should exist");
    assert_eq!(content.len() as u64, entry.size);
    assert_eq!(sha256_prefixed(&content), entry.content_digest);

    let success_execution = execute_oci(&prepared, "success", "hello");
    assert_eq!(success_execution.exit_code, Some(0));
}

#[test]
fn same_artifact_can_execute_multiple_times() {
    let prepared = prepare_artifact_fixture();

    let first = execute_oci(&prepared, "success", "hello");
    let second = execute_oci(&prepared, "success", "hello");

    assert_eq!(first.exit_code, Some(0));
    assert_eq!(second.exit_code, Some(0));
    assert_eq!(first.artifact_identity, second.artifact_identity);
    assert_eq!(first.artifact_identity, prepared.artifact_identity);
    assert_ne!(first.runtime_execution_id, second.runtime_execution_id);
    assert_ne!(
        first.runtime_execution_identifier,
        second.runtime_execution_identifier
    );
}

#[test]
fn artifact_recovers_after_oci_execution() {
    let prepared = prepare_artifact_fixture();
    let identity_before = prepared.artifact_identity.clone();

    let _ = execute_oci(&prepared, "success", "hello");
    let _ = execute_oci(&prepared, "fail", "hello");

    let recovered = recover(&prepared);
    assert_eq!(recovered.artifact().identity, identity_before);

    let rematerialized = expect_oci(OciConsumer.materialize_to_oci_image(
        recovered.artifact(),
        &recovered,
        "bin/runtime-fixture",
    ));
    assert_eq!(rematerialized.artifact_identity, identity_before);

    OciConsumer
        .remove_image(&rematerialized.image_reference)
        .expect("rematerialized image should be removable");
}

#[test]
fn recovered_content_remains_valid() {
    let prepared = prepare_artifact_fixture();

    let _ = execute_oci(&prepared, "success", "hello");
    let recovered = recover(&prepared);

    let entry = recovered
        .artifact()
        .entries
        .iter()
        .find(|entry| entry.path == "bin/runtime-fixture")
        .expect("runtime fixture entry should exist");

    let mut content = Vec::new();
    recovered
        .open("bin/runtime-fixture")
        .expect("recovered executable should be readable")
        .read_to_end(&mut content)
        .expect("recovered executable bytes should be readable");

    assert_eq!(content.len() as u64, entry.size);
    assert_eq!(sha256_local(&content), entry.content_digest);
}

#[test]
fn oci_consumer_has_no_provider_dependency() {
    let source = include_str!("../examples/oci-consumer/src/lib.rs").to_lowercase();
    let manifest = include_str!("../examples/oci-consumer/Cargo.toml").to_lowercase();
    let forbidden = [
        "fly",
        "render",
        "kubernetes",
        "aws_sdk",
        "google_cloud",
        "azure_",
        "gcp",
        "eks",
        "aks",
    ];

    for token in forbidden {
        assert!(
            !source.contains(token) && !manifest.contains(token),
            "oci consumer must not depend on provider term: {token}"
        );
    }
}
