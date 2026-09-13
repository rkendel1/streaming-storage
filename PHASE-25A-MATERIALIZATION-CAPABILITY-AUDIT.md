# Phase 25A Materialization Capability Audit

## Scope

Phase 24 proved one runtime path:

Logical Artifact → ZIP materialization → external runtime process

Phase 25A audits Artifact Engine output/materialization capability as a representation boundary question, not an artifact-type expansion.

## Architectural Rule

One logical `Artifact` identity may be materialized into multiple physical representations.

We preserve:

Artifact → ZIP/TAR/OCI/WASM/... materialization → runtime-specific consumption

We reject:

`ZipArtifact`, `TarArtifact`, `DockerArtifact`, `WasmArtifact` model forks.

## Capability Inventory

| Output / Representation | Why it matters | Current status |
| --- | --- | --- |
| ZIP | portable filesystem bundle | **PROVEN** (Phase 24 runtime consumer) |
| TAR | Unix/container/filesystem interoperability | **PROVEN as engine materialization capability** |
| Directory/tree | direct local runtime consumption | **NOT PROVEN** |
| Executable/binary | direct OS process execution | **PROVEN through materialized bundle execution** |
| OCI image/layout | container runtime interoperability | **NOT PROVEN** |
| OCI-compatible filesystem layer | container construction/composition | **NOT PROVEN** |
| WASM module/component | WASI/sandbox runtime interoperability | **NOT PROVEN** |
| Stream/byte output for external sinks | pipes/object stores/http consumers | **PARTIALLY PROVEN** (`materialize_to_vec`) |
| Platform-specific package | mobile/native distribution | **NOT PROVEN** |

## Evidence

- ZIP/TAR materializers are exposed on the public `artifact` crate API surface.
- Materialization links back to logical identity through `MaterializationResult.artifact_identity`.
- New Phase 25A tests prove:
  - stable logical identity across ZIP and TAR outputs,
  - deterministic materialization per output type,
  - format-specific materialization reporting without changing logical artifact identity.

## Conclusions

1. Artifact Engine already supports multiple physical materializations for one logical artifact identity (ZIP, TAR).
2. Representation breadth and runtime breadth are not equivalent; ZIP/TAR availability does not prove OCI/WASM/runtime adapters.
3. The next implementation proof should target OCI consumer execution while preserving logical identity boundaries.

## Next Phase Sequence

- **Phase 25B:** OCI consumer proof (artifact → OCI representation → container runtime → process).
- **Phase 26:** generalized runtime/deployment adapter contract after multi-representation runtime proofs.
