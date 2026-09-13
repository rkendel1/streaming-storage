use artifact::{
    core::sha256_prefixed, normalize_relative_path, Artifact, ArtifactError, ContentResolver,
};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use tempfile::TempDir;

static NEXT_RUNTIME_EXECUTION_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_IMAGE_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug)]
pub struct OciMaterialization {
    pub artifact_identity: String,
    pub oci_representation_digest: String,
    pub image_reference: String,
}

#[derive(Clone, Debug)]
pub struct OciRuntimeExecution {
    pub runtime_execution_id: u64,
    pub runtime_execution_identifier: String,
    pub artifact_identity: String,
    pub oci_representation_digest: String,
    pub image_reference: String,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct OciConsumer;

impl OciConsumer {
    pub fn materialize_to_oci_image(
        &self,
        artifact: &Artifact,
        resolver: &dyn ContentResolver,
        executable_relative_path: &str,
    ) -> io::Result<OciMaterialization> {
        ensure_docker_runtime()?;

        let staging = TempDir::new()?;
        stage_artifact_contents(artifact, resolver, staging.path())?;
        let executable_relative_path =
            normalize_relative_path(executable_relative_path).map_err(materialization_error)?;
        write_dockerfile(staging.path(), &executable_relative_path)?;

        let image_reference = format!(
            "artifact-oci-consumer-proof:{}-{}-{}",
            artifact
                .identity
                .strip_prefix("sha256:")
                .unwrap_or("artifact"),
            std::process::id(),
            NEXT_IMAGE_ID.fetch_add(1, Ordering::Relaxed)
        );

        let output = run_docker(&[
            "build",
            "--quiet",
            "--tag",
            &image_reference,
            staging.path().to_str().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "staging path must be utf-8")
            })?,
        ])?;
        if !output.status.success() {
            return Err(io::Error::other(format!(
                "docker build failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        let digest_output =
            run_docker(&["image", "inspect", "--format", "{{.Id}}", &image_reference])?;
        if !digest_output.status.success() {
            return Err(io::Error::other(format!(
                "docker image inspect failed: {}",
                String::from_utf8_lossy(&digest_output.stderr)
            )));
        }

        let oci_representation_digest = String::from_utf8_lossy(&digest_output.stdout)
            .trim()
            .to_string();

        Ok(OciMaterialization {
            artifact_identity: artifact.identity.clone(),
            oci_representation_digest,
            image_reference,
        })
    }

    pub fn execute(
        &self,
        materialization: &OciMaterialization,
        arguments: &[&str],
        environment: &[(&str, &str)],
    ) -> io::Result<OciRuntimeExecution> {
        ensure_docker_runtime()?;

        let runtime_execution_id = NEXT_RUNTIME_EXECUTION_ID.fetch_add(1, Ordering::Relaxed);
        let runtime_execution_identifier = format!("artifact-oci-runtime-{runtime_execution_id}");

        let mut command = Command::new("docker");
        command
            .arg("run")
            .arg("--rm")
            .arg("--name")
            .arg(&runtime_execution_identifier);

        for (key, value) in environment {
            command.arg("--env").arg(format!("{key}={value}"));
        }

        command.arg(&materialization.image_reference);
        command.args(arguments);

        let output = command.output()?;

        Ok(OciRuntimeExecution {
            runtime_execution_id,
            runtime_execution_identifier,
            artifact_identity: materialization.artifact_identity.clone(),
            oci_representation_digest: materialization.oci_representation_digest.clone(),
            image_reference: materialization.image_reference.clone(),
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }

    pub fn remove_image(&self, image_reference: &str) -> io::Result<()> {
        let output = run_docker(&["image", "rm", "--force", image_reference])?;
        if output.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("No such image") {
            return Ok(());
        }

        Err(io::Error::other(format!(
            "docker image rm failed: {}",
            stderr
        )))
    }
}

fn stage_artifact_contents(
    artifact: &Artifact,
    resolver: &dyn ContentResolver,
    root: &Path,
) -> io::Result<()> {
    for entry in &artifact.entries {
        let relative_path = normalize_relative_path(&entry.path).map_err(materialization_error)?;
        let path = root.join(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut content = Vec::new();
        let mut reader = resolver
            .resolve(&entry.path)
            .map_err(materialization_error)?;
        reader
            .read_to_end(&mut content)
            .map_err(|source| io::Error::other(format!("{}: {source}", entry.path)))?;

        if content.len() as u64 != entry.size {
            return Err(io::Error::other(format!(
                "{}: expected {} bytes, found {}",
                entry.path,
                entry.size,
                content.len()
            )));
        }

        let digest = sha256_prefixed(&content);
        if digest != entry.content_digest {
            return Err(io::Error::other(format!(
                "{}: content digest mismatch while staging",
                entry.path
            )));
        }

        let mut file = File::create(&path)?;
        file.write_all(&content)?;
        file.sync_all()?;
    }

    Ok(())
}

fn write_dockerfile(root: &Path, executable_relative_path: &str) -> io::Result<()> {
    let dockerfile = root.join("Dockerfile");
    let entrypoint = serde_json::to_string(&vec![format!("/artifact/{executable_relative_path}")])
        .map_err(|source| io::Error::other(source.to_string()))?;
    let mut file = File::create(dockerfile)?;
    writeln!(file, "FROM alpine:3.22")?;
    writeln!(file, "COPY . /artifact")?;
    writeln!(file, "WORKDIR /artifact")?;
    writeln!(file, "RUN chmod 755 /artifact/{executable_relative_path}")?;
    writeln!(file, "ENTRYPOINT {entrypoint}")?;
    file.sync_all()?;
    Ok(())
}

fn ensure_docker_runtime() -> io::Result<()> {
    let output = run_docker(&["info"])?;
    if output.status.success() {
        return Ok(());
    }

    Err(io::Error::other(format!(
        "OCI RUNTIME ENVIRONMENT BLOCKED: {}",
        String::from_utf8_lossy(&output.stderr)
    )))
}

fn run_docker(arguments: &[&str]) -> io::Result<Output> {
    Command::new("docker").args(arguments).output()
}

fn materialization_error(error: ArtifactError) -> io::Error {
    io::Error::other(error.to_string())
}
