use crate::evaluator;
use crate::schema::{ConfigItem, ConfigType};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use toml::{Table, Value};

/// 读取配置文件，缺失的条目回落到 schema 默认值。
///
/// 本函数**没有副作用**：文件不存在时不会创建它，也不会写盘。
/// 需要拿到"可以直接落盘的初始配置"的调用方应显式调用 [`ensure_config`]。
///
/// 文件中存在但类型不合法的条目会被跳过（使用默认值），并把告警写入 `warnings`，
/// 由调用方决定如何呈现，避免库代码直接往 stderr 打印。
pub fn load_config(
    path: &Path,
    items: &[ConfigItem],
    warnings: &mut Vec<String>,
) -> Result<HashMap<String, Value>> {
    let mut values = evaluator::collect_defaults(items);

    if path.exists() {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {path:?}"))?;

        let parsed: Table = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {path:?}"))?;

        merge_file_values(parsed, items, &mut values, warnings);
    }

    // 环境变量优先级最高：即使 .config 不存在也应生效
    apply_env_overrides(items, &mut values)?;

    Ok(values)
}

/// 读取配置并求出"实际生效"的值：`depends_on` 不满足的项按隐含值回落。
///
/// 与 [`load_config`] 的区别只在最后一步——凡是会把配置写进产物的调用方
/// （代码生成、`cargo` 包装器、`savedefconfig`）都应该用这个函数，
/// 否则"父项已关闭、子项仍出现在头文件里"的矛盾会一直存在。
///
/// TUI 内部编辑用 [`load_config`]：它需要保留原始值，这样把父项重新打开时
/// 子项能恢复成用户上次的选择。
pub fn load_effective_config(
    path: &Path,
    items: &[ConfigItem],
    warnings: &mut Vec<String>,
) -> Result<HashMap<String, Value>> {
    let values = load_config(path, items, warnings)?;
    evaluator::effective_values(items, &values)
}

/// 把已经解析出来的 `.config` 键值并入 `values`。
///
/// 未知键与类型非法键不会中断加载：前者忽略，后者回落到默认值，
/// 两者都记录到 `warnings` 供调用方展示。
fn merge_file_values(
    parsed: Table,
    items: &[ConfigItem],
    values: &mut HashMap<String, Value>,
    warnings: &mut Vec<String>,
) {
    for (key, val) in parsed {
        let Some(item) = items.iter().find(|i| i.name == key) else {
            warnings.push(format!("Unknown config key '{key}' ignored"));
            continue;
        };
        if let Err(e) = item.validate(&val) {
            warnings.push(format!("{e}; falling back to default"));
            continue;
        }
        values.insert(key, val);
    }
}

/// 只读取文件中显式写出的合法键值，不掺入任何默认值，也不读取环境变量。
///
/// defconfig 类文件就是"差异集"：这里的返回值恰好代表用户显式声明的条目。
pub fn load_file_values(path: &Path, items: &[ConfigItem]) -> Result<HashMap<String, Value>> {
    let mut explicit = HashMap::new();
    if !path.exists() {
        return Ok(explicit);
    }

    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read config file: {path:?}"))?;
    let parsed: Table = toml::from_str(&content)
        .with_context(|| format!("Failed to parse config file: {path:?}"))?;

    for (key, val) in parsed {
        let Some(item) = items.iter().find(|i| i.name == key) else {
            eprintln!("Warning: unknown config key '{key}' ignored");
            continue;
        };
        if let Err(e) = item.validate(&val) {
            eprintln!("Warning: {e}; falling back to default");
            continue;
        }
        explicit.insert(key, val);
    }

    Ok(explicit)
}

/// 确保配置文件存在：不存在时按 schema 默认值创建，返回是否新建。
///
/// 这是唯一会"隐式产生文件"的入口，且必须由命令显式调用，
/// 而不是藏在读取逻辑里。
pub fn ensure_config(path: &Path, items: &[ConfigItem]) -> Result<bool> {
    if path.exists() {
        return Ok(false);
    }
    let values = evaluator::collect_defaults(items);
    save_config(path, &values)?;
    Ok(true)
}

