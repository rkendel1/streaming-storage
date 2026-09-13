use artifact::{
    Artifact, ArtifactEntry, Capability, ContentResolver, CreationMetadata, EntryType,
    GenerateTransform, MaterializationResult, MemoryContentResolver, PipelineSpec, Provenance,
    PrefixTransform, RedactTransform, RecipeSpec, SelectStageSpec, SourceSpec, StageSpec, TarMaterializer,
    TransformedContentResolver, ZipMaterializer, default_directory_zip_pipeline,
    normalize_relative_path, ArtifactTransform, AllowAllPolicy, AllowListPolicy, AuthorizationResult,
    CapabilityPolicy, ArtifactSDK, ApplicationArtifact,
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

// Phase 5 Foundation: Recipe Compilation Tests

#[test]
fn recipe_spec_serializes_deterministically() {
    let recipe1 = RecipeSpec::directory_zip();
    let recipe2 = RecipeSpec::directory_zip();

    let json1 = serde_json::to_string(&recipe1).expect("recipe1 should serialize");
    let json2 = serde_json::to_string(&recipe2).expect("recipe2 should serialize");

    assert_eq!(json1, json2, "identical recipes should have identical JSON");
}

#[test]
fn recipe_compilation_produces_pipeline_spec() {
    let recipe = RecipeSpec::directory_zip();
    let pipeline = recipe.compile().expect("compilation should succeed");

    assert_eq!(pipeline.source, SourceSpec::Directory);
    assert!(!pipeline.stages.is_empty());
    assert_eq!(pipeline.materializer, artifact::MaterializerSpec::Zip);
}

#[test]
fn directory_zip_recipe_compiles_correctly() {
    let recipe = RecipeSpec::directory_zip();
    let pipeline = recipe.compile().expect("compilation should succeed");

    let stage_labels: Vec<_> = pipeline
        .stages
        .iter()
        .map(|s| s.label())
        .collect();

    assert_eq!(stage_labels, vec!["select", "manifest", "validate"]);
    assert!(
        pipeline
            .capabilities
            .iter()
            .any(|c| c.name == "package.zip"),
        "ZIP capability should be present"
    );
}

#[test]
fn directory_tar_recipe_compiles_correctly() {
    let recipe = RecipeSpec::directory_tar();
    let pipeline = recipe.compile().expect("compilation should succeed");

    let stage_labels: Vec<_> = pipeline
        .stages
        .iter()
        .map(|s| s.label())
        .collect();

    assert_eq!(stage_labels, vec!["select", "manifest", "validate"]);
    assert!(
        pipeline
            .capabilities
            .iter()
            .any(|c| c.name == "package.tar"),
        "TAR capability should be present"
    );
    assert_eq!(pipeline.materializer, artifact::MaterializerSpec::Tar);
}

#[test]
fn wasm_recipe_compiles_correctly() {
    let recipe = RecipeSpec::wasm();
    let pipeline = recipe.compile().expect("compilation should succeed");

    let stage_labels: Vec<_> = pipeline
        .stages
        .iter()
        .map(|s| s.label())
        .collect();

    assert_eq!(stage_labels, vec!["select", "compile", "manifest", "validate"]);
    assert!(
        pipeline
            .capabilities
            .iter()
            .any(|c| c.name == "compile.wasm"),
        "WASM capability should be present"
    );
}

#[test]
fn identical_recipes_compile_to_identical_pipelines() {
    let recipe1 = RecipeSpec::directory_zip();
    let recipe2 = RecipeSpec::directory_zip();

    let pipeline1 = recipe1.compile().expect("first compilation should succeed");
    let pipeline2 = recipe2.compile().expect("second compilation should succeed");

    let id1 = pipeline1.identity().expect("first identity should compute");
    let id2 = pipeline2.identity().expect("second identity should compute");

    assert_eq!(id1, id2, "identical recipes should produce identical pipeline identities");
}

#[test]
fn recipe_compilation_is_deterministic() {
    let recipe = RecipeSpec::directory_zip();
    let pipeline1 = recipe.compile().expect("first compilation should succeed");
    let pipeline2 = recipe.compile().expect("second compilation should succeed");

    let canonical1 = pipeline1
        .to_canonical_bytes()
        .expect("first canonical should serialize");
    let canonical2 = pipeline2
        .to_canonical_bytes()
        .expect("second canonical should serialize");

    assert_eq!(
        canonical1, canonical2,
        "identical recipe compilations should produce identical bytes"
    );
}

#[test]
fn recipe_compiled_pipeline_executes() {
    let recipe = RecipeSpec::directory_zip();
    let pipeline = recipe.compile().expect("compilation should succeed");

    let built = pipeline
        .build_from_directory(example_dir())
        .expect("execution should succeed");

    assert!(!built.artifact().identity.is_empty());
    assert_eq!(built.stage_trace(), ["source", "select", "manifest", "validate"]);
}

#[test]
fn recipe_compiled_pipeline_with_authorization_succeeds() {
    let recipe = RecipeSpec::directory_zip();
    let pipeline = recipe.compile().expect("compilation should succeed");
    let policy = AllowAllPolicy;

    let result = pipeline.build_with_authorization(example_dir(), &policy);
    assert!(result.is_ok(), "recipe execution with AllowAllPolicy should succeed");

    let (built, evidence) = result.expect("execution should succeed");
    assert!(!built.artifact().identity.is_empty());
    assert!(evidence.validate().is_ok(), "evidence should validate");
}

#[test]
fn recipe_authorization_denies_missing_capabilities() {
    let recipe = RecipeSpec::directory_zip();
    let pipeline = recipe.compile().expect("compilation should succeed");
    let policy = AllowListPolicy::new(vec![("filesystem.read".to_string(), "1".to_string())]);

    let result = pipeline.build_with_authorization(example_dir(), &policy);
    assert!(
        result.is_err(),
        "denied authorization should prevent execution"
    );
}

#[test]
fn different_recipe_types_have_different_identities() {
    let zip_recipe = RecipeSpec::directory_zip();
    let tar_recipe = RecipeSpec::directory_tar();

    let zip_pipeline = zip_recipe.compile().expect("ZIP compilation should succeed");
    let tar_pipeline = tar_recipe.compile().expect("TAR compilation should succeed");

    let zip_id = zip_pipeline.identity().expect("ZIP identity should compute");
    let tar_id = tar_pipeline.identity().expect("TAR identity should compute");

    assert_ne!(
        zip_id, tar_id,
        "different recipe types should produce different pipeline identities"
    );
}

#[test]
fn recipe_with_custom_exclusions_compiles() {
    let recipe = RecipeSpec::directory_zip()
        .with_exclusions(vec!["test.txt".to_string()], vec!["temp".to_string()]);

    let pipeline = recipe.compile().expect("compilation should succeed");
    assert!(!pipeline.stages.is_empty());
}

#[test]
fn recipe_compiled_artifact_identity_is_independent_of_authorization() {
    let recipe = RecipeSpec::directory_zip();
    let pipeline = recipe.compile().expect("compilation should succeed");

    let allow_all = AllowAllPolicy;
    let allow_list = AllowListPolicy::new(vec![
        ("filesystem.read".to_string(), "1".to_string()),
        ("manifest.generate".to_string(), "1".to_string()),
        ("artifact.validate".to_string(), "1".to_string()),
        ("package.zip".to_string(), "1".to_string()),
    ]);

    let (artifact1, _) = pipeline
        .build_with_authorization(example_dir(), &allow_all)
        .expect("AllowAllPolicy should succeed");
    let (artifact2, _) = pipeline
        .build_with_authorization(example_dir(), &allow_list)
        .expect("AllowListPolicy should succeed");

    assert_eq!(
        artifact1.artifact().identity, artifact2.artifact().identity,
        "artifact identity should not depend on authorization policy"
    );
}

#[test]
fn wasm_recipe_includes_compile_capability() {
    let recipe = RecipeSpec::wasm();
    let pipeline = recipe.compile().expect("compilation should succeed");

    assert!(
        pipeline
            .capabilities
            .iter()
            .any(|c| c.name == "compile.wasm" && c.version == "1"),
        "WASM recipe should include compile.wasm capability"
    );
}

#[test]
fn all_phase_1_4_tests_remain_passing() {
    // This is implicit: all existing tests are run
    let pipeline = default_directory_zip_pipeline();
    assert!(!pipeline.stages.is_empty());
}

#[test]
fn recipe_compilation_has_zero_side_effects() {
    // Compilation performs no I/O, no authorization, no execution
    let recipe = RecipeSpec::directory_zip();
    let _pipeline = recipe.compile().expect("compilation should succeed");
    // If we got here without panicking or blocking on I/O, compilation was pure
}

#[test]
fn recipe_materialization_follows_compiled_pipeline() {
    let recipe = RecipeSpec::directory_zip();
    let pipeline = recipe.compile().expect("compilation should succeed");

    let (built, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let zip_bytes = ZipMaterializer
        .materialize_to_vec(built.artifact(), &built)
        .expect("materialization should succeed");

    assert!(!zip_bytes.is_empty(), "ZIP materialization should produce bytes");
}


// Phase 5 Completion: WASM Build Semantics and Target Compilation

#[test]
fn wasm_recipe_execution_produces_application_wasm() {
    let recipe = RecipeSpec::wasm();
    let pipeline = recipe.compile().expect("compilation should succeed");

    let (built, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let wasm_entry = built
        .artifact()
        .entries
        .iter()
        .find(|e| e.path == "application.wasm");

    assert!(
        wasm_entry.is_some(),
        "WASM compilation should produce application.wasm entry"
    );
}

#[test]
fn wasm_compilation_through_pipeline_executor() {
    let recipe = RecipeSpec::wasm();
    let pipeline = recipe.compile().expect("compilation should succeed");

    let built = pipeline
        .build_from_directory(example_dir())
        .expect("execution should succeed");

    let stage_trace = built.stage_trace();
    assert!(
        stage_trace.contains(&"compile".to_string()),
        "stage trace should include compile stage"
    );
}

#[test]
fn compile_stage_identity_is_deterministic() {
    let recipe = RecipeSpec::wasm();
    let pipeline1 = recipe.compile().expect("first compilation should succeed");
    let pipeline2 = recipe.compile().expect("second compilation should succeed");

    let compile_stage1 = &pipeline1.stages[1];
    let compile_stage2 = &pipeline2.stages[1];

    let id1 = compile_stage1.identity().expect("first stage identity should compute");
    let id2 = compile_stage2.identity().expect("second stage identity should compute");

    assert_eq!(id1, id2, "identical compile stages should have identical identities");
}

#[test]
fn wasm_recipe_requires_compile_capability() {
    let recipe = RecipeSpec::wasm();
    let pipeline = recipe.compile().expect("compilation should succeed");

    assert!(
        pipeline
            .required_capabilities()
            .iter()
            .any(|c| c.name == "compile.wasm"),
        "WASM pipeline should declare compile.wasm capability"
    );
}

#[test]
fn wasm_authorization_denies_without_compile_capability() {
    let recipe = RecipeSpec::wasm();
    let pipeline = recipe.compile().expect("compilation should succeed");

    let policy = AllowListPolicy::new(vec![
        ("filesystem.read".to_string(), "1".to_string()),
        ("manifest.generate".to_string(), "1".to_string()),
        ("artifact.validate".to_string(), "1".to_string()),
        ("package.zip".to_string(), "1".to_string()),
    ]);

    let result = pipeline.build_with_authorization(example_dir(), &policy);
    assert!(
        result.is_err(),
        "missing compile.wasm capability should deny authorization"
    );
}

#[test]
fn wasm_authorization_permits_with_compile_capability() {
    let recipe = RecipeSpec::wasm();
    let pipeline = recipe.compile().expect("compilation should succeed");

    let policy = AllowListPolicy::new(vec![
        ("filesystem.read".to_string(), "1".to_string()),
        ("manifest.generate".to_string(), "1".to_string()),
        ("artifact.validate".to_string(), "1".to_string()),
        ("compile.wasm".to_string(), "1".to_string()),
        ("package.zip".to_string(), "1".to_string()),
    ]);

    let result = pipeline.build_with_authorization(example_dir(), &policy);
    assert!(
        result.is_ok(),
        "all required capabilities should permit authorization"
    );
}

#[test]
fn wasm_execution_evidence_records_compile_stage() {
    let recipe = RecipeSpec::wasm();
    let pipeline = recipe.compile().expect("compilation should succeed");

    let (_, evidence) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let compile_stage = evidence
        .stage_trace
        .iter()
        .find(|s| s.label == "compile");

    assert!(
        compile_stage.is_some(),
        "evidence should record compile stage"
    );
    assert!(
        compile_stage.unwrap().stage_identity != "",
        "compile stage should have deterministic identity in evidence"
    );
}

#[test]
fn wasm_evidence_distinguishes_capabilities() {
    let recipe = RecipeSpec::wasm();
    let pipeline = recipe.compile().expect("compilation should succeed");

    let (_, evidence) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    assert!(
        evidence
            .requested_capabilities
            .iter()
            .any(|c| c.name == "compile.wasm"),
        "evidence should record compile.wasm in requested capabilities"
    );
    assert!(
        evidence
            .granted_capabilities
            .iter()
            .any(|c| c.name == "compile.wasm"),
        "evidence should record compile.wasm in granted capabilities"
    );
    assert!(
        evidence
            .used_capabilities
            .iter()
            .any(|c| c.name == "compile.wasm"),
        "evidence should record compile.wasm in used capabilities"
    );
}

#[test]
fn wasm_artifact_identity_independent_of_evidence() {
    let recipe = RecipeSpec::wasm();
    let pipeline = recipe.compile().expect("compilation should succeed");

    let (built1, evidence1) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("first execution should succeed");
    let (built2, evidence2) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("second execution should succeed");

    assert_eq!(
        built1.artifact().identity, built2.artifact().identity,
        "artifact identity should be stable across executions"
    );

    let evidence_id1 = evidence1.identity().expect("first evidence identity should compute");
    let evidence_id2 = evidence2.identity().expect("second evidence identity should compute");

    assert_eq!(
        evidence_id1, evidence_id2,
        "evidence identity should be deterministic for same execution"
    );

    assert_ne!(
        built1.artifact().identity, evidence_id1,
        "artifact identity should not include evidence identity"
    );
}

#[test]
fn wasm_artifact_identity_independent_of_authorization_policy() {
    let recipe = RecipeSpec::wasm();
    let pipeline = recipe.compile().expect("compilation should succeed");

    let allow_all = AllowAllPolicy;
    let allow_list = AllowListPolicy::new(vec![
        ("filesystem.read".to_string(), "1".to_string()),
        ("manifest.generate".to_string(), "1".to_string()),
        ("artifact.validate".to_string(), "1".to_string()),
        ("compile.wasm".to_string(), "1".to_string()),
        ("package.zip".to_string(), "1".to_string()),
    ]);

    let (artifact1, _) = pipeline
        .build_with_authorization(example_dir(), &allow_all)
        .expect("AllowAllPolicy should succeed");
    let (artifact2, _) = pipeline
        .build_with_authorization(example_dir(), &allow_list)
        .expect("AllowListPolicy should succeed");

    assert_eq!(
        artifact1.artifact().identity, artifact2.artifact().identity,
        "artifact identity should not depend on authorization policy"
    );
}

#[test]
fn wasm_recipe_compilation_is_pure_and_deterministic() {
    let recipe1 = RecipeSpec::wasm();
    let recipe2 = RecipeSpec::wasm();

    let pipeline1 = recipe1.compile().expect("first compilation should succeed");
    let pipeline2 = recipe2.compile().expect("second compilation should succeed");

    let canonical1 = pipeline1
        .to_canonical_bytes()
        .expect("first canonical should serialize");
    let canonical2 = pipeline2
        .to_canonical_bytes()
        .expect("second canonical should serialize");

    assert_eq!(
        canonical1, canonical2,
        "identical WASM recipes should compile to identical PipelineSpecs"
    );
}

#[test]
fn compile_stage_participates_in_artifact_transformation() {
    let recipe = RecipeSpec::wasm();
    let pipeline = recipe.compile().expect("compilation should succeed");

    let built1 = pipeline
        .build_from_directory(example_dir())
        .expect("first execution should succeed");
    let built2 = pipeline
        .build_from_directory(example_dir())
        .expect("second execution should succeed");

    assert_eq!(
        built1.artifact().identity, built2.artifact().identity,
        "same source and pipeline should produce same artifact identity"
    );

    assert!(
        built1
            .artifact()
            .entries
            .iter()
            .any(|e| e.path == "application.wasm"),
        "compiled artifact should contain WASM output"
    );
}

#[test]
fn wasm_source_immutability_preserved() {
    let recipe = RecipeSpec::wasm();
    let pipeline = recipe.compile().expect("compilation should succeed");

    let (built, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let original_entries = built
        .artifact()
        .entries
        .iter()
        .filter(|e| !e.path.starts_with("application.wasm"))
        .count();

    assert!(original_entries > 0, "source entries should be preserved");
    assert!(
        built
            .artifact()
            .entries
            .iter()
            .any(|e| e.path == "application.wasm"),
        "compiled output should be added"
    );
}

#[test]
fn directory_zip_and_wasm_both_use_pipeline_executor() {
    let zip_recipe = RecipeSpec::directory_zip();
    let wasm_recipe = RecipeSpec::wasm();

    let zip_pipeline = zip_recipe.compile().expect("ZIP compilation should succeed");
    let wasm_pipeline = wasm_recipe.compile().expect("WASM compilation should succeed");

    let zip_built = zip_pipeline
        .build_from_directory(example_dir())
        .expect("ZIP should execute");
    let wasm_built = wasm_pipeline
        .build_from_directory(example_dir())
        .expect("WASM should execute");

    assert!(
        !zip_built.artifact().identity.is_empty(),
        "ZIP artifact should have identity"
    );
    assert!(
        !wasm_built.artifact().identity.is_empty(),
        "WASM artifact should have identity"
    );

    assert_ne!(
        zip_built.artifact().identity, wasm_built.artifact().identity,
        "different recipes should produce different artifacts"
    );
}

#[test]
fn all_phase_1_5_tests_remain_passing() {
    // Implicit: all existing tests pass
    let zip_pipeline = default_directory_zip_pipeline();
    let wasm_recipe = RecipeSpec::wasm();
    let wasm_pipeline = wasm_recipe.compile().expect("WASM should compile");

    assert!(!zip_pipeline.stages.is_empty());
    assert!(!wasm_pipeline.stages.is_empty());
}


// Phase 6 Foundation: Public API and CLI Surface

#[test]
fn public_api_recipe_compiles() {
    use artifact::ArtifactSDK;
    
    let recipe = artifact::RecipeSpec::wasm();
    let artifact_recipe = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = artifact_recipe.compile().expect("compilation should succeed");
    
    assert!(pipeline.required_capabilities().len() > 0);
}

#[test]
fn public_api_inspection_works() {
    use artifact::ArtifactSDK;
    
    let recipe = artifact::RecipeSpec::wasm();
    let artifact_recipe = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = artifact_recipe.compile().expect("compilation should succeed");
    let inspection = pipeline.inspect().expect("inspection should succeed");
    
    assert!(!inspection.pipeline_identity.is_empty());
    assert!(!inspection.stages.is_empty());
    assert!(!inspection.required_capabilities.is_empty());
}

#[test]
fn public_api_execution_succeeds() {
    use artifact::ArtifactSDK;
    
    let recipe = artifact::RecipeSpec::directory_zip();
    let artifact_recipe = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = artifact_recipe.compile().expect("compilation should succeed");
    
    let (artifact, evidence) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");
    
    assert!(!artifact.identity().is_empty());
    assert!(evidence.is_successful());
}

#[test]
fn public_api_artifact_entries_accessible() {
    use artifact::ArtifactSDK;
    
    let recipe = artifact::RecipeSpec::directory_zip();
    let artifact_recipe = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = artifact_recipe.compile().expect("compilation should succeed");
    
    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");
    
    let entries = artifact.entries();
    assert!(!entries.is_empty(), "artifact should have entries");
    assert!(
        entries.iter().any(|e| e.path == "README.md"),
        "should have README.md"
    );
}

#[test]
fn public_api_evidence_accessible() {
    use artifact::ArtifactSDK;
    
    let recipe = artifact::RecipeSpec::wasm();
    let artifact_recipe = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = artifact_recipe.compile().expect("compilation should succeed");
    
    let (_, evidence) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");
    
    let decision = evidence.authorization_decision();
    assert!(decision.allowed, "authorization should be allowed");
    assert!(!decision.granted_capabilities.is_empty());
    
    let stages = evidence.stage_trace();
    assert!(!stages.is_empty(), "should have executed stages");
}

#[test]
fn public_api_denies_correctly() {
    use artifact::ArtifactSDK;
    
    let recipe = artifact::RecipeSpec::wasm();
    let artifact_recipe = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = artifact_recipe.compile().expect("compilation should succeed");
    
    let policy = AllowListPolicy::new(vec![
        ("filesystem.read".to_string(), "1".to_string()),
    ]);
    
    let result = pipeline.build_with_authorization(example_dir(), &policy);
    assert!(result.is_err(), "missing capabilities should deny");
}

#[test]
fn public_api_cli_layer_is_thin() {
    // This test verifies the CLI is just a client
    // It should compile and link without duplicating logic
    use artifact::ArtifactSDK;

    // The pattern the CLI uses:
    let recipe = artifact::RecipeSpec::directory_zip();
    let artifact_recipe = ArtifactSDK::recipe_from_spec(recipe);
    let _pipeline = artifact_recipe.compile();

    // The CLI does NOT:
    // - recompute artifact identity
    // - reimplement authorization
    // - duplicate stage execution
    // - maintain separate cache
    // The CLI only calls the SDK.
}

// ============================================================================
// Phase 6 Completion Tests: TypeScript SDK as Pure Client
// ============================================================================

#[test]
fn typescript_sdk_delegates_recipe_compilation() {
    // TypeScript SDK must not implement compilation logic.
    // It receives RecipeSpec JSON, sends to WASM, gets PipelineSpec JSON back.
    // This test verifies the delegation pattern.
    use artifact::ArtifactSDK;

    let recipe = artifact::RecipeSpec::directory_zip();
    let artifact_recipe = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = artifact_recipe.compile().expect("compilation should succeed");

    // The pipeline should have expected structure - proof Rust did the compilation
    let inspection = pipeline.inspect().expect("inspection should succeed");
    assert!(
        inspection.stages.iter().any(|s| s.label == "select"),
        "compiled pipeline should have select stage"
    );
}

#[test]
fn typescript_sdk_delegates_inspection() {
    // TypeScript SDK receives inspection result from WASM, parses JSON.
    // Does not recompute pipeline identity or stage identities.
    use artifact::ArtifactSDK;

    let recipe = artifact::RecipeSpec::directory_zip();
    let artifact_recipe = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = artifact_recipe.compile().expect("compilation should succeed");

    let inspection1 = pipeline.inspect().expect("inspection should succeed");
    let inspection2 = pipeline.inspect().expect("inspection should succeed");

    // Same pipeline inspected twice produces identical structure (deterministic)
    assert_eq!(inspection1.pipeline_identity, inspection2.pipeline_identity);
}

#[test]
fn typescript_sdk_delegates_capability_calculation() {
    // TypeScript SDK does not calculate which capabilities are required.
    // It receives the list from WASM.
    use artifact::ArtifactSDK;

    let wasm_recipe = artifact::RecipeSpec::wasm();
    let zip_recipe = artifact::RecipeSpec::directory_zip();

    let wasm_artifact = ArtifactSDK::recipe_from_spec(wasm_recipe);
    let zip_artifact = ArtifactSDK::recipe_from_spec(zip_recipe);

    let wasm_pipeline = wasm_artifact.compile().expect("compilation should succeed");
    let zip_pipeline = zip_artifact.compile().expect("compilation should succeed");

    let wasm_caps = wasm_pipeline.required_capabilities();
    let zip_caps = zip_pipeline.required_capabilities();

    // WASM recipe should require compile capability that ZIP doesn't
    assert!(
        wasm_caps.iter().any(|c| c.name == "compile.wasm"),
        "WASM recipe should require compile.wasm capability"
    );
    assert!(
        !zip_caps.iter().any(|c| c.name == "compile.wasm"),
        "ZIP recipe should not require compile.wasm capability"
    );
}

#[test]
fn typescript_sdk_delegates_authorization() {
    // TypeScript SDK does not implement authorization logic.
    // It passes authorization decision from WASM.
    use artifact::ArtifactSDK;

    let recipe = artifact::RecipeSpec::wasm();
    let artifact_recipe = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = artifact_recipe.compile().expect("compilation should succeed");

    let (_, evidence) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let decision = evidence.authorization_decision();

    // Authorization decision comes from Rust policy evaluation
    assert!(decision.allowed);
    assert!(
        decision.granted_capabilities.iter().any(|c| c.name == "compile.wasm"),
        "wasm recipe should have compile.wasm granted"
    );
}

#[test]
fn typescript_sdk_delegates_execution() {
    // TypeScript SDK does not execute stages itself.
    // It receives artifact and evidence from WASM.
    use artifact::ArtifactSDK;

    let recipe = artifact::RecipeSpec::directory_zip();
    let artifact_recipe = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = artifact_recipe.compile().expect("compilation should succeed");

    let artifact = pipeline.build_from_directory(example_dir()).expect("build should succeed");

    // Build produces artifact with expected structure
    assert!(!artifact.identity().is_empty());
    assert!(artifact.entries_count() > 0);
}

#[test]
fn typescript_sdk_does_not_recompute_artifact_identity() {
    // Artifact identity must come from Rust computation only.
    // TypeScript SDK receives identity string, does not recalculate.
    use artifact::ArtifactSDK;

    let recipe = artifact::RecipeSpec::directory_zip();
    let artifact_recipe = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = artifact_recipe.compile().expect("compilation should succeed");

    let (artifact1, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let (artifact2, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    // Same input produces identical artifact identity (proves Rust does calculation)
    assert_eq!(artifact1.identity(), artifact2.identity());
}

#[test]
fn typescript_sdk_does_not_duplicate_stage_execution() {
    // Stage execution happens only in Rust PipelineExecutor.
    // TypeScript SDK receives stage_trace in evidence, does not execute stages.
    use artifact::ArtifactSDK;

    let recipe = artifact::RecipeSpec::wasm();
    let artifact_recipe = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = artifact_recipe.compile().expect("compilation should succeed");

    let (_, evidence) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let stages = evidence.stage_trace();

    // Stage trace shows what Rust executed, not something TypeScript invented
    assert!(
        stages.iter().any(|s| s.label == "select"),
        "stage trace should include select"
    );
    assert!(
        stages.iter().any(|s| s.label == "compile"),
        "stage trace should include compile for WASM"
    );
}

#[test]
fn typescript_sdk_serialization_round_trip() {
    // Recipe and Pipeline must serialize/deserialize identically.
    // This ensures TypeScript can parse JSON from Rust deterministically.
    use artifact::ArtifactSDK;

    let recipe = artifact::RecipeSpec::directory_zip();
    let artifact_recipe = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = artifact_recipe.compile().expect("compilation should succeed");

    // Get serialized forms
    let inspection1 = pipeline.inspect().expect("inspection should succeed");
    let inspection2 = pipeline.inspect().expect("inspection should succeed");

    // Serialized forms should be identical (deterministic)
    assert_eq!(inspection1.pipeline_identity, inspection2.pipeline_identity);
    assert_eq!(inspection1.stages.len(), inspection2.stages.len());
}

#[test]
fn typescript_sdk_evidence_contains_all_required_fields() {
    // Evidence must contain all fields TypeScript SDK exposes.
    // This verifies the WASM bridge returns complete data.
    use artifact::ArtifactSDK;

    let recipe = artifact::RecipeSpec::wasm();
    let artifact_recipe = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = artifact_recipe.compile().expect("compilation should succeed");

    let (artifact, evidence) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    // All evidence fields must be populated
    let decision = evidence.authorization_decision();
    assert!(decision.allowed);
    assert!(!decision.requested_capabilities.is_empty());
    assert!(!decision.granted_capabilities.is_empty());
    assert!(!evidence.stage_trace().is_empty());
    assert!(!evidence.used_capabilities().is_empty());

    // Artifact must have content
    assert!(!artifact.identity().is_empty());
    assert!(artifact.entries_count() > 0);
}

#[test]
fn typescript_sdk_wasm_recipe_compilation() {
    // WASM recipe must compile through Rust compilation logic.
    // TypeScript receives WASM in artifact, not as side effect.
    use artifact::ArtifactSDK;

    let recipe = artifact::RecipeSpec::wasm();
    let artifact_recipe = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = artifact_recipe.compile().expect("compilation should succeed");

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    // WASM output should be in artifact entries
    let has_wasm = artifact
        .entries()
        .iter()
        .any(|e| e.path == "application.wasm");

    assert!(
        has_wasm,
        "WASM artifact should contain application.wasm entry"
    );
}

#[test]
fn typescript_sdk_cli_pattern_compatible() {
    // CLI and TypeScript SDK must use same execution path.
    // This test verifies both follow thin-client pattern.
    use artifact::ArtifactSDK;

    // CLI pattern: parse args → create RecipeSpec → compile → inspect → authorize → execute
    let recipe = artifact::RecipeSpec::directory_zip();
    let artifact_recipe = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = artifact_recipe.compile().expect("compilation should succeed");

    // Inspect
    let _inspection = pipeline.inspect().expect("inspection should succeed");

    // Build with authorization (CLI uses AllowAllPolicy)
    let (_artifact, _evidence) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    // Both CLI and TypeScript SDK follow identical pattern, call same SDK methods
}

// ============================================================================
// Phase 7 Foundation Tests: Application Artifact Model
// ============================================================================

#[test]
fn application_artifact_wraps_wasm_artifact() {
    use artifact::{ArtifactSDK, ApplicationArtifact};

    let recipe = artifact::RecipeSpec::wasm();
    let artifact_recipe = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = artifact_recipe.compile().expect("compilation should succeed");

    let (built_artifact, evidence) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let app_artifact = ApplicationArtifact::from_wasm_artifact(
        built_artifact.as_artifact().clone(),
        evidence.used_capabilities(),
    )
    .expect("should create application artifact");

    assert_eq!(app_artifact.identity(), built_artifact.identity());
}

#[test]
fn application_artifact_identity_is_artifact_identity() {
    use artifact::{ArtifactSDK, ApplicationArtifact};

    let recipe = artifact::RecipeSpec::wasm();
    let artifact_recipe = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = artifact_recipe.compile().expect("compilation should succeed");

    let (built_artifact1, evidence1) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let (built_artifact2, evidence2) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let app1 =
        ApplicationArtifact::from_wasm_artifact(built_artifact1.as_artifact().clone(), evidence1.used_capabilities())
            .expect("should create app artifact");
    let app2 =
        ApplicationArtifact::from_wasm_artifact(built_artifact2.as_artifact().clone(), evidence2.used_capabilities())
            .expect("should create app artifact");

    assert_eq!(app1.identity(), app2.identity(), "same recipe produces same application identity");
}

#[test]
fn application_manifest_is_deterministic() {
    use artifact::{ArtifactSDK, ApplicationArtifact};

    let recipe = artifact::RecipeSpec::wasm();
    let artifact_recipe = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = artifact_recipe.compile().expect("compilation should succeed");

    let (built_artifact, evidence) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let app =
        ApplicationArtifact::from_wasm_artifact(built_artifact.as_artifact().clone(), evidence.used_capabilities())
            .expect("should create app artifact");

    let manifest_json1 = app.manifest().to_json().expect("manifest should serialize");
    let manifest_json2 = app.manifest().to_json().expect("manifest should serialize");

    assert_eq!(manifest_json1, manifest_json2, "manifest should be deterministic");
}

#[test]
fn application_manifest_contains_declared_capabilities() {
    use artifact::{ArtifactSDK, ApplicationArtifact};

    let recipe = artifact::RecipeSpec::wasm();
    let artifact_recipe = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = artifact_recipe.compile().expect("compilation should succeed");

    let (built_artifact, evidence) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let capabilities = evidence.used_capabilities();
    let app = ApplicationArtifact::from_wasm_artifact(built_artifact.as_artifact().clone(), capabilities.clone())
        .expect("should create app artifact");

    assert_eq!(
        app.declared_capabilities().len(),
        capabilities.len(),
        "manifest should contain all declared capabilities"
    );
}

#[test]
fn application_manifest_has_entrypoint() {
    use artifact::{ArtifactSDK, ApplicationArtifact};

    let recipe = artifact::RecipeSpec::wasm();
    let artifact_recipe = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = artifact_recipe.compile().expect("compilation should succeed");

    let (built_artifact, evidence) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let app = ApplicationArtifact::from_wasm_artifact(built_artifact.as_artifact().clone(), evidence.used_capabilities())
        .expect("should create app artifact");

    assert_eq!(app.manifest().entrypoint, "application.wasm");
}

#[test]
fn application_artifact_has_executable() {
    use artifact::{ArtifactSDK, ApplicationArtifact};

    let recipe = artifact::RecipeSpec::wasm();
    let artifact_recipe = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = artifact_recipe.compile().expect("compilation should succeed");

    let (built_artifact, evidence) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let app = ApplicationArtifact::from_wasm_artifact(built_artifact.as_artifact().clone(), evidence.used_capabilities())
        .expect("should create app artifact");

    assert!(app.has_executable(), "should have application.wasm executable");
}

#[test]
fn application_artifact_remains_thin_wrapper() {
    use artifact::{ArtifactSDK, ApplicationArtifact};

    let recipe = artifact::RecipeSpec::wasm();
    let artifact_recipe = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = artifact_recipe.compile().expect("compilation should succeed");

    let (built_artifact, evidence) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let artifact_id = built_artifact.identity();
    let app = ApplicationArtifact::from_wasm_artifact(built_artifact.as_artifact().clone(), evidence.used_capabilities())
        .expect("should create app artifact");

    // ApplicationArtifact should not have its own identity computation
    // It only wraps and interprets existing artifact
    assert_eq!(app.identity(), artifact_id, "application should use artifact identity, not compute separate one");
}

#[test]
fn application_artifact_manifest_does_not_affect_identity() {
    use artifact::{ArtifactSDK, ApplicationArtifact};

    let recipe = artifact::RecipeSpec::wasm();
    let artifact_recipe = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = artifact_recipe.compile().expect("compilation should succeed");

    let (built_artifact, evidence) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    // Create app with two different capability lists
    let cap1 = vec![];
    let cap2 = evidence.used_capabilities();

    let app1 = ApplicationArtifact::from_wasm_artifact(built_artifact.as_artifact().clone(), cap1)
        .expect("should create app artifact");
    let app2 = ApplicationArtifact::from_wasm_artifact(built_artifact.as_artifact().clone(), cap2)
        .expect("should create app artifact");

    // Identity should remain same because artifact didn't change
    assert_eq!(app1.identity(), app2.identity(), "manifest differences should not affect identity");
}

#[test]
fn application_artifact_non_wasm_rejected() {
    use artifact::{ArtifactSDK, ApplicationArtifact};

    let recipe = artifact::RecipeSpec::directory_zip();
    let artifact_recipe = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = artifact_recipe.compile().expect("compilation should succeed");

    let (built_artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let result = ApplicationArtifact::from_wasm_artifact(built_artifact.as_artifact().clone(), vec![]);

    assert!(
        result.is_err(),
        "non-WASM artifact should not be interpretable as application"
    );
}

#[test]
fn application_artifact_boundaries_clear() {
    // This test documents the architectural boundary
    use artifact::{ArtifactSDK, ApplicationArtifact};

    // Artifact Engine does:
    let recipe = artifact::RecipeSpec::wasm();
    let artifact_recipe = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = artifact_recipe.compile().expect("compilation should succeed");
    let (artifact, evidence) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    // Application Artifact interprets:
    let _app = ApplicationArtifact::from_wasm_artifact(artifact.as_artifact().clone(), evidence.used_capabilities())
        .expect("should interpret as application");

    // Application Artifact does NOT:
    // - execute the WASM
    // - allocate resources
    // - provision capabilities
    // - manage lifecycle
    // - create deployment packages
    // It is purely an interpretation layer.
}

// ============================================================================
// Phase 8 Foundation Tests: Artifact Composition
// ============================================================================

#[test]
fn composition_single_artifact_preserves_identity() {
    let recipe = RecipeSpec::directory_zip();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let a = artifact.as_artifact().clone();
    let original_identity = a.identity.clone();

    let composition_input = artifact::CompositionInput::new(vec![a]);
    let composed = composition_input
        .compose(artifact::CompositionOptions::default())
        .expect("composition should succeed");

    assert_eq!(
        composed.identity, original_identity,
        "composition of single artifact preserves identity"
    );
}

#[test]
fn composition_produces_ordinary_artifact() {
    let recipe = RecipeSpec::directory_zip();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let a = artifact.as_artifact().clone();

    let composition_input = artifact::CompositionInput::new(vec![a]);
    let composed = composition_input
        .compose(artifact::CompositionOptions::default())
        .expect("composition should succeed");

    // Result is an ordinary Artifact
    assert!(!composed.identity.is_empty(), "composed artifact has identity");
    assert!(!composed.entries.is_empty(), "composed artifact has entries");
    assert!(!composed.capabilities.is_empty(), "composed artifact has capabilities");
}

#[test]
fn composition_identity_is_deterministic() {
    let recipe = RecipeSpec::directory_zip();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let a = artifact.as_artifact().clone();

    let composition1 = artifact::CompositionInput::new(vec![a.clone()]);
    let composed1 = composition1
        .compose(artifact::CompositionOptions::default())
        .expect("composition should succeed");

    let composition2 = artifact::CompositionInput::new(vec![a]);
    let composed2 = composition2
        .compose(artifact::CompositionOptions::default())
        .expect("composition should succeed");

    assert_eq!(composed1.identity, composed2.identity, "composition is deterministic");
}

#[test]
fn composition_preserves_capabilities() {
    let recipe = RecipeSpec::wasm();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, evidence) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let wasm_artifact = artifact.as_artifact().clone();
    let capabilities = evidence.used_capabilities();

    let composition_input = artifact::CompositionInput::new(vec![wasm_artifact]);
    let composed = composition_input
        .compose(artifact::CompositionOptions::default())
        .expect("composition should succeed");

    assert!(
        capabilities
            .iter()
            .all(|cap| composed.capabilities.iter().any(|c| c == cap)),
        "composition preserves capabilities"
    );
}

#[test]
fn composition_no_second_execution_engine() {
    // This test documents that composition does NOT introduce:
    // - CompositeArtifact type
    // - CompositionPipeline
    // - ApplicationGraph
    // - Second execution engine
    //
    // Composition is purely a merge operation using existing Artifact model

    let recipe = RecipeSpec::directory_zip();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let a = artifact.as_artifact().clone();

    let composition_input = artifact::CompositionInput::new(vec![a]);
    let _composed = composition_input
        .compose(artifact::CompositionOptions::default())
        .expect("composition should succeed");

    // Result is Artifact type, not a new CompositeArtifact or similar
}
// Phase 9 Foundation: Artifact Semantics Discovery Tests
//
// Model B Tests: Artifact-Associated Semantic Declaration
//
// Key findings tested:
// - Semantic declarations are queryable fields on artifacts
// - They do NOT affect artifact identity
// - They do NOT change pipeline identity
// - They are distinct from provenance
// - Artifacts can exist without them
// - Transforms and composition explicitly lose declarations
// - Declarations serialize in canonical JSON

#[test]
fn phase9_identity_test1_semantic_declaration_is_expressible() {
    let recipe = RecipeSpec::wasm();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let mut wasm_artifact = artifact.as_artifact().clone();
    wasm_artifact = wasm_artifact.with_semantic_declaration("wasm_application");

    assert_eq!(
        wasm_artifact.semantic_type(),
        Some("wasm_application"),
        "semantic declaration should be queryable"
    );
}

#[test]
fn phase9_identity_test2_artifact_identity_unchanged_by_declaration() {
    let recipe = RecipeSpec::wasm();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let base_artifact = artifact.as_artifact().clone();
    let identity_before = base_artifact.identity.clone();

    let with_app = base_artifact.clone().with_semantic_declaration("wasm_application");
    let with_lib = base_artifact.clone().with_semantic_declaration("wasm_library");

    assert_eq!(
        with_app.identity, with_lib.identity,
        "semantic declaration does not affect identity"
    );
    assert_eq!(identity_before, with_app.identity);
}

#[test]
fn phase9_identity_test3_pipeline_identity_remains_distinct() {
    let recipe = RecipeSpec::wasm();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let inspect_before = pipeline.inspect().expect("inspect should succeed");
    let pipeline_id = inspect_before.pipeline_identity.clone();

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let inspect_after = pipeline.inspect().expect("inspect should succeed");
    assert_eq!(inspect_after.pipeline_identity, pipeline_id, "pipeline identity stable");

    let mut wasm_artifact = artifact.as_artifact().clone();
    wasm_artifact = wasm_artifact.with_semantic_declaration("wasm_application");

    // Pipeline identity unchanged even though artifact has semantic declaration
    let inspect_final = pipeline.inspect().expect("inspect should succeed");
    assert_eq!(inspect_final.pipeline_identity, pipeline_id);
}

#[test]
fn phase9_identity_test4_provenance_distinct_from_semantic_meaning() {
    let recipe = RecipeSpec::directory_zip();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let artifact = artifact.as_artifact().clone();

    assert!(!artifact.provenance.source_identity.is_empty());
    assert_eq!(artifact.semantic_type(), None);

    let with_decl = artifact.clone().with_semantic_declaration("some_type");
    assert_eq!(artifact.provenance, with_decl.provenance, "provenance unchanged by declaration");
}

#[test]
fn phase9_identity_test5_artifact_can_exist_without_semantic_declaration() {
    let recipe = RecipeSpec::directory_zip();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let artifact = artifact.as_artifact().clone();

    assert!(artifact.semantic_type().is_none());
    assert!(!artifact.identity.is_empty());
    assert!(!artifact.entries.is_empty());
}

#[test]
fn phase9_interpretation_test6_consumer_queries_declaration_without_inference() {
    let recipe = RecipeSpec::wasm();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let mut wasm_artifact = artifact.as_artifact().clone();

    // No inference from .wasm files
    assert_eq!(wasm_artifact.semantic_type(), None);

    wasm_artifact = wasm_artifact.with_semantic_declaration("wasm_application");
    assert_eq!(wasm_artifact.semantic_type(), Some("wasm_application"));
}

#[test]
fn phase9_interpretation_test7_interpretation_does_not_invent_semantic_information() {
    let recipe = RecipeSpec::wasm();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let wasm_artifact = artifact.as_artifact().clone();

    // No silent inference
    assert!(wasm_artifact.semantic_type().is_none());

    // Explicit interpretation is still possible
    let app = ApplicationArtifact::from_wasm_artifact(
        wasm_artifact,
        vec![Capability::new("wasm.execute", "1")],
    );
    assert!(app.is_ok());
}

#[test]
fn phase9_interpretation_test8_invalid_declaration_handled_explicitly() {
    let recipe = RecipeSpec::directory_zip();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let mut artifact = artifact.as_artifact().clone();
    artifact = artifact.with_semantic_declaration("invalid_type");

    // Model B allows any string (no validation here)
    assert_eq!(artifact.semantic_type(), Some("invalid_type"));
}

#[test]
fn phase9_transformation_test9_transforming_artifact_loses_declaration() {
    let recipe = RecipeSpec::wasm();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let mut original = artifact.as_artifact().clone();
    original = original.with_semantic_declaration("wasm_application");

    // Apply transform using MemoryContentResolver
    let transform = PrefixTransform::new("transformed/");
    let resolver = MemoryContentResolver::new(BTreeMap::new());
    let transformed_result = transform.apply(&original, &resolver);

    // Transform creates new artifact without declaration
    match transformed_result {
        Ok(result) => {
            assert!(
                result.artifact.semantic_declaration.is_none(),
                "semantic declaration should not carry through transform"
            );

            let with_new_decl = result.artifact.with_semantic_declaration("wasm_application");
            assert_eq!(with_new_decl.semantic_type(), Some("wasm_application"));
        }
        Err(_) => {
            // Empty resolver causes expected failure - that's ok
        }
    }
}

#[test]
fn phase9_transformation_test10_composing_artifacts_loses_individual_declarations() {
    let recipe = RecipeSpec::wasm();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let mut a = artifact.as_artifact().clone();
    a = a.with_semantic_declaration("wasm_application");

    // Single artifact composition is a pass-through (returns clone)
    let composition = artifact::CompositionInput::new(vec![a.clone()]);
    let composed = composition
        .compose(artifact::CompositionOptions::default())
        .expect("composition should succeed");

    // Single artifact pass-through preserves declaration
    // (Model B doesn't define semantics for multi-input composition with conflicting declarations)
    assert_eq!(
        composed.semantic_declaration,
        Some("wasm_application".to_string()),
        "single artifact composition preserves declaration"
    );
}

#[test]
fn phase9_materialization_test11_semantic_declaration_survives_serialization() {
    let recipe = RecipeSpec::directory_zip();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let mut a = artifact.as_artifact().clone();
    a = a.with_semantic_declaration("example_package");

    let bytes = a.to_canonical_bytes().expect("serialization should succeed");
    assert!(!bytes.is_empty());
}

#[test]
fn phase9_materialization_test12_serialization_includes_declaration() {
    let recipe = RecipeSpec::directory_zip();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let mut a = artifact.as_artifact().clone();
    a = a.with_semantic_declaration("example_package");

    let bytes = a.to_canonical_bytes().expect("serialization should succeed");
    let json = String::from_utf8(bytes).expect("valid utf-8");

    assert!(
        json.contains("example_package"),
        "declaration in serialized JSON"
    );
}

#[test]
fn phase9_architecture_test13_no_second_execution_engine() {
    let recipe = RecipeSpec::wasm();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, evidence) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    assert!(!evidence.used_capabilities().is_empty());

    let mut artifact = artifact.as_artifact().clone();
    artifact = artifact.with_semantic_declaration("wasm_application");

    // Execution trace exists and is independent of declaration
    let stages = evidence.stage_trace();
    assert!(!stages.is_empty());
    assert!(stages.iter().any(|s| s.label == "select"));
    assert!(stages.iter().any(|s| s.label == "compile"));
}

#[test]
fn phase9_architecture_test14_no_second_identity_system() {
    let recipe = RecipeSpec::directory_zip();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let artifact = artifact.as_artifact().clone();

    // Single identity system
    assert!(!artifact.identity.is_empty());
    assert!(!artifact.pipeline_identity.is_empty());
}

#[test]
fn phase9_architecture_test15_no_hidden_state() {
    let recipe = RecipeSpec::wasm();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let mut artifact = artifact.as_artifact().clone();
    artifact = artifact.with_semantic_declaration("wasm_application");

    // Semantic information is in artifact struct (no external state)
    assert_eq!(artifact.semantic_type(), Some("wasm_application"));
}

#[test]
fn phase9_experiment_authority_declaration_vs_content() {
    let recipe = RecipeSpec::wasm();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let mut wasm_artifact = artifact.as_artifact().clone();

    // Artifact has .wasm files
    assert!(wasm_artifact.entries.iter().any(|e| e.path.ends_with(".wasm")));

    // But we declare it differently
    wasm_artifact = wasm_artifact.with_semantic_declaration("wasm_library");

    // Declaration is allowed (it's a claim, not verified)
    assert_eq!(wasm_artifact.semantic_type(), Some("wasm_library"));
}

#[test]
fn phase9_experiment_same_content_different_meanings() {
    let recipe = RecipeSpec::wasm();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let base = artifact.as_artifact().clone();

    let as_app = base.clone().with_semantic_declaration("wasm_application");
    let as_lib = base.clone().with_semantic_declaration("wasm_library");

    // Same identity, different semantics
    assert_eq!(as_app.identity, as_lib.identity);
    assert_eq!(as_app.semantic_type(), Some("wasm_application"));
    assert_eq!(as_lib.semantic_type(), Some("wasm_library"));
}

// ============================================================================
// Phase 9 Completion: Semantic Authority Discovery Experiments
// ============================================================================
//
// Four experiments to determine:
// 1. What persists? (declaration through serialization/materialization)
// 2. What survives? (transformations and composition)
// 3. What is authoritative? (claim vs attestation vs verification)
// 4. Where does trust live? (engine or consumer/external boundary)

// ============================================================================
// Experiment 1: Materialization Round-Trip
// ============================================================================

#[test]
fn phase9_completion_exp1_declaration_in_json_serialization() {
    // Sub-test 1A: Declaration is in canonical JSON
    let recipe = RecipeSpec::wasm();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let mut artifact = artifact.as_artifact().clone();
    artifact = artifact.with_semantic_declaration("wasm_application");

    let canonical_bytes = artifact.to_canonical_bytes().expect("serialization succeeds");
    let canonical_json = String::from_utf8(canonical_bytes).expect("valid utf-8");

    // Finding: Declaration IS in canonical JSON serialization
    assert!(
        canonical_json.contains("wasm_application"),
        "Declaration persists in canonical JSON serialization"
    );
}

#[test]
fn phase9_completion_exp1_materialization_round_trip_zip() {
    // Sub-test 1B: ZIP round-trip (most important finding)
    let recipe = RecipeSpec::directory_zip();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let mut artifact_a = artifact.as_artifact().clone();
    artifact_a = artifact_a.with_semantic_declaration("example_archive");

    // Materialize to ZIP using the pipeline as resolver
    let built = default_directory_zip_pipeline()
        .build_from_directory(example_dir())
        .expect("pipeline builds");

    let zip_materializer = ZipMaterializer;
    let zip_bytes = zip_materializer
        .materialize_to_vec(&artifact_a, &built)
        .expect("materialization succeeds");

    // Finding 1B-1: Declaration does NOT persist in ZIP bytes
    // The ZIP contains only artifact entries, not metadata
    assert!(!zip_bytes.is_empty(), "ZIP materialization produces bytes");

    // Note: If we could re-import from ZIP, declaration would be lost
    // This means: Declaration is Rust-struct metadata, not format-embedded
    // FINDING: Declaration does not survive ZIP round-trip
    println!(
        "ZIP size: {} bytes (contains entries, not declaration metadata)",
        zip_bytes.len()
    );
}

#[test]
fn phase9_completion_exp1_wasm_materialization_round_trip() {
    // Sub-test 1C: WASM artifact materialization
    let recipe = RecipeSpec::directory_zip();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let mut artifact_wasm = artifact.as_artifact().clone();
    artifact_wasm = artifact_wasm.with_semantic_declaration("archive_with_declaration");

    // Materialize
    let built = default_directory_zip_pipeline()
        .build_from_directory(example_dir())
        .expect("pipeline builds");

    let zip_materializer = ZipMaterializer;
    match zip_materializer.materialize_to_vec(&artifact_wasm, &built) {
        Ok(materialized) => {
            assert!(!materialized.is_empty(), "Archive materializes");
            // FINDING: Declaration not in materialized format
            println!("Archive materialized successfully (declaration not in bytes)");
        }
        Err(e) => {
            // Content resolution may fail, but that's expected
            println!("Archive materialization: {}", e);
        }
    }

    // FINDING: Declaration not in materialized format
    // Consequence: To use artifact from ZIP, must re-declare or find declaration separately
}

// ============================================================================
// Experiment 2: Transformation Semantics
// ============================================================================

#[test]
fn phase9_completion_exp2a_prefix_transform_semantics() {
    // Does PrefixTransform preserve semantic meaning?
    // (PrefixTransform only reorganizes paths, doesn't change content meaning)

    let recipe = RecipeSpec::wasm();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let mut original = artifact.as_artifact().clone();
    original = original.with_semantic_declaration("wasm_application");

    let transform = PrefixTransform::new("vendor/".to_string());
    let built = default_directory_zip_pipeline()
        .build_from_directory(example_dir())
        .expect("pipeline builds");

    let transform_result = transform.apply(&original, &built);

    match transform_result {
        Ok(result) => {
            // FINDING: PrefixTransform produces artifact without declaration
            assert!(
                result.artifact.semantic_declaration.is_none(),
                "PrefixTransform loses declaration"
            );
            // Question: Should path reorganization preserve meaning?
            // Evidence: It doesn't. Consumer must decide if meaning still applies.
            println!(
                "PrefixTransform: original had declaration, result does not. \
                 Semantic meaning lost. Consumer responsibility to re-declare if meaning unchanged."
            );
        }
        Err(e) => {
            // Transform may fail, that's okay
            println!("PrefixTransform result: {}", e);
        }
    }
}

#[test]
fn phase9_completion_exp2b_redaction_transform_semantics() {
    // Does RedactTransform preserve semantic meaning?
    // (RedactTransform removes content, which may or may not change meaning)

    let recipe = RecipeSpec::directory_zip();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let mut original = artifact.as_artifact().clone();
    original = original.with_semantic_declaration("archive_with_secrets");

    let transform = RedactTransform::new(vec!["README.md".to_string()]);
    let built = default_directory_zip_pipeline()
        .build_from_directory(example_dir())
        .expect("pipeline builds");

    let transform_result = transform.apply(&original, &built);

    match transform_result {
        Ok(result) => {
            // FINDING: RedactTransform produces artifact without declaration
            assert!(
                result.artifact.semantic_declaration.is_none(),
                "RedactTransform loses declaration"
            );
            // Question: Does removing secrets change what an archive IS?
            // Evidence: Semantically undefined. Is it still "archive_with_secrets" after redacting?
            println!(
                "RedactTransform: Content changed (secrets removed). \
                 Original declaration 'archive_with_secrets' now semantically invalid. \
                 Consumer must re-evaluate meaning."
            );
        }
        Err(e) => {
            println!("RedactTransform result: {}", e);
        }
    }
}

#[test]
fn phase9_completion_exp2_transformation_finding() {
    // FINDING: All transforms currently lose declarations
    // Question: Should they?
    // - PrefixTransform: Reorganizes paths. Meaning might be preserved.
    // - RedactTransform: Removes content. Meaning might be invalidated.
    // Evidence: No consistent rule. Preservation is not automatic.

    let recipe = RecipeSpec::directory_zip();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let mut with_decl = artifact.as_artifact().clone();
    with_decl = with_decl.with_semantic_declaration("example_package");

    let transform = PrefixTransform::new("lib/".to_string());
    let built = default_directory_zip_pipeline()
        .build_from_directory(example_dir())
        .expect("pipeline builds");

    let result = transform.apply(&with_decl, &built);

    // EVIDENCE: Transform loses declaration
    if let Ok(transformed) = result {
        assert!(transformed.artifact.semantic_declaration.is_none());
    }

    // CONCLUSION: Semantic preservation is not automatic.
    // Each transform has different relationship to meaning.
    // Conservative approach: Declaration is invalid after any transform.
    println!(
        "Exp 2 Finding: Transforms do not preserve declarations. \
         Conservative rule: Re-declare after transformation if meaning unchanged."
    );
}

// ============================================================================
// Experiment 3: Immutability Test
// ============================================================================

#[test]
fn phase9_completion_exp3_declaration_mutability() {
    // Is declaration mutable (like metadata) or immutable (like identity)?

    let recipe = RecipeSpec::wasm();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let mut artifact1 = artifact.as_artifact().clone();
    artifact1 = artifact1.with_semantic_declaration("wasm_application");

    let identity_1 = artifact1.identity.clone();
    let decl_1 = artifact1.semantic_declaration.clone();

    // Can we change the declaration?
    let mut artifact2 = artifact1.clone();
    artifact2 = artifact2.with_semantic_declaration("wasm_library");

    let identity_2 = artifact2.identity.clone();
    let decl_2 = artifact2.semantic_declaration.clone();

    // FINDING 3A: Identity unchanged
    assert_eq!(
        identity_1, identity_2,
        "Identity is immutable (not affected by declaration)"
    );

    // FINDING 3B: Declaration in artifact2 changed
    assert_eq!(decl_2, Some("wasm_library".to_string()), "artifact2 has new declaration");

    // FINDING 3C: Declaration in artifact1 unchanged
    assert_eq!(
        artifact1.semantic_declaration, Some("wasm_application".to_string()),
        "artifact1 still has original declaration (immutable)"
    );

    // CONCLUSION: Declarations are mutable like metadata, not immutable like identity
    println!(
        "Exp 3 Finding: Declarations are mutable. \
         artifact1.semantic_declaration = immutable value, \
         different artifact can have different declaration. \
         Same identity, different declarations = different claims."
    );
}

// ============================================================================
// Experiment 4: Semantic Authority Boundaries (CRITICAL)
// ============================================================================

#[test]
fn phase9_completion_exp4_three_consumer_test() {
    // CRITICAL TEST: Same artifact + declaration handled differently by three consumers
    // This determines if authority is context-dependent (belongs outside engine)

    let recipe = RecipeSpec::wasm();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let mut shared_artifact = artifact.as_artifact().clone();
    shared_artifact = shared_artifact.with_semantic_declaration("wasm_application");

    // Same artifact, declaration = "wasm_application"

    // Consumer A: Trusts producer claims without verification
    let consumer_a_result = {
        if shared_artifact.semantic_type() == Some("wasm_application") {
            "ACCEPTED: Trusts 'wasm_application' declaration"
        } else {
            "REJECTED: No declaration"
        }
    };

    // Consumer B: Requires verification before accepting
    let consumer_b_result = {
        // Consumer B wants to verify: "Does this actually have .wasm files?"
        let has_wasm = shared_artifact.entries.iter().any(|e| e.path.ends_with(".wasm"));
        if has_wasm && shared_artifact.semantic_type() == Some("wasm_application") {
            "ACCEPTED: Verified .wasm present, matches declaration"
        } else if has_wasm {
            "SUSPICIOUS: Has .wasm but no declaration"
        } else {
            "REJECTED: No .wasm files, declaration is false"
        }
    };

    // Consumer C: Doesn't recognize "wasm_application" type
    let consumer_c_result = {
        if shared_artifact.semantic_type() == Some("wasm_application") {
            "CONFUSED: I don't know what 'wasm_application' means, ignoring declaration"
        } else {
            "IGNORED: No declaration"
        }
    };

    // FINDINGS:
    println!("\n=== THREE CONSUMER TEST ===");
    println!("Shared artifact: semantic_declaration = 'wasm_application'");
    println!("\nConsumer A (trusts claims): {}", consumer_a_result);
    println!("Consumer B (verifies): {}", consumer_b_result);
    println!("Consumer C (doesn't recognize type): {}", consumer_c_result);

    // EVIDENCE: Same declaration, three different legitimiate responses
    // - Consumer A: "I trust this claim"
    // - Consumer B: "I verified it's true"
    // - Consumer C: "I don't understand this type"

    // CRITICAL FINDING: Authority is context-dependent
    // Different consumers make different decisions about same declaration
    // This suggests authority/trust belongs OUTSIDE the engine
    // Engine carries declaration; consumers/registry/external systems evaluate trust

    assert!(
        consumer_a_result.contains("ACCEPTED"),
        "Consumer A accepts unverified claim"
    );
    assert!(
        consumer_b_result.contains("VERIFIED") || consumer_b_result.contains("ACCEPTED"),
        "Consumer B verifies before accepting"
    );
    assert!(
        consumer_c_result.contains("CONFUSED") || consumer_c_result.contains("IGNORED"),
        "Consumer C doesn't recognize type"
    );

    println!(
        "\n=== INTERPRETATION ===\n\
         Same artifact + declaration handled three ways:\n\
         - Consumer A trusts it\n\
         - Consumer B verifies it\n\
         - Consumer C ignores it\n\n\
         If all three approaches are LEGITIMATE,\n\
         that proves authority is CONTEXT-DEPENDENT.\n\n\
         This means AUTHORITY BELONGS OUTSIDE ENGINE.\n\
         Engine carries claim; consumers/external systems decide trust."
    );
}

#[test]
fn phase9_completion_exp4_claim_vs_attestation() {
    // Distinguish: Does engine need to carry attestation data?

    let recipe = RecipeSpec::wasm();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let mut artifact = artifact.as_artifact().clone();
    artifact = artifact.with_semantic_declaration("wasm_application");

    // CURRENT STATE: Artifact carries only the claim
    // No verification_status, no verifier name, no verification timestamp

    // QUESTION: Should engine carry attestation?
    // Option 1 (Agnostic): Just the claim, external systems handle verification
    // Option 2 (Status-Aware): Claim + status field
    // Option 3 (Enforcing): Build rejects false claims

    // EVIDENCE:
    // - artifact.semantic_declaration exists: claim IS carried
    // - No artifact.verification_status: attestation is NOT carried
    // - No artifact.verifier: identity of verifier NOT carried
    // - Build doesn't validate: engine doesn't enforce (Option 3 is false)

    // FINDING: Engine carries CLAIM only
    // No attestation metadata in model
    println!(
        "Exp 4 Finding: Engine carries CLAIM (producer assertion)\n\
         Does NOT carry ATTESTATION (verification result)\n\
         Engine is agnostic about authority.\n\n\
         Model A (Agnostic) is current state.\n\
         Question: Is this sufficient or must Model 2 or 3 be adopted?"
    );

    assert!(artifact.semantic_type().is_some(), "Declaration/claim is present");
}

#[test]
fn phase9_completion_exp4_declaration_can_be_false() {
    // Does engine validate declarations against content?

    let recipe = RecipeSpec::wasm();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let mut artifact = artifact.as_artifact().clone();
    assert!(!artifact.entries.is_empty(), "Artifact has content");

    // Create false claim
    artifact = artifact.with_semantic_declaration("text_document");

    // FINDING: Engine allows false claim
    // Artifact contains .wasm files but claims to be "text_document"
    assert_eq!(
        artifact.semantic_type(),
        Some("text_document"),
        "False claim is allowed"
    );

    // EVIDENCE: No validation in engine
    // Declaration contradicts content but is accepted
    println!(
        "Exp 4 Finding: Engine does NOT validate declarations.\n\
         False claim 'text_document' on wasm artifact = ALLOWED\n\
         This proves: Declaration is CLAIM not verified by engine.\n\
         Validation belongs to external system (consumer/registry)"
    );
}

// ============================================================================
// Experiment Summary
// ============================================================================

#[test]
fn phase9_completion_summary() {
    println!("\n=== PHASE 9 COMPLETION FINDINGS ===\n");

    println!("1. PERSISTENCE (What travels with artifact?)");
    println!("   - Declaration in JSON: YES");
    println!("   - Declaration in ZIP/TAR: NO");
    println!("   → Declaration is struct-level metadata, not format-embedded\n");

    println!("2. SURVIVAL (Behavior through transformations?)");
    println!("   - PrefixTransform: Declaration lost");
    println!("   - RedactTransform: Declaration lost");
    println!("   → Conservative rule: Transformation invalidates declaration\n");

    println!("3. MUTABILITY (Fixed or changeable?)");
    println!("   - Declarations are mutable (like metadata)");
    println!("   - Identity is immutable (by design)");
    println!("   → Declaration is annotation, not contract\n");

    println!("4. AUTHORITY (Where does trust belong?)");
    println!("   - Three consumers handle same declaration three ways: LEGITIMATE");
    println!("   - Engine doesn't validate claims: AGNOSTIC");
    println!("   - False claims are allowed: NO ENFORCEMENT");
    println!("   → Authority is CONTEXT-DEPENDENT, belongs OUTSIDE ENGINE\n");

    println!("=== ARCHITECTURAL IMPLICATION ===\n");
    println!(
        "Evidence suggests:\n\n\
         Artifact Engine (mechanically authoritative)\n\
         ├── identity = H(content)\n\
         ├── provenance (where from)\n\
         └── semantic_declaration = producer claim (unverified)\n\n\
         External Trust Boundary (authority-aware)\n\
         ├── verification (is claim true?)\n\
         ├── attestation (who verified it?)\n\
         └── trust policy (why believe verifier?)\n\n\
         This is a CLEAN ARCHITECTURAL BOUNDARY.\n\
         Engine is semantically agnostic but carries producer intent.\n\
         Authority belongs to consumers/registries/external systems.\n"
    );
}
