use artifact::{
    Artifact, ArtifactTransform, Capability, CreationMetadata, EntryContentResolver,
    LocalArtifactStore, MaterializerSpec, MemoryContentResolver, PipelineSpec, PrefixTransform,
    Provenance, RecoveredArtifact, SelectStageSpec, SourceSpec, StageSpec,
    TransformedContentResolver, ZipMaterializer,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::process::Command;
use tempfile::TempDir;

fn kernel_pipeline(materializer: MaterializerSpec, stages: Vec<StageSpec>) -> PipelineSpec {
    let mut all_stages = vec![StageSpec::Select(
        SelectStageSpec::new(Vec::new(), Vec::new()).expect("empty selection is valid"),
    )];
    all_stages.extend(stages);
    all_stages.push(StageSpec::Manifest);
    all_stages.push(StageSpec::Validate);

    PipelineSpec {
        source: SourceSpec::Directory,
        stages: all_stages,
        materializer,
        capabilities: vec![
            Capability::new("filesystem.read", "1"),
            Capability::new("manifest.generate", "1"),
            Capability::new("artifact.validate", "1"),
        ],
    }
}

fn source_tree(contents: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new().expect("temp dir should be created");
    for (path, content) in contents {
        let full_path = dir.path().join(path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).expect("parent directory should be created");
        }
        fs::write(full_path, content).expect("source file should be written");
    }
    dir
}

fn digest_for_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut encoded = String::from("sha256:");
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

fn transform_artifact() -> (TempDir, Artifact, BTreeMap<String, Vec<u8>>, String, String) {
    let source = source_tree(&[("app.txt", "hello"), ("lib/data.txt", "world")]);
    let built = kernel_pipeline(MaterializerSpec::Zip, Vec::new())
        .build_from_directory(source.path())
        .expect("source artifact should build");
    let input_identity = built.artifact().identity.clone();
    let transform = PrefixTransform::new("bundle");
    let transformed = transform
        .apply(built.artifact(), &built)
        .expect("production transform should succeed");
    (
        source,
        transformed.artifact,
        transformed.content_updates,
        input_identity,
        transform
            .transform_identity()
            .expect("transform identity should be deterministic"),
    )
}

fn assert_recovered_matches(
    recovered: &RecoveredArtifact,
    original: &Artifact,
    original_zip: &[u8],
    input_identity: &str,
    transform_identity: &str,
) {
    let artifact = recovered.artifact();
    assert_eq!(artifact.identity, original.identity);
    assert_eq!(artifact.entries, original.entries);
    assert_eq!(artifact.provenance, original.provenance);
    assert_eq!(artifact.pipeline_identity, original.pipeline_identity);
    assert_eq!(artifact.capabilities, original.capabilities);
    assert_eq!(artifact.lineage(), original.lineage());
    assert_eq!(artifact.lineage().len(), 1);
    assert_eq!(
        artifact.lineage()[0].input_artifact_identity,
        input_identity
    );
    assert_eq!(artifact.lineage()[0].transform_identity, transform_identity);

    let mut resolved = String::new();
    recovered
        .open("bundle/app.txt")
        .expect("recovered content should resolve")
        .read_to_string(&mut resolved)
        .expect("recovered content should be readable");
    assert_eq!(resolved, "hello");

    let recovered_zip = ZipMaterializer
        .materialize_to_vec(artifact, recovered)
        .expect("recovered artifact should materialize");
    let second_recovered_zip = ZipMaterializer
        .materialize_to_vec(artifact, recovered)
        .expect("recovered artifact should materialize deterministically");
    assert_eq!(recovered_zip, original_zip);
    assert_eq!(recovered_zip, second_recovered_zip);
    assert_ne!(digest_for_bytes(&recovered_zip), artifact.identity);
}

