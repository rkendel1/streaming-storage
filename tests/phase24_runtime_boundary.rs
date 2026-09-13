use artifact::{
    EntryContentResolver, LocalArtifactStore, RecipeSpec, RecoveredArtifact, ZipMaterializer,
};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Mutex;
use tempfile::TempDir;

#[path = "../examples/runtime-consumer/src/lib.rs"]
mod runtime_consumer;

use runtime_consumer::{RuntimeConsumer, RuntimeExecution};

static RUNTIME_EXECUTION_LOCK: Mutex<()> = Mutex::new(());

struct PreparedArtifact {
    artifact_identity: String,
    store_root: TempDir,
    materialization_root: TempDir,
    materialized_zip: PathBuf,
}

fn prepare_artifact_fixture() -> PreparedArtifact {
    let source_root = TempDir::new().expect("source root should be created");
    let executable_path = source_root.path().join("bin/runtime-fixture");
    if let Some(parent) = executable_path.parent() {
        fs::create_dir_all(parent).expect("fixture executable directory should be created");
    }

    let current_exe = std::env::current_exe().expect("current test binary should be available");
    fs::copy(&current_exe, &executable_path).expect("fixture executable should be copied");
    let mut permissions = fs::metadata(&executable_path)
        .expect("fixture executable metadata should be available")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&executable_path, permissions)
        .expect("fixture executable permissions should be set");

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
    ZipMaterializer
        .materialize_to_path(&artifact, &built, &materialized_zip)
        .expect("artifact should materialize to zip");

    PreparedArtifact {
        artifact_identity: artifact.identity,
        store_root,
        materialization_root,
        materialized_zip,
    }
}

fn execute_runtime(
    prepared: &PreparedArtifact,
    mode: &str,
    runtime_input: &str,
) -> RuntimeExecution {
    let _execution_guard = RUNTIME_EXECUTION_LOCK
        .lock()
        .expect("runtime execution lock should be available");
    RuntimeConsumer
        .execute_materialized_zip(
            &prepared.artifact_identity,
            &prepared.materialized_zip,
            "bin/runtime-fixture",
            &["--exact", "phase24_runtime_fixture_process", "--nocapture"],
            &[
                ("PHASE24_RUNTIME_FIXTURE_MODE", mode),
                ("ARTIFACT_RUNTIME_INPUT", runtime_input),
            ],
        )
        .expect("runtime should execute materialized artifact")
}

fn recover(prepared: &PreparedArtifact) -> RecoveredArtifact {
    LocalArtifactStore::open(prepared.store_root.path())
        .expect("fresh store should open")
        .recover(&prepared.artifact_identity)
        .expect("artifact should recover by identity")
}

