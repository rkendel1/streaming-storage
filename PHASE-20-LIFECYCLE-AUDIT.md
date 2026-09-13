# Phase 20 — Artifact Lifecycle Audit

This document traces the real production lifecycle of an artifact through every major transition and identifies what is proven versus claimed.

## Lifecycle Diagram

```
SOURCE
  ↓
PIPELINE EXECUTION
  ↓
ARTIFACT CONSTRUCTION
  ↓
IDENTITY COMPUTATION
  ↓
[TRANSFORMATION] (optional)
  ↓
[COMPOSITION] (optional)
  ↓
[PERSISTENCE] (optional)
  ↓
[RECOVERY] (optional)
  ↓
MATERIALIZATION (optional)
```

---

## 1. SOURCE → PIPELINE EXECUTION

**What happens:** Source content is supplied to a pipeline specification for execution.

**Production implementation:** `src/pipeline/mod.rs:480-530` (`PipelineExecutor::execute`). Executes ordered stages (Generate, Transform, Compile, Select). Each stage accesses source through `ContentResolver` without mutating the logical artifact.

**Identity effect:** None yet. Pipeline stages read source but do not yet create an artifact identity.

**Provenance effect:** Source content is discovered and identified (sha256 digest computed).

**Lineage effect:** None. No lineage records yet.

**Persistence effect:** None. Source is runtime content only.

**Evidence:** `tests/phase16_kernel.rs` proves stable artifact identity from identical source trees and changed identity when content changes. `tests/phase18_durability_integrity.rs` proves source content hashing during persistence.

**Status:** PROVEN. Pipeline execution is demonstrated in production tests.

---

## 2. PIPELINE EXECUTION → ARTIFACT CONSTRUCTION

**What happens:** Pipeline stages produce entries and compiled content, which are assembled into an Artifact value.

**Production implementation:** `src/pipeline/mod.rs:506-530` creates entries from discovered/transformed content. `src/core/mod.rs:186-217` (`Artifact::from_parts`) constructs the artifact from entries, pipeline identity, capabilities, and provenance.

**Identity effect:** Begins artifact identity computation (not yet finalized).

**Provenance effect:** Records source identity and pipeline identity.

**Lineage effect:** None. Artifact construction does not append lineage.

**Persistence effect:** None. Artifact exists in memory only.

**Validation:** Entry path normalization, deduplication, and layout validation occur here.

**Evidence:** `tests/phase16_kernel.rs` proves artifact construction and identity computation. `tests/phase13b_production_boundary.rs` proves real artifact construction without external registries.

**Status:** PROVEN. Artifact construction is production code exercised in tests.

---

## 3. ARTIFACT CONSTRUCTION → IDENTITY COMPUTATION

**What happens:** Artifact identity is computed deterministically from entries, pipeline identity, canonical capabilities, and source provenance identity.

**Production implementation:** `src/core/mod.rs:342-372` (`compute_artifact_identity`). Canonical input includes entries, pipeline_identity, capabilities, and provenance.source_identity. Excludes lineage, semantic_declaration, volatile creation metadata, and storage/process details.

**Identity effect:** Identity is finalized and immutable.

**Provenance effect:** Source identity is fixed (it was already captured).

**Lineage effect:** None.

**Persistence effect:** None yet.

**Determinism:** Same inputs always produce same identity. Proven by `tests/phase18_durability_integrity.rs:94-121` (equivalent artifacts from different source paths produce same identity).

**Evidence:** `tests/phase16_kernel.rs` proves identity stability. `tests/phase18_durability_integrity.rs` proves identity preservation across storage and recovery.

**Status:** PROVEN. Identity computation is deterministic and tested.

---

## 4. [TRANSFORMATION] (Optional)

**What happens (if transformation is applied):** An existing Artifact is passed to a Transform, which produces a new Artifact.

**Production implementation:** `src/transforms/mod.rs:75-89` (`ArtifactTransform::apply`). Transforms take input Artifact and ContentResolver, apply logic, return new Artifact with transformation record appended.

**Identity effect:** Output artifact identity is recomputed fresh. Input artifact identity is recorded in lineage but does not define output identity.

**Provenance effect:** Output retains input provenance. Volatile metadata is not carried forward.

**Lineage effect:** Output append TransformationRecord(input_identity, transform_identity, transform_kind).

**Persistence effect:** No automatic persistence. Result artifact persists only if explicitly persisted.

