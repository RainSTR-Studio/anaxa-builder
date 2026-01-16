use anaxa_builder::{codegen, config_io, graph, parser, tui};
use anyhow::Result;
use clap::{Parser, Subcommand};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "anaxa-config")]
#[command(about = "Anaxa Configuration System", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Check {
        #[arg(short, long, default_value = ".")]
        dir: String,
    },
    Dump {
        #[arg(short, long, default_value = ".")]
        dir: String,
    },
    Menuconfig {
        #[arg(short, long, default_value = ".")]
        dir: String,
        #[arg(long, default_value = ".config")]
        config: String,
    },
    Generate {
        #[arg(short, long, default_value = ".")]
        dir: String,
        #[arg(long, default_value = "generated")]
        out: String,
        #[arg(long, default_value = ".config")]
        config_file: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Check { dir } => {
            let path = Path::new(dir);
            println!("Scanning directory: {:?}", path);
            let config = parser::scan_and_parse(path)?;
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
        Commands::Dump { dir } => {
            let path = Path::new(dir);
            let config = parser::scan_and_parse(path)?;
            println!("{:#?}", config);
        }
        Commands::Menuconfig { dir, config } => {
            let path = Path::new(dir);
            let config_path = PathBuf::from(config);
            let parsed_config = parser::scan_and_parse(path)?;

            tui::run(parsed_config, config_path)?;
        }
        Commands::Generate {
            dir,
            out,
            config_file,
        } => {
            let path = Path::new(dir);
            let out_path = PathBuf::from(out);
            let config_path = PathBuf::from(config_file);
            let parsed_config = parser::scan_and_parse(path)?;

            let values = config_io::load_config(&config_path, &parsed_config.items)?;

            fs::create_dir_all(&out_path)?;

            let c_content = codegen::c::generate(&parsed_config.items, &values)?;
            fs::write(out_path.join("autoconf.h"), c_content)?;
            println!("Generated {:?}", out_path.join("autoconf.h"));

            let rust_content = codegen::rust::generate_consts(&parsed_config.items, &values)?;
            fs::write(out_path.join("config.rs"), rust_content)?;
            println!("Generated {:?}", out_path.join("config.rs"));

            match graph::ConfigGraph::build(&parsed_config.items) {
                Ok(g) => {
                    let dot_content = codegen::dot::generate(&g)?;
                    fs::write(out_path.join("depends.dot"), dot_content)?;
                    println!("Generated {:?}", out_path.join("depends.dot"));
                }
                Err(e) => eprintln!("Warning: Failed to build graph for DOT generation: {}", e),
            }

            println!("--- Cargo Instructions (for build.rs) ---");
            let cargo_content = codegen::rust::generate_cargo_keys(&parsed_config.items, &values)?;
            print!("{}", cargo_content);
        }
    }

    Ok(())
}
