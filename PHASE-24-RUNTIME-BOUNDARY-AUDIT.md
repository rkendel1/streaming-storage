# Phase 24 Runtime Boundary Audit

## Context

**Phase 23B Status:** CONSUMER SURFACE PROVEN
- All 18 acceptance criteria operationally demonstrated
- Complete artifact lifecycle proven: CREATE → TRANSFORM → COMPOSE → PERSIST → RECOVER → MATERIALIZE
- 11 behavioral tests passing, 154 kernel tests still passing, zero regressions
- ContentResolver contract discovered and verified

**Phase 24 Objective:** Prove that an external runtime can execute a materialized Artifact Engine artifact while preserving all ownership boundaries.

---

## The Runtime Boundary Question

Phase 23 proved:

```
Artifact Engine owns artifact
  ↓
Consumer application wraps artifact with domain metadata
  ↓
Consumer application owns "what to build" and "how to build"
  ↓
Artifact persists and recovers independently
```

Phase 24 asks:

```
Artifact Engine owns logical artifact
  ↓
Artifact materializes to consumable representation
  ↓
External runtime executes that representation
  ↓
Does Artifact Engine remain independent of runtime?
Does runtime remain unaware of Artifact Engine semantics?
Does artifact identity survive execution?
Can artifact be recovered independently of runtime state?
```

This is **not** a deployment/orchestration proof (no Fly, Render, K8s).

This is a **conceptual boundary proof**: Can an artifact cross into external execution without Artifact Engine becoming a runtime?

---

## What Phase 23B Actually Proved (Operational Evidence)

### Artifact Identity Is Kernel-Owned
- **Test:** consumer_does_not_compute_artifact_identity
- **Evidence:** Consumer reads artifact.identity, never computes it
- **Implication:** Identity is immutable once computed; consumer cannot modify it

### Materialization Produces Physical Representation
- **Test:** consumer_materialization_to_zip
- **Evidence:** ZipMaterializer.materialize_to_vec() produces bytes
- **Key Finding:** Artifact.identity ≠ materialization output digest
  - Logical artifact identity: `sha256:...` (content-addressed)
  - ZIP output digest: different from artifact identity
  - Proves: logical and physical representations are distinct

### Content Resolution Works End-to-End
- **Test:** consumer_materialization_to_zip, consumer_persistence_and_recovery
- **Evidence:** ContentResolver provides content during all operations
- **Contract Discovered:** resolver.resolve(path: &str) → Box<dyn Read>
  - Takes entry **path**, not content digest
  - Returns bytes matching entry's content_digest
  - All transforms, materializers, persist, recover call resolver

### Persistence and Recovery Preserve Identity
- **Test:** consumer_persistence_and_recovery, consumer_context_boundary_with_filesystem
- **Evidence:**
  - Artifact.persist() succeeds to filesystem
  - Artifact.recover() from filesystem returns identical artifact
  - Identity unchanged: `artifact.identity == recovered_artifact.identity`
  - Separate store instances read same persisted data

### Composition Is Deterministic
- **Test:** consumer_composition_with_deterministic_identity
- **Evidence:** Same input artifacts composed twice produce identical identity
- **Implication:** Composition output is reproducible, not dependent on runtime order

---

## What Phase 23B Did NOT Prove (Gaps for Phase 24)

### 1. Artifact → Runtime Boundary
Not tested: Can an external system (not in consumer proof context) consume a materialized artifact?

**Current evidence:** Materialization works; consumer calls materialize_to_vec()
**Gap:** No actual external process executes the materialized bytes

### 2. Runtime Execution Semantics
Not tested: What does it mean for a runtime to execute a materialized artifact?

**Current evidence:** None (not in Phase 23 scope)
**Gap:** No real executable, no process launch, no exit status

### 3. Runtime State Isolation
Not tested: Can runtime be destroyed without affecting artifact state?

**Current evidence:** Artifact recovers after context boundary (separate store instances)
**Gap:** No actual runtime process; no actual ephemeral state

### 4. Execution Failure Doesn't Corrupt Artifact
Not tested: If runtime fails, is artifact still valid?

