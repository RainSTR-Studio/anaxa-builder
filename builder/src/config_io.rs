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
    } else {
        // 生成默认配置文件
        for item in items {
            if item.default.is_some() {
                values.insert(item.name.clone(), item.default.clone().unwrap());
            }
        }
        save_config(path, &values)?;
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
