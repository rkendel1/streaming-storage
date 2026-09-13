# Phase 17 Durability Implementation

## Proven

`tests/phase17_durability.rs` proves a production logical artifact can be created, identified, persisted, recovered from a fresh store instance, resolved for content, and materialized with the same deterministic ZIP bytes.

The test suite also proves:

- recovered artifact identity equals original artifact identity;
- entries, provenance, capabilities, pipeline identity, and transformation lineage survive recovery;
- recovered transformed artifacts retain lineage pointing to the source artifact identity and transform identity;
- recovered content is served by a production `ContentResolver`;
- materialization digest remains distinct from logical artifact identity;
- two independent `LocalArtifactStore` instances can recover the same artifact without shared memory;
- a subprocess can persist an artifact and a second subprocess can recover and materialize it;
- truncated artifact state, missing content, and corrupted content fail explicitly.

## Implemented

Phase 17 adds `LocalArtifactStore` and `RecoveredArtifact` in `src/storage.rs`.

`LocalArtifactStore::persist` validates artifact structure, writes all entry content into digest-addressed files, verifies content size/digest during persistence, and writes the artifact's canonical serialized bytes under its identity (`src/storage.rs:28-41`, `src/storage.rs:82-117`).

`LocalArtifactStore::recover` reads by artifact identity, deserializes the artifact, rejects identity mismatches, validates manifest consistency, recomputes identity from kernel fields, verifies each content blob, and returns a `RecoveredArtifact` (`src/storage.rs:44-69`, `src/storage.rs:144-178`, `src/storage.rs:180-205`).

`RecoveredArtifact` implements `ContentResolver`, allowing recovered artifacts to be materialized through existing production materializers (`src/storage.rs:120-142`).

Storage writes use a local filesystem root with separate artifact and content directories, temporary files, file sync, rename, and directory sync (`src/storage.rs:10-11`, `src/storage.rs:207-232`). The implementation does not add a database, registry service, distributed store, background worker, hidden cache, or global mutable state.

## Existing

Before Phase 17, the repository already had:

- deterministic artifact identity (`src/core/mod.rs:342-372`);
- complete artifact serialization (`src/core/mod.rs:164-176`, `src/core/mod.rs:241-243`);
- deterministic pipeline identity (`src/pipeline/mod.rs:344-368`);
- source provenance (`src/pipeline/mod.rs:763-789`, `src/pipeline/mod.rs:811-822`);
- transformation lineage (`src/core/mod.rs:137-142`, `src/transforms/mod.rs:75-89`);
- production content resolver interfaces (`src/pipeline/mod.rs:593-604`);
- deterministic ZIP/TAR materializers (`src/materializers/zip.rs:22-80`, `src/materializers/tar.rs:11-68`).

## External

Phase 17 does not persist or implement:

- external attestations;
- external trust decisions;
- revocation authority;
- consumer policy;
- registry metadata;
- arbitrary external claims;
- remote object storage;
- replication or synchronization.

Those remain outside Artifact Engine's kernel boundary.

## Not yet proven

Phase 17 proves a minimal local filesystem durability boundary. It does not prove distributed durability, concurrent writer semantics, crash-safe multi-artifact transactions, retention policy, garbage collection, cloud persistence, registry lookup, or external trust recovery.

The local test directory used by the tests is a real storage/process boundary for evidence, not a durable product storage service claim.
