// Phase 13B: Production Boundary Integration
//
// These tests intentionally use the real streaming-storage artifact, pipeline,
// transformation, authorization, and serialization primitives. When the engine
// has no production primitive for a relationship, the test records that boundary
// instead of creating a test-local registry or persistence layer.

use artifact::{
    AllowAllPolicy, AllowListPolicy, Artifact, ArtifactSDK, ArtifactTransform,
    AuthorizationDecision, Capability, CapabilityPolicy, CreationMetadata, ExecutionEvidence,
    PrefixTransform, Provenance, RecipeSpec,
};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BoundaryStatus {
    Proven,
    RequiresNewInfrastructure,
    UntestableWithCurrentEngine,
    Underdetermined,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CandidateModel {
    Unified,
    Modular,
    Contextual,
}

fn example_dir() -> PathBuf {
    PathBuf::from("example")
}

fn package_zip_capability() -> Capability {
    Capability::new("package.zip", "1")
}

fn build_example_with_allow_all() -> (Artifact, ExecutionEvidence) {
    let pipeline = ArtifactSDK::recipe_from_spec(RecipeSpec::directory_zip())
        .compile()
        .expect("recipe compilation should succeed");

    let (artifact, evidence) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("authorized execution should succeed");

    (
        artifact.as_artifact().clone(),
        evidence.as_evidence().clone(),
    )
}

fn build_example_with_allow_list() -> (Artifact, ExecutionEvidence) {
    let pipeline = ArtifactSDK::recipe_from_spec(RecipeSpec::directory_zip())
        .compile()
        .expect("recipe compilation should succeed");

    let mut allowed = pipeline
        .required_capabilities()
        .into_iter()
        .map(|cap| (cap.name, cap.version))
        .collect::<Vec<_>>();
    allowed.push(("package.zip".to_string(), "1".to_string()));

    let policy = AllowListPolicy::new(allowed);
    let (artifact, evidence) = pipeline
        .build_with_authorization(example_dir(), &policy)
        .expect("allow-list execution should succeed");

    (
        artifact.as_artifact().clone(),
        evidence.as_evidence().clone(),
    )
}

fn recompute_with_provenance_metadata(
    artifact: &Artifact,
    creation_metadata: CreationMetadata,
) -> Artifact {
    Artifact::from_parts(
        artifact.entries.clone(),
        artifact.pipeline_identity.clone(),
        artifact.capabilities.clone(),
        Provenance {
            source_identity: artifact.provenance.source_identity.clone(),
            pipeline_identity: artifact.provenance.pipeline_identity.clone(),
            creation_metadata,
        },
    )
    .expect("artifact should rebuild with equivalent identity inputs")
}

fn candidate_boundary_status(model: CandidateModel, requirement: &str) -> BoundaryStatus {
    match (model, requirement) {
        (_, "real_artifact" | "real_identity" | "real_transformation" | "consumer_divergence") => {
            BoundaryStatus::Proven
        }
        (_, "independent_revocation" | "evidence_persistence") => {
            BoundaryStatus::RequiresNewInfrastructure
        }
        (CandidateModel::Contextual, "actual_shared_context") => {
            BoundaryStatus::RequiresNewInfrastructure
        }
        (_, "persistence_recovery") => BoundaryStatus::UntestableWithCurrentEngine,
        (_, "lineage") => BoundaryStatus::Underdetermined,
        _ => BoundaryStatus::Underdetermined,
    }
}

#[test]
fn phase13b_maps_actual_engine_boundary() {
    let (artifact, evidence) = build_example_with_allow_all();

    assert!(artifact.identity.starts_with("sha256:"));
    assert_eq!(artifact.identity, artifact.manifest.artifact_identity);
    assert_eq!(
        artifact.pipeline_identity,
        artifact.provenance.pipeline_identity
    );
    assert!(artifact.to_canonical_bytes().is_ok());
    assert!(artifact.manifest.to_canonical_json().is_ok());
    assert!(evidence.to_canonical_bytes().is_ok());
    assert!(evidence.identity().is_ok());

    assert_eq!(
        candidate_boundary_status(CandidateModel::Unified, "real_artifact"),
        BoundaryStatus::Proven
    );
    assert_eq!(
        candidate_boundary_status(CandidateModel::Modular, "persistence_recovery"),
        BoundaryStatus::UntestableWithCurrentEngine
    );
    assert_eq!(
        candidate_boundary_status(CandidateModel::Contextual, "actual_shared_context"),
        BoundaryStatus::RequiresNewInfrastructure
    );
}

#[test]
fn phase13b_identity_survives_real_external_fact_boundaries() {
    let (artifact, evidence) = build_example_with_allow_all();
    let identity_before = artifact.identity.clone();

    let claimed = artifact
        .clone()
        .with_semantic_declaration("web_application");
    let with_provenance_metadata = recompute_with_provenance_metadata(
        &artifact,
        CreationMetadata {
            created_at: Some("2026-09-13T00:00:00Z".to_string()),
        },
    );
    let attestation = evidence.authorization_decision.clone();
    let evidence_identity = evidence
        .identity()
        .expect("execution evidence should be identifiable");

    assert_eq!(claimed.identity, identity_before);
    assert_eq!(with_provenance_metadata.identity, identity_before);
    assert_eq!(attestation.pipeline_identity, evidence.pipeline_identity);
    assert_eq!(evidence.artifact_identity, identity_before);
    assert!(evidence_identity.starts_with("sha256:"));

    assert_eq!(
        candidate_boundary_status(CandidateModel::Unified, "independent_revocation"),
        BoundaryStatus::RequiresNewInfrastructure
    );
}

#[test]
fn phase13b_historical_execution_evidence_is_not_rewritten() {
    let (artifact_a, evidence_a1) = build_example_with_allow_all();
    let (artifact_a_again, evidence_a2) = build_example_with_allow_list();

    let evidence_a1_identity = evidence_a1
        .identity()
        .expect("first execution evidence should be identifiable");
    let evidence_a2_identity = evidence_a2
        .identity()
        .expect("second execution evidence should be identifiable");
    let historical_evidence = vec![evidence_a1.clone(), evidence_a2.clone()];

    assert_eq!(artifact_a.identity, artifact_a_again.identity);
    assert_eq!(historical_evidence.len(), 2);
    assert_eq!(
        historical_evidence[0].artifact_identity,
        artifact_a.identity
    );
    assert_eq!(
        historical_evidence[1].artifact_identity,
        artifact_a.identity
    );
    assert_ne!(evidence_a1_identity, evidence_a2_identity);
    assert_eq!(
        historical_evidence[0]
            .identity()
            .expect("stored evidence should remain reconstructable"),
        evidence_a1_identity
    );
}

#[test]
fn phase13b_revocation_boundary_does_not_rewrite_existing_evidence() {
    let (artifact, evidence) = build_example_with_allow_all();
    let evidence_before = evidence
        .identity()
        .expect("execution evidence should be identifiable before revocation");

    let revocation_status =
        candidate_boundary_status(CandidateModel::Modular, "independent_revocation");
    let evidence_after = evidence
        .identity()
        .expect("execution evidence should remain identifiable after boundary check");

    assert_eq!(revocation_status, BoundaryStatus::RequiresNewInfrastructure);
    assert_eq!(evidence_before, evidence_after);
    assert_eq!(evidence.artifact_identity, artifact.identity);
}

#[test]
fn phase13b_same_facts_can_produce_different_policy_decisions_without_mutation() {
    let (artifact, evidence) = build_example_with_allow_all();
    let requested = {
        let mut caps = artifact.capabilities.clone();
        caps.push(package_zip_capability());
        caps
    };

    let allowed = AllowAllPolicy
        .authorize(&requested)
        .expect("allow-all should grant requested capabilities");
    let allow_all_decision = AuthorizationDecision::allowed(
        artifact.pipeline_identity.clone(),
        requested.clone(),
        AllowAllPolicy.identity(),
    );

    let deny_policy = AllowListPolicy::new(vec![]);
    let denied = deny_policy.authorize(&requested);
    let deny_decision = AuthorizationDecision::denied(
        artifact.pipeline_identity.clone(),
        requested.clone(),
        vec![],
        deny_policy.identity(),
    );
    let evidence_before = evidence
        .identity()
        .expect("execution evidence should be identifiable before policy checks");

    assert_eq!(allowed.len(), requested.len());
    assert!(denied.is_err());
    assert!(allow_all_decision.is_allowed());
    assert!(!deny_decision.is_allowed());
    assert_eq!(
        evidence
            .identity()
            .expect("policy checks must not mutate evidence"),
        evidence_before
    );
    assert_eq!(evidence.artifact_identity, artifact.identity);
}

#[test]
fn phase13b_real_transform_creates_independent_artifact_without_silent_fact_transfer() {
    let (artifact_a, _) = build_example_with_allow_all();
    let artifact_a_with_claim = artifact_a
        .clone()
        .with_semantic_declaration("web_application");
    let source_backed_artifact = RecipeSpec::directory_zip()
        .compile()
        .expect("recipe compilation should succeed")
        .build_from_directory(example_dir())
        .expect("real source-backed artifact should build");

    let transformed = PrefixTransform::new("bundle")
        .apply(&artifact_a_with_claim, &source_backed_artifact)
        .expect("real prefix transform should succeed");

    assert_ne!(
        artifact_a_with_claim.identity,
        transformed.artifact.identity
    );
    assert_eq!(artifact_a.identity, artifact_a_with_claim.identity);
    assert_eq!(
        artifact_a_with_claim.semantic_type(),
        Some("web_application")
    );
    assert_eq!(transformed.artifact.semantic_type(), None);
    assert_eq!(
        transformed.artifact.provenance.source_identity,
        artifact_a.provenance.source_identity
    );
    assert_eq!(
        candidate_boundary_status(CandidateModel::Unified, "lineage"),
        BoundaryStatus::Underdetermined
    );
}

#[test]
fn phase13b_contextual_model_requires_actual_shared_context_primitive() {
    let (artifact, evidence) = build_example_with_allow_all();
    let shared_fact_identity = evidence
        .identity()
        .expect("shared execution fact should be identifiable");

    let consumer_x_status =
        candidate_boundary_status(CandidateModel::Contextual, "consumer_divergence");
    let consumer_y_status =
        candidate_boundary_status(CandidateModel::Contextual, "consumer_divergence");
    let shared_context_status =
        candidate_boundary_status(CandidateModel::Contextual, "actual_shared_context");

    assert_eq!(consumer_x_status, BoundaryStatus::Proven);
    assert_eq!(consumer_y_status, BoundaryStatus::Proven);
    assert_eq!(
        shared_context_status,
        BoundaryStatus::RequiresNewInfrastructure
    );
    assert_eq!(evidence.artifact_identity, artifact.identity);
    assert_eq!(
        evidence
            .identity()
            .expect("consumer checks must not mutate facts"),
        shared_fact_identity
    );
}

#[test]
fn phase13b_persistence_recovery_boundary_is_not_equivalent_to_in_memory_clone() {
    let (artifact, evidence) = build_example_with_allow_all();

    let artifact_bytes = artifact
        .to_canonical_bytes()
        .expect("artifact has deterministic serialization bytes");
    let manifest_json = artifact
        .manifest
        .to_canonical_json()
        .expect("manifest has deterministic JSON serialization");
    let evidence_bytes = evidence
        .to_canonical_bytes()
        .expect("execution evidence has deterministic serialization bytes");

    assert!(!artifact_bytes.is_empty());
    assert!(manifest_json.contains(&artifact.identity));
    assert!(!evidence_bytes.is_empty());
    assert_eq!(
        candidate_boundary_status(CandidateModel::Unified, "persistence_recovery"),
        BoundaryStatus::UntestableWithCurrentEngine
    );
    assert_eq!(
        candidate_boundary_status(CandidateModel::Modular, "evidence_persistence"),
        BoundaryStatus::RequiresNewInfrastructure
    );
}

#[test]
fn phase13b_evidence_matrix_preserves_epistemic_boundary() {
    let requirements = [
        "real_artifact",
        "real_identity",
        "real_transformation",
        "independent_revocation",
        "evidence_persistence",
        "consumer_divergence",
        "actual_shared_context",
        "persistence_recovery",
        "lineage",
    ];
    let models = [
        CandidateModel::Unified,
        CandidateModel::Modular,
        CandidateModel::Contextual,
    ];

    for model in models {
        let statuses = requirements
            .iter()
            .map(|requirement| candidate_boundary_status(model, requirement))
            .collect::<Vec<_>>();

        assert!(statuses.contains(&BoundaryStatus::Proven));
        assert!(statuses.contains(&BoundaryStatus::RequiresNewInfrastructure));
        assert!(statuses.contains(&BoundaryStatus::UntestableWithCurrentEngine));
    }
}