/// 按键名字典序写出配置，保证同一份配置在任何平台上产生相同字节。
///
/// `HashMap` 的迭代顺序不稳定，直接遍历会让 `.config` 每次都产生无意义的 diff。
pub fn save_config(path: &Path, values: &HashMap<String, Value>) -> Result<()> {
    let mut keys: Vec<&String> = values.keys().collect();
    keys.sort();

    let mut table = Table::new();
    for k in keys {
        table.insert(k.clone(), values[k].clone());
    }

    let content = toml::to_string_pretty(&table)?;
    fs::write(path, content).with_context(|| format!("Failed to write config file: {path:?}"))?;

    Ok(())
}

/// 从 `name` 环境变量读取覆盖值并写入 `values`，成功覆盖时返回 `true`。
///
/// 覆盖值按 schema 声明的类型解析：布尔接受 `1/0/true/false`，整数接受十进制
/// 或 `0x` 前缀（与 `Hex` 一致），字符串类原样使用。解析失败视为配置错误，
/// 因为用户显式设置了变量却写错，静默忽略会让构建结果与预期不符。
fn apply_env_override(
    name: &str,
    item: &ConfigItem,
    values: &mut HashMap<String, Value>,
) -> Result<bool> {
    let Ok(raw) = std::env::var(name) else {
        return Ok(false);
    };
    let trimmed = raw.trim();

    let parsed = match item.config_type {
        ConfigType::Bool => match trimmed.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Value::Boolean(true),
            "0" | "false" | "no" | "off" => Value::Boolean(false),
            other => anyhow::bail!(
                "environment variable {name}={other:?} is not a boolean (expected 1/0/true/false)"
            ),
        },
        ConfigType::Int | ConfigType::Hex => {
            let parsed = match trimmed
                .strip_prefix("0x")
                .or_else(|| trimmed.strip_prefix("0X"))
            {
                Some(hex) => i64::from_str_radix(hex, 16),
                None => trimmed.parse::<i64>(),
            };
            match parsed {
                Ok(i) => Value::Integer(i),
                Err(_) => anyhow::bail!(
                    "environment variable {name}={trimmed:?} is not an integer for config '{name}'"
                ),
            }
        }
        ConfigType::String | ConfigType::Choice | ConfigType::Cstr => Value::String(raw.clone()),
    };

    item.validate(&parsed)
        .map_err(|e| anyhow::anyhow!("environment override {name}={raw:?} is invalid: {e}"))?;
    values.insert(item.name.clone(), parsed);
    Ok(true)
}

/// 按 `环境变量 > .config > schema 默认值` 的优先级套用环境变量覆盖。
///
/// 环境变量名与配置名一致（见 README「值优先级」一节）。
/// 返回被覆盖的配置名列表，便于调用方提示用户。
pub fn apply_env_overrides(
    items: &[ConfigItem],
    values: &mut HashMap<String, Value>,
) -> Result<Vec<String>> {
    let mut applied = Vec::new();
    for item in items {
        if apply_env_override(&item.name, item, values)? {
            applied.push(item.name.clone());
        }
    }
    Ok(applied)
}

