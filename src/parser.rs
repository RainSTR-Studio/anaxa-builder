use crate::schema::{ConfigItem, ConfigNode, KconfigFile};
use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Recursively scans the given root directory for `Kconfig.toml` files
/// and builds a hierarchical `ConfigNode` tree.
pub fn build_config_tree<P: AsRef<Path>>(root: P) -> Result<ConfigNode> {
    let root_path = root.as_ref().canonicalize()?;
    let mut nodes: BTreeMap<PathBuf, ConfigNode> = BTreeMap::new();

    for entry in WalkDir::new(&root_path)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_name() == "Kconfig.toml" {
            let path = entry.path();
            let rel_path = path.parent().unwrap().strip_prefix(&root_path)?;

            let content = fs::read_to_string(path)
                .with_context(|| format!("Failed to read config file: {:?}", path))?;

            let kconfig: KconfigFile = toml::from_str(&content)
                .with_context(|| format!("Failed to parse TOML structure in: {:?}", path))?;

            let desc = kconfig
                .title
                .clone()
                .unwrap_or_else(|| rel_path.to_string_lossy().into_owned());

            nodes.insert(
                rel_path.to_path_buf(),
                ConfigNode {
                    desc,
                    configs: kconfig.configs.unwrap_or_default(),
                    children: Vec::new(),
                    path: rel_path.to_string_lossy().into_owned(),
                    depends_on: kconfig.depends_on.clone(),
                },
            );
        }
    }

    // Assemble the tree
    // We use a separate map to store the final nodes because we need to move them into children
    let mut paths: Vec<_> = nodes.keys().cloned().collect();
    // Sort by depth descending so we attach children to parents correctly
    paths.sort_by_key(|p| std::cmp::Reverse(p.components().count()));

    for path in paths {
        if path.as_os_str().is_empty() {
            continue;
        }

        let current_node = nodes.remove(&path).unwrap();

        // Find parent
        let mut parent_path = path.parent().unwrap_or(Path::new("")).to_path_buf();
        while !parent_path.as_os_str().is_empty() && !nodes.contains_key(&parent_path) {
            parent_path = parent_path.parent().unwrap_or(Path::new("")).to_path_buf();
        }

        if let Some(parent_node) = nodes.get_mut(&parent_path) {
            parent_node.children.push(current_node);
        } else if parent_path.as_os_str().is_empty() && nodes.contains_key(&PathBuf::new()) {
            nodes
                .get_mut(&PathBuf::new())
                .unwrap()
                .children
                .push(current_node);
        } else {
            // If no parent found, it's effectively a root-level child or we need to create a root
            nodes.insert(path, current_node);
        }
    }

    nodes
        .remove(&PathBuf::new())
        .context("No root Kconfig.toml found in the root directory")
}

/// Helper to flatten the hierarchical tree into a flat list of items
pub fn flatten_configs(node: &ConfigNode) -> Vec<ConfigItem> {
    let mut all_configs = node.configs.clone();
    for child in &node.children {
        all_configs.extend(flatten_configs(child));
    }
    all_configs
}

/// Legacy function for compatibility, if needed
pub fn parse_kconfigs<P: AsRef<Path>>(root: P) -> Result<Vec<ConfigItem>> {
    let tree = build_config_tree(root)?;
    Ok(flatten_configs(&tree))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::ConfigType;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_flatten_configs() {
        let item1 = ConfigItem {
            name: "A".to_string(),
            config_type: ConfigType::Bool,
            default: None,
            desc: "A".to_string(),
            depends_on: None,
            help: None,
            options: None,
            feature: None,
            range: None,
            regex: None,
        };
        let item2 = ConfigItem {
            name: "B".to_string(),
            config_type: ConfigType::Bool,
            default: None,
            desc: "B".to_string(),
            depends_on: None,
            help: None,
            options: None,
            feature: None,
            range: None,
            regex: None,
        };

        let root = ConfigNode {
            desc: "root".to_string(),
            configs: vec![item1.clone()],
            children: vec![ConfigNode {
                desc: "child".to_string(),
                configs: vec![item2.clone()],
                children: Vec::new(),
                path: "child".to_string(),
                depends_on: None,
            }],
            path: "".to_string(),
            depends_on: None,
        };

        let flattened = flatten_configs(&root);
        assert_eq!(flattened.len(), 2);
        assert_eq!(flattened[0].name, "A");
        assert_eq!(flattened[1].name, "B");
    }

    #[test]
    fn test_build_config_tree() -> Result<()> {
        let dir = tempdir()?;
        let root_path = dir.path();

        let root_kconfig = r#"
            title = "Root"
            [[config]]
            name = "ROOT_OPT"
            type = "bool"
            default = true
            desc = "Root option"
        "#;
        fs::write(root_path.join("Kconfig.toml"), root_kconfig)?;

        let sub_path = root_path.join("sub");
        fs::create_dir(&sub_path)?;
        let sub_kconfig = r#"
            title = "Sub"
            [[config]]
            name = "SUB_OPT"
            type = "bool"
            default = false
            desc = "Sub option"
        "#;
        fs::write(sub_path.join("Kconfig.toml"), sub_kconfig)?;

        let tree = build_config_tree(root_path)?;

        assert_eq!(tree.desc, "Root");
        assert_eq!(tree.configs.len(), 1);
        assert_eq!(tree.configs[0].name, "ROOT_OPT");
        assert_eq!(tree.children.len(), 1);
        assert_eq!(tree.children[0].desc, "Sub");
        assert_eq!(tree.children[0].configs.len(), 1);
        assert_eq!(tree.children[0].configs[0].name, "SUB_OPT");

        Ok(())
    }
}
