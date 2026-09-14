use artifact_workbench::{ArtifactWorkbench, BuildOptions};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use tempfile::TempDir;

fn fixture_source() -> TempDir {
    let source = TempDir::new().expect("fixture source should be created");
    fs::create_dir_all(source.path().join("bin")).expect("bin directory should be created");
    fs::create_dir_all(source.path().join("src")).expect("src directory should be created");
    fs::write(source.path().join("README.md"), "# Workbench fixture\n")
        .expect("README should be written");
    fs::write(
        source.path().join("src/lib.txt"),
        "artifact workbench fixture\n",
    )
    .expect("source file should be written");
    fs::write(
        source.path().join("bin/app"),
        "#!/usr/bin/env sh\necho Hello from Artifact Engine\n",
    )
    .expect("runtime script should be written");
    let mut permissions = fs::metadata(source.path().join("bin/app"))
        .expect("runtime script metadata should be readable")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(source.path().join("bin/app"), permissions)
        .expect("runtime script should be executable");
    source
}

fn workbench() -> (ArtifactWorkbench, TempDir, TempDir) {
    let store = TempDir::new().expect("store should be created");
    let outputs = TempDir::new().expect("outputs should be created");
    let workbench =
        ArtifactWorkbench::new(store.path(), outputs.path()).expect("workbench should be created");
    (workbench, store, outputs)
}

#[test]
fn workbench_exercises_zip_to_local_runtime_receipt() {
    let source = fixture_source();
    let (mut workbench, _store, _outputs) = workbench();

    let imported = workbench
        .import_source(source.path())
        .expect("source import should succeed");
    assert_eq!(imported.source_kind, "Local");
    assert!(imported.entries.iter().any(|entry| entry == "bin/"));
    assert!(imported.entries.iter().any(|entry| entry == "bin/app"));

    let artifact = workbench
        .build_artifact(BuildOptions::default())
        .expect("artifact should be built by the engine");
    assert!(artifact.identity.starts_with("sha256:"));
    assert_eq!(artifact.lineage, "None");
    assert!(artifact.entries >= 3);

    let zip = workbench
        .select_output("zip")
        .expect("zip output should be materialized");
    assert_eq!(zip.artifact_identity, artifact.identity);
    assert_eq!(zip.output, "ZIP");
    assert!(zip.representation_identity.starts_with("sha256:"));
    assert_ne!(zip.representation_identity, artifact.identity);

    let after_target = workbench
        .select_target("local-runtime")
        .expect("local runtime target should be selectable");
    assert_eq!(
        after_target.artifact_identity.as_deref(),
        Some(artifact.identity.as_str())
    );
    assert_eq!(after_target.output.as_deref(), Some("ZIP"));
    assert_eq!(after_target.target.as_deref(), Some("Local Runtime"));

    let first_receipt = workbench
        .execute("bin/app")
        .expect("local runtime execution should succeed");
    assert_eq!(first_receipt.artifact.identity, artifact.identity);
    assert_eq!(
        first_receipt.representation.representation_identity,
        zip.representation_identity
    );
    assert_eq!(first_receipt.target, "Local Runtime");
    let first_execution = first_receipt
        .execution
        .as_ref()
        .expect("receipt should include execution");
    assert_eq!(first_execution.status, "Completed");
    assert_eq!(first_execution.exit_code, Some(0));
    assert!(
        first_execution
            .stdout
            .contains("Hello from Artifact Engine")
    );

    let second_receipt = workbench
        .execute("bin/app")
        .expect("running again should succeed");
    let second_execution = second_receipt
        .execution
        .as_ref()
        .expect("second receipt should include execution");
    assert_eq!(second_receipt.artifact.identity, artifact.identity);
    assert_ne!(first_execution.identity, second_execution.identity);
}

#[test]
fn source_import_distinguishes_local_and_unsupported_remote_inputs() {
    let source = fixture_source();
    let local_file = source.path().join("README.md");
    let (mut workbench, _store, _outputs) = workbench();

    let imported_file = workbench
        .import_source(&local_file)
        .expect("local files should be staged as source input");
    assert_eq!(imported_file.source_kind, "Local");
    assert_eq!(imported_file.display_name, "README.md");
    assert!(
        imported_file
            .entries
            .iter()
            .any(|entry| entry == "README.md")
    );
    assert!(imported_file.total_size_bytes > 0);

    let artifact = workbench
        .build_artifact(BuildOptions::default())
        .expect("staged local file source should build through the engine");
    assert!(artifact.identity.starts_with("sha256:"));

    assert!(
        workbench
            .import_source("http://example.com/artifact.zip")
            .expect_err("remote URLs must require HTTPS")
            .to_string()
            .contains("HTTPS")
    );
    assert!(
        workbench
            .import_source("https://example.com/")
            .expect_err("arbitrary webpages are not artifacts")
            .to_string()
            .contains("direct downloadable .zip")
    );
    assert!(
        workbench
            .import_source("https://github.com/rkendel1/streaming-storage/tree/main")
            .expect_err("GitHub importer should accept repository URLs, not arbitrary pages")
            .to_string()
            .contains("repository URL")
    );
}

