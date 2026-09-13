# Phase 21 — Kernel Boundary & Public Contract Audit

This audit resolves the two remaining boundary questions and defines the explicit contract between the Artifact Engine kernel and external systems.

---

## Question 1: Does Authorization Belong to the Artifact Kernel?

### Evidence Examined

**Authorization types in kernel:**
- `CapabilityPolicy` (trait for policy evaluation)
- `AuthorizationDecision` (record of grant/deny decision)
- `ExecutionEvidence` (record of execution with authorization details)

**Authorization in production paths:**
- `PipelineSpec::build_from_directory()` — No authorization check
- `PipelineSpec::build_with_authorization()` — Applies policy, gates execution, records evidence
- Core operations (transform, compose, persist, recover, materialize) — No authorization checks

**Authorization semantics:**
- Optional pathway: Applications can call either `build_from_directory` (no policy) or `build_with_authorization` (with policy)
- Policy enforcement: Delegated to `CapabilityPolicy` implementations (AllowAllPolicy, AllowListPolicy, or custom)
- Evidence recording: Applications construct `AuthorizationDecision` and `ExecutionEvidence` after decisions

### Decision

**Authorization is EXTERNAL to the Artifact Kernel.**

**Rationale:**
1. The kernel exports authorization primitives (policy interface, decision/evidence types) so external systems can build authorization on top of the kernel.
2. No kernel operation requires authorization to proceed. All operations work without authorization.
3. Authorization is optional: applications choose whether to enforce policies.
4. The kernel provides the building blocks; policy enforcement belongs to the caller.

**Corollary:** The kernel correctly provides `CapabilityPolicy` and `ExecutionEvidence` as supporting primitives for external systems to build authorization. The kernel does not enforce authorization; external systems do.

### Contract

- **Kernel exports:** `CapabilityPolicy` trait, `AuthorizationDecision`, `ExecutionEvidence`, reference implementations (AllowAllPolicy, AllowListPolicy)
- **Kernel does not:** Require authorization for any operation
- **External systems:** Define what "authorized" means, apply policies, record decisions
- **Integration point:** `PipelineSpec::build_with_authorization(policy)` for authorization-aware builds

---

## Question 2: What Is the Precise Semantic Difference Between Transformation Lineage and Composition Provenance?

### Evidence Examined

**Transformation lineage (`TransformationRecord`):**
- Appended to artifact on transform output
- Records: input artifact identity, transform identity, transform kind
- Single-input only (one parent artifact)
- Not part of artifact identity
- Purpose: Observing derivation relationships between artifacts

**Composition provenance (`source_identity`):**
- Stored in artifact's `Provenance` on composition result
- Records: composition result identity computed from input identities
- Multi-input (multiple parent artifacts)
- Participates in artifact identity computation (source_identity affects identity)
- Purpose: Recording that artifact was created from a composition operation

**Current implementation:**
- Transform: `A --[T]--> B` (B has TransformationRecord pointing to A)
- Composition: `[A, B] --[compose]--> C` (C has provenance.source_identity recording composition inputs)
- No conflation: Composition doesn't create TransformationRecord; transformation doesn't create provenance updates

### Decision

**Transformation lineage and composition provenance are semantically distinct. No change required.**

**Rationale:**

1. **Semantically correct distinction:**
   - Transformation: One artifact produces another artifact via a logical operation
   - Composition: Multiple independent artifacts combine into one via orchestration

2. **Implementation is correct:**
   - Composition uses provenance (creation source context)
   - Transformation uses lineage (derivation relationship)
   - These are not duplicates; they serve different purposes

3. **Multi-input lineage semantics:**
   - Currently: Composition records inputs in source_identity, not lineage
   - This is correct: Composition is not transformation
   - If multi-input transformation becomes necessary later, a new lineage record type can be introduced by evidence
   - Current "single-input only" describes proven semantics, not eternal limitation

### Contract

- **Transformation:** Single-input operation producing new artifact with lineage record (observational)
- **Composition:** Multi-artifact operation producing new artifact with provenance (creation source)
- **Lineage:** Records `(input_artifact_identity, transform_identity, transform_kind)` for derivation tracing
- **Provenance:** Records `(source_identity, pipeline_identity, creation_metadata)` for creation context
- **Semantic declarations:** Optional producer claims about artifact meaning (e.g., "compiled binary"), non-identity-bearing
- **No conflation:** Composition is not recorded as transformation; lineage is not used to define artifact identity

---

## Artifact Engine Kernel Boundary

