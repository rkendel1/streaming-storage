use artifact::{
    Artifact, ArtifactEntry, Capability, ContentResolver, CreationMetadata, EntryType,
    MaterializationResult, MemoryContentResolver, PipelineSpec, Provenance, SelectStageSpec,
    SourceSpec, StageSpec, TarMaterializer, ZipMaterializer, default_directory_zip_pipeline,
    normalize_relative_path,
};
use sha2::Digest;
use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Read};
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn pipeline_stages_execute_in_declared_order() {
    let built = default_directory_zip_pipeline()
        .build_from_directory(example_dir())
        .expect("pipeline should build example");

    assert_eq!(built.stage_trace(), ["select", "manifest", "validate"]);
}

#[test]
fn selection_includes_safe_content_and_excludes_secrets_and_dependencies() {
    let built = default_directory_zip_pipeline()
        .build_from_directory(example_dir())
        .expect("pipeline should build example");
    let paths = built
        .artifact()
        .entries
        .iter()
        .map(|entry| entry.path.as_str())
        .collect::<Vec<_>>();

    assert_eq!(paths, vec!["README.md", "app/app.js", "app/index.html"]);
}

#[test]
fn path_safety_rejects_traversal_and_absolute_paths() {
    for invalid in [
        "../foo",
        "../../foo",
        "/absolute/path",
        "foo/../bar",
        r"C:\temp\evil",
    ] {
        assert!(
            normalize_relative_path(invalid).is_err(),
            "{invalid} should be rejected"
        );
    }
}

#[test]
fn canonicalization_ignores_filesystem_creation_order() {
    let first = fixture_dir(&[("b.txt", "bravo"), ("a.txt", "alpha")]);
    let second = fixture_dir(&[("a.txt", "alpha"), ("b.txt", "bravo")]);
    let pipeline = default_directory_zip_pipeline();

    let first_id = pipeline
        .build_from_directory(first.path())
        .expect("first artifact should build")
        .artifact()
        .identity
        .clone();
    let second_id = pipeline
        .build_from_directory(second.path())
        .expect("second artifact should build")
        .artifact()
        .identity
        .clone();

    assert_eq!(first_id, second_id);
}

#[test]
fn manifest_serialization_is_deterministic() {
    let built = default_directory_zip_pipeline()
        .build_from_directory(example_dir())
        .expect("pipeline should build example");

    let first = built
        .artifact()
        .manifest
        .to_canonical_json()
        .expect("manifest serialization should succeed");
    let second = built
        .artifact()
        .manifest
        .to_canonical_json()
        .expect("manifest serialization should succeed");

    assert_eq!(first, second);
}

#[test]
fn pipeline_identity_is_deterministic_for_equivalent_pipelines() {
    let left = PipelineSpec {
        source: SourceSpec::Directory,
        stages: vec![
            StageSpec::Select(
                SelectStageSpec::new(
                    vec![".env".into()],
                    vec!["node_modules".into(), "cache".into()],
                )
                .expect("selection should be valid"),
            ),
            StageSpec::Manifest,
            StageSpec::Validate,
        ],
        materializer: artifact::MaterializerSpec::Zip,
        capabilities: vec![
            Capability::new("package.zip", "1"),
            Capability::new("filesystem.read", "1"),
            Capability::new("artifact.validate", "1"),
            Capability::new("manifest.generate", "1"),
        ],
    };
    let right = PipelineSpec {
        source: SourceSpec::Directory,
        stages: vec![
            StageSpec::Select(
                SelectStageSpec::new(
                    vec![".env".into()],
                    vec!["cache".into(), "node_modules".into()],
                )
                .expect("selection should be valid"),
            ),
            StageSpec::Manifest,
            StageSpec::Validate,
        ],
        materializer: artifact::MaterializerSpec::Zip,
        capabilities: vec![
            Capability::new("manifest.generate", "1"),
            Capability::new("artifact.validate", "1"),
            Capability::new("filesystem.read", "1"),
            Capability::new("package.zip", "1"),
        ],
    };

    assert_eq!(
        left.identity().expect("left id"),
        right.identity().expect("right id")
    );
}

