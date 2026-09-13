use std::fs;
use std::path::{Path, PathBuf};

fn rust_files_under(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();

    for entry in fs::read_dir(root).expect("directory should be readable") {
        let entry = entry.expect("directory entry should be readable");
        let path = entry.path();
        if path.is_dir() {
            files.extend(rust_files_under(&path));
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }

    files
}

#[test]
fn kernel_source_contains_no_deployment_boundary_types() {
    let src_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let forbidden_terms = [
        "Deployment",
        "Provider",
        "RemoteExecution",
        "DeploymentId",
        "ProviderId",
    ];

    for path in rust_files_under(&src_root) {
        let content = fs::read_to_string(&path).expect("source file should be readable");
        for term in forbidden_terms {
            assert!(
                !content.contains(term),
                "expected {} to remain free of `{term}`, but found it in {}",
                src_root.display(),
                path.display()
            );
        }
    }
}

#[test]
fn phase27c_audit_concludes_deployment_model_is_underdetermined() {
    let audit_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("PHASE-27C-DEPLOYMENT-MODEL-AUDIT.md");
    let audit = fs::read_to_string(&audit_path).expect("phase 27c audit should exist");

    assert!(audit.contains("DEPLOYMENT MODEL UNDERDETERMINED"));
    assert!(audit.contains("Representation Transport"));
    assert!(audit.contains("Deployment\n    ≠\nExecution"));
}
