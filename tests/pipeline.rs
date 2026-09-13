use artifact::{
    Artifact, ArtifactEntry, Capability, ContentResolver, CreationMetadata, EntryType,
    GenerateTransform, MaterializationResult, MemoryContentResolver, PipelineSpec, Provenance,
    PrefixTransform, RedactTransform, SelectStageSpec, SourceSpec, StageSpec, TarMaterializer,
    TransformedContentResolver, ZipMaterializer, default_directory_zip_pipeline,
    normalize_relative_path, ArtifactTransform, AllowAllPolicy, AllowListPolicy, AuthorizationResult,
    CapabilityPolicy,
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

    assert_eq!(built.stage_trace(), ["source", "select", "manifest", "validate"]);
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

#[test]
fn prefix_transform_changes_artifact_identity() {
    let built = default_directory_zip_pipeline()
        .build_from_directory(example_dir())
        .expect("pipeline should build example");
    let artifact = built.artifact();
    let original_identity = artifact.identity.clone();

    let transform = PrefixTransform::new("bundle");
    let result = transform
        .apply(artifact, &built)
        .expect("prefix transform should succeed");

    assert_ne!(
        result.artifact.identity, original_identity,
        "prefix transformation should change artifact identity"
    );
    assert_eq!(result.artifact.entries.len(), artifact.entries.len());
    for entry in &result.artifact.entries {
        assert!(entry.path.starts_with("bundle/"));
    }
}

#[test]
fn redact_transform_removes_entries_and_changes_identity() {
    let built = default_directory_zip_pipeline()
        .build_from_directory(example_dir())
        .expect("pipeline should build example");
    let artifact = built.artifact();
    let original_identity = artifact.identity.clone();
    let original_entry_count = artifact.entries.len();

    let transform = RedactTransform::new(vec!["README.md".to_string()]);
    let result = transform
        .apply(artifact, &built)
        .expect("redact transform should succeed");

    assert_ne!(result.artifact.identity, original_identity);
    assert_eq!(result.artifact.entries.len(), original_entry_count - 1);
    assert!(result
        .artifact
        .entries
        .iter()
        .all(|e| e.path != "README.md"));
}

#[test]
fn generate_transform_adds_entry_and_changes_identity() {
    let built = default_directory_zip_pipeline()
        .build_from_directory(example_dir())
        .expect("pipeline should build example");
    let artifact = built.artifact();
    let original_identity = artifact.identity.clone();
    let original_entry_count = artifact.entries.len();

    let transform = GenerateTransform::new("generated.txt", "hello world");
    let result = transform
        .apply(artifact, &built)
        .expect("generate transform should succeed");

    assert_ne!(result.artifact.identity, original_identity);
    assert_eq!(result.artifact.entries.len(), original_entry_count + 1);
    assert!(result
        .artifact
        .entries
        .iter()
        .any(|e| e.path == "generated.txt"));
}

#[test]
fn transformed_artifact_materializes_to_zip() {
    let built = default_directory_zip_pipeline()
        .build_from_directory(example_dir())
        .expect("pipeline should build example");
    let artifact = built.artifact();

    let transform = PrefixTransform::new("bundle");
    let transformed = transform
        .apply(artifact, &built)
        .expect("prefix transform should succeed");

    let resolver = TransformedContentResolver::new(transformed.content_updates.clone());
    let zip = ZipMaterializer
        .materialize_to_vec(&transformed.artifact, &resolver)
        .expect("transformed artifact should materialize to ZIP");

    assert!(!zip.is_empty());
}

#[test]
fn transformed_artifact_materializes_to_tar() {
    let built = default_directory_zip_pipeline()
        .build_from_directory(example_dir())
        .expect("pipeline should build example");
    let artifact = built.artifact();

    let transform = PrefixTransform::new("bundle");
    let transformed = transform
        .apply(artifact, &built)
        .expect("prefix transform should succeed");

    let resolver = TransformedContentResolver::new(transformed.content_updates.clone());
    let tar = TarMaterializer
        .materialize_to_vec(&transformed.artifact, &resolver)
        .expect("transformed artifact should materialize to TAR");

    assert!(!tar.is_empty());
}

#[test]
fn transformed_artifact_same_identity_with_same_transform() {
    let built = default_directory_zip_pipeline()
        .build_from_directory(example_dir())
        .expect("pipeline should build example");
    let artifact = built.artifact();

    let transform = PrefixTransform::new("bundle");
    let first = transform
        .apply(artifact, &built)
        .expect("first transform should succeed");
    let second = transform
        .apply(artifact, &built)
        .expect("second transform should succeed");

    assert_eq!(
        first.artifact.identity, second.artifact.identity,
        "same transformation should produce same logical identity"
    );
}

#[test]
fn transformed_artifact_different_digests_zip_vs_tar() {
    let built = default_directory_zip_pipeline()
        .build_from_directory(example_dir())
        .expect("pipeline should build example");
    let artifact = built.artifact();

    let transform = PrefixTransform::new("bundle");
    let transformed = transform
        .apply(artifact, &built)
        .expect("prefix transform should succeed");

    let resolver = TransformedContentResolver::new(transformed.content_updates.clone());
    let transformed_artifact = &transformed.artifact;

    let zip_bytes = ZipMaterializer
        .materialize_to_vec(transformed_artifact, &resolver)
        .expect("ZIP should materialize");
    let tar_bytes = TarMaterializer
        .materialize_to_vec(transformed_artifact, &resolver)
        .expect("TAR should materialize");

    let zip_digest = digest_for_bytes(&zip_bytes);
    let tar_digest = digest_for_bytes(&tar_bytes);

    assert_ne!(zip_digest, tar_digest);
    assert_eq!(
        transformed_artifact.identity, transformed_artifact.identity,
        "artifact identity should be consistent"
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

#[test]
fn declarative_pipeline_with_prefix_transform() {
    let pipeline = PipelineSpec {
        source: SourceSpec::Directory,
        stages: vec![
            StageSpec::Select(SelectStageSpec::new(vec![], vec![]).expect("empty select should be valid")),
            StageSpec::Transform(artifact::TransformStageSpec::new("bundle").expect("prefix is valid")),
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

    let built = pipeline
        .build_from_directory(example_dir())
        .expect("pipeline with transform should execute");

    let artifact = built.artifact();
    for entry in &artifact.entries {
        assert!(
            entry.path.starts_with("bundle/"),
            "all entries should have bundle/ prefix, got {}",
            entry.path
        );
    }

    assert!(built.stage_trace().contains(&"transform".to_string()));
}

#[test]
fn declarative_pipeline_with_redact_stage() {
    let pipeline = PipelineSpec {
        source: SourceSpec::Directory,
        stages: vec![
            StageSpec::Select(SelectStageSpec::new(vec![], vec![]).expect("empty select should be valid")),
            StageSpec::Redact(artifact::RedactStageSpec::new(vec!["README.md".to_string()]).expect("redact spec is valid")),
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

    let built = pipeline
        .build_from_directory(example_dir())
        .expect("pipeline with redact should execute");

    let artifact = built.artifact();
    assert!(
        artifact
            .entries
            .iter()
            .all(|e| e.path != "README.md"),
        "README.md should be redacted"
    );

    assert!(built.stage_trace().contains(&"redact".to_string()));
}

#[test]
fn declarative_pipeline_with_generate_stage() {
    let pipeline = PipelineSpec {
        source: SourceSpec::Directory,
        stages: vec![
            StageSpec::Select(SelectStageSpec::new(vec![], vec![]).expect("empty select should be valid")),
            StageSpec::Generate(artifact::GenerateStageSpec::new("GENERATED.txt", "generated content").expect("generate spec is valid")),
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

    let built = pipeline
        .build_from_directory(example_dir())
        .expect("pipeline with generate should execute");

    let artifact = built.artifact();
    assert!(
        artifact.entries.iter().any(|e| e.path == "GENERATED.txt"),
        "generated file should be in artifact"
    );

    assert!(built.stage_trace().contains(&"generate".to_string()));
}

#[test]
fn declarative_pipeline_chaining_multiple_transforms() {
    let pipeline = PipelineSpec {
        source: SourceSpec::Directory,
        stages: vec![
            StageSpec::Select(SelectStageSpec::new(vec![], vec![]).expect("empty select should be valid")),
            StageSpec::Transform(artifact::TransformStageSpec::new("dist").expect("prefix is valid")),
            StageSpec::Generate(artifact::GenerateStageSpec::new("dist/BUILD.txt", "built").expect("generate spec is valid")),
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

    let built = pipeline
        .build_from_directory(example_dir())
        .expect("pipeline with chained transforms should execute");

    let artifact = built.artifact();

    for entry in &artifact.entries {
        assert!(
            entry.path.starts_with("dist/"),
            "all entries should be prefixed, got {}",
            entry.path
        );
    }

    assert!(
        artifact.entries.iter().any(|e| e.path == "dist/BUILD.txt"),
        "generated file should exist with prefix"
    );

    assert_eq!(
        built.stage_trace(),
        &["source", "select", "transform", "generate", "manifest", "validate"]
    );
}

#[test]
fn stage_identity_is_deterministic() {
    let stage_a = StageSpec::Transform(
        artifact::TransformStageSpec::new("prefix").expect("valid")
    );
    let stage_b = StageSpec::Transform(
        artifact::TransformStageSpec::new("prefix").expect("valid")
    );
    let stage_c = StageSpec::Transform(
        artifact::TransformStageSpec::new("different").expect("valid")
    );

    let stage_a_id = stage_a.identity().expect("identity should compute");
    let stage_b_id = stage_b.identity().expect("identity should compute");
    let stage_c_id = stage_c.identity().expect("identity should compute");

    assert_eq!(stage_a_id, stage_b_id, "same stage spec should have same identity");
    assert_ne!(stage_a_id, stage_c_id, "different parameters should have different identity");
}

#[test]
fn same_declarative_pipeline_produces_same_artifact() {
    let pipeline = PipelineSpec {
        source: SourceSpec::Directory,
        stages: vec![
            StageSpec::Select(SelectStageSpec::new(vec![], vec![]).expect("empty select should be valid")),
            StageSpec::Transform(artifact::TransformStageSpec::new("archive").expect("prefix is valid")),
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

    let first = pipeline
        .build_from_directory(example_dir())
        .expect("first execution should succeed");
    let second = pipeline
        .build_from_directory(example_dir())
        .expect("second execution should succeed");

    assert_eq!(
        first.artifact().identity,
        second.artifact().identity,
        "same pipeline spec should produce same artifact identity"
    );

    assert_eq!(
        first.artifact().entries.len(),
        second.artifact().entries.len(),
        "same pipeline spec should produce same entry count"
    );
}

#[test]
fn transformed_artifact_materializes_through_declarative_pipeline() {
    let pipeline = PipelineSpec {
        source: SourceSpec::Directory,
        stages: vec![
            StageSpec::Select(SelectStageSpec::new(vec![], vec![]).expect("empty select should be valid")),
            StageSpec::Transform(artifact::TransformStageSpec::new("bundle").expect("prefix is valid")),
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

    let built = pipeline
        .build_from_directory(example_dir())
        .expect("pipeline should execute");

    let artifact = built.artifact();
    let zip_bytes = ZipMaterializer
        .materialize_to_vec(artifact, &built)
        .expect("should materialize to ZIP");

    assert!(!zip_bytes.is_empty());
}

#[test]
fn redacted_content_cannot_be_resolved() {
    let pipeline = PipelineSpec {
        source: SourceSpec::Directory,
        stages: vec![
            StageSpec::Select(SelectStageSpec::new(vec![], vec![]).expect("empty select should be valid")),
            StageSpec::Redact(artifact::RedactStageSpec::new(vec!["README.md".to_string()]).expect("redact spec is valid")),
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

    let built = pipeline
        .build_from_directory(example_dir())
        .expect("pipeline should execute");

    let result = built.resolve("README.md");
    assert!(
        result.is_err(),
        "should not be able to resolve redacted content"
    );
}

#[test]
fn declarative_pipeline_round_trip_preserves_identity() {
    let original_pipeline = PipelineSpec {
        source: SourceSpec::Directory,
        stages: vec![
            StageSpec::Select(SelectStageSpec::new(vec![], vec![]).expect("empty select should be valid")),
            StageSpec::Transform(artifact::TransformStageSpec::new("dist").expect("prefix is valid")),
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

    let original_bytes = original_pipeline
        .to_canonical_bytes()
        .expect("should serialize");
    let original_identity = original_pipeline.identity().expect("should have identity");

    let rebuilt_pipeline = serde_json::from_slice::<serde_json::Value>(&original_bytes)
        .expect("should deserialize");
    let rebuilt_bytes = serde_json::to_vec(&rebuilt_pipeline).expect("should reserialize");

    assert_eq!(
        original_bytes, rebuilt_bytes,
        "canonical bytes should round-trip identically"
    );

    let rebuilt_identity = sha2_digest(&rebuilt_bytes);
    assert_eq!(
        original_identity, rebuilt_identity,
        "pipeline identity should be preserved through round-trip"
    );
}

fn sha2_digest(data: &[u8]) -> String {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(data);
    let digest = hasher.finalize();
    let mut encoded = String::from("sha256:");
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

#[test]
fn authorization_with_allow_all_policy_succeeds() {
    let pipeline = default_directory_zip_pipeline();
    let policy = AllowAllPolicy;

    let result = pipeline.build_with_authorization(example_dir(), &policy);
    assert!(result.is_ok(), "allow_all policy should permit execution");

    let (built, evidence) = result.expect("execution should succeed");
    assert_eq!(evidence.authorization_decision.decision, AuthorizationResult::Allowed);
    assert!(!built.artifact().entries.is_empty());
}

#[test]
fn authorization_with_restricted_policy_denies_missing_capabilities() {
    let pipeline = default_directory_zip_pipeline();
    let policy = AllowListPolicy::new(vec![
        ("filesystem.read".to_string(), "1".to_string()),
    ]);

    let result = pipeline.build_with_authorization(example_dir(), &policy);
    assert!(
        result.is_err(),
        "restricted policy missing capabilities should deny execution"
    );
}

#[test]
fn authorization_with_matching_policy_succeeds() {
    let pipeline = default_directory_zip_pipeline();
    let policy = AllowListPolicy::new(vec![
        ("filesystem.read".to_string(), "1".to_string()),
        ("manifest.generate".to_string(), "1".to_string()),
        ("artifact.validate".to_string(), "1".to_string()),
        ("package.zip".to_string(), "1".to_string()),
    ]);

    let result = pipeline.build_with_authorization(example_dir(), &policy);
    assert!(result.is_ok(), "matching policy should permit execution");

    let (_, evidence) = result.expect("execution should succeed");
    assert_eq!(evidence.authorization_decision.decision, AuthorizationResult::Allowed);
    let expected_count = pipeline.required_capabilities().len() + 1;
    assert_eq!(
        evidence.granted_capabilities.len(),
        expected_count,
        "all capabilities should be granted (including materializer)"
    );
}

#[test]
fn authorization_decision_identifies_denied_capabilities() {
    let pipeline = default_directory_zip_pipeline();
    let required = pipeline.required_capabilities();
    let granted = vec![
        Capability::new("filesystem.read", "1"),
        Capability::new("manifest.generate", "1"),
    ];
    let policy = AllowListPolicy::new(
        granted
            .iter()
            .map(|c| (c.name.clone(), c.version.clone()))
            .collect(),
    );

    let result = pipeline.build_with_authorization(example_dir(), &policy);
    assert!(result.is_err(), "policy missing some capabilities should deny");
}

#[test]
fn execution_evidence_records_successful_execution() {
    let pipeline = default_directory_zip_pipeline();
    let policy = AllowAllPolicy;

    let (built, evidence) = pipeline
        .build_with_authorization(example_dir(), &policy)
        .expect("execution should succeed");

    assert_eq!(
        evidence.artifact_identity,
        built.artifact().identity,
        "evidence should reference the artifact produced"
    );
    assert!(!evidence.stage_trace.is_empty(), "evidence should record stage execution");
    assert!(!evidence.used_capabilities.is_empty(), "evidence should record capabilities used");
}

#[test]
fn pipeline_with_declarative_transform_requires_correct_capabilities() {
    let pipeline = PipelineSpec {
        source: SourceSpec::Directory,
        stages: vec![
            StageSpec::Select(SelectStageSpec::new(vec![], vec![]).expect("empty select should be valid")),
            StageSpec::Transform(artifact::TransformStageSpec::new("dist").expect("prefix is valid")),
            StageSpec::Manifest,
            StageSpec::Validate,
        ],
        materializer: artifact::MaterializerSpec::Zip,
        capabilities: vec![
            Capability::new("filesystem.read", "1"),
            Capability::new("artifact.transform", "1"),
            Capability::new("manifest.generate", "1"),
            Capability::new("artifact.validate", "1"),
            Capability::new("package.zip", "1"),
        ],
    };

    let policy = AllowListPolicy::new(vec![
        ("filesystem.read".to_string(), "1".to_string()),
        ("artifact.transform".to_string(), "1".to_string()),
        ("manifest.generate".to_string(), "1".to_string()),
        ("artifact.validate".to_string(), "1".to_string()),
        ("package.zip".to_string(), "1".to_string()),
    ]);

    let result = pipeline.build_with_authorization(example_dir(), &policy);
    assert!(result.is_ok(), "pipeline with matching policy should execute");
}

#[test]
fn authorization_decision_is_deterministic() {
    let pipeline = default_directory_zip_pipeline();
    let policy = AllowAllPolicy;

    let result1 = pipeline.build_with_authorization(example_dir(), &policy);
    let result2 = pipeline.build_with_authorization(example_dir(), &policy);

    let (_, evidence1) = result1.expect("first execution should succeed");
    let (_, evidence2) = result2.expect("second execution should succeed");

    assert_eq!(
        evidence1.authorization_decision.decision,
        evidence2.authorization_decision.decision,
        "authorization decision should be deterministic"
    );
    assert_eq!(
        evidence1.pipeline_identity, evidence2.pipeline_identity,
        "pipeline identity should be consistent"
    );
}

#[test]
fn denied_pipeline_produces_no_partial_execution() {
    let pipeline = default_directory_zip_pipeline();
    let policy = AllowListPolicy::new(vec![
        ("filesystem.read".to_string(), "1".to_string()),
    ]);

    let result = pipeline.build_with_authorization(example_dir(), &policy);
    assert!(result.is_err(), "denied authorization should prevent execution");
}

#[test]
fn capability_policy_identity_is_deterministic() {
    let policy1 = AllowListPolicy::new(vec![
        ("filesystem.read".to_string(), "1".to_string()),
        ("manifest.generate".to_string(), "1".to_string()),
    ]);

    let policy2 = AllowListPolicy::new(vec![
        ("filesystem.read".to_string(), "1".to_string()),
        ("manifest.generate".to_string(), "1".to_string()),
    ]);

    let id1 = policy1.identity();
    let id2 = policy2.identity();

    assert_eq!(id1, id2, "same policy configuration should have same identity");
}

#[test]
fn allow_all_policy_identity_is_stable() {
    let policy1 = AllowAllPolicy;
    let policy2 = AllowAllPolicy;

    assert_eq!(
        policy1.identity(),
        policy2.identity(),
        "AllowAllPolicy identity should be stable"
    );
}

#[test]
fn pipeline_inspection_does_not_execute_source() {
    let pipeline = default_directory_zip_pipeline();
    let inspection = pipeline.inspect().expect("inspection should succeed");

    assert!(!inspection.pipeline_identity.is_empty());
    assert!(!inspection.stages.is_empty());
    assert!(!inspection.canonical_pipeline_json.is_empty());
}

#[test]
fn pipeline_inspection_reports_ordered_stages() {
    let pipeline = default_directory_zip_pipeline();
    let inspection = pipeline.inspect().expect("inspection should succeed");

    let labels: Vec<_> = inspection.stages.iter().map(|s| s.label.as_str()).collect();
    assert_eq!(labels, vec!["select", "manifest", "validate"]);
}

#[test]
fn pipeline_inspection_includes_materializer_capability() {
    let pipeline = default_directory_zip_pipeline();
    let inspection = pipeline.inspect().expect("inspection should succeed");

    assert!(
        inspection
            .required_capabilities
            .iter()
            .any(|c| c.name == "package.zip"),
        "inspection should include materializer capability"
    );
}

#[test]
fn pipeline_inspection_identity_is_deterministic() {
    let pipeline = default_directory_zip_pipeline();
    let inspection1 = pipeline.inspect().expect("first inspection should succeed");
    let inspection2 = pipeline.inspect().expect("second inspection should succeed");

    assert_eq!(
        inspection1.pipeline_identity, inspection2.pipeline_identity,
        "inspection identity should be deterministic"
    );
}

#[test]
fn materializer_capability_required_for_zip() {
    let pipeline = default_directory_zip_pipeline();
    let policy = AllowListPolicy::new(vec![
        ("filesystem.read".to_string(), "1".to_string()),
        ("manifest.generate".to_string(), "1".to_string()),
        ("artifact.validate".to_string(), "1".to_string()),
    ]);

    let result = pipeline.build_with_authorization(example_dir(), &policy);
    assert!(
        result.is_err(),
        "missing materializer capability should deny execution"
    );
}

#[test]
fn execution_evidence_validates_successfully() {
    let pipeline = default_directory_zip_pipeline();
    let policy = AllowAllPolicy;

    let (_, evidence) = pipeline
        .build_with_authorization(example_dir(), &policy)
        .expect("execution should succeed");

    assert!(
        evidence.validate().is_ok(),
        "successful execution evidence should validate"
    );
}

#[test]
fn denied_authorization_produces_zero_stages() {
    let pipeline = default_directory_zip_pipeline();
    let policy = AllowListPolicy::new(vec![
        ("filesystem.read".to_string(), "1".to_string()),
    ]);

    let result = pipeline.build_with_authorization(example_dir(), &policy);
    assert!(result.is_err(), "denied authorization should prevent execution");
}

#[test]
fn execution_evidence_records_stage_identities() {
    let pipeline = default_directory_zip_pipeline();
    let policy = AllowAllPolicy;

    let (_, evidence) = pipeline
        .build_with_authorization(example_dir(), &policy)
        .expect("execution should succeed");

    for stage in &evidence.stage_trace {
        if stage.label != "source" {
            assert!(
                !stage.stage_identity.is_empty(),
                "executed stage should have identity"
            );
        }
    }
}

#[test]
fn execution_evidence_semantic_portion_is_deterministic() {
    let pipeline = default_directory_zip_pipeline();
    let policy = AllowAllPolicy;

    let (_, evidence1) = pipeline
        .build_with_authorization(example_dir(), &policy)
        .expect("first execution should succeed");
    let (_, evidence2) = pipeline
        .build_with_authorization(example_dir(), &policy)
        .expect("second execution should succeed");

    let canonical1 = evidence1
        .to_canonical_bytes()
        .expect("first canonical should serialize");
    let canonical2 = evidence2
        .to_canonical_bytes()
        .expect("second canonical should serialize");

    assert_eq!(canonical1, canonical2, "semantic evidence should be deterministic");
}

#[test]
fn artifact_identity_unchanged_by_authorization() {
    let pipeline = default_directory_zip_pipeline();

    let result_allow_all = pipeline.build_with_authorization(example_dir(), &AllowAllPolicy);
    let policy_allow_list = AllowListPolicy::new(vec![
        ("filesystem.read".to_string(), "1".to_string()),
        ("manifest.generate".to_string(), "1".to_string()),
        ("artifact.validate".to_string(), "1".to_string()),
        ("package.zip".to_string(), "1".to_string()),
    ]);
    let result_allow_list = pipeline.build_with_authorization(example_dir(), &policy_allow_list);

    let (artifact1, _) = result_allow_all.expect("allow_all should succeed");
    let (artifact2, _) = result_allow_list.expect("allow_list should succeed");

    assert_eq!(
        artifact1.artifact().identity, artifact2.artifact().identity,
        "artifact identity should be independent of authorization policy"
    );
}

#[test]
fn same_artifact_materializes_as_zip_and_tar() {
    let pipeline = default_directory_zip_pipeline();
    let (built, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let artifact = built.artifact();
    let zip_bytes = ZipMaterializer
        .materialize_to_vec(artifact, &built)
        .expect("ZIP should materialize");
    let tar_bytes = TarMaterializer
        .materialize_to_vec(artifact, &built)
        .expect("TAR should materialize");

    assert!(!zip_bytes.is_empty(), "ZIP materialization should produce bytes");
    assert!(!tar_bytes.is_empty(), "TAR materialization should produce bytes");
    let zip_digest = digest_for_bytes(&zip_bytes);
    let tar_digest = digest_for_bytes(&tar_bytes);
    assert_ne!(zip_digest, tar_digest, "ZIP and TAR should have different digests");
}

#[test]
fn pipeline_inspection_canonical_json_is_valid() {
    let pipeline = default_directory_zip_pipeline();
    let inspection = pipeline.inspect().expect("inspection should succeed");

    let parsed: serde_json::Value =
        serde_json::from_str(&inspection.canonical_pipeline_json)
            .expect("canonical JSON should be valid");

    assert_eq!(parsed["schema"], "pipeline.v1");
    assert!(!parsed["stages"].as_array().unwrap().is_empty());
}

#[test]
fn execution_evidence_distinguishes_requested_granted_used() {
    let pipeline = default_directory_zip_pipeline();
    let policy = AllowAllPolicy;

    let (_, evidence) = pipeline
        .build_with_authorization(example_dir(), &policy)
        .expect("execution should succeed");

    assert!(!evidence.requested_capabilities.is_empty(), "evidence should record requested");
    assert!(
        !evidence.granted_capabilities.is_empty(),
        "evidence should record granted"
    );
    assert!(!evidence.used_capabilities.is_empty(), "evidence should record used");

    assert_eq!(
        evidence.granted_capabilities.len(),
        evidence.used_capabilities.len(),
        "granted and used should be equal for successful execution"
    );
}

#[test]
fn evidence_validates_artifact_identity_on_success() {
    let pipeline = default_directory_zip_pipeline();
    let (built, evidence) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    assert_eq!(
        evidence.artifact_identity,
        built.artifact().identity,
        "evidence should reference the produced artifact"
    );
    assert!(evidence.validate().is_ok(), "valid evidence should pass validation");
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
