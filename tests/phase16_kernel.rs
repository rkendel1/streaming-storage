use artifact::{
    AllowAllPolicy, AllowListPolicy, Artifact, ArtifactTransform, AuthorizationDecision,
    Capability, CapabilityPolicy, CreationMetadata, EntryContentResolver, ExecutionEvidence,
    GenerateTransform, MaterializerSpec, MemoryContentResolver, PipelineSpec, PrefixTransform,
    Provenance, SelectStageSpec, SourceSpec, StageSpec, TarMaterializer, TransformationRecord,
    ZipMaterializer,
};
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

fn rebuild_with_creation_metadata(artifact: &Artifact, created_at: &str) -> Artifact {
    Artifact::from_parts(
        artifact.entries.clone(),
        artifact.pipeline_identity.clone(),
        artifact.capabilities.clone(),
        Provenance {
            source_identity: artifact.provenance.source_identity.clone(),
            pipeline_identity: artifact.provenance.pipeline_identity.clone(),
            creation_metadata: CreationMetadata {
                created_at: Some(created_at.to_string()),
            },
        },
    )
    .expect("artifact should rebuild from production parts")
}

#[test]
fn phase16_artifact_and_pipeline_identity_are_deterministic_kernel_values() {
    let source_a = source_tree(&[("app/main.txt", "hello"), ("README.md", "same")]);
    let source_b = source_tree(&[("README.md", "same"), ("app/main.txt", "hello")]);
    let source_changed = source_tree(&[("app/main.txt", "hello!"), ("README.md", "same")]);

    let pipeline = kernel_pipeline(MaterializerSpec::Zip, Vec::new());
    let same_pipeline_identity = pipeline
        .identity()
        .expect("pipeline identity should be calculable");
    let equivalent_pipeline_identity = kernel_pipeline(MaterializerSpec::Zip, Vec::new())
        .identity()
        .expect("equivalent pipeline identity should be calculable");
    let changed_pipeline_identity = kernel_pipeline(
        MaterializerSpec::Zip,
        vec![StageSpec::Generate(
            artifact::GenerateStageSpec::new("generated.txt", "logical")
                .expect("generate stage should be valid"),
        )],
    )
    .identity()
    .expect("changed pipeline identity should be calculable");
    let changed_materializer_pipeline_identity = kernel_pipeline(MaterializerSpec::Tar, Vec::new())
        .identity()
        .expect("materializer is part of the pipeline specification");

    let artifact_a = pipeline
        .build_from_directory(source_a.path())
        .expect("source A should build");
    let artifact_b = pipeline
        .build_from_directory(source_b.path())
        .expect("source B should build");
    let artifact_changed = pipeline
        .build_from_directory(source_changed.path())
        .expect("changed source should build");
    let artifact_with_physical_metadata =
        rebuild_with_creation_metadata(artifact_a.artifact(), "2026-09-13T19:12:00Z");

    assert_eq!(same_pipeline_identity, equivalent_pipeline_identity);
    assert_ne!(same_pipeline_identity, changed_pipeline_identity);
    assert_ne!(
        same_pipeline_identity,
        changed_materializer_pipeline_identity
    );
    assert_eq!(
        artifact_a.artifact().identity,
        artifact_b.artifact().identity
    );
    assert_ne!(
        artifact_a.artifact().identity,
        artifact_changed.artifact().identity
    );
    assert_eq!(
        artifact_a.artifact().identity,
        artifact_with_physical_metadata.identity
    );
}

#[test]
fn phase16_transformation_produces_observable_lineage_without_fact_transfer() {
    let source = source_tree(&[("app.txt", "hello")]);
    let built = kernel_pipeline(MaterializerSpec::Zip, Vec::new())
        .build_from_directory(source.path())
        .expect("source should build");
    let artifact_a = built.artifact().clone();
    let claimed_a = artifact_a.clone().with_semantic_declaration("application");

    let transform = PrefixTransform::new("bundle");
    let transformed = transform
        .apply(&claimed_a, &built)
        .expect("production prefix transform should succeed");
    let artifact_b = transformed.artifact;

    assert_ne!(artifact_a.identity, artifact_b.identity);
    assert_eq!(claimed_a.identity, artifact_a.identity);
    assert_eq!(claimed_a.semantic_type(), Some("application"));
    assert_eq!(artifact_b.semantic_type(), None);
    assert_eq!(artifact_b.lineage().len(), 1);
    assert_eq!(
        artifact_b.lineage()[0],
        TransformationRecord {
            input_artifact_identity: artifact_a.identity.clone(),
            transform_identity: transform
                .transform_identity()
                .expect("transform identity should be deterministic"),
            transform_kind: "prefix".to_string(),
        }
    );
    assert_eq!(
        artifact_b.provenance.source_identity,
        artifact_a.provenance.source_identity
    );
}