#[test]
fn phase17_recovers_artifact_semantics_and_content_across_store_instances() {
    let store_dir = TempDir::new().expect("store dir should be created");
    let (source, artifact, contents, input_identity, transform_identity) = transform_artifact();
    let resolver = TransformedContentResolver::new(contents);
    let original_zip = ZipMaterializer
        .materialize_to_vec(&artifact, &resolver)
        .expect("original materialization should succeed");

    let writer = LocalArtifactStore::open(store_dir.path()).expect("store should open");
    writer
        .persist(&artifact, &resolver)
        .expect("artifact should persist");
    drop(writer);
    drop(resolver);
    drop(source);

    let reader = LocalArtifactStore::open(store_dir.path()).expect("fresh store should open");
    let recovered = reader
        .recover(&artifact.identity)
        .expect("artifact should recover");
    assert_recovered_matches(
        &recovered,
        &artifact,
        &original_zip,
        &input_identity,
        &transform_identity,
    );

    let second_reader =
        LocalArtifactStore::open(store_dir.path()).expect("second store should open");
    let second_recovered = second_reader
        .recover(&artifact.identity)
        .expect("second reader should recover without shared memory");
    assert_recovered_matches(
        &second_recovered,
        &artifact,
        &original_zip,
        &input_identity,
        &transform_identity,
    );
}

#[test]
fn phase17_rejects_corrupt_incomplete_and_missing_durable_state() {
    let store_dir = TempDir::new().expect("store dir should be created");
    let (_source, artifact, contents, _input_identity, _transform_identity) = transform_artifact();
    let resolver = TransformedContentResolver::new(contents);
    let store = LocalArtifactStore::open(store_dir.path()).expect("store should open");
    store
        .persist(&artifact, &resolver)
        .expect("artifact should persist");

    let artifact_path = store
        .artifact_path(&artifact.identity)
        .expect("artifact path should be available");
    fs::write(&artifact_path, b"{\"identity\":")
        .expect("test should be able to truncate artifact state");
    let truncated = store
        .recover(&artifact.identity)
        .expect_err("truncated artifact state must fail explicitly");
    assert!(truncated.to_string().contains("serialization error"));

    store
        .persist(&artifact, &resolver)
        .expect("artifact should persist again");
    let first_entry = &artifact.entries[0];
    let content_path = store
        .content_path(&first_entry.content_digest)
        .expect("content path should be available");
    fs::remove_file(&content_path).expect("test should remove content");
    let missing = store
        .recover(&artifact.identity)
        .expect_err("missing content must fail explicitly");
    assert!(missing.to_string().contains("missing content"));

    store
        .persist(&artifact, &resolver)
        .expect("artifact should persist after missing content");
    fs::write(&content_path, b"drift").expect("test should corrupt content");
    let corrupt = store
        .recover(&artifact.identity)
        .expect_err("corrupt content must fail explicitly");
    assert!(corrupt.to_string().contains("stored content"));
}

#[test]
fn phase17_identity_rules_survive_persistence_without_physical_metadata() {
    let store_dir = TempDir::new().expect("store dir should be created");
    let source = source_tree(&[("same.txt", "same")]);
    let pipeline = kernel_pipeline(MaterializerSpec::Zip, Vec::new());
    let built = pipeline
        .build_from_directory(source.path())
        .expect("source artifact should build");
    let artifact = built.artifact().clone();
    let with_volatile_metadata = Artifact::from_parts(
        artifact.entries.clone(),
        artifact.pipeline_identity.clone(),
        artifact.capabilities.clone(),
        Provenance {
            source_identity: artifact.provenance.source_identity.clone(),
            pipeline_identity: artifact.provenance.pipeline_identity.clone(),
            creation_metadata: CreationMetadata {
                created_at: Some("2026-09-13T19:27:00Z".to_string()),
            },
        },
    )
    .expect("artifact should rebuild with volatile metadata");
    assert_eq!(artifact.identity, with_volatile_metadata.identity);

    let store = LocalArtifactStore::open(store_dir.path()).expect("store should open");
    store
        .persist(&with_volatile_metadata, &built)
        .expect("artifact should persist");
    let recovered = store
        .recover(&artifact.identity)
        .expect("artifact should recover");

    assert_eq!(recovered.artifact().identity, artifact.identity);
    assert_eq!(
        recovered.artifact().provenance.creation_metadata.created_at,
        Some("2026-09-13T19:27:00Z".to_string())
    );
}

