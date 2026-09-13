# Phase 18 Durability Integrity Audit

Phase 18 audits whether the Phase 17 durability implementation is only a durable materialization of the Artifact Kernel, rather than a second artifact model, registry, identity authority, or hidden execution system.

## Production scope audited

The production durability surface is `LocalArtifactStore` and `RecoveredArtifact` in `src/storage.rs`. `LocalArtifactStore` owns only a filesystem root (`src/storage.rs:13-16`), creates `artifacts/` and `contents/` directories when opened (`src/storage.rs:18-25`), persists an `Artifact` plus a `ContentResolver` (`src/storage.rs:28-41`), and recovers by requested artifact identity (`src/storage.rs:44-75`). `RecoveredArtifact` exposes the recovered `Artifact` and implements `ContentResolver` for durable content (`src/storage.rs:126-155`).

## Identity authority

Artifact identity remains authoritative in the Artifact Kernel. `Artifact::from_parts` sorts entries, validates entry layout, canonicalizes capabilities, and computes `Artifact.identity` from entries, pipeline identity, capabilities, and provenance (`src/core/mod.rs:186-217`). The identity calculation uses a canonical artifact containing entries, pipeline identity, capabilities, and provenance source identity (`src/core/mod.rs:342-372`).

`LocalArtifactStore` validates identity but does not redefine it. `persist` calls the same recovery validation before writing (`src/storage.rs:28-40`). `recover` reads the durable bytes at the requested identity path, rejects an artifact whose serialized identity differs from the requested identity, and then validates the recovered artifact (`src/storage.rs:44-57`). Validation rebuilds an `Artifact` through `Artifact::from_parts` and rejects mismatched recomputed identity (`src/storage.rs:158-191`).

Storage paths are not identity authority. `artifact_path` maps an already-existing digest string to a file name under the store root (`src/storage.rs:77-85`), and `digest_file_name` only accepts `sha256:` digests with 64 hex characters (`src/storage.rs:248-258`). The path is derived from artifact identity; it is not an input to `Artifact::from_parts` or `compute_artifact_identity` (`src/core/mod.rs:186-217`, `src/core/mod.rs:342-372`).

## Content authority

Content authority is split intentionally between kernel metadata and bytes, with validation binding them. Each artifact entry records path, size, and `content_digest` in the kernel artifact type (`src/core/mod.rs:164-176`). During persistence, the store asks the supplied `ContentResolver` for each entry path, hashes the bytes, validates digest and size against the artifact entry, and writes bytes to the digest-addressed content path (`src/storage.rs:88-122`). During recovery, the store derives each content path from the entry digest and verifies size and digest before constructing `RecoveredArtifact` (`src/storage.rs:58-75`, `src/storage.rs:194-219`).

The durable content file is not allowed to become a competing source of truth. If an existing content file is present during persistence, the store verifies it before reusing it (`src/storage.rs:116-121`). `RecoveredArtifact::resolve` revalidates the durable content against the recovered entry before opening it, so downstream materializers do not accept post-recovery content drift as new artifact content (`src/storage.rs:144-155`).

## Manifest authority

The manifest is a consistency record inside the artifact, not a competing artifact model. `Artifact` contains both explicit fields and `manifest` (`src/core/mod.rs:164-176`). Recovery validation rejects any artifact whose manifest version, identity, entries, pipeline identity, capabilities, or provenance do not match the artifact fields (`src/storage.rs:158-176`). The store therefore treats the manifest as redundant consistency evidence for the same kernel artifact, not as a separate authority.

## Storage addressing

The filesystem layout is implementation detail. The only durable directories are `artifacts/` and `contents/` (`src/storage.rs:10-11`). Artifact JSON is addressed by the artifact's existing identity (`src/storage.rs:39-40`, `src/storage.rs:77-81`). Content blobs are addressed by each entry's existing content digest (`src/storage.rs:84-85`, `src/storage.rs:88-122`). The identity calculation does not include filesystem location, store instance, directory layout, local machine, storage UUID, process ID, or persistence timestamp (`src/core/mod.rs:342-372`).

