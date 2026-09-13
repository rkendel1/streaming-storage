# Phase 26A Runtime / Deployment Boundary Audit

## Scope

Phase 26A audits production code and tests from Phases 23B through 25B to determine what runtime and deployment relationships are actually proven before introducing any generalized runtime or deployment abstraction.

This phase does **not** introduce `Runtime`, `RuntimeExecutor`, `Execution`, `Deployment`, `DeploymentAdapter`, `Provider`, registry, scheduler, or equivalent kernel types.

## Evidence Reviewed

- Phase 23B external consumer lifecycle proof: `examples/consumer-proof/src/lib.rs` and `PHASE-23B-COMPLETION.md`.
- Phase 24 ZIP runtime consumer proof: `examples/runtime-consumer/src/lib.rs`, `tests/phase24_runtime_boundary.rs`, and `PHASE-24-RUNTIME-BOUNDARY.md`.
- Phase 25A materialization capability audit: `src/materializers/zip.rs`, `src/materializers/tar.rs`, `tests/phase25_materialization_capability_audit.rs`, and `PHASE-25A-MATERIALIZATION-CAPABILITY-AUDIT.md`.
- Phase 25B OCI consumer proof: `examples/oci-consumer/src/lib.rs`, `tests/phase25b_oci_consumer.rs`, and `PHASE-25B-OCI-CONSUMER-PROOF.md`.
- Kernel/storage boundaries: `src/core/mod.rs`, `src/pipeline/mod.rs`, and `src/storage.rs`.

## Proven

### Phase 23B: artifact to materialization to external consumer

- An external consumer can use production Artifact Engine types without reimplementing artifact identity, transformation, lineage, composition, persistence, recovery, content resolution, or materialization semantics. The consumer proof declares consumer-owned source/build metadata and kernel-owned artifact identity, lineage, persistence, content resolution, and materialization in `examples/consumer-proof/src/lib.rs:1-23`.
- Logical artifact identity is supplied by the kernel and read by the consumer rather than computed by the consumer. This is exercised in `examples/consumer-proof/src/lib.rs:133-166`.
- A consumer can persist and recover an artifact through `LocalArtifactStore`, and recovered identity matches the original identity in `examples/consumer-proof/src/lib.rs:501-556`.
- A consumer can materialize an artifact to ZIP using `ZipMaterializer` without changing the logical artifact identity, and the ZIP digest is a separate physical representation digest in `examples/consumer-proof/src/lib.rs:558-601`.
- Phase 23B proves an external consumer can receive a materialized representation, but it does not prove a runtime execution boundary; its context-boundary test uses a second store instance over a filesystem, not a spawned runtime process (`examples/consumer-proof/src/lib.rs:603-651`).

### Phase 24: artifact to ZIP to runtime-owned staging to OS process to execution result

- Artifact production uses the production recipe pipeline and filesystem source ingestion before runtime execution (`tests/phase24_runtime_boundary.rs:46-58`).
- Artifact Engine materializes a ZIP to an explicit path before runtime handoff (`tests/phase24_runtime_boundary.rs:60-64`).
- The runtime consumer accepts the already-materialized ZIP path, executable relative path, arguments, and environment, then owns extraction into a temporary runtime directory (`examples/runtime-consumer/src/lib.rs:25-48`).
- Runtime execution is a real OS process via `std::process::Command`, and the runtime result captures a runtime execution ID, artifact identity reference, runtime directory, executable path, exit code, stdout, and stderr (`examples/runtime-consumer/src/lib.rs:43-61`).
- Runtime staging is ephemeral: the temporary runtime directory is dropped before the execution result is returned, and the test verifies it no longer exists (`examples/runtime-consumer/src/lib.rs:50-51`, `tests/phase24_runtime_boundary.rs:145-155`).
- Successful execution preserves artifact identity and produces stdout (`tests/phase24_runtime_boundary.rs:114-130`).
- Failed execution captures non-zero exit and stderr without corrupting the persisted artifact or lineage (`tests/phase24_runtime_boundary.rs:157-175`).
- Multiple executions of the same artifact produce the same artifact identity and distinct runtime execution IDs (`tests/phase24_runtime_boundary.rs:177-189`).
- Recovery and rematerialization still work after successful and failed runtime executions (`tests/phase24_runtime_boundary.rs:191-210`).
- Phase 24 explicitly tests absence of deployment/provider terms in the ZIP runtime consumer (`tests/phase24_runtime_boundary.rs:237-258`).

### Phase 25A: artifact to multiple physical representations

