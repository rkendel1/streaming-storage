# Phase 27 Deployment Boundary Audit

## Phase 26B baseline

Phase 26B proved a local runtime contract outside the Artifact Engine kernel:

```text
Materialized Representation
        ↓
Runtime Contract
        ↓
Process
        ↓
Execution Result
```

The audited files are:

- `examples/runtime-contract/`
- `examples/runtime-consumer/`
- `examples/oci-consumer/`
- `tests/phase26b_runtime_contract.rs`

## Exact Phase 26B inputs

### Artifact identity

The runtime input carries the logical artifact identity as `RuntimeExecutionInput::artifact_identity`.
The ZIP and OCI representations also carry the same logical identity.
The runtime contract rejects a mismatch before execution.

### Representation identity

Phase 26B has two representation identities:

- ZIP: `ZipMaterializer::materialize_to_path(...).output_digest`
- OCI: `OciMaterialization::oci_representation_digest`, produced by Docker image inspection

The representation identity is returned in `RuntimeExecutionResult::representation_identity`.

### Execution identity

Execution identity is runtime-owned:

- ZIP execution identity is `zip-runtime-{counter}`.
- OCI execution identity is `artifact-oci-runtime-{counter}` and is also used as the local Docker container name.

Repeated executions produce distinct execution identities without changing artifact identity.

### Entry point

The fixture entry point is `bin/runtime-fixture`.
For OCI execution, the representation entry point and execution input entry point must match.

### Args

`RuntimeExecutionInput::arguments` is the complete argument vector passed to the process or container command.
The Phase 26B tests use an empty argument vector.

### Environment

`RuntimeExecutionInput::environment` is the complete key/value environment list supplied by the runtime contract.
The Phase 26B fixture uses:

- `PHASE26B_RUNTIME_MODE`
- `ARTIFACT_RUNTIME_INPUT`

### stdout

The process stdout is captured into `RuntimeExecutionResult::stdout`.
The Phase 26B success path emits `{ARTIFACT_RUNTIME_INPUT}-runtime`.

### stderr

The process stderr is captured into `RuntimeExecutionResult::stderr`.
The Phase 26B success path emits `phase26b stderr`.
The failure path emits `phase26b failure requested`.

### Exit status

The process exit status is captured into `RuntimeExecutionResult::exit_code`.
The Phase 26B success path returns `Some(0)` and the requested failure path returns `Some(42)`.

## What Phase 26B explicitly does not model

Phase 26B does not model:

- remote execution;
- deployment;
- provider lifecycle;
- credentials;
- provider identity;
- network configuration;
- scaling;
- health;
- registry behavior;
- orchestration;
- durable execution history;
- CPU or memory scheduling policy;
- remote logs beyond the process stdout/stderr captured locally;
- any kernel-owned runtime or deployment framework.

## Kernel boundary before Phase 27

Phase 26B keeps all runtime contract code under `examples/` and tests.
It adds no runtime, Docker, OCI, provider, deployment, registry, lifecycle, or execution-history state to `src/`.
