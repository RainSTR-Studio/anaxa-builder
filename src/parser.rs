use crate::schema::{ConfigItem, KconfigFile, Menu};
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug)]
pub struct ParsedConfig {
    pub items: Vec<ConfigItem>,
    pub menus: Vec<(PathBuf, Menu)>,
    pub file_map: Vec<(PathBuf, Vec<ConfigItem>)>,
}

pub fn scan_and_parse(root_dir: &Path) -> Result<ParsedConfig> {
    let mut items = Vec::new();
    let mut menus = Vec::new();
    let mut file_map = Vec::new();

    for entry in WalkDir::new(root_dir).sort_by_file_name() {
        let entry = entry?;
        if entry.file_name() == "Kconfig.toml" {
            let path = entry.path();
            let content = fs::read_to_string(path)
                .with_context(|| format!("Failed to read config file: {:?}", path))?;

            let config_file: KconfigFile = toml::from_str(&content)
                .with_context(|| format!("Failed to parse TOML: {:?}", path))?;

            let mut file_items = Vec::new();

            if let Some(menu) = config_file.menu {
                menus.push((path.to_path_buf(), menu));
            }

            if let Some(configs) = config_file.configs {
                items.extend(configs.clone());
                file_items.extend(configs);
            }

            file_map.push((path.to_path_buf(), file_items));
        }
    }

    Ok(ParsedConfig {
        items,
        menus,
        file_map,
    })
}