#[test]
fn phase17_process_boundary_persists_then_recovers() {
    let store_dir = TempDir::new().expect("store dir should be created");
    let current_exe = std::env::current_exe().expect("test executable should be known");
    let identity_path = store_dir.path().join("identity.txt");
    let digest_path = store_dir.path().join("digest.txt");

    let persist_status = Command::new(&current_exe)
        .arg("--exact")
        .arg("phase17_process_boundary_helper")
        .arg("--nocapture")
        .env("PHASE17_HELPER_MODE", "persist")
        .env("PHASE17_STORE_ROOT", store_dir.path())
        .env("PHASE17_IDENTITY_PATH", &identity_path)
        .status()
        .expect("persist helper should launch");
    assert!(persist_status.success());

    let recover_status = Command::new(&current_exe)
        .arg("--exact")
        .arg("phase17_process_boundary_helper")
        .arg("--nocapture")
        .env("PHASE17_HELPER_MODE", "recover")
        .env("PHASE17_STORE_ROOT", store_dir.path())
        .env("PHASE17_IDENTITY_PATH", &identity_path)
        .env("PHASE17_DIGEST_PATH", &digest_path)
        .status()
        .expect("recover helper should launch");
    assert!(recover_status.success());

    let identity = fs::read_to_string(&identity_path).expect("identity should be written");
    let digest = fs::read_to_string(&digest_path).expect("digest should be written");
    assert!(identity.starts_with("sha256:"));
    assert!(digest.starts_with("sha256:"));
    assert_ne!(identity, digest);
}

#[test]
fn phase17_process_boundary_helper() {
    let Ok(mode) = std::env::var("PHASE17_HELPER_MODE") else {
        return;
    };
    let store_root = std::env::var_os("PHASE17_STORE_ROOT").expect("store root env is required");
    let identity_path =
        std::env::var_os("PHASE17_IDENTITY_PATH").expect("identity path env is required");
    let store = LocalArtifactStore::open(store_root).expect("store should open in helper");

    match mode.as_str() {
        "persist" => {
            let (_source, artifact, contents, _input_identity, _transform_identity) =
                transform_artifact();
            let resolver = TransformedContentResolver::new(contents);
            store
                .persist(&artifact, &resolver)
                .expect("helper should persist artifact");
            fs::write(identity_path, &artifact.identity).expect("helper should write identity");
        }
        "recover" => {
            let digest_path =
                std::env::var_os("PHASE17_DIGEST_PATH").expect("digest path env is required");
            let identity = fs::read_to_string(identity_path).expect("helper should read identity");
            let recovered = store
                .recover(&identity)
                .expect("helper should recover artifact");
            let bytes = ZipMaterializer
                .materialize_to_vec(recovered.artifact(), &recovered)
                .expect("helper should materialize recovered artifact");
            fs::write(digest_path, digest_for_bytes(&bytes)).expect("helper should write digest");
        }
        other => panic!("unknown helper mode: {other}"),
    }
}

#[test]
fn phase17_persist_reports_missing_source_content_explicitly() {
    let store_dir = TempDir::new().expect("store dir should be created");
    let source = source_tree(&[("app.txt", "hello")]);
    let built = kernel_pipeline(MaterializerSpec::Zip, Vec::new())
        .build_from_directory(source.path())
        .expect("source artifact should build");
    let mut contents = BTreeMap::new();
    contents.insert("different.txt".to_string(), b"hello".to_vec());
    let resolver = MemoryContentResolver::new(contents);
    let store = LocalArtifactStore::open(store_dir.path()).expect("store should open");

    let err = store
        .persist(built.artifact(), &resolver)
        .expect_err("missing content must not be substituted");
    assert!(err.to_string().contains("missing content"));
}
