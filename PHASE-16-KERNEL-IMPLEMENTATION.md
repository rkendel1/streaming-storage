# Phase 16 — Production Kernel Implementation & Evidence

Phase 16 turns the Phase 15 Artifact Kernel from documentation into production behavior exercised by production tests.

Important correction: Phase 14 and Phase 15 were documentation/analysis phases. They are not implementation validation. Phase 13B was the prior production-test boundary.

## Implemented during Phase 16

- Added `TransformationRecord` to production core artifacts (`src/core/mod.rs`).
- Added non-identity `Artifact::lineage()` and `Artifact::with_transformation_record(...)` accessors (`src/core/mod.rs`).
- Extended the production `ArtifactTransform` trait with deterministic `transform_kind()` and `transform_identity()` (`src/transforms/mod.rs`).
- Recorded input artifact identity, transform identity, and transform kind for `PrefixTransform`, `RedactTransform`, and `GenerateTransform` outputs (`src/transforms/mod.rs`).
- Added serde deserialization for existing kernel value types so production artifact representations can round-trip through JSON while preserving semantic identity (`src/core/mod.rs`).

The lineage record is deliberately outside artifact identity. It makes transformation relationships observable without becoming a registry, history database, or consumer policy system.

## Proven by production tests

`tests/phase16_kernel.rs` exercises real production types and APIs. It proves:

- logically identical source trees produce stable artifact identity;
- meaningful content changes alter artifact identity;
- volatile creation metadata does not alter artifact identity;
- pipeline identity is deterministic and changes when relevant pipeline definition changes;
- materializer kind is part of pipeline identity, but materialized bytes/digests remain distinct from artifact identity;
- production transforms create distinct output artifacts;
- transform lineage from A to B is observable through production `Artifact` data;
- claims represented by semantic declarations do not mutate artifact identity or silently transfer through transforms;
- engine execution evidence references artifacts but has its own identity;
- authorization/policy checks do not mutate artifact identity;
- ZIP and TAR materialization are deterministic physical outputs with separate digests;
- `ContentResolver` remains the content path and is independent of identity calculation;
- JSON round-tripping preserves artifact identity and lineage;
- the engine remains functional without registries, trust systems, policy engines, or fact stores.

## Existing before Phase 16

- `Artifact`, `ArtifactEntry`, `Manifest`, `Provenance`, and `MaterializationResult` existed in `src/core/mod.rs`.
- `PipelineSpec`, `StageSpec`, `ContentResolver`, `SourceBackedArtifact`, `MemoryContentResolver`, and `TransformedContentResolver` existed in `src/pipeline/mod.rs`.
- ZIP and TAR materializers existed in `src/materializers/`.
- Engine execution evidence and execution authorization existed in `src/authorization/mod.rs`.
- Production-boundary tests existed from Phase 13B in `tests/phase13b_production_boundary.rs`.

## External by design

The following remain outside the engine:

- registry behavior;
- databases or durable fact stores;
- PKI, signatures, key management, and trust services;
- revocation authority or revocation distribution;
- consumer accept/reject/ignore policy engines;
- remote storage or distributed synchronization;
- deployment/runtime systems.

## Not yet proven

- Durable recovery of artifacts or fact histories across process/storage boundaries.
- General claim, attestation, revocation, or external evidence custody APIs.
- Consumer-specific policy decisions as persistent historical records.
- Remote execution or distributed artifact synchronization.

## Validation results

Commands run during Phase 16:

```text
cargo test
cargo test --test pipeline transform
cargo test --test phase16_kernel
```

Results observed:

- Baseline `cargo test`: passed, 154 tests passed.
- Focused transform tests: passed, 17 tests passed.
- Phase 16 kernel tests: passed, 6 tests passed.

Repository-wide `cargo fmt --check` currently reports formatting diffs in pre-existing files outside the Phase 16 change set. To keep this change surgical, only Phase 16-touched Rust files were formatted.
