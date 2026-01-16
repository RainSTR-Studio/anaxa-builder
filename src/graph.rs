use crate::schema::ConfigItem;
use anyhow::{Result, anyhow};
use petgraph::algo::tarjan_scc;
use petgraph::graphmap::DiGraphMap;
use std::collections::HashMap;

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
