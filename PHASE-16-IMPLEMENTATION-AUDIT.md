# Phase 16 — Implementation Audit

Phase 16 starts from the correction that Phase 13B was the last phase with substantive production-engine tests. Phase 14 was boundary analysis and Phase 15 was kernel definition; neither is implementation validation.

This audit inspects production code only. Phase 13 test-local records are not implementation substitutes.

## Classification key

1. Already implemented correctly
2. Partially implemented
3. Implemented but violates Phase 15
4. Missing and required
5. Missing but explicitly external/non-goal

## Concept audit

| Phase 15 concept | Classification | Production evidence | Finding |
| -- | -- | -- | -- |
| Artifact | 1 | `src/core/mod.rs:165`, `src/core/mod.rs:187` | `Artifact` is the production logical artifact with identity, entries, manifest, pipeline identity, capabilities, provenance, and explicit non-identity lineage/fact metadata. |
| Artifact Identity | 1 | `src/core/mod.rs:342`, `src/core/mod.rs:362` | `compute_artifact_identity` hashes normalized entries, pipeline identity, canonical capabilities, and provenance source identity only. Volatile metadata, lineage, facts, evidence, and materialization output are excluded. |
| Pipeline Identity | 1 | `src/pipeline/mod.rs:345`, `src/pipeline/mod.rs:353`, `src/pipeline/mod.rs:358` | `PipelineSpec::identity` deterministically hashes the canonical pipeline spec, including source, ordered stages, materializer, and canonical capabilities. |
| Transformation | 1 | `src/transforms/mod.rs:9`, `src/transforms/mod.rs:25`, `src/transforms/mod.rs:98`, `src/transforms/mod.rs:162` | The production `ArtifactTransform` trait and built-in transforms produce new `Artifact` values. Phase 16 added deterministic transform identity and explicit transformation records. |
| Provenance | 1 | `src/core/mod.rs:130`, `src/core/mod.rs:138`, `src/transforms/mod.rs:85` | Creation provenance records source identity, pipeline identity, and descriptive creation metadata. Transformation lineage is now observable as separate non-identity metadata, not merged with claims or evidence. |
| Content | 1 | `src/core/mod.rs:117`, `src/pipeline/mod.rs:593`, `src/pipeline/mod.rs:607`, `src/pipeline/mod.rs:880`, `src/pipeline/mod.rs:902` | Entries carry path/type/size/content digest; content bytes are resolved through production `ContentResolver` implementations rather than being embedded in identity. |
| Materialization | 1 | `src/core/mod.rs:179`, `src/materializers/zip.rs:19`, `src/materializers/zip.rs:22`, `src/materializers/tar.rs:8`, `src/materializers/tar.rs:11` | ZIP/TAR materializers emit physical bytes. `MaterializationResult` records artifact identity and separate output digest/format/size. |
| Claims | 2 | `src/core/mod.rs:175`, `src/core/mod.rs:227` | `semantic_declaration` is a minimal artifact-associated claim-like metadata field and does not affect identity. A durable claim/fact model is not implemented and remains external by Phase 15. |
| Attestations | 5 | `src/authorization/mod.rs:7`, `src/authorization/mod.rs:168` | The engine has authorization decisions and execution evidence for engine operations, but no general attestation primitive, signer, verifier, PKI, or custody model. Phase 15 keeps these external. |
| Evidence | 1 / 5 | `src/authorization/mod.rs:168`, `src/authorization/mod.rs:292`, `src/authorization/mod.rs:308` | Engine-performed operation evidence exists as `ExecutionEvidence`. External evidence custody and generalized evidence registries are external/non-goals. |
| Revocation | 5 | `PHASE-15-KERNEL.md:47`, `PHASE-15-KERNEL.md:214` | No production revocation primitive exists. Phase 15 states revocation authority and durable revocation facts are not current engine primitives. |
| Policy/consumer interpretation | 5 | `src/authorization/mod.rs:98`, `PHASE-15-KERNEL.md:175`, `PHASE-15-KERNEL.md:197` | `CapabilityPolicy` authorizes engine execution capabilities only. Consumer accept/reject/ignore policy remains contextual and outside artifact state. |

## Minimum production kernel selected

The minimum real kernel for Phase 16 is:

```text
Source
  -> Pipeline
  -> Logical Artifact
  -> Transformation
  -> New Logical Artifact
  -> Materialization
```

Production already supplied deterministic artifacts, pipeline identity, creation provenance, content resolvers, materializers, and engine execution evidence. The smallest missing production semantic was an observable transformation relationship from input artifact to output artifact. Phase 16 implements that relationship as non-identity lineage metadata on the output artifact.

## Explicit non-goals preserved

Phase 16 does not add a registry, database, persistence service, distributed storage, trust service, PKI, signature service, revocation database, authorization provider, consumer policy engine, remote execution framework, background workers, hidden caches, sidecars, or global mutable state.
