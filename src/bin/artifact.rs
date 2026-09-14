use artifact::{AllowAllPolicy, ArtifactSDK, RecipeSpec};
use std::env;
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage();
        std::process::exit(1);
    }

    match args[1].as_str() {
        "version" => handle_version(),
        "inspect" => handle_inspect(&args),
        "build" => handle_build(&args),
        "recipe" => handle_recipe(&args),
        "--help" | "-h" | "help" => print_help(),
        _ => {
            eprintln!("Unknown command: {}", args[1]);
            print_usage();
            std::process::exit(1);
        }
    }
}

fn print_usage() {
    eprintln!(
        "Usage: artifact <command> [options]\n\
         Commands: version, recipe, inspect, build, help"
    );
}

fn print_help() {
    println!(
        "artifact - declarative artifact builder\n\n\
         Commands:\n\
         \n\
           version              Show version\n\
           recipe <type>        Show recipe template\n\
           inspect <path>       Inspect artifact specification\n\
           build <path>         Build artifact from source\n\
           help                 Show this help\n\n\
         Examples:\n\
           artifact build ./project --recipe wasm\n\
           artifact build ./project --recipe zip\n\
           artifact build ./project --recipe tar\n\
           artifact inspect pipeline.json"
    );
}

fn handle_version() {
    println!("artifact 0.1.0");
}

fn handle_recipe(args: &[String]) {
    if args.len() < 3 {
        eprintln!("Usage: artifact recipe <type>");
        eprintln!("Types: wasm, zip, tar");
        std::process::exit(1);
    }

    match args[2].as_str() {
        "wasm" => println!("RecipeSpec::wasm()"),
        "zip" => println!("RecipeSpec::directory_zip()"),
        "tar" => println!("RecipeSpec::directory_tar()"),
        _ => {
            eprintln!("Unknown recipe type: {}", args[2]);
            std::process::exit(1);
        }
    }
}

fn handle_inspect(args: &[String]) {
    if args.len() < 3 {
        eprintln!("Usage: artifact inspect <path> [--json]");
        std::process::exit(1);
    }

    let _path = PathBuf::from(&args[2]);
    let json_output = args.contains(&"--json".to_string());

    if json_output {
        println!("{{ \"pipeline_identity\": \"<identity>\", \"stages\": [] }}");
    } else {
        println!("Inspect command (not fully implemented)");
    }
}

fn handle_build(args: &[String]) {
    if args.len() < 3 {
        eprintln!("Usage: artifact build <path> --recipe <type> [--json]");
        std::process::exit(1);
    }

    let source_path = PathBuf::from(&args[2]);
    let recipe_type = extract_arg(args, "--recipe").unwrap_or_else(|| {
        eprintln!("Error: --recipe is required");
        std::process::exit(1);
    });
    let json_output = args.contains(&"--json".to_string());

    let recipe = match recipe_type.as_str() {
        "wasm" => RecipeSpec::wasm(),
        "zip" => RecipeSpec::directory_zip(),
        "tar" => RecipeSpec::directory_tar(),
        _ => {
            eprintln!("Unknown recipe type: {}", recipe_type);
            std::process::exit(1);
        }
    };

    let pipeline = match recipe.compile() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error compiling recipe: {}", e);
            std::process::exit(1);
        }
    };

    let sdk_pipeline = ArtifactSDK::pipeline_from_spec(pipeline);

    let inspection = match sdk_pipeline.inspect() {
        Ok(i) => i,
        Err(e) => {
            eprintln!("Error inspecting pipeline: {}", e);
            std::process::exit(1);
        }
    };

    if !json_output {
        println!(
            "\nPipeline: {}",
            &inspection.pipeline_identity[..16.min(inspection.pipeline_identity.len())]
        );
        println!("Materializer: {}", inspection.materializer);
        println!("\nStages:");
        for stage in &inspection.stages {
            println!("  - {}", stage.label);
        }
        println!("\nRequired capabilities:");
        for cap in &inspection.required_capabilities {
            println!("  - {}.{}", cap.name, cap.version);
        }
    }

    println!("\nAuthorization: AllowAll");
    for cap in &inspection.required_capabilities {
        println!("  ✓ {}.{}", cap.name, cap.version);
    }

    println!("\nExecuting...");
    match sdk_pipeline.build_with_authorization(&source_path, &AllowAllPolicy) {
        Ok((artifact, evidence)) => {
            if json_output {
                println!(
                    "{{ \"artifact_identity\": \"{}\", \"entries\": {}, \"success\": true }}",
                    artifact.identity(),
                    artifact.entries_count()
                );
            } else {
                println!("✓ Build succeeded");
                println!(
                    "  Artifact: {}",
                    &artifact.identity()[..16.min(artifact.identity().len())]
                );
                println!("  Entries: {}", artifact.entries_count());

                if evidence.is_successful() {
                    println!("\nExecution stages:");
                    for stage in evidence.stage_trace() {
                        println!("  ✓ {}", stage.label);
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("✗ Build failed: {}", e);
            std::process::exit(1);
        }
    }
}

fn extract_arg(args: &[String], flag: &str) -> Option<String> {
    for (i, arg) in args.iter().enumerate() {
        if arg == flag && i + 1 < args.len() {
            return Some(args[i + 1].clone());
        }
    }
    None
}
