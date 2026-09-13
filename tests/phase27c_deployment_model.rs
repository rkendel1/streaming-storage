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

fn strip_comments_and_strings(source: &str) -> String {
    let mut stripped = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    let mut in_line_comment = false;
    let mut in_block_comment_depth = 0usize;
    let mut in_string = false;
    let mut in_char = false;
    let mut escape = false;

    while let Some(ch) = chars.next() {
        if in_line_comment {
            if ch == '\n' {
                in_line_comment = false;
                stripped.push('\n');
            }
            continue;
        }

        if in_block_comment_depth > 0 {
            if ch == '/' && chars.peek() == Some(&'*') {
                chars.next();
                in_block_comment_depth += 1;
            } else if ch == '*' && chars.peek() == Some(&'/') {
                chars.next();
                in_block_comment_depth -= 1;
            } else if ch == '\n' {
                stripped.push('\n');
            }
            continue;
        }

        if in_string {
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                in_string = false;
            }
            if ch == '\n' {
                stripped.push('\n');
            }
            continue;
        }

        if in_char {
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '\'' {
                in_char = false;
            }
            if ch == '\n' {
                stripped.push('\n');
            }
            continue;
        }

        if ch == '/' && chars.peek() == Some(&'/') {
            chars.next();
            in_line_comment = true;
            continue;
        }

        if ch == '/' && chars.peek() == Some(&'*') {
            chars.next();
            in_block_comment_depth = 1;
            continue;
        }

        if ch == '"' {
            in_string = true;
            continue;
        }

        if ch == '\'' {
            in_char = true;
            continue;
        }

        stripped.push(ch);
    }

    stripped
}

fn contains_identifier(source: &str, identifier: &str) -> bool {
    source
        .match_indices(identifier)
        .any(|(start, _)| is_identifier_match(source, start, identifier.len()))
}

fn is_identifier_match(source: &str, start: usize, len: usize) -> bool {
    let before = source[..start].chars().next_back();
    let after = source[start + len..].chars().next();

    !before.is_some_and(is_identifier_char) && !after.is_some_and(is_identifier_char)
}

fn is_identifier_char(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphanumeric()
}

#[test]
fn kernel_source_contains_no_deployment_boundary_identifiers() {
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
        let code = strip_comments_and_strings(&content);
        for term in forbidden_terms {
            assert!(
                !contains_identifier(&code, term),
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
    let audit = fs::read_to_string(&audit_path)
        .expect("phase 27c audit should exist")
        .to_ascii_lowercase();

    assert!(audit.contains("deployment model underdetermined"));
    assert!(audit.contains("representation transport"));
    assert!(audit.contains("deployment is not execution"));
}
