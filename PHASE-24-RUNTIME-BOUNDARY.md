# Phase 24 Runtime Consumer Boundary

## Scope

This phase proves that a logical Artifact produced by the Artifact Engine can be materialized and then consumed by an external runtime process without importing runtime semantics into the Artifact Engine.

Execution chain under test:

Artifact Engine → logical Artifact → engine-owned materialization → runtime-owned materialized representation → external process → execution result

## Proven

- Artifact → materialization using production `RecipeSpec::directory_zip()` and `ZipMaterializer`.
- Materialization → runtime handoff via runtime-owned ZIP extraction in `examples/runtime-consumer/src/lib.rs`.
- Real process execution by spawning an extracted executable over an OS process boundary.
- Artifact identity preservation before and after runtime execution.
- Runtime state separation through ephemeral runtime directories and runtime execution identifiers.
- Runtime failure isolation (non-zero process exit does not mutate artifact/store semantics).
- Repeatable execution of the same artifact with stable artifact identity and distinct runtime execution IDs.
- Recovery after execution through a fresh `LocalArtifactStore` and re-materialization.
- Provider neutrality: runtime consumer source uses local filesystem/process primitives only.
- No duplicated kernel semantics: runtime consumer does not own artifact identity/serialization/persistence/lineage/recovery.

## Not Proven

- OCI/container representation.
- Docker daemon integration.
- Provider deployments (Fly, Render, Kubernetes, or cloud APIs).
- Remote runtime orchestration, scheduling, or execution persistence.

## Materialization Boundary Clarification

ZIP in this proof is a materialization format produced by Artifact Engine. Runtime extraction/interpretation is runtime behavior, not an Artifact Engine execution semantic.

## Evidence Matrix

| Boundary | Evidence | Result |
| --- | --- | --- |
| Artifact → materialization | `tests/phase24_runtime_boundary.rs::runtime_executes_materialized_artifact` | PROVEN |
| Materialization → runtime | `tests/phase24_runtime_boundary.rs::runtime_executes_materialized_artifact`, `examples/runtime-consumer/src/lib.rs::extract_zip` | PROVEN |
| Real process execution | `tests/phase24_runtime_boundary.rs::runtime_executes_materialized_artifact` | PROVEN |
| Identity preservation | `tests/phase24_runtime_boundary.rs::runtime_does_not_change_artifact_identity` | PROVEN |
| Runtime state separation | `tests/phase24_runtime_boundary.rs::runtime_state_is_ephemeral` | PROVEN |
| Runtime failure isolation | `tests/phase24_runtime_boundary.rs::runtime_failure_does_not_corrupt_artifact` | PROVEN |
| Repeat execution | `tests/phase24_runtime_boundary.rs::same_artifact_can_execute_multiple_times` | PROVEN |
| Recovery after execution | `tests/phase24_runtime_boundary.rs::artifact_recovers_after_execution` | PROVEN |
| Provider neutrality | `tests/phase24_runtime_boundary.rs::runtime_has_no_provider_dependency` | PROVEN |
| No duplicated kernel semantics | `examples/runtime-consumer/src/lib.rs` source audit | PROVEN |

## Final Verdict

RUNTIME BOUNDARY PROVEN
