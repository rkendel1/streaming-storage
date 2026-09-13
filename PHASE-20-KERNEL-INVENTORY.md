# Phase 20 — Kernel Inventory Audit

This inventory catalogs every major production type and state in the Artifact Engine kernel, auditing what it actually is, who owns it, what makes it authoritative, and what production evidence supports its current role.

## Core Types

### Artifact

**Location:** `src/core/mod.rs:164-176`

**What it is:** The singular logical artifact object. Holds identity, entries, manifest, pipeline identity, capabilities, provenance, lineage, and optional semantic declaration.

**Ownership:** Owned directly. Created through `Artifact::from_parts`, persisted and recovered by `LocalArtifactStore`.

**Authority:** Identity computation is authoritative—`Artifact::from_parts` is the sole identity authority in the codebase. Invoked by storage validation, transform applications, composition, and kernel APIs.

**Identity effect:** Yes. Artifact identity is its fundamental property. Artifact identity is deterministic, stable, and computed from entries, pipeline identity, canonical capabilities, and source provenance identity. No other state affects identity.

**Persistence:** Yes. Serialized as canonical JSON by `Artifact::to_canonical_bytes()` and written by `LocalArtifactStore::persist`.

**Derived state:** Manifest, identity computation, canonical capabilities, entry sorting.

**External:** No. Artifact is a kernel-owned primitive.

**Production test:** `tests/phase16_kernel.rs` proves identity stability and transformation lineage observation. `tests/phase18_durability_integrity.rs` proves persistence/recovery identity preservation. `tests/phase19_composition.rs` proves composition result determinism.

---

### ArtifactEntry

**Location:** `src/core/mod.rs:116-122`

**What it is:** A normalized path, entry type, size, and content digest. Entries are stored in sorted order within an Artifact.

**Ownership:** Owned by Artifact. Created and validated by `Artifact::from_parts`.

**Authority:** Entry layout is validated—paths must be normalized, no duplicates, no path conflicts. Content digest is authoritative only with respect to validating durable content during recovery.

**Identity effect:** Yes. Entries (path, size, content_digest) participate directly in artifact identity computation.

**Persistence:** Yes. Persisted within the Artifact's JSON serialization.

**Validation:** `validate_entry_layout` enforces path normalization, uniqueness, and no parent/child conflicts.

**Production test:** `tests/phase13_behavioral_contract.rs` proves entry sorting and identity stability. `tests/phase18_durability_integrity.rs` proves entry content validation during recovery.

---

### Manifest

**Location:** `src/core/mod.rs:144-152`

**What it is:** A consistency record within Artifact. Mirrors manifest_version, artifact_identity, entries, pipeline_identity, capabilities, and provenance.

**Ownership:** Owned by Artifact. Not independently updated or reused.

**Authority:** Not authoritative. Manifest validation during recovery (`src/storage.rs:158-176`) checks that manifest fields exactly match Artifact fields. Any mismatch fails recovery.

**Identity effect:** No. Manifest records identity but does not compute it.

**Persistence:** Yes. Persisted as part of Artifact.

**Role:** Recovery validation. A durable consistency check that artifact fields and manifest agree before recovering.

**Production test:** `tests/phase18_durability_integrity.rs` proves manifest validation rejects corrupted/inconsistent artifacts.

---

### Provenance

**Location:** `src/core/mod.rs:130-135`

**What it is:** Artifact creation context. Holds source_identity, pipeline_identity, and creation_metadata (volatile, non-identity).

**Ownership:** Owned by Artifact. Created at artifact build time.

**Authority:** Source identity participates in artifact identity computation. Volatile metadata is descriptive only.

**Identity effect:** Partial. Source identity (via `compute_artifact_identity`) affects artifact identity. Volatile creation metadata does not.

**Persistence:** Yes. Serialized as part of Artifact.

**Immutability:** Source and pipeline provenance are immutable for the artifact. Creation metadata is set once at build time.

**Production test:** `tests/phase16_kernel.rs` proves volatile metadata does not change identity. `tests/phase18_durability_integrity.rs` proves provenance survives recovery.

---

### TransformationRecord

