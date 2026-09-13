# Phase 25B OCI Consumer Proof

## Scope

This phase proves a second runtime-consumption path for one logical Artifact identity:

Logical Artifact → OCI-compatible image materialization → generic container runtime (Docker) → external process → execution result.

The Artifact Engine remains the owner of logical identity, content, persistence, lineage, and recovery semantics.

## Proven

- Real Artifact Engine construction via production `RecipeSpec::directory_zip()` and `PipelineSpec::build_from_directory`.
- OCI-compatible physical representation materialization through an external consumer (`examples/oci-consumer/src/lib.rs`) that stages Artifact content and builds a container image with Docker.
- Real container-runtime execution through `docker run` with captured stdout/stderr, exit status, and runtime execution identifier.
- Logical artifact identity preservation across OCI materialization and runtime execution.
- Separate physical representation identity (`oci_representation_digest`) from logical artifact identity.
- Deterministic OCI representation digest for repeated materialization of the same recovered logical artifact in the same environment.
- Failure isolation: non-zero runtime exit does not mutate Artifact identity or corrupt persisted/recovered Artifact content.
- Repeat execution: same Artifact executes multiple times with stable logical identity and distinct runtime execution identifiers.
- Persistence/recovery continuity after OCI execution using `LocalArtifactStore` recovery and content revalidation.
- Cross-representation identity continuity for the same logical Artifact across ZIP runtime consumption and OCI runtime consumption.
- Provider neutrality: OCI consumer contains no Fly/Render/Kubernetes/cloud-provider APIs.

## Not Proven

- Docker production deployment behavior.
- OCI registry push/pull integration.
- Fly deployment.
- Render deployment.
- Kubernetes deployment.
- Remote container execution.
- Distributed scheduling/orchestration.

## Evidence

- `tests/phase25b_oci_consumer.rs::oci_materializes_real_artifact`
- `tests/phase25b_oci_consumer.rs::oci_representation_is_deterministic`
- `tests/phase25b_oci_consumer.rs::oci_runtime_executes_artifact`
- `tests/phase25b_oci_consumer.rs::oci_does_not_change_artifact_identity`
- `tests/phase25b_oci_consumer.rs::same_artifact_has_stable_identity_across_zip_and_oci`
- `tests/phase25b_oci_consumer.rs::oci_runtime_failure_does_not_corrupt_artifact`
- `tests/phase25b_oci_consumer.rs::same_artifact_can_execute_multiple_times`
- `tests/phase25b_oci_consumer.rs::artifact_recovers_after_oci_execution`
- `tests/phase25b_oci_consumer.rs::recovered_content_remains_valid`
- `tests/phase25b_oci_consumer.rs::oci_consumer_has_no_provider_dependency`

## Final Verdict

OCI CONSUMER PROVEN
