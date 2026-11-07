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

/// 解析结果：配置树 + 参与解析的全部 `Kconfig.toml` 绝对路径。
///
/// `files` 供 `build.rs` 逐文件发射 `cargo:rerun-if-changed`：仅监听扫描根目录
/// 会漏掉 `[menu]` 通过 `../` 引用到扫描根之外的外部目录。
#[derive(Debug, Clone)]
pub struct ConfigTree {
    pub root: ConfigNode,
    pub files: Vec<PathBuf>,
}

/// Recursively scans the given root directory for `Kconfig.toml` files
/// and builds a hierarchical `ConfigNode` tree.
pub fn build_config_tree<P: AsRef<Path>>(root: P) -> Result<ConfigNode> {
    Ok(build_config_tree_detailed(root)?.root)
}

/// 同 [`build_config_tree`]，但额外返回参与解析的 `Kconfig.toml` 文件清单。
///
/// `[menu]` 指向的路径若不存在、或存在但没有 `Kconfig.toml`、或被多个父菜单
/// 同时声明，都会累积为本函数的错误，而不是静默丢失菜单项——静默跳过会让
/// "路径写错"表现为"整个菜单凭空消失"，用户无从排查。
pub fn build_config_tree_detailed<P: AsRef<Path>>(root: P) -> Result<ConfigTree> {
    let root_path = root.as_ref().canonicalize()?;
    let mut nodes: BTreeMap<PathBuf, ConfigNode> = BTreeMap::new();
    // Map from child path to explicit parent path
    let mut explicit_links: BTreeMap<PathBuf, PathBuf> = BTreeMap::new();
    // 参与解析的 Kconfig.toml，按发现顺序去重后返回给调用方做增量追踪
    let mut files: Vec<PathBuf> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    let mut queue = VecDeque::new();
    queue.push_back(root_path.clone());

    let mut scanned_roots = HashSet::new();

    while let Some(scan_root) = queue.pop_front() {
        if scanned_roots.iter().any(|r| scan_root.starts_with(r)) {
            continue;
        }
        scanned_roots.insert(scan_root.clone());

        for entry in WalkDir::new(&scan_root).follow_links(true).into_iter() {
            // 遍历失败不能静默丢弃：漏读一个目录等于漏掉整棵子树里的配置项
            let entry = match entry {
                Ok(entry) => entry,
                Err(e) => {
                    errors.push(format!("failed to scan {:?}: {}", scan_root.display(), e));
                    continue;
                }
            };

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
                    for (label, target_str) in menu {
                        let mut target_path = parent_dir.join(target_str);

                        if target_path.file_name() == Some(std::ffi::OsStr::new("Kconfig.toml")) {
                            target_path.pop();
                        }

                        let canon_target = match target_path.canonicalize() {
                            Ok(p) => p,
                            Err(_) => {
                                errors.push(format!(
                                    "[menu] entry '{}' in {:?} points to '{}', which does not exist",
                                    label, abs_path, target_str
                                ));
                                continue;
                            }
                        };

                        if !canon_target.join("Kconfig.toml").is_file() {
                            errors.push(format!(
                                "[menu] entry '{}' in {:?} points to '{}', which contains no Kconfig.toml",
                                label, abs_path, target_str
                            ));
                            continue;
                        }

                        // 已经被自动扫描收录的目录无需再入队：
                        // 重新扫描同一目录会让队列空转，而 nodes/findme 已有去重兜底
                        let already_scanned =
                            scanned_roots.iter().any(|r| canon_target.starts_with(r));
                        if !already_scanned {
                            queue.push_back(canon_target.clone());
                        }

                        let child_rel = diff_paths(&canon_target, &root_path);
                        if let Some(previous_parent) =
                            explicit_links.insert(child_rel.clone(), rel_path.clone())
                        {
                            // 同一对父子重复声明是冗余但无害的写法；只有跨父级的
                            // 重复挂载才是错误（会生成两条 import 路径）
                            if previous_parent != rel_path {
                                errors.push(format!(
                                    "[menu] entry '{}' in {:?} re-links {:?}, which is already mapped under {:?}",
                                    label, abs_path, child_rel, previous_parent
                                ));
                            }
                        }
                    }
                }

                let desc = kconfig
                    .title
                    .clone()
                    .unwrap_or_else(|| rel_path.to_string_lossy().into_owned());

                if !files.contains(&abs_path) {
                    files.push(abs_path.clone());
                }

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

    // 结构性错误必须先于树装配上报：装配失败时（如找不到根 Kconfig.toml）
    // 用户只会看到一系列连锁错误，而真正的原因（比如某个菜单项写错了路径）被淹没
    if !errors.is_empty() {
        anyhow::bail!(
            "Failed to build configuration tree:\n  - {}",
            errors.join("\n  - ")
        );
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

    let root = nodes
        .remove(&PathBuf::new())
        .context("No root Kconfig.toml found in the root directory")?;

    // 装配后仍留在 nodes 里的节点无法从根到达，其配置项永远不会出现在菜单或
    // 产物里。这同样属于"路径写错却无声无息"的一类，必须报出来。
    if !nodes.is_empty() {
        let orphans: Vec<String> = nodes.keys().map(|p| format!("{p:?}")).collect();
        anyhow::bail!(
            "Configuration nodes are unreachable from the root menu: {}",
            orphans.join(", ")
        );
    }

    Ok(ConfigTree { root, files })
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

    /// 写一个最小可用的 Kconfig.toml，`menu` 为空对象时即"没有子菜单"。
    fn write_kconfig(dir: &Path, extra: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(
            dir.join("Kconfig.toml"),
            format!("title = \"T\"\ndesc = \"{}\"\n{extra}\n", dir.display()),
        )
        .unwrap();
    }

    #[test]
    fn menu_entry_pointing_nowhere_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        write_kconfig(tmp.path(), "[menu]\n\"Ghost\" = \"missing-dir\"");
        let err = build_config_tree_detailed(tmp.path())
            .unwrap_err()
            .to_string();
        assert!(err.contains("does not exist"), "{err}");
    }

    #[test]
    fn menu_entry_without_kconfig_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        write_kconfig(tmp.path(), "[menu]\n\"Empty\" = \"empty-dir\"");
        std::fs::create_dir_all(tmp.path().join("empty-dir")).unwrap();
        let err = build_config_tree_detailed(tmp.path())
            .unwrap_err()
            .to_string();
        assert!(err.contains("contains no Kconfig.toml"), "{err}");
    }

    #[test]
    fn redundant_link_inside_scanned_subtree_is_accepted() {
        // 子目录本来就在扫描范围内时，再写一次 [menu] 只是显式声明，不应报错
        let tmp = tempfile::tempdir().unwrap();
        write_kconfig(tmp.path(), "[menu]\n\"Sub\" = \"sub\"");
        write_kconfig(&tmp.path().join("sub"), "");

        let tree = build_config_tree_detailed(tmp.path()).unwrap();
        assert_eq!(tree.root.children.len(), 1, "{}", tree.files.len());
    }

    #[test]
    fn menu_relinking_same_dir_under_two_parents_is_an_error() {
        // 同一目录被两个父级同时挂载会生成两条冲突的导入路径
        let tmp = tempfile::tempdir().unwrap();
        write_kconfig(
            tmp.path(),
            "[menu]\n\"A\" = \"a\"\n\"B\" = \"b\"\n\"Shared\" = \"shared\"",
        );
        write_kconfig(&tmp.path().join("shared"), "");
        write_kconfig(&tmp.path().join("a"), "[menu]\n\"Shared\" = \"../shared\"");
        write_kconfig(&tmp.path().join("b"), "");

        let err = build_config_tree_detailed(tmp.path())
            .unwrap_err()
            .to_string();
        assert!(err.contains("re-links"), "{err}");
    }

    #[test]
    fn reports_every_broken_menu_entry_at_once() {
        // 逐个报错会让用户改一处重跑一次，必须一次性列出全部问题
        let tmp = tempfile::tempdir().unwrap();
        write_kconfig(tmp.path(), "[menu]\n\"A\" = \"gone-a\"\n\"B\" = \"gone-b\"");
        let err = build_config_tree_detailed(tmp.path())
            .unwrap_err()
            .to_string();
        assert!(err.contains("gone-a") && err.contains("gone-b"), "{err}");
    }

    #[test]
    fn tree_lists_every_kconfig_file_for_rerun_tracking() {
        // build 脚本要靠这份清单告诉 cargo 哪些文件变化需要重新生成
        let tmp = tempfile::tempdir().unwrap();
        write_kconfig(tmp.path(), "[menu]\n\"Sub\" = \"nested/sub\"");
        let canonical_sub = tmp.path().join("nested").join("sub");
        write_kconfig(&canonical_sub, "");

        let tree = build_config_tree_detailed(tmp.path()).unwrap();
        assert_eq!(tree.files.len(), 2, "{:?}", tree.files);
        assert!(tree.files.iter().any(|p| p.starts_with(&canonical_sub)));
        // root 不能被重复计入
        let roots = tree
            .files
            .iter()
            .filter(|p| p.parent() == Some(tmp.path()))
            .count();
        assert_eq!(roots, 1, "{:?}", tree.files);
    }
}