fn sha256_prefixed(bytes: &[u8]) -> String {
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
fn runtime_executes_materialized_artifact() {
    let prepared = prepare_artifact_fixture();
    assert!(prepared.materialization_root.path().join("artifact.zip").exists());

    let execution = execute_runtime(&prepared, "success", "hello");

    assert_eq!(execution.artifact_identity, prepared.artifact_identity);
    assert_eq!(execution.exit_code, Some(0));
    assert!(execution.executable_path.ends_with("bin/runtime-fixture"));
    assert!(
        execution
            .stdout
            .lines()
            .any(|line| line.trim() == "hello-runtime")
    );
}

#[test]
fn runtime_does_not_change_artifact_identity() {
    let prepared = prepare_artifact_fixture();
    let identity_before_execution = prepared.artifact_identity.clone();

    let execution = execute_runtime(&prepared, "success", "hello");
    let recovered_after_execution = recover(&prepared);
    let identity_after_execution = recovered_after_execution.artifact().identity.clone();

    assert_eq!(execution.artifact_identity, identity_before_execution);
    assert_eq!(identity_after_execution, identity_before_execution);
}

#[test]
fn runtime_state_is_ephemeral() {
    let prepared = prepare_artifact_fixture();

    let execution = execute_runtime(&prepared, "success", "hello");

    assert!(execution.runtime_execution_id > 0);
    assert!(!execution.runtime_directory.exists());
    let recovered = recover(&prepared);
    assert_eq!(recovered.artifact().identity, prepared.artifact_identity);
}

#[test]
fn runtime_failure_does_not_corrupt_artifact() {
    let prepared = prepare_artifact_fixture();
    let identity_before_failure = prepared.artifact_identity.clone();

    let failure_execution = execute_runtime(&prepared, "fail", "ignored");
    assert_eq!(failure_execution.exit_code, Some(42));
    assert_eq!(failure_execution.artifact_identity, identity_before_failure);
    assert!(failure_execution.stderr.contains("runtime failure requested"));

    let recovered = recover(&prepared);
    assert_eq!(recovered.artifact().identity, identity_before_failure);
    assert_eq!(recovered.artifact().lineage().len(), 0);

    let rematerialized = ZipMaterializer
        .materialize_to_vec(recovered.artifact(), &recovered)
        .expect("recovered artifact should rematerialize after runtime failure");
    assert!(!rematerialized.is_empty());
}

#[test]
fn same_artifact_can_execute_multiple_times() {
    let prepared = prepare_artifact_fixture();

    let first = execute_runtime(&prepared, "success", "hello");
    let second = execute_runtime(&prepared, "success", "hello");

    assert_eq!(first.exit_code, Some(0));
    assert_eq!(second.exit_code, Some(0));
    assert_eq!(first.artifact_identity, second.artifact_identity);
    assert_eq!(first.artifact_identity, prepared.artifact_identity);
    assert_ne!(first.runtime_execution_id, second.runtime_execution_id);
}

#[test]
fn artifact_recovers_after_execution() {
    let prepared = prepare_artifact_fixture();
    let identity_before = prepared.artifact_identity.clone();

    let _ = execute_runtime(&prepared, "success", "hello");
    let _ = execute_runtime(&prepared, "fail", "hello");

    let recovered = LocalArtifactStore::open(prepared.store_root.path())
        .expect("fresh store should open")
        .recover(&identity_before)
        .expect("artifact should recover after runtime executions");

    assert_eq!(recovered.artifact().identity, identity_before);

    let materialized = ZipMaterializer
        .materialize_to_vec(recovered.artifact(), &recovered)
        .expect("recovered artifact should materialize");
    assert!(!materialized.is_empty());
}

#[test]
fn recovered_content_remains_valid() {
    let prepared = prepare_artifact_fixture();

    let _ = execute_runtime(&prepared, "success", "hello");
    let recovered = recover(&prepared);

    let entry = recovered
        .artifact()
        .entries
        .iter()
        .find(|entry| entry.path == "bin/runtime-fixture")
        .expect("runtime executable entry should exist");

    let mut content = Vec::new();
    recovered
        .open("bin/runtime-fixture")
        .expect("recovered executable should be readable")
        .read_to_end(&mut content)
        .expect("recovered executable bytes should be readable");

    assert_eq!(content.len() as u64, entry.size);
    assert_eq!(sha256_prefixed(&content), entry.content_digest);
}

#[test]
fn runtime_has_no_provider_dependency() {
    let source = include_str!("../examples/runtime-consumer/src/lib.rs").to_lowercase();
    let forbidden = [
        "docker",
        "oci",
        "kubernetes",
        "fly",
        "render",
        "daemon",
        "credential",
        "aws",
        "gcp",
        "azure",
    ];

    for token in forbidden {
        assert!(
            !source.contains(token),
            "runtime consumer must not depend on provider term: {token}"
        );
    }
}

#[test]
fn phase24_runtime_fixture_process() {
    let Ok(mode) = std::env::var("PHASE24_RUNTIME_FIXTURE_MODE") else {
        return;
    };

    match mode.as_str() {
        "success" => {
            let input = std::env::var("ARTIFACT_RUNTIME_INPUT")
                .expect("runtime input should be provided in success mode");
            println!("{input}-runtime");
        }
        "fail" => {
            eprintln!("runtime failure requested");
            std::process::exit(42);
        }
        _ => {
            eprintln!("unknown runtime mode");
            std::process::exit(64);
        }
    }
}
