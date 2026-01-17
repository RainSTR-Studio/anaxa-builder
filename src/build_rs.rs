use crate::{codegen, config_io, parser};
use anyhow::{Context, Result};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

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
    let kconfig_dir = kconfig_dir.as_ref();
    let config_file = config_file.as_ref();

    let tree = parser::build_config_tree(kconfig_dir)?;
    let configs = parser::flatten_configs(&tree);

    let values = config_io::load_config(config_file, &configs)?;

    let out_dir = env::var_os("OUT_DIR").context("OUT_DIR not set")?;
    let out_path = PathBuf::from(out_dir).join("config.rs");
    let rust_code = codegen::rust::generate_consts(&configs, &values)?;
    fs::write(&out_path, rust_code)
        .with_context(|| format!("Failed to write to {:?}", out_path))?;

    println!("cargo:rerun-if-changed={}", config_file.display());

    emit_rerun_if_changed(kconfig_dir)?;

    for item in &configs {
        if let Some(val) = values.get(&item.name) {
            if val.as_bool() == Some(true) {
                println!("cargo:rustc-cfg={}", item.name);
            }
        }
    }

    Ok(())
}

fn emit_rerun_if_changed(dir: &Path) -> Result<()> {
    use walkdir::WalkDir;
    for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        if entry.file_name() == "Kconfig.toml" {
            println!("cargo:rerun-if-changed={}", entry.path().display());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_emit_cargo_instructions() -> Result<()> {
        let dir = tempdir()?;
        let kconfig_path = dir.path().join("Kconfig.toml");
        fs::write(
            &kconfig_path,
            r#"
[[config]]
name = "TEST_BOOL"
type = "bool"
default = true
desc = "Test"
"#,
        )?;

        let config_file = dir.path().join(".config");
        fs::write(&config_file, "TEST_BOOL = true\n")?;

        let out_dir = tempdir()?;
        unsafe {
            env::set_var("OUT_DIR", out_dir.path());
        }

        emit_cargo_instructions(dir.path(), &config_file)?;

        let config_rs = out_dir.path().join("config.rs");
        assert!(config_rs.exists());
        let content = fs::read_to_string(config_rs)?;
        assert!(content.contains("pub const TEST_BOOL: bool = true;"));

        Ok(())
    }
}
