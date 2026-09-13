use artifact::{
    AllowAllPolicy, Artifact, ArtifactSDK, Capability, CompositionInput, CompositionOptions,
    LocalArtifactStore, MaterializerSpec, MemoryContentResolver, PipelineSpec, RecipeSpec,
    SelectStageSpec, SourceBackedArtifact, SourceSpec, StageSpec, TarMaterializer, ZipMaterializer,
};
use std::collections::BTreeMap;
use std::fs;
use tempfile::TempDir;

fn kernel_pipeline() -> PipelineSpec {
    PipelineSpec {
        source: SourceSpec::Directory,
        stages: vec![
            StageSpec::Select(
                SelectStageSpec::new(Vec::new(), Vec::new()).expect("empty selection is valid"),
            ),
            StageSpec::Manifest,
            StageSpec::Validate,
        ],
        materializer: MaterializerSpec::Zip,
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
    let built = kernel_pipeline()
        .build_from_directory(source.path())
        .expect("source artifact should build");
    (source, built)
}

fn compose_two_artifacts() -> (
    TempDir,
    SourceBackedArtifact,
    TempDir,
    SourceBackedArtifact,
    Artifact,
    MemoryContentResolver,
) {
    let (source_a, built_a) = build_source_artifact(&[("a.txt", "alpha")]);
    let (source_b, built_b) = build_source_artifact(&[("b.txt", "beta")]);
    let composed =
        CompositionInput::new(vec![built_a.artifact().clone(), built_b.artifact().clone()])
            .compose(CompositionOptions::default())
            .expect("composition should succeed");
    let resolver = memory_resolver(&[("a.txt", "alpha"), ("b.txt", "beta")]);

    (source_a, built_a, source_b, built_b, composed, resolver)
}

fn memory_resolver(contents: &[(&str, &str)]) -> MemoryContentResolver {
    MemoryContentResolver::new(
        contents
            .iter()
            .map(|(path, content)| (path.to_string(), content.as_bytes().to_vec()))
            .collect::<BTreeMap<_, _>>(),
    )
}

fn assert_same_artifact(left: &Artifact, right: &Artifact) {
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
fn independent_artifact_identity_survives_explicit_composition() {
    let (_source_a, built_a, _source_b, built_b, composed, _resolver) = compose_two_artifacts();
    let artifact_a = built_a.artifact().clone();
    let artifact_b = built_b.artifact().clone();
    let identity_a = artifact_a.identity.clone();
    let identity_b = artifact_b.identity.clone();

    assert_ne!(identity_a, identity_b);
    assert_eq!(artifact_a.identity, identity_a);
    assert_eq!(artifact_b.identity, identity_b);
    assert_eq!(built_a.artifact().identity, identity_a);
    assert_eq!(built_b.artifact().identity, identity_b);
    assert_ne!(composed.identity, identity_a);
    assert_ne!(composed.identity, identity_b);
    assert!(composed.provenance.source_identity.contains(&identity_a));
    assert!(composed.provenance.source_identity.contains(&identity_b));
}

#[test]
fn composition_result_is_a_deterministic_artifact_without_mutating_inputs() {
    let (_source_a, built_a) = build_source_artifact(&[("a.txt", "alpha")]);
    let (_source_b, built_b) = build_source_artifact(&[("b.txt", "beta")]);
    let original_a = built_a.artifact().clone();
    let original_b = built_b.artifact().clone();

    let first = CompositionInput::new(vec![original_a.clone(), original_b.clone()])
        .compose(CompositionOptions::default())
        .expect("first composition should succeed");
    let second =
        CompositionInput::new(vec![built_a.artifact().clone(), built_b.artifact().clone()])
            .compose(CompositionOptions::default())
            .expect("second composition should succeed");

    assert_same_artifact(&first, &second);
    assert_eq!(built_a.artifact().identity, original_a.identity);
    assert_eq!(built_b.artifact().identity, original_b.identity);
    assert_eq!(first.entries.len(), 2);
    assert!(first.entries.iter().any(|entry| entry.path == "a.txt"));
    assert!(first.entries.iter().any(|entry| entry.path == "b.txt"));
    assert!(first.lineage().is_empty());
    assert!(first.semantic_declaration.is_none());
}

#[test]
fn equivalent_ordered_composition_inputs_produce_equivalent_result_identity() {
    let (_source_a, built_a) = build_source_artifact(&[("a.txt", "alpha")]);
    let (_source_b, built_b) = build_source_artifact(&[("b.txt", "beta")]);
    let (_source_a2, built_a2) = build_source_artifact(&[("a.txt", "alpha")]);
    let (_source_b2, built_b2) = build_source_artifact(&[("b.txt", "beta")]);
    assert_eq!(built_a.artifact().identity, built_a2.artifact().identity);
    assert_eq!(built_b.artifact().identity, built_b2.artifact().identity);

    let composed =
        CompositionInput::new(vec![built_a.artifact().clone(), built_b.artifact().clone()])
            .compose(CompositionOptions::default())
            .expect("composition should succeed");
    let equivalent = CompositionInput::new(vec![
        built_a2.artifact().clone(),
        built_b2.artifact().clone(),
    ])
    .compose(CompositionOptions::default())
    .expect("equivalent composition should succeed");
    let reversed =
        CompositionInput::new(vec![built_b.artifact().clone(), built_a.artifact().clone()])
            .compose(CompositionOptions::default())
            .expect("reversed composition should succeed");

    assert_same_artifact(&composed, &equivalent);
    assert_ne!(
        composed.identity, reversed.identity,
        "CompositionInput is an ordered production input, so reversed input order is a different operation"
    );
}

#[test]
fn composed_results_persist_and_recover_without_creating_input_records() {
    let (_source_a, built_a, _source_b, built_b, composed, resolver) = compose_two_artifacts();
    let identity_a = built_a.artifact().identity.clone();
    let identity_b = built_b.artifact().identity.clone();
    let identity_c = composed.identity.clone();
    let store_root = TempDir::new().expect("store root should be created");
    let store = LocalArtifactStore::open(store_root.path()).expect("store should open");

    store
        .persist(&composed, &resolver)
        .expect("composed result should persist");
    let recovered = store
        .recover(&identity_c)
        .expect("composed result should recover");

    assert_same_artifact(&composed, recovered.artifact());
    assert_eq!(built_a.artifact().identity, identity_a);
    assert_eq!(built_b.artifact().identity, identity_b);
    assert!(
        store.recover(&identity_a).is_err(),
        "storing C must not create hidden registry records for A"
    );
    assert!(
        store.recover(&identity_b).is_err(),
        "storing C must not create hidden registry records for B"
    );
}

#[test]
fn storing_inputs_does_not_create_or_discover_a_composition_result() {
    let (_source_a, built_a, _source_b, built_b, composed, _resolver) = compose_two_artifacts();
    let store_root = TempDir::new().expect("store root should be created");
    let store = LocalArtifactStore::open(store_root.path()).expect("store should open");

    store
        .persist(built_a.artifact(), &built_a)
        .expect("A should persist");
    store
        .persist(built_b.artifact(), &built_b)
        .expect("B should persist");

    assert!(
        store.recover(&composed.identity).is_err(),
        "persisting A and B must not implicitly persist or discover C"
    );
}

#[test]
fn materialization_of_composed_result_does_not_mutate_logical_identities() {
    let (_source_a, built_a, _source_b, built_b, composed, resolver) = compose_two_artifacts();
    let identity_a = built_a.artifact().identity.clone();
    let identity_b = built_b.artifact().identity.clone();
    let identity_c = composed.identity.clone();

    let zip = ZipMaterializer
        .materialize_to_vec(&composed, &resolver)
        .expect("composed artifact should materialize to ZIP");
    let tar = TarMaterializer
        .materialize_to_vec(&composed, &resolver)
        .expect("composed artifact should materialize to TAR");

    assert!(!zip.is_empty());
    assert!(!tar.is_empty());
    assert_ne!(zip, tar);
    assert_eq!(built_a.artifact().identity, identity_a);
    assert_eq!(built_b.artifact().identity, identity_b);
    assert_eq!(composed.identity, identity_c);
}

#[test]
fn negative_boundaries_are_not_created_by_composition() {
    let (_source_a, built_a) = build_source_artifact(&[("a.txt", "alpha")]);
    let (_source_b, built_b) = build_source_artifact(&[("b.txt", "beta")]);
    let declared_a = built_a
        .artifact()
        .clone()
        .with_semantic_declaration("consumer:declared-a");
    let declared_b = built_b
        .artifact()
        .clone()
        .with_semantic_declaration("consumer:declared-b");
    let original_a = declared_a.identity.clone();
    let original_b = declared_b.identity.clone();

    let composed = CompositionInput::new(vec![declared_a, declared_b])
        .compose(CompositionOptions::default())
        .expect("composition should succeed");

    assert!(composed.lineage().is_empty());
    assert!(composed.semantic_declaration.is_none());
    assert_eq!(built_a.artifact().identity, original_a);
    assert_eq!(built_b.artifact().identity, original_b);
    assert_ne!(composed.provenance.source_identity, original_a);
    assert_ne!(composed.provenance.source_identity, original_b);

    let public_pipeline = ArtifactSDK::recipe_from_spec(RecipeSpec::directory_zip())
        .compile()
        .expect("compilation should succeed");
    let (_, evidence) = public_pipeline
        .build_with_authorization(
            source_tree(&[("evidence.txt", "evidence")]).path(),
            &AllowAllPolicy,
        )
        .expect("authorized production build should succeed");
    assert_ne!(
        composed.provenance.source_identity,
        evidence.as_evidence().artifact_identity,
        "composition participation is not execution evidence or authorization"
    );
}

#[test]
fn composition_requires_explicit_inputs_and_rejects_path_collisions() {
    let empty_error = CompositionInput::new(Vec::new())
        .compose(CompositionOptions::default())
        .expect_err("empty composition should be rejected");
    assert!(empty_error.to_string().contains("at least one artifact"));

    let (_source_a, built_a) = build_source_artifact(&[("same.txt", "alpha")]);
    let (_source_b, built_b) = build_source_artifact(&[("same.txt", "beta")]);
    let collision_error =
        CompositionInput::new(vec![built_a.artifact().clone(), built_b.artifact().clone()])
            .compose(CompositionOptions::default())
            .expect_err("colliding paths should be rejected");

    assert!(
        collision_error
            .to_string()
            .contains("collision in composition")
    );
}