**Location:** `src/core/mod.rs:137-142`

**What it is:** Lineage metadata recording a single transformation. Holds input artifact identity, transform identity, and transform kind.

**Ownership:** Owned by Artifact.lineage (a Vec).

**Authority:** Not authoritative. Lineage does not redefine artifact identity. Records are appended only by explicit transform invocation.

**Identity effect:** No. Lineage is excluded from `compute_artifact_identity`.

**Persistence:** Yes. Serialized as part of Artifact if persisted.

**Semantics:** Single-input only. Each record names exactly one input artifact identity and one transform.

**Production test:** `tests/phase16_kernel.rs` proves lineage observation without artifact identity redefinition. `tests/phase18_durability_integrity.rs` proves lineage survives persistence/recovery. `tests/phase19_composition.rs` confirms composition does not use TransformationRecord (composition outputs are artifacts without intermediate lineage).

---

## Pipeline & Execution

### PipelineSpec

**Location:** `src/pipeline/mod.rs:76-120`

**What it is:** Specification for an ordered sequence of execution stages. Immutable once constructed.

**Ownership:** Passed to artifact build and execution. Not stored by kernel alone.

**Authority:** Pipeline identity is deterministic from spec. Identity participates in artifact identity.

**Identity effect:** Yes. Pipeline identity is derived from stages and affects artifact identity.

**Persistence:** Only through artifact provenance/identity, not independently.

**Determinism:** Spec-to-identity is deterministic (`src/pipeline/mod.rs:499-504`). Same spec produces same pipeline identity.

**Production test:** `tests/phase16_kernel.rs` proves pipeline identity stability and change sensitivity.

---

### ContentResolver

**Location:** `src/pipeline/mod.rs:19-38`

**What it is:** Trait defining content access. Implemented by `MemoryContentResolver`, `SourceBackedArtifact`, `TransformedContentResolver`, and `RecoveredArtifact`.

**Ownership:** Owned externally. Supplied to materializers and transform applications.

**Authority:** Content provider during persistence and transformation. Not identity authority.

**Identity effect:** No. Resolver is consumed during materialization but does not affect artifact identity.

**Semantics:** Stateless content provider. Same resolver may be used multiple times.

**Production test:** `tests/phase18_durability_integrity.rs` proves content resolution validation during recovery.

---

## Storage & Durability

### LocalArtifactStore

**Location:** `src/storage.rs:13-16`

**What it is:** Local filesystem-backed durable storage. Only owns a root PathBuf.

**Ownership:** Owns filesystem directories `artifacts/` and `contents/`.

**Authority:** Not authoritative. Validates identity but does not redefine it.

**Identity effect:** No. Storage paths are derived from artifact identity, not inputs to identity computation.

**Operations:** `persist(artifact, resolver)` and `recover(identity)`. No listing, discovery, or registry operations.

**Persistence model:** Artifact JSON at `artifacts/<identity_hex>`, content blobs at `contents/<digest_hex>`.

**Production test:** `tests/phase18_durability_integrity.rs` proves persistence/recovery cycle preserves identity and enables independent recovery.

---

### RecoveredArtifact

**Location:** `src/storage.rs:126-155`

**What it is:** Runtime wrapper around a recovered Artifact and content resolver. Implements ContentResolver.

**Ownership:** Returned by `LocalArtifactStore::recover`. Not persisted.

**Authority:** Artifact itself is authoritative; RecoveredArtifact validates and revalidates content.

**Identity effect:** No. Delegates identity to wrapped Artifact.

**Semantics:** Provides durable content access while validating. Revalidates content digest/size before opening.

**Production test:** `tests/phase18_durability_integrity.rs` proves independent store instances can recover the same artifact and revalidate content.

---

## Transformation & Composition

### ArtifactTransform

**Location:** `src/transforms/mod.rs:9-18`

**What it is:** Trait for applying a logical operation to an Artifact. Produces a new Artifact.

**Ownership:** External. Implemented by PrefixTransform, RedactTransform, GenerateTransform.

**Authority:** Transform outputs are artifacts; identity is recomputed by the engine.

**Identity effect:** No direct. Transformation produces a new artifact whose identity is independently computed.

