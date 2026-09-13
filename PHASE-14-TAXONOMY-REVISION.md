# Phase 14 — Taxonomy Revision

Phase 12 treated Unified, Modular, and Contextual as three competing artifact representation models. Phase 13B and Phase 14 boundary analysis show that this taxonomy is partly wrong.

## Original taxonomy

```text
Unified
  One record containing artifact-associated relationship facts

Modular
  Separate records linked by artifact identity

Contextual
  Per-consumer context containing facts and decisions
```

Phase 12 showed all three could encode the abstract Phase 11 relationship graph. Phase 13B then showed that Contextual was not actually tested against a production shared-context boundary.

## Corrected taxonomy

```text
Artifact fact representations
  ├─ Unified fact envelope
  └─ Modular fact records

External consumption architectures
  └─ Contextual shared-fact interpretation
       ├─ shared fact source
       ├─ consumer-specific policy
       └─ consumer-specific decision
```

## Why Contextual moves categories

Contextual requires:

- a shared authoritative fact source;
- independently instantiated consumers;
- consumer-specific policies;
- divergent decisions over the same facts.

Those are properties of a consumption architecture, not properties of an artifact representation.

The artifact does not need to know that Consumer X accepts it and Consumer Y rejects it in order to remain an artifact. It only needs stable identity, content, provenance, and engine-produced facts. Consumer divergence belongs outside the artifact kernel.

## Revised model meanings

### Unified fact envelope

Unified means:

```text
Artifact identity
  └── one associated envelope of policy-agnostic facts
```

It may be useful if a product needs portable grouping of facts. It should not imply that all facts must be stored atomically or that consumer decisions belong inside the envelope.

### Modular fact records

Modular means:

```text
Artifact identity
  ├── claim facts
  ├── attestation facts
  ├── evidence facts
  ├── revocation facts
  └── lineage/provenance facts
```

It may be useful if facts need independent lifecycles, issuers, durability, or addressing. It should not imply that Artifact Engine must become the database for those records.

### Contextual interpretation

Contextual means:

```text
Shared facts
  ├── Consumer X policy → decision
  └── Consumer Y policy → decision
```

It is a consumer-side architecture. It may consume Unified or Modular artifact facts, but it is not itself an artifact-state representation.

## Consequences

1. Do not force Contextual to remain a peer of Unified and Modular during artifact representation selection.
2. Future representation work should first decide whether Artifact Engine needs a policy-agnostic fact primitive at all.
3. If a fact primitive is needed, compare Unified and Modular as artifact-fact representation options.
4. Evaluate Contextual separately as a consumption architecture that may sit above either representation.
5. Do not treat Contextual's need for shared infrastructure as a failure of Artifact Engine unless Artifact Engine explicitly takes responsibility for shared fact custody.

## Revised selection question

The next representation question should not be:

> Which of Unified, Modular, or Contextual should Artifact Engine choose?

It should be:

> Does Artifact Engine need to own artifact-associated facts beyond current artifact/provenance/evidence primitives?

If yes:

> Should those artifact-associated facts be exposed as a unified envelope, modular records, or both?

Separately:

> What external consumption architecture interprets those facts for specific consumers?
