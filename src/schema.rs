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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Menu {
    pub title: String,
    pub visible_if: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KconfigFile {
    pub menu: Option<Menu>,
    #[serde(rename = "config")]
    pub configs: Option<Vec<ConfigItem>>,
}