#[test]
fn duplicate_manifest_stages_are_rejected() {
    let pipeline = PipelineSpec {
        source: SourceSpec::Directory,
        stages: vec![
            StageSpec::Select(
                SelectStageSpec::new(vec![".env".into()], vec!["node_modules".into()])
                    .expect("selection should be valid"),
            ),
            StageSpec::Manifest,
            StageSpec::Manifest,
            StageSpec::Validate,
        ],
        materializer: artifact::MaterializerSpec::Zip,
        capabilities: vec![
            Capability::new("filesystem.read", "1"),
            Capability::new("manifest.generate", "1"),
            Capability::new("artifact.validate", "1"),
            Capability::new("package.zip", "1"),
        ],
    };

    let error = pipeline
        .build_from_directory(example_dir())
        .expect_err("duplicate manifest stages should fail");

    assert!(
        error
            .to_string()
            .contains("manifest stage may only appear once")
    );
}

#[test]
fn artifact_identity_is_deterministic_for_same_source_and_pipeline() {
    let source = fixture_dir(&[("app/index.html", "<html></html>"), ("README.md", "hi")]);
    let pipeline = default_directory_zip_pipeline();

    let first = pipeline
        .build_from_directory(source.path())
        .expect("first build should succeed")
        .artifact()
        .identity
        .clone();
    let second = pipeline
        .build_from_directory(source.path())
        .expect("second build should succeed")
        .artifact()
        .identity
        .clone();

    assert_eq!(first, second);
}

#[test]
fn zip_output_is_deterministic_for_same_artifact() {
    let built = default_directory_zip_pipeline()
        .build_from_directory(example_dir())
        .expect("pipeline should build example");

    let first = ZipMaterializer
        .materialize_to_vec(built.artifact(), &built)
        .expect("first zip should build");
    let second = ZipMaterializer
        .materialize_to_vec(built.artifact(), &built)
        .expect("second zip should build");

    assert_eq!(first, second);
}

#[test]
fn zip_output_is_deterministic_across_equivalent_builds() {
    let pipeline = default_directory_zip_pipeline();

    let first = pipeline
        .build_from_directory(example_dir())
        .expect("first artifact should build");
    let second = pipeline
        .build_from_directory(example_dir())
        .expect("second artifact should build");

    let first_zip = ZipMaterializer
        .materialize_to_vec(first.artifact(), &first)
        .expect("first zip should build");
    let second_zip = ZipMaterializer
        .materialize_to_vec(second.artifact(), &second)
        .expect("second zip should build");

    assert_eq!(first_zip, second_zip);
}

#[test]
fn zip_materialization_reports_digest_without_rereading_output() {
    let built = default_directory_zip_pipeline()
        .build_from_directory(example_dir())
        .expect("pipeline should build example");
    let output_dir = TempDir::new().expect("temp dir should be created");
    let output_path = output_dir.path().join("example.zip");

    let report = ZipMaterializer
        .materialize_to_path(built.artifact(), &built, &output_path)
        .expect("zip should materialize to disk");

    let bytes = fs::read(&output_path).expect("zip bytes should be readable");
    assert_eq!(
        report,
        MaterializationResult {
            artifact_identity: built.artifact().identity.clone(),
            materializer_format: "zip".to_string(),
            output_digest: digest_for_bytes(&bytes),
            size_bytes: bytes.len() as u64,
        }
    );
}

#[test]
fn logical_artifact_is_inspectable_without_zip_materialization() {
    let built = default_directory_zip_pipeline()
        .build_from_directory(example_dir())
        .expect("pipeline should build example");

    assert_eq!(built.artifact().manifest.manifest_version, 1);
    assert_eq!(built.artifact().entries.len(), 3);
}

#[test]
fn artifact_rejects_conflicting_entries() {
    let error = Artifact::from_parts(
        vec![
            ArtifactEntry {
                path: "app".to_string(),
                entry_type: EntryType::File,
                size: 1,
                content_digest: digest_for("a"),
            },
            ArtifactEntry {
                path: "app/index.html".to_string(),
                entry_type: EntryType::File,
                size: 1,
                content_digest: digest_for("b"),
            },
        ],
        "sha256:pipeline".to_string(),
        vec![Capability::new("package.zip", "1")],
        Provenance {
            source_identity: "sha256:source".to_string(),
            pipeline_identity: "sha256:pipeline".to_string(),
            creation_metadata: CreationMetadata::default(),
        },
    )
    .expect_err("conflicting file and directory paths should be rejected");

    assert!(error.to_string().contains("conflicting artifact paths"));
}

