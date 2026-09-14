# Phase 29 — Artifact Workbench

## Verdict

WORKBENCH PROVEN

## What this proves

Phase 29 adds `examples/artifact-workbench`, a single-page human control surface for the existing Artifact Engine lifecycle:

```text
IMPORT SOURCE → CONFIGURE → BUILD ARTIFACT → SELECT OUTPUT → SELECT TARGET → MATERIALIZE / EXECUTE → RECEIVE RESULT
```

The Workbench is an external consumer. It does not add target, deployment, transfer, provider, registry, cache, or runtime concepts to `src/`.

## Boundary ownership

| Boundary | Owner used by the Workbench |
| --- | --- |
| Artifact identity | Existing Artifact Engine artifact produced by `PipelineSpec::build_from_directory` |
| Artifact persistence/recovery | Existing `LocalArtifactStore` |
| ZIP representation | Existing `ZipMaterializer` |
| TAR representation | Existing `TarMaterializer` |
| Local runtime execution | Existing `examples/runtime-consumer` ZIP runtime consumer |
| OCI image | Shown as an external-consumer capability; not faked by the UI |
| Directory / WASM / OCI layout | Shown unavailable until a real materializer exists |
| Remote host / Other targets | Shown unavailable until a real deployment boundary exists |

The UI state is deliberately small: source, pipeline, artifact, output, target, runtime inputs, and result. It displays `artifact.identity`, `representation_identity`, and `execution.identity` values returned by the underlying systems; browser code does not calculate identities or keep an artifact registry/cache.

## Human flow

Run the Workbench:

```sh
cargo run --manifest-path examples/artifact-workbench/Cargo.toml -- 127.0.0.1:8787
```

Then open `http://127.0.0.1:8787` and:

1. Import a source directory.
2. Build the artifact.
3. Select ZIP.
4. Select Local Runtime.
5. Run an executable inside the artifact, such as `bin/app`.
6. Read the receipt showing artifact identity, representation identity, target, execution identity, exit code, stdout, and stderr.

## Browser evidence

The browser acceptance path is captured by Playwright:

```sh
npm install
npx playwright install chromium
npm run test:phase29-browser
```

The test starts the real Workbench server, imports the public GitHub repository URL, imports a direct downloadable ZIP URL, previews local browser-selected files, imports a local host path, builds an artifact through the engine, selects an implemented ZIP output, selects the local runtime target, executes successfully, executes a real failing program, and re-runs the same artifact.

The resulting screenshots are stored in `artifacts/phase29/`:

- `01-empty-workbench.png`
- `02-github-source.png`
- `03-github-imported.png`
- `04-direct-url.png`
- `05-local-source.png`
- `06-source-preview.png`
- `07-artifact-configuration.png`
- `08-artifact-created.png`
- `09-output-selection.png`
- `10-target-selection.png`
- `11-execution.png`
- `12-result.png`
- `13-failure.png`
- `14-rerun.png`

## Negative coverage

`tests/phase29_workbench.rs` proves:

- changing output does not change artifact identity;
- running twice does not change artifact identity;
- different representations retain the same artifact identity;
- each execution gets its own execution identity;
- target selection does not mutate the artifact;
- browser UI code does not calculate artifact identity or maintain an artifact registry/cache;
- unsupported outputs and targets cannot be selected;
- failed execution does not corrupt the artifact;
- a restarted operation can recover the same persisted artifact.
