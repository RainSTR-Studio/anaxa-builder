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
    Cstr,
}

impl ConfigType {
    pub fn format_value_c(&self, val: &toml::Value) -> Option<String> {
        match self {
            ConfigType::Bool => val
                .as_bool()
                .map(|b| if b { "1".into() } else { "0".into() }),
            ConfigType::Int => val.as_integer().map(|i| i.to_string()),
            ConfigType::Hex => val.as_integer().map(|i| format!("0x{:x}", i)),
            ConfigType::String | ConfigType::Choice | ConfigType::Cstr => {
                val.as_str().map(|s| format!("\"{}\"", escape_c_string(s)))
            }
        }
    }

    pub fn format_value_rust(&self, val: &toml::Value) -> Option<String> {
        match self {
            ConfigType::Bool => val.as_bool().map(|b| b.to_string()),
            ConfigType::Int => val.as_integer().map(|i| i.to_string()),
            ConfigType::Hex => val.as_integer().map(|i| format!("0x{:x}", i)),
            ConfigType::String | ConfigType::Choice => val
                .as_str()
                .map(|s| format!("\"{}\"", escape_rust_string(s))),
            ConfigType::Cstr => val.as_str().map(|s| {
                // escape_cstr_bytes 已追加 `\0`，这里不能再写一次，否则产生双重 NUL
                format!(
                    "unsafe {{ ::core::ffi::CStr::from_bytes_with_nul_unchecked(b\"{}\") }}",
                    escape_cstr_bytes(s)
                )
            }),
        }
    }

    pub fn rust_type(&self) -> &'static str {
        match self {
            ConfigType::Bool => "bool",
            ConfigType::Int => "i64",
            ConfigType::Hex => "u64",
            ConfigType::String | ConfigType::Choice => "&str",
            ConfigType::Cstr => "&'static ::core::ffi::CStr",
        }
    }
}

