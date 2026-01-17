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

        for item in items {
            if let Some(ref dep) = item.depends_on {
                let vars = extract_variables(dep);
                for var in vars {
                    if let Some(&dependency) = item_map.get(var.as_str()) {
                        graph.add_edge(dependency, item.name.as_str(), ());
                    }
                }
            }
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

fn extract_variables(expr: &str) -> Vec<String> {
    expr.split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|s| !s.is_empty() && !s.chars().next().unwrap().is_numeric())
        .map(|s| s.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{ConfigItem, ConfigType};

    fn create_item(name: &str, depends_on: Option<&str>) -> ConfigItem {
        ConfigItem {
            name: name.to_string(),
            config_type: ConfigType::Bool,
            default: None,
            desc: name.to_string(),
            depends_on: depends_on.map(|s| s.to_string()),
            help: None,
            options: None,
            feature: None,
            range: None,
            regex: None,
        }
    }

    #[test]
    fn test_extract_variables() {
        let vars = extract_variables("A && (B || !C)");
        assert_eq!(vars, vec!["A", "B", "C"]);

        let vars = extract_variables("ENABLE_NET && MAX_SOCKETS > 10");
        assert_eq!(vars, vec!["ENABLE_NET", "MAX_SOCKETS"]);
    }

    #[test]
    fn test_graph_build_success() -> Result<()> {
        let items = vec![
            create_item("A", None),
            create_item("B", Some("A")),
            create_item("C", Some("B && A")),
        ];

        let graph = ConfigGraph::build(&items)?;
        assert_eq!(graph.graph.node_count(), 3);
        assert_eq!(graph.graph.edge_count(), 3);
        Ok(())
    }

    #[test]
    fn test_graph_cycle_detection() {
        let items = vec![create_item("A", Some("B")), create_item("B", Some("A"))];

        let result = ConfigGraph::build(&items);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Cycle detected"));
    }

    #[test]
    fn test_graph_self_cycle() {
        let items = vec![create_item("A", Some("A"))];

        let result = ConfigGraph::build(&items);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Self-dependency cycle"));
    }
}
