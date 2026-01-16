# PROJECT KNOWLEDGE BASE

**Generated:** 2026-01-17
**Commit:** 02dcb34
**Branch:** master

## OVERVIEW
This module translates the internal configuration state into various code artifacts.

## STRUCTURE
```
codegen/
├── c.rs        # C header generator (e.g., autoconf.h)
├── dot.rs      # Graphviz dependency graph generator (e.g., depends.dot)
├── rust.rs     # Rust constants and cfg generator (e.g., config.rs)
└── mod.rs      # Module entry point
```

## WHERE TO LOOK
| Task                                | Location          | Notes                                                    |
| ----------------------------------- | ----------------- | -------------------------------------------------------- |
| Add/modify C `#define` macros         | `c.rs`            | Generates `CONFIG_*` prefixed defines.                   |
| Add/modify Rust `const` values      | `rust.rs`         | See `generate_consts`.                                   |
| Change Cargo `cfg` key generation     | `rust.rs`         | See `generate_cargo_keys` for `cargo:rustc-cfg` output.  |
| Alter dependency graph visualization  | `dot.rs`          | Uses `petgraph::dot` for Graphviz output.                |
| Orchestrate a new generator         | `(parent scope)`  | Call new generator functions from `main.rs` or `lib.rs`. |

## CONVENTIONS
- **Stateless Generators**: Each generator is a pure function. It accepts configuration state (`AppState`, `ConfigGraph`, etc.) and returns a `Result<String>`.
- **No Direct I/O**: File writing is handled by the calling context (e.g., `main.rs`), not within the generator modules themselves. This keeps them portable and easy to test.
- **Input-Driven**: Generators operate exclusively on the data passed to them, without referencing global state.

## ANTI-PATTERNS
- **Do not** add file I/O operations (`std::fs`) within any generator module. The caller is responsible for writing the returned string to a file.
- **Do not** embed configuration parsing or evaluation logic here. This module's only responsibility is to format output.
- **Do not** make generators dependent on each other; they should be independent transformations of the core configuration state.