impl fmt::Display for ConfigType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// 按 C 字符串字面量规则转义（不含两侧引号）。
///
/// 只转义会破坏字面量或改变语义的字符：`\`、`"`、换行/回车/制表与其余控制字符。
/// 非 ASCII 字符保留原样，由消费方头文件的编码（UTF-8）决定。
pub(crate) fn escape_c_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 || c == '\u{7f}' => {
                out.push_str(&format!("\\x{:02x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

/// 按 Rust `str` 字面量规则转义（不含两侧引号）。
///
/// 除控制字符与引号/反斜杠外，还需要转义 `{`、`}`，避免生成的常量被后续
/// 的 `format!`/`println!` 模板误解析；其余控制符统一用 `\u{...}` 形式。
pub(crate) fn escape_rust_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '{' => out.push_str("\\u{7b}"),
            '}' => out.push_str("\\u{7d}"),
            c if (c as u32) < 0x20 || c == '\u{7f}' => {
                out.push_str(&format!("\\u{{{:x}}}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

/// 按 Rust 字节串 `b"..."` 规则转义（不含两侧引号），并追加 C 字符串结尾的 `\0`。
///
/// 输入被视为 UTF-8 文本，逐字符判断：可打印 ASCII 直接输出，
/// 非 ASCII 字符按 UTF-8 字节转义为 `\xNN`，保证字节串内容与原文完全一致。
pub(crate) fn escape_cstr_bytes(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_ascii() && ((c as u32) < 0x20 || c == '\u{7f}') => {
                out.push_str(&format!("\\x{:02x}", c as u32));
            }
            c if c.is_ascii() => out.push(c),
            c => {
                let mut buf = [0u8; 4];
                for b in c.encode_utf8(&mut buf).as_bytes() {
                    out.push_str(&format!("\\x{:02x}", b));
                }
            }
        }
    }
    out.push_str("\\0");
    out
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
    pub range: Option<(i64, i64)>,
    pub regex: Option<String>,
    pub rust_type: Option<String>,
}

impl ConfigItem {
    pub fn validate(&self, value: &toml::Value) -> Result<(), String> {
        match self.config_type {
            ConfigType::Bool => {
                if !value.is_bool() {
                    return Err(format!(
                        "Config '{}' expected bool, found {:?}",
                        self.name, value
                    ));
                }
            }
            ConfigType::Int | ConfigType::Hex => {
                let val = value.as_integer().ok_or_else(|| {
                    format!("Config '{}' expected integer, found {:?}", self.name, value)
                })?;
                if let Some((min, max)) = self.range {
                    if val < min || val > max {
                        return Err(format!(
                            "Config '{}' value {} out of range [{}, {}]",
                            self.name, val, min, max
                        ));
                    }
                }
                if let Some(rt) = &self.rust_type {
                    match rt.as_str() {
                        "u8" => {
                            if val < 0 || val > u8::MAX as i64 {
                                return Err(format!(
                                    "Config '{}' value {} exceeds u8 range",
                                    self.name, val
                                ));
                            }
                        }
                        "i8" => {
                            if val < i8::MIN as i64 || val > i8::MAX as i64 {
                                return Err(format!(
                                    "Config '{}' value {} exceeds i8 range",
                                    self.name, val
                                ));
                            }
                        }
                        "u16" => {
                            if val < 0 || val > u16::MAX as i64 {
                                return Err(format!(
                                    "Config '{}' value {} exceeds u16 range",
                                    self.name, val
                                ));
                            }
                        }
                        "i16" => {
                            if val < i16::MIN as i64 || val > i16::MAX as i64 {
                                return Err(format!(
                                    "Config '{}' value {} exceeds i16 range",
                                    self.name, val
                                ));
                            }
                        }
                        "u32" => {
                            if val < 0 || val > u32::MAX as i64 {
                                return Err(format!(
                                    "Config '{}' value {} exceeds u32 range",
                                    self.name, val
                                ));
                            }
                        }
                        "i32" => {
                            if val < i32::MIN as i64 || val > i32::MAX as i64 {
                                return Err(format!(
                                    "Config '{}' value {} exceeds i32 range",
                                    self.name, val
                                ));
                            }
                        }
                        // u64/usize 都要求非负，共用同一检查（用 guard 避免嵌套 if）
                        "u64" | "usize" if val < 0 => {
                            return Err(format!(
                                "Config '{}' value {} cannot be negative for {}",
                                self.name, val, rt
                            ));
                        }
                        "u64" | "usize" => {}
                        _ => {}
                    }
                }
            }
            ConfigType::String | ConfigType::Cstr => {
                let val = value.as_str().ok_or_else(|| {
                    format!("Config '{}' expected string, found {:?}", self.name, value)
                })?;
                if let Some(regex_str) = &self.regex {
                    let re = regex::Regex::new(regex_str)
                        .map_err(|e| format!("Invalid regex for config '{}': {}", self.name, e))?;
                    if !re.is_match(val) {
                        return Err(format!(
                            "Config '{}' value \"{}\" does not match regex / {} /",
                            self.name, val, regex_str
                        ));
                    }
                }
            }
            ConfigType::Choice => {
                let val = value.as_str().ok_or_else(|| {
                    format!(
                        "Config '{}' expected string (choice), found {:?}",
                        self.name, value
                    )
                })?;
                if let Some(options) = &self.options {
                    if !options.contains(&val.to_string()) {
                        return Err(format!(
                            "Config '{}' value \"{}\" is not a valid option. Valid options are: {:?}",
                            self.name, val, options
                        ));
                    }
                }
            }
        }
        Ok(())
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
    pub desc: Option<String>,
    pub help: Option<String>,
    pub depends_on: Option<String>,
    #[serde(rename = "config")]
    pub configs: Option<Vec<ConfigItem>>,
    pub menu: Option<std::collections::BTreeMap<String, String>>,
}

/// Represents a node in the configuration hierarchy
#[derive(Debug, Clone)]
pub struct ConfigNode {
    pub desc: String,
    pub description: Option<String>,
    pub help: Option<String>,
    pub configs: Vec<ConfigItem>,
    pub children: Vec<ConfigNode>,
    pub path: String,
    pub depends_on: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use toml::Value;

    fn item(t: ConfigType, default: Value) -> ConfigItem {
        ConfigItem {
            name: "X".into(),
            config_type: t,
            default: Some(default),
            desc: String::new(),
            depends_on: None,
            help: None,
            options: None,
            feature: None,
            range: None,
            regex: None,
            rust_type: None,
        }
    }

    #[test]
    fn c_string_escapes_quotes_and_backslashes() {
        assert_eq!(escape_c_string(r#"a"b\c"#), r#"a\"b\\c"#);
        assert_eq!(escape_c_string("line\n\r\t"), "line\\n\\r\\t");
        assert_eq!(escape_c_string("\u{1}"), "\\x01");
    }

    #[test]
    fn rust_string_escapes_braces_and_controls() {
        assert_eq!(escape_rust_string("{fmt}"), "\\u{7b}fmt\\u{7d}");
        assert_eq!(escape_rust_string("a\"b"), "a\\\"b");
        assert_eq!(escape_rust_string("\u{1b}"), "\\u{1b}");
    }

    #[test]
    fn cstr_bytes_escapes_non_ascii_as_utf8_and_appends_nul() {
        // 'é' 的 UTF-8 编码是 0xC3 0xA9
        assert_eq!(escape_cstr_bytes("é"), "\\xc3\\xa9\\0");
        assert_eq!(escape_cstr_bytes("a"), "a\\0");
    }

    #[test]
    fn formatted_values_are_syntactically_valid() {
        // 不做转义时会生成非法代码: #define CONFIG_X "he said "hi""
        let raw = Value::String(r#"he said "hi"#.into());
        let it = item(ConfigType::String, raw.clone());
        assert_eq!(
            it.config_type.format_value_c(&raw).unwrap(),
            r#""he said \"hi""#
        );
        assert_eq!(
            it.config_type.format_value_rust(&raw).unwrap(),
            r#""he said \"hi""#
        );
    }

    #[test]
    fn cstr_formatting_has_exactly_one_nul() {
        let raw = Value::String("anaxa".into());
        let it = item(ConfigType::Cstr, raw.clone());
        let generated = it.config_type.format_value_rust(&raw).unwrap();
        assert!(generated.contains(r#"(b"anaxa\0")"#), "{generated}");
        assert!(!generated.contains(r"\0\0"), "{generated}");
    }

    #[test]
    fn bool_and_int_formatting_unchanged() {
        let b = item(ConfigType::Bool, Value::Boolean(true));
        assert_eq!(
            b.config_type
                .format_value_c(&Value::Boolean(true))
                .as_deref(),
            Some("1")
        );
        let i = item(ConfigType::Int, Value::Integer(42));
        assert_eq!(
            i.config_type
                .format_value_rust(&Value::Integer(42))
                .as_deref(),
            Some("42")
        );
    }

    #[test]
    fn validate_rejects_wrong_types_and_out_of_range() {
        let mut it = item(ConfigType::Int, Value::Integer(1));
        assert!(it.validate(&Value::Integer(1)).is_ok());
        assert!(it.validate(&Value::Boolean(true)).is_err());

        it.range = Some((1, 10));
        assert!(it.validate(&Value::Integer(11)).is_err());
        assert!(it.validate(&Value::Integer(10)).is_ok());

        it.rust_type = Some("u8".into());
        assert!(it.validate(&Value::Integer(-1)).is_err());
    }

    #[test]
    fn validate_regex_and_choice() {
        let mut it = item(ConfigType::String, Value::String("abc".into()));
        it.regex = Some("^[a-z]+$".into());
        assert!(it.validate(&Value::String("abc".into())).is_ok());
        assert!(it.validate(&Value::String("A1".into())).is_err());

        let mut ch = item(ConfigType::Choice, Value::String("RR".into()));
        ch.options = Some(vec!["RR".into(), "FIFO".into()]);
        assert!(ch.validate(&Value::String("RR".into())).is_ok());
        assert!(ch.validate(&Value::String("CFS".into())).is_err());
    }
}
