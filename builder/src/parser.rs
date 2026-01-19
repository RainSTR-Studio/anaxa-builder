use crate::schema::{ConfigItem, ConfigNode, ConfigType, KconfigFile};
use anyhow::{Context, Result};
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Calculate relative path from base to path (handling .. if needed)
fn diff_paths(path: &Path, base: &Path) -> PathBuf {
    let path_comps: Vec<_> = path.components().collect();
    let base_comps: Vec<_> = base.components().collect();

    let min_len = path_comps.len().min(base_comps.len());
    let mut common_len = 0;
    while common_len < min_len && path_comps[common_len] == base_comps[common_len] {
        common_len += 1;
    }

    let mut result = PathBuf::new();
    if common_len < base_comps.len() {
        for _ in 0..(base_comps.len() - common_len) {
            result.push("..");
        }
    }
    for i in common_len..path_comps.len() {
        result.push(path_comps[i].as_os_str());
    }

    result
}

/// Recursively scans the given root directory for `Kconfig.toml` files
/// and builds a hierarchical `ConfigNode` tree.
pub fn build_config_tree<P: AsRef<Path>>(root: P) -> Result<ConfigNode> {
    let root_path = root.as_ref().canonicalize()?;
    let mut nodes: BTreeMap<PathBuf, ConfigNode> = BTreeMap::new();
    // Map from child path to explicit parent path
    let mut explicit_links: BTreeMap<PathBuf, PathBuf> = BTreeMap::new();

    let mut queue = VecDeque::new();
    queue.push_back(root_path.clone());

    let mut scanned_roots = HashSet::new();

    while let Some(scan_root) = queue.pop_front() {
        if scanned_roots.iter().any(|r| scan_root.starts_with(r)) {
            continue;
        }
        scanned_roots.insert(scan_root.clone());

        for entry in WalkDir::new(&scan_root)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_name() == "Kconfig.toml" {
                let path = entry.path();
                let abs_path = path.canonicalize().unwrap_or(path.to_path_buf());
                let parent_dir = abs_path.parent().unwrap();

                let rel_path = diff_paths(parent_dir, &root_path);

                if nodes.contains_key(&rel_path) {
                    continue;
                }

                let content = fs::read_to_string(&abs_path)
                    .with_context(|| format!("Failed to read config file: {:?}", abs_path))?;

                let kconfig: KconfigFile = toml::from_str(&content).with_context(|| {
                    format!("Failed to parse TOML structure in: {:?}", abs_path)
                })?;

                if let Some(menu) = &kconfig.menu {
                    for (_key, target_str) in menu {
                        let mut target_path = parent_dir.join(target_str);

                        if target_path.file_name() == Some(std::ffi::OsStr::new("Kconfig.toml")) {
                            target_path.pop();
                        }

                        if let Ok(canon_target) = target_path.canonicalize() {
                            if !scanned_roots.iter().any(|r| canon_target.starts_with(r)) {
                                queue.push_back(canon_target.clone());
                            }

                            let child_rel = diff_paths(&canon_target, &root_path);
                            explicit_links.insert(child_rel, rel_path.clone());
                        }
                    }
                }

                let desc = kconfig
                    .title
                    .clone()
                    .unwrap_or_else(|| rel_path.to_string_lossy().into_owned());

                nodes.insert(
                    rel_path.clone(),
                    ConfigNode {
                        desc,
                        description: kconfig.desc.clone(),
                        help: kconfig.help.clone(),
                        configs: kconfig.configs.unwrap_or_default(),
                        children: Vec::new(),
                        path: rel_path.to_string_lossy().into_owned(),
                        depends_on: kconfig.depends_on.clone(),
                    },
                );
            }
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
        // Priority: Explicit link > Implicit directory parent
        let parent_path = if let Some(explicit_parent) = explicit_links.get(&path) {
            explicit_parent.clone()
        } else {
            let mut p = path.parent().unwrap_or(Path::new("")).to_path_buf();
            while !p.as_os_str().is_empty() && !nodes.contains_key(&p) {
                p = p.parent().unwrap_or(Path::new("")).to_path_buf();
            }
            p
        };

        if let Some(parent_node) = nodes.get_mut(&parent_path) {
            parent_node.children.push(current_node);
        } else if parent_path.as_os_str().is_empty() && nodes.contains_key(&PathBuf::new()) {
            // Fallback: attach to root if parent is empty and root exists
            nodes
                .get_mut(&PathBuf::new())
                .unwrap()
                .children
                .push(current_node);
        } else {
            // If no parent found (and not explicitly linked to a valid node),
            // put it back or keep it isolated (effectively invisible in this tree logic unless it IS root)
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

/// Validate configuration items for global uniqueness and reserved names
pub fn validate_configs(configs: &[ConfigItem]) -> Result<()> {
    let mut seen_names = HashSet::new();
    const RESERVED_NAMES: &[&str] = &["ANAXA_BUILDER_VERSION"];
    let mut errors = Vec::new();

    for item in configs {
        if RESERVED_NAMES.contains(&item.name.as_str()) {
            errors.push(format!(
                "Config name '{}' is reserved and cannot be used",
                item.name
            ));
        }

        if !seen_names.insert(item.name.clone()) {
            errors.push(format!("Duplicate config name found: '{}'", item.name));
        }

        if let Some(default_val) = &item.default {
            if let Err(e) = item.validate(default_val) {
                errors.push(format!(
                    "Invalid default value for config '{}': {}",
                    item.name, e
                ));
            }
        }

        if item.config_type == ConfigType::Choice
            && (item.options.is_none() || item.options.as_ref().map_or(true, |o| o.is_empty()))
        {
            errors.push(format!(
                "Config '{}' is a choice but has no options",
                item.name
            ));
        }
    }

    if !errors.is_empty() {
        anyhow::bail!(
            "Configuration validation failed:\n  - {}",
            errors.join("\n  - ")
        );
    }
    Ok(())
}

/// Legacy function for compatibility, if needed
pub fn parse_kconfigs<P: AsRef<Path>>(root: P) -> Result<Vec<ConfigItem>> {
    let tree = build_config_tree(root)?;
    Ok(flatten_configs(&tree))
}
