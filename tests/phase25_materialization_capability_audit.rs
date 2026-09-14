use artifact::{
    Artifact, ArtifactEntry, CreationMetadata, EntryType, MemoryContentResolver, Provenance,
    TarMaterializer, ZipMaterializer, core::sha256_prefixed,
};
use std::collections::BTreeMap;
use std::fs;
use tempfile::TempDir;

fn build_logical_fixture() -> (Artifact, MemoryContentResolver) {
    let mut entries = BTreeMap::new();
    entries.insert(
        "README.txt".to_string(),
        b"phase25 logical fixture".to_vec(),
    );
    entries.insert(
        "bin/app".to_string(),
        b"#!/usr/bin/env sh\necho phase25\n".to_vec(),
    );

    let artifact_entries: Vec<ArtifactEntry> = entries
        .iter()
        .map(|(path, content)| ArtifactEntry {
            path: path.clone(),
            entry_type: EntryType::File,
            size: content.len() as u64,
            content_digest: sha256_prefixed(content),
        })
        .collect();

    let pipeline_identity = sha256_prefixed(b"phase25a_pipeline");
    let source_identity = sha256_prefixed(b"phase25a_source");
    let artifact = Artifact::from_parts(
        artifact_entries,
        pipeline_identity.clone(),
        Vec::new(),
        Provenance {
            source_identity,
            pipeline_identity,
            creation_metadata: CreationMetadata::default(),
        },
    )
    .expect("logical artifact fixture should build");

    (artifact, MemoryContentResolver::new(entries))
}

#[test]
fn same_artifact_has_stable_identity_across_outputs() {
    let (artifact, resolver) = build_logical_fixture();
    let output_root = TempDir::new().expect("output root should be created");
    let zip_output = output_root.path().join("artifact.zip");
    let tar_output = output_root.path().join("artifact.tar");

    let zip_result = ZipMaterializer
        .materialize_to_path(&artifact, &resolver, &zip_output)
        .expect("zip materialization should succeed");
    let tar_result = TarMaterializer
        .materialize_to_path(&artifact, &resolver, &tar_output)
        .expect("tar materialization should succeed");

    assert_eq!(zip_result.artifact_identity, artifact.identity);
    assert_eq!(tar_result.artifact_identity, artifact.identity);
    assert_ne!(zip_result.output_digest, artifact.identity);
    assert_ne!(tar_result.output_digest, artifact.identity);
    assert_ne!(zip_result.output_digest, tar_result.output_digest);
}

#[test]
fn materialization_is_deterministic_per_output_type() {
    let (artifact, resolver) = build_logical_fixture();
    let output_root = TempDir::new().expect("output root should be created");
    let zip_output_first = output_root.path().join("artifact-1.zip");
    let zip_output_second = output_root.path().join("artifact-2.zip");
    let tar_output_first = output_root.path().join("artifact-1.tar");
    let tar_output_second = output_root.path().join("artifact-2.tar");

    let zip_first = ZipMaterializer
        .materialize_to_vec(&artifact, &resolver)
        .expect("first zip materialization should succeed");
    let zip_second = ZipMaterializer
        .materialize_to_vec(&artifact, &resolver)
        .expect("second zip materialization should succeed");
    assert_eq!(sha256_prefixed(&zip_first), sha256_prefixed(&zip_second));

    let tar_first = TarMaterializer
        .materialize_to_vec(&artifact, &resolver)
        .expect("first tar materialization should succeed");
    let tar_second = TarMaterializer
        .materialize_to_vec(&artifact, &resolver)
        .expect("second tar materialization should succeed");
    assert_eq!(sha256_prefixed(&tar_first), sha256_prefixed(&tar_second));

    let zip_path_result = ZipMaterializer
        .materialize_to_path(&artifact, &resolver, &zip_output_first)
        .expect("first zip path materialization should succeed");
    let zip_path_result_second = ZipMaterializer
        .materialize_to_path(&artifact, &resolver, &zip_output_second)
        .expect("second zip path materialization should succeed");
    let tar_path_result = TarMaterializer
        .materialize_to_path(&artifact, &resolver, &tar_output_first)
        .expect("first tar path materialization should succeed");
    let tar_path_result_second = TarMaterializer
        .materialize_to_path(&artifact, &resolver, &tar_output_second)
        .expect("second tar path materialization should succeed");
    let zip_path_bytes =
        fs::read(&zip_output_first).expect("first zip path bytes should be readable");
    let zip_path_bytes_second =
        fs::read(&zip_output_second).expect("second zip path bytes should be readable");
    let tar_path_bytes =
        fs::read(&tar_output_first).expect("first tar path bytes should be readable");
    let tar_path_bytes_second =
        fs::read(&tar_output_second).expect("second tar path bytes should be readable");
    assert_eq!(
        zip_path_result.output_digest,
        sha256_prefixed(&zip_path_bytes)
    );
    assert_eq!(
        tar_path_result.output_digest,
        sha256_prefixed(&tar_path_bytes)
    );
    assert_eq!(
        zip_path_result.output_digest,
        zip_path_result_second.output_digest
    );
    assert_eq!(
        tar_path_result.output_digest,
        tar_path_result_second.output_digest
    );
    assert_eq!(zip_path_bytes, zip_path_bytes_second);
    assert_eq!(tar_path_bytes, tar_path_bytes_second);
}

#[test]
fn materialization_result_is_format_specific_identity_link() {
    let (artifact, resolver) = build_logical_fixture();
    let output_root = TempDir::new().expect("output root should be created");
    let zip_output = output_root.path().join("artifact.zip");
    let tar_output = output_root.path().join("artifact.tar");
    let zip_vec = ZipMaterializer
        .materialize_to_vec(&artifact, &resolver)
        .expect("zip vec materialization should succeed");
    let tar_vec = TarMaterializer
        .materialize_to_vec(&artifact, &resolver)
        .expect("tar vec materialization should succeed");

    let zip_result = ZipMaterializer
        .materialize_to_path(&artifact, &resolver, &zip_output)
        .expect("zip materialization should succeed");
    let tar_result = TarMaterializer
        .materialize_to_path(&artifact, &resolver, &tar_output)
        .expect("tar materialization should succeed");

    assert_eq!(zip_result.materializer_format, "zip");
    assert_eq!(tar_result.materializer_format, "tar");
    assert_eq!(zip_result.artifact_identity, artifact.identity);
    assert_eq!(tar_result.artifact_identity, artifact.identity);

    let zip_bytes = fs::read(&zip_output).expect("zip bytes should be readable");
    let tar_bytes = fs::read(&tar_output).expect("tar bytes should be readable");
    assert_eq!(zip_result.output_digest, sha256_prefixed(&zip_bytes));
    assert_eq!(tar_result.output_digest, sha256_prefixed(&tar_bytes));
    assert_ne!(zip_result.output_digest, tar_result.output_digest);
    assert_ne!(sha256_prefixed(&zip_vec), sha256_prefixed(&tar_vec));
}
