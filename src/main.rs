use anaxa_builder::{graph, parser};
use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "anaxa-config")]
#[command(about = "Anaxa Configuration System", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(default_value = "src")]
    dir: PathBuf,
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
        #[arg(long)]
        c: bool,
        #[arg(long)]
        rust: bool,
        #[arg(long)]
        dot: bool,
    },
    /// Wrapper for cargo build with dynamic features from config
    Build {
        #[arg(short, long, default_value = ".config")]
        config_file: PathBuf,
        #[arg(last = true)]
        args: Vec<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let dir = &cli.dir;

    match &cli.command {
        Commands::Check => {
            let tree = parser::build_config_tree(dir)?;
            let configs = parser::flatten_configs(&tree);
            graph::ConfigGraph::build(&configs)?;
            println!("Configuration valid ({} items, no cycles).", configs.len());
        }
        Commands::Dump => {
            let tree = parser::build_config_tree(dir)?;
            println!("{:#?}", tree);
        }
        Commands::Menuconfig { config: _ } => {
            println!("TUI not yet implemented in this session.");
        }
        Commands::Generate { .. } => {
            println!("Generator not yet fully integrated.");
        }
        Commands::Build { .. } => {
            println!("Build wrapper not yet implemented.");
        }
    }
    Ok(())
}