```
                     EXTERNAL SYSTEM
                           │
          ┌────────────────┴────────────────┐
          │                                 │
    supplies/decides                  consumes
    external concerns               kernel semantics
          │                                 │
   ┌──────┴──────────┐           ┌─────────┴─────────────┐
   │                 │           │                       │
 Trust            Policy       Artifact                Identity
 Claims           Choice       Lifecycle              (deterministic,
 Attestations     Registry     Transform              stable,
 Revocation       Runtime      Composition            portable)
 Deployment       Selection    Materialization        │
   │                 │           │                    Persistence
   │                 │      Lineage                   Recovery
   │                 │      (single-input,           Composition
   │                 │       provenance-             Transformation
   │                 │       distinguished)          Provenance
   │                 │                              Semantic decl.
   │                 │                              Materialization
   │                 │           │                       │
   └─────────────────┘           └───────────────────────┘
                                   Artifact Engine
                                   (production-proven)
```

---

## Public Kernel Surface

### Core Types (Public Contract)

**Always import these if you use Artifact Engine:**
- `Artifact` — Logical artifact with identity, entries, provenance, lineage
- `Provenance` — Source identity and pipeline identity (immutable for artifact)
- `TransformationRecord` — Lineage record (input identity, transform identity/kind)
- `ArtifactEntry` — Normalized path with content digest and size
- `Capability` — Declared capability for artifact
- `PipelineSpec` — Pipeline specification for artifact build
- `LocalArtifactStore` — Durable artifact persistence and recovery
- `RecoveredArtifact` — Artifact recovered from storage with content validation
- `ArtifactTransform` — Trait for implementing transformations
- `CompositionInput` — Multi-artifact composition specification
- `ContentResolver` — Trait for providing entry content
- `MaterializationResult` — Result of materializing artifact to physical format

### Supporting Types (Integration Points)

**Use these if you implement custom infrastructure:**
- `CapabilityPolicy` — Trait for defining authorization policies
- `AuthorizationDecision` — Record of authorization decision (grant/deny)
- `ExecutionEvidence` — Record of execution with authorization and stage trace
- Custom `ArtifactTransform` implementations
- Custom `ContentResolver` implementations
- Custom `CapabilityPolicy` implementations

### Public API Wrappers (User-Facing)

**Use these if you want type-safe high-level APIs:**
- `ArtifactSDK` — Entry point for recipe/pipeline building
- `ArtifactPipeline` — Type-safe pipeline interface with inspection and building
- `PublicArtifact` — User-facing artifact wrapper
- `PublicExecutionEvidence` — User-facing evidence wrapper

### NOT Public Contract (Internal Implementation)

Do NOT depend on:
- `PipelineState` — Internal execution state
- `SourceBackedArtifact` — Internal artifact representation during building
- `TransformedContentResolver` — Internal content resolver
- Internal stage specifications (`GenerateStageSpec`, `TransformStageSpec`, etc.)

---

## External Integration Requirements

### What External Systems Must Provide

1. **Durable persistence beyond LocalArtifactStore** (if needed)
   - LocalArtifactStore uses filesystem; if you need cloud storage, implement your own storage layer
   - Artifact kernel provides the identity contract; you provide durability

2. **Trust decisions and authorization policies** (if needed)
   - Kernel provides CapabilityPolicy interface; you define what "authorized" means
   - Use `CapabilityPolicy` trait or `build_with_authorization` for policy-aware builds

3. **External fact management** (claims, attestations, revocation)
   - Kernel does not own these
   - You can attach them to artifacts by referencing artifact identity
   - You own the storage and validation of external facts

4. **Registry and discovery** (if needed)
   - Kernel does not provide artifact lookup/discovery
   - You implement registry if you need to find artifacts by name, tag, or other metadata
   - Artifact kernel provides stable identity as the key

5. **Deployment and runtime systems**
   - Kernel provides artifact definition and materialization
   - You provide deployment, scheduling, execution frameworks

### What External Systems Must Never Reimplement

Do NOT reimplement:
- **Artifact identity computation** — It is deterministic; recomputation will diverge
- **Persistence/recovery logic** — LocalArtifactStore is the canonical implementation
- **Composition determinism** — CompositionInput ensures deterministic result identity
- **Transformation lineage** — LineageRecord semantics are non-negotiable
- **Entry normalization and validation** — Path rules and conflict detection are kernel-enforced
- **Content digest validation** — Digest matching on persist/recover is critical
- **Materialization semantics** — ZIP/TAR format consistency is essential

If you reimplement these, your system will silently diverge from the kernel's guarantees.

---

## Nine-Question Completion Test

An external engineer should answer these from the contract alone:

### 1. What does Artifact Engine own?

**Answer:** Artifact identity, pipeline semantics, transformation lineage, composition, provenance of creation, durable persistence, recovery with validation, materialization. It does not own trust, claims, attestations, registry, discovery, deployment, or consumer policy.