#[test]
fn materialization_rejects_content_drift() {
    let artifact = Artifact::from_parts(
        vec![ArtifactEntry {
            path: "app.js".to_string(),
            entry_type: EntryType::File,
            size: 5,
            content_digest: digest_for("hello"),
        }],
        "sha256:pipeline".to_string(),
        vec![Capability::new("package.zip", "1")],
        Provenance {
            source_identity: "sha256:source".to_string(),
            pipeline_identity: "sha256:pipeline".to_string(),
            creation_metadata: CreationMetadata::default(),
        },
    )
    .expect("artifact should be valid");

    let resolver = StaticResolver::new([("app.js", "jello")]);
    let error = ZipMaterializer
        .materialize_to_vec(&artifact, &resolver)
        .expect_err("materialization should reject mismatched content");

    assert!(error.to_string().contains("content digest drifted"));
}

#[cfg(unix)]
#[test]
fn symlinks_are_rejected() {
    let dir = TempDir::new().expect("temp dir should be created");
    let target = dir.path().join("target.txt");
    let link = dir.path().join("linked.txt");
    fs::write(&target, "safe").expect("target should be written");
    std::os::unix::fs::symlink(&target, &link).expect("symlink should be created");

    let error = default_directory_zip_pipeline()
        .build_from_directory(dir.path())
        .expect_err("symlink input should be rejected");

    assert!(error.to_string().contains("symlinks are rejected"));
}

#[test]
fn format_independence_same_artifact_produces_different_digests() {
    let built = default_directory_zip_pipeline()
        .build_from_directory(example_dir())
        .expect("pipeline should build example");
    let artifact = built.artifact();

    let zip_result = ZipMaterializer
        .materialize_to_vec(artifact, &built)
        .expect("zip materialization should succeed");
    let tar_result = TarMaterializer
        .materialize_to_vec(artifact, &built)
        .expect("tar materialization should succeed");

    assert_ne!(
        zip_result, tar_result,
        "ZIP and TAR should produce different bytes"
    );
    let zip_digest = digest_for_bytes(&zip_result);
    let tar_digest = digest_for_bytes(&tar_result);
    assert_ne!(zip_digest, tar_digest, "ZIP and TAR should have different digests");
}

#[test]
fn format_independence_artifact_identity_is_shared() {
    let built = default_directory_zip_pipeline()
        .build_from_directory(example_dir())
        .expect("pipeline should build example");
    let artifact = built.artifact();
    let artifact_identity = artifact.identity.clone();

    let output_dir = TempDir::new().expect("temp dir should be created");
    let zip_path = output_dir.path().join("example.zip");
    let tar_path = output_dir.path().join("example.tar");

    let zip_result = ZipMaterializer
        .materialize_to_path(artifact, &built, &zip_path)
        .expect("zip should materialize");
    let tar_result = TarMaterializer
        .materialize_to_path(artifact, &built, &tar_path)
        .expect("tar should materialize");

    assert_eq!(zip_result.artifact_identity, artifact_identity);
    assert_eq!(tar_result.artifact_identity, artifact_identity);
    assert_eq!(
        zip_result.artifact_identity, tar_result.artifact_identity,
        "both materializations should have same logical artifact identity"
    );
    assert_ne!(
        zip_result.output_digest, tar_result.output_digest,
        "physical representations should have different digests"
    );
}

#[test]
fn memory_content_resolver_enables_source_independence() {
    let built = default_directory_zip_pipeline()
        .build_from_directory(example_dir())
        .expect("pipeline should build example");
    let artifact = built.artifact().clone();

    let mut contents = BTreeMap::new();
    for entry in &artifact.entries {
        let path = entry.path.as_str();
        let bytes = fs::read(example_dir().join(path)).expect("source file should be readable");
        contents.insert(path.to_string(), bytes);
    }

    let memory_resolver = MemoryContentResolver::new(contents);
    let zip_from_memory = ZipMaterializer
        .materialize_to_vec(&artifact, &memory_resolver)
        .expect("memory materialization should succeed");

    let zip_from_source = ZipMaterializer
        .materialize_to_vec(&artifact, &built)
        .expect("source materialization should succeed");

    assert_eq!(zip_from_memory, zip_from_source);
}

