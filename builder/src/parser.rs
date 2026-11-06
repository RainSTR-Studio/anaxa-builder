use crate::graph::extract_variables;
use crate::schema::{ConfigItem, ConfigNode, ConfigType, KconfigFile};
use anyhow::{Context, Result};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
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
    for comp in path_comps.iter().skip(common_len) {
        result.push(comp.as_os_str());
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
                    for target_str in menu.values() {
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

/// Validate configuration items for global uniqueness and reserved names.
///
/// 除名称与默认值外，这里也是 `depends_on` 表达式的唯一静态校验入口：
/// 标识符是否合法、引用的符号是否存在、能否求值出布尔结果。
/// 这些检查必须在校验阶段完成，否则非法表达式会在 TUI 或 build 阶段
/// 被静默忽略（见 `App::is_visible_config` 的 `unwrap_or(true)`）。
pub fn validate_configs(configs: &[ConfigItem]) -> Result<()> {
    let mut seen_names = HashSet::new();
    const RESERVED_NAMES: &[&str] = &["ANAXA_BUILDER_VERSION"];
    let mut errors = Vec::new();

    // 收集所有已声明符号及其类型：依赖表达式既要查存在性，也要按类型构造样本值
    let declared: HashMap<&str, &ConfigType> = configs
        .iter()
        .map(|i| (i.name.as_str(), &i.config_type))
        .collect();

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
            && item.options.as_ref().is_none_or(|o| o.is_empty())
        {
            errors.push(format!(
                "Config '{}' is a choice but has no options",
                item.name
            ));
        }

        if let Some(expr) = &item.depends_on {
            for msg in check_dependency_expr(expr, &declared) {
                errors.push(format!(
                    "Invalid depends_on for config '{}': {}",
                    item.name, msg
                ));
            }
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

/// 校验单个依赖表达式，返回全部问题（而不是遇到第一个就返回）。
///
/// 检查顺序：标识符合法性 → 符号存在性 → 布尔可求值性。
fn check_dependency_expr(expr: &str, declared: &HashMap<&str, &ConfigType>) -> Vec<String> {
    if expr.trim().is_empty() {
        return Vec::new();
    }

    let mut errors = Vec::new();

    // 1. 标识符合法性：evalexpr 不会校验标识符字符集，会把 `MAX-SOCKETS`
    //    静默拆成 `MAX - SOCKETS` 三个 token，因此必须显式拦截。
    let written = extract_variables(expr);
    for name in &written {
        if !is_valid_identifier(name) {
            errors.push(format!(
                "'{}' is not a valid identifier (letters, digits and '_' only, cannot start with a digit)",
                name
            ));
        }
    }

    // 2. 符号存在性：未声明的符号在求值期必然报 unbound variable
    for name in &written {
        if is_valid_identifier(name) && !declared.contains_key(name.as_str()) {
            errors.push(format!(
                "unknown symbol '{}' (not declared as any [[config]] name)",
                name
            ));
        }
    }

    // 结构性错误已确定时不再做语法检查，避免同一问题重复报错
    if !errors.is_empty() {
        return errors;
    }

    // 3. 语法与布尔可求值性：按**声明类型**给每个符号绑定一个样本值后求值。
    //    统一绑布尔会让 `MAX_SOCKETS > 2` 这类合法表达式误报类型错误；
    //    用样本值还能同时抓出 `A &&`、`1 == 1` 这类“合法但结果非布尔”的表达式。
    use evalexpr::{ContextWithMutableVariables, HashMapContext};
    let mut ctx: HashMapContext = HashMapContext::new();
    for name in &written {
        let sample = sample_value(name, declared);
        let _ = ctx.set_value(name.clone(), sample);
    }
    match evalexpr::eval_with_context(expr, &ctx) {
        Ok(evalexpr::Value::Boolean(_)) => {}
        Ok(other) => errors.push(format!(
            "expression does not evaluate to a boolean (got {:?})",
            other
        )),
        Err(e) => errors.push(format!("syntax error: {}", e)),
    }

    errors
}

/// 为静态校验构造一个符合声明类型的样本值。
///
/// 具体取值无所谓（先 `true` 后 `1`），关键是**类型正确**，
/// 否则 `MAX_SOCKETS > 2` 这类跨类型比较会在校验阶段被误判为语法错误。
fn sample_value(name: &str, declared: &HashMap<&str, &ConfigType>) -> evalexpr::Value {
    match declared.get(name) {
        Some(ConfigType::Bool) => evalexpr::Value::Boolean(true),
        Some(ConfigType::Int) | Some(ConfigType::Hex) => evalexpr::Value::Int(1),
        // 未声明的符号已在前面报错，这里给个中性值避免二次报错
        _ => evalexpr::Value::String(String::new()),
    }
}

/// 依赖表达式中的标识符必须是 ASCII 字母/下划线开头，后接字母、数字或下划线。
///
/// 该规则与 `evalexpr` 的词法切分保持兼容：表达式里的 `-` 只会被当作减号，
/// 因此 `MAX-SOCKETS` 这类写法必须被拒绝，而不是被拆成两个符号。
fn is_valid_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Legacy function for compatibility, if needed
pub fn parse_kconfigs<P: AsRef<Path>>(root: P) -> Result<Vec<ConfigItem>> {
    let tree = build_config_tree(root)?;
    Ok(flatten_configs(&tree))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bool_item(name: &str, dep: Option<&str>) -> ConfigItem {
        ConfigItem {
            name: name.into(),
            config_type: ConfigType::Bool,
            default: Some(toml::Value::Boolean(false)),
            desc: String::new(),
            depends_on: dep.map(|s| s.to_string()),
            help: None,
            options: None,
            feature: None,
            range: None,
            regex: None,
            rust_type: None,
        }
    }

    #[test]
    fn accepts_well_formed_expressions() {
        let configs = vec![
            bool_item("A", None),
            bool_item("B", None),
            bool_item("C", Some("A && B")),
            bool_item("D", Some("A || !B")),
            bool_item("E", Some("A")),
            bool_item("F", Some("  ")),
        ];
        assert!(validate_configs(&configs).is_ok());
    }

    #[test]
    fn rejects_unknown_symbols() {
        let configs = vec![bool_item("A", Some("A && GHOST"))];
        let err = validate_configs(&configs).unwrap_err().to_string();
        assert!(err.contains("unknown symbol 'GHOST'"), "{err}");
    }

    #[test]
    fn rejects_invalid_identifier_characters() {
        // evalexpr 把 `MAX-SOCKETS > 2` 拆成 `MAX - SOCKETS > 2`，随后把 MAX
        // 当成变量名；若无人拦截，用户只会看到一个莫名其妙的 unbound variable。
        let configs = vec![bool_item("A", Some("MAX-SOCKETS > 2"))];
        let err = validate_configs(&configs).unwrap_err().to_string();
        assert!(err.contains("unknown symbol 'MAX'"), "{err}");

        let configs = vec![bool_item("A", Some("MY.VAR"))];
        let err = validate_configs(&configs).unwrap_err().to_string();
        assert!(err.contains("unknown symbol"), "{err}");
    }

    #[test]
    fn rejects_digits_leading_symbol_name() {
        // `2X` 以数字开头，extract_variables 会把它当字面量丢弃，
        // 因此它不会出现在未知符号列表里，也不会误报为标识符错误。
        let configs = vec![bool_item("A", Some("2X"))];
        let err = validate_configs(&configs).unwrap_err().to_string();
        assert!(err.contains("syntax error"), "{err}");
    }

    #[test]
    fn rejects_syntax_errors() {
        let configs = vec![bool_item("A", None), bool_item("B", Some("A &&"))];
        let err = validate_configs(&configs).unwrap_err().to_string();
        assert!(err.contains("syntax error"), "{err}");
    }

    #[test]
    fn rejects_non_boolean_expression() {
        let configs = vec![
            bool_item("A", None),
            bool_item("B", None),
            bool_item("C", Some("A == B")),
        ];
        // A == B 在全部绑为 true 时合法且为布尔，应当通过
        assert!(validate_configs(&configs).is_ok());

        // 数值表达式不是布尔依赖
        let configs = vec![bool_item("A", Some("1 + 1"))];
        let err = validate_configs(&configs).unwrap_err().to_string();
        assert!(err.contains("does not evaluate to a boolean"), "{err}");
    }

    #[test]
    fn reports_all_problems_not_just_the_first() {
        let configs = vec![
            bool_item("A", Some("GHOST_ONE")),
            bool_item("B", Some("GHOST_TWO")),
        ];
        let err = validate_configs(&configs).unwrap_err().to_string();
        assert!(
            err.contains("GHOST_ONE") && err.contains("GHOST_TWO"),
            "{err}"
        );
    }

    #[test]
    fn mixed_type_expressions_are_accepted() {
        // 整型配置参与比较是合法用法，静态校验必须按声明类型绑定样本值
        let configs = vec![
            bool_item("ENABLE_NET", None),
            ConfigItem {
                name: "MAX_SOCKETS".into(),
                config_type: ConfigType::Int,
                default: Some(toml::Value::Integer(8)),
                desc: String::new(),
                depends_on: None,
                help: None,
                options: None,
                feature: None,
                range: None,
                regex: None,
                rust_type: None,
            },
            bool_item("FANCY", Some("ENABLE_NET && MAX_SOCKETS > 2")),
        ];
        assert!(validate_configs(&configs).is_ok());
    }

    #[test]
    fn identifier_rules() {
        assert!(is_valid_identifier("ENABLE_NET"));
        assert!(is_valid_identifier("_private"));
        assert!(is_valid_identifier("A1"));
        assert!(!is_valid_identifier("1A"));
        assert!(!is_valid_identifier("A-B"));
        assert!(!is_valid_identifier(""));
    }
}
