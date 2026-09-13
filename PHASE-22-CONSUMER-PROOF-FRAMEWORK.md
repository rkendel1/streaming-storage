# Phase 22 — Consumer Proof #1 Framework

This phase validates that the Phase 21 boundary is operationally real by integrating an external system that uses the Artifact Engine kernel without reimplementing any kernel semantics.

---

## Transition from Research to Production Engineering

**Phase 20:** "What does the kernel actually do?" → PROVEN KERNEL
**Phase 21:** "Where exactly does the kernel stop?" → EXPLICIT BOUNDARY
**Phase 22:** "Can an external system actually build on the kernel?" → OPERATIONAL PROOF

Phase 22 is the first genuine production engineering exercise. Its success is not measured in features added but in a negative test: external systems can consume the kernel without reimplementing core semantics.

---

## Consumer Proof #1 Concept

An external system should demonstrate:

```
Source Code
     ↓
Artifact Engine: Build Source Artifact
     ↓
Build System: Invoke Compiler/Builder
     ↓
Artifact Engine: Transform to Build Artifact
     ↓
Artifact Engine: Persist Build Artifact
     ↓
Artifact Engine: Recover Build Artifact
     ↓
Artifact Engine: Materialize to ZIP/TAR
     ↓
Consumer Owns: Distribution, Deployment, Installation
```

The consumer:
- Uses Artifact Engine for identity (no parallel identity system)
- Uses Artifact Engine for persistence and recovery
- Uses Artifact Engine for transformation lineage
- Uses Artifact Engine for composition (if multi-artifact builds)
- Owns domain semantics: what to compile, how to compile, what defines a successful build

---

## Why a Build Pipeline?

A build pipeline is the strongest first proof because it:

1. **Exercises the full artifact lifecycle:** Source → build → persist → recover → materialize
2. **Tests transformation:** Source artifact → compiled artifact via build transform
3. **Tests composition** (if needed): Multiple source artifacts → single build product
4. **Tests persistence/recovery:** Artifacts must survive across build runs
5. **Tests lineage:** Build artifacts should be traceable to source + build process
6. **Owns clear domain semantics:** "How to compile" is external to the kernel
7. **Avoids false confidence:** A build system cannot accidentally duplicate kernel by accident

---

## Consumer Proof #1 Acceptance Criteria

The proof succeeds if and only if the consumer demonstrates all of the following:

### Identity and Persistence

- [ ] Consumer obtains artifact identity exclusively from `Artifact::from_parts`
- [ ] Consumer does not compute or store a parallel artifact identity system
- [ ] Consumer persists artifacts through `LocalArtifactStore::persist`
- [ ] Consumer recovers artifacts through `LocalArtifactStore::recover`
- [ ] Recovered artifacts have the same identity as originally persisted

### Transformation and Lineage

- [ ] Build process applies transformations through `ArtifactTransform::apply`
- [ ] Output artifacts have observable lineage via `Artifact::lineage()`
- [ ] Lineage traces correctly: source artifact → build transform → output artifact
- [ ] Consumer does not manually create or manipulate `TransformationRecord`

### Composition (if applicable)

- [ ] Multi-artifact builds use `CompositionInput::compose`
- [ ] Composition result has deterministic identity
- [ ] Input artifacts are not mutated by composition
- [ ] Consumer does not manually construct composed artifact identity

### Materialization

- [ ] Final build products are materialized through `ZipMaterializer` or `TarMaterializer`
- [ ] Materialization produces deterministic output digests
- [ ] Consumer does not implement custom ZIP/TAR logic

### No Semantic Duplication

- [ ] Consumer does not implement artifact identity computation
- [ ] Consumer does not implement persistence/recovery logic
- [ ] Consumer does not implement transformation lineage tracking
- [ ] Consumer does not implement composition determinism calculation
- [ ] Consumer does not implement content digest validation
- [ ] Consumer does not maintain a parallel artifact catalog or registry

### Domain Semantics Owned by Consumer

- [ ] Consumer owns "what to compile" (source selection, build inputs)
- [ ] Consumer owns "how to compile" (toolchain, compiler flags, build steps)
- [ ] Consumer owns "what defines success" (test validation, output verification)
- [ ] Consumer owns policy about when to rebuild
- [ ] Consumer owns distribution and deployment decisions
- [ ] Consumer owns caching strategy above the artifact layer

### External Metadata Attachment

- [ ] Consumer can attach domain metadata (build logs, test results, performance metrics) without corrupting artifact identity
- [ ] Metadata is stored outside the artifact (e.g., in a separate database keyed by artifact identity)
- [ ] Kernel artifact remains unchanged and valid regardless of external metadata

### Boundary Preservation

- [ ] Kernel can be used independently of the consumer system
- [ ] Consumer can be replaced without redefining artifact identity
- [ ] A recovered artifact remains the same artifact from both kernel and consumer perspective
- [ ] No hidden adapter, cache, or registry layer is required to make integration work

### No Authorization Leakage

- [ ] Consumer defines its own authorization policy (if needed) using `CapabilityPolicy`
- [ ] Consumer does not embed authorization decisions in artifact identity
- [ ] Authorization failure does not corrupt persistent artifacts

---

## Failure Scenarios (Evidence of Boundary Problems)

If the consumer cannot achieve acceptance criteria, investigation must answer:

