# Phase 18 Durability Integrity

Phase 18 validates that artifact persistence is a durable materialization of the Artifact Kernel, not a second semantic model.

## Proven

`tests/phase18_durability_integrity.rs` establishes the Phase 18 production proof set:

- Equivalent logical artifacts built from different source paths and persisted through separate `LocalArtifactStore` roots recover with the same identity and semantic fields (`tests/phase18_durability_integrity.rs:94-121`).
- Persisting, recovering, re-persisting, and recovering a transformed artifact preserves identity, provenance, and the original single transformation record (`tests/phase18_durability_integrity.rs:123-157`).
- Recovery does not create a transformation or mutate lineage; the relationship remains `A --[T]--> B` (`tests/phase18_durability_integrity.rs:123-157`).
- Semantic declarations remain outside the identity contract while surviving persistence/recovery (`tests/phase18_durability_integrity.rs:159-195`).
- Metadata corruption, content substitution, and post-recovery content drift fail closed instead of redefining an artifact (`tests/phase18_durability_integrity.rs:197-273`).
- Durable records can be copied between store roots without changing logical identity, proving storage root is not identity (`tests/phase18_durability_integrity.rs:275-311`).
- Recovered artifacts can be materialized as ZIP and TAR; output digests remain downstream representation digests and do not mutate persisted artifact state (`tests/phase18_durability_integrity.rs:313-363`).
- Independent store instances can recover the same artifact after source and writer state are dropped, proving no hidden in-memory state is required (`tests/phase18_durability_integrity.rs:365-399`).

## Existing

Phase 17 already established the first local durability primitive: complete artifact serialization, digest-addressed content blobs, recovery validation, missing/corrupt state detection, recovered content resolution, and subprocess recovery. The production implementation persists complete artifact bytes and content bytes in `LocalArtifactStore` (`src/storage.rs:28-41`, `src/storage.rs:88-122`), recovers and validates artifacts by identity (`src/storage.rs:44-75`, `src/storage.rs:158-219`), and exposes recovered content through `RecoveredArtifact` (`src/storage.rs:126-155`).

The kernel identity contract already existed before storage: `Artifact::from_parts` computes identity from entries, pipeline identity, capabilities, and provenance source identity (`src/core/mod.rs:186-217`, `src/core/mod.rs:342-372`). Transformation lineage is represented by `TransformationRecord` and stored on `Artifact` (`src/core/mod.rs:137-176`).

## Corrected

The audit found one narrow durability boundary defect: recovered content was verified during `recover`, but a durable blob changed after recovery could be opened later by `RecoveredArtifact::resolve` without revalidation. ZIP materialization independently verified content while writing, but TAR materialization relied on the resolver boundary.

The fix stores each recovered content path with its expected artifact entry and revalidates size and digest on every recovered content resolution (`src/storage.rs:58-75`, `src/storage.rs:126-155`). The new Phase 18 test corrupts content after recovery and proves resolution fails closed (`tests/phase18_durability_integrity.rs:251-273`).

## External

Phase 18 does not add and does not prove any of the following:

- database storage;
- remote object storage;
- replication or synchronization;
- distributed locking;
- artifact catalogs or global discovery;
- registries;
- trust stores;
- attestation stores;
- revocation stores;
- authorization policy stores;
- claims systems;
- garbage collection;
- background workers, queues, caches, or event buses.

Those remain outside the Artifact Engine durability boundary.

## Not proven

Phase 18 does not prove distributed durability, concurrent writer coordination, crash-safe multi-artifact transactions, long-term retention policy, garbage collection safety, remote transport, authorization, trust, revocation, or global artifact discovery. The validated contract is local filesystem persistence/recovery of individual artifacts without changing artifact semantics.

## Completion statement

Artifact persistence is an implementation detail of the Artifact Engine, not a second semantic model. An artifact can be stored, recovered, and materialized without storage changing what the artifact is.
