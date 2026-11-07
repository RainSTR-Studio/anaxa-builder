//! Helpers for `build.rs` to integrate Anaxa configuration.
use crate::schema::ConfigType;
use crate::{codegen, config_io, evaluator, parser};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// Helper for `build.rs` to integrate Anaxa configuration.
pub struct BuildHelper {
    kconfig_dir: PathBuf,
    config_file: PathBuf,
    out_dir: PathBuf,
}

impl BuildHelper {
    /// Create a new helper instance.
    ///
    /// # Errors
    /// This will return an error if the `OUT_DIR` environment variable
    /// is not set, which is required for writing the generated `config.rs`.
    pub fn new() -> Result<Self> {
        let out_dir = env::var_os("OUT_DIR").context("OUT_DIR not set")?.into();
        Ok(Self {
            kconfig_dir: PathBuf::from("src"),
            config_file: PathBuf::from(".config"),
            out_dir,
        })
    }

    /// Specify the directory to scan for `Kconfig.toml` files.
    pub fn with_kconfig_dir<P: Into<PathBuf>>(mut self, dir: P) -> Self {
        self.kconfig_dir = dir.into();
        self
    }

    /// Specify the path to the configuration file (e.g., `.config`).
    pub fn with_config_file<P: Into<PathBuf>>(mut self, file: P) -> Self {
        self.config_file = file.into();
        self
    }

    /// Build the configuration and emit Cargo instructions.
    pub fn build(self) -> Result<()> {
        emit_instructions(&self.kconfig_dir, &self.config_file, &self.out_dir)
    }
}

/// Helper for `build.rs` to integrate Anaxa configuration.
///
/// This function:
/// 1. Scans `kconfig_dir` for `Kconfig.toml` files.
/// 2. Loads configuration values from `config_file`.
/// 3. Generates `config.rs` in `OUT_DIR`.
/// 4. Emits `cargo:rustc-cfg` for enabled boolean configs.
/// 5. Emits `cargo:rerun-if-changed` for the config file and all `Kconfig.toml` files.
pub fn emit_cargo_instructions<P1, P2>(kconfig_dir: P1, config_file: P2) -> Result<()>
where
    P1: AsRef<Path>,
    P2: AsRef<Path>,
{
    let out_dir = env::var_os("OUT_DIR").context("OUT_DIR not set")?;
    emit_instructions(
        kconfig_dir.as_ref(),
        config_file.as_ref(),
        Path::new(&out_dir),
    )
}

/// [`BuildHelper::build`] 与 [`emit_cargo_instructions`] 的唯一实现。
///
/// 两套逻辑各写一遍是此前的实际状态，结果是指令集逐渐漂移
/// （`emit_cargo_instructions` 一度漏发 `rustc-check-cfg`）。
/// 所有 `cargo:` 指令都必须只在一个地方发射。
fn emit_instructions(kconfig_dir: &Path, config_file: &Path, out_dir: &Path) -> Result<()> {
    check_version_compatibility()?;

    let tree = parser::build_config_tree_detailed(kconfig_dir)?;
    let configs = parser::flatten_configs(&tree.root);
    parser::validate_configs(&configs)?;

    // build 脚本只读配置：文件缺失时用默认值继续，绝不隐式生成 .config
    let mut warnings = Vec::new();
    let raw = config_io::load_config(config_file, &configs, &mut warnings)?;
    for w in &warnings {
        println!("cargo:warning=anaxa: {w}");
    }

    // 产物一律使用"生效值"：依赖不满足的项不能出现在 autoconf.h / config.rs / cfg 里
    let values = evaluator::effective_values(&configs, &raw)?;
    for (name, from, to) in evaluator::dependency_overrides(&configs, &raw, &values) {
        println!(
            "cargo:warning=anaxa: '{name}' is {from} in the config file but forced to {to} because its depends_on is not satisfied"
        );
    }

    let rust_code = codegen::rust::generate_consts(&configs, &values)?;
    let out_path = out_dir.join("config.rs");
    fs::write(&out_path, rust_code).with_context(|| format!("Failed to write to {out_path:?}"))?;

    println!("cargo:rerun-if-changed={}", config_file.display());
    // 逐文件追踪：只监听扫描根目录会漏掉 `[menu]` 用 `../` 引用的外部目录，
    // 那些目录里的 Kconfig.toml 改了不会触发 build.rs 重跑
    for file in &tree.files {
        println!("cargo:rerun-if-changed={}", file.display());
    }

    emit_cfg_flags(&configs, &values);
    emit_env_vars(&values);

    Ok(())
}

/// 为布尔项声明 `check-cfg` 并在开启时发射对应 `cfg`。
fn emit_cfg_flags(configs: &[crate::schema::ConfigItem], values: &HashMap<String, toml::Value>) {
    for item in configs {
        // 只有布尔项才映射为 cfg；非布尔项没有 cfg 语义
        if item.config_type != ConfigType::Bool {
            continue;
        }
        // check-cfg 描述的是"这个名字是已知 cfg"，与当前取值无关，
        // 因此必须对所有布尔项无条件声明；只在开启时声明会让未开启的
        // cfg 在引用处触发 unexpected_cfgs 警告
        println!("cargo::rustc-check-cfg=cfg({})", item.name);
        if values.get(&item.name).and_then(|v| v.as_bool()) == Some(true) {
            println!("cargo:rustc-cfg={}", item.name);
        }
    }
}

/// 把配置值导出为 `ANAXA_<NAME>` 环境变量，供运行期读取。
fn emit_env_vars(values: &HashMap<String, toml::Value>) {
    for (k, v) in values {
        let v_str = match v {
            toml::Value::String(s) => s.clone(),
            toml::Value::Integer(i) => i.to_string(),
            toml::Value::Float(f) => f.to_string(),
            toml::Value::Boolean(b) => b.to_string(),
            _ => continue,
        };
        println!("cargo:rustc-env=ANAXA_{}={}", k.to_uppercase(), v_str);
    }
}

fn check_version_compatibility() -> Result<()> {
    if let Ok(cli_version) = env::var("ANAXA_CLI_VERSION") {
        let lib_version = env!("CARGO_PKG_VERSION");
        if cli_version != lib_version {
            // Check if major/minor versions match
            let cli_parts: Vec<&str> = cli_version.split('.').collect();
            let lib_parts: Vec<&str> = lib_version.split('.').collect();

            if cli_parts.len() >= 2 && lib_parts.len() >= 2 {
                if cli_parts[0] != lib_parts[0] || cli_parts[1] != lib_parts[1] {
                    println!(
                        "cargo:warning=Version mismatch: anaxa-builder CLI (v{}) and library (v{}) are not compatible.",
                        cli_version, lib_version
                    );
                    println!(
                        "cargo:warning=Please ensure both CLI and library have the same major.minor version."
                    );
                }
            } else if cli_version != lib_version {
                println!(
                    "cargo:warning=Version mismatch detected: CLI v{}, Lib v{}",
                    cli_version, lib_version
                );
            }
        }
    }
    Ok(())
}
