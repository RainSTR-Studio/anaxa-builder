use crate::schema::ConfigItem;
use anyhow::{anyhow, Result};
use petgraph::algo::tarjan_scc;
use petgraph::graphmap::DiGraphMap;
use std::collections::HashMap;

#[derive(Debug)]
pub struct ConfigGraph<'a> {
    pub graph: DiGraphMap<&'a str, ()>,
}

impl<'a> ConfigGraph<'a> {
    pub fn build(items: &'a [ConfigItem]) -> Result<Self> {
        let mut graph = DiGraphMap::new();
        let item_map: HashMap<&str, &str> = items
            .iter()
            .map(|i| (i.name.as_str(), i.name.as_str()))
            .collect();

        for item in items {
            graph.add_node(item.name.as_str());
        }

        // 未定义的符号必须报错：静默跳过会让"依赖写错"表现为"依赖恒成立"，
        // 而依赖图正是环检测与 DOT 输出的唯一依据。
        let mut undefined = Vec::new();
        for item in items {
            if let Some(ref dep) = item.depends_on {
                for var in extract_variables(dep) {
                    match item_map.get(var.as_str()) {
                        Some(&dependency) => {
                            graph.add_edge(dependency, item.name.as_str(), ());
                        }
                        None => undefined.push(format!(
                            "config '{}' depends on undefined symbol '{}'",
                            item.name, var
                        )),
                    }
                }
            }
        }
        if !undefined.is_empty() {
            return Err(anyhow!(
                "Invalid configuration dependencies:\n  - {}",
                undefined.join("\n  - ")
            ));
        }

        let sccs = tarjan_scc(&graph);
        for scc in sccs {
            if scc.len() > 1 {
                return Err(anyhow!(
                    "Cycle detected in configuration dependencies: {:?}",
                    scc
                ));
            }
            if scc.len() == 1 {
                let node = scc[0];
                if graph.contains_edge(node, node) {
                    return Err(anyhow!("Self-dependency cycle detected: {}", node));
                }
            }
        }

        Ok(Self { graph })
    }
}

/// 从依赖表达式中提取被引用的符号名。
///
/// 依据 `evalexpr` 的词法边界切分：非字母数字下划线的字符（运算符、括号、
/// 空白）都视为分隔符，数字开头的片段是字面量而非符号。
/// 该函数只负责"找出像符号的片段"，不判断其是否真的合法，
/// 合法性由 `parser::check_dependency_expr` 负责。
pub(crate) fn extract_variables(expr: &str) -> Vec<String> {
    expr.split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|s| !s.is_empty() && !s.chars().next().unwrap().is_numeric())
        .map(|s| s.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::ConfigType;

    fn item(name: &str, dep: Option<&str>) -> ConfigItem {
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
    fn extract_variables_ignores_numbers_and_operators() {
        assert_eq!(
            extract_variables("ENABLE_NET && !IPV6_DISABLE"),
            vec!["ENABLE_NET".to_string(), "IPV6_DISABLE".to_string()]
        );
        // 3 是数字字面量，不应被当作符号；MAX_SOCKETS 必须整体取出
        assert_eq!(
            extract_variables("MAX_SOCKETS > 3"),
            vec!["MAX_SOCKETS".to_string()]
        );
    }

    #[test]
    fn extracts_identifier_lookalikes_for_reporting() {
        let found = extract_variables("MAX-SOCKETS");
        assert_eq!(found, vec!["MAX".to_string(), "SOCKETS".to_string()]);
    }

    #[test]
    fn detects_cycles_and_self_dependency() {
        let items = vec![item("A", Some("B")), item("B", Some("A"))];
        let err = ConfigGraph::build(&items).unwrap_err().to_string();
        assert!(err.contains("Cycle detected"), "{err}");

        let items = vec![item("A", Some("A"))];
        let err = ConfigGraph::build(&items).unwrap_err().to_string();
        assert!(err.contains("Self-dependency"), "{err}");
    }

    #[test]
    fn undefined_symbols_are_rejected() {
        let items = vec![item("A", Some("GHOST"))];
        let err = ConfigGraph::build(&items).unwrap_err().to_string();
        assert!(err.contains("undefined symbol 'GHOST'"), "{err}");
    }

    #[test]
    fn valid_dependencies_build_edges() {
        let items = vec![item("A", None), item("B", Some("A"))];
        let graph = ConfigGraph::build(&items).unwrap();
        assert!(graph.graph.contains_edge("A", "B"));
        assert!(!graph.graph.contains_edge("B", "A"));
    }
}
