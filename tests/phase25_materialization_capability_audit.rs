use artifact::{RecipeSpec, TarMaterializer, ZipMaterializer};
use std::fs;
use tempfile::TempDir;

fn build_fixture_artifact() -> (TempDir, artifact::SourceBackedArtifact) {
    let source_root = TempDir::new().expect("source root should be created");
    fs::create_dir_all(source_root.path().join("bin"))
        .expect("fixture bin directory should be created");
    fs::write(source_root.path().join("bin/app"), b"#!/usr/bin/env sh\necho phase25\n")
        .expect("fixture executable should be written");
    fs::write(source_root.path().join("README.txt"), b"phase25 fixture")
        .expect("fixture readme should be written");

    let pipeline = RecipeSpec::directory_zip()
        .compile()
        .expect("directory zip recipe should compile");
    let built = pipeline
        .build_from_directory(source_root.path())
        .expect("artifact should build from fixture directory");
    (source_root, built)
}

#[test]
fn same_artifact_has_stable_identity_across_outputs() {
    let (_source_root, built) = build_fixture_artifact();
    let artifact = built.artifact();
    let output_root = TempDir::new().expect("output root should be created");
    let zip_output = output_root.path().join("artifact.zip");
    let tar_output = output_root.path().join("artifact.tar");

    let zip_result = ZipMaterializer
        .materialize_to_path(artifact, &built, &zip_output)
        .expect("zip materialization should succeed");
    let tar_result = TarMaterializer
        .materialize_to_path(artifact, &built, &tar_output)
        .expect("tar materialization should succeed");

    assert_eq!(zip_result.artifact_identity, artifact.identity);
    assert_eq!(tar_result.artifact_identity, artifact.identity);
    assert_ne!(zip_result.output_digest, artifact.identity);
    assert_ne!(tar_result.output_digest, artifact.identity);
    assert_ne!(zip_result.output_digest, tar_result.output_digest);
}

#[test]
fn materialization_is_deterministic_per_output_type() {
    let (_source_root, built) = build_fixture_artifact();
    let artifact = built.artifact();

    let zip_first = ZipMaterializer
        .materialize_to_vec(artifact, &built)
        .expect("first zip materialization should succeed");
    let zip_second = ZipMaterializer
        .materialize_to_vec(artifact, &built)
        .expect("second zip materialization should succeed");
    assert_eq!(zip_first, zip_second);

    let tar_first = TarMaterializer
        .materialize_to_vec(artifact, &built)
        .expect("first tar materialization should succeed");
    let tar_second = TarMaterializer
        .materialize_to_vec(artifact, &built)
        .expect("second tar materialization should succeed");
    assert_eq!(tar_first, tar_second);
}

#[test]
fn artifact_model_remains_output_agnostic() {
    let core_source = include_str!("../src/core/mod.rs");
    let forbidden_output_typed_artifacts = [
        "ZipArtifact",
        "TarArtifact",
        "DockerArtifact",
        "WasmArtifact",
        "OciArtifact",
    ];

    for token in forbidden_output_typed_artifacts {
        assert!(
            !core_source.contains(token),
            "logical artifact model should not introduce output-specific artifact type: {token}"
        );
    }
}
