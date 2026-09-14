# Phase 30 — Expanded Artifact Output Materializers

## Verdict

EXPANDED MATERIALIZATION CAPABILITY PROVEN

## What changed

Phase 30 expands the output vocabulary around the same logical `Artifact` without introducing new artifact subclasses.

- Proven portable representations in the kernel:
  - ZIP
  - TAR
  - TAR + gzip
  - TAR + zstd
  - Directory
  - Raw File
  - Git Tree
  - App Bundle
- Proven capability-gated direct binary representations:
  - WASM Module
  - WASM Component
- Still visible but not proven in the workbench:
  - OCI Image (external consumer)
  - OCI Layout
  - ISO

## Identity rule

Every materializer returns the same logical `artifact_identity` together with a representation-specific identity or digest.

- ZIP/TAR/TAR.gz/TAR.zst/Directory/App Bundle return representation digests
- Git Tree returns a Git tree identity
- Raw File returns an output digest for the single file
- WASM Module/Component return direct binary digests for the materialized bytes

The artifact identity remains unchanged across outputs.

## Capability matrix

| Representation | Production | Deterministic | Independently consumable | UI |
| --- | --- | --- | --- | --- |
| ZIP | ✓ | ✓ | ✓ | ✓ |
| TAR | ✓ | ✓ | ✓ | ✓ |
| TAR.gz | ✓ | ✓ | ✓ | ✓ |
| TAR.zst | ✓ | ✓ | ✓ | ✓ |
| Directory | ✓ | ✓ representation digest | ✓ | ✓ |
| Raw File | ✓ single-file only | ✓ | ✓ | ✓ |
| Git Tree | ✓ | ✓ | ✓ | ✓ |
| App Bundle | ✓ entrypoint-gated | ✓ | ✓ | ✓ |
| WASM Module | ✓ matching binary only | ✓ | ✓ | ✓ |
| WASM Component | ✓ matching binary only | ✓ | ✓ | ✓ |
| OCI Image | external consumer | not proven here | external | ✓ |
| OCI Layout | NOT PROVEN | NOT PROVEN | NOT PROVEN | ✓ disabled |
| ISO | NOT PROVEN | NOT PROVEN | NOT PROVEN | ✓ disabled |

## Browser evidence

Playwright writes Phase 30 screenshots to `artifacts/phase30/` through:

```sh
npm install
npx playwright install chromium
npm run test:phase30-browser
```

The browser flow verifies:

1. a real artifact can be built;
2. the output picker is grouped by category and capability-aware;
3. each supported output keeps the same artifact identity while producing a distinct representation identity;
4. unsupported outputs remain disabled with engine-provided reasons.
