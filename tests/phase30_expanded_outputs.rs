use artifact::{
    AppBundleMaterializer, Artifact, ArtifactEntry, CreationMetadata, DirectoryMaterializer,
    EntryType, GitTreeMaterializer, MemoryContentResolver, Provenance, RawFileMaterializer,
    TarCompression, TarMaterializer, WasmMaterializer, WasmRepresentationKind, ZipMaterializer,
    core::sha256_prefixed,
};
use artifact_workbench::{ArtifactWorkbench, BuildOptions, CapabilityState};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use tempfile::TempDir;

fn build_artifact(entries: BTreeMap<&str, Vec<u8>>) -> (Artifact, MemoryContentResolver) {
    let artifact_entries: Vec<ArtifactEntry> = entries
        .iter()
        .map(|(path, content)| ArtifactEntry {
            path: (*path).to_string(),
            entry_type: EntryType::File,
            size: content.len() as u64,
            content_digest: sha256_prefixed(content),
        })
        .collect();

    let pipeline_identity = sha256_prefixed(b"phase30_pipeline");
    let source_identity = sha256_prefixed(b"phase30_source");
    let artifact = Artifact::from_parts(
        artifact_entries,
        pipeline_identity.clone(),
        Vec::new(),
        Provenance {
            source_identity,
            pipeline_identity,
            creation_metadata: CreationMetadata::default(),
        },
    )
    .expect("logical artifact fixture should build");

    let resolver = MemoryContentResolver::new(
        entries
            .into_iter()
            .map(|(path, content)| (path.to_string(), content))
            .collect(),
    );
    (artifact, resolver)
}

fn multi_file_fixture() -> (Artifact, MemoryContentResolver) {
    build_artifact(BTreeMap::from([
        ("README.txt", b"phase30 logical fixture".to_vec()),
        ("bin/app", b"#!/usr/bin/env sh\necho phase30\n".to_vec()),
        ("src/lib.txt", b"artifact workbench fixture\n".to_vec()),
    ]))
}

fn raw_file_fixture() -> (Artifact, MemoryContentResolver) {
    build_artifact(BTreeMap::from([("output.bin", b"\x00\x01\x02phase30".to_vec())]))
}

#[test]
fn expanded_outputs_preserve_logical_identity_and_representation_separation() {
    let (artifact, resolver) = multi_file_fixture();
    let output_root = TempDir::new().expect("output root should be created");

    let zip = ZipMaterializer
        .materialize_to_path(&artifact, &resolver, output_root.path().join("artifact.zip"))
        .expect("zip should materialize");
    let tar = TarMaterializer
        .materialize_to_path(&artifact, &resolver, output_root.path().join("artifact.tar"))
        .expect("tar should materialize");
    let tar_gzip = TarMaterializer
        .materialize_to_path_with_compression(
            &artifact,
            &resolver,
            output_root.path().join("artifact.tar.gz"),
            TarCompression::Gzip,
        )
        .expect("tar gzip should materialize");
    let tar_zstd = TarMaterializer
        .materialize_to_path_with_compression(
            &artifact,
            &resolver,
            output_root.path().join("artifact.tar.zst"),
            TarCompression::Zstd,
        )
        .expect("tar zstd should materialize");
    let directory = DirectoryMaterializer
        .materialize_to_path(&artifact, &resolver, output_root.path().join("artifact-directory"))
        .expect("directory should materialize");
    let git_tree = GitTreeMaterializer
        .materialize_to_path(&artifact, &resolver, output_root.path().join("artifact-git-tree"))
        .expect("git tree should materialize");
    let app_bundle = AppBundleMaterializer
        .materialize_to_path(&artifact, &resolver, output_root.path().join("artifact-app-bundle"))
        .expect("app bundle should materialize");

    for result in [&zip, &tar, &tar_gzip, &tar_zstd, &directory, &git_tree, &app_bundle] {
        assert_eq!(result.artifact_identity, artifact.identity);
        assert_ne!(result.output_digest, artifact.identity);
        assert!(result.size_bytes > 0);
    }

    let digests: BTreeSet<&str> = [
        zip.output_digest.as_str(),
        tar.output_digest.as_str(),
        tar_gzip.output_digest.as_str(),
        tar_zstd.output_digest.as_str(),
        directory.output_digest.as_str(),
        git_tree.output_digest.as_str(),
        app_bundle.output_digest.as_str(),
    ]
    .into_iter()
    .collect();
    assert_eq!(digests.len(), 7);

    assert!(output_root.path().join("artifact-directory/bin/app").is_file());
    assert!(output_root
        .path()
        .join("artifact-app-bundle/manifest.json")
        .is_file());
}

#[test]
fn single_file_outputs_are_gated_and_materialize_directly() {
    let (single_artifact, single_resolver) = raw_file_fixture();
    let (multi_artifact, multi_resolver) = multi_file_fixture();
    let output_root = TempDir::new().expect("output root should be created");

    let raw = RawFileMaterializer
        .materialize_to_path(
            &single_artifact,
            &single_resolver,
            output_root.path().join("output.bin"),
        )
        .expect("raw file should materialize");
    assert_eq!(raw.artifact_identity, single_artifact.identity);
    assert_eq!(raw.output_digest, single_artifact.entries[0].content_digest);
    assert_eq!(
        fs::read(output_root.path().join("output.bin")).expect("raw bytes should be readable"),
        b"\x00\x01\x02phase30".to_vec()
    );

    assert!(RawFileMaterializer
        .materialize_to_path(
            &multi_artifact,
            &multi_resolver,
            output_root.path().join("not-allowed.bin"),
        )
        .expect_err("multi-file raw output must be rejected")
        .to_string()
        .contains("exactly one file"));
}

