use artifact::{PipelineSpec, ZipMaterializer, default_directory_zip_pipeline};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "artifact",
    about = "Build deterministic logical artifacts and materialize them as ZIP files"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Inspect {
        source: PathBuf,
    },
    Manifest {
        source: PathBuf,
    },
    Build {
        source: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let pipeline = default_directory_zip_pipeline();

    match cli.command {
        Command::Inspect { source } => inspect(&pipeline, source)?,
        Command::Manifest { source } => manifest(&pipeline, source)?,
        Command::Build { source, output } => build(&pipeline, source, output)?,
    }

    Ok(())
}

fn inspect(pipeline: &PipelineSpec, source: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let built = pipeline.build_from_directory(&source)?;
    let artifact = built.artifact();

    print_pipeline(pipeline);
    println!("Artifact:");
    println!("  id: {}", artifact.identity);
    println!("  entries: {}", artifact.entries.len());
    println!("  size: {}", artifact.total_size());
    println!("  pipeline_id: {}", artifact.pipeline_identity);
    println!("  source_id: {}", artifact.provenance.source_identity);
    println!("  capabilities:");
    for capability in &artifact.capabilities {
        println!("    - {}@{}", capability.name, capability.version);
    }
    println!("Entries:");
    for entry in &artifact.entries {
        println!(
            "  - {} ({:?}, {} bytes, {})",
            entry.path, entry.entry_type, entry.size, entry.content_digest
        );
    }

    Ok(())
}

fn manifest(pipeline: &PipelineSpec, source: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let built = pipeline.build_from_directory(&source)?;
    println!("{}", built.artifact().manifest.to_canonical_json()?);
    Ok(())
}

fn build(
    pipeline: &PipelineSpec,
    source: PathBuf,
    output: PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    let built = pipeline.build_from_directory(&source)?;
    let artifact = built.artifact();
    let zip = ZipMaterializer.materialize_to_path(artifact, &built, &output)?;

    print_pipeline(pipeline);
    println!("Artifact:");
    println!("  id: {}", artifact.identity);
    println!("  entries: {}", artifact.entries.len());
    println!("  size: {}", artifact.total_size());
    println!("Output:");
    println!("  format: zip");
    println!("  digest: {}", zip.output_digest);
    println!("  size: {}", zip.size_bytes);
    println!("  path: {}", output.display());

    Ok(())
}

fn print_pipeline(pipeline: &PipelineSpec) {
    println!("Pipeline:");
    println!("  {}", pipeline.source.label());
    for stage in &pipeline.stages {
        println!("  → {}", stage.label());
    }
    println!("  → {}", pipeline.materializer.label());
}
