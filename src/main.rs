use anaxa_builder::{graph, parser};
use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "cargo-anaxa")]
#[command(bin_name = "cargo anaxa")]
#[command(version, about = "Anaxa Configuration System", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Source directory containing Kconfig.toml files
    #[arg(short, long, default_value = "src", global = true)]
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
        /// Path to the local configuration file
        #[arg(short, long, default_value = ".config")]
        config: PathBuf,
    },
    /// Generate code artifacts (Rust, C, DOT)
    Generate {
        /// Output directory for generated files
        #[arg(short, long, default_value = "generated")]
        out: PathBuf,
        /// Path to the local configuration file
        #[arg(short, long, default_value = ".config")]
        config_file: PathBuf,
        /// Generate C autoconf.h header
        #[arg(long)]
        c: bool,
        /// Generate Rust constants and cfgs
        #[arg(long)]
        rust: bool,
        /// Generate DOT dependency graph
        #[arg(long)]
        dot: bool,
    },
    /// Wrapper for cargo build with dynamic features from config
    Build {
        /// Path to the local configuration file
        #[arg(short, long, default_value = ".config")]
        config_file: PathBuf,
        /// Do not inject ANAXA_* environment variables
        #[arg(long)]
        no_env: bool,
        /// Additional arguments to pass to cargo build
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Generate clean config
    Savedefconfig {
        #[arg(short, long)]
        out: PathBuf,
        #[arg(short, long, default_value = ".config")]
        config_file: PathBuf,
    },
    /// Generate config from defconfig
    Defconfig {
        #[arg(short, long)]
        file: PathBuf,
        #[arg(short, long, default_value = ".config")]
        config_file: PathBuf,
    },
}

fn main() -> Result<()> {
    let mut args: Vec<String> = std::env::args().collect();
    // When called as `cargo anaxa`, the arguments are `["cargo-anaxa", "anaxa", ...]`
    // We skip the "anaxa" part if it's there.
    if args.len() > 1 && args[1] == "anaxa" {
        args.remove(1);
    }

    let cli = Cli::parse_from(args);
    let dir = &cli.dir;

    match &cli.command {
        Commands::Check => {
            let tree = parser::build_config_tree(dir)?;
            let configs = parser::flatten_configs(&tree);
            graph::ConfigGraph::build(&configs)?;

            for item in &configs {
                if let Some(default_val) = &item.default {
                    if let Err(e) = item.validate(default_val) {
                        anyhow::bail!("Invalid default value for config '{}': {}", item.name, e);
                    }
                }
                if item.config_type == anaxa_builder::schema::ConfigType::Choice
                    && (item.options.is_none() || item.options.as_ref().unwrap().is_empty())
                {
                    anyhow::bail!("Config '{}' is a choice but has no options", item.name);
                }
            }

            println!("Configuration valid ({} items, no cycles).", configs.len());
        }
        Commands::Dump => {
            let tree = parser::build_config_tree(dir)?;
            println!("{:#?}", tree);
        }
        Commands::Menuconfig { config } => {
            let tree = parser::build_config_tree(dir)?;
            anaxa_builder::tui::run(tree, config.clone())?;
        }
        Commands::Generate {
            out,
            config_file,
            c,
            rust,
            dot,
        } => {
            let tree = parser::build_config_tree(dir)?;
            let configs = parser::flatten_configs(&tree);
            let values = anaxa_builder::config_io::load_config(config_file, &configs)?;

            if !out.exists() {
                std::fs::create_dir_all(out)?;
            }

            if *rust {
                let rust_code = anaxa_builder::codegen::rust::generate_consts(&configs, &values)?;
                std::fs::write(out.join("config.rs"), rust_code)?;
                println!("Generated Rust constants in {:?}", out.join("config.rs"));
            }

            if *c {
                let c_code = anaxa_builder::codegen::c::generate(&configs, &values)?;
                std::fs::write(out.join("autoconf.h"), c_code)?;
                println!("Generated C header in {:?}", out.join("autoconf.h"));
            }

            if *dot {
                let graph = anaxa_builder::graph::ConfigGraph::build(&configs)?;
                let dot_code = anaxa_builder::codegen::dot::generate(&graph)?;
                std::fs::write(out.join("depends.dot"), dot_code)?;
                println!("Generated DOT graph in {:?}", out.join("depends.dot"));
            }
        }
        Commands::Build {
            config_file,
            no_env,
            args,
        } => {
            let tree = parser::build_config_tree(dir)?;
            let configs = parser::flatten_configs(&tree);
            let values = anaxa_builder::config_io::load_config(config_file, &configs)?;

            let mut features = Vec::new();
            let mut cfgs = Vec::new();
            for item in &configs {
                if let Some(val) = values.get(&item.name) {
                    if val.as_bool() == Some(true) {
                        if cfgs.contains(&item.name) {
                            continue;
                        }
                        cfgs.push(item.name.clone());
                        if let Some(f) = &item.feature {
                            features.extend(f.iter().cloned());
                        }
                    }
                }
            }

            let mut cmd = std::process::Command::new("cargo");
            cmd.arg("build");
            if !features.is_empty() {
                cmd.arg("--features");
                cmd.arg(features.join(","));
            }
            if !cfgs.is_empty() {
                cmd.env("RUSTFLAGS", format!("--cfg {}", cfgs.join(" --cfg ")));
            }
            if !*no_env {
                for (k, v) in values.iter() {
                    let v = match v {
                        toml::Value::String(s) => s.clone(),
                        toml::Value::Integer(i) => i.to_string(),
                        toml::Value::Float(f) => f.to_string(),
                        toml::Value::Boolean(b) => b.to_string(),
                        _ => continue,
                    };
                    cmd.env(format!("ANAXA_{}", k.to_uppercase()), v);
                }
            }
            cmd.args(args);

            println!("Executing: {:?}", cmd);
            let status = cmd.status()?;
            if !status.success() {
                std::process::exit(status.code().unwrap_or(1));
            }
        }
        Commands::Savedefconfig { out, config_file } => {
            let tree = parser::build_config_tree(dir)?;
            let configs = parser::flatten_configs(&tree);
            let values = anaxa_builder::config_io::load_config(config_file, &configs)?;
            let minimal = anaxa_builder::config_io::get_minimal_config(&values, &configs);
            anaxa_builder::config_io::save_config(out, &minimal)?;
            println!("Saved minimal defconfig to {:?}", out);
        }
        Commands::Defconfig { file, config_file } => {
            let tree = parser::build_config_tree(dir)?;
            let configs = parser::flatten_configs(&tree);
            let values = anaxa_builder::config_io::load_config(file, &configs)?;
            anaxa_builder::config_io::save_config(config_file, &values)?;
            println!("Updated configuration from {:?} to {:?}", file, config_file);
        }
    }
    Ok(())
}
