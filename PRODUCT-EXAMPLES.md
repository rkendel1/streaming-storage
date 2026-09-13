# Product Examples

These examples describe why Artifact Engine exists. They are intentionally product-facing and avoid adding infrastructure that the engine does not own.

## 1. Same artifact, multiple formats

A build system produces one logical artifact. One consumer needs ZIP. Another needs TAR.

Artifact Engine keeps the artifact identity stable while producing different physical outputs:

```text
Logical Artifact sha256:A
  ├─ ZIP output sha256:Z
  └─ TAR output sha256:T
```

The ZIP and TAR bytes differ. The artifact is the same.

## 2. Transform without losing the original

A release artifact contains internal files. A public handoff requires a redacted version.

Artifact Engine models this as:

```text
Artifact A
  ↓ redact/filter
Artifact B
```

B is not "A with some ZIP entries removed." B is a new logical artifact with its own identity. A remains intact, and claims about A do not silently transfer to B.

## 3. Deployment material without deployment ownership

A deployment system needs a deterministic package from a source repository.

Artifact Engine can:

1. select deployment-relevant files;
2. build a logical artifact;
3. identify it;
4. materialize it for the runtime.

The deployment system still owns rollout, environment policy, credentials, and runtime decisions.

## 4. AI-generated handoff with stable identity

An agent generates or assembles output for another system.

Artifact Engine can package that output as a logical artifact with deterministic identity and creation provenance. Review systems can attach claims or evidence to that identity without changing the artifact.

The accepting system decides whether to trust the result.

## 5. Supply-chain facts without becoming a trust service

A verifier may produce an attestation about an artifact. A scanner may produce evidence. A consumer may reject the artifact because its policy distrusts the verifier.

Artifact Engine provides the stable artifact identity that these facts reference.

It does not become:

- the verifier;
- the revocation authority;
- the policy engine;
- the registry;
- the trust provider.

## 6. Future materializers without changing the artifact model

Today the engine can materialize ZIP and TAR. A future materializer could emit another physical format.

That future format should not redefine artifact identity. It should consume the same logical artifact and produce its own output digest.

## One-line product value

Artifact Engine gives teams a stable logical artifact before packaging, deployment, registry storage, or trust decisions enter the workflow.
