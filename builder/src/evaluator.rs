use crate::schema::{ConfigItem, ConfigType};
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

    /// 按 schema 声明的**全部**符号预绑定取值，缺失项回落到声明默认值。
    ///
    /// 依赖表达式只可能引用 schema 里声明过的符号（`validate_configs` 已保证），
    /// 所以预绑定之后求值不会再出现 unbound variable。不做预绑定时，
    /// `defconfig` 缺项或手写 `.config` 漏项都会让整条表达式直接报错，
    /// 而 Kconfig 的语义是"未设置即 n"，不是"表达式失效"。
    pub fn bind_all(
        &mut self,
        items: &[ConfigItem],
        values: &HashMap<String, toml::Value>,
    ) -> Result<()> {
        for item in items {
            let bound = values
                .get(&item.name)
                .or(item.default.as_ref())
                .cloned()
                .unwrap_or_else(|| zero_value(&item.config_type));
            self.set_variable(&item.name, &bound)?;
        }
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

/// 该类型的零值：既没有显式取值、也没有声明 `default` 时的兜底绑定值。
pub fn zero_value(config_type: &ConfigType) -> toml::Value {
    match config_type {
        ConfigType::Bool => toml::Value::Boolean(false),
        ConfigType::Int | ConfigType::Hex => toml::Value::Integer(0),
        ConfigType::String | ConfigType::Choice | ConfigType::Cstr => {
            toml::Value::String(String::new())
        }
    }
}

/// 依赖不满足时该项的隐含值。
///
/// 与 Kconfig 保持一致：`bool` 一律变为关闭（`n`），其余类型回落到声明默认值
/// （Kconfig 里 `int`/`string` 的 `depends_on` 不满足时保留 `default`，
/// 只有布尔/三态会真的变成 `n`）。
pub fn implied_value(item: &ConfigItem) -> toml::Value {
    match item.config_type {
        ConfigType::Bool => toml::Value::Boolean(false),
        _ => item
            .default
            .clone()
            .unwrap_or_else(|| zero_value(&item.config_type)),
    }
}

/// 求出"实际生效"的配置值：`depends_on` 不满足的项按 [`implied_value`] 回落。
///
/// 产物（`autoconf.h` / `config.rs` / `--cfg` / `ANAXA_*` 环境变量）必须用这里的结果。
/// 直接用文件里的原始值会让"父项已关闭"的子项照样出现在头文件里，
/// 生成的宏与实际生效的配置互相矛盾。
///
/// 依赖可以成链（A 关 → B 隐藏 → 依赖 B 的 C 也隐藏），因此迭代到不动点。
/// 每轮读快照、写副本，结果与遍历顺序无关；把值置为隐含值只会让取值朝
/// "关闭"单向变化，过程单调、不会震荡，至多 `items.len() + 1` 轮收敛。
pub fn effective_values(
    items: &[ConfigItem],
    values: &HashMap<String, toml::Value>,
) -> Result<HashMap<String, toml::Value>> {
    let mut current: HashMap<String, toml::Value> = HashMap::with_capacity(items.len());
    for item in items {
        let bound = values
            .get(&item.name)
            .or(item.default.as_ref())
            .cloned()
            .unwrap_or_else(|| zero_value(&item.config_type));
        current.insert(item.name.clone(), bound);
    }

    let max_rounds = items.len() + 1;
    for _ in 0..max_rounds {
        let mut evaluator = Evaluator::new();
        evaluator.bind_all(items, &current)?;

        let mut next = current.clone();
        let mut changed = false;
        for item in items {
            let visible = match item.depends_on.as_ref() {
                Some(expr) => evaluator.check_dependency(expr)?,
                None => true,
            };
            if !visible {
                let implied = implied_value(item);
                if next.get(&item.name) != Some(&implied) {
                    next.insert(item.name.clone(), implied);
                    changed = true;
                }
            }
        }

        current = next;
        if !changed {
            return Ok(current);
        }
    }

    // 走到上限仍未稳定说明依赖链出现了本不该存在的增长；
    // 如实返回最后一轮结果，由调用方的校验环节负责报错，避免死循环
    Ok(current)
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

/// 列出因依赖不满足而被改写的配置项，供 CLI / `build.rs` 输出告警。
///
/// 返回 `(名称, 文件中的值, 生效值)`，只有两者不同时才出现。
pub fn dependency_overrides(
    items: &[ConfigItem],
    raw: &HashMap<String, toml::Value>,
    effective: &HashMap<String, toml::Value>,
) -> Vec<(String, toml::Value, toml::Value)> {
    let mut report = Vec::new();
    for item in items {
        let (Some(from), Some(to)) = (raw.get(&item.name), effective.get(&item.name)) else {
            continue;
        };
        if from != to {
            report.push((item.name.clone(), from.clone(), to.clone()));
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use toml::Value;

    fn make_item(
        name: &str,
        config_type: ConfigType,
        default: Value,
        dep: Option<&str>,
    ) -> ConfigItem {
        ConfigItem {
            name: name.into(),
            config_type,
            default: Some(default),
            desc: String::new(),
            depends_on: dep.map(|s| s.to_string()),
            help: None,
            options: None,
            feature: None,
            range: None,
            regex: None,
            rust_type: None,
        }
    }

    #[test]
    fn bind_all_covers_items_absent_from_values() {
        // 场景：defconfig 只写了一半的键，表达式引用了缺失的符号
        let items = vec![
            make_item("ENABLE_NET", ConfigType::Bool, Value::Boolean(false), None),
            make_item(
                "MAX_SOCKETS",
                ConfigType::Int,
                Value::Integer(16),
                Some("ENABLE_NET"),
            ),
        ];
        let values: HashMap<String, Value> = HashMap::new();

        let mut evaluator = Evaluator::new();
        evaluator.bind_all(&items, &values).unwrap();

        // 未出现 unbound variable，且缺失项按默认值绑定
        assert!(!evaluator.check_dependency("ENABLE_NET").unwrap());
        assert!(evaluator.check_dependency("MAX_SOCKETS == 16").unwrap());
    }

    #[test]
    fn effective_values_turns_off_bools_with_unmet_dependency() {
        let items = vec![
            make_item("ENABLE_NET", ConfigType::Bool, Value::Boolean(false), None),
            make_item(
                "USE_TLS",
                ConfigType::Bool,
                Value::Boolean(true),
                Some("ENABLE_NET"),
            ),
        ];
        let mut raw = HashMap::new();
        raw.insert("ENABLE_NET".to_string(), Value::Boolean(false));
        // 文件里残留 USE_TLS = true（上一次 ENABLE_NET 开着时保存的）
        raw.insert("USE_TLS".to_string(), Value::Boolean(true));

        let effective = effective_values(&items, &raw).unwrap();
        assert_eq!(effective.get("USE_TLS"), Some(&Value::Boolean(false)));
        // 原始值不受影响：重新打开 ENABLE_NET 后 USE_TLS 应当还原
        assert_eq!(raw.get("USE_TLS"), Some(&Value::Boolean(true)));
    }

    #[test]
    fn effective_values_hides_dependency_chain_transitively() {
        // A 关 → B 关 → C 也关（C 只依赖 B）
        let items = vec![
            make_item("A", ConfigType::Bool, Value::Boolean(false), None),
            make_item("B", ConfigType::Bool, Value::Boolean(true), Some("A")),
            make_item("C", ConfigType::Bool, Value::Boolean(true), Some("B")),
        ];
        let mut raw = HashMap::new();
        raw.insert("A".to_string(), Value::Boolean(false));
        raw.insert("B".to_string(), Value::Boolean(true));
        raw.insert("C".to_string(), Value::Boolean(true));

        let effective = effective_values(&items, &raw).unwrap();
        assert_eq!(effective.get("B"), Some(&Value::Boolean(false)));
        assert_eq!(effective.get("C"), Some(&Value::Boolean(false)));
    }

    #[test]
    fn effective_values_keeps_visible_items_untouched() {
        let items = vec![
            make_item("ENABLE_NET", ConfigType::Bool, Value::Boolean(false), None),
            make_item(
                "MAX_SOCKETS",
                ConfigType::Int,
                Value::Integer(16),
                Some("ENABLE_NET"),
            ),
        ];
        let mut raw = HashMap::new();
        raw.insert("ENABLE_NET".to_string(), Value::Boolean(true));
        raw.insert("MAX_SOCKETS".to_string(), Value::Integer(64));

        let effective = effective_values(&items, &raw).unwrap();
        assert_eq!(effective.get("MAX_SOCKETS"), Some(&Value::Integer(64)));
    }

    #[test]
    fn non_bool_hidden_items_fall_back_to_default() {
        // Kconfig 里 int/string 的 depends_on 不满足时保留 default，而不是清零
        let items = vec![
            make_item("ENABLE_NET", ConfigType::Bool, Value::Boolean(false), None),
            make_item(
                "MAX_SOCKETS",
                ConfigType::Int,
                Value::Integer(16),
                Some("ENABLE_NET"),
            ),
        ];
        let mut raw = HashMap::new();
        raw.insert("ENABLE_NET".to_string(), Value::Boolean(false));
        raw.insert("MAX_SOCKETS".to_string(), Value::Integer(512));

        let effective = effective_values(&items, &raw).unwrap();
        assert_eq!(effective.get("MAX_SOCKETS"), Some(&Value::Integer(16)));
    }

    #[test]
    fn dependency_overrides_reports_only_changed_entries() {
        let items = vec![
            make_item("ENABLE_NET", ConfigType::Bool, Value::Boolean(false), None),
            make_item(
                "USE_TLS",
                ConfigType::Bool,
                Value::Boolean(true),
                Some("ENABLE_NET"),
            ),
        ];
        let mut raw = HashMap::new();
        raw.insert("ENABLE_NET".to_string(), Value::Boolean(false));
        raw.insert("USE_TLS".to_string(), Value::Boolean(true));

        let effective = effective_values(&items, &raw).unwrap();
        let report = dependency_overrides(&items, &raw, &effective);
        assert_eq!(report.len(), 1);
        assert_eq!(report[0].0, "USE_TLS");
    }
}