Phase 18 production tests prove that equivalent logical artifacts built from different source paths and persisted through different store roots recover with the same identity and semantic fields (`tests/phase18_durability_integrity.rs:94-121`). They also prove intact durable state copied between store roots recovers with the same artifact identity rather than being rebound to the destination store (`tests/phase18_durability_integrity.rs:275-311`).

## Recovery authority

Recovery reconstructs an existing artifact. It deserializes the stored `Artifact`, checks that the stored identity equals the requested identity, validates the stored artifact against kernel identity recomputation, and returns `RecoveredArtifact` (`src/storage.rs:44-75`, `src/storage.rs:158-191`). It does not call transformation code or append lineage. The only production transformation code appends `TransformationRecord` when a transform is explicitly applied (`src/transforms/mod.rs:75-89`, `src/transforms/mod.rs:139-153`, `src/transforms/mod.rs:232-246`).

Phase 18 tests persist, recover, re-persist, and recover a transformed artifact while proving identity, provenance, semantic state, and one-entry lineage remain unchanged (`tests/phase18_durability_integrity.rs:123-157`). This proves `A --[T]--> B` is not rewritten as `A --[T]--> B --[recover]--> C` or `A --[T]--> B --[persist]--> C`.

## Storage is not a registry

`LocalArtifactStore` supports identity-to-durable-state lookup only: `recover(&identity)` reads the artifact record for that identity (`src/storage.rs:44-75`). There is no production API for listing artifacts, querying relationships, global discovery, trust, revocation, authorization policy, claims, attestations, or consumer decisions in `src/storage.rs`. The store persists exactly the supplied artifact bytes and entry content after validation (`src/storage.rs:28-41`, `src/storage.rs:88-122`).

Authorization and execution evidence are exported elsewhere as separate production concepts (`src/lib.rs:14-18`), while storage exports only `LocalArtifactStore` and `RecoveredArtifact` (`src/lib.rs:38`). The Phase 18 tests exercise store instances strictly through `persist`, `recover`, `artifact_path`, and `content_path`; no registry, catalog, trust, or policy behavior is available or required (`tests/phase18_durability_integrity.rs:94-392`).

## Claims and semantic declarations

The existing claim-like semantic declaration is stored on `Artifact` as `semantic_declaration` (`src/core/mod.rs:164-176`) and set by `with_semantic_declaration` without recomputing identity (`src/core/mod.rs:228-235`). It is not part of `compute_artifact_identity`, whose canonical input omits lineage and semantic declaration (`src/core/mod.rs:342-372`).

Phase 18 tests prove that changing the semantic declaration preserves the logical identity while the declaration itself survives persistence/recovery (`tests/phase18_durability_integrity.rs:159-195`). This confirms declarations are kernel metadata outside identity, not a generalized claims system and not a storage-owned trust mechanism.

## Corruption and substitution behavior

Metadata corruption fails closed because recovery rejects serialized identity mismatch before returning a recovered artifact (`src/storage.rs:44-57`) and validates manifest consistency plus recomputed identity (`src/storage.rs:158-191`). Content corruption fails closed because recovery and recovered resolution verify content size and digest (`src/storage.rs:58-75`, `src/storage.rs:144-155`, `src/storage.rs:194-219`).

Phase 18 tests prove identity-corrupted artifact metadata fails recovery, digest-addressed content substitution fails recovery, and content altered after recovery fails during recovered content resolution (`tests/phase18_durability_integrity.rs:197-273`). Storage manipulation therefore does not silently redefine a logical artifact.

## Materialization boundary

