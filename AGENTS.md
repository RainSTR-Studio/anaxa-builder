# PROJECT KNOWLEDGE BASE

**Generated:** 2026-01-18
**Context:** Rust, Kconfig-replacement, TOML, TUI

## OVERVIEW
Modern configuration management system (Kconfig alternative) written in Rust.
Uses TOML for schema (`Kconfig.toml`) and generates C/Rust artifacts.
Core features: Dependency graph, Type safety, TUI (Ratatui), Cargo integration.

## STRUCTURE
```
.
├── src/
│   ├── main.rs       # CLI entry point (clap)
│   ├── lib.rs        # Library root, module exports
│   ├── tui/          # Interactive terminal UI (Ratatui)
│   ├── codegen/      # Code generators (C, Rust, DOT)
│   ├── parser.rs     # TOML schema parsing & flattening
│   ├── evaluator.rs  # Dependency logic (evalexpr)
│   ├── graph.rs      # Dependency graph (petgraph)
│   ├── config_io.rs  # .config read/write
│   └── schema.rs     # Data models (ConfigItem, ConfigNode)
├── generated/        # Output directory (Non-standard location)
└── Cargo.toml        # Dependencies: ratatui, clap, toml, petgraph
```

## WHERE TO LOOK
| Task | Location | Notes |
|------|----------|-------|
| **CLI Commands** | `src/main.rs` | `Commands` enum defines verbs |
| **Config Logic** | `src/evaluator.rs` | Logic expression handling |
| **Schema/Types** | `src/schema.rs` | `ConfigType`, `ConfigItem` |
| **UI Logic** | `src/tui/mod.rs` | State management, Event loop |
| **File I/O** | `src/config_io.rs` | Load/Save `.config` |

## CONVENTIONS
- **Testing**: Inline unit tests `#[cfg(test)]` in source files. No `tests/` dir.
- **Config**: Uses `Kconfig.toml` inside source directories.
- **Output**: Writes generated files to `generated/` (not `target/`).
- **Error Handling**: Uses `anyhow::Result` and `thiserror`.

## ANTI-PATTERNS (THIS PROJECT)
- **Do not** place generated files in `src/`. Use `generated/`.
- **Do not** use `unwrap()` in production code; use `anyhow` context.
- **Do not** mix UI logic in `main.rs`; keep it in `src/tui`.

## COMMANDS
```bash
cargo anaxa menuconfig  # Start TUI
cargo anaxa build       # Build wrapper
cargo anaxa generate    # Generate artifacts
cargo test             # Run inline tests
```

## NOTES
- `src/net/Kconfig.toml` serves as an example/fixture.
- The project wraps cargo commands to inject configuration features/cfgs.