- ZIP and TAR materializers both produce `MaterializationResult` values that include the logical `artifact_identity`, a `materializer_format`, an `output_digest`, and `size_bytes` (`src/materializers/zip.rs:32-50`, `src/materializers/tar.rs:21-39`).
- ZIP and TAR materialization both resolve content by artifact entry path through `ContentResolver`, which is the production content access boundary (`src/materializers/zip.rs:53-81`, `src/materializers/tar.rs:42-68`, `src/pipeline/mod.rs:593-604`).
- One logical artifact has stable identity across ZIP and TAR outputs, while each physical output has a separate digest from the artifact identity and from the other representation (`tests/phase25_materialization_capability_audit.rs:48-67`).
- Each output type is deterministic for repeated materialization of the same artifact/content (`tests/phase25_materialization_capability_audit.rs:69-120`).
- Materialization output identity is format-specific and not a replacement for logical artifact identity (`tests/phase25_materialization_capability_audit.rs:122-153`).

### Phase 25B: artifact to OCI representation to container runtime to process to execution result

- OCI materialization is implemented by an external consumer, not by the Artifact Engine kernel. `OciConsumer::materialize_to_oci_image` takes an `Artifact`, a `ContentResolver`, and an executable relative path, stages validated artifact contents, writes a Dockerfile, builds an image, and records `artifact_identity`, `oci_representation_digest`, and `image_reference` (`examples/oci-consumer/src/lib.rs:37-94`).
- OCI staging validates content size and digest against artifact entries before creating the physical image representation (`examples/oci-consumer/src/lib.rs:154-197`).
- Container runtime execution uses `docker run --rm --name`, accepts arguments and environment, and captures runtime execution ID, runtime execution identifier, artifact identity, OCI representation digest, image reference, exit code, stdout, and stderr (`examples/oci-consumer/src/lib.rs:97-133`).
- OCI materialization links back to the artifact identity, and the OCI representation digest is separate from logical artifact identity (`tests/phase25b_oci_consumer.rs:138-154`).
- Repeated OCI materialization of the same recovered artifact in the same environment preserves artifact identity and representation digest (`tests/phase25b_oci_consumer.rs:156-178`).
- OCI runtime execution preserves artifact identity and returns process stdout/exit status (`tests/phase25b_oci_consumer.rs:180-192`).
- Artifact identity remains stable before and after OCI runtime execution (`tests/phase25b_oci_consumer.rs:194-205`).
- The same logical artifact identity is stable across both ZIP and OCI runtime execution paths, while ZIP and OCI representation digests remain distinct (`tests/phase25b_oci_consumer.rs:207-234`).
- OCI runtime failure captures non-zero exit and stderr without corrupting recovered artifact content (`tests/phase25b_oci_consumer.rs:236-268`).
- Multiple OCI executions of the same artifact produce stable artifact identity and distinct runtime execution IDs/identifiers (`tests/phase25b_oci_consumer.rs:270-286`).
- Recovery and rematerialization still work after OCI executions (`tests/phase25b_oci_consumer.rs:288-309`).
- Phase 25B tests absence of hidden cloud/provider deployment terms in the OCI consumer (`tests/phase25b_oci_consumer.rs:336-358`).

## Proven Relationship Graph

The evidence supports this relationship:

```text
Artifact
  materializes as
Physical Representation
  consumed by
Runtime Consumer
  executes
Process
  produces
Execution Result
```

Evidence:

- `Artifact` stores logical identity, entries, manifest, capabilities, provenance, and lineage (`src/core/mod.rs:164-184`).
- `MaterializationResult` links a logical artifact identity to a materializer format, output digest, and size (`src/core/mod.rs:178-184`).
- ZIP/TAR materializers create physical representations from artifacts and content resolvers (`src/materializers/zip.rs:32-81`, `src/materializers/tar.rs:21-68`).
- The ZIP runtime consumer consumes a ZIP file location and executes an OS process (`examples/runtime-consumer/src/lib.rs:25-61`).
- The OCI consumer consumes artifact content into an OCI image representation, then executes that image through Docker as a container runtime (`examples/oci-consumer/src/lib.rs:37-133`).

The evidence does **not** establish this as an Artifact Engine-owned relationship:

```text
Artifact
  deployment
Provider
  remote environment
Execution
```

There is no production deployment adapter, provider API integration, credential flow, remote environment model, rollout lifecycle, or provider-owned execution state in the audited code. Existing documents discuss deployment as external, and Phase 24/25B tests only execute local OS/Docker runtime paths.

