# Phase 13B — Production Boundary Integration

Phase 13B tests the Phase 11 relationship contract against the existing engine primitives. It does not select Unified, Modular, or Contextual.

## Engine boundary map

| Phase 11 relationship | Existing engine primitive | Integration point |
| -- | -- | -- |
| Artifact identity is immutable | `Artifact::from_parts`, `Artifact.identity` | Recompute with unchanged canonical identity inputs |
| Claims do not alter identity | `Artifact.semantic_declaration` | `with_semantic_declaration` metadata |
| Provenance metadata does not alter identity | `Provenance.creation_metadata` | Artifact identity canonicalization excludes creation metadata |
| Execution/authorization evidence references an artifact | `ExecutionEvidence`, `AuthorizationDecision` | `build_with_authorization` |
| Consumer policies may differ | `CapabilityPolicy`, `AllowAllPolicy`, `AllowListPolicy` | Same requested capabilities, different policy decisions |
| Transformations produce new artifacts | `PrefixTransform`, `ArtifactTransform` | Real transform machinery over a real `SourceBackedArtifact` |
| Persistence | Deterministic serialization only | No deserialize/store/recover primitive for relationship facts |
| Shared external context | None | Requires infrastructure outside the current engine |
| Revocation | None | Requires a production revocation fact primitive |

## Evidence matrix

| Requirement | Unified | Modular | Contextual |
| -- | -- | -- | -- |
| Real Artifact | PROVEN | PROVEN | PROVEN |
| Real identity semantics | PROVEN | PROVEN | PROVEN |
| Real transformation | PROVEN | PROVEN | PROVEN |
| Historical attestations | UNDERDETERMINED | UNDERDETERMINED | UNDERDETERMINED |
| Independent claims | PROVEN | PROVEN | PROVEN |
| Independent revocation | REQUIRES NEW INFRASTRUCTURE | REQUIRES NEW INFRASTRUCTURE | REQUIRES NEW INFRASTRUCTURE |
| Evidence persistence | REQUIRES NEW INFRASTRUCTURE | REQUIRES NEW INFRASTRUCTURE | REQUIRES NEW INFRASTRUCTURE |
| Consumer divergence | PROVEN | PROVEN | PROVEN |
| Actual shared context | UNDERDETERMINED | UNDERDETERMINED | REQUIRES NEW INFRASTRUCTURE |
| Persistence/recovery | UNTESTABLE WITH CURRENT ENGINE | UNTESTABLE WITH CURRENT ENGINE | UNTESTABLE WITH CURRENT ENGINE |
| No hidden infrastructure | PROVEN | PROVEN | PROVEN |
| Existing engine primitives sufficient | FAILED | FAILED | FAILED |

## Proven

- A real `Artifact` keeps the same identity when semantic claims or provenance creation metadata are introduced.
- Real execution evidence and authorization decisions are independently identifiable and reference the artifact identity.
- Real policy evaluation can diverge for the same requested capabilities without mutating existing facts.
- A real prefix transformation creates a distinct artifact while preserving the source artifact and not silently transferring claims.

## Not proven

- No candidate has production persistence/recovery for claims, attestations, revocations, evidence, and shared context facts.
- Historical attestations can be represented as independent engine values, but the engine has no production attachment or storage boundary for them.
- Transformation lineage is only weakly observable through existing provenance source identity; the engine does not record a parent artifact identity edge.

## Operational requirements discovered

- A production revocation fact primitive is required before revocation can be tested at the production boundary.
- A production persistence/recovery primitive is required before external relationship facts can survive process destruction.
- Model C requires an actual authoritative shared fact source; independently instantiated consumers over a test-local collection are not sufficient evidence.

## Still undecided

Unified, Modular, and Contextual remain undecided. Phase 13B shows that the existing engine supports real artifacts, identity semantics, transforms, authorization evidence, and policy divergence, but not enough production infrastructure to prove any candidate representation viable end-to-end.
