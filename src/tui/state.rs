use crate::logic::Evaluator;
use crate::schema::ConfigItem;
use std::collections::HashMap;
use toml::Value;

#[derive(Clone)]
pub struct AppState {
    pub items: Vec<ConfigItem>,
    pub values: HashMap<String, Value>,
    pub evaluator: Evaluator,
}

impl AppState {
    pub fn new(items: Vec<ConfigItem>, values: HashMap<String, Value>) -> Self {
        let mut evaluator = Evaluator::new();
        for (k, v) in &values {
            let _ = evaluator.set_variable(k, v);
        }

        Self {
            items,
            values,
            evaluator,
        }
    }

    pub fn update_value(&mut self, name: &str, value: Value) {
        self.values.insert(name.to_string(), value.clone());
        let _ = self.evaluator.set_variable(name, &value);
    }

    pub fn is_visible(&self, item: &ConfigItem) -> bool {
        if let Some(ref dep) = item.depends_on {
            self.evaluator.check_dependency(dep).unwrap_or(false)
        } else {
            true
        }
    }
}
