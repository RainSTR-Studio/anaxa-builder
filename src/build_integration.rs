use crate::{codegen, config_io, parser};
use anyhow::{Context, Result};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub struct Builder {
    search_dir: PathBuf,
    config_file: PathBuf,
    out_dir: Option<PathBuf>,
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

impl Builder {
    pub fn new() -> Self {
        Self {
            search_dir: PathBuf::from("."),
            config_file: PathBuf::from(".config"),
            out_dir: env::var_os("OUT_DIR").map(PathBuf::from),
        }
    }

    pub fn search_dir<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.search_dir = path.as_ref().to_path_buf();
        self
    }

    pub fn config_file<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.config_file = path.as_ref().to_path_buf();
        self
    }

    pub fn out_dir<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.out_dir = Some(path.as_ref().to_path_buf());
        self
    }

    pub fn generate(self) -> Result<()> {
        let out_dir = self
            .out_dir
            .ok_or_else(|| anyhow::anyhow!("OUT_DIR not set and not provided explicitly"))?;

        // 1. Scan and parse configuration
        println!("cargo:rerun-if-changed={}", self.search_dir.display());
        let parsed_config = parser::scan_and_parse(&self.search_dir)?;

        // Emit rerun-if-changed for every Kconfig.toml found
        for (path, _) in &parsed_config.file_map {
            println!("cargo:rerun-if-changed={}", path.display());
        }

        // However, we MUST watch the .config file.
        if self.config_file.exists() {
            println!("cargo:rerun-if-changed={}", self.config_file.display());
        }

        // 2. Load values
        let values = config_io::load_config(&self.config_file, &parsed_config.items)?;

        // 3. Create output directory
        fs::create_dir_all(&out_dir)?;

        // 4. Generate Rust constants
        let rust_content = codegen::rust::generate_consts(&parsed_config.items, &values)?;
        let rust_path = out_dir.join("config.rs");
        fs::write(&rust_path, rust_content)
            .with_context(|| format!("Failed to write to {:?}", rust_path))?;

        // 5. Generate C header (optional, but good for FFI)
        let c_content = codegen::c::generate(&parsed_config.items, &values)?;
        let c_path = out_dir.join("autoconf.h");
        fs::write(&c_path, c_content)
            .with_context(|| format!("Failed to write to {:?}", c_path))?;

        // 6. Generate Cargo keys
        let cargo_content = codegen::rust::generate_cargo_keys(&parsed_config.items, &values)?;
        print!("{}", cargo_content);

        Ok(())
    }
}