## Runtime Boundary

The smallest runtime relationship supported by Phase 24 and Phase 25B evidence is:

```text
Materialized Representation
  consumed by
Runtime Consumer
  executes
Process
  returns
Execution Result
```

### Common runtime concerns proven by both runtime consumers

| Concern | ZIP runtime evidence | OCI runtime evidence | Status |
| --- | --- | --- | --- |
| Representation input | ZIP path passed to `execute_materialized_zip` (`examples/runtime-consumer/src/lib.rs:26-33`) | `OciMaterialization` passed to `execute` (`examples/oci-consumer/src/lib.rs:97-102`) | PROVEN |
| Entry point | Executable relative path argument (`examples/runtime-consumer/src/lib.rs:30-41`) | Executable relative path encoded into Dockerfile entrypoint (`examples/oci-consumer/src/lib.rs:37-49`, `examples/oci-consumer/src/lib.rs:199-210`) | PROVEN |
| Arguments | `command.args(arguments)` (`examples/runtime-consumer/src/lib.rs:43-44`) | `command.args(arguments)` after image reference (`examples/oci-consumer/src/lib.rs:119-120`) | PROVEN |
| Environment | `command.env(key, value)` (`examples/runtime-consumer/src/lib.rs:45-47`) | `docker run --env key=value` (`examples/oci-consumer/src/lib.rs:115-117`) | PROVEN |
| Execution | OS process command output (`examples/runtime-consumer/src/lib.rs:43-48`) | Docker runtime command output (`examples/oci-consumer/src/lib.rs:108-122`) | PROVEN |
| stdout/stderr | Captured in `RuntimeExecution` (`examples/runtime-consumer/src/lib.rs:53-61`) | Captured in `OciRuntimeExecution` (`examples/oci-consumer/src/lib.rs:124-133`) | PROVEN |
| Exit status | Captured as `exit_code` (`examples/runtime-consumer/src/lib.rs:58`) | Captured as `exit_code` (`examples/oci-consumer/src/lib.rs:130`) | PROVEN |
| Runtime execution identifier | Numeric runtime execution ID (`examples/runtime-consumer/src/lib.rs:9-19`, `tests/phase24_runtime_boundary.rs:177-189`) | Numeric ID and Docker container name (`examples/oci-consumer/src/lib.rs:11-31`, `tests/phase25b_oci_consumer.rs:270-286`) | PROVEN |
| Resources | No CPU/memory/resource contract beyond process execution | No CPU/memory/resource contract beyond Docker defaults | NOT PROVEN |

### Constraint

The runtime boundary is independent of artifact identity ownership. Runtime results can reference artifact identity, but neither Phase 24 nor Phase 25B persists runtime execution IDs as artifact identity or mutates recovered artifacts after execution.

### Required relationship

Phase 26B may prove the smallest common runtime contract only if it remains limited to materialized representation consumption, execution inputs, process execution, and result capture. The evidence does not require a deployment contract to be included in that runtime relationship.

## Deployment Boundary

Deployment remains a separate and mostly unproven relationship.

### What deployment evidence exists

- Older product/boundary documents identify deployment policy, rollout, environment trust, credentials, registry/discovery, and provider behavior as external to Artifact Engine, but those are architectural constraints rather than deployment implementation proofs.
- Phase 24 and Phase 25B provider-neutrality tests show runtime consumers do not depend on Fly, Render, Kubernetes, or cloud SDKs (`tests/phase24_runtime_boundary.rs:237-258`, `tests/phase25b_oci_consumer.rs:336-358`).

### What deployment evidence does not exist

No audited production code or test proves:

- provider credentials,
- deployment identity,
- remote environment configuration,
- network setup,
- provider resource allocation,
- lifecycle/rollout/replacement/teardown,
- health checks,
- scaling,
- remote log retrieval,
- registry push/pull,
- provider-specific state,
- remote execution identifiers as deployment identities.

### Separation finding

Runtime execution and deployment can be separated without losing behavior currently demonstrated by Phases 24 and 25B, because all demonstrated behavior occurs through local materialized ZIP execution or local Docker image execution. No demonstrated test requires provider credentials, remote environment lifecycle, rollout, scaling, networking, or deployment identity.

This separation is **permitted by evidence**, but deployment as its own concrete relationship is **not yet proven** by implementation.

## Ownership Matrix

