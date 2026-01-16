# Anaxa 配置管理系统实现计划

一个构建 Rust 原生配置管理系统（我们暂命名为 `rconfig` 或 `anaxa-config`）的完整实现计划。

基于调研，我们将采用 **TOML 作为 Schema 定义语言**（替代古老的 Kconfig 语法），利用 **Cursive** 构建稳定的 TUI，并通过 **build.rs** 深度集成到 Cargo 构建流中。

## 核心架构概览

系统由三个主要部分组成：
1.  **Core (Parser & Logic)**: 解析 TOML Schema，处理依赖关系 (`depends_on`)，合并默认值。
2.  **TUI (Frontend)**: 基于 `cursive` 的交互式配置界面 (`cargo menuconfig`)。
3.  **Generator (Backend)**: 在编译时生成 Rust `cfg`、Rust常量文件和 C 头文件。

---

## 1. 详细实施步骤

### Phase 1: 核心库与 Schema 设计 (The "Model")
目标：定义配置如何描述，以及如何解析和合并。

*   **Schema 定义 (TOML)**: 设计一套类似 Kconfig 的 TOML 结构。
    *   支持类型：`bool`, `string`, `int`, `hex`, `choice` (单选组)。
    *   支持字段：`name`, `type`, `default`, `desc`, `depends_on`, `help`, `feature` (控制 Cargo features)。
    *   `depends_on` 解析：支持简单的逻辑表达式（如 `ENABLE_NET & !IPV6_DISABLE`）。
*   **递归扫描器**:
    *   实现递归遍历 `src/` 寻找 `Kconfig.toml` (或类似命名)。
    *   建立配置树（Config Tree），处理父子关系（Menu 嵌套）。
*   **依赖图构建 (Graph)**:
    *   使用 `petgraph` 构建配置项之间的依赖关系图。
    *   实现循环依赖检测（Cycle Detection），防止 A->B->A 的死循环。
*   **值合并策略**:
    *   优先级：`ENV VAR` > `.config` (本地保存值) > `Kconfig.toml` (默认值)。

**示例 Schema (`src/net/Kconfig.toml`):**
```toml
[menu]
title = "Networking Support"

[[config]]
name = "ENABLE_NET"
type = "bool"
default = true
desc = "Enable networking subsystem"
# feature 字段由 wrapper 解析，用于生成 --features 参数
feature = ["net"]

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
desc = "Maximum number of open sockets"

```

### Phase 2: TUI 交互界面 (The "View")
目标：实现 `cargo menuconfig`。

*   **库选择**: **Cursive** (因其处理表单和复杂交互逻辑优于 Ratatui)。
*   **功能实现**:
    *   **Tree View**: 左侧或层级式导航菜单。
    *   **Dynamic Visibility**: 根据 `depends_on` 实时隐藏/显示或禁用选项。
    *   **Choice 渲染**: 使用 Radio Button Group 渲染 `choice` 类型（单选组）。
    *   **Search**: 按 `/` 键搜索配置项名称或描述。
    *   **Help**: 按 `?` 键显示详细帮助信息。
    *   **Save/Load**: 将最终配置序列化为 `.config` (TOML格式) 文件。

### Phase 3: Cargo 集成与代码生成 (The "Controller")
目标：让配置在编译时生效。

#### 3.1 build.rs 集成
*   使用 `walkdir` 扫描 Schema 和 `.config` 文件。
*   **关键**: 输出 `cargo:rerun-if-changed=src/**/Kconfig.toml` 和 `cargo:rerun-if-changed=.config`，避免过度重新编译。
*   **生成器 (Generators)**:
    *   **Rust CFG**: 仅针对 Bool 类型，生成 `cargo:rustc-cfg=ENABLE_NET`。
    *   **Rust Consts**: 针对 Int/String/Hex，生成 `src/generated/config.rs`，包含 `pub const MAX_SOCKETS: i32 = 16;`。
    *   **C Header(可选)**: 生成 `include/autoconf.h`，包含 `#define CONFIG_MAX_SOCKETS 16`。
    *   **依赖图(可选)**: 生成 `src/generated/depends.dot`，包含依赖关系图。

#### 3.2 Cargo Features (Wrapper 方案)
*   **限制**: 标准 `build.rs` 无法动态启用 `Cargo.toml` 中定义的 features。
*   **解决方案**: 实现 `cargo anaxa build` wrapper 命令。
    *   流程：读取 `.config` -> 解析 `feature` 字段 -> 拼接 `--features "net,usb"` -> 调用底层 `cargo build`。
    *   用法：用户通过 `cargo anaxa build` 编译，而不是直接 `cargo build`。

---

## 2. 开发路线图 (Roadmap)

| 阶段 | 任务 | 预计产出 |
| :--- | :--- | :--- |
| **Week 1** | **Schema & Parser** | 定义数据结构（含 Choice 类型），实现 TOML 解析，集成 `petgraph` 构建依赖图并实现循环检测。 |
| **Week 2** | **CLI & TUI Prototype** | `cargo anaxa menuconfig` 能跑通，显示配置列表（含 Choice 单选组），支持保存/加载 `.config`。 |
| **Week 3** | **Build Integration** | 编写 `build.rs` 生成 `cfg`/`consts`，实现 `cargo anaxa build` wrapper 传递动态 features。 |
| **Week 4** | **Polishing** | 完善 TUI（搜索、帮助、动态隐藏依赖项），增加 Hex/Int 输入校验，编写文档。 |

---

## 3. 技术栈推荐

*   **CLI 框架**: `clap`
*   **TUI 库**: `cursive` (配合 `cursive_tree_view`)
*   **序列化**: `serde`, `toml`
*   **表达式解析**: `evalexpr` (用于解析 `depends_on = "A && B"`)
*   **图算法**: `petgraph` (构建依赖图、循环检测)
*   **构建辅助**: `walkdir` (文件扫描)
*   **Wrapper**: 调用底层 `cargo` 命令，传递动态 features

---

## 4. 目录结构建议

建议将工具作为一个 Workspace 成员或独立的 Cargo 子命令 crate：

```text
anaxa-config/
├── Cargo.toml
├── src/
│   ├── main.rs           # CLI 入口 (init, menuconfig, gen, build)
│   ├── schema.rs         # 配置项定义 (ConfigItem, ConfigType)
│   ├── parser.rs         # 递归扫描与 TOML 解析
│   ├── graph.rs          # 依赖图构建与循环检测
│   ├── logic.rs          # 依赖检查与求值
│   ├── tui/              # Cursive 界面实现
│   └── gen/              # 生成器 (RustCfg, AutoconfHeader)
```

