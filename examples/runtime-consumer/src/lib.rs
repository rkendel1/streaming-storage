use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use tempfile::TempDir;
use zip::ZipArchive;

static NEXT_RUNTIME_EXECUTION_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug)]
pub struct RuntimeExecution {
    pub runtime_execution_id: u64,
    pub artifact_identity: String,
    pub runtime_directory: PathBuf,
    pub executable_path: PathBuf,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RuntimeConsumer;

impl RuntimeConsumer {
    pub fn execute_materialized_zip(
        &self,
        artifact_identity: &str,
        materialized_zip: impl AsRef<Path>,
        executable_relative_path: &str,
        arguments: &[&str],
        environment: &[(&str, &str)],
    ) -> io::Result<RuntimeExecution> {
        let runtime_root = TempDir::new()?;

        // Archive extraction is runtime behavior. It interprets an engine-produced
        // materialization and stages it for process execution.
        extract_zip(materialized_zip.as_ref(), runtime_root.path())?;

        let executable_path = runtime_root.path().join(executable_relative_path);
        ensure_executable_permissions(&executable_path)?;

        let mut command = Command::new(&executable_path);
        command.args(arguments);
        for (key, value) in environment {
            command.env(key, value);
        }
        let output = command.output()?;

        let runtime_directory = runtime_root.path().to_path_buf();
        drop(runtime_root);

        Ok(RuntimeExecution {
            runtime_execution_id: NEXT_RUNTIME_EXECUTION_ID.fetch_add(1, Ordering::Relaxed),
            artifact_identity: artifact_identity.to_string(),
            runtime_directory,
            executable_path,
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

fn extract_zip(zip_path: &Path, destination: &Path) -> io::Result<()> {
    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
        let Some(name) = entry.enclosed_name() else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "zip entry path escapes destination",
            ));
        };

        let output_path = destination.join(name);
        if entry.name().ends_with('/') {
            fs::create_dir_all(&output_path)?;
            continue;
        }

        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut output = File::create(&output_path)?;
        io::copy(&mut entry, &mut output)?;
    }

    Ok(())
}

fn ensure_executable_permissions(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions)?;
    }
    Ok(())
}
