# Phase 14 — Boundary and Missing-Primitives Investigation

Phase 14 answers a product-boundary question, not a representation-selection question:

> Which missing capabilities are responsibilities of Artifact Engine, and which belong outside the engine?

This phase does **not** select Unified, Modular, or Contextual. It also does **not** implement a registry, facts database, revocation store, metadata subsystem, or policy engine.

## Evidence base

Phase 14 uses the following repository evidence:

- Phase 9: semantic declarations are claims, not identity; consumers must not infer semantic authority from content alone (`tests/pipeline.rs:2786-3146`).
- Phase 10: consumers can use distributed records linked by artifact identity; no investigated scenario required a single atomic record (`tests/phase10_investigation.rs:767-835`).
- Phase 11: the relationship graph separates artifacts, claims, attestations, evidence, revocation, provenance, and policy decisions (`tests/phase11_lifecycle.rs:497-552`).
- Phase 12: Unified, Modular, and Contextual could all encode the abstract relationship graph, so model choice was not evidence-determined (`tests/phase12_representation.rs:337-367`).
- Phase 13B: real engine primitives prove artifact identity, transformation, authorization/evidence identity, serialization bytes, and policy divergence, while persistence, revocation, shared context, and durable external facts remain unproven (`PHASE-13-PRODUCTION-BOUNDARY.md:19-57`).

## 1. Missing primitive inventory

| Missing capability | Phase 11 relationship requiring it | Phase 13B gap evidence | Existing partial engine primitive | What would have to exist to satisfy it | Inherently Artifact Engine responsibility? |
| -- | -- | -- | -- | -- | -- |
| Durable external facts | Claims, attestations, evidence, revocation, and policy are separate from artifact identity | Phase 13B marks persistence/recovery untestable and evidence persistence requiring new infrastructure | Deterministic serialization for `Artifact`, `Manifest`, `AuthorizationDecision`, and `ExecutionEvidence` | A durable fact lifecycle outside transient Rust values | Not proven. Durability of facts is likely external unless the engine promises fact custody |
| Revocation | Revocation is separate mutable state and must not rewrite historical attestation | Phase 13B records revocation as requiring a production primitive | None | A revocation fact that references an attestation or artifact-associated fact without mutating history | Not proven. The engine may expose identifiers needed by revocation, but current evidence does not force ownership |
| Evidence persistence | Evidence persists across attestation changes and supports multiple interpretations | Phase 13B can serialize evidence but cannot recover persisted evidence state | `ExecutionEvidence::to_canonical_bytes`, `ExecutionEvidence::identity` | A durable evidence record store or portable evidence artifact | Not proven. Execution evidence is an engine output; long-term custody may be external |
| Attestation persistence | Multiple attestations coexist for one artifact | Phase 13B can create multiple execution evidence values but has no attachment/storage boundary | `AuthorizationDecision`, `ExecutionEvidence` | Durable attestation/evidence records linked to artifact identity | Not proven. Attestation may be produced by external verifiers |
| Claim persistence | Claim is independent of artifact identity and can change without artifact changing | Phase 13B proves claim does not affect identity but no claim storage survives process destruction | `Artifact.semantic_declaration`, canonical artifact serialization | A durable claim record or explicit portable claim field with semantics | Ambiguous. Minimal semantic declarations exist, but trust-bearing claims are not proven engine-owned |
| Provenance persistence | Artifact has immutable provenance; lineage must be traceable | Provenance fields persist inside real `Artifact`; transformation lineage remains underdetermined | `Provenance`, `Manifest`, `Artifact.provenance` | Parent/child lineage records if lineage beyond source and pipeline identity is required | Partially yes. Artifact creation provenance is engine-owned; external lineage graph persistence is not proven |
| Recovery | Relationship graph must survive destruction of in-memory state | Phase 13B rejects in-memory clone as persistence/recovery | Serialization bytes only; no deserialize/repository API | A recovery contract for artifacts and any associated facts | Artifact recovery from materialized outputs is not fully established; external fact recovery is not engine-proven |
| Shared consumer context | Multiple consumers may differ on same attestation | Phase 13B says actual shared context requires new infrastructure | `CapabilityPolicy` trait enables policy evaluation per call | Shared authoritative fact source plus independent consumer views | No. Shared consumer context is an external consumption architecture |
| Policy state | Consumer policy selects which attestation/evidence to trust | Phase 13B proves policy divergence with `AllowAllPolicy` and `AllowListPolicy` | `CapabilityPolicy`, `policy_identity`, requested/granted/denied capabilities | Durable policy definitions and state management | No. The engine can evaluate supplied policies, but should not own organizational policy |
| Historical state reconstruction | Original attestation and later revocation must both remain observable | Phase 13B shows evidence values can remain unchanged but no historical fact store exists | Canonical identities for evidence/decisions | Append-only or otherwise history-preserving record source | Not proven. Historical reconstruction is likely registry/audit infrastructure |
| Synchronization | Distributed facts linked by identity may need coordination | Phase 10 found no scenario required atomic single record; Phase 13B did not establish synchronization | Artifact identity as stable join key | External synchronization semantics if consumers require consistency | No current evidence. Synchronization belongs to systems that own fact custody |
| Concurrency/atomicity across related facts | Claims, attestations, evidence, revocations have relationships but remain separable | Phase 10 explicitly found no scenario required atomicity; Phase 13B marks multi-fact persistence absent | Stable artifact identity and deterministic fact identities | Transaction boundary if a product requires coordinated multi-fact updates | Not proven. Do not introduce transaction semantics into Artifact Engine without new evidence |