**If consumer needs to compute artifact identity:**
- Is the kernel identity contract unclear?
- Is `Artifact::from_parts` too restrictive?
- Is there a legitimate use case the kernel should handle?

**If consumer needs to implement persistence/recovery:**
- Is `LocalArtifactStore` insufficient?
- Is there a legitimate requirement beyond filesystem storage?
- Is the recovery API inadequate?

**If consumer needs to track lineage manually:**
- Does `TransformationRecord` not capture the needed semantics?
- Is the lineage model incomplete for the consumer's workflow?

**If consumer needs a parallel artifact registry:**
- Does the kernel lack needed discovery mechanisms?
- Is artifact identity insufficient for the consumer's use case?
- Should the kernel provide artifact naming/tagging?

**If consumer needs to reimplement composition:**
- Is `CompositionInput` insufficient for multi-artifact workflows?
- Is the composition result identity wrong for the consumer's model?

Any failure scenario means the Phase 21 boundary definition is incomplete and must be revised before proceeding.

---

## Success Scenario

Consumer Proof #1 succeeds when:

```
                    Consumer (Build System)
                           │
        ┌──────────────────┴──────────────────┐
        │                                     │
    owns domain                            uses kernel
        │                                     │
  - source selection               - Artifact identity
  - compilation                    - Persistence
  - verification                   - Recovery
  - authorization                  - Transform/lineage
  - distribution                   - Composition
                                   - Materialization
        │                                     │
        └──────────────────┬──────────────────┘
                           │
                      Artifacts
                      (stable,
                       reusable,
                       portable)
```

And an engineer can demonstrate:
- "How to prove artifact identity is correct?" → "Recompute it via `Artifact::from_parts`"
- "How to recover a build artifact?" → "Use `LocalArtifactStore::recover(identity)`"
- "How to track what produced an artifact?" → "Check `Artifact::lineage()`"
- "How to combine multiple build outputs?" → "Use `CompositionInput::compose`"
- "How to distribute a build?" → "Materialize via `ZipMaterializer` or `TarMaterializer`"
- "What does the build system own?" → "Compilation, verification, distribution"
- "What does the kernel own?" → "Identity, persistence, lineage, composition, materialization"

---

## Implementation Guidance

### Recommended First Consumer Structure

```rust
// consumer/src/lib.rs

pub struct BuildArtifact {
    artifact: Artifact,  // kernel owns this
    build_logs: String,  // consumer owns this
    test_results: TestSummary,  // consumer owns this
}

impl BuildArtifact {
    pub fn from_build_process(
        source: &Artifact,
        build_spec: &BuildSpec,
        store: &LocalArtifactStore,
    ) -> Result<Self, BuildError> {
        // 1. Apply build transform to source
        let built = apply_build_transform(source, build_spec)?;
        
        // 2. Validate build (consumer responsibility)
        let validation = validate_build(&built)?;
        
        // 3. Persist artifact (kernel responsibility)
        let resolver = build_content_resolver(&built)?;
        store.persist(&built, &resolver)?;
        
        // 4. Return consumer wrapper (kernel artifact + domain data)
        Ok(Self {
            artifact: built,
            build_logs: validation.logs,
            test_results: validation.tests,
        })
    }
    
    pub fn artifact_identity(&self) -> &str {
        &self.artifact.identity
    }
    
    pub fn recover(identity: &str, store: &LocalArtifactStore) -> Result<Self, BuildError> {
        let recovered = store.recover(identity)?;
        let artifact = recovered.artifact().clone();
        // Re-fetch consumer metadata from external store (database, etc.)
        let (logs, tests) = fetch_metadata(identity)?;
        Ok(Self {
            artifact,
            build_logs: logs,
            test_results: tests,
        })
    }
}
```

### Key Pattern: Wrapper, Not Reimplementation

```rust
// Good: Wrapper pattern
pub struct ConsumerArtifact {
    kernel_artifact: Artifact,
    consumer_metadata: ExternalData,
}

// Bad: Reimplementation pattern
pub struct ConsumerArtifact {
    identity: String,  // ❌ Don't recompute identity
    entries: Vec<Entry>,  // ❌ Don't reimplement artifacts
    persistence_cache: HashMap<String, Artifact>,  // ❌ Don't cache
}
```

---

## Phase 22 Completion Criterion

Phase 22 is complete when:

1. **Consumer integration is implemented** using the wrapper pattern
2. **All acceptance criteria are met** (checklist above)
3. **Artifact lifecycle is exercised** (source → build → persist → recover → materialize)
4. **No kernel semantics are reimplemented** in the consumer
5. **The boundary is operationally proven** (external system uses kernel without duplication)

---

## Outcome

If Consumer Proof #1 succeeds:
- The Phase 21 boundary is operationally valid
- External systems can confidently build on the kernel
- The kernel is production-ready for integration

If Consumer Proof #1 fails:
- A specific boundary question is unanswered
- Phase 21 findings are incomplete and must be revised
- Return to boundary audit rather than proceeding with more integration

Either outcome is valuable: success means the kernel works in practice; failure means the boundary definition needs refinement before external systems depend on it.

---

## What Phase 22 Is NOT

Phase 22 is not:
- Adding new kernel architecture (registries, databases, trust systems)
- Expanding the kernel (more transforms, more materializers)
- Building a complete deployment system
- Implementing a production artifact service

Phase 22 is a proof that the boundary works. That is its only purpose.