#[test]
fn wasm_outputs_require_a_matching_binary_kind() {
    let output_root = TempDir::new().expect("output root should be created");
    let module_bytes = b"\0asm\x01\0\0\0".to_vec();
    let component_bytes = b"\0asm\x0a\0\x01\0".to_vec();

    let (module_artifact, module_resolver) =
        build_artifact(BTreeMap::from([("module.wasm", module_bytes.clone())]));
    let (component_artifact, component_resolver) =
        build_artifact(BTreeMap::from([("component.wasm", component_bytes.clone())]));

    assert_eq!(
        WasmMaterializer
            .detect_kind(&module_artifact, &module_resolver)
            .expect("module should be detected"),
        WasmRepresentationKind::Module
    );
    assert_eq!(
        WasmMaterializer
            .detect_kind(&component_artifact, &component_resolver)
            .expect("component should be detected"),
        WasmRepresentationKind::Component
    );

    let module = WasmMaterializer
        .materialize_to_path(
            &module_artifact,
            &module_resolver,
            output_root.path().join("module.wasm"),
            WasmRepresentationKind::Module,
        )
        .expect("module output should materialize");
    let component = WasmMaterializer
        .materialize_to_path(
            &component_artifact,
            &component_resolver,
            output_root.path().join("component.wasm"),
            WasmRepresentationKind::Component,
        )
        .expect("component output should materialize");

    assert_eq!(module.output_digest, sha256_prefixed(&module_bytes));
    assert_eq!(component.output_digest, sha256_prefixed(&component_bytes));
    assert!(WasmMaterializer
        .materialize_to_path(
            &module_artifact,
            &module_resolver,
            output_root.path().join("wrong-kind.wasm"),
            WasmRepresentationKind::Component,
        )
        .expect_err("module must not materialize as component")
        .to_string()
        .contains("matching"));
}

#[test]
fn workbench_output_picker_is_capability_aware() {
    let source = TempDir::new().expect("fixture source should be created");
    fs::create_dir_all(source.path().join("bin")).expect("bin directory should exist");
    fs::write(source.path().join("README.md"), "# phase30\n").expect("readme should be written");
    fs::write(source.path().join("bin/app"), "#!/usr/bin/env sh\necho phase30\n")
        .expect("app should be written");
    let store = TempDir::new().expect("store should exist");
    let outputs = TempDir::new().expect("outputs should exist");
    let mut workbench =
        ArtifactWorkbench::new(store.path(), outputs.path()).expect("workbench should build");

    workbench
        .import_source(source.path())
        .expect("source should import");
    let artifact = workbench
        .build_artifact(BuildOptions::default())
        .expect("artifact should build");

    let raw_file = artifact
        .output_options
        .iter()
        .find(|output| output.id == "raw-file")
        .expect("raw file output should be visible");
    assert_eq!(raw_file.state, CapabilityState::Unavailable);
    assert!(raw_file.detail.contains("exactly one file"));
    let git_tree = artifact
        .output_options
        .iter()
        .find(|output| output.id == "git-tree")
        .expect("git tree output should be visible");
    assert_eq!(git_tree.state, CapabilityState::Available);
    let app_bundle = artifact
        .output_options
        .iter()
        .find(|output| output.id == "app-bundle")
        .expect("app bundle output should be visible");
    assert_eq!(app_bundle.state, CapabilityState::Available);
    let wasm_component = artifact
        .output_options
        .iter()
        .find(|output| output.id == "wasm-component")
        .expect("wasm component output should be visible");
    assert_eq!(wasm_component.state, CapabilityState::Unavailable);

    let zip = workbench.select_output("zip").expect("zip should materialize");
    let tar = workbench.select_output("tar").expect("tar should materialize");
    let tar_gzip = workbench
        .select_output("tar-gzip")
        .expect("tar gzip should materialize");
    let tar_zstd = workbench
        .select_output("tar-zstd")
        .expect("tar zstd should materialize");
    let directory = workbench
        .select_output("directory")
        .expect("directory should materialize");
    let git = workbench
        .select_output("git-tree")
        .expect("git tree should materialize");
    let bundle = workbench
        .select_output("app-bundle")
        .expect("app bundle should materialize");

    for representation in [&zip, &tar, &tar_gzip, &tar_zstd, &directory, &git, &bundle] {
        assert_eq!(representation.artifact_identity, artifact.identity);
    }

    let representation_ids: BTreeSet<&str> = [
        zip.representation_identity.as_str(),
        tar.representation_identity.as_str(),
        tar_gzip.representation_identity.as_str(),
        tar_zstd.representation_identity.as_str(),
        directory.representation_identity.as_str(),
        git.representation_identity.as_str(),
        bundle.representation_identity.as_str(),
    ]
    .into_iter()
    .collect();
    assert_eq!(representation_ids.len(), 7);
}
