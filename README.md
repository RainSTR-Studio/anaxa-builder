# Anaxa Builder

[![Crates.io](https://img.shields.io/crates/v/anaxa-builder?style=for-the-badge&logo=rust&color=orange)](https://crates.io/crates/anaxa-builder)
[![Docs.rs](https://img.shields.io/docsrs/anaxa-builder?style=for-the-badge&logo=docs.rs)](https://docs.rs/anaxa-builder)
[![License](https://img.shields.io/badge/license-MIT-blue?style=for-the-badge&logo=open-source-initiative)](LICENSE)
[![Rust Version](https://img.shields.io/badge/rust-1.70+-brown?style=for-the-badge&logo=rust)](https://www.rust-lang.org/)

现代化的 Rust 原生配置管理系统，旨在替代传统的 Kconfig，使用 TOML 作为 Schema 定义语言。

## 特性

- 📝 **TOML Schema**: 使用现代化的 TOML 格式定义配置，替代古老的 Kconfig 语法
- 🖥️ **交互式 TUI**: 终端用户界面，提供直观的配置体验
- 🔍 **依赖管理**: 自动解析 `depends_on` 依赖关系，构建依赖图并进行循环检测
- 🎯 **类型安全**: 支持 `bool`、`int`、`string`、`hex`、`choice` 等多种配置类型
- 🛡️ **静态校验**: 支持数值范围限制 (`range`) 和正则表达式匹配 (`regex`)
- 🔧 **代码生成**: 自动生成 C 头文件、Rust 常量和 Cargo CFG keys
- 🚀 **Cargo 集成**: 像 `cargo run` 一样无缝运行，自动注入配置为 Features 和 CFG
- 🌳 **灵活映射**: 支持自动递归扫描，也支持通过 `[menu]` 显式定义逻辑层级
- 🏗️ **构建系统集成**: 提供 `BuildHelper` Fluent API，轻松集成到 `build.rs`

## 安装

```bash
cargo install anaxa-builder
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
cargo anaxa config-check

# 查看解析后的配置结构
cargo anaxa dump
```

### 3. 交互式配置

```bash
# 启动 TUI 配置界面
cargo anaxa menuconfig
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

### 4. 运行项目

无需手动传递复杂的 `--features` 或 `--cfg`，直接使用包装命令：

```bash
# 自动注入配置并运行
cargo anaxa run

# 自动注入配置并检查
cargo anaxa check

# 自动注入配置并构建
cargo anaxa build --release
```

这会自动：
- 读取 `.config` 中的值
- 将 `bool` 类型且开启的选项注入为 Cargo Features (如果指定了 `feature` 字段)
- 将所有开启的 `bool` 选项注入为 `--cfg NAME`
- 将所有配置值注入为环境变量 `ANAXA_NAME=VALUE`

## 高级功能：显式菜单映射

默认情况下，Anaxa 会按照物理目录结构自动构建配置菜单。如果你的逻辑结构与物理结构不一致（例如想把深层子目录提升到根菜单），可以使用 `[menu]` 块。

```toml
# 根目录 Kconfig.toml
title = "My Project"

[menu]
# 将 src/Kconfig.toml 映射为 "Kernel Core" 菜单
core = "src"

# 将 drivers/net/Kconfig.toml 映射为 "Network Drivers" 菜单
# 即使物理上它在很深的目录，逻辑上它现在是根菜单的直接子项
net_drivers = "drivers/net"

# 任意路径映射
fs = "filesystem/vfs"
```

这种方式允许你完全解耦物理文件结构和逻辑配置菜单。子模块的 `title` 依然由其自身的 `Kconfig.toml` 定义，父模块只负责层级组织。

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
│   ├── tui/            # 交互式终端界面 (Action/Handler/UI 分离架构)
│   ├── codegen/        # 代码生成器（C、Rust、DOT）
│   ├── schema.rs       # 配置项数据模型
│   ├── parser.rs       # TOML 解析器 (支持显式/隐式映射)
│   ├── graph.rs        # 依赖图构建
│   ├── logic.rs        # 表达式求值逻辑
│   └── config_io.rs    # .config 文件读写
├── generated/          # 生成的代码文件
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
- **TUI**: `ratatui` + `crossterm`

## 命令参考

```bash
# 验证 Schema 和依赖
cargo anaxa config-check

# 包装 Cargo 命令（自动注入 Features/CFG/Env）
cargo anaxa run
cargo anaxa check
cargo anaxa build

# 启动交互式配置
cargo anaxa menuconfig

# 生成代码
cargo anaxa generate
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
- [x] Cargo 包装命令 (run/check/build)
- [x] 显式菜单映射 ([menu])
- [ ] 搜索功能增强 (TUI)
- [ ] TUI 帮助系统完善
- [ ] build.rs 深度集成

## 贡献

欢迎提交 Issue 和 Pull Request！

## 许可证

MIT License
