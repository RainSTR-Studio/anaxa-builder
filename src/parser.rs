use crate::schema::{ConfigItem, KconfigFile, Menu};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug)]
pub struct ParsedConfig {
    pub items: Vec<ConfigItem>,
    pub menus: HashMap<PathBuf, Menu>,
    pub file_map: Vec<(PathBuf, Vec<ConfigItem>)>,
}

pub fn scan_and_parse(root_dir: &Path) -> Result<ParsedConfig> {
    let mut items = Vec::new();
    let mut menus = HashMap::new();
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
                let relative_path = path
                    .strip_prefix(root_dir)
                    .with_context(|| format!("Failed to get relative path: {:?}", path))?;
                menus.insert(relative_path.to_path_buf(), menu);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_scan_and_parse() -> Result<()> {
        let dir = tempdir()?;
        let root = dir.path();

        let net_dir = root.join("net");
        fs::create_dir(&net_dir)?;
        let config_content = r#"
[menu]
title = "Networking"

[[config]]
name = "ENABLE_NET"
type = "bool"
default = true
desc = "Enable networking"
"#;
        fs::write(net_dir.join("Kconfig.toml"), config_content)?;

        let parsed = scan_and_parse(root)?;
        assert_eq!(parsed.items.len(), 1);
        assert_eq!(parsed.items[0].name, "ENABLE_NET");
        assert_eq!(parsed.menus.len(), 1);
        assert_eq!(
            parsed
                .menus
                .get(&PathBuf::from("net/Kconfig.toml"))
                .unwrap()
                .title,
            "Networking"
        );

        Ok(())
    }
}