#[test]
fn tar_materialization_produces_deterministic_output() {
    let built = default_directory_zip_pipeline()
        .build_from_directory(example_dir())
        .expect("pipeline should build example");
    let artifact = built.artifact();

    let first = TarMaterializer
        .materialize_to_vec(artifact, &built)
        .expect("first tar should build");
    let second = TarMaterializer
        .materialize_to_vec(artifact, &built)
        .expect("second tar should build");

    assert_eq!(first, second, "TAR output should be deterministic");
}

#[test]
fn tar_materialization_to_path_returns_correct_result() {
    let built = default_directory_zip_pipeline()
        .build_from_directory(example_dir())
        .expect("pipeline should build example");
    let output_dir = TempDir::new().expect("temp dir should be created");
    let output_path = output_dir.path().join("example.tar");

    let result = TarMaterializer
        .materialize_to_path(built.artifact(), &built, &output_path)
        .expect("tar should materialize to disk");

    assert_eq!(result.materializer_format, "tar");
    assert_eq!(result.artifact_identity, built.artifact().identity);
    let bytes = fs::read(&output_path).expect("tar bytes should be readable");
    assert_eq!(result.output_digest, digest_for_bytes(&bytes));
    assert_eq!(result.size_bytes, bytes.len() as u64);
}

#[test]
fn provenance_changes_do_not_alter_artifact_identity() {
    let artifact = Artifact::from_parts(
        vec![ArtifactEntry {
            path: "app.js".to_string(),
            entry_type: EntryType::File,
            size: 5,
            content_digest: digest_for("hello"),
        }],
        "sha256:pipeline".to_string(),
        vec![Capability::new("package.zip", "1")],
        Provenance {
            source_identity: "sha256:source".to_string(),
            pipeline_identity: "sha256:pipeline".to_string(),
            creation_metadata: CreationMetadata::default(),
        },
    )
    .expect("artifact should be valid");

    let artifact_with_timestamp = Artifact::from_parts(
        vec![ArtifactEntry {
            path: "app.js".to_string(),
            entry_type: EntryType::File,
            size: 5,
            content_digest: digest_for("hello"),
        }],
        "sha256:pipeline".to_string(),
        vec![Capability::new("package.zip", "1")],
        Provenance {
            source_identity: "sha256:source".to_string(),
            pipeline_identity: "sha256:pipeline".to_string(),
            creation_metadata: CreationMetadata {
                created_at: Some("2026-09-13T00:00:00Z".to_string()),
            },
        },
    )
    .expect("artifact with timestamp should be valid");

    assert_eq!(
        artifact.identity, artifact_with_timestamp.identity,
        "artifact identity should not change with creation_metadata"
    );
}

fn example_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("example")
}

fn fixture_dir(entries: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new().expect("temp dir should be created");
    for (path, contents) in entries {
        let absolute = dir.path().join(path);
        if let Some(parent) = absolute.parent() {
            fs::create_dir_all(parent).expect("parent directories should be created");
        }
        fs::write(&absolute, contents).expect("fixture file should be written");
    }
    dir
}

fn digest_for(value: &str) -> String {
    digest_for_bytes(value.as_bytes())
}

fn digest_for_bytes(value: &[u8]) -> String {
    let mut hasher = sha2::Sha256::new();
    sha2::Digest::update(&mut hasher, value);
    let digest = sha2::Digest::finalize(hasher);
    let mut encoded = String::from("sha256:");
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

struct StaticResolver {
    entries: std::collections::BTreeMap<String, Vec<u8>>,
}

impl StaticResolver {
    fn new(entries: [(&str, &str); 1]) -> Self {
        Self {
            entries: entries
                .into_iter()
                .map(|(path, value)| (path.to_string(), value.as_bytes().to_vec()))
                .collect(),
        }
    }
}

impl ContentResolver for StaticResolver {
    fn resolve(&self, path: &str) -> Result<Box<dyn Read>, artifact::ArtifactError> {
        let bytes = self
            .entries
            .get(path)
            .cloned()
            .expect("resolver entry should exist");
        Ok(Box::new(Cursor::new(bytes)))
    }
}