| Concern | Artifact Engine | Runtime | Deployment System | Not Yet Proven |
| --- | --- | --- | --- | --- |
| Logical identity | Owns deterministic artifact identity (`src/core/mod.rs:186-217`; `examples/consumer-proof/src/lib.rs:133-166`) | May reference but not compute/mutate it (`tests/phase24_runtime_boundary.rs:132-143`; `tests/phase25b_oci_consumer.rs:194-205`) | NOT PROVEN | NOT PROVEN |
| Content | Owns entry metadata and content digest contract (`src/core/mod.rs:116-122`; `src/storage.rs:88-123`) | Reads staged/materialized content for execution (`examples/runtime-consumer/src/lib.rs:65-100`; `examples/oci-consumer/src/lib.rs:154-197`) | NOT PROVEN | NOT PROVEN |
| Transformation | Owns transform output/lineage (`examples/consumer-proof/src/lib.rs:395-444`) | NOT PROVEN | NOT PROVEN | NOT PROVEN |
| Lineage | Owns transformation records (`src/core/mod.rs:137-142`, `src/core/mod.rs:219-226`) | Proven not changed by runtime failure (`tests/phase24_runtime_boundary.rs:167-170`) | NOT PROVEN | NOT PROVEN |
| Composition | Owns composition result as normal Artifact (`examples/consumer-proof/src/lib.rs:447-499`) | NOT PROVEN | NOT PROVEN | NOT PROVEN |
| Materialization | Owns ZIP/TAR production (`src/materializers/zip.rs:32-81`; `src/materializers/tar.rs:21-68`) | Consumes materialized ZIP and OCI image representation (`examples/runtime-consumer/src/lib.rs:25-61`; `examples/oci-consumer/src/lib.rs:97-133`) | NOT PROVEN | NOT PROVEN: OCI materialization is external consumer code, not kernel-owned (`examples/oci-consumer/src/lib.rs:37-94`) |
| Representation digest | Reports ZIP/TAR output digest (`src/materializers/zip.rs:45-50`; `src/materializers/tar.rs:34-39`) | OCI consumer records OCI representation digest (`examples/oci-consumer/src/lib.rs:14-19`, `examples/oci-consumer/src/lib.rs:77-94`) | NOT PROVEN | NOT PROVEN: cross-provider representation digest semantics |
| Runtime staging | NOT PROVEN as kernel-owned | Owns ZIP extraction/temp runtime directory and OCI content/image staging (`examples/runtime-consumer/src/lib.rs:34-51`; `examples/oci-consumer/src/lib.rs:45-49`) | NOT PROVEN | NOT PROVEN: durable or remote staging |
| Process execution | NOT PROVEN as kernel-owned | Owns OS/Docker process execution (`examples/runtime-consumer/src/lib.rs:43-48`; `examples/oci-consumer/src/lib.rs:108-122`) | NOT PROVEN | NOT PROVEN: remote process orchestration |
| Execution result | NOT PROVEN as kernel-owned | Owns runtime result structs with IDs, status, stdout, stderr (`examples/runtime-consumer/src/lib.rs:11-20`; `examples/oci-consumer/src/lib.rs:21-31`) | NOT PROVEN | NOT PROVEN: persisted/remote execution result lifecycle |
| Remote deployment | NOT PROVEN | NOT PROVEN | NOT PROVEN | NOT PROVEN: no production deployment proof |
| Provider credentials | NOT PROVEN | NOT PROVEN | NOT PROVEN | NOT PROVEN: no credential flow in audited code |
| Provider lifecycle | NOT PROVEN | NOT PROVEN | NOT PROVEN | NOT PROVEN: no provider lifecycle proof |
| Scaling | NOT PROVEN | NOT PROVEN | NOT PROVEN | NOT PROVEN: no scaling proof |
| Networking | NOT PROVEN | NOT PROVEN | NOT PROVEN | NOT PROVEN: no networking proof |
| Health | NOT PROVEN | NOT PROVEN | NOT PROVEN | NOT PROVEN: no health-check proof |
| Logs | NOT PROVEN | Runtime captures local stdout/stderr (`examples/runtime-consumer/src/lib.rs:58-60`; `examples/oci-consumer/src/lib.rs:130-132`) | NOT PROVEN | NOT PROVEN: remote/provider log ownership |
| Recovery | Owns artifact recovery and content revalidation (`src/storage.rs:44-75`; `src/storage.rs:144-155`; `src/storage.rs:157-218`) | Proven not to corrupt recovery after execution (`tests/phase24_runtime_boundary.rs:191-210`; `tests/phase25b_oci_consumer.rs:288-309`) | NOT PROVEN | NOT PROVEN: runtime/deployment recovery policy |