**Current evidence:** None (no execution in Phase 23)
**Gap:** No failure case tested

### 5. Repeatable Execution
Not tested: Can same artifact be executed multiple times?

**Current evidence:** None (one-time materialization in Phase 23)
**Gap:** No multiple execution cycles tested

### 6. Recovery After Execution
Not tested: Can artifact be recovered after runtime has executed it?

**Theoretical:** Yes (based on Phase 23 boundary isolation)
**Gap:** Not demonstrated with real execution

### 7. Identity Preservation Across Execution
Not tested: artifact.identity before execution == artifact.identity after execution?

**Theoretical:** Yes (artifact is immutable in kernel)
**Gap:** Not demonstrated with real execution

### 8. Runtime Doesn't Duplicate Kernel Semantics
Not tested: Runtime proof that external process doesn't recompute identity, lineage, etc.

**Current evidence:** consumer proof shows consumer doesn't duplicate
**Gap:** No external runtime artifact created; no source audit of hypothetical runtime

---

## Artifact Engine Responsibilities (From Phase 23 Evidence)

✓ Artifact identity computation (kernel-owned, immutable)
✓ Transformation lineage creation
✓ Composition determinism
✓ Persistence/recovery implementation
✓ Content resolution coordination
✓ Materialization (logical → physical)

The kernel is the **single source of truth** for all artifact semantics.

---

## Consumer Responsibilities (From Phase 23 Evidence)

✓ Domain semantics (what to build, how to build)
✓ Domain metadata (build logs, success status)
✓ Build orchestration (if needed)
✓ Consumer-specific identity (e.g., BuildConfig identity - distinct from artifact identity)

The consumer wraps artifacts but never duplicates kernel semantics.

---

## Runtime Responsibilities (Hypothetical, to be Proven in Phase 24)

**Must Own:**
- Process execution
- Process lifecycle (start, monitor, stop)
- Runtime environment (filesystem, variables)
- Process resources (CPU, memory, etc.)
- Execution result (exit status, output, logs)
- Runtime state (ephemeral, not persisted to artifact)

**Must NOT Own:**
- Artifact identity
- Artifact serialization
- Transformation lineage
- Composition identity
- Content persistence
- Artifact registry

---

## Current Public API Surface Relevant to Phase 24

### Materialization (Artifact → Runtime Representation)
```rust
pub struct ZipMaterializer;
pub fn materialize_to_vec<R: ContentResolver>(&self, artifact: &Artifact, resolver: &R) -> Result<Vec<u8>, ArtifactError>
pub fn materialize_to_path<R: ContentResolver>(&self, artifact: &Artifact, resolver: &R, output: impl AsRef<Path>) -> Result<MaterializationResult, ArtifactError>

pub struct TarMaterializer;
// Same methods as ZipMaterializer
```

### Materialization Result
```rust
pub struct MaterializationResult {
    pub artifact_identity: String,          // Link back to source artifact
    pub materializer_format: String,        // "zip" or "tar"
    pub output_digest: String,              // Digest of materialized output
    pub size_bytes: u64,                    // Size of materialized representation
}
```

### Artifact Metadata Needed by Runtime
```rust
pub struct Artifact {
    pub identity: String,                   // Immutable identity
    pub entries: Vec<ArtifactEntry>,        // Logical structure
    pub lineage: Vec<TransformationRecord>, // Provenance
    pub pipeline_identity: String,          // Source pipeline
    pub provenance: Provenance,             // Creation metadata
    // ... other fields
}

pub struct ArtifactEntry {
    pub path: String,                       // Logical path in artifact
    pub entry_type: EntryType,              // File, Directory, Link
    pub size: u64,                          // Logical size
    pub content_digest: String,             // SHA256 of content
}
```

### Persistence/Recovery (Artifact → Runtime Needs to Know About)
```rust
pub fn persist(&self, artifact: &Artifact, resolver: &dyn ContentResolver) -> Result<(), ArtifactError>
pub fn recover(&self, identity: &str) -> Result<RecoveredArtifact, ArtifactError>
```

