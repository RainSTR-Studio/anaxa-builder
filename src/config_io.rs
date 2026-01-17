use crate::evaluator;
use crate::schema::ConfigItem;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use toml::{Table, Value};

pub fn load_config(path: &Path, items: &[ConfigItem]) -> Result<HashMap<String, Value>> {
    let mut values = evaluator::collect_defaults(items);

    if path.exists() {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {:?}", path))?;

        let parsed: Table = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {:?}", path))?;

        for (key, val) in parsed {
            if let Some(item) = items.iter().find(|i| i.name == key) {
                if let Err(e) = item.validate(&val) {
                    eprintln!("Warning: {}", e);
                    continue;
                }
                values.insert(key, val);
            }
        }
    }

    Ok(values)
}

pub fn save_config(path: &Path, values: &HashMap<String, Value>) -> Result<()> {
    let mut table = Table::new();

    for (k, v) in values {
        table.insert(k.clone(), v.clone());
    }

    let content = toml::to_string_pretty(&table)?;
    fs::write(path, content).with_context(|| format!("Failed to write config file: {:?}", path))?;

    Ok(())
}

pub fn get_minimal_config(
    current_values: &HashMap<String, Value>,
    items: &[ConfigItem],
) -> HashMap<String, Value> {
    let defaults = evaluator::collect_defaults(items);
    let mut minimal = HashMap::new();

    for (name, value) in current_values {
        if let Some(default_val) = defaults.get(name) {
            if value != default_val {
                minimal.insert(name.clone(), value.clone());
            }
        } else {
            minimal.insert(name.clone(), value.clone());
        }
    }

    minimal
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{ConfigItem, ConfigType};
    use toml::Value;

    #[test]
    fn test_get_minimal_config() {
        let items = vec![
            ConfigItem {
                name: "A".to_string(),
                config_type: ConfigType::Bool,
                default: Some(Value::Boolean(true)),
                desc: "A".to_string(),
                depends_on: None,
                help: None,
                options: None,
                feature: None,
                range: None,
                regex: None,
            },
            ConfigItem {
                name: "B".to_string(),
                config_type: ConfigType::Int,
                default: Some(Value::Integer(10)),
                desc: "B".to_string(),
                depends_on: None,
                help: None,
                options: None,
                feature: None,
                range: None,
                regex: None,
            },
        ];

        let mut current = HashMap::new();
        current.insert("A".to_string(), Value::Boolean(false));
        current.insert("B".to_string(), Value::Integer(10));

        let minimal = get_minimal_config(&current, &items);

        assert_eq!(minimal.len(), 1);
        assert_eq!(minimal.get("A"), Some(&Value::Boolean(false)));
        assert_eq!(minimal.get("B"), None);
    }

    #[test]
    fn test_load_save_config() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let config_path = dir.path().join(".config");

        let items = vec![ConfigItem {
            name: "ENABLE_A".to_string(),
            config_type: ConfigType::Bool,
            default: Some(Value::Boolean(true)),
            desc: "A".to_string(),
            depends_on: None,
            help: None,
            options: None,
            feature: None,
            range: None,
            regex: None,
        }];

        let mut values = HashMap::new();
        values.insert("ENABLE_A".to_string(), Value::Boolean(false));

        save_config(&config_path, &values)?;
        let loaded = load_config(&config_path, &items)?;

        assert_eq!(loaded.get("ENABLE_A"), Some(&Value::Boolean(false)));
        Ok(())
    }
}
