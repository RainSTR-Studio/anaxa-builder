use crate::schema::ConfigItem;
use anyhow::{Context, Result};
use evalexpr::{ContextWithMutableVariables, HashMapContext, Value};
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct Evaluator {
    context: HashMapContext,
}

impl Evaluator {
    pub fn new() -> Self {
        Self {
            context: HashMapContext::new(),
        }
    }

    pub fn set_variable(&mut self, name: &str, value: &toml::Value) -> Result<()> {
        let val = match value {
            toml::Value::Boolean(b) => Value::Boolean(*b),
            toml::Value::Integer(i) => Value::Int(*i),
            toml::Value::String(s) => Value::String(s.clone()),
            _ => return Ok(()),
        };
        self.context.set_value(name.to_string(), val)?;
        Ok(())
    }

    pub fn check_dependency(&self, expr: &str) -> Result<bool> {
        if expr.trim().is_empty() {
            return Ok(true);
        }

        let val = evalexpr::eval_with_context(expr, &self.context)
            .with_context(|| format!("Failed to evaluate expression: {}", expr))?;

        match val {
            Value::Boolean(b) => Ok(b),
            Value::Int(i) => Ok(i != 0),
            _ => Ok(false),
        }
    }
}

impl Default for Evaluator {
    fn default() -> Self {
        Self::new()
    }
}

pub fn collect_defaults(items: &[ConfigItem]) -> HashMap<String, toml::Value> {
    let mut map = HashMap::new();
    for item in items {
        if let Some(ref val) = item.default {
            map.insert(item.name.clone(), val.clone());
        }
    }
    map
}
