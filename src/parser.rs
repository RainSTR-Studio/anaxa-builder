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
