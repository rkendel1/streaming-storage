# Phase 16 — External Product Validation

Phase 16 changes the question from architecture selection to product validation:

> Does the Artifact Engine kernel solve a real problem that existing file/package/artifact systems do not solve cleanly?

This phase does **not** implement persistence, attestations, registries, revocation, policy, PKI, or a Unified/Modular representation. It tests whether the Phase 15 kernel explains concrete workflows without introducing concepts that are not part of the artifact itself.

## Validation rule

A workflow is validated if the kernel can explain it using only:

- source;
- pipeline;
- logical artifact;
- artifact identity;
- transformation;
- creation provenance;
- engine execution evidence;
- materialization;
- artifact-associated facts that remain outside identity;
- external/contextual consumer decisions.

A workflow is **not** validated by pretending that Artifact Engine owns registries, trust, credentials, durable fact custody, deployment, or consumer policy.

## Workflow validation matrix

| Workflow | Kernel explains artifact? | Requires concepts outside artifact? | Product signal |
| -- | -- | -- | -- |
| Software artifact | Yes | Deployment policy and runtime are external | Strong |
| AI/agent artifact | Yes | Verification authority and handoff policy are external | Strong |
| Application deployment | Yes | Runtime, rollout, and environment policy are external | Strong |
| Supply-chain artifact | Partially | Attestation custody, revocation, and policy are external | Strong boundary signal |
| Transformation | Yes | Downstream acceptance remains external | Strong |
| Portability | Yes | Future materializer implementation is deferred | Strong |

## 1. Software artifact

```text
source
  ↓
build pipeline
  ↓
logical artifact
  ↓
transform
  ↓
deploy materialization
```

### Kernel explanation

- Source content is selected and normalized.
- A pipeline produces a logical artifact.
- Artifact identity names the logical artifact, not the archive file.
- A transformation can redact, prefix, generate, or compile content and produce a new artifact identity.
- ZIP or TAR materialization produces deployable bytes.

### Boundary

Deployment target, rollout policy, runtime health, and environment trust are not artifact state. They are external consumer decisions.

### Validation result

Validated. Artifact Engine explains the artifact portion without becoming a deployment platform.

## 2. AI/agent artifact

```text
generated or assembled state
  ↓
artifact
  ↓
provenance
  ↓
verification evidence
  ↓
handoff
```

### Kernel explanation

- Generated state can be captured as source content.
- The logical artifact gives the generated output deterministic identity.
- Provenance records the engine-owned creation boundary: source identity and pipeline identity.
- Engine-produced evidence can describe engine execution.
- Handoff can reference artifact identity and optional associated facts.

### Boundary

Whether the generated artifact is acceptable, safe, reviewed, or authorized is a consumer policy decision. Verification services and human review systems may produce facts about the artifact, but Artifact Engine does not become those systems.

### Validation result

Validated. The kernel cleanly separates generated artifact identity from external verification and handoff decisions.

## 3. Application deployment

```text
source repository
  ↓
selected deployment material
  ↓
immutable logical artifact
  ↓
runtime materialization
```

### Kernel explanation

- Selection stages choose deployment-relevant files and exclude unsafe or irrelevant inputs.
- The artifact is immutable once identified.
- Transformations can produce runtime-specific layouts without mutating the original artifact.
- Materialization turns the logical artifact into deployable physical bytes.

### Boundary

Runtime execution, environment variables, service identity, rollout strategy, and deployment approval remain outside the artifact kernel.

### Validation result

Validated. Artifact Engine explains what is being deployed without taking ownership of deployment.

## 4. Supply-chain artifact

```text
artifact
  ↓
claims
  ↓
attestations
  ↓
evidence
  ↓
consumer policy
```

### Kernel explanation

- Artifact identity gives claims, attestations, and evidence a stable reference point.
- Claims, attestations, and evidence are artifact-associated facts, not artifact identity.
- Engine-produced execution evidence is a partial primitive for facts produced by engine operations.
- Consumer policy can evaluate facts without mutating the artifact.

### Boundary

Durable attestation storage, revocation distribution, verifier identity, credentials, PKI, registry state, and policy decisions are external. Artifact Engine can explain the join point, not own the entire supply-chain trust system.

### Validation result

Partially validated with a strong boundary. The kernel explains why artifact identity matters for supply-chain workflows, but durable external fact infrastructure remains outside the current engine.

## 5. Transformation

```text
A
  ↓ redact / filter / repackage
B
```

### Kernel explanation

- A transformation operates on the logical artifact, not materialized ZIP/TAR bytes.
- B is a new logical artifact with a new identity if logical entries/content change.
- A remains intact.
- Existing claims or attestations about A do not silently become facts about B.
- Lineage is a fact to expose or carry explicitly; it is not a consumer decision.

### Boundary

Whether B is acceptable for a consumer, deployable, or equivalent enough to A is contextual.

### Validation result

Validated. This is one of the clearest product differentiators: Artifact Engine models transformation as logical artifact evolution, not archive rewriting.

## 6. Portability

```text
same logical artifact
  ├─ ZIP
  ├─ TAR
  └─ future materializer
```

### Kernel explanation

- The logical artifact identity remains stable across materializers.
- Each materialization has its own output digest.
- Materializer-specific bytes and metadata do not define artifact identity.
- Future materializers can be added without changing what an artifact is.

### Boundary

Storage location, transport protocol, runtime requirements, and consumer policy are outside the materialization boundary.

### Validation result

Validated. Artifact Engine is not a ZIP replacement; it is a logical artifact substrate that can emit ZIP, TAR, or future formats.

## Product validation findings

1. The kernel explains real workflows without becoming a registry, database, trust service, or deployment platform.
2. The strongest product distinction is logical artifact identity independent of physical representation.
3. Transformations are product-defining because they create new logical artifacts instead of mutating archive bytes.
4. Artifact identity is a stable join point for external claims, attestations, evidence, and revocation facts.
5. Supply-chain workflows show why associated facts matter, but also confirm that Artifact Engine should not own trust infrastructure yet.
6. Contextual interpretation remains external: consumers evaluate artifacts and facts according to their own policies.

## Deferred capabilities

The following remain intentionally deferred:

- durable fact storage;
- registry behavior;
- external attestation custody;
- revocation authority;
- policy engine;
- credential and key management;
- distributed synchronization;
- remote artifact storage;
- future materializer encoding choices;
- explicit lineage fact format.

## Phase 16 conclusion

Artifact Engine solves a real product problem when a team needs to identify, inspect, transform, and materialize logical artifacts without confusing those artifacts with archive files, deployment decisions, or trust systems.

The product is more than a ZIP replacement because it gives artifacts stable logical identity and transformation semantics before physical packaging. The engine stops at the artifact boundary; external systems consume artifacts and facts to make policy, trust, deployment, and storage decisions.
