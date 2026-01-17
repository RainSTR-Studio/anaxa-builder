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

#[cfg(test)]
mod tests {
    use super::*;
    use toml::Value as TomlValue;

    #[test]
    fn test_evaluator_basic_bool() -> Result<()> {
        let mut evaluator = Evaluator::new();
        evaluator.set_variable("A", &TomlValue::Boolean(true))?;
        evaluator.set_variable("B", &TomlValue::Boolean(false))?;

        assert!(evaluator.check_dependency("A")?);
        assert!(!evaluator.check_dependency("B")?);
        assert!(evaluator.check_dependency("A && !B")?);
        assert!(!evaluator.check_dependency("A && B")?);
        Ok(())
    }

    #[test]
    fn test_evaluator_integers() -> Result<()> {
        let mut evaluator = Evaluator::new();
        evaluator.set_variable("MAX", &TomlValue::Integer(10))?;
        evaluator.set_variable("MIN", &TomlValue::Integer(0))?;

        assert!(evaluator.check_dependency("MAX > MIN")?);
        assert!(evaluator.check_dependency("MAX == 10")?);
        assert!(evaluator.check_dependency("MAX")?);
        assert!(!evaluator.check_dependency("MIN")?);
        Ok(())
    }

    #[test]
    fn test_evaluator_strings() -> Result<()> {
        let mut evaluator = Evaluator::new();
        evaluator.set_variable("MODE", &TomlValue::String("PROD".to_string()))?;

        assert!(evaluator.check_dependency("MODE == \"PROD\"")?);
        assert!(!evaluator.check_dependency("MODE == \"DEV\"")?);
        Ok(())
    }

    #[test]
    fn test_evaluator_empty_expr() -> Result<()> {
        let evaluator = Evaluator::new();
        assert!(evaluator.check_dependency("")?);
        assert!(evaluator.check_dependency("  ")?);
        Ok(())
    }
}
