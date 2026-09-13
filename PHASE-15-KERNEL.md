# Phase 15 — Artifact Kernel Definition

Phase 15 defines the smallest durable semantic model justified by Phases 9–14.

It does **not** implement a registry, PKI, trust service, consumer policy system, generic database, remote storage layer, or new storage architecture.

## 1. Artifact Kernel

| Kernel concept | Meaning | Owns | Does not own | Immutable? | Participates in artifact identity? | Must be persisted? | Portable? |
| -- | -- | -- | -- | -- | -- | -- | -- |
| Artifact | A logical description of selected content produced by a pipeline | entries, content digests, manifest, artifact identity, pipeline identity, capabilities, creation provenance | trust state, consumer decisions, registry state, external fact custody | Yes, once identified | Yes; entries, pipeline identity, capabilities, and source identity define it | Yes, if the artifact is to be reused or inspected later | Yes, as logical artifact data and materialized output |
| Artifact Identity | The deterministic digest naming the logical artifact | canonical identity input over entries, pipeline identity, capabilities, and source identity | claims, attestations, evidence, revocation, policy decisions, ZIP/TAR bytes, volatile creation metadata | Yes | It is the identity | Yes | Yes |
| Pipeline Identity | The deterministic digest naming the pipeline specification | source kind, ordered stages, materializer kind, declared capabilities | runtime policy state, source bytes, output archive bytes, external facts | Yes for a given spec | Yes, because artifact identity includes pipeline identity | Yes, if reproducibility/inspection matters | Yes |
| Transformation | A logical operation that produces a new artifact | structural/content changes to artifact entries plus updated content resolver state | consumer trust interpretation, automatic claim transfer, external fact rewriting | The operation spec is immutable; output artifact is immutable | Yes, through changed entries/content and resulting artifact identity | The resulting artifact should be persisted if it is used later | Yes |
| Provenance | Engine-owned creation context for the artifact | source identity, pipeline identity, non-identity creation metadata | history of claims, attestations, evidence, revocations, policy decisions | Source/pipeline provenance is immutable for the artifact; volatile metadata is descriptive | Source identity participates; volatile creation metadata does not | Yes as part of artifact/manifest | Yes |
| Materialization | A physical representation of a logical artifact | output format, output digest, output size, bytes emitted by a materializer | logical identity, semantic truth, trust state, consumer policy | A materialization result is immutable for the emitted bytes | No; materialization has a separate output digest | Persist if the physical output is needed | Yes as format bytes |

### Kernel definition

The Artifact Kernel is:

```text
Source
  ↓
Pipeline
  ↓
Logical Artifact
  ↓
Artifact Identity
  ↓
Transformation
  ↓
New Logical Artifact
  ↓
Materialization
```

The kernel owns deterministic artifact construction, identity, inspection, transformation, provenance of creation, execution evidence for engine-performed operations, and independent physical materialization.

## 2. Minimum semantic layer

| Semantic concept | Classification | Rationale |
| -- | -- | -- |
| Claim | B: artifact-associated historical fact, when explicit | Phase 9 shows a claim can be attached without changing identity, can be wrong, and is not inferred from content. It is not intrinsic artifact state. |
| Attestation | B: artifact-associated historical fact | Phase 11 shows an attestation references an immutable artifact and may attest a claim at a time. It must remain separate from policy decisions. |
| Evidence | B: artifact-associated historical fact | Phase 11 shows evidence is independent of attestation trust decisions and can support multiple interpretations. Engine-produced execution evidence is a real partial primitive. |
| Revocation | B or C: fact about another fact, depending on authority | Phase 11 shows revocation must not rewrite the original attestation. Whether its authority is engine-local or external is not established. |
| Policy Decision | E: derived decision; also D when consumer-specific | Phase 11 and Phase 14 show policies evaluate facts contextually. ACCEPT/REJECT decisions are not artifact state. |

### Minimal candidate

The smallest candidate justified by the evidence is:

```text
Artifact
 ├── identity
 ├── content entries and digests
 ├── pipeline identity
 ├── creation provenance
 └── associated facts, explicitly outside identity
```

This candidate is accepted only as a **conceptual boundary**, not as an implementation requirement. The current engine already has the first four elements. "Associated facts" are justified as policy-agnostic facts linked by artifact identity, but Phase 14 did not prove that Artifact Engine must own their durable storage.

## 3. Identity boundary

`artifact_identity` includes:

- normalized artifact entries;
- entry types, sizes, and content digests;
- pipeline identity;
- canonical capabilities;
- provenance source identity.

`artifact_identity` excludes:

- semantic claims;
- attestations;
- verification or execution evidence;
- revocation facts;
- policy decisions;
- trust relationships;
- credentials;
- volatile creation metadata;
- ZIP/TAR bytes and materializer output digest.

### Counterexamples

Same artifact plus a new claim remains the same artifact because the content and producing pipeline did not change.

Same artifact plus a new attestation remains the same artifact because the attestation is a fact about the artifact, not a change to artifact content.

Same artifact plus new evidence remains the same artifact because evidence may support interpretation, but does not alter the logical entries.

Same artifact plus a revoked attestation remains the same artifact because revocation is a fact about an attestation, not a mutation of artifact content.