### 2. What identity guarantees do I get?

**Answer:** Deterministic (same content+pipeline→same identity), stable (persists across processes/stores), portable (environment-independent), immutable (once computed, never changes), and recomputable (same inputs always produce same identity).

### 3. What does persistence guarantee?

**Answer:** Artifact metadata and entry content are written to durable storage, validated on write, revalidated on recovery. Identity is verified on recovery. Corruption fails closed (identity mismatch or content digest mismatch causes recovery failure). Repeated persistence/recovery preserves artifact identity.

### 4. What is provenance versus lineage?

**Answer:** Provenance is creation source context (source identity, pipeline identity, creation metadata). Provenance participates in artifact identity. Lineage is transformation derivation (which artifact was input, what transform, what transform kind). Lineage does not participate in artifact identity. Composition records inputs in provenance, not lineage.

### 5. What does a semantic declaration mean—and what does it not mean?

**Answer:** A semantic declaration is a producer-supplied, non-identity claim about what the artifact represents (e.g., "compiled binary"). It survives persistence/recovery. It does NOT affect artifact identity, NOT participate in transformation, and is NOT interpreted by the kernel. It is for external systems to interpret.

### 6. Does Artifact Engine authorize me, or do I authorize Artifact Engine?

**Answer:** You authorize Artifact Engine. The kernel provides CapabilityPolicy interface and authorization-aware build paths (build_with_authorization), but the kernel does not require authorization. You define what "authorized" means and apply policies when you choose.

### 7. What trust/claim/attestation machinery must I supply?

**Answer:** All of it. Artifact Engine does not own trust, claims, attestations, or revocation. You build these external to the kernel. You reference artifacts by their stable identity and manage your own fact storage.

### 8. What must I never reimplement around the kernel?

**Answer:** Artifact identity computation, persistence/recovery logic, composition result identity, transformation lineage semantics, entry normalization, content digest validation, materialization consistency. If you reimplement these, your system silently diverges from the kernel.

### 9. What can I safely persist, materialize, transform, and recover?

**Answer:** Any artifact produced by the kernel (native or composed or transformed). You can persist artifacts, recover them by identity, apply transformations (producing new artifacts), compose multiple artifacts, and materialize to ZIP/TAR. All operations are safe; identity and lineage are preserved through all lifecycle transitions. Composition does not mutate inputs. Transformation does not redefine artifact identity (output identity is recomputed fresh).

---

## Phase 21 Findings

### Authorization

**Status: EXTERNAL**
- Decision: Kernel exports authorization primitives; external systems enforce policies
- Evidence: No kernel operation requires authorization; it's optional at application level
- Contract: Use `CapabilityPolicy` and `build_with_authorization` for policy-aware builds

### Multi-Input Lineage

**Status: SEMANTICALLY CORRECT**
- Decision: Composition is not transformation; provenance is correct mechanism
- Evidence: Composition produces deterministic result identity; lineage is single-input by design
- Contract: Composition records inputs in provenance; transformation records in lineage

### No Defects Found

- No correctness issues in authorization implementation
- No correctness issues in composition/lineage distinction
- No evidence of hidden state or implicit behavior
- Boundary diagram and contract are clear and implementable

---

## Phase 21 Completion Checklist

- ✓ Resolved authorization boundary from evidence (external, not kernel-enforced)
- ✓ Clarified provenance/lineage/semantic distinction (semantically correct, no change needed)
- ✓ Defined public kernel surface (core types, supporting types, wrappers, internal details)
- ✓ Stated external integration requirements (what to provide, what never to reimplement)
- ✓ Created authoritative boundary diagram
- ✓ Answered nine completion questions
- ✓ Assessed all defects (none found)
- ✓ Locked the contract for external integration
- ✓ All findings are evidence-based, not philosophical
- ✓ No production code changes required (no defects discovered)

**Phase 21 audit complete. Kernel boundary is now explicit and evidence-backed.**

---

## What This Means

The Artifact Engine kernel is now production-ready for external integration with an explicit contract:

1. **Kernel owns:** Identity, lifecycle, persistence, recovery, lineage, composition
2. **External owns:** Trust, policy, registry, deployment, runtime
3. **Authorization:** Kernel provides primitives; external systems enforce
4. **No ambiguity:** The boundary diagram and nine-question test define integration exactly

External engineers can now confidently build systems on the kernel without reimplementing core semantics or discovering hidden behavior.

The next phase is not architectural research. It is **Consumer Proof #1**: demonstrating that an independent system can build on the kernel without duplicating identity, persistence, lineage, composition, or materialization semantics.