**Key Finding:** MaterializationResult provides artifact_identity as a link. This is sufficient for runtime to maintain association without duplicating identity.

---

## Consumer Proof As Reference

The existing `examples/consumer-proof/src/lib.rs` demonstrates:

1. **How to use Artifact Engine without duplication:**
   - Wrapper pattern (ConsumerBuild = Artifact + BuildMetadata)
   - Delegates all kernel operations
   - Owns only domain-specific state

2. **ContentResolver implementation pattern:**
   - TestContentResolver maps path → Vec<u8>
   - Provides content during transforms, materializers, persistence

3. **Real execution environment isn't needed** for consumer proof
   - Tests create artifacts, transform, compose, persist, recover
   - All operations work with real kernel code
   - No mock or stub runtime

For Phase 24, we need to extend this pattern with:

- Real executable artifact (not just data files)
- Real process execution (not just API calls)
- Verification that artifact properties survive execution
- Proof that execution doesn't require artifact engine involvement

---

## Materialization Gap Analysis

### What Materializers Currently Do
- Take logical Artifact
- Resolve all content via ContentResolver
- Serialize to physical format (ZIP/TAR)
- Return MaterializationResult with artifact_identity

### What Runtime Would Do With Materialized Artifact
- Receive materialized bytes or path
- Extract to runtime environment
- Execute logical application
- Record exit status

### Gap: How Does Runtime Find Application Entry Point?
**Current Status:** MaterializationResult provides artifact_identity and output_digest

**Questions for Phase 24:**
1. Should runtime discover entry point from artifact.entries?
2. Should runtime receive explicit entry point specification?
3. Should runtime inspect materialized structure?
4. Is current ZIP structure (with artifact.entries paths) sufficient?

**Current Evidence:** ZIP materializer produces valid ZIP with correct paths from artifact.entries

This should be sufficient; runtime can extract ZIP and execute file at a known path.

---

## Executable Artifact Options for Phase 24

### Option A: Existing Rust Binary
- Compile a simple Rust program (e.g., examples that already exist)
- Package into Artifact
- Materialize
- Execute via subprocess
- **Advantage:** Guaranteed working, tests own build system
- **Disadvantage:** Ties test to Rust toolchain

### Option B: Tiny Standalone Executable
- Create minimal C/shell executable
- Binary committed to repository
- Materialized and executed
- **Advantage:** No build step, reproducible
- **Disadvantage:** Platform-specific

### Option C: Shell Script Entry Point
- Create simple shell script
- Package into Artifact
- Materialize
- Execute via /bin/sh
- **Advantage:** Platform-independent
- **Disadvantage:** Less realistic as test artifact

**Recommendation:** Option A (existing Rust binary) if available; otherwise Option C (shell script)

---

## Phase 24 Scope Definition

### What WILL Be Done
1. Create examples/runtime-consumer/ package
2. Implement minimal external runtime that:
   - Receives materialized artifact representation
   - Extracts to temporary directory
   - Executes entry point
   - Captures exit status
   - Does NOT reconstruct artifact
   - Does NOT compute artifact identity
   - Does NOT persist artifact
3. Create integration tests:
   - Artifact → Materialization → Execution
   - Artifact identity preserved
   - Runtime state is ephemeral
   - Runtime failure doesn't corrupt artifact
   - Repeatable execution
   - Recovery after execution
4. Create PHASE-24-RUNTIME-BOUNDARY.md evidence documentation
5. Create evidence matrix proving each boundary

### What Will NOT Be Done
- No Docker/OCI implementation
- No cloud provider integration (Fly, Render, K8s)
- No deployment scheduler
- No registry/discovery
- No provider-specific code
- No modification to kernel for runtime support

---

## Success Criteria for Phase 24

All of the following must be true:

1. **Artifact → Materialization Works**
   - Existing materializer produces valid output
   - MaterializationResult links back to artifact_identity

2. **Materialization → Runtime Transfer Works**
   - Runtime consumer receives materialized bytes
   - Runtime extracts to local filesystem

