# Phase 27C Deployment Model Audit

## Objective

Determine what deployment relationships are already logically required by the proven Artifact/materialization/runtime boundaries, and what remains provider-dependent hypothesis that cannot be honestly proven without a real remote target.

## Final verdict

**DEPLOYMENT MODEL UNDERDETERMINED**

Phase 26B proved only:

```text
Materialized Representation
        ↓
Runtime Contract
        ↓
Process
        ↓
Execution Result
```

Phase 27A/27B introduced a useful external deployment experiment design, but not universal deployment evidence. The current repository evidence does **not** justify a deployment contract in `src/`.

## Audited contracts

The audit is grounded in the actual exposed or proven interfaces already present in the repository:

- `Artifact` (`src/core/mod.rs`)
- materialization as an operation, not a shared kernel type
- `MaterializationResult` (`src/core/mod.rs`)
- `LocalArtifactStore` (`src/storage.rs`)
- `RecoveredArtifact` (`src/storage.rs`)
- `RuntimeExecutionInput` (`examples/runtime-contract/src/lib.rs`)
- `RuntimeExecutionResult` (`examples/runtime-contract/src/lib.rs`)
- `ZipRuntimeContract` (`examples/runtime-contract/src/lib.rs`)
- `OciRuntimeContract` (`examples/runtime-contract/src/lib.rs`)
- `OciMaterialization` (`examples/oci-consumer/src/lib.rs`)

Important negative evidence: there is currently **no** generic `Deployment`, `Provider`, `DeploymentId`, `ProviderId`, or remote execution kernel type in `src/`.

## Proven ownership boundary

### Artifact Engine owns

- **logical identity** — `Artifact.identity`
- **content** — `Artifact.entries` plus digest/size validation
- **lineage** — `Artifact.lineage`
- **composition** — composition produces another ordinary `Artifact`
- **persistence** — `LocalArtifactStore::persist`
- **recovery** — `LocalArtifactStore::recover` and `RecoveredArtifact`
- **materialization** — materializers emit format-specific bytes and `MaterializationResult`

### Runtime owns

- **representation acquisition** — `ZipRepresentation`/`OciRepresentation`
- **staging** — ZIP extraction and OCI image preparation/loading are runtime-side concerns
- **entry point** — `RuntimeExecutionInput::executable_relative_path`
- **arguments** — `RuntimeExecutionInput::arguments`
- **environment** — `RuntimeExecutionInput::environment`
- **process execution** — external runtime consumer invocation
- **stdout** — `RuntimeExecutionResult::stdout`
- **stderr** — `RuntimeExecutionResult::stderr`
- **exit status** — `RuntimeExecutionResult::exit_code`
- **execution identity** — `RuntimeExecutionResult::runtime_execution_identifier`
- **ephemeral runtime state** — runtime directories, containers, and other execution-local state

## What appears only when deployment is imagined

These concerns do not exist in the proven kernel/runtime contract and appear only once a remote provider is hypothesized:

- representation transport
- provider identity
- deployment identity
- remote environment selection
- credentials
- remote lifecycle state
- remote logs/metadata beyond observed execution output
- rollout, scaling, health, scheduling, and persistence policy

## Deployment relationship graph

Baseline:

```text
Artifact
   │
   ▼
Representation
```

Relationship audit:

| Proposed relationship | Current evidence | Status | Reason |
| --- | --- | --- | --- |
| `Artifact -> Representation` | Phase 25A + materializers + `MaterializationResult` | **PROVEN** | One logical artifact can produce concrete materialized outputs. |
| `Representation -> Runtime Execution` | Phase 24 + 25B + 26B | **PROVEN** | ZIP and OCI representations are executable through the external runtime contract. |
| `Representation -> Transport` | Phase 27 SSH Docker path design | **HYPOTHESIS** | A transport path is imaginable, but not universally proven and not observed without a real target. |
| `Representation -> Deployment` | Phase 27 SSH Docker `docker create` candidate | **HYPOTHESIS** | A provider may materialize deployable remote state from a representation, but that is not yet a general fact. |
| `Transport -> Remote Environment` | SSH Docker path only | **PROVIDER-SPECIFIC** | `docker save -> scp -> docker load` is one possible path, not a universal deployment rule. |
| `Deployment -> Remote Runtime` | SSH Docker path only | **HYPOTHESIS** | A deployment may prepare or reference a remote runtime, but the relationship is still unproven generally. |
| `Deployment -> Execution` | Docker `create/start` candidate split | **HYPOTHESIS** | The repository proves execution, not deployment. Separation is plausible but not yet universal evidence. |
| `Remote Runtime -> Execution` | No real target exercised in this environment | **UNKNOWN** | Remote execution remains unproven here. |

## Representation transport is not deployment

Phase 27's hypothetical SSH flow is:

```text
OCI image
   ↓
docker save
   ↓
scp
   ↓
docker load
```

This supports only a **possible transport path** for a representation.

It does **not** prove that:

```text
Representation Transport
        =
Deployment
```

The first provider-specific candidate for deployment state in that flow is `docker create`, not `scp` or `docker load`.

Therefore:

```text
Representation Transport
        ≠
Deployment
```

