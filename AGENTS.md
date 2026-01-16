# PROJECT KNOWLEDGE BASE

**Generated:** 2026-01-16
**Commit:** 02dcb34
**Branch:** master

## OVERVIEW
Anaxa-builder is a modern, Rust-native configuration management system designed as a replacement for Kconfig using TOML schemas. It provides a TUI for interactive configuration and generates C headers, Rust constants, and Cargo CFG keys.

## STRUCTURE
```
anaxa-builder/
├── src/
│   ├── codegen/    # Generators for C, Rust, and DOT outputs
│   ├── tui/        # Cursive-based Terminal UI implementation
│   ├── schema.rs   # Core data models for configuration items
│   ├── parser.rs   # Recursive TOML configuration scanner
│   ├── graph.rs    # Dependency graph building and cycle detection
│   ├── logic.rs    # Dependency expression evaluation logic
│   ├── config_io.rs # Loading/saving .config (TOML format)
│   └── lib.rs      # Library entry point
├── generated/      # Build artifacts (autoconf.h, config.rs, depends.dot)
├── plan.md         # Project roadmap and architecture plan
└── Cargo.toml      # Project manifest
```

## WHERE TO LOOK
| Task | Location | Notes |
|------|----------|-------|
| Add config types | `src/schema.rs` | Update `ConfigType` enum and serialization logic |
| Modify parsing logic | `src/parser.rs` | Handles recursive `Kconfig.toml` scanning |
| Update TUI | `src/tui/` | Uses `cursive` and `cursive-tree-view` |
| New code generator | `src/codegen/` | Modules for `c.rs`, `rust.rs`, and `dot.rs` |
| Expresssion eval | `src/logic.rs` | Uses `evalexpr` for `depends_on` logic |

## CONVENTIONS
- **Configuration Format**: Always use `Kconfig.toml` for defining configuration options.
- **Recursive Scanning**: Configuration items are automatically aggregated from all subdirectories of `src/`.
- **Value Priority**: ENV VAR > `.config` (TOML) > `Kconfig.toml` defaults.
- **Build Integration**: Rust `cfg` keys generated via `cargo:rustc-cfg` during code generation.

## ANTI-PATTERNS (THIS PROJECT)
- **Do not** use traditional Kconfig syntax; the system is strictly TOML-based.
- **Do not** manually include configuration files; the parser handles recursive discovery.
- **Do not** commit `generated/` artifacts or `.config` files (enforced by `.gitignore`).

## UNIQUE STYLES
- **Modern Kconfig**: Mimics Kconfig logic (menus, depends_on, choices) but leverages TOML for structure and `evalexpr` for logic.
- **Graph-Validated**: Mandatory dependency cycle detection using `petgraph` before any generation.

## COMMANDS
```bash
cargo run -- check         # Validate schemas and check for cycles
cargo run -- dump          # Inspect parsed configuration structure
cargo run -- menuconfig    # Launch interactive TUI
cargo run -- generate      # Generate code artifacts in generated/
```

## NOTES
- Build integration currently requires manual execution of the `generate` command, though future integration with `build.rs` is planned.
- TUI currently uses a flat list mapped to a tree structure; hierarchical menu representation is partially implemented via directory mapping.