/// 求出与默认值不同、需要写进 defconfig 的最小配置集。
pub fn get_minimal_config(
    current_values: &HashMap<String, Value>,
    items: &[ConfigItem],
) -> HashMap<String, Value> {
    let defaults = evaluator::collect_defaults(items);
    let mut minimal = HashMap::new();

    for (name, value) in current_values {
        match defaults.get(name) {
            Some(default_val) if value == default_val => {}
            _ => {
                minimal.insert(name.clone(), value.clone());
            }
        }
    }

    minimal
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::ConfigType;

    fn items() -> Vec<ConfigItem> {
        vec![
            ConfigItem {
                name: "ENABLE_NET".into(),
                config_type: ConfigType::Bool,
                default: Some(Value::Boolean(false)),
                desc: String::new(),
                depends_on: None,
                help: None,
                options: None,
                feature: None,
                range: None,
                regex: None,
                rust_type: None,
            },
            ConfigItem {
                name: "MAX_SOCKETS".into(),
                config_type: ConfigType::Int,
                default: Some(Value::Integer(4)),
                desc: String::new(),
                depends_on: None,
                help: None,
                options: None,
                feature: None,
                range: None,
                regex: None,
                rust_type: None,
            },
        ]
    }

    #[test]
    fn load_missing_file_does_not_create_it() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".config");
        let mut warnings = Vec::new();
        let values = load_config(&path, &items(), &mut warnings).unwrap();

        assert!(!path.exists(), "load_config must not create the file");
        assert_eq!(values.get("MAX_SOCKETS"), Some(&Value::Integer(4)));
        assert!(warnings.is_empty());
    }

    #[test]
    fn ensure_config_creates_file_with_defaults() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".config");

        assert!(ensure_config(&path, &items()).unwrap());
        assert!(path.exists());
        // 已存在时不重复创建
        assert!(!ensure_config(&path, &items()).unwrap());
    }

    #[test]
    fn invalid_values_fall_back_with_warning() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".config");
        fs::write(&path, "MAX_SOCKETS = \"not-a-number\"\nUNKNOWN_KEY = 1\n").unwrap();

        let mut warnings = Vec::new();
        let values = load_config(&path, &items(), &mut warnings).unwrap();

        assert_eq!(values.get("MAX_SOCKETS"), Some(&Value::Integer(4)));
        assert_eq!(warnings.len(), 2, "{warnings:?}");
    }

    #[test]
    fn save_config_orders_keys_deterministically() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".config");
        let values: HashMap<String, Value> = [
            ("Z_LAST".to_string(), Value::Integer(1)),
            ("A_FIRST".to_string(), Value::Integer(2)),
            ("M_MID".to_string(), Value::Integer(3)),
        ]
        .into_iter()
        .collect();

        save_config(&path, &values).unwrap();
        let first = fs::read_to_string(&path).unwrap();
        save_config(&path, &values).unwrap();
        let second = fs::read_to_string(&path).unwrap();

        assert_eq!(first, second);
        let a = first.find("A_FIRST").unwrap();
        let m = first.find("M_MID").unwrap();
        let z = first.find("Z_LAST").unwrap();
        assert!(a < m && m < z, "{first}");
    }

    /// 环境变量是进程级全局状态，cargo test 默认多线程运行，
    /// 涉及 env 的测试必须串行化，否则彼此会读到对方设置的值。
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn env_overrides_take_precedence_over_file() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".config");
        fs::write(&path, "MAX_SOCKETS = 8\n").unwrap();

        // 环境变量优先级最高，且按 Hex/Int 类型解析
        std::env::set_var("MAX_SOCKETS", "0x10");
        let mut warnings = Vec::new();
        let values = load_config(&path, &items(), &mut warnings).unwrap();
        std::env::remove_var("MAX_SOCKETS");

        assert_eq!(values.get("MAX_SOCKETS"), Some(&Value::Integer(16)));
    }

    #[test]
    fn env_override_applies_without_config_file() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // 使用不与其他测试冲突的变量名：只覆盖 .config 里不存在的键
        std::env::set_var("ENABLE_NET", "true");
        let values =
            apply_env_overrides(&items(), &mut evaluator::collect_defaults(&items())).unwrap();
        std::env::remove_var("ENABLE_NET");

        assert_eq!(values, vec!["ENABLE_NET".to_string()]);
    }

    #[test]
    fn invalid_env_values_are_errors() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("MAX_SOCKETS", "lots");
        let mut values = evaluator::collect_defaults(&items());
        let err = apply_env_overrides(&items(), &mut values)
            .unwrap_err()
            .to_string();
        std::env::remove_var("MAX_SOCKETS");

        assert!(err.contains("is not an integer"), "{err}");
    }

    #[test]
    fn minimal_config_only_keeps_non_defaults() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let items = items();
        let mut values = evaluator::collect_defaults(&items);
        values.insert("ENABLE_NET".into(), Value::Boolean(true));

        let minimal = get_minimal_config(&values, &items);
        assert_eq!(minimal.len(), 1);
        assert_eq!(minimal.get("ENABLE_NET"), Some(&Value::Boolean(true)));
    }
}
