# streaming-storage

`artifact` is a proof of an artifact system where the **logical artifact** is independent of its physical representation.

## Architecture overview

```text
                 Pipeline
                    │
        ┌───────────┴───────────┐
        ↓                       ↓
     Stages                 Capabilities
        │
        ↓
   Logical Artifact
        │
        ├── ContentResolver
        │       │
        │   ┌───┼────┐
        │   ↓   ↓    ↓
        │  FS  Mem  Future
        │
   ┌────┼────────┐
   ↓    ↓        ↓
  ZIP  TAR      Future
```

## Phase 2 status (current)

**What Phase 2 proves:**

One logical artifact can have multiple independent materializations with different physical digests.

```text
Artifact
  identity: sha256:AAAA
  entries: 3 files
  size: 195 bytes
       │
       ├─→ ZIP Materializer
       │       output_digest: sha256:BBBB
       │       size: 511 bytes
       │
       └─→ TAR Materializer
               output_digest: sha256:CCCC
               size: 4096 bytes
```

AAAA ≠ BBBB ≠ CCCC: logical identity is distinct from physical representations.

**Phase 2 scope:**

Implemented:
- ContentResolver as first-class abstraction (filesystem and in-memory)
- MemoryContentResolver proving source independence
- TAR materializer with deterministic output
- MaterializationResult separating artifact_identity from output_digest
- 21 integration tests covering both Phase 1 and Phase 2

Explicit non-goals: AppPort, `.app`, WASM execution, remote execution, OCI, encryption, signatures, databases, cloud storage, AI-generated pipelines, platform-specific packaging, delta encoding, CAS, and signing.

## Repository structure

```text
example/                  Example hostile-friendly input tree
src/
  bin/artifact.rs         Thin CLI
  core/                   Artifact, manifest, identity, provenance, capabilities
  materializers/          ZIP materializer
  pipeline/               Directory source and select/manifest/validate stages
tests/                    Integration tests for determinism and safety
```

## Foundation choice

This implementation uses Rust because it provides strong typing, deterministic data modeling, good streaming-oriented I/O primitives, and a clean library-first architecture with a thin CLI.

## Core model

### Source

Phase 1 implements `DirectorySource`.

It recursively reads a directory, sorts directory entries deterministically, normalizes relative paths, hashes regular files, and rejects hostile filesystem objects.

### Stage

Phase 1 implements three explicit stages:

- `select`
- `manifest`
- `validate`

Stages are data in `PipelineSpec`, not hidden function calls.

### Pipeline

Canonical pipeline representation:

```json
{"schema":"pipeline.v1","source":"directory","stages":[{"kind":"select","exclude_exact":[".env"],"exclude_prefixes":["node_modules"]},{"kind":"manifest"},{"kind":"validate"}],"materializer":"zip","capabilities":[{"name":"artifact.validate","version":"1"},{"name":"filesystem.read","version":"1"},{"name":"manifest.generate","version":"1"},{"name":"package.zip","version":"1"}]}
```

The pipeline digest is `sha256(canonical_pipeline_json)`.

### Logical artifact

The logical artifact contains:

- artifact identity
- entries
- deterministic manifest
- pipeline identity
- explicit capabilities
- provenance

The logical artifact does **not** depend on ZIP types or ZIP APIs.

### Materializer

`ZipMaterializer` consumes `Artifact` plus an `EntryContentResolver`.

That keeps the artifact inspectable without requiring ZIP materialization and keeps the door open for additional materializers.

## Entry model

Each artifact entry stores:

- `path`
- `entry_type`
- `size`
- `content_digest`

Phase 1 rules:

- regular files become artifact entries
- directories are traversed but are not standalone logical entries; directory structure is implicit in normalized file paths
- symlinks are rejected
- special files are rejected
- unreadable files fail the build
- duplicate normalized paths are rejected
- file/directory path conflicts such as `app` and `app/index.html` are rejected
- paths longer than 4096 bytes are rejected

Rejected paths include `../foo`, `foo/../bar`, `/absolute/path`, and Windows-style absolute paths.

## Phase 3 recommendation

Phase 2 has proven the architectural thesis:
- Logical artifacts are independent of their physical representation ✓
- Multiple materializers can coexist without changing the Artifact model ✓
- Content resolution is decoupled from the artifact model ✓
- Artifact identity is separate from materialization identity ✓

**Phase 3 should introduce declarative capability pipelines.**

The original FFmpeg analogy becomes powerful here:
```text
artifact
  ├─→ select (filter)
  ├─→ manifest (capture)
  ├─→ validate (probe)
  └─→ materialize (encode)
```

Phase 3 next steps (recommended but not implemented):
- transform stage (structural changes: rename, recompose)
- redact stage (selective content removal)
- generate stage (derived content synthesis)
- authorize stage (capability attestation)
- attest stage (integrity signing)
- index stage (searchable metadata)
- cache stage (content-addressed storage)

Each stage:
- takes Artifact in, produces Artifact out
- transforms logical structure without coupling to materializers
- supports streaming for large artifacts
- maintains deterministic identity

Non-goals (still out of scope):
- OCI image production
- Remote execution or cloud services
- Signature/encryption algorithms
- Database persistence
- AI pipeline generation

## Manifest specification

Manifest schema fields:

- `manifest_version`
- `artifact_identity`
- `entries`
- `pipeline_identity`
- `capabilities`
- `provenance`

The manifest is deterministically serialized as compact JSON with:

