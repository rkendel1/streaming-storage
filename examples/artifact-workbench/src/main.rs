fn main() -> Result<(), Box<dyn std::error::Error>> {
    artifact_workbench::server::run_from_env()
}
