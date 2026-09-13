# Artifact Engine

Artifact Engine is a logical artifact substrate with deterministic identity, transformation lineage, and physical materialization that is independent of archive format.

It is implemented as the Rust crate `artifact`, with a library-first core and a thin CLI.

## What it does

Artifact Engine turns source content into logical artifacts:

```text
Source
  ↓
Pipeline
  ↓
Logical Artifact
  ↓
Artifact Identity
  ↓
Transformations
  ↓
New Logical Artifacts
  ↓
Materializations
```

The core product boundary is:

- create logical artifacts from source content;
- compute deterministic artifact and pipeline identities;
- preserve artifact creation provenance;
- inspect artifacts and pipelines without requiring archive output;
- transform logical artifacts into new logical artifacts;
- materialize logical artifacts to physical formats such as ZIP and TAR;
- produce execution evidence for engine-performed operations.

## Core model

### Artifact

An artifact is an immutable logical description of selected content. It contains normalized entries, entry content digests, a deterministic manifest, pipeline identity, capabilities, creation provenance, and artifact identity.

Artifact identity is independent of ZIP/TAR bytes, filesystem traversal order, host metadata, and volatile creation metadata.

### Pipeline

A pipeline is declarative. It describes the source kind, ordered stages, materializer, and required capabilities. Its identity is the digest of the canonical pipeline specification.

Current stages include selection, transformation, redaction, generation, compilation, manifest, and validation.

### Transformation

Transformations operate on logical artifacts, not archive bytes. A structural transform creates a new logical artifact with a new identity.

Implemented transforms include:

- `PrefixTransform`
- `RedactTransform`
- `GenerateTransform`

### Materialization

Materialization converts a logical artifact into physical bytes:

```text
Logical Artifact
      ↓
Materializer
      ↓
Physical representation
```

ZIP and TAR materializers produce deterministic outputs. Materialized outputs have output digests that are separate from artifact identity.

The same logical artifact can be materialized to multiple formats.

## Semantic and facts boundary

Artifact Engine distinguishes artifact identity from facts and decisions about an artifact.

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

Claims, attestations, evidence, and revocations may be artifact-associated facts, but they are not intrinsic artifact identity. Consumer decisions such as accept, reject, deploy, or ignore are contextual and belong outside Artifact Engine.

Contextual interpretation is therefore an external consumption architecture, not an artifact representation.

## What Artifact Engine does not do

Artifact Engine is not:

- a registry;
- a database;
- a PKI;
- a key manager;
- a trust service;
- a consumer policy engine;
- a revocation authority;
- a distributed synchronization system;
- a remote storage service;
- an application runtime;
- a deployment platform.

External systems may store artifacts, manage claims, issue attestations, distribute revocations, enforce policy, and decide whether to trust or deploy an artifact. Artifact Engine supplies deterministic artifact primitives and policy-agnostic evidence for those systems to consume.

## Repository structure

```text
example/                  Example input tree
src/
  bin/artifact.rs         Thin CLI
  core/                   Artifact, manifest, identity, provenance, capabilities
  materializers/          ZIP and TAR materializers
  pipeline/               Directory source, stages, resolvers
  transforms/             Logical artifact transforms
tests/                    Integration and investigation tests
```

## Basic usage

Run the CLI with Cargo:

```bash
cargo run --bin artifact -- version
cargo run --bin artifact -- inspect ./example
cargo run --bin artifact -- build ./example --recipe zip
cargo run --bin artifact -- build ./example --recipe tar
```

Run the test suite:

```bash
cargo test
```

## Documentation

- `PRODUCT-DEFINITION.md` defines the current product boundary.
- `PRODUCT-EXAMPLES.md` gives market-facing workflow examples.
- `PHASE-15-KERNEL.md` records the artifact kernel and semantic boundary analysis.
- `PHASE-16-PRODUCT-VALIDATION.md` tests the kernel against concrete external workflows.
- `PHASE-14-BOUNDARY-ANALYSIS.md` records the prior boundary investigation.
- `PHASE-14-TAXONOMY-REVISION.md` records why Contextual is an external consumption architecture.
