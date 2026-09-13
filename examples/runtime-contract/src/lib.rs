use std::io;
use std::path::Path;

#[path = "../../oci-consumer/src/lib.rs"]
pub mod oci_consumer;
#[path = "../../runtime-consumer/src/lib.rs"]
pub mod runtime_consumer;

use oci_consumer::{OciConsumer, OciMaterialization};
use runtime_consumer::RuntimeConsumer;

#[derive(Clone, Copy, Debug)]
pub struct RuntimeExecutionInput<'a> {
    pub artifact_identity: &'a str,
    pub executable_relative_path: &'a str,
    pub arguments: &'a [&'a str],
    pub environment: &'a [(&'a str, &'a str)],
}

#[derive(Clone, Copy, Debug)]
pub struct ZipRepresentation<'a> {
    pub artifact_identity: &'a str,
    pub representation_identity: &'a str,
    pub path: &'a Path,
}

#[derive(Clone, Copy, Debug)]
pub struct OciRepresentation<'a> {
    pub artifact_identity: &'a str,
    pub representation_identity: &'a str,
    pub image_reference: &'a str,
    pub executable_relative_path: &'a str,
}

impl<'a> OciRepresentation<'a> {
    pub fn from_materialization(
        materialization: &'a OciMaterialization,
        executable_relative_path: &'a str,
    ) -> Self {
        Self {
            artifact_identity: &materialization.artifact_identity,
            representation_identity: &materialization.oci_representation_digest,
            image_reference: &materialization.image_reference,
            executable_relative_path,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeExecutionResult {
    pub runtime_execution_identifier: String,
    pub artifact_identity: String,
    pub representation_identity: String,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

pub trait ExternalRuntimeExecutor<R> {
    fn execute(
        &self,
        representation: &R,
        input: RuntimeExecutionInput<'_>,
    ) -> io::Result<RuntimeExecutionResult>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ZipRuntimeContract;

impl<'a> ExternalRuntimeExecutor<ZipRepresentation<'a>> for ZipRuntimeContract {
    fn execute(
        &self,
        representation: &ZipRepresentation<'a>,
        input: RuntimeExecutionInput<'_>,
    ) -> io::Result<RuntimeExecutionResult> {
        ensure_same_artifact(representation.artifact_identity, input.artifact_identity)?;

        let execution = RuntimeConsumer.execute_materialized_zip(
            input.artifact_identity,
            representation.path,
            input.executable_relative_path,
            input.arguments,
            input.environment,
        )?;

        Ok(RuntimeExecutionResult {
            runtime_execution_identifier: format!("zip-runtime-{}", execution.runtime_execution_id),
            artifact_identity: execution.artifact_identity,
            representation_identity: representation.representation_identity.to_string(),
            exit_code: execution.exit_code,
            stdout: execution.stdout,
            stderr: execution.stderr,
        })
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct OciRuntimeContract;

impl<'a> ExternalRuntimeExecutor<OciRepresentation<'a>> for OciRuntimeContract {
    fn execute(
        &self,
        representation: &OciRepresentation<'a>,
        input: RuntimeExecutionInput<'_>,
    ) -> io::Result<RuntimeExecutionResult> {
        ensure_same_artifact(representation.artifact_identity, input.artifact_identity)?;
        ensure_same_entry_point(
            representation.executable_relative_path,
            input.executable_relative_path,
        )?;

        let materialization = OciMaterialization {
            artifact_identity: representation.artifact_identity.to_string(),
            oci_representation_digest: representation.representation_identity.to_string(),
            image_reference: representation.image_reference.to_string(),
        };
        let execution =
            OciConsumer.execute(&materialization, input.arguments, input.environment)?;

        Ok(RuntimeExecutionResult {
            runtime_execution_identifier: execution.runtime_execution_identifier,
            artifact_identity: execution.artifact_identity,
            representation_identity: execution.oci_representation_digest,
            exit_code: execution.exit_code,
            stdout: execution.stdout,
            stderr: execution.stderr,
        })
    }
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