Materialization remains downstream. ZIP and TAR materializers consume an `Artifact` and a `ContentResolver` and return a `MaterializationResult` with `artifact_identity`, materializer format, output digest, and size (`src/materializers/zip.rs:21-50`, `src/materializers/tar.rs:10-40`). They write representations from artifact entries through content resolution (`src/materializers/zip.rs:53-80`, `src/materializers/tar.rs:42-68`).

Phase 18 tests materialize both ZIP and TAR from a recovered artifact and prove the artifact identity is unchanged, materialization digests are distinct from logical identity and from each other, and the persisted artifact JSON is unchanged by materialization (`tests/phase18_durability_integrity.rs:313-363`).

## Filesystem representation

- Artifact metadata is the serialized complete `Artifact` written by `artifact.to_canonical_bytes()` to `artifacts/<artifact sha256 hex>` (`src/storage.rs:39-40`, `src/storage.rs:77-81`).
- Content is raw entry bytes written to `contents/<content sha256 hex>` (`src/storage.rs:84-85`, `src/storage.rs:88-122`).
- Content names are derived from entry `content_digest`; artifact names are derived from artifact `identity` (`src/storage.rs:77-85`).
- Authoritative logical identity is kernel recomputation from `Artifact::from_parts`, not path names (`src/storage.rs:178-189`, `src/core/mod.rs:342-372`).
- Derived state includes storage paths and materialized ZIP/TAR outputs (`src/storage.rs:77-85`, `src/materializers/zip.rs:21-50`, `src/materializers/tar.rs:10-40`).
- A durable record is recoverable only when artifact JSON exists, deserializes as `Artifact`, matches the requested identity, passes manifest and identity validation, and all entry content blobs exist with matching size and digest (`src/storage.rs:44-75`, `src/storage.rs:158-219`).
- A durable record is invalid when artifact metadata is missing/corrupt, manifest and artifact fields disagree, recomputed identity differs, content is missing, content size differs, or content digest differs (`src/storage.rs:44-75`, `src/storage.rs:158-219`).
- Deleting a materialized ZIP/TAR output is safe for artifact recovery because materializer outputs are not stored in `artifacts/` or `contents/` by `LocalArtifactStore` and are created by materializers as downstream files (`src/storage.rs:10-11`, `src/materializers/zip.rs:32-50`, `src/materializers/tar.rs:21-40`). Deleting required artifact JSON or content blobs makes the corresponding artifact unrecoverable (`src/storage.rs:44-75`, `src/storage.rs:194-219`).

## Hidden state inspection

`LocalArtifactStore` stores only a `PathBuf` root (`src/storage.rs:13-16`). There are no static mutable maps, singleton stores, implicit caches, fallback stores, implicit databases, background workers, or automatic recovery side effects in `src/storage.rs`. The only temporary file is a per-write local temp path used by `write_durable`, then renamed into place (`src/storage.rs:221-238`).

Phase 18 tests drop the original source-backed artifact and writer store, then open two independent store instances to recover and resolve the same artifact (`tests/phase18_durability_integrity.rs:365-399`). This proves no in-memory shared state is required for normal recovery.

## API boundary review

The public API preserves the conceptual distinction: `Artifact` is exported from the core module (`src/lib.rs:20-24`), while `LocalArtifactStore` and `RecoveredArtifact` are exported from storage (`src/lib.rs:38`). `Artifact` contains identity, entries, manifest, provenance, lineage, and semantic declaration (`src/core/mod.rs:164-176`). `LocalArtifactStore` contains only a storage root and methods for durable persistence/recovery (`src/storage.rs:13-85`).

No API rename or broad type change was required. The only corrective production change required by the audit was making `RecoveredArtifact::resolve` revalidate durable content before opening it, preserving the invariant that recovered content cannot silently drift after recovery (`src/storage.rs:144-155`).

## Audit conclusion

Artifact persistence remains an implementation detail of the Artifact Engine. The store serves the artifact by durably writing and validating kernel state; it does not define identity, own content semantics, append lineage, own claims, provide registry behavior, or perform hidden execution.
