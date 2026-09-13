# Phase 26B Runtime Contract Proof

## Scope

Phase 26B proves the smallest common external runtime relationship established independently by Phase 24 and Phase 25B:

```text
Materialized Representation
        ↓
Runtime-owned execution inputs
        ↓
Process execution
        ↓
Execution Result
```

This phase keeps the contract outside the Artifact Engine kernel. The implementation lives under `examples/runtime-contract/` and adapts the existing Phase 24 ZIP runtime consumer and Phase 25B OCI consumer without adding runtime, Docker, OCI, deployment, provider, registry, or scheduler types to `src/`.

## Proven

### Common runtime relationship

The external contract in `examples/runtime-contract/src/lib.rs` defines only:

- a materialized representation reference;
- an executable/entry point reference;
- arguments;
- environment;
- invocation of an external runtime consumer;
- a runtime execution result containing runtime execution identifier, logical artifact identity reference, representation identity, exit status, stdout, and stderr.

The shared semantic relationship is proven by `tests/phase26b_runtime_contract.rs` for both ZIP and OCI representations.

### ZIP implementation

`ZipRuntimeContract` adapts the existing Phase 24 `RuntimeConsumer`. It accepts a `ZipRepresentation`, passes the already-materialized ZIP path, entry point, arguments, and environment to `RuntimeConsumer::execute_materialized_zip`, and converts the existing result into the common `RuntimeExecutionResult`.

ZIP-specific behavior remains ZIP-specific:

- ZIP archive extraction;
- runtime-owned temporary staging directory;
- OS process execution from the extracted entry point.

The contract does not require OCI staging to behave like ZIP extraction.

### OCI implementation

`OciRuntimeContract` adapts the existing Phase 25B `OciConsumer`. It accepts an `OciRepresentation`, verifies that the execution input refers to the same logical artifact and entry point, rebuilds the existing `OciMaterialization` reference, and delegates to `OciConsumer::execute`.

OCI-specific behavior remains OCI-specific:

- artifact content staging into an OCI-compatible image;
- Docker image execution as the container runtime proof;
- Docker container name as the runtime execution identifier.

Docker remains an OCI runtime implementation detail, not a deployment abstraction.

### Shared execution inputs

The contract requires only:

- logical artifact identity reference;
- materialized representation reference;
- executable/entry point;
- arguments;
- environment.

No resource scheduling, networking, health, scaling, registry, credentials, rollout, lifecycle, or durable execution history fields are present.

### Shared execution result semantics

The common result records:

- runtime execution identifier;
- logical artifact identity reference;
- representation identity;
- exit status;
- stdout;
- stderr.

`runtime_contract_captures_exit_status` proves non-zero exits are returned as runtime execution results. `runtime_contract_captures_stdout_and_stderr` proves stdout/stderr are captured without turning runtime output into Artifact Engine state.

### Logical identity preservation

`runtime_contract_preserves_artifact_identity` proves that the ZIP and OCI runtime contract paths both return the same logical artifact identity and that recovery after each path still returns the original artifact identity.

### Representation identity separation

The cross-representation proof verifies:

```text
artifact_identity_zip == artifact_identity_oci
representation_identity_zip != representation_identity_oci
```

ZIP representation identity is the `ZipMaterializer` output digest. OCI representation identity is the OCI image digest returned by the external OCI consumer.

### Execution identity separation

The same proof verifies:

```text
execution_id_zip != execution_id_oci
```

`runtime_contract_produces_distinct_execution_ids` also proves repeat executions produce distinct runtime execution identifiers without changing artifact identity.

### Failure isolation

`runtime_contract_does_not_mutate_artifact` runs failing ZIP and OCI executions, verifies non-zero exit status, recovers the persisted artifact, validates content digest/size, and then runs subsequent successful ZIP and OCI executions.

Runtime failure remains runtime failure. It is not converted into Artifact failure, persistence failure, or lineage mutation.

### Recovery

`runtime_contract_supports_recovered_artifact` proves this sequence for both representations:

```text
create Artifact
  ↓
persist
  ↓
recover
  ↓
materialize
  ↓
runtime contract
  ↓
execute
  ↓
recover Artifact again
```

The test verifies logical identity and content integrity after execution. The runtime contract does not become part of Artifact persistence.

### Absence of deployment dependency

`runtime_contract_has_no_deployment_dependency` verifies the external contract and adapted consumers do not require Fly, Render, Kubernetes, AWS, GCP, Azure, registry credentials, cloud credentials, remote deployment IDs, rollout IDs, provider lifecycle, remote health, or scaling.

## Not Proven

Phase 26B intentionally does not prove:

- remote execution;
- deployment;
- provider lifecycle;
- credentials;
- networking;
- scaling;
- health;
- registry behavior;
- orchestration;
- durable execution history;
- CPU or memory scheduling policy;
- remote logs beyond local execution stdout/stderr;
- a kernel-owned runtime framework.

## Kernel Integrity Check

Phase 26B introduces no changes under `src/`.

The phase therefore introduces no:

- runtime identity into Artifact identity;
- runtime state into Artifact persistence;
- provider state into Artifact state;
- deployment state into Artifact lineage;
- execution IDs into logical Artifact identity;
- Docker/OCI types into the Artifact kernel.

The implemented relationship is:

```text
Artifact Engine
    │
    │ materialized representation
    ▼
External Runtime Contract
    │
    ├── ZIP Runtime
    └── OCI Runtime
```

not:

```text
Artifact Engine
    │
    └── Runtime Framework
          ├── Docker
          ├── ZIP
          └── Providers
```

## Files Added

- `examples/runtime-contract/src/lib.rs`
- `examples/runtime-contract/Cargo.toml`
- `tests/phase26b_runtime_contract.rs`

## Final Verdict

RUNTIME CONTRACT PROVEN
