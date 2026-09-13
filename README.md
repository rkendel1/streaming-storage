# streaming-storage

`artifact` is a Phase 1 proof of an artifact pipeline where the **logical artifact** is independent of its physical representation.

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
   ┌────┼─────┐
   ↓    ↓     ↓
  ZIP  TAR   OCI
```

Only ZIP works today. The architecture keeps ZIP at the edge so new materializers can be added without changing the artifact model.

## Phase 1 scope

Implemented vertical slice:

```text
directory
   ↓
select
   ↓
manifest
   ↓
validate
   ↓
logical artifact
   ↓
ZIP materializer
   ↓
deterministic ZIP
```

Explicit non-goals for this repository state: AppPort, `.app`, WASM execution, remote execution, OCI, TAR, encryption, signatures, databases, cloud storage, AI-generated pipelines, and platform-specific packaging.

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

## CLI

Commands:

```bash
cargo run --bin artifact -- inspect ./example
cargo run --bin artifact -- manifest ./example
cargo run --bin artifact -- build ./example --output /tmp/example.zip
```

Example build output:

```text
Pipeline:
  directory
  → select
  → manifest
  → validate
  → zip
Artifact:
  id: sha256:...
  entries: 3
  size: 121
Output:
  format: zip
  digest: sha256:...
```

## Example fixture

`example/` demonstrates safe selection:

- included: `README.md`, `app/index.html`, `app/app.js`
- excluded: `.env`, `node_modules/`

## Tests

The test suite covers:

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