**Lineage:** Implementations append TransformationRecord to output artifact.

**Production test:** `tests/phase16_kernel.rs` proves transformations produce distinct artifacts with observable lineage.

---

### CompositionInput

**Location:** `src/composition.rs:5-7`

**What it is:** Explicit multi-artifact input specification for composition. Holds Vec<Artifact> and collision policy.

**Ownership:** External. Created by caller, consumed by `compose()`.

**Authority:** Inputs are explicit artifact values. No discovery or lookup.

**Identity effect:** Input artifact identities become the result's source provenance via composed identity.

**Semantics:** Ordered, explicit, collision-aware. Composition result identity depends on input identities and collision policy.

**Multi-artifact:** Yes. Multiple independent artifacts can be composed. Single-artifact composition returns a clone.

**Persistence:** No. CompositionInput is not persisted. Result artifact may be persisted separately.

**Production test:** `tests/phase19_composition.rs` proves composition produces deterministic result identity and orders are significant.

---

## Materialization

### ZipMaterializer & TarMaterializer

**Location:** `src/materializers/zip.rs` and `src/materializers/tar.rs`

**What it is:** Downstream converters of Artifact to physical ZIP/TAR format.

**Ownership:** Stateless, external tools.

**Authority:** Produce deterministic physical outputs. Not identity authority.

**Identity effect:** No. Results report artifact_identity without changing it. Output digests are distinct from artifact identity.

**Content resolution:** Consume ContentResolver during materialization. Do not modify artifact.

**Production test:** `tests/phase18_durability_integrity.rs` proves materialization does not mutate persisted artifact identity.

---

## Authorization & Evidence

### ExecutionEvidence

**Location:** `src/authorization/mod.rs`

**What it is:** Record of an engine-performed operation referencing artifacts.

**Ownership:** Exported from engine. Not persisted by kernel.

**Authority:** Self-contained operational record. Not reused for identity or transformation lineage.

**Identity effect:** No.

**Semantics:** Separate from artifact identity. Multiple evidence records can reference the same artifact.

**Production test:** `tests/phase16_kernel.rs` proves evidence independence from identity.

---

### CapabilityPolicy

**Location:** `src/authorization/mod.rs`

**What it is:** Authorization boundary. Encodes what operations are allowed based on artifact capabilities.

**Ownership:** External evaluation. Not persisted in artifact.

**Authority:** Guides execution decisions. Does not mutate artifacts.

**Identity effect:** No.

**Semantics:** Capability policies are contextual and external to kernel artifact state.

**Production test:** `tests/phase16_kernel.rs` proves authorization does not redefine identity.

---

## Summary of Kernel State Diagram

```
Artifact (identity, entries, provenance)
    ├─ identity computation (from entries, pipeline, capabilities, source)
    ├─ entries (paths, digests, sorted, validated)
    ├─ manifest (consistency record)
    ├─ provenance (source identity, pipeline identity, metadata)
    ├─ lineage (optional transformation records, not identity)
    ├─ semantic_declaration (optional, not identity)
    └─ capabilities (canonicalized, affects identity)

Pipeline
    └─ pipeline_identity (deterministic, affects artifact identity)

Materialization
    └─ output_digest (distinct from artifact identity)

Composition
    ├─ multiple inputs (explicit)
    └─ result artifact (deterministic identity)

Storage
    └─ LocalArtifactStore (durable validation, not identity authority)
```

---

## Inventory Conclusion

The kernel is a coherent, tightly-bounded system owned by the engine:

- **Artifact** is the singular logical artifact model.
- **Identity** is deterministic, stable, computed once, never redefined.
- **Entries** participate in identity; content is validated but not identity-defining.
- **Provenance** records creation context; source identity participates, volatile metadata does not.
- **Lineage** records transformations; does not participate in identity.
- **Composition** supports multi-artifact operations with deterministic result identity.
- **Materialization** is downstream; outputs are distinct from logical identity.
- **Storage** validates and persists without redefining identity.
- **No registry, discovery, or global state exists.** All operations are explicit.

Every major type has production tests proving its actual role. No type serves a hidden or undocumented function.