#[test]
fn phase16_provenance_evidence_claims_and_policy_are_not_artifact_identity() {
    let source = source_tree(&[("app.txt", "hello")]);
    let pipeline = kernel_pipeline(MaterializerSpec::Zip, Vec::new());
    let (public_artifact, public_evidence) = artifact::ArtifactSDK::pipeline_from_spec(pipeline)
        .build_with_authorization(source.path(), &AllowAllPolicy)
        .expect("authorized production build should succeed");
    let artifact = public_artifact.as_artifact().clone();
    let evidence = public_evidence.as_evidence().clone();
    let identity = artifact.identity.clone();
    let requested = {
        let mut capabilities = artifact.capabilities.clone();
        capabilities.push(Capability::new("package.zip", "1"));
        capabilities
    };

    let claimed = artifact.clone().with_semantic_declaration("release");
    let attestation = AuthorizationDecision::allowed(
        artifact.pipeline_identity.clone(),
        requested.clone(),
        AllowAllPolicy.identity(),
    );
    let evidence_identity = evidence
        .identity()
        .expect("engine execution evidence should be identifiable");
    let deny_policy = AllowListPolicy::new(Vec::new());
    let denied = deny_policy.authorize(&requested);
    let contextual_decision = AuthorizationDecision::denied(
        artifact.pipeline_identity.clone(),
        requested,
        Vec::new(),
        deny_policy.identity(),
    );

    assert_eq!(claimed.identity, identity);
    assert_eq!(attestation.pipeline_identity, artifact.pipeline_identity);
    assert_eq!(evidence.artifact_identity, identity);
    assert_ne!(evidence_identity, identity);
    assert!(denied.is_err());
    assert!(!contextual_decision.is_allowed());
    assert_eq!(artifact.identity, identity);
    assert_eq!(
        artifact.provenance.pipeline_identity,
        artifact.pipeline_identity
    );
    assert_ne!(artifact.provenance.source_identity, evidence_identity);
}

#[test]
fn phase16_materialization_and_content_resolution_remain_physical_boundaries() {
    let source = source_tree(&[("app.txt", "hello"), ("lib/data.txt", "world")]);
    let built = kernel_pipeline(MaterializerSpec::Zip, Vec::new())
        .build_from_directory(source.path())
        .expect("source should build");
    let artifact = built.artifact().clone();

    let first_zip = ZipMaterializer
        .materialize_to_vec(&artifact, &built)
        .expect("first zip materialization should succeed");
    let second_zip = ZipMaterializer
        .materialize_to_vec(&artifact, &built)
        .expect("second zip materialization should succeed");
    let tar_bytes = TarMaterializer
        .materialize_to_vec(&artifact, &built)
        .expect("tar materialization should succeed");
    let mut resolved = String::new();
    built
        .open("app.txt")
        .expect("production content resolver should open content")
        .read_to_string(&mut resolved)
        .expect("resolved content should be readable");

    let mut memory_contents = BTreeMap::new();
    memory_contents.insert("app.txt".to_string(), b"hello".to_vec());
    memory_contents.insert("lib/data.txt".to_string(), b"world".to_vec());
    let memory_resolver = MemoryContentResolver::new(memory_contents);
    let zip_from_memory = ZipMaterializer
        .materialize_to_vec(&artifact, &memory_resolver)
        .expect("memory content resolver should materialize same logical artifact");

    assert_eq!(first_zip, second_zip);
    assert_eq!(first_zip, zip_from_memory);
    assert_eq!(resolved, "hello");
    assert_ne!(first_zip, tar_bytes);
    assert_ne!(digest_for_bytes(&first_zip), artifact.identity);
    assert_ne!(digest_for_bytes(&tar_bytes), artifact.identity);
    assert_eq!(artifact.identity, built.artifact().identity);
}

#[test]
fn phase16_serialization_round_trip_preserves_semantic_identity() {
    let source = source_tree(&[("app.txt", "hello")]);
    let built = kernel_pipeline(MaterializerSpec::Zip, Vec::new())
        .build_from_directory(source.path())
        .expect("source should build");
    let generated = GenerateTransform::new("generated.txt", "generated")
        .apply(built.artifact(), &built)
        .expect("production generate transform should succeed");
    let artifact = generated.artifact;
    let identity = artifact.identity.clone();
    let lineage = artifact.lineage().to_vec();

    let canonical_bytes = artifact
        .to_canonical_bytes()
        .expect("artifact should serialize through production API");
    let decoded: Artifact =
        serde_json::from_slice(&canonical_bytes).expect("artifact should deserialize");

    assert_eq!(decoded.identity, identity);
    assert_eq!(decoded.manifest.artifact_identity, identity);
    assert_eq!(decoded.lineage(), lineage.as_slice());
    assert_eq!(
        decoded.provenance.source_identity,
        artifact.provenance.source_identity
    );
}

#[test]
fn phase16_engine_remains_functional_without_external_fact_infrastructure() {
    let source = source_tree(&[("app.txt", "hello")]);
    let pipeline = kernel_pipeline(MaterializerSpec::Zip, Vec::new());
    let built = pipeline
        .build_from_directory(source.path())
        .expect("production build should not require a registry or fact store");
    let materialized = ZipMaterializer
        .materialize_to_vec(built.artifact(), &built)
        .expect("production materialization should not require a trust service");
    let evidence = ExecutionEvidence::success(
        built.artifact().pipeline_identity.clone(),
        built.artifact(),
        AuthorizationDecision::allowed(
            built.artifact().pipeline_identity.clone(),
            built.artifact().capabilities.clone(),
            AllowAllPolicy.identity(),
        ),
        Vec::new(),
        built.artifact().capabilities.clone(),
        built.artifact().capabilities.clone(),
    );

    assert!(!materialized.is_empty());
    assert_eq!(evidence.artifact_identity, built.artifact().identity);
    assert!(evidence.validate().is_ok());
}