## 2. Product boundary reconstruction

### What an Artifact needs to know to remain an Artifact

The existing architecture shows that an artifact needs:

- normalized entries and content digests;
- artifact identity derived from entries, pipeline identity, capabilities, and source identity;
- manifest data that describes the logical artifact;
- artifact creation provenance sufficient to identify source and pipeline;
- capabilities required to produce or materialize the artifact;
- content resolution and materialization boundaries.

These are intrinsic to being a logical artifact in the current engine.

### What external consumers need to decide what to do with an Artifact

Consumers may need:

- semantic claim or declared purpose;
- attestation by a verifier;
- verification evidence;
- revocation or supersession state;
- trust relationships;
- consumer policy;
- organizational authority;
- credentials;
- deployment, registry, or runtime context.

These are not intrinsic to artifact identity. Phase 9 shows the same content can have different declared meanings. Phase 11 shows attestations, evidence, revocation, and policy decisions have distinct relationships. Phase 13B shows the real engine can evaluate supplied policy, but does not own durable policy or trust state.

## 3. Facts-vs-decisions boundary

The current boundary is:

```text
Artifact-associated factual state
  ├─ artifact identity
  ├─ entries and content digests
  ├─ provenance
  ├─ claim/declaration, if explicitly attached
  ├─ attestation/evidence, if represented as facts
  └─ revocation/supersession, if represented as facts

Consumer-specific decision state
  ├─ policy
  ├─ trust relationship
  ├─ authority model
  ├─ credentials
  └─ ACCEPT / REJECT / IGNORE decision
```

Phase 14 finding:

- The engine can own or expose facts without becoming a trust system only if the facts remain policy-agnostic and identity-addressed.
- The engine must not convert facts into trust decisions.
- A statement such as "Verifier V observed Artifact A at time T" is a fact candidate.
- A statement such as "Consumer X accepts Artifact A" is a contextual policy decision and should remain outside the artifact kernel.

## 4. Revocation semantic analysis

Revocation is not one concept. The product-level distinctions are:

| Statement | Category | Notes |
| -- | -- | -- |
| Attestation existed | Immutable historical fact | Must not be rewritten by later state |
| Attestation was superseded | Historical relationship or lifecycle fact | Indicates a later fact replaces or narrows an earlier one |
| Attestation was revoked | New fact about an earlier fact | Should not mutate the attestation itself |
| Attestation is currently unacceptable | Derived current state | Depends on revocation facts, time, and policy |
| Consumer rejected attestation | Policy-specific decision | Belongs to consumer context |

Phase 14 finding:

- Artifact Engine does not currently need to decide whether a revoked attestation is acceptable.
- If Artifact Engine ever represents revocation, it should be as a fact that references another fact, not as mutation of attestation history.
- The need to preserve original attestation plus revocation is proven by Phase 11, but the need for Artifact Engine to own the revocation store is not proven.

## 5. Persistence boundary analysis

Artifact durability and external semantic-fact durability are different.

### Existing engine durability-adjacent primitives

- `Artifact::to_canonical_bytes` serializes artifact state.
- `Manifest::to_canonical_json` serializes manifest state.
- Materializers produce deterministic ZIP/TAR bytes and report materialization results.
- `ExecutionEvidence::to_canonical_bytes` serializes execution evidence.

### Not currently established

- durable storage API;
- deserialization/recovery API for external facts;
- content-addressed fact store;
- portable fact bundle;
- transaction boundary;
- synchronization contract;
- registry or shared fact source.

Phase 14 finding:

- The engine promises deterministic construction and materialization behavior, not general-purpose storage.
- External facts may need to be durable, content-addressed, serialized, portable, recoverable, transactional, or independently addressable, but Phase 13B did not prove those guarantees belong inside Artifact Engine.
- If another product layer wants durable semantic fact custody, it should be treated as a separate responsibility unless future evidence proves it is intrinsic to artifact construction/materialization.