unless future provider evidence proves otherwise.

## Deployment is not execution

The current proven boundary ends at execution.

The SSH Docker experiment suggests this possible provider-specific sequence:

```text
docker create
      ↓
deployment state
      ↓
docker start
      ↓
execution
```

That is analytically useful, but it is still not a universal deployment fact. The current repository evidence supports only:

```text
Representation
      ↓
Execution
```

through an external runtime contract.

Therefore:

```text
Deployment
    ≠
Execution
```

is the safest current architectural conclusion, with deployment-as-persistent-state remaining **not proven** until a real provider is exercised.

## Identity analysis

```text
Artifact Identity
        │
        ▼
Representation Identity
        │
        ▼
??? Deployment Identity
        │
        ▼
Execution Identity
```

| Identity | Exposed today? | Durable? | Owner | Deterministic? | Survives representation changes? | Shared by multiple executions? | Lifecycle semantics? | Status |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Artifact identity | Yes | Yes | Artifact Engine | Yes | Yes | Yes | No provider lifecycle | **PROVEN** |
| Representation identity | Yes | Representation-specific | Materializer / external representation producer | Yes per representation path | No | Yes | No provider lifecycle | **PROVEN** |
| Deployment identity | No production interface | Unknown | Provider candidate only | Unknown | Unknown | Unknown | Candidate only | **NOT PROVEN** |
| Execution identity | Yes | No, execution-scoped | Runtime | No | N/A | No | Per-execution only | **PROVEN** |

The Phase 27 adapter's Docker container id is only a provider-specific candidate for deployment identity. It is not a kernel fact.

## Deployment lifecycle analysis

Candidate states:

```text
unknown
   ↓
requested
   ↓
materialized
   ↓
running
   ↓
stopped
   ↓
removed
```

Audit result:

- `unknown` and `requested` are plausible analytical states, but not exposed in production code.
- `materialized`, `running`, `stopped`, and `removed` align closely with Docker/container behavior.
- nothing in the current kernel/runtime proofs shows that this is a universal deployment lifecycle.

Therefore:

**Docker lifecycle ≠ proven universal deployment lifecycle**

## Credentials boundary

Candidate relationship:

```text
Credential
    ↓
Provider access
    ↓
Deployment operation
```

Current evidence supports only that credentials would be consumed externally by a provider/runtime integration.

Current conclusion:

- Artifact Engine does **not** need to know a credential exists.
- `RuntimeExecutionInput` does not model credentials.
- `Artifact`, persistence, recovery, and materialization do not model credentials.

Status: **EXTERNAL**

## Remote state audit

Candidate remote state:

- deployment state
- runtime state
- execution state
- logs
- provider metadata

Compare that with Artifact Engine durable state:

- artifact identity
- manifest
- entries
- provenance
- lineage
- durable stored content

Current conclusion:

```text
Remote Deployment State
        ≠
Artifact State
```

No existing evidence requires provider state to become artifact state, artifact lineage, or artifact persistence.

## Smallest hypothetical deployment contract

Analytical minimum only:

```text
Deployment Request
    representation
    target
    runtime inputs
```

```text
Deployment Result
    provider identity
    deployment identity
    observed lifecycle
```

This is a **candidate representation only — not an implementation decision**.

The current evidence does not justify new Rust structs in `src/`, nor a selected provider abstraction.

## Concern summary

| Concern | Current evidence | Status |
| --- | --- | --- |
| Artifact identity | Phase 16+ kernel evidence | **PROVEN** |
| Representation identity | 25A / 25B / 26B | **PROVEN** |
| Runtime execution | 24 / 25B / 26B | **PROVEN** |
| Artifact transport | SSH experiment design | **HYPOTHESIS** |
| Deployment identity | Docker container candidate | **HYPOTHESIS** |
| Remote execution | No real target in this environment | **NOT PROVEN** |
| Deployment lifecycle | Docker-specific design | **PROVIDER-SPECIFIC** |
| Credentials | SSH requirement only | **EXTERNAL** |
| Registry | Not required by SSH path | **PROVIDER/PATH-SPECIFIC** |
| Remote logs | Docker logs candidate | **NOT PROVEN** |
| Scaling | None | **NOT PROVEN** |
| Health | None | **NOT PROVEN** |
| Rollout | None | **NOT PROVEN** |
| Deployment persistence | Docker container suggests possible persistence | **NOT PROVEN UNIVERSALLY** |

## Kernel boundary protection

Phase 27C does not add deployment concepts to `src/`.

The correct current architecture remains:

```text
Artifact Engine
    │
    ▼
Representation
    │
    ▼
External runtime / external provider experiments
```

not:

```text
Artifact Engine
    │
    └── Deployment framework
          ├── Provider
          ├── Deployment
          └── Remote execution state
```

## Conclusion

The repository now has enough evidence to say:

- deployment-related relationships can be **named and audited**;
- transport, deployment, and execution should remain **separate analytical questions**;
- deployment identity, lifecycle, credentials, and remote state remain **outside** the proven kernel boundary;
- a real provider is still required to turn these hypotheses into evidence.

The correct result of Phase 27C is therefore:

**DEPLOYMENT MODEL UNDERDETERMINED**
