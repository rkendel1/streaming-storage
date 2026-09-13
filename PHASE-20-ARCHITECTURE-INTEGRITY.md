# Phase 20 — Architecture Integrity Audit

This audit searches the codebase for hidden state, implicit registries, global mutable state, sidecar files, test-only substitutes that production depends on, and any patterns that violate the stated kernel boundaries.

---

## Search 1: Global Mutable State

**Query:** Search for `static mut`, `lazy_static`, `thread_local!`, `once_cell`, global registries, singletons.

**Findings:**

- **`src/lib.rs`** exports core types (`Artifact`, `LocalArtifactStore`, `PipelineSpec`, etc.) as public re-exports. No static registry.
- **`src/storage.rs`** owns only a `PathBuf` root in the `LocalArtifactStore` struct. No static caches or shared mutable state.
- **`src/pipeline/mod.rs`** executes pipelines through `PipelineExecutor` which owns state locally (no globals).
- **`src/core/mod.rs`** defines types only; no mutable static state.
- **`src/transforms/mod.rs`** defines transform traits and implementations. No global transform registry.

**Conclusion:** No global mutable state found. No singletons. No lazy_static caches. All state is owned by instances or passed explicitly.

**Status:** VERIFIED. Architecture is clean.

---

## Search 2: Implicit Caches

**Query:** Search for caches, memoization, hidden lookup tables.

**Findings:**

- **`src/storage.rs:232-238`** uses a temporary file for atomic writes, then renames into place. This is correct for crash safety. No persistent cache.
- **`src/core/mod.rs`** computes identity on demand in `Artifact::from_parts`. No identity cache.
- **`src/pipeline/mod.rs`** stores pipeline execution state locally in `PipelineState` and `PipelineExecutor`. No global pipeline cache.

**Conclusion:** No implicit caches. All computations are explicit and transparent.

**Status:** VERIFIED. No hidden memoization.

---

## Search 3: Filesystem Discovery

**Query:** Search for directory scanning, file discovery, or implicit artifact loading.

**Findings:**

- **`src/storage.rs`** only reads/writes at explicit paths. No glob patterns, no directory scanning, no artifact discovery.
  - `persist` writes to `artifacts/<identity>` and `contents/<digest>`.
  - `recover` reads from `artifacts/<identity>` only when explicitly requested.
- **`src/pipeline/mod.rs`** loads source content from supplied paths. No automatic discovery.

**Conclusion:** No filesystem scanning. No implicit discovery. All operations are explicit.

**Status:** VERIFIED. Boundaries are respected.

---

## Search 4: Sidecar Files

**Query:** Search for `.metadata`, `.info`, `.state`, or other sidecar/marker files.

**Findings:**

- **`src/storage.rs`** stores only:
  - `artifacts/<identity_hex>` (artifact JSON)
  - `contents/<digest_hex>` (entry content)
  - No index files, no metadata sidecars, no .lock files.

**Conclusion:** No sidecar files. Only canonical artifact and content addressable storage.

**Status:** VERIFIED. Storage is minimal.

---

## Search 5: Environment-Dependent Identity

**Query:** Search for environment variables, hostname, process ID, or other volatile state affecting identity.

**Findings:**

- **`src/core/mod.rs:342-372`** (`compute_artifact_identity`) canonical input:
  - Entries (path, size, digest)
  - Pipeline identity
  - Capabilities
  - Provenance source_identity
  - Excludes: timestamps, process ID, hostname, environment, filesystem paths, store instance.

**Verification:** `tests/phase18_durability_integrity.rs:94-121` proves equivalent artifacts from different processes/directories have same identity.

**Conclusion:** Identity is environment-independent. Portable.

**Status:** VERIFIED. Identity is stable across systems.

---

## Search 6: Fallback Storage

**Query:** Search for fallback stores, implicit secondary storage, or automatic failover.

**Findings:**

- **`src/storage.rs`** defines only `LocalArtifactStore` with a single root path. No fallback logic.
- **`src/lib.rs`** exports only `LocalArtifactStore`. No automatic store selection.

**Conclusion:** Single explicit store per use. No hidden fallback.

**Status:** VERIFIED. Storage model is explicit.

---

## Search 7: Duplicate Artifact Representations

**Query:** Search for multiple artifact models, parallel type hierarchies, or inconsistent representations.

**Findings:**

- **Single artifact model:** `Artifact` in `src/core/mod.rs:164-176` is the sole logical artifact type.
- **No separate persistence model:** `LocalArtifactStore` persists `Artifact` directly, not a serialization wrapper.
- **No in-memory vs. durable split:** Same type used in both.
- **`ApplicationArtifact`** (`src/application.rs:42-75`) wraps `Artifact` but delegates identity. Not a competing model.
- **`PublicArtifact`** (`src/public_api.rs:95-123`) wraps `Artifact` but delegates identity. Not a competing model.

**Conclusion:** Single canonical artifact representation. Wrappers delegate identity to the canonical model.

**Status:** VERIFIED. No duplicate models.

---

## Search 8: Implicit Registries

**Query:** Search for maps, indices, catalogs, or lookups that shadow the explicit API.

**Findings:**

- **`src/storage.rs`** has no registry. Only explicit `persist` and `recover` operations.
- **`src/transforms/mod.rs`** has no global transform registry. Transforms are passed explicitly to `apply`.
- **`src/composition.rs`** has no composition registry. Artifacts are passed explicitly to `compose`.
- **`src/pipeline/mod.rs`** has no pipeline registry. Specs are constructed and executed explicitly.

**Verification:** `tests/phase19_composition.rs:storing_inputs_does_not_create` proves that persisting input artifacts does not create implicit composition records.

