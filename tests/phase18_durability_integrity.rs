use artifact::{
    Artifact, ArtifactTransform, Capability, EntryContentResolver, LocalArtifactStore,
    MaterializerSpec, PipelineSpec, PrefixTransform, SelectStageSpec, SourceBackedArtifact,
    SourceSpec, StageSpec, TarMaterializer, TransformedContentResolver, ZipMaterializer,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
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

fn build_source_artifact(contents: &[(&str, &str)]) -> (TempDir, SourceBackedArtifact) {
    let source = source_tree(contents);
    let built = kernel_pipeline(MaterializerSpec::Zip, Vec::new())
        .build_from_directory(source.path())
        .expect("source artifact should build");
    (source, built)
}

fn transform_artifact() -> (TempDir, Artifact, BTreeMap<String, Vec<u8>>, String, String) {
    let (source, built) = build_source_artifact(&[("app.txt", "alpha"), ("lib/data.txt", "beta")]);
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

fn assert_same_semantics(left: &Artifact, right: &Artifact) {
    assert_eq!(left.identity, right.identity);
    assert_eq!(left.entries, right.entries);
    assert_eq!(left.manifest, right.manifest);
    assert_eq!(left.pipeline_identity, right.pipeline_identity);
    assert_eq!(left.capabilities, right.capabilities);
    assert_eq!(left.provenance, right.provenance);
    assert_eq!(left.lineage(), right.lineage());
    assert_eq!(left.semantic_declaration, right.semantic_declaration);
}

#[test]
fn same_logical_artifact_has_same_identity_in_different_stores_and_paths() {
    let (source_a, built_a) = build_source_artifact(&[("nested/file.txt", "same content")]);
    let (source_b, built_b) = build_source_artifact(&[("nested/file.txt", "same content")]);
    assert_ne!(source_a.path(), source_b.path());
    assert_eq!(built_a.artifact().identity, built_b.artifact().identity);

    let store_root_a = TempDir::new().expect("first store root should be created");
    let store_root_b = TempDir::new().expect("second store root should be created");
    assert_ne!(store_root_a.path(), store_root_b.path());
    let store_a = LocalArtifactStore::open(store_root_a.path()).expect("first store should open");
    let store_b = LocalArtifactStore::open(store_root_b.path()).expect("second store should open");

    store_a
        .persist(built_a.artifact(), &built_a)
        .expect("artifact should persist in first store");
    store_b
        .persist(built_b.artifact(), &built_b)
        .expect("artifact should persist in second store");

    let recovered_a = store_a
        .recover(&built_a.artifact().identity)
        .expect("artifact should recover from first store");
    let recovered_b = store_b
        .recover(&built_b.artifact().identity)
        .expect("artifact should recover from second store");
    assert_same_semantics(recovered_a.artifact(), recovered_b.artifact());
}

#[test]
fn repeated_recovery_and_repersisting_preserve_identity_provenance_and_lineage() {
    let (_source, artifact, contents, input_identity, transform_identity) = transform_artifact();
    let resolver = TransformedContentResolver::new(contents);
    let store_root_a = TempDir::new().expect("first store root should be created");
    let store_root_b = TempDir::new().expect("second store root should be created");
    let store_a = LocalArtifactStore::open(store_root_a.path()).expect("first store should open");
    let store_b = LocalArtifactStore::open(store_root_b.path()).expect("second store should open");

    store_a
        .persist(&artifact, &resolver)
        .expect("transformed artifact should persist");
    let recovered_once = store_a
        .recover(&artifact.identity)
        .expect("transformed artifact should recover");
    store_b
        .persist(recovered_once.artifact(), &recovered_once)
        .expect("recovered artifact should persist without semantic mutation");
    let recovered_twice = store_b
        .recover(&artifact.identity)
        .expect("re-persisted artifact should recover");

    assert_same_semantics(&artifact, recovered_once.artifact());
    assert_same_semantics(&artifact, recovered_twice.artifact());
    assert_eq!(recovered_twice.artifact().lineage().len(), 1);
    assert_eq!(
        recovered_twice.artifact().lineage()[0].input_artifact_identity,
        input_identity
    );
    assert_eq!(
        recovered_twice.artifact().lineage()[0].transform_identity,
        transform_identity
    );
    assert_eq!(recovered_twice.artifact().provenance, artifact.provenance);
}

#[test]
fn semantic_declarations_remain_outside_identity_but_survive_recovery() {
    let (_source, built) = build_source_artifact(&[("claim.txt", "claim content")]);
    let base = built.artifact().clone();
    let declared_x = base.clone().with_semantic_declaration("declaration:x");
    let declared_y = base.clone().with_semantic_declaration("declaration:y");

    assert_eq!(base.identity, declared_x.identity);
    assert_eq!(base.identity, declared_y.identity);
    assert_ne!(declared_x.semantic_type(), declared_y.semantic_type());

    let store_root = TempDir::new().expect("store root should be created");
    let store = LocalArtifactStore::open(store_root.path()).expect("store should open");
    store
        .persist(&declared_x, &built)
        .expect("declared artifact should persist");
    let recovered_x = store
        .recover(&base.identity)
        .expect("declared artifact should recover");
    assert_eq!(recovered_x.artifact().identity, base.identity);
    assert_eq!(
        recovered_x.artifact().semantic_type(),
        Some("declaration:x")
    );

    store
        .persist(&declared_y, &built)
        .expect("changed declaration should persist under same identity contract");
    let recovered_y = store
        .recover(&base.identity)
        .expect("changed declaration should recover");
    assert_eq!(recovered_y.artifact().identity, base.identity);
    assert_eq!(
        recovered_y.artifact().semantic_type(),
        Some("declaration:y")
    );
}

#[test]
fn metadata_corruption_and_content_substitution_fail_closed() {
    let (_source_a, built_a) = build_source_artifact(&[("file.txt", "alpha")]);
    let (_source_b, built_b) = build_source_artifact(&[("file.txt", "omega")]);
    let store_root = TempDir::new().expect("store root should be created");
    let store = LocalArtifactStore::open(store_root.path()).expect("store should open");
    store
        .persist(built_a.artifact(), &built_a)
        .expect("first artifact should persist");
    store
        .persist(built_b.artifact(), &built_b)
        .expect("second artifact should persist");

    let artifact_path = store
        .artifact_path(&built_a.artifact().identity)
        .expect("artifact path should be available");
    let mut metadata: Value =
        serde_json::from_slice(&fs::read(&artifact_path).expect("artifact state should be read"))
            .expect("artifact state should be json");
    metadata["identity"] = Value::String(built_b.artifact().identity.clone());
    fs::write(
        &artifact_path,
        serde_json::to_vec(&metadata).expect("corrupt metadata should serialize"),
    )
    .expect("test should corrupt metadata");
    let metadata_err = store
        .recover(&built_a.artifact().identity)
        .expect_err("identity-corrupted metadata must fail closed");
    assert!(
        metadata_err
            .to_string()
            .contains("stored artifact identity")
    );

    store
        .persist(built_a.artifact(), &built_a)
        .expect("first artifact should be restored");
    let first_entry = &built_a.artifact().entries[0];
    let second_entry = &built_b.artifact().entries[0];
    fs::copy(
        store
            .content_path(&second_entry.content_digest)
            .expect("second content path should be available"),
        store
            .content_path(&first_entry.content_digest)
            .expect("first content path should be available"),
    )
    .expect("test should substitute content blobs");
    let substitution_err = store
        .recover(&built_a.artifact().identity)
        .expect_err("content substitution must fail closed");
    assert!(
        substitution_err
            .to_string()
            .contains("stored content digest mismatch")
    );
}

#[test]
fn recovered_content_resolution_revalidates_after_recovery() {
    let (_source, built) = build_source_artifact(&[("file.txt", "alpha")]);
    let store_root = TempDir::new().expect("store root should be created");
    let store = LocalArtifactStore::open(store_root.path()).expect("store should open");
    store
        .persist(built.artifact(), &built)
        .expect("artifact should persist");
    let recovered = store
        .recover(&built.artifact().identity)
        .expect("artifact should recover");

    let content_path = store
        .content_path(&built.artifact().entries[0].content_digest)
        .expect("content path should be available");
    fs::write(content_path, b"omega").expect("test should corrupt content after recovery");

    let err = match recovered.open("file.txt") {
        Ok(_) => panic!("recovered resolver must reject post-recovery content drift"),
        Err(err) => err,
    };
    assert!(err.to_string().contains("stored content digest mismatch"));
}

#[test]
fn cross_store_copy_preserves_artifact_identity_without_redefining_it() {
    let (_source, built) = build_source_artifact(&[("copy.txt", "portable")]);
    let store_root_a = TempDir::new().expect("first store root should be created");
    let store_root_b = TempDir::new().expect("second store root should be created");
    let store_a = LocalArtifactStore::open(store_root_a.path()).expect("first store should open");
    let store_b = LocalArtifactStore::open(store_root_b.path()).expect("second store should open");
    store_a
        .persist(built.artifact(), &built)
        .expect("artifact should persist");

    fs::copy(
        store_a
            .artifact_path(&built.artifact().identity)
            .expect("source artifact path should be available"),
        store_b
            .artifact_path(&built.artifact().identity)
            .expect("destination artifact path should be available"),
    )
    .expect("artifact state should copy across stores");
    for entry in &built.artifact().entries {
        fs::copy(
            store_a
                .content_path(&entry.content_digest)
                .expect("source content path should be available"),
            store_b
                .content_path(&entry.content_digest)
                .expect("destination content path should be available"),
        )
        .expect("content state should copy across stores");
    }

    let recovered = store_b
        .recover(&built.artifact().identity)
        .expect("copied artifact should recover from independent store root");
    assert_same_semantics(built.artifact(), recovered.artifact());
}

#[test]
fn materialization_is_downstream_and_does_not_mutate_persisted_state() {
    let (_source, artifact, contents, _input_identity, _transform_identity) = transform_artifact();
    let resolver = TransformedContentResolver::new(contents);
    let store_root = TempDir::new().expect("store root should be created");
    let store = LocalArtifactStore::open(store_root.path()).expect("store should open");
    store
        .persist(&artifact, &resolver)
        .expect("artifact should persist");
    let recovered = store
        .recover(&artifact.identity)
        .expect("artifact should recover");
    let durable_before = fs::read(
        store
            .artifact_path(&artifact.identity)
            .expect("artifact path should be available"),
    )
    .expect("durable artifact state should be readable before materialization");

    let zip_path = store_root.path().join("out.zip");
    let tar_path = store_root.path().join("out.tar");
    let zip_result = ZipMaterializer
        .materialize_to_path(recovered.artifact(), &recovered, &zip_path)
        .expect("recovered artifact should materialize as zip");
    let tar_result = TarMaterializer
        .materialize_to_path(recovered.artifact(), &recovered, &tar_path)
        .expect("recovered artifact should materialize as tar");

    assert_eq!(zip_result.artifact_identity, artifact.identity);
    assert_eq!(tar_result.artifact_identity, artifact.identity);
    assert_eq!(recovered.artifact().identity, artifact.identity);
    assert_ne!(zip_result.output_digest, artifact.identity);
    assert_ne!(tar_result.output_digest, artifact.identity);
    assert_ne!(zip_result.output_digest, tar_result.output_digest);
    assert_eq!(
        digest_for_bytes(&fs::read(zip_path).expect("zip should exist")),
        zip_result.output_digest
    );
    assert_eq!(
        digest_for_bytes(&fs::read(tar_path).expect("tar should exist")),
        tar_result.output_digest
    );

    let durable_after = fs::read(
        store
            .artifact_path(&artifact.identity)
            .expect("artifact path should be available"),
    )
    .expect("durable artifact state should be readable after materialization");
    assert_eq!(durable_before, durable_after);
}

#[test]
fn independent_store_instances_recover_without_hidden_in_memory_state() {
    let store_root = TempDir::new().expect("store root should be created");
    let identity = {
        let (source, built) = build_source_artifact(&[("independent.txt", "reader")]);
        let store = LocalArtifactStore::open(store_root.path()).expect("writer store should open");
        store
            .persist(built.artifact(), &built)
            .expect("artifact should persist");
        let identity = built.artifact().identity.clone();
        drop(store);
        drop(built);
        drop(source);
        identity
    };

    let reader_one = LocalArtifactStore::open(store_root.path()).expect("first reader should open");
    let reader_two =
        LocalArtifactStore::open(store_root.path()).expect("second reader should open");
    let recovered_one = reader_one
        .recover(&identity)
        .expect("first reader should recover artifact");
    let recovered_two = reader_two
        .recover(&identity)
        .expect("second reader should recover artifact");

    assert_same_semantics(recovered_one.artifact(), recovered_two.artifact());
    let mut content = String::new();
    recovered_two
        .open("independent.txt")
        .expect("second reader should resolve content")
        .read_to_string(&mut content)
        .expect("content should be readable");
    assert_eq!(content, "reader");
}
