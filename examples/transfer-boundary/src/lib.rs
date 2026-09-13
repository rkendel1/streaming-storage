use sha2::{Digest, Sha256};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExportedRepresentation {
    pub artifact_identity: String,
    pub representation_identity: String,
    pub representation_size: u64,
    pub representation_format: String,
    pub export_path: PathBuf,
    pub transfer_digest: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImportedRepresentation {
    pub artifact_identity: String,
    pub representation_identity: String,
    pub destination_image_reference: String,
    pub transfer_path: PathBuf,
    pub transfer_digest: String,
}

pub fn export_oci_representation(
    artifact_identity: &str,
    representation_identity: &str,
    image_reference: &str,
    export_path: &Path,
) -> io::Result<ExportedRepresentation> {
    if let Some(parent) = export_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let export_path_utf8 = export_path
        .to_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "export path must be utf-8"))?;

    let output = run_docker(&["save", "--output", export_path_utf8, image_reference])?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "docker save failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    let representation_size = fs::metadata(export_path)?.len();
    let transfer_digest = sha256_file(export_path)?;

    Ok(ExportedRepresentation {
        artifact_identity: artifact_identity.to_string(),
        representation_identity: representation_identity.to_string(),
        representation_size,
        representation_format: "docker-archive".to_string(),
        export_path: export_path.to_path_buf(),
        transfer_digest,
    })
}

pub fn transfer_representation(source: &Path, destination: &Path) -> io::Result<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::copy(source, destination)?;
    Ok(())
}

pub fn import_oci_representation(
    artifact_identity: &str,
    expected_representation_identity: &str,
    expected_transfer_digest: &str,
    transfer_path: &Path,
    destination_image_reference: &str,
) -> io::Result<ImportedRepresentation> {
    let received_digest = sha256_file(transfer_path)?;
    if received_digest != expected_transfer_digest {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "transfer digest mismatch",
        ));
    }

    let transfer_path_utf8 = transfer_path.to_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "transfer path must be utf-8 for docker load",
        )
    })?;

    let load_output = run_docker(&["load", "--input", transfer_path_utf8])?;
    if !load_output.status.success() {
        return Err(io::Error::other(format!(
            "docker load failed: {}",
            String::from_utf8_lossy(&load_output.stderr)
        )));
    }

    let loaded_reference = loaded_image_reference(&String::from_utf8_lossy(&load_output.stdout))
        .unwrap_or_else(|| expected_representation_identity.to_string());
    let tag_output = run_docker(&["tag", &loaded_reference, destination_image_reference])?;
    if !tag_output.status.success() {
        return Err(io::Error::other(format!(
            "docker tag failed: {}",
            String::from_utf8_lossy(&tag_output.stderr)
        )));
    }

    let imported_representation_identity = inspect_image_identity(destination_image_reference)?;
    if imported_representation_identity != expected_representation_identity {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "representation identity mismatch after transfer",
        ));
    }

    Ok(ImportedRepresentation {
        artifact_identity: artifact_identity.to_string(),
        representation_identity: imported_representation_identity,
        destination_image_reference: destination_image_reference.to_string(),
        transfer_path: transfer_path.to_path_buf(),
        transfer_digest: received_digest,
    })
}

pub fn inspect_image_identity(image_reference: &str) -> io::Result<String> {
    let inspect_output = run_docker(&["image", "inspect", "--format", "{{.Id}}", image_reference])?;
    if !inspect_output.status.success() {
        return Err(io::Error::other(format!(
            "docker image inspect failed: {}",
            String::from_utf8_lossy(&inspect_output.stderr)
        )));
    }

    Ok(String::from_utf8_lossy(&inspect_output.stdout)
        .trim()
        .to_string())
}

pub fn remove_image(image_reference: &str) -> io::Result<()> {
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

fn sha256_file(path: &Path) -> io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    let mut encoded = String::from("sha256:");
    for byte in hasher.finalize() {
        use std::fmt::Write as _;
        let _ = write!(encoded, "{byte:02x}");
    }
    Ok(encoded)
}

fn loaded_image_reference(load_output: &str) -> Option<String> {
    load_output.lines().find_map(|line| {
        line.strip_prefix("Loaded image: ")
            .or_else(|| line.strip_prefix("Loaded image ID: "))
            .map(|reference| reference.trim().to_string())
            .filter(|reference| !reference.is_empty())
    })
}

fn run_docker(arguments: &[&str]) -> io::Result<Output> {
    Command::new("docker").args(arguments).output()
}