Same source plus a structural transformation becomes a new artifact because the logical entries/content relation changed. This is already proven by the existing transformation machinery.

## 4. Provenance boundary

Artifact Engine owns **provenance of artifact creation**:

- source identity;
- pipeline identity;
- descriptive creation metadata that does not alter identity.

Artifact Engine does not automatically own **history of claims, attestations, evidence, and revocations about the artifact**.

Those histories may reference artifact identity, but they are not the same thing as artifact provenance. Merging them would turn the engine into a fact registry or audit database, which Phase 14 did not justify.

No Phase 14 evidence requires changing the existing provenance model. The current distinction remains correct:

- source identity participates in artifact identity;
- volatile creation metadata is preserved descriptively but excluded from identity.

## 5. Materialization boundary

The locked boundary is:

```text
Logical Artifact
      ↓
Materializer
      ↓
Physical representation
```

ZIP, TAR, and future formats are materializations. A materialization has its own output digest and does not define the logical artifact identity.

Semantic facts may eventually:

- travel alongside a materialization;
- be separately retrievable by artifact identity;
- be optionally embedded in a physical representation;
- remain entirely external.

Phase 15 does not choose an encoding. The semantic requirement is only that any carried fact must remain distinguishable from artifact identity and from consumer decisions.

## 6. Artifact lifecycle

### Artifact lifecycle operations

```text
CREATE
  ↓
IDENTIFY
  ↓
INSPECT
  ↓
TRANSFORM
  ↓
NEW ARTIFACT
  ↓
MATERIALIZE
```

- `CREATE`: discover source content and build a logical artifact.
- `IDENTIFY`: compute deterministic artifact identity.
- `INSPECT`: examine artifact/pipeline structure without requiring materialization.
- `TRANSFORM`: apply a logical operation to produce a new artifact.
- `NEW ARTIFACT`: treat transform output as its own immutable logical artifact.
- `MATERIALIZE`: emit physical bytes in a selected format.

### Associated fact lifecycle

```text
CLAIM
ATTEST
EVIDENCE
REVOKE
```

These operations produce or update facts about artifacts or about other facts. They are not artifact identity operations. They may be supported by future policy-agnostic APIs, but durable custody is not currently an engine promise.

### External consumer lifecycle

```text
CONSUME
  ├─ collect artifact and facts
  ├─ apply consumer policy
  └─ decide ACCEPT / REJECT / IGNORE
```

Consumption is external/contextual. Artifact Engine may supply inputs, but does not own the decision.

## 7. What Artifact Engine explicitly does not promise

| Concern | Boundary |
| -- | -- |
| Trust | Does not own it, but can carry policy-agnostic facts that a trust system evaluates |
| Authorization | Owns execution authorization for engine operations; does not own consumer authorization policy |
| Identity | Owns artifact and pipeline identity; does not own human, organization, credential, or verifier identity |
| Signatures | Does not own them, but could carry signature facts if a future fact model is established |
| Key management | Does not own it and should not model it as kernel state |
| PKI | Does not own it and should not model it as kernel state |
| Revocation authority | Does not own it, but may carry revocation facts if a future fact model is established |
| Consumer policy | Does not own it, but can expose inputs that policies evaluate |
| Registry | Does not own it and should not model registry behavior in the kernel |
| Distributed synchronization | Does not own it and should not model it without an external system boundary |
| Remote storage | Does not own it; materialized artifacts or fact records may be stored elsewhere |
| Application runtime | Does not own it; application artifacts can exist without runtime execution ownership |
| Deployment | Does not own it; deployment systems consume artifacts and facts |

## 8. Final concept table

| Concept | Artifact Engine owns? | Identity-bearing? | Immutable? | External/contextual? |
| -- | -- | -- | -- | -- |
| Artifact | Yes | Yes | Yes | No |
| Pipeline | Yes | Yes | Yes, for a given spec | No |
| Provenance | Partially: creation provenance yes; external fact history no | Source identity yes; volatile metadata no | Creation provenance yes | External fact history is contextual/external |
| Claim | Not as intrinsic artifact state; may expose as associated fact later | No for artifact identity | Individual claim fact should be immutable if recorded historically | Often external |
| Attestation | Not as intrinsic artifact state; engine evidence can act as a partial fact | No for artifact identity | Yes as historical fact | Usually external authority |
| Evidence | Engine owns evidence for engine-performed operations; external evidence is outside | No for artifact identity | Yes as historical fact | Often external |
| Revocation | No current primitive | No for artifact identity | Revocation fact should be immutable; current acceptability is derived | Yes |
| Policy decision | No | No | Decision record may be historical, but decision result is contextual | Yes |
| Materialization | Yes | Has output digest, separate from artifact identity | Yes for emitted bytes | No, though storage location is external |

## Phase 15 conclusion

Artifact Engine becomes a product by owning a precise artifact kernel: deterministic logical artifacts, stable identity, transformations, creation provenance, inspection, engine execution evidence, and independent materialization.

The minimum semantic layer is not a registry. It is a boundary: claims, attestations, evidence, and revocations may be artifact-associated facts, but they remain outside artifact identity and outside consumer decisions. Consumer policy, trust, authority, credentials, and shared context belong outside the engine.