1. fixed field order from the Rust data model
2. entries sorted by normalized path
3. capabilities sorted lexicographically by `(name, version, parameters)`
4. parameter maps stored in key order
5. no timestamps included in canonical identity material

## Identity and canonicalization specification

### Path canonicalization

1. Convert `\` to `/`
2. Reject absolute paths
3. Remove empty and `.` segments
4. Reject any `..` segment
5. Join remaining segments with `/`
6. Reject results longer than 4096 bytes

### Pipeline identity

`pipeline_identity = sha256(canonical_pipeline_json)`

Canonical pipeline JSON uses:

- schema tag `pipeline.v1`
- source kind only (`directory`)
- declared stages in declared order
- canonicalized selection parameters
- materializer kind only (`zip`)
- sorted declared capabilities

### Source identity

`source_identity = sha256(canonical_source_json)`

Canonical source JSON uses the normalized selected entries only:

- path
- entry type
- size
- content digest

It excludes traversal order, absolute paths, timestamps, host metadata, and machine information.

### Artifact identity

`artifact_identity = sha256(canonical_artifact_json)`

Canonical artifact JSON includes:

- schema tag `artifact.v1`
- sorted logical entries
- pipeline identity
- sorted capabilities
- provenance source identity only

It intentionally excludes:

- absolute source paths
- filesystem traversal order
- timestamps
- process IDs
- hostnames
- random values
- ZIP metadata or ZIP byte layout

## Capabilities

Capabilities are explicit declarations, not hidden behavior.

Phase 1 capabilities:

- `filesystem.read`
- `manifest.generate`
- `artifact.validate`
- `package.zip`

Each capability is versioned and supports deterministic parameter maps for future expansion.

## Security decisions

Arbitrary source directories are treated as hostile input.

Phase 1 decisions:

- reject symlinks rather than attempting to resolve them
- reject unsupported/special filesystem objects
- reject traversal and absolute paths after normalization
- reject duplicate normalized paths
- reject file/directory entry conflicts
- fail fast on unreadable input
- do not preserve arbitrary host metadata
- use deterministic stored ZIP entries with fixed timestamps and permissions

This is an explicit security baseline, not a fake authorization system.

## Identity model (Phase 2)

### Artifact identity

The logical artifact identity includes:
- normalized entries (path, type, size, content_digest)
- pipeline identity
- capabilities
- provenance source identity

It **explicitly excludes**:
- creation_metadata (timestamps, hostnames, PIDs, etc.)
- materializer format
- physical representation

### Materialization identity

Each physical representation has a separate identity:
- output_digest: sha256(physical bytes)

This is fundamentally different from artifact_identity.

### Content resolver

The ContentResolver abstraction decouples entry content from the artifact model:

```rust
pub trait ContentResolver: Send + Sync {
    fn resolve(&self, path: &str) -> Result<Box<dyn Read>, ArtifactError>;
}
```

Two implementations:
- **SourceBackedArtifact**: resolves from original filesystem paths
- **MemoryContentResolver**: resolves from in-memory BTreeMap

This proves:
- sources can be deleted after artifact creation (memory resolver persists)
- artifact and source are separate concerns
- new resolvers can be added without changing Artifact

## Materializers (Phase 2)

### ZIP materializer

Deterministic ZIP with:
- stored (no compression)
- fixed timestamps (epoch)
- fixed permissions (0o644)
- sorted entries by path

### TAR materializer

Deterministic TAR with:
- GNU tar format
- fixed timestamps (default)
- sorted entries by path

Both materializers:
- validate entry layout
- verify content digest during materialization
- reject content drift
- produce identical output when run twice

## CLI (Phase 2)

Commands:

```bash
cargo run --bin artifact -- inspect ./example
cargo run --bin artifact -- manifest ./example
cargo run --bin artifact -- build ./example --output /tmp/example.zip --format zip
cargo run --bin artifact -- build ./example --output /tmp/example.tar --format tar
```

Example output:

```text
Pipeline:
  directory
  → select
  → manifest
  → validate
  → zip
Artifact:
  id: sha256:8d36505d1a3fedb0d94f26b4d83160cf9954a9f73a89f2abaeb4effe77dab0a1
  entries: 3
  size: 195
Output:
  format: zip
  digest: sha256:1b76267333b429145186e2b282db8d71fa6a88f73098521865cc41cea05c2573
  size: 511
  path: /tmp/example.zip
```

Same artifact with tar:

```text
Output:
  format: tar
  digest: sha256:1b72aa7cb25a73f73523a85f8afea9e9924377958ab914c5fe397a0f6234a554
  size: 4096
  path: /tmp/example.tar
```

Note: artifact id is identical, output digests differ.

## Example fixture

`example/` demonstrates safe selection:

- included: `README.md`, `app/index.html`, `app/app.js`
- excluded: `.env`, `node_modules/`

## Tests

21 integration tests covering:

**Phase 1 (15 tests):**
- stage execution order
- file selection and exclusion
- path safety
- pipeline identity determinism
- artifact identity determinism
- manifest determinism
- traversal-order independence
- deterministic ZIP output
- logical artifact inspection without ZIP materialization
- conflicting entry rejection
- content drift detection
- symlink rejection

**Phase 2 (6 new tests):**
- format independence: same artifact produces different digests in ZIP vs TAR
- artifact identity shared across materializers
- memory content resolver enables source independence
- TAR materialization determinism
- TAR result reporting
- provenance metadata does not alter artifact identity

All tests use fixture directories and determinism verification to prove architectural properties.
