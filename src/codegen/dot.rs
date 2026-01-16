use crate::graph::ConfigGraph;
use anyhow::Result;
use petgraph::dot::{Config, Dot};

pub fn generate(graph: &ConfigGraph) -> Result<String> {
    let output = format!(
        "{:?}",
        Dot::with_config(&graph.graph, &[Config::EdgeNoLabel])
    );
    Ok(output)
}