#[test]
fn output_and_target_boundaries_do_not_mutate_artifact_identity() {
    let source = fixture_source();
    let (mut workbench, _store, _outputs) = workbench();
    workbench
        .import_source(source.path())
        .expect("source import should succeed");
    let artifact = workbench
        .build_artifact(BuildOptions::default())
        .expect("artifact should be built");

    let zip = workbench
        .select_output("zip")
        .expect("zip should materialize");
    let tar = workbench
        .select_output("tar")
        .expect("tar should materialize");
    assert_eq!(zip.artifact_identity, artifact.identity);
    assert_eq!(tar.artifact_identity, artifact.identity);
    assert_ne!(zip.representation_identity, tar.representation_identity);

    let summary = workbench
        .select_target("download")
        .expect("download target should be selectable");
    assert_eq!(
        summary.artifact_identity.as_deref(),
        Some(artifact.identity.as_str())
    );
    assert_eq!(
        summary.representation_identity.as_deref(),
        Some(tar.representation_identity.as_str())
    );
}

#[test]
fn unsupported_boundaries_are_visible_but_not_selectable() {
    let capabilities = ArtifactWorkbench::capabilities();
    let directory = capabilities
        .outputs
        .iter()
        .find(|output| output.id == "directory")
        .expect("directory output should be shown");
    assert_eq!(directory.state, artifact_workbench::CapabilityState::Available);
    assert_eq!(directory.category, "Portable");

    let source = fixture_source();
    let (mut workbench, _store, _outputs) = workbench();
    workbench
        .import_source(source.path())
        .expect("source import should succeed");
    let artifact = workbench
        .build_artifact(BuildOptions::default())
        .expect("artifact should be built");
    let raw_file = artifact
        .output_options
        .iter()
        .find(|output| output.id == "raw-file")
        .expect("raw file output should be present");
    assert_eq!(raw_file.state, artifact_workbench::CapabilityState::Unavailable);
    assert!(raw_file.detail.contains("exactly one file"));
    let app_bundle = artifact
        .output_options
        .iter()
        .find(|output| output.id == "app-bundle")
        .expect("app bundle output should be present");
    assert_eq!(app_bundle.state, artifact_workbench::CapabilityState::Available);

    assert!(workbench.select_output("directory").is_ok());
    assert!(workbench.select_output("raw-file").is_err());
    assert!(workbench.select_output("wasm-component").is_err());
    assert!(workbench.select_output("oci").is_err());
    assert!(workbench.select_target("docker").is_err());
    assert!(workbench.select_target("remote-host").is_err());
    assert!(
        workbench
            .build_artifact(BuildOptions {
                semantic_declaration: Some("application".to_string())
            })
            .is_err()
    );
}

#[test]
fn failed_execution_does_not_corrupt_or_replace_artifact() {
    let source = fixture_source();
    let (mut workbench, _store, _outputs) = workbench();
    workbench
        .import_source(source.path())
        .expect("source import should succeed");
    let artifact = workbench
        .build_artifact(BuildOptions::default())
        .expect("artifact should be built");
    let representation = workbench
        .select_output("zip")
        .expect("zip output should be materialized");
    workbench
        .select_target("local-runtime")
        .expect("local runtime should be selected");

    assert!(workbench.execute("missing/app").is_err());

    let receipt = workbench
        .execute("bin/app")
        .expect("valid execution after failure should still work");
    assert_eq!(receipt.artifact.identity, artifact.identity);
    assert_eq!(
        receipt.representation.representation_identity,
        representation.representation_identity
    );
}

#[test]
fn persisted_artifact_can_be_recovered_after_restarting_operation() {
    let source = fixture_source();
    let (mut workbench, _store, _outputs) = workbench();
    workbench
        .import_source(source.path())
        .expect("source import should succeed");
    let artifact = workbench
        .build_artifact(BuildOptions::default())
        .expect("artifact should be built");

    workbench.reset_operation();
    let recovered = workbench
        .recover_artifact(&artifact.identity)
        .expect("persisted artifact should recover by engine identity");
    assert_eq!(recovered.identity, artifact.identity);
    assert_eq!(recovered.entries, artifact.entries);

    let zip = workbench
        .select_output("zip")
        .expect("recovered artifact should materialize");
    assert_eq!(zip.artifact_identity, artifact.identity);
}

#[test]
fn browser_ui_does_not_calculate_identity_or_keep_registry() {
    let app = include_str!("../examples/artifact-workbench/src/ui/app.js");
    let index = include_str!("../examples/artifact-workbench/src/ui/index.html");
    assert!(!app.contains("crypto.subtle"));
    assert!(!app.contains("createHash"));
    assert!(!app.contains("artifactRegistry"));
    assert!(!app.contains("artifactCache"));
    assert!(app.contains("option.state !== 'available'"));
    assert!(index.contains("https://github.com/org/repo"));
    assert!(index.contains("https://example.com/artifact.zip"));
    assert!(index.contains("Choose files"));
    assert!(index.contains("Choose folder"));
}
