use artifact::{
    core::sha256_prefixed, EntryContentResolver, LocalArtifactStore, RecipeSpec, RecoveredArtifact,
};
use runtime_contract::{
    ExternalRuntimeExecutor, OciRepresentation, OciRuntimeContract, RuntimeExecutionInput,
    RuntimeExecutionResult,
};
use std::fs;
use std::io::{self, Read};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tempfile::TempDir;
use transfer_boundary::{
    export_oci_representation, import_oci_representation, inspect_image_identity, remove_image,
    transfer_representation, ExportedRepresentation, ImportedRepresentation,
};

#[path = "../examples/oci-consumer/src/lib.rs"]
mod oci_consumer;
#[path = "../examples/runtime-contract/src/lib.rs"]
mod runtime_contract;
#[path = "../examples/transfer-boundary/src/lib.rs"]
mod transfer_boundary;

use oci_consumer::{OciConsumer, OciMaterialization};

static PHASE28_LOCK: Mutex<()> = Mutex::new(());
static NEXT_DESTINATION_ID: AtomicU64 = AtomicU64::new(1);

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

struct TransferRoundTrip {
    context_a_root: TempDir,
    context_b_root: TempDir,
    exported: ExportedRepresentation,
    imported: ImportedRepresentation,
}

impl Drop for TransferRoundTrip {
    fn drop(&mut self) {
        let _ = remove_image(&self.imported.destination_image_reference);
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
        "#!/bin/sh\nif [ \"${PHASE28_RUNTIME_MODE}\" = \"fail\" ]; then\n  echo phase28 failure requested >&2\n  exit 42\nfi\nprintf '%s-runtime\\n' \"${ARTIFACT_RUNTIME_INPUT}\"\necho phase28 stderr >&2\n",
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
        .expect("artifact should persist before transfer boundary test");

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

fn unique_destination_image() -> String {
    let id = NEXT_DESTINATION_ID.fetch_add(1, Ordering::Relaxed);
    format!("artifact-transfer-boundary:{}-{id}", std::process::id())
}

fn perform_transfer(prepared: &PreparedArtifact) -> TransferRoundTrip {
    let context_a_root = TempDir::new().expect("context A root should be created");
    let context_b_root = TempDir::new().expect("context B root should be created");

    let source_archive = context_a_root.path().join("oci-representation.tar");
    let exported = export_oci_representation(
        &prepared.artifact_identity,
        &prepared.oci_materialization.oci_representation_digest,
        &prepared.oci_materialization.image_reference,
        &source_archive,
    )
    .expect("source context should export oci representation");

    let destination_archive = context_b_root.path().join("received-representation.tar");
    transfer_representation(&source_archive, &destination_archive)
        .expect("representation should transfer to destination context");

    let destination_image_reference = unique_destination_image();
    let imported = import_oci_representation(
        &prepared.artifact_identity,
        &prepared.oci_materialization.oci_representation_digest,
        &exported.transfer_digest,
        &destination_archive,
        &destination_image_reference,
    )
    .expect("destination context should import transferred representation");

    TransferRoundTrip {
        context_a_root,
        context_b_root,
        exported,
        imported,
    }
}

fn execute_transferred(
    round_trip: &TransferRoundTrip,
    runtime_input: &str,
) -> RuntimeExecutionResult {
    let representation = OciRepresentation {
        artifact_identity: round_trip.imported.artifact_identity.as_str(),
        representation_identity: round_trip.imported.representation_identity.as_str(),
        image_reference: round_trip.imported.destination_image_reference.as_str(),
        executable_relative_path: ENTRY_POINT,
    };
    let execution_input = RuntimeExecutionInput {
        artifact_identity: round_trip.imported.artifact_identity.as_str(),
        executable_relative_path: ENTRY_POINT,
        arguments: &[],
        environment: &[
            ("PHASE28_RUNTIME_MODE", "success"),
            ("ARTIFACT_RUNTIME_INPUT", runtime_input),
        ],
    };

    OciRuntimeContract
        .execute(&representation, execution_input)
        .expect("transferred representation should execute")
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
fn transfer_preserves_artifact_identity() {
    let _guard = PHASE28_LOCK
        .lock()
        .expect("phase28 lock should be available");
    let prepared = prepare_artifact_fixture();
    let round_trip = perform_transfer(&prepared);

    assert_eq!(
        round_trip.exported.artifact_identity,
        prepared.artifact_identity
    );
    assert_eq!(
        round_trip.imported.artifact_identity,
        prepared.artifact_identity
    );
}

#[test]
fn transfer_preserves_representation_identity() {
    let _guard = PHASE28_LOCK
        .lock()
        .expect("phase28 lock should be available");
    let prepared = prepare_artifact_fixture();
    let round_trip = perform_transfer(&prepared);

    assert_eq!(
        round_trip.exported.representation_identity,
        prepared.oci_materialization.oci_representation_digest
    );
    assert_eq!(
        round_trip.imported.representation_identity,
        prepared.oci_materialization.oci_representation_digest
    );
}

#[test]
fn transfer_creates_independent_destination_context() {
    let _guard = PHASE28_LOCK
        .lock()
        .expect("phase28 lock should be available");
    let prepared = prepare_artifact_fixture();
    let round_trip = perform_transfer(&prepared);

    let source_archive = round_trip
        .context_a_root
        .path()
        .join("oci-representation.tar");
    let destination_archive = round_trip
        .context_b_root
        .path()
        .join("received-representation.tar");
    assert_ne!(source_archive, destination_archive);
    assert!(source_archive.exists());
    assert!(destination_archive.exists());

    fs::remove_file(&source_archive).expect("source context archive should be removable");
    assert!(destination_archive.exists());

    let destination_identity =
        inspect_image_identity(&round_trip.imported.destination_image_reference)
            .expect("destination representation should remain inspectable");
    assert_eq!(
        destination_identity,
        prepared.oci_materialization.oci_representation_digest
    );
}

#[test]
fn destination_can_consume_transferred_representation() {
    let _guard = PHASE28_LOCK
        .lock()
        .expect("phase28 lock should be available");
    let prepared = prepare_artifact_fixture();
    let round_trip = perform_transfer(&prepared);

    let execution = execute_transferred(&round_trip, "destination");

    assert_eq!(execution.artifact_identity, prepared.artifact_identity);
    assert_eq!(
        execution.representation_identity,
        prepared.oci_materialization.oci_representation_digest
    );
    assert_eq!(execution.exit_code, Some(0));
}

#[test]
fn source_context_can_be_removed_after_transfer() {
    let _guard = PHASE28_LOCK
        .lock()
        .expect("phase28 lock should be available");
    let prepared = prepare_artifact_fixture();
    let round_trip = perform_transfer(&prepared);

    remove_image(&prepared.oci_materialization.image_reference)
        .expect("source image should be removable after transfer");
    fs::remove_dir_all(round_trip.context_a_root.path())
        .expect("source context directory should be removable after transfer");

    let execution = execute_transferred(&round_trip, "independent");
    assert_eq!(execution.exit_code, Some(0));
    assert!(execution
        .stdout
        .lines()
        .any(|line| line.trim() == "independent-runtime"));
}

#[test]
fn corrupted_transfer_fails_integrity_check() {
    let _guard = PHASE28_LOCK
        .lock()
        .expect("phase28 lock should be available");
    let prepared = prepare_artifact_fixture();
    let context_a_root = TempDir::new().expect("context A root should be created");
    let context_b_root = TempDir::new().expect("context B root should be created");

    let source_archive = context_a_root.path().join("oci-representation.tar");
    let exported = export_oci_representation(
        &prepared.artifact_identity,
        &prepared.oci_materialization.oci_representation_digest,
        &prepared.oci_materialization.image_reference,
        &source_archive,
    )
    .expect("source context should export oci representation");

    let destination_archive = context_b_root.path().join("received-representation.tar");
    transfer_representation(&source_archive, &destination_archive)
        .expect("representation should transfer to destination context");

    let mut bytes = fs::read(&destination_archive).expect("destination archive should be readable");
    bytes[0] ^= 0xFF;
    fs::write(&destination_archive, bytes).expect("destination archive should be rewritten");

    let import_error = import_oci_representation(
        &prepared.artifact_identity,
        &prepared.oci_materialization.oci_representation_digest,
        &exported.transfer_digest,
        &destination_archive,
        &unique_destination_image(),
    )
    .expect_err("corrupted transfer should fail integrity validation");

    assert_eq!(import_error.kind(), io::ErrorKind::InvalidData);
    assert!(import_error
        .to_string()
        .contains("transfer digest mismatch"));
}

#[test]
fn transfer_does_not_mutate_artifact() {
    let _guard = PHASE28_LOCK
        .lock()
        .expect("phase28 lock should be available");
    let prepared = prepare_artifact_fixture();
    let identity_before = prepared.artifact_identity.clone();

    let _ = perform_transfer(&prepared);

    let recovered = recover(&prepared);
    assert_eq!(recovered.artifact().identity, identity_before);
    assert_recovered_content_valid(&prepared);
}

#[test]
fn transfer_does_not_create_lineage() {
    let _guard = PHASE28_LOCK
        .lock()
        .expect("phase28 lock should be available");
    let prepared = prepare_artifact_fixture();

    let _ = perform_transfer(&prepared);

    let recovered = recover(&prepared);
    assert_eq!(recovered.artifact().lineage().len(), 0);
}

#[test]
fn repeated_transfer_preserves_identity() {
    let _guard = PHASE28_LOCK
        .lock()
        .expect("phase28 lock should be available");
    let prepared = prepare_artifact_fixture();

    let first = perform_transfer(&prepared);
    let second = perform_transfer(&prepared);

    assert_eq!(
        first.imported.artifact_identity,
        second.imported.artifact_identity
    );
    assert_eq!(
        first.imported.representation_identity,
        second.imported.representation_identity
    );
}

#[test]
fn transferred_representation_executes() {
    let _guard = PHASE28_LOCK
        .lock()
        .expect("phase28 lock should be available");
    let prepared = prepare_artifact_fixture();
    let round_trip = perform_transfer(&prepared);

    let execution = execute_transferred(&round_trip, "phase28");

    assert_eq!(execution.exit_code, Some(0));
    assert!(execution
        .stdout
        .lines()
        .any(|line| line.trim() == "phase28-runtime"));
    assert!(execution.stderr.contains("phase28 stderr"));
    assert!(!execution.runtime_execution_identifier.is_empty());
}

#[test]
fn execution_identity_remains_runtime_owned() {
    let _guard = PHASE28_LOCK
        .lock()
        .expect("phase28 lock should be available");
    let prepared = prepare_artifact_fixture();
    let round_trip = perform_transfer(&prepared);

    let first = execute_transferred(&round_trip, "first");
    let second = execute_transferred(&round_trip, "second");

    assert_ne!(
        first.runtime_execution_identifier,
        second.runtime_execution_identifier
    );
    assert_eq!(first.artifact_identity, second.artifact_identity);
    assert_eq!(
        first.representation_identity,
        second.representation_identity
    );
}

#[test]
fn recovered_artifact_survives_transfer() {
    let _guard = PHASE28_LOCK
        .lock()
        .expect("phase28 lock should be available");
    let prepared = prepare_artifact_fixture();
    let identity_before = prepared.artifact_identity.clone();

    let recovered_before = recover(&prepared);
    let recovered_materialization = OciConsumer
        .materialize_to_oci_image(recovered_before.artifact(), &recovered_before, ENTRY_POINT)
        .expect("recovered artifact should materialize to oci image");

    let context_a_root = TempDir::new().expect("context A root should be created");
    let context_b_root = TempDir::new().expect("context B root should be created");
    let source_archive = context_a_root.path().join("recovered-oci.tar");
    let exported = export_oci_representation(
        &identity_before,
        &recovered_materialization.oci_representation_digest,
        &recovered_materialization.image_reference,
        &source_archive,
    )
    .expect("recovered representation should export");
    let destination_archive = context_b_root.path().join("recovered-received-oci.tar");
    transfer_representation(&source_archive, &destination_archive)
        .expect("recovered representation should transfer");

    let destination_image = unique_destination_image();
    let imported = import_oci_representation(
        &identity_before,
        &recovered_materialization.oci_representation_digest,
        &exported.transfer_digest,
        &destination_archive,
        &destination_image,
    )
    .expect("recovered representation should import");

    let representation = OciRepresentation {
        artifact_identity: imported.artifact_identity.as_str(),
        representation_identity: imported.representation_identity.as_str(),
        image_reference: imported.destination_image_reference.as_str(),
        executable_relative_path: ENTRY_POINT,
    };
    let input = RuntimeExecutionInput {
        artifact_identity: imported.artifact_identity.as_str(),
        executable_relative_path: ENTRY_POINT,
        arguments: &[],
        environment: &[
            ("PHASE28_RUNTIME_MODE", "success"),
            ("ARTIFACT_RUNTIME_INPUT", "recovered"),
        ],
    };
    let execution = OciRuntimeContract
        .execute(&representation, input)
        .expect("recovered transferred representation should execute");

    assert_eq!(execution.exit_code, Some(0));
    assert_eq!(recover(&prepared).artifact().identity, identity_before);
    assert_recovered_content_valid(&prepared);

    remove_image(&recovered_materialization.image_reference)
        .expect("recovered source image should be removable");
    remove_image(&imported.destination_image_reference)
        .expect("recovered destination image should be removable");
}

#[test]
fn transfer_boundary_has_no_deployment_or_provider_dependency() {
    let source = include_str!("../examples/transfer-boundary/src/lib.rs").to_lowercase();
    let manifest = include_str!("../examples/transfer-boundary/Cargo.toml").to_lowercase();
    let forbidden = [
        "deployment",
        "provider",
        "registry credential",
        "cloud credential",
        "remote deployment",
        "kubernetes",
        "aws_sdk",
        "google_cloud",
        "azure_",
        "gcp",
        "transfer id",
    ];

    for token in forbidden {
        assert!(
            !source.contains(token) && !manifest.contains(token),
            "transfer boundary must not depend on forbidden term: {token}"
        );
    }
}

#[test]
fn transfer_export_records_expected_metadata() {
    let _guard = PHASE28_LOCK
        .lock()
        .expect("phase28 lock should be available");
    let prepared = prepare_artifact_fixture();
    let round_trip = perform_transfer(&prepared);

    assert_eq!(
        round_trip.exported.representation_format,
        "docker-archive".to_string()
    );
    assert!(round_trip.exported.representation_size > 0);
    assert!(round_trip.exported.transfer_digest.starts_with("sha256:"));
    assert_eq!(
        round_trip.imported.transfer_digest,
        round_trip.exported.transfer_digest
    );
}
