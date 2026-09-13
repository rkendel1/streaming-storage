# Phase 19 — Production Artifact Composition & Relationship Integrity

## Proven

Phase 19 proves with production types that multiple independent artifacts can participate in a composition without redefining the participating artifacts.

- Independent artifacts with different logical content have different identities, and composition leaves those identities unchanged (`tests/phase19_composition.rs:88-105`).
- A multi-artifact composition result is a normal production `Artifact`, deterministic for equivalent ordered inputs, and it does not mutate either input artifact (`tests/phase19_composition.rs:107-130`, `tests/phase19_composition.rs:132-161`).
- Persisting and recovering a composed result preserves the result identity and artifact fields, but does not create stored records for the inputs (`tests/phase19_composition.rs:163-190`).
- Persisting only the input artifacts does not implicitly create or discover a composition result (`tests/phase19_composition.rs:192-209`).
- ZIP and TAR materialization consume the composed logical artifact without changing A, B, or C identity (`tests/phase19_composition.rs:211-231`).
- Composition does not create transformation lineage, semantic declarations, execution evidence, trust semantics, or authorization semantics (`tests/phase19_composition.rs:233-273`).
- Empty composition and path collisions fail closed rather than using implicit discovery or overwrite behavior (`tests/phase19_composition.rs:275-294`).

## Existing

The production implementation already had an explicit composition boundary:

- `CompositionInput` owns an explicit `Vec<Artifact>` and `compose` returns an `Artifact` (`src/composition.rs:5-7`, `src/composition.rs:31-82`).
- Single-artifact composition is a pass-through clone and does not create a new identity (`src/composition.rs:38-40`).
- Multi-artifact composition merges entries by path and rejects collisions under the current policy (`src/composition.rs:42-60`).
- The production kernel already distinguishes artifacts from transformations. Transformations use `ArtifactTransform::apply` and single-input `TransformationRecord` lineage (`src/transforms/mod.rs:9-18`, `src/core/mod.rs:137-142`).
- Materializers already consume an artifact plus content resolver and report physical output metadata rather than creating logical artifacts (`src/materializers/zip.rs:23-51`, `src/materializers/tar.rs:12-40`).

## Implemented

Phase 19 made one minimal production correction:

- Composition results now use `Artifact::from_parts` instead of directly constructing `Artifact` and `Manifest` fields (`src/composition.rs:76-82`). This keeps composed results on the same identity, manifest, validation, and durability path as all other production artifacts.
- Composition operation identity is deterministic over the ordered input artifact identities and collision policy, and is stored as the result `pipeline_identity` (`src/composition.rs:72-95`).

No graph database, registry, relationship store, catalog, event bus, queue, background worker, or generalized orchestration engine was added.

## Required relationships

The only relationship Phase 19 proves the production kernel needs for current behavior is:

```text
CompositionInput([A.identity, B.identity], options) -> C
```

That relationship is represented inside C as creation provenance:

```text
C.provenance.source_identity = composed[A.identity,B.identity]
C.pipeline_identity = identity(composition operation)
```

This is enough to explain C as a composed artifact while preserving:

```text
A.identity = stable
B.identity = stable
C.identity = stable
```

Input order is observable today because `CompositionInput` is a `Vec<Artifact>` and the composition source/procedure records input identities in that order (`src/composition.rs:5-7`, `src/composition.rs:46-78`). Phase 19 tests equivalent ordered inputs as equivalent and reversed inputs as a different operation (`tests/phase19_composition.rs:132-161`).

## External relationships

These remain outside the Artifact Kernel:

- global artifact catalog membership;
- discovery of related artifacts;
- mutable workflow history;
- trust decisions between artifacts;
- authorization between artifacts;
- consumer semantic declarations;
- orchestration job/execution state.

Consumers may build those relationships externally, but Phase 19 does not require the kernel to persist them.

## Composition versus transformation lineage

The existing `TransformationRecord` is a single-input transform lineage record (`src/core/mod.rs:137-142`). Built-in transforms append it when one input artifact is transformed into one output artifact (`src/transforms/mod.rs:75-89`).

Phase 19 composition is not silently treated as transformation lineage. A composed result has creation provenance identifying the participating artifact identities, but an empty `lineage` unless a real transform produced it (`tests/phase19_composition.rs:122-129`, `tests/phase19_composition.rs:252-253`).

If a future production operation needs a true multi-input transformation, the exact missing boundary is a multi-input transform lineage representation. That is not needed to prove current composition behavior, and Phase 19 does not generalize `TransformationRecord` into graph edges.

## Not proven

- Unordered composition equivalence is not proven; current production input is ordered.
- Relationship records with their own `RelationshipId` are not proven necessary.
- Composition discovery, query, or catalog APIs are not proven necessary.
- Authorization, trust, deployment, and runtime orchestration semantics remain out of scope.
