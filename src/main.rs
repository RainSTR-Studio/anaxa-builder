use anaxa_builder::{codegen, config_io, graph, parser, tui};
use anyhow::Result;
use clap::{Parser, Subcommand};
use std::fs;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "anaxa-config")]
#[command(about = "Anaxa Configuration System", long_about = None)]
struct Cli {
    #[arg(default_value = ".")]
    dir: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Validate schemas and check for cycles
    Check,
    /// Inspect parsed configuration structure
    Dump,
    /// Launch interactive TUI
    Menuconfig {
        #[arg(short, long, default_value = ".config")]
        config: PathBuf,
    },
    /// Generate code artifacts
    Generate {
        #[arg(short, long, default_value = "generated")]
        out: PathBuf,
        #[arg(short, long, default_value = ".config")]
        config_file: PathBuf,

        /// Generate C header
        #[arg(long)]
        c: bool,

        /// Generate Rust constants
        #[arg(long)]
        rust: bool,

        /// Generate DOT dependency graph
        #[arg(long)]
        dot: bool,
    },
    /// Wrapper for cargo build with dynamic features from config
    Build {
        #[arg(short, long, default_value = ".config")]
        config_file: PathBuf,

        /// Arguments to pass to cargo build
        #[arg(last = true)]
        args: Vec<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let dir = &cli.dir;

    match &cli.command {
        Commands::Check => {
            println!("Scanning directory: {:?}", dir);
            let config = parser::scan_and_parse(dir)?;
            println!(
                "Found {} config items and {} menus.",
                config.items.len(),
                config.menus.len()
            );

            println!("Building dependency graph...");
            match graph::ConfigGraph::build(&config.items) {
                Ok(_) => println!("Dependency graph built successfully. No cycles detected."),
                Err(e) => eprintln!("Error: {}", e),
            }
        }
        Commands::Dump => {
            let config = parser::scan_and_parse(dir)?;
            println!("{:#?}", config);
        }
        Commands::Menuconfig { config } => {
            let parsed_config = parser::scan_and_parse(dir)?;
            tui::run(parsed_config, config.clone())?;
        }
        Commands::Generate {
            out,
            config_file,
            c,
            rust,
            dot,
        } => {
            let parsed_config = parser::scan_and_parse(dir)?;
            let values = config_io::load_config(config_file, &parsed_config.items)?;

            // If no specific flags are set, generate all
            let generate_all = !(*c || *rust || *dot);
            let do_c = *c || generate_all;
            let do_rust = *rust || generate_all;
            let do_dot = *dot || generate_all;

            fs::create_dir_all(out)?;

            if do_c {
                let c_content = codegen::c::generate(&parsed_config.items, &values)?;
                fs::write(out.join("autoconf.h"), c_content)?;
                println!("Generated {:?}", out.join("autoconf.h"));
            }

            if do_rust {
                let rust_content = codegen::rust::generate_consts(&parsed_config.items, &values)?;
                fs::write(out.join("config.rs"), rust_content)?;
                println!("Generated {:?}", out.join("config.rs"));
            }

            if do_dot {
                match graph::ConfigGraph::build(&parsed_config.items) {
                    Ok(g) => {
                        let dot_content = codegen::dot::generate(&g)?;
                        fs::write(out.join("depends.dot"), dot_content)?;
                        println!("Generated {:?}", out.join("depends.dot"));
                    }
                    Err(e) => eprintln!("Warning: Failed to build graph for DOT generation: {}", e),
                }
            }
        }
        Commands::Build { config_file, args } => {
            let parsed_config = parser::scan_and_parse(dir)?;
            let values = config_io::load_config(config_file, &parsed_config.items)?;

            // Build evaluator and populate with values for dependency checking
            let mut evaluator = anaxa_builder::logic::Evaluator::new();
            for (name, val) in &values {
                evaluator.set_variable(name, val)?;
            }

            let mut features = Vec::new();
            for item in &parsed_config.items {
                if let Some(item_features) = &item.feature {
                    // Only bool items trigger features for now
                    let is_enabled = if let Some(val) = values.get(&item.name) {
                        val.as_bool() == Some(true)
                    } else {
                        false
                    };

                    if is_enabled {
                        // Verify dependencies are met before enabling feature
                        let deps_met = if let Some(deps) = &item.depends_on {
                            evaluator.check_dependency(deps)?
                        } else {
                            true
                        };

                        if deps_met {
                            features.extend(item_features.clone());
                        }
                    }
                }
            }

            println!("Wrapper: Running cargo build with features: {:?}", features);

            let mut cmd = std::process::Command::new("cargo");
            cmd.arg("build");
            if !features.is_empty() {
                cmd.arg("--features");
                cmd.arg(features.join(","));
            }
            cmd.args(args);

            // Pass through stdin/stdout/stderr
            let status = cmd.status()?;
            if !status.success() {
                std::process::exit(status.code().unwrap_or(1));
            }
        }
    }

    Ok(())
}
