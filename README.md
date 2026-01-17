# Anaxa Builder

现代化的 Rust 原生配置管理系统，旨在替代传统的 Kconfig，使用 TOML 作为 Schema 定义语言。

## 特性

- 📝 **TOML Schema**: 使用现代化的 TOML 格式定义配置，替代古老的 Kconfig 语法
- 🖥️ **交互式 TUI**: 终端用户界面，提供直观的配置体验
- 🔍 **依赖管理**: 自动解析 `depends_on` 依赖关系，构建依赖图并进行循环检测
- 🎯 **类型安全**: 支持 `bool`、`int`、`string`、`hex`、`choice` 等多种配置类型
- 🛡️ **静态校验**: 支持数值范围限制 (`range`) 和正则表达式匹配 (`regex`)
- 🔧 **代码生成**: 自动生成 C 头文件、Rust 常量和 Cargo CFG keys
- 🏗️ **构建系统集成**: 提供 `BuildHelper` Fluent API，轻松集成到 `build.rs`
- 🌳 **递归扫描**: 自动发现并聚合 `src/` 目录下所有子目录的配置文件

## 安装

```bash
# 克隆仓库
git clone https://github.com/RainSTR-Studio/anaxa-builder.git
cd anaxa-builder

# 使用 cargo 安装
cargo install --path .
```

## 快速开始

### 1. 定义配置 Schema

在项目目录下创建 `Kconfig.toml` 文件：

```toml
# 示例: src/net/Kconfig.toml
title = "Networking Support"

[[config]]
name = "ENABLE_NET"
type = "bool"
default = true
desc = "Enable networking subsystem"
feature = ["net"]  # 对应 Cargo features

[[config]]
name = "SCHEDULER"
type = "choice"
default = "RR"
desc = "Process Scheduler Algorithm"
options = ["RR", "FIFO", "CFS"]

[[config]]
name = "MAX_SOCKETS"
type = "int"
default = 16
depends_on = "ENABLE_NET"
range = [1, 1024]
desc = "Maximum number of open sockets"

[[config]]
name = "DEVICE_NAME"
type = "string"
default = "anaxa-node"
regex = "^[a-z0-9-]+$"
desc = "Device identification name"
```

### 2. 验证配置

```bash
# 检查 Schema 有效性并检测循环依赖
cargo run -- check

# 查看解析后的配置结构
cargo run -- dump
```

### 3. 交互式配置

```bash
# 启动 TUI 配置界面
cargo run -- menuconfig
```

在 TUI 中：
- 使用方向键导航
- 按 `[Y]` 启用/禁用 bool 选项
- 按 `[N]` 禁用选项
- 按 `[M]` 选择/取消选择 choice 选项
- 按 `?` 查看帮助信息
- 按 `/` 搜索配置项
- 按 `[S]` 保存配置到 `.config`
- 按 `[Q]` 退出

### 4. 生成代码

```bash
# 生成代码到 generated/ 目录
cargo run -- generate
```

这将生成：
- `generated/autoconf.h` - C 头文件
- `generated/config.rs` - Rust 常量
- `generated/depends.dot` - 依赖关系图（可选）

### 5. 在 build.rs 中集成

在你的 `build.rs` 中添加以下代码，即可实现配置自动生成和环境变量注入：

```rust
fn main() -> anyhow::Result<()> {
    anaxa_builder::BuildHelper::new()?
        .with_kconfig_dir("src")     // Schema 扫描目录
        .with_config_file(".config")  // 配置文件路径
        .build()?;
    Ok(())
}
```

这会自动：
- 生成 `config.rs` 到 `OUT_DIR`
- 设置 `cargo:rustc-cfg` 标志
- 注入 `ANAXA_` 前缀的环境变量
- 自动处理 `rerun-if-changed` 逻辑

## 配置类型

| 类型 | 说明 | 示例 |
|------|------|------|
| `bool` | 布尔值 | `true` / `false` |
| `int` | 整数 | `42` |
| `string` | 字符串 | `"hello"` |
| `hex` | 十六进制 | `0x1A2B` |
| `choice` | 单选组 | 从预定义选项中选择 |

## Schema 字段

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `name` | String | 是 | 配置项名称 |
| `type` | ConfigType | 是 | 配置类型 (见上) |
| `default` | Any | 是 | 默认值 |
| `desc` | String | 否 | 简短描述 |
| `help` | String | 否 | 详细帮助信息 |
| `depends_on` | String | 否 | 依赖表达式 |
| `feature` | Vec<String> | 否 | 对应的 Cargo features |
| `options` | Vec<String> | 否 | choice 类型的可选值 |
| `range` | [i64, i64] | 否 | 整数取值范围 |
| `regex` | String | 否 | 字符串正则表达式约束 |

## 依赖表达式

支持使用 `evalexpr` 语法的逻辑表达式：

```toml
depends_on = "ENABLE_NET && !IPV6_DISABLE"
depends_on = "USE_TLS || USE_SSL"
```

## 目录结构

```
anaxa-builder/
├── src/
│   ├── codegen/        # 代码生成器（C、Rust、DOT）
│   ├── schema.rs      # 配置项数据模型
│   ├── parser.rs      # TOML 解析器
│   ├── graph.rs       # 依赖图构建
│   ├── logic.rs       # 表达式求值逻辑
│   └── config_io.rs   # .config 文件读写
├── generated/         # 生成的代码文件
│   ├── autoconf.h
│   ├── config.rs
│   └── depends.dot
├── Cargo.toml
└── README.md
```

## 技术栈

- **CLI**: `clap`
- **序列化**: `serde` + `toml`
- **表达式解析**: `evalexpr`
- **图算法**: `petgraph`
- **文件扫描**: `walkdir`

## 命令参考

```bash
# 验证 Schema 和依赖
cargo run -- check

# 查看配置结构
cargo run -- dump

# 启动交互式配置
cargo run -- menuconfig

# 生成代码
cargo run -- generate
```

## 值优先级

配置值的优先级从高到低：

1. **环境变量**: `ENABLE_NET=true`
2. **.config 文件**: 用户保存的配置
3. **Schema 默认值**: Kconfig.toml 中定义的默认值

## 开发路线图

- [x] TOML Schema 定义与解析
- [x] 依赖图构建与循环检测
- [x] 交互式 TUI（基础功能）
- [x] 代码生成（C、Rust、DOT）
- [ ] 搜索功能增强
- [ ] TUI 帮助系统完善
- [ ] build.rs 深度集成
- [ ] Cargo Features 动态支持

## 贡献

欢迎提交 Issue 和 Pull Request！

## 许可证

MIT License