**Multi-artifact:** No. Transform accepts single input. (Multi-artifact operation is Composition, not Transformation.)

**Evidence:** `tests/phase16_kernel.rs` proves transformations produce new artifacts with observable lineage. `tests/phase18_durability_integrity.rs` proves lineage survives persistence/recovery without redefining identity.

**Status:** PROVEN. Transformation is production code with observable lineage.

---

## 5. [COMPOSITION] (Optional)

**What happens (if composition is invoked):** Multiple independent Artifacts are explicitly provided to CompositionInput, which produces a new Artifact.

**Production implementation:** `src/composition.rs:26-82` (`CompositionInput::compose`). Takes ordered Vec<Artifact> and collision policy. Returns single Artifact with result identity computed from input identities and policy.

**Identity effect:** Result identity is deterministic from input identities and collision policy. Does not append transformation lineage.

**Provenance effect:** Result stores composed source provenance (input identities and collision policy). Retains source/pipeline identity context.

**Lineage effect:** None. Composition does not create TransformationRecord. Composition is not silently treated as a transformation.

**Multi-artifact:** Yes. Composition is the explicit multi-artifact operation.

**Persistence effect:** No automatic persistence. Result persists only if explicitly persisted.

**Ordering:** Input order is significant. Different ordered compositions produce different result identities.

**Evidence:** `tests/phase19_composition.rs` proves deterministic composition result identity, ordering sensitivity, and no hidden input artifact records.

**Status:** PROVEN. Composition is production code with deterministic identity.

---

## 6. [PERSISTENCE] (Optional)

**What happens (if persisted):** Artifact is validated, serialized to canonical JSON, and written to durable storage. Entry content is written as separate content blobs.

**Production implementation:** `src/storage.rs:28-41` (`LocalArtifactStore::persist`). Validates artifact identity, writes canonical bytes, and writes entry content through supplied ContentResolver.

**Identity effect:** None. Identity remains unchanged.

**Provenance effect:** Provenance is serialized and persisted as-is.

**Lineage effect:** Lineage is serialized and persisted as-is.

**Content effect:** Entry digests are validated before writing. Content is addressed by digest, not path. `src/storage.rs:88-122` validates each entry's size and digest before writing.

**Durable structure:** Artifact JSON at `artifacts/<identity_sha256>`, content blobs at `contents/<digest_sha256>`.

**Storage paths:** Derived from artifact identity and entry digests. Not inputs to identity.

**Evidence:** `tests/phase18_durability_integrity.rs` proves persistence preserves identity and enables independent recovery.

**Status:** PROVEN. Persistence is production code exercised in tests.

---

## 7. [RECOVERY] (Optional)

**What happens (if recovered from storage):** Artifact is retrieved by requested identity, deserialized, validated, and returned as RecoveredArtifact wrapping the artifact and a content resolver.

**Production implementation:** `src/storage.rs:44-75` (`LocalArtifactStore::recover`). Reads artifact JSON, checks stored identity matches requested identity, validates artifact against kernel recomputation, recovers content paths.

**Identity effect:** None. Identity is unchanged and revalidated.

**Provenance effect:** Provenance is recovered as-is.

**Lineage effect:** Lineage is recovered as-is. No lineage is appended for the recovery operation itself.

**Content effect:** Entry content is located by stored digest and revalidated (`src/storage.rs:194-219`). Content digests and sizes must match stored metadata.

**Validation:** `src/storage.rs:158-191` validates recovered artifact against kernel identity recomputation. Manifest consistency is checked.

**Corruption behavior:** Fails closed. Identity mismatch, manifest inconsistency, or missing/corrupted content is rejected.

**Semantics:** Recovery reconstructs; does not transform or execute.

**Evidence:** `tests/phase18_durability_integrity.rs` proves recovery preserves identity and enables repeated recovery without mutation. `tests/phase18_durability_integrity.rs:197-273` proves corruption detection.

**Status:** PROVEN. Recovery is production code with validated identity and content.

---

## 8. MATERIALIZATION (Optional)

**What happens (if materialized):** Artifact is converted to physical format (ZIP, TAR) by resolving entries through ContentResolver and writing format-specific structure.

**Production implementation:** `src/materializers/zip.rs:23-80` and `src/materializers/tar.rs:12-68`. Consume Artifact and ContentResolver, produce MaterializationResult with artifact_identity, format, output_digest, and size.