## 6. Contextual model analysis

The Contextual model is no longer best understood as a peer artifact-state representation.

Its real shape is:

```text
Artifact facts
      ↓
External/shared fact source
      ↓
Consumer-specific interpretation
```

That makes Contextual an external consumption architecture involving:

- shared fact source;
- independently instantiated consumers;
- consumer-specific policy;
- consumer-specific decisions.

Phase 14 finding:

- Contextual can explain divergent consumers over shared facts.
- Contextual requires infrastructure that the current engine does not provide.
- Contextual should not be forced to compete as "the artifact representation" unless future evidence shows the artifact itself must contain consumer-specific views.

This warrants a taxonomy correction in `PHASE-14-TAXONOMY-REVISION.md`.

## 7. Revisit the three models

### Unified

Possible corrected interpretation:

```text
Artifact
  └── associated facts
```

Unified remains viable only as an artifact-associated fact envelope. It must not become a trust decision container unless the product boundary changes.

### Modular

Possible corrected interpretation:

```text
Artifact
 ├── Claim records
 ├── Attestation records
 ├── Evidence records
 └── Provenance / lineage records
```

Modular remains viable as separately addressable facts linked by artifact identity. Phase 10 suggests distributed records are acceptable, but does not prove the engine must provide the storage layer.

### Contextual

Corrected interpretation:

```text
Artifact facts
      ↓
External/shared fact source
      ↓
Consumer-specific interpretation
```

Contextual is likely not an artifact representation. It is a consumption architecture over artifact-associated facts.

## 8. Minimum viable Artifact Engine product kernel

### Artifact Engine MUST own

- Artifact identity.
- Entry/content digest model.
- Path safety and entry layout validation.
- Pipeline identity and deterministic pipeline execution.
- Artifact creation provenance required to explain source and pipeline.
- Deterministic materialization boundaries and output digest reporting.
- Capability declarations required for pipeline/materializer execution.
- Execution evidence for engine-performed operations.

### Artifact Engine MAY expose

- Policy-agnostic fact identifiers.
- Canonical serialization for engine-produced facts.
- Hooks or extension points for attaching external facts by artifact identity.
- Lineage facts for engine-performed transformations.
- Verification/evidence values for operations the engine itself performs.

### Artifact Engine MUST NOT own

- Consumer trust decisions.
- Organizational authority.
- Credential lifecycle.
- PKI.
- Registry service behavior.
- General-purpose database behavior.
- Durable multi-party shared state.
- Policy authoring and policy governance.
- Mutation of historical facts to reflect current acceptability.

### External systems own

- Fact custody beyond the engine process.
- Attestation authorities.
- Revocation authorities and revocation distribution.
- Durable evidence stores.
- Consumer policy state.
- Trust relationships.
- Credentials.
- Shared consumer context.
- Synchronization and concurrency semantics for related external facts.
- Historical audit reconstruction across multiple producers/consumers.

## 9. Explicit unresolved questions

1. Should Artifact Engine define a minimal, policy-agnostic `Fact` identity format, or leave fact identity entirely to external systems?
2. Should engine-produced execution evidence be materializable as an artifact-like output?
3. Does transformation lineage require explicit parent artifact identity, or is source/pipeline provenance enough?
4. Should semantic declarations remain embedded optional artifact metadata, or move into external claim facts?
5. What is the product meaning of "provenance persistence" beyond the existing `Provenance` fields in `Artifact` and `Manifest`?
6. If revocation is represented, should it reference attestations, evidence, claims, or any artifact-associated fact?
7. Is there a minimum portable bundle format for artifact plus selected external facts that avoids becoming a registry?
8. Should capability policy evaluation remain only an execution guard, or also produce externally consumable decision evidence?
9. Which future phase should test whether a fact-envelope API is sufficient before representation selection resumes?

## Phase 14 conclusion

Phase 13B exposed missing capabilities, but Phase 14 does not assign all of them to Artifact Engine.

The minimum Artifact Engine remains small: construct, identify, transform, inspect, authorize execution, produce evidence for engine actions, and materialize artifacts deterministically.

Claims, attestations, revocations, evidence custody, shared context, policy, trust, credentials, synchronization, and historical reconstruction are external unless future evidence proves a narrower policy-agnostic fact primitive is intrinsic to the artifact kernel.

Unified and Modular remain possible artifact-associated fact representations. Contextual is better classified as an external consumption architecture over shared facts. Representation selection should remain paused until the unresolved fact-boundary questions are answered.