## Identity Boundaries

The audited evidence supports this semantic invariant:

```text
Logical Artifact identity
  ≠ Materialization identity / representation digest
  ≠ Runtime execution identity
  ≠ Deployment / provider identity
```

- Logical artifact identity is stored on `Artifact.identity` and reflected in `Manifest.artifact_identity` (`src/core/mod.rs:144-184`).
- ZIP/TAR materialization reports retain the artifact identity while producing distinct output digests (`src/materializers/zip.rs:45-50`, `src/materializers/tar.rs:34-39`, `tests/phase25_materialization_capability_audit.rs:48-67`).
- OCI materialization stores both artifact identity and OCI representation digest/image reference, and tests verify the OCI digest differs from artifact identity (`examples/oci-consumer/src/lib.rs:14-19`, `tests/phase25b_oci_consumer.rs:138-154`).
- Runtime execution IDs are generated independently of artifact identity in both runtime consumers (`examples/runtime-consumer/src/lib.rs:9-19`, `examples/oci-consumer/src/lib.rs:11-31`).
- Multiple executions preserve artifact identity while producing distinct runtime execution identifiers (`tests/phase24_runtime_boundary.rs:177-189`, `tests/phase25b_oci_consumer.rs:270-286`).
- No deployment/provider identity exists in the audited production code, so it is distinct by absence rather than by a concrete implemented type.

## Provider Neutrality Audit

Provider neutrality is preserved for the currently proven runtime boundary.

- The Phase 24 runtime consumer depends only on `tempfile` and `zip` and uses filesystem/process primitives (`examples/runtime-consumer/Cargo.toml:6-8`, `examples/runtime-consumer/src/lib.rs:1-7`).
- The Phase 24 provider-neutrality test forbids Docker, OCI, Kubernetes, Fly, Render, AWS, Google Cloud, and Azure terms in the ZIP runtime consumer (`tests/phase24_runtime_boundary.rs:237-258`).
- The Phase 25B OCI consumer uses Docker as a local container runtime implementation detail, while the test forbids Fly, Render, Kubernetes, AWS, Google Cloud, Azure, GCP, EKS, and AKS provider terms (`tests/phase25b_oci_consumer.rs:336-358`).
- No cloud credentials, registry credentials, deployment state, provider lifecycle state, or provider execution IDs are persisted into Artifact Engine identity or storage paths in the audited code.
- Docker-specific assumptions do exist inside the external OCI consumer (`examples/oci-consumer/src/lib.rs:61-78`, `examples/oci-consumer/src/lib.rs:108-122`), but they do not leak into `src/` kernel modules or logical artifact identity.

## Not Proven

- A generalized runtime trait or shared production runtime type.
- A deployment adapter, provider abstraction, registry integration, scheduler, rollout controller, or remote environment model.
- Runtime resource configuration beyond passing arguments/environment and receiving process output.
- Durable runtime execution records.
- Remote execution identifiers and their relationship to local runtime execution IDs.
- Provider credentials, lifecycle, health, scaling, networking, logs, recovery, replacement, teardown, or provider-specific state.
- That Docker image creation should be Artifact Engine materialization rather than external consumer-owned representation construction. Phase 25B proves it as an external OCI consumer path only.

## Open Questions

1. Should Phase 26B prove a small shared runtime contract across ZIP and OCI by adapting existing external consumers without moving runtime types into the kernel?
2. Is OCI image construction a materialization concern that belongs in Artifact Engine, or a consumer-owned packaging step before runtime execution? Existing Phase 25B code proves only the latter.
3. What is the smallest additional deployment experiment that would prove provider identity, credentials, remote lifecycle, and remote logs without collapsing those concerns into runtime execution?
4. Should runtime execution records ever be persisted, and if so, by which external system rather than Artifact Engine identity?

## Phase 26 Recommendation

Phase 26B should prove only the smallest common runtime relationship already supported by Phases 24 and 25B:

```text
Materialized Representation
  -> runtime-owned execution inputs
  -> process execution
  -> execution result
```

The next implementation phase should demonstrate that two existing consumers can share that relationship without adding deployment responsibilities. It should not introduce deployment/provider concepts unless a separate provider experiment proves that deployment requires a distinct contract.

If a deployment proof is desired, it should be a separate follow-up experiment that consumes an already materialized representation through a deployment adapter and proves provider identity, credentials/configuration boundaries, remote lifecycle, and logs independently from runtime execution.

## Final Verdict

RUNTIME/DEPLOYMENT BOUNDARY PROVEN
