use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use tempfile::TempDir;

#[path = "../../runtime-contract/src/lib.rs"]
pub mod runtime_contract;

use runtime_contract::{OciRepresentation, RuntimeExecutionInput, RuntimeExecutionResult};

static NEXT_DEPLOYMENT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BoundaryProofStatus {
    Proven,
    NotProven(String),
}

impl BoundaryProofStatus {
    pub fn is_proven(&self) -> bool {
        matches!(self, Self::Proven)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SshRemoteRuntime {
    pub target: String,
}

impl SshRemoteRuntime {
    pub fn from_env() -> Option<Self> {
        std::env::var("PHASE27_SSH_TARGET")
            .ok()
            .filter(|target| !target.trim().is_empty())
            .map(|target| Self { target })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeploymentTransportEvidence {
    pub transported: String,
    pub local_representation_identity: String,
    pub remote_representation_identity: String,
    pub registry_required: bool,
    pub credentials_required: bool,
    pub remote_rebuilt: bool,
    pub digest_survived_transport: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeploymentLifecycleEvidence {
    pub create_observed: bool,
    pub execute_observed: bool,
    pub observe_observed: bool,
    pub delete_observed: bool,
    pub deployment_persistent_until_delete: bool,
    pub execution_implicitly_creates_deployment_state: bool,
    pub multiple_executions_reuse_same_deployment: BoundaryProofStatus,
    pub failed_deployments_leave_state_behind: BoundaryProofStatus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoteDeploymentResult {
    pub provider_identity: String,
    pub artifact_identity: String,
    pub representation_identity: String,
    pub deployment_identity: Option<String>,
    pub remote_execution_identifier: String,
    pub deployment_identity_is_distinct: BoundaryProofStatus,
    pub execution_identity_is_distinct: BoundaryProofStatus,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub transport: DeploymentTransportEvidence,
    pub lifecycle: DeploymentLifecycleEvidence,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Phase27NotProven {
    pub criterion: &'static str,
    pub reason: String,
}

#[derive(Clone, Debug)]
pub struct SshDockerDeploymentBoundary {
    remote: SshRemoteRuntime,
}

impl SshDockerDeploymentBoundary {
    pub fn new(remote: SshRemoteRuntime) -> Self {
        Self { remote }
    }

    pub fn execute_oci_representation(
        &self,
        representation: &OciRepresentation<'_>,
        input: RuntimeExecutionInput<'_>,
    ) -> io::Result<RemoteDeploymentResult> {
        ensure_same_artifact(representation.artifact_identity, input.artifact_identity)?;
        ensure_same_entry_point(
            representation.executable_relative_path,
            input.executable_relative_path,
        )?;

        let sequence = NEXT_DEPLOYMENT_ID.fetch_add(1, Ordering::Relaxed);
        let deployment_name = format!("phase27-deployment-{}-{sequence}", std::process::id());
        let remote_tar = format!("/tmp/{deployment_name}.tar");
        let image_archive = ImageArchive::create(representation.image_reference)?;

        let remote_representation_identity = self.transport_image(
            image_archive.path(),
            &remote_tar,
            representation.image_reference,
        )?;

        let deployment_identity = self.create_deployment(
            &deployment_name,
            representation.image_reference,
            input.arguments,
            input.environment,
        )?;

        let execution_identifier = deployment_identity.clone();
        let exit_code = self.execute_deployment(&deployment_name)?;
        let logs = self.observe_deployment(&deployment_name)?;
        self.delete_deployment(&deployment_name)?;

        Ok(RemoteDeploymentResult {
            provider_identity: format!("ssh://{}", self.remote.target),
            artifact_identity: representation.artifact_identity.to_string(),
            representation_identity: representation.representation_identity.to_string(),
            deployment_identity: Some(deployment_identity.clone()),
            remote_execution_identifier: execution_identifier,
            deployment_identity_is_distinct: if deployment_identity != representation.artifact_identity
                && deployment_identity != representation.representation_identity
            {
                BoundaryProofStatus::Proven
            } else {
                BoundaryProofStatus::NotProven(
                    "remote container id was not distinct from artifact or representation identity"
                        .to_string(),
                )
            },
            execution_identity_is_distinct: BoundaryProofStatus::NotProven(
                "ssh docker create/start uses the container id as both deployment handle and execution handle"
                    .to_string(),
            ),
            exit_code,
            stdout: logs.stdout,
            stderr: logs.stderr,
            transport: DeploymentTransportEvidence {
                transported: format!("docker image archive for {}", representation.image_reference),
                local_representation_identity: representation.representation_identity.to_string(),
                remote_representation_identity: remote_representation_identity.clone(),
                registry_required: false,
                credentials_required: true,
                remote_rebuilt: false,
                digest_survived_transport: remote_representation_identity
                    == representation.representation_identity,
            },
            lifecycle: DeploymentLifecycleEvidence {
                create_observed: true,
                execute_observed: true,
                observe_observed: true,
                delete_observed: true,
                deployment_persistent_until_delete: true,
                execution_implicitly_creates_deployment_state: false,
                multiple_executions_reuse_same_deployment: BoundaryProofStatus::NotProven(
                    "the experiment creates one remote container per execution to keep teardown explicit"
                        .to_string(),
                ),
                failed_deployments_leave_state_behind: BoundaryProofStatus::NotProven(
                    "no provider failure state was observed beyond the explicit container lifecycle"
                        .to_string(),
                ),
            },
        })
    }

    fn transport_image(
        &self,
        local_archive: &Path,
        remote_tar: &str,
        image_reference: &str,
    ) -> io::Result<String> {
        run_command(
            Command::new("scp")
                .arg(local_archive)
                .arg(format!("{}:{remote_tar}", self.remote.target)),
        )?;
        let load_result = run_command(&mut remote_command(
            &self.remote.target,
            &["docker", "load", "-i", remote_tar],
        ))?;
        let inspect_result = run_command(&mut remote_command(
            &self.remote.target,
            &[
                "docker",
                "image",
                "inspect",
                "--format",
                "{{.Id}}",
                image_reference,
            ],
        ))?;
        let _ = remote_command(&self.remote.target, &["rm", "-f", remote_tar]).output();

        if !load_result.status.success() {
            return Err(command_error("remote docker load", &load_result));
        }
        if !inspect_result.status.success() {
            return Err(command_error(
                "remote docker image inspect",
                &inspect_result,
            ));
        }

        Ok(String::from_utf8_lossy(&inspect_result.stdout)
            .trim()
            .to_string())
    }

    fn create_deployment(
        &self,
        deployment_name: &str,
        image_reference: &str,
        arguments: &[&str],
        environment: &[(&str, &str)],
    ) -> io::Result<String> {
        let mut args = vec!["docker", "create", "--name", deployment_name];
        let mut env_args = Vec::new();
        for (key, value) in environment {
            env_args.push("--env".to_string());
            env_args.push(format!("{key}={value}"));
        }
        let string_args: Vec<&str> = env_args.iter().map(String::as_str).collect();
        args.extend(string_args);
        args.push(image_reference);
        args.extend(arguments.iter().copied());

        let output = run_command(&mut remote_command(&self.remote.target, &args))?;
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
        }

        Err(command_error("remote docker create", &output))
    }

    fn execute_deployment(&self, deployment_name: &str) -> io::Result<Option<i32>> {
        let start = run_command(&mut remote_command(
            &self.remote.target,
            &["docker", "start", deployment_name],
        ))?;
        if !start.status.success() {
            return Err(command_error("remote docker start", &start));
        }

        let wait = run_command(&mut remote_command(
            &self.remote.target,
            &["docker", "wait", deployment_name],
        ))?;
        if !wait.status.success() {
            return Err(command_error("remote docker wait", &wait));
        }

        String::from_utf8_lossy(&wait.stdout)
            .trim()
            .parse::<i32>()
            .map(Some)
            .map_err(|error| {
                io::Error::other(format!(
                    "remote docker wait returned invalid exit code: {error}"
                ))
            })
    }

    fn observe_deployment(&self, deployment_name: &str) -> io::Result<RuntimeExecutionResult> {
        let output = run_command(&mut remote_command(
            &self.remote.target,
            &["docker", "logs", deployment_name],
        ))?;

        Ok(RuntimeExecutionResult {
            runtime_execution_identifier: deployment_name.to_string(),
            artifact_identity: String::new(),
            representation_identity: String::new(),
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }

    fn delete_deployment(&self, deployment_name: &str) -> io::Result<()> {
        let output = run_command(&mut remote_command(
            &self.remote.target,
            &["docker", "rm", "--force", deployment_name],
        ))?;
        if output.status.success() {
            return Ok(());
        }

        Err(command_error("remote docker rm", &output))
    }
}

pub fn missing_remote_evidence(criterion: &'static str) -> Phase27NotProven {
    Phase27NotProven {
        criterion,
        reason: "PHASE27_SSH_TARGET was not configured, so no real remote runtime was available"
            .to_string(),
    }
}

struct ImageArchive {
    _root: TempDir,
    path: PathBuf,
}

impl ImageArchive {
    fn create(image_reference: &str) -> io::Result<Self> {
        let root = TempDir::new()?;
        let path = root.path().join("image.tar");
        let output = run_command(
            Command::new("docker")
                .arg("save")
                .arg("--output")
                .arg(&path)
                .arg(image_reference),
        )?;
        if !output.status.success() {
            return Err(command_error("docker save", &output));
        }

        Ok(Self { _root: root, path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

fn remote_command(target: &str, arguments: &[&str]) -> Command {
    let mut command = Command::new("ssh");
    command.arg(target);
    for argument in arguments {
        command.arg(argument);
    }
    command
}

fn run_command(command: &mut Command) -> io::Result<Output> {
    command.output()
}

fn command_error(command: &str, output: &Output) -> io::Error {
    io::Error::other(format!(
        "{command} failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    ))
}

fn ensure_same_artifact(representation_identity: &str, input_identity: &str) -> io::Result<()> {
    if representation_identity == input_identity {
        return Ok(());
    }

    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "representation artifact identity does not match execution input",
    ))
}

fn ensure_same_entry_point(
    representation_entry_point: &str,
    input_entry_point: &str,
) -> io::Result<()> {
    if representation_entry_point == input_entry_point {
        return Ok(());
    }

    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "representation entry point does not match execution input",
    ))
}