**Conclusion:** No hidden registries. All operations are explicit.

**Status:** VERIFIED. No implicit discovery or lookup.

---

## Search 9: Test-Only Substitutes in Production

**Query:** Search for production code that depends on test-only types, mocks, or stubs.

**Findings:**

- **`src/lib.rs`** exports only production types (`Artifact`, `LocalArtifactStore`, etc.). No test doubles exported.
- **`src/tests/`** and `tests/` modules define test helpers (like `create_artifact_from_entries`) but these are not used by production code.
- **Production implementations are straightforward:** No abstraction layers that hide testing frameworks.

**Conclusion:** Production code is independent of test infrastructure.

**Status:** VERIFIED. No test doubles in production path.

---

## Search 10: Bypass Paths

**Query:** Search for code paths that skip kernel validation (identity recomputation, entry layout validation, content verification).

**Findings:**

- **`Artifact::from_parts`** (the identity authority) always validates:
  - Entry layout (`validate_entry_layout`)
  - Canonical capabilities (`canonical_capabilities`)
  - Recomputes identity
  - All callers go through this.

- **`LocalArtifactStore::recover`** always validates:
  - Artifact identity recomputation
  - Manifest consistency
  - Content digest/size
  - No bypass for corruption.

- **No manual Artifact construction:** Artifact fields are private; callers must use `from_parts`.

**Verification:** `tests/phase18_durability_integrity.rs:197-273` proves corrupted state fails recovery.

**Conclusion:** No bypass paths exist. All validation is mandatory.

**Status:** VERIFIED. Validation gates all operations.

---

## Search 11: Hidden Dependencies

**Query:** Search for implicit dependencies between modules, cyclic imports, or circular state relationships.

**Findings:**

- **Dependency graph:**
  - `core` is depended on by `storage`, `transforms`, `composition`, `materializers`.
  - `pipeline` is used by application code but doesn't depend on `storage`.
  - `storage` depends on `core` only.
  - `transforms` depends on `core` only.
  - `composition` depends on `core` only.
  - No cycles.

- **No hidden state sharing:** Each module owns its state clearly.

**Conclusion:** Dependency graph is acyclic and transparent.

**Status:** VERIFIED. No hidden dependencies.

---

## Search 12: Violated Boundaries

**Query:** Search for code that violates stated kernel boundaries (e.g., storage making trust decisions, core defining policies).

**Findings:**

- **Core module (`src/core/mod.rs`):**
  - Owns artifact definition and identity computation.
  - Does not own trust, authorization, claims, attestations, revocation, registry, or deployment.
  - ✓ Correct.

- **Storage module (`src/storage.rs`):**
  - Owns persistence and recovery.
  - Does not own artifact identity definition, content semantics, claims, authorization, or registry.
  - ✓ Correct.

- **Transform module (`src/transforms/mod.rs`):**
  - Defines transform interface and implementations.
  - Does not own artifact identity, storage, or execution authorization.
  - ✓ Correct.

- **Composition module (`src/composition.rs`):**
  - Owns composition operation.
  - Does not create transformation lineage, mutate inputs, or persist results automatically.
  - ✓ Correct.

- **Materialization (`src/materializers/`):**
  - Owns physical format generation.
  - Does not mutate artifact, redefine identity, or persist output.
  - ✓ Correct.

- **Authorization module (`src/authorization/mod.rs`):**
  - Defines `CapabilityPolicy` and `ExecutionEvidence`.
  - **Not integrated into production paths.** Authorization decisions are defined but not enforced.
  - Status: IMPLEMENTED / NOT PROVEN (see notes below).

**Conclusion:** Boundaries are respected. One subsystem (authorization) is defined but not integrated.

**Status:** VERIFIED except for authorization integration.

---

## Authorization Integration Gap

**Finding:** `CapabilityPolicy` and `ExecutionEvidence` are exported and defined, but no production operation checks authorization before proceeding. The examples show authorization evaluation, but the kernel does not enforce authorization gates.

**Implication:** Authorization is a client-responsibility, not a kernel primitive. The engine does not validate that a caller has permission to perform operations.

**Current behavior:** Any caller can create, transform, compose, persist, or recover artifacts without authorization checks.

**Is this correct?** 
- **For the kernel:** Yes. The kernel is a pure data structure. Authorization is a contextual, policy-dependent concern.
- **For production use:** Depends on deployment context. A production system would need a wrapper that enforces authorization before calling kernel operations.

**Recommendation:** Document that authorization is external. Either:
1. Remove `CapabilityPolicy` and `ExecutionEvidence` from the kernel (move to example/integration layer), OR
2. Add a comprehensive test proving the authorization boundary and how external systems should enforce it.

**Status:** GAP IDENTIFIED. Not a correctness issue, but a boundary clarity issue.

---

## Conclusion of Integrity Audit

### Clean Architecture

The codebase has:
- ✓ No global mutable state
- ✓ No implicit caches
- ✓ No filesystem discovery
- ✓ No sidecar files
- ✓ No environment-dependent identity
- ✓ No fallback storage
- ✓ No duplicate representations
- ✓ No hidden registries
- ✓ No test doubles in production
- ✓ No bypass paths around validation
- ✓ No circular dependencies
- ✓ Correctly enforced boundaries

### One Integration Gap

- Authorization is defined but not enforced in kernel operations

### One Semantic Boundary to Clarify

- Multi-input lineage semantics (composition vs. transformation) should be documented

### Overall

The Artifact Engine kernel is **architecturally sound with no hidden state or implicit behavior.** All operations are explicit, all validation is mandatory, and all state ownership is clear.

The one gap is not a correctness defect; it's a design boundary that should be explicitly documented for external users.
