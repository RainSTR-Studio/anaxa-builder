use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ConfigType {
    Bool,
    Int,
    Hex,
    String,
    Choice,
}

impl ConfigType {
    pub fn format_value_c(&self, val: &toml::Value) -> Option<String> {
        match self {
            ConfigType::Bool => val
                .as_bool()
                .map(|b| if b { "1".into() } else { "0".into() }),
            ConfigType::Int => val.as_integer().map(|i| i.to_string()),
            ConfigType::Hex => val.as_integer().map(|i| format!("0x{:x}", i)),
            ConfigType::String | ConfigType::Choice => val.as_str().map(|s| format!("\"{}\"", s)),
        }
    }

    pub fn format_value_rust(&self, val: &toml::Value) -> Option<String> {
        match self {
            ConfigType::Bool => val.as_bool().map(|b| b.to_string()),
            ConfigType::Int => val.as_integer().map(|i| i.to_string()),
            ConfigType::Hex => val.as_integer().map(|i| format!("0x{:x}", i)),
            ConfigType::String | ConfigType::Choice => val.as_str().map(|s| format!("\"{}\"", s)),
        }
    }

    pub fn rust_type(&self) -> &'static str {
        match self {
            ConfigType::Bool => "bool",
            ConfigType::Int => "i64",
            ConfigType::Hex => "u64",
            ConfigType::String | ConfigType::Choice => "&str",
        }
    }
}

impl fmt::Display for ConfigType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigItem {
    pub name: String,
    #[serde(rename = "type")]
    pub config_type: ConfigType,
    pub default: Option<toml::Value>,
    pub desc: String,
    pub depends_on: Option<String>,
    pub help: Option<String>,
    pub options: Option<Vec<String>>,
    pub feature: Option<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use toml::Value;

    #[test]
    fn test_config_type_format_c() {
        assert_eq!(
            ConfigType::Bool.format_value_c(&Value::Boolean(true)),
            Some("1".to_string())
        );
        assert_eq!(
            ConfigType::Int.format_value_c(&Value::Integer(42)),
            Some("42".to_string())
        );
        assert_eq!(
            ConfigType::Hex.format_value_c(&Value::Integer(255)),
            Some("0xff".to_string())
        );
        assert_eq!(
            ConfigType::String.format_value_c(&Value::String("hi".to_string())),
            Some("\"hi\"".to_string())
        );
    }

    #[test]
    fn test_config_type_format_rust() {
        assert_eq!(
            ConfigType::Bool.format_value_rust(&Value::Boolean(true)),
            Some("true".to_string())
        );
        assert_eq!(
            ConfigType::Int.format_value_rust(&Value::Integer(42)),
            Some("42".to_string())
        );
        assert_eq!(
            ConfigType::String.format_value_rust(&Value::String("hi".to_string())),
            Some("\"hi\"".to_string())
        );
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Menu {
    pub title: String,
    pub desc: Option<String>,
    pub depends_on: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KconfigFile {
    pub title: Option<String>,
    pub depends_on: Option<String>,
    #[serde(rename = "config")]
    pub configs: Option<Vec<ConfigItem>>,
}

/// Represents a node in the configuration hierarchy
#[derive(Debug, Clone)]
pub struct ConfigNode {
    pub desc: String,
    pub configs: Vec<ConfigItem>,
    pub children: Vec<ConfigNode>,
    pub path: String,
    pub depends_on: Option<String>,
}
