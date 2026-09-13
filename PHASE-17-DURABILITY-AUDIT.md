# Phase 17 Durability Audit

Phase 17 asks what the smallest production persistence/recovery capability must be for a logical artifact to survive a process or storage boundary without changing semantic identity.

## Current production kernel state

### Logical artifact location

The logical artifact exists as `core::Artifact`, with identity, entries, manifest, pipeline identity, capabilities, provenance, transformation lineage, and optional semantic declaration stored in the production type (`src/core/mod.rs:164-176`). Pipeline execution returns that artifact inside `SourceBackedArtifact` (`src/pipeline/mod.rs:607-633`).

### Serialization completeness

`Artifact`, `Manifest`, `ArtifactEntry`, `Provenance`, `Capability`, and `TransformationRecord` derive `Serialize`/`Deserialize` (`src/core/mod.rs:92-176`). `Artifact::to_canonical_bytes` serializes the complete artifact object currently owned by the kernel (`src/core/mod.rs:241-243`).

Serialization alone was not sufficient durability before this phase because it did not provide an artifact-addressed persistence location, content persistence, recovery validation, or a recovered content resolver.

### Identity-bearing state

Artifact identity is computed from normalized entries, pipeline identity, canonical capabilities, and provenance source identity (`src/core/mod.rs:342-372`). Creation metadata is stored in provenance (`src/core/mod.rs:130-135`) but intentionally excluded from identity by the canonical provenance used during identity calculation (`src/core/mod.rs:348-369`).

Pipeline identity is computed from the canonical pipeline specification, including source, stages, materializer, and canonical capabilities (`src/pipeline/mod.rs:344-368`).

### Lineage

Transformation lineage is represented by `TransformationRecord` (`src/core/mod.rs:137-142`) and stored on `Artifact` as `lineage` (`src/core/mod.rs:172-173`). Production transforms append lineage when producing transformed artifacts (`src/transforms/mod.rs:75-89`, `src/transforms/mod.rs:139-153`, `src/transforms/mod.rs:232-246`).

Lineage is serialized with the artifact, but it is not part of the artifact identity calculation (`src/core/mod.rs:342-372`). Therefore durable recovery must preserve lineage as semantic kernel state without redefining identity.

### Provenance

Provenance is stored in both `Artifact` and `Manifest` (`src/core/mod.rs:144-152`, `src/core/mod.rs:164-176`). Source provenance is produced from selected entries during directory source discovery (`src/pipeline/mod.rs:763-789`, `src/pipeline/mod.rs:811-822`).

Provenance source identity is identity-bearing; volatile creation metadata is not (`src/core/mod.rs:342-372`).

### Content storage and resolution

Entries carry path, type, size, and content digest (`src/core/mod.rs:116-122`). The current `ContentResolver` interface resolves content by artifact path (`src/pipeline/mod.rs:593-604`).

Before Phase 17, content was resolved either from source paths retained by `SourceBackedArtifact` (`src/pipeline/mod.rs:607-648`) or from in-memory transformed content (`src/pipeline/mod.rs:879-920`). That meant artifact serialization could survive independently, but content resolution could still depend on original source files or process-local memory.

### Materialization reproduction

ZIP and TAR materializers consume an `Artifact` plus a `ContentResolver` (`src/materializers/zip.rs:22-80`, `src/materializers/tar.rs:11-68`). ZIP fixes archive metadata such as modified time and permissions for deterministic bytes (`src/materializers/zip.rs:61-64`). Both materializers verify content size/digest while writing (`src/materializers/zip.rs:70-76`, `src/materializers/tar.rs:52-61`).

Materialization can be reproduced after recovery only if the recovered artifact also has a resolver backed by durable content, not by dropped source paths or in-memory maps.

### Existing storage abstraction

No existing production type provided artifact-addressed persistence and recovery. The closest abstractions were `SourceBackedArtifact`, `MemoryContentResolver`, and `TransformedContentResolver`; they resolve content but do not define a durable artifact lifecycle (`src/pipeline/mod.rs:607-648`, `src/pipeline/mod.rs:879-920`).

### ContentResolver sufficiency

`ContentResolver` is sufficient as the input/output contract for persistence and materialization because it abstracts content reads by normalized entry path (`src/pipeline/mod.rs:593-604`). It was insufficient by itself for recovery because a recovered artifact also needs durable path-to-content binding and verification against entry digests.

### Ephemeral identity risk

Artifact identity does not depend on host paths, process IDs, timestamps, or materialization bytes in current production identity code (`src/core/mod.rs:342-372`). Directory traversal order is normalized before artifact creation (`src/pipeline/mod.rs:671-675`) and artifact entries are sorted during construction (`src/core/mod.rs:193-198`, `src/core/mod.rs:375-378`).

## Audit conclusion

Outcome B applies: a minimal local durable store was required.

The kernel already had serializable artifact state and deterministic identity, but it lacked a production persistence/recovery primitive that:

- stores the complete logical artifact;
- stores content bytes needed by the artifact entries;
- recovers by artifact identity;
- validates recovered identity and content instead of trusting serialized bytes alone;
- provides a recovered `ContentResolver` for rematerialization;
- crosses a real filesystem/process boundary.

No database, registry, distributed system, trust infrastructure, or second artifact model was required.
