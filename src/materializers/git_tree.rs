use crate::core::{Artifact, ArtifactError, MaterializationResult, sha256_prefixed, validate_entry_layout};
use crate::pipeline::ContentResolver;
use sha1::{Digest, Sha1};
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

#[derive(Clone, Copy, Debug, Default)]
pub struct GitTreeMaterializer;

impl GitTreeMaterializer {
    pub fn materialize_to_path<R: ContentResolver>(
        &self,
        artifact: &Artifact,
        resolver: &R,
        output: impl AsRef<Path>,
    ) -> Result<MaterializationResult, ArtifactError> {
        validate_entry_layout(&artifact.entries)?;
        let output = output.as_ref();
        if output.exists() {
            fs::remove_dir_all(output).map_err(|source| ArtifactError::io(output, source))?;
        }
        fs::create_dir_all(output).map_err(|source| ArtifactError::io(output, source))?;

        let mut tree = TreeNode::default();
        for entry in &artifact.entries {
            let path = output.join(&entry.path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|source| ArtifactError::io(parent, source))?;
            }
            let bytes = read_entry_bytes(resolver, &entry.path, entry.size, &entry.content_digest)?;
            fs::write(&path, &bytes).map_err(|source| ArtifactError::io(&path, source))?;
            let mode = file_mode(&entry.path, &bytes);
            #[cfg(unix)]
            if mode == GitFileMode::Executable {
                let mut permissions = fs::metadata(&path)
                    .map_err(|source| ArtifactError::io(&path, source))?
                    .permissions();
                permissions.set_mode(0o755);
                fs::set_permissions(&path, permissions)
                    .map_err(|source| ArtifactError::io(&path, source))?;
            }
            tree.insert_file(&entry.path, git_blob_oid(&bytes)?, mode);
        }

        Ok(MaterializationResult {
            artifact_identity: artifact.identity.clone(),
            materializer_format: "git-tree".to_string(),
            output_digest: tree.oid()?,
            size_bytes: artifact.total_size(),
        })
    }
}

#[derive(Default)]
struct TreeNode {
    children: BTreeMap<String, GitNode>,
}

enum GitNode {
    Blob {
        oid: String,
        mode: GitFileMode,
    },
    Tree(TreeNode),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GitFileMode {
    Regular,
    Executable,
}

impl GitFileMode {
    fn tree_mode(self) -> &'static str {
        match self {
            Self::Regular => "100644",
            Self::Executable => "100755",
        }
    }
}

impl Default for GitNode {
    fn default() -> Self {
        Self::Tree(TreeNode::default())
    }
}

impl TreeNode {
    fn insert_file(&mut self, path: &str, oid: String, mode: GitFileMode) {
        let mut parts = path.split('/').peekable();
        self.insert_parts(&mut parts, oid, mode);
    }

    fn insert_parts<'a, I>(
        &mut self,
        parts: &mut std::iter::Peekable<I>,
        oid: String,
        mode: GitFileMode,
    )
    where
        I: Iterator<Item = &'a str>,
    {
        let Some(part) = parts.next() else {
            return;
        };
        if parts.peek().is_none() {
            self.children
                .insert(part.to_string(), GitNode::Blob { oid, mode });
            return;
        }
        let child = self
            .children
            .entry(part.to_string())
            .or_insert_with(GitNode::default);
        let GitNode::Tree(tree) = child else {
            return;
        };
        tree.insert_parts(parts, oid, mode);
    }

    fn oid(&self) -> Result<String, ArtifactError> {
        let mut body = Vec::new();
        for (name, node) in &self.children {
            let (mode, oid) = match node {
                GitNode::Blob { oid, mode } => (mode.tree_mode(), oid.clone()),
                GitNode::Tree(tree) => ("040000", tree.oid()?),
            };
            body.extend_from_slice(mode.as_bytes());
            body.push(b' ');
            body.extend_from_slice(name.as_bytes());
            body.push(0);
            body.extend_from_slice(&hex_to_bytes(&oid)?);
        }
        let mut data = format!("tree {}\0", body.len()).into_bytes();
        data.extend_from_slice(&body);
        Ok(hex_sha1(&data))
    }
}

fn git_blob_oid(bytes: &[u8]) -> Result<String, ArtifactError> {
    let mut data = format!("blob {}\0", bytes.len()).into_bytes();
    data.extend_from_slice(bytes);
    Ok(hex_sha1(&data))
}

fn hex_sha1(bytes: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(bytes);
    hex_encode(&hasher.finalize())
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn hex_to_bytes(value: &str) -> Result<Vec<u8>, ArtifactError> {
    if value.len() % 2 != 0 {
        return Err(ArtifactError::Materialization(
            "invalid git object identifier length".to_string(),
        ));
    }
    (0..value.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&value[index..index + 2], 16).map_err(|_| {
                ArtifactError::Materialization("invalid git object identifier bytes".to_string())
            })
        })
        .collect()
}

fn read_entry_bytes<R: ContentResolver>(
    resolver: &R,
    path: &str,
    expected_size: u64,
    expected_digest: &str,
) -> Result<Vec<u8>, ArtifactError> {
    let mut reader = resolver.resolve(path)?;
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .map_err(|source| ArtifactError::Materialization(format!("{path}: {source}")))?;
    if bytes.len() as u64 != expected_size {
        return Err(ArtifactError::Materialization(format!(
            "{path}: expected {expected_size} bytes, wrote {}",
            bytes.len()
        )));
    }
    if sha256_prefixed(&bytes) != expected_digest {
        return Err(ArtifactError::Materialization(format!(
            "{path}: content digest drifted during materialization"
        )));
    }
    Ok(bytes)
}

fn file_mode(path: &str, bytes: &[u8]) -> GitFileMode {
    if path.starts_with("bin/") || bytes.starts_with(b"#!") {
        GitFileMode::Executable
    } else {
        GitFileMode::Regular
    }
}