3. **Runtime Execution Works**
   - Real executable runs in external process
   - Exit status captured
   - Output observable

4. **Artifact Identity Preserved**
   - artifact.identity before execution == artifact.identity after execution
   - No mutation during execution

5. **Runtime State Is Ephemeral**
   - Runtime temporary directory destroyed
   - Artifact kernel state unchanged
   - Independent recovery succeeds

6. **Execution Failure Doesn't Break Artifact**
   - Runtime exits non-zero
   - Artifact still recoverable
   - Artifact properties still valid

7. **Repeatable Execution**
   - Same artifact executed twice
   - Both executions use same artifact.identity
   - Execution instances have separate runtime IDs

8. **No Kernel Semantics in Runtime**
   - Runtime doesn't compute identity
   - Runtime doesn't persist artifacts
   - Runtime doesn't create lineage
   - Source audit confirms no duplication

9. **Provider Neutral**
   - No provider-specific imports
   - Works on local machine with subprocess
   - No hosted control planes, registries, or credentials

10. **Documentation Complete**
    - PHASE-24-RUNTIME-BOUNDARY.md evidence
    - Evidence matrix with test results
    - Clear verdict: PROVEN or GAP

---

## Next Steps

1. **Design runtime-consumer package structure**
   - Where should it live? (examples/runtime-consumer)
   - What should it depend on? (artifact kernel, standard lib)

2. **Choose executable artifact**
   - What should it do?
   - How should runtime invoke it?

3. **Implement minimal runtime consumer**
   - Accept materialized artifact
   - Extract to temp dir
   - Execute entry point
   - Capture result

4. **Create integration tests**
   - Full lifecycle: Artifact → Execution → Recovery
   - Failure cases
   - Repeatability

5. **Document boundary**
   - Create evidence matrix
   - Document discoverd runtime contract
   - Final verdict

---

## Appendix: Phase 23B Proof Structure (For Reference)

The consumer proof established this pattern:

```rust
// Consumer owns metadata, not artifacts
pub struct BuildMetadata {
    pub config: BuildConfig,        // consumer owns "how to build"
    pub build_logs: String,         // consumer owns "what happened"
    pub success: bool,              // consumer owns "success criteria"
}

// Consumer wraps artifact with metadata
pub struct ConsumerBuild {
    artifact: Artifact,             // kernel owns this
    metadata: BuildMetadata,        // consumer owns this
}

// Consumer doesn't duplicate kernel work
impl ConsumerBuild {
    pub fn identity(&self) -> &str {
        &self.artifact.identity  // read from kernel, never compute
    }
}
```

Phase 24 should follow similar pattern for runtime:

```rust
// Runtime owns execution state, not artifacts
pub struct ExecutionResult {
    pub exit_status: i32,           // runtime owns "what happened"
    pub stdout: Vec<u8>,            // runtime owns "output"
    pub stderr: Vec<u8>,            // runtime owns "error output"
    pub duration_ms: u64,           // runtime owns "timing"
}

// Runtime wraps execution with artifact reference
pub struct ExecutionContext {
    artifact_identity: String,      // link to kernel artifact (read-only)
    materialized_path: PathBuf,     // where runtime extracted artifact
    execution_result: ExecutionResult,  // runtime-specific state
}

// Runtime doesn't duplicate kernel work
impl ExecutionContext {
    pub fn artifact_identity(&self) -> &str {
        &self.artifact_identity  // use kernel identity, never compute
    }
}
```

---

## Conclusion

Phase 23B proved that external systems can use Artifact Engine without reimplementing kernel semantics.

Phase 24 must prove that external execution can consume a materialized artifact without Artifact Engine becoming a runtime.

This requires:
1. Real executable artifact
2. Real process execution
3. Proof that artifact identity survives execution
4. Proof that execution is downstream of artifact state
5. Proof that runtime doesn't duplicate kernel semantics

The conceptual boundary is clear from Phase 23.

Phase 24 must demonstrate it operationally with real execution.

---

END OF PHASE 24 RUNTIME BOUNDARY AUDIT
