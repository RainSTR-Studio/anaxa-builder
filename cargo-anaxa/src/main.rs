mod tui;

use anaxa_builder::internal::{graph, parser};
use anyhow::Result;
use clap::{Parser, Subcommand};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "cargo-anaxa")]
#[command(bin_name = "cargo anaxa")]
#[command(version, about = "Anaxa Configuration System", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Source directory containing Kconfig.toml files
    #[arg(short, long, default_value = ".", global = true)]
    dir: PathBuf,
}

#[derive(Subcommand)]
enum Commands {
    /// Validate schemas and check for cycles
    ConfigCheck,
    /// Inspect parsed configuration structure
    Dump,
    /// Launch interactive TUI
    Menuconfig {
        /// Path to the local configuration file
        #[arg(short, long, default_value = ".config")]
        config: PathBuf,
    },
    /// Generate code artifacts (Rust, C, DOT)
    ///
    /// 未指定 --c/--rust/--dot 时默认生成全部三种产物。
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
    /// Wrapper for cargo check with dynamic features from config
    Check {
        /// Path to the local configuration file
        #[arg(short, long, default_value = ".config")]
        config_file: PathBuf,
        /// Do not inject ANAXA_* environment variables
        #[arg(long)]
        no_env: bool,
        /// Additional arguments to pass to cargo check
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
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
    /// Wrapper for cargo run with dynamic features from config
    Run {
        /// Path to the local configuration file
        #[arg(short, long, default_value = ".config")]
        config_file: PathBuf,
        /// Do not inject ANAXA_* environment variables
        #[arg(long)]
        no_env: bool,
        /// Additional arguments to pass to cargo run
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Wrapper for cargo test with dynamic features from config
    Test {
        /// Path to the local configuration file
        #[arg(short, long, default_value = ".config")]
        config_file: PathBuf,
        /// Do not inject ANAXA_* environment variables
        #[arg(long)]
        no_env: bool,
        /// Additional arguments to pass to cargo test
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
        Commands::ConfigCheck => {
            let tree = parser::build_config_tree(dir)?;
            let configs = parser::flatten_configs(&tree);
            parser::validate_configs(&configs)?;
            graph::ConfigGraph::build(&configs)?;

            println!("Configuration valid ({} items, no cycles).", configs.len());
        }
        Commands::Dump => {
            let tree = parser::build_config_tree(dir)?;
            println!("{:#?}", tree);
        }
        Commands::Menuconfig { config } => {
            let tree = parser::build_config_tree(dir)?;
            // 进入 TUI 之前先做静态校验：非法 depends_on 在备用屏幕里只会显示成
            // 一条转瞬即逝的通知，退出后就再也看不到，用户无法据此定位问题
            let configs = parser::flatten_configs(&tree);
            parser::validate_configs(&configs)?;
            tui::run(tree, config.clone())?;
        }
        Commands::Generate {
            out,
            config_file,
            c,
            rust,
            dot,
        } => {
            // 未指定任何产物类型时默认全部生成：静默什么都不做对用户毫无意义
            let (c, rust, dot) = if !c && !rust && !dot {
                println!("No artifact selected, generating all (Rust, C, DOT).");
                (true, true, true)
            } else {
                (*c, *rust, *dot)
            };

            let tree = parser::build_config_tree(dir)?;
            let configs = parser::flatten_configs(&tree);
            parser::validate_configs(&configs)?;
            let values = load_config_with_report(config_file, &configs)?;

            if !out.exists() {
                std::fs::create_dir_all(out)?;
            }

            if rust {
                let rust_code = anaxa_builder::codegen::rust::generate_consts(&configs, &values)?;
                std::fs::write(out.join("config.rs"), rust_code)?;
                println!("Generated Rust constants in {:?}", out.join("config.rs"));
            }

            if c {
                let c_code = anaxa_builder::codegen::c::generate(&configs, &values)?;
                std::fs::write(out.join("autoconf.h"), c_code)?;
                println!("Generated C header in {:?}", out.join("autoconf.h"));
            }

            if dot {
                let graph = anaxa_builder::graph::ConfigGraph::build(&configs)?;
                let dot_code = anaxa_builder::codegen::dot::generate(&graph)?;
                std::fs::write(out.join("depends.dot"), dot_code)?;
                println!("Generated DOT graph in {:?}", out.join("depends.dot"));
            }
        }
        Commands::Check {
            config_file,
            no_env,
            args,
        } => {
            run_cargo_wrapper("check", dir, config_file, *no_env, args)?;
        }
        Commands::Build {
            config_file,
            no_env,
            args,
        } => {
            run_cargo_wrapper("build", dir, config_file, *no_env, args)?;
        }
        Commands::Run {
            config_file,
            no_env,
            args,
        } => {
            run_cargo_wrapper("run", dir, config_file, *no_env, args)?;
        }
        Commands::Test {
            config_file,
            no_env,
            args,
        } => {
            run_cargo_wrapper("test", dir, config_file, *no_env, args)?;
        }
        Commands::Savedefconfig { out, config_file } => {
            let tree = parser::build_config_tree(dir)?;
            let configs = parser::flatten_configs(&tree);
            let values = load_config_with_report(config_file, &configs)?;
            let minimal = anaxa_builder::config_io::get_minimal_config(&values, &configs);
            anaxa_builder::config_io::save_config(out, &minimal)?;

            // 只记录"与默认值不同"的项，因此还原时缺失项会自动取 schema 默认值
            println!(
                "Saved defconfig with {} non-default entries to {:?} (out of {} configs)",
                minimal.len(),
                out,
                configs.len()
            );
        }
        Commands::Defconfig { file, config_file } => {
            let tree = parser::build_config_tree(dir)?;
            let configs = parser::flatten_configs(&tree);

            // defconfig 是"以默认值为底、只覆盖差异项"的语义：
            // 必须先收集 schema 默认值，再用 defconfig 覆盖，否则缺失项会丢值。
            let explicit = anaxa_builder::config_io::load_file_values(file, &configs)?;
            let mut values = anaxa_builder::evaluator::collect_defaults(&configs);
            let touched = override_values(&mut values, &explicit, &configs);

            // 落盘写生效值：defconfig 的目的是"重建同一次构建"，
            // 把依赖不满足、实际不生效的差异项写进去只会制造不稳定的往返
            let values = anaxa_builder::evaluator::effective_values(&configs, &values)?;
            anaxa_builder::config_io::save_config(config_file, &values)?;
            println!(
                "Wrote {:?} from {:?}: {} entries from schema defaults, {} overridden by defconfig",
                config_file,
                file,
                configs.len() - touched,
                touched
            );
        }
    }
    Ok(())
}

/// 用 defconfig 中的值覆盖 `values`（后者应已填满 schema 默认值），返回被覆盖的条目数。
///
/// 只覆盖 schema 中真实存在的键；`load_config` 已剔除未知键并给出告警。
fn override_values(
    values: &mut HashMap<String, toml::Value>,
    overrides: &HashMap<String, toml::Value>,
    configs: &[anaxa_builder::schema::ConfigItem],
) -> usize {
    let mut touched = 0;
    for item in configs {
        if let Some(val) = overrides.get(&item.name) {
            values.insert(item.name.clone(), val.clone());
            touched += 1;
        }
    }
    touched
}

/// 读取配置并把条目级告警打印到 stderr（CLI 场景下 stderr 即用户可见通道）。
///
/// 返回的是**生效值**：`depends_on` 不满足的项已按隐含值回落。凡是会把配置
/// 写进产物或注入编译参数的调用方都必须走这里，否则关闭的父项下面那些子项
/// 依然会出现在 `autoconf.h`、`--cfg` 和 `--features` 里。
fn load_config_with_report(
    path: &std::path::Path,
    configs: &[anaxa_builder::schema::ConfigItem],
) -> Result<HashMap<String, toml::Value>> {
    let mut warnings = Vec::new();
    let values = anaxa_builder::config_io::load_effective_config(path, configs, &mut warnings)?;
    for w in &warnings {
        eprintln!("Warning: {}", w);
    }
    Ok(values)
}

fn run_cargo_wrapper(
    subcommand: &str,
    dir: &Path,
    config_file: &Path,
    no_env: bool,
    args: &[String],
) -> Result<()> {
    let tree = parser::build_config_tree(dir)?;
    let configs = parser::flatten_configs(&tree);
    parser::validate_configs(&configs)?;
    let values = load_config_with_report(config_file, &configs)?;

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
    cmd.arg(subcommand);
    cmd.env("ANAXA_CLI_VERSION", env!("CARGO_PKG_VERSION"));
    if !features.is_empty() {
        cmd.arg("--features");
        cmd.arg(features.join(","));
    }
    if !cfgs.is_empty() {
        let raw_rustflags = std::env::var("RUSTFLAGS").unwrap_or_default();
        cmd.env(
            "RUSTFLAGS",
            format!("{} --cfg {}", raw_rustflags, cfgs.join(" --cfg ")),
        );
    }
    if !no_env {
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
    Ok(())
}
