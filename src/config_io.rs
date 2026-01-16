use crate::logic;
use crate::schema::ConfigItem;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use toml::{Table, Value};

pub fn load_config(path: &Path, items: &[ConfigItem]) -> Result<HashMap<String, Value>> {
    let mut values = logic::collect_defaults(items);

    if path.exists() {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {:?}", path))?;

        let parsed: Table = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {:?}", path))?;

        for (key, val) in parsed {
            if items.iter().any(|i| i.name == key) {
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
