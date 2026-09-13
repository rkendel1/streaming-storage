# Product Definition

## One sentence

Artifact Engine is a logical artifact substrate that creates, identifies, transforms, inspects, and materializes artifacts with deterministic identity independent of physical representation.

## Problem it solves

Software artifacts are often treated as archive files, deployment outputs, or registry entries. Artifact Engine separates the logical artifact from those physical forms. It gives an artifact a deterministic identity based on normalized content, pipeline identity, capabilities, and creation provenance, then allows that logical artifact to be transformed and materialized to ZIP, TAR, or future formats without confusing artifact identity with archive bytes, trust decisions, or registry state.

## Core primitives

- **Artifact**: immutable logical content entries, content digests, manifest, capabilities, pipeline identity, and creation provenance.
- **Artifact Identity**: deterministic digest of the logical artifact identity inputs.
- **Pipeline**: declarative source, ordered stages, materializer selection, and capabilities.
- **Pipeline Identity**: deterministic digest of the canonical pipeline specification.
- **Transformation**: logical operation that creates a new logical artifact with a new identity.
- **Provenance**: engine-owned creation context for source and pipeline.
- **Materialization**: physical output bytes and output digest for a logical artifact.
- **Execution Evidence**: engine-produced evidence for engine-performed pipeline execution and authorization.

## Lifecycle

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

Associated facts have a separate lifecycle:

```text
CLAIM
ATTEST
EVIDENCE
REVOKE
```

Consumer interpretation is external:

```text
CONSUME
  ↓
apply consumer policy
  ↓
ACCEPT / REJECT / IGNORE
```

## Identity model

An artifact remains the same artifact when only external or associated facts change.

Artifact identity includes:

- normalized entries;
- entry type, size, and content digest;
- pipeline identity;
- canonical capabilities;
- provenance source identity.

Artifact identity excludes:

- claims;
- attestations;
- evidence;
- revocations;
- policy decisions;
- trust relationships;
- credentials;
- volatile creation metadata;
- materialized ZIP/TAR bytes.

A structural transformation changes the logical artifact and produces a new identity. A new claim, attestation, evidence record, or revocation does not silently create a new artifact identity.

## Semantic model

Artifact Engine distinguishes artifact state from artifact-associated facts and consumer decisions.

- **Intrinsic artifact state**: content entries, content digests, pipeline identity, capabilities, creation provenance, artifact identity.
- **Artifact-associated facts**: claims, attestations, evidence, revocations, and lineage facts when explicitly represented.
- **Consumer decisions**: policy-specific interpretation such as accepting, rejecting, deploying, or ignoring an artifact.

Artifact-associated facts must remain outside artifact identity unless future evidence proves a specific fact changes the artifact itself.

## External boundary

External systems own:

- durable fact custody;
- registries;
- attestation authorities;
- revocation authorities;
- trust relationships;
- credentials and key management;
- PKI;
- consumer policy;
- organizational authority;
- distributed synchronization;
- remote storage;
- deployment and runtime decisions.

Artifact Engine may produce or carry policy-agnostic facts, but it does not decide whether a consumer should trust or use an artifact.

## Materialization model

```text
Logical Artifact
      ↓
Materializer
      ↓
Physical representation
```

ZIP and TAR are physical materializations. They have output digests that are separate from artifact identity. The same logical artifact can be materialized to different formats while keeping the same artifact identity.

Semantic facts may later travel alongside, inside, or separately from materializations, but Phase 15 does not choose a file format or storage architecture for them.

## Explicit non-goals

Artifact Engine deliberately does not become:

- a database;
- a registry;
- a PKI;
- a key manager;
- a trust provider;
- a consumer policy engine;
- a revocation authority;
- a distributed synchronization system;
- a remote storage service;
- an application runtime;
- a deployment platform.

It can carry or produce facts that those systems use. It does not own those systems' decisions.

## Example lifecycle

1. A directory source is selected.
2. A pipeline declares selection, manifest, validation, capabilities, and a materializer.
3. Artifact Engine builds a logical artifact from normalized entries and content digests.
4. The artifact receives a deterministic artifact identity.
5. The artifact can be inspected without producing ZIP or TAR bytes.
6. A prefix transformation creates a new logical artifact with a different identity.
7. The transformed artifact can be materialized as ZIP or TAR.
8. ZIP and TAR outputs have different output digests, but both represent the same transformed logical artifact.
9. A claim or attestation about the artifact can exist as an associated fact without changing artifact identity.
10. A consumer applies its own policy to those facts outside Artifact Engine.