**Identity effect:** None. Artifact identity is unchanged and reported in MaterializationResult.

**Provenance effect:** None. Materialization does not modify artifact provenance.

**Lineage effect:** None. Materialization does not modify artifact lineage.

**Content resolution:** Materializers iterate entries and request content through resolver. Resolver is trusted to provide correct bytes.

**Output:** Bytes are written to a destination path (not managed by artifact storage).

**Determinism:** Same artifact and resolver produce same output digest. Proven by `tests/phase18_durability_integrity.rs`.

**Mutability:** Materialization is downstream. Does not mutate persisted artifact.

**Evidence:** `tests/phase18_durability_integrity.rs:313-363` proves materialization preserves artifact identity and produces distinct output digests.

**Status:** PROVEN. Materialization is production code exercised in tests.

---

## Lifecycle Transition Matrix

| Transition | Implementation | Identity preservation | Lineage behavior | Persisted | Test evidence |
| --- | --- | --- | --- | --- | --- |
| Source → Pipeline | `PipelineExecutor::execute` | Not yet | N/A | No | phase16 |
| Pipeline → Artifact | `Artifact::from_parts` | Begins | N/A | No | phase13b, phase16 |
| Artifact → Identity | `compute_artifact_identity` | Finalized | N/A | No | phase16, phase18 |
| Transform (optional) | `ArtifactTransform::apply` | Recomputed fresh | Appended | No | phase16, phase18 |
| Composition (optional) | `CompositionInput::compose` | Deterministic from inputs | None appended | No | phase19 |
| Persist (optional) | `LocalArtifactStore::persist` | Unchanged | Serialized | Yes | phase18 |
| Recover (optional) | `LocalArtifactStore::recover` | Revalidated | Deserialized | Yes (durable) | phase18 |
| Materialize (optional) | Materializers | Unchanged | Unaffected | Physical bytes | phase18 |

---

## Critical Transitions Verified

### A → B → A (Persist then Recover)

**Question:** Does an artifact retain identity through persist/recover?

**Evidence:** `tests/phase18_durability_integrity.rs:123-157` proves:
- Create artifact A
- Persist A
- Recover A
- A_recovered.identity == A.identity
- A_recovered.provenance == A.provenance
- A_recovered.lineage == A.lineage

**Status:** PROVEN.

---

### A → T(A) → B (Transform)

**Question:** Does transformation produce a distinct artifact B with observable lineage to A?

**Evidence:** `tests/phase16_kernel.rs` proves:
- Transform A → B produces new artifact identity
- B.lineage contains record of A's identity and transform identity
- B.identity does not include B.lineage (identity computed before lineage appended)

**Status:** PROVEN.

---

### Compose(A, B) → C

**Question:** Does composition of two artifacts produce a deterministic artifact C with input artifacts captured in provenance?

**Evidence:** `tests/phase19_composition.rs` proves:
- Compose(A, B) produces C with deterministic identity
- C.provenance records input artifact identities in source_identity
- Persist(C) does not create records for A or B
- Recover(C) recovers only C, not A or B

**Status:** PROVEN.

---

### Persist → Recover → Materialize

**Question:** Can a recovered artifact be materialized with correct content?

**Evidence:** `tests/phase18_durability_integrity.rs:313-363` proves:
- Persist(A)
- A_recovered = Recover(A)
- Materialize(A_recovered) produces correct ZIP
- ZIP digest is deterministic
- A_recovered.identity == A.identity

**Status:** PROVEN.

---

### Store Independence

**Question:** Can two independent store instances recover the same artifact?

**Evidence:** `tests/phase18_durability_integrity.rs:365-399` proves:
- Store1.persist(A)
- Copy durable state to Store2 path
- Store2.recover(A.identity) succeeds
- A_recovered_store1.identity == A_recovered_store2.identity

**Status:** PROVEN.

---

## Lifecycle Conclusion

The artifact lifecycle is **completely proven in production**.

Every major transition:
- Is implemented in production code
- Has production tests exercising the transition
- Preserves or correctly transforms identity
- Persists/recovers the expected state
- Has no hidden side effects

No transition claims behavior beyond what the tests demonstrate. No transition depends on undocumented external systems.

The lifecycle is semantically coherent: artifacts flow through optional transformations and compositions, serialize for durability, recover with identity validation, and materialize to physical format—all without re-entrance, implicit discovery, or global state.
