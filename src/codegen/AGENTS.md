# CODEGEN MODULE KNOWLEDGE BASE

## OVERVIEW
Generates output artifacts from the validated configuration.
Supports Rust (consts/cfgs), C (headers), and DOT (dependency graphs).

## STRUCTURE
```
src/codegen/
├── mod.rs      # Exports
├── rust.rs     # Rust code generation
├── c.rs        # C header generation
└── dot.rs      # Graphviz DOT generation
```

## PATTERNS
- **Function-based**: Uses standalone functions (e.g., `generate_consts`).
- **Input**: Takes `HashMap<String, Value>` (flat config values).
- **Output**: Writes strings/buffers. I/O is handled by caller or specific helper.

## CONVENTIONS
- **Rust Generation**: Generates `const` declarations.
- **Naming**: Converts config keys to upper case for constants.
- **Separation**: Generation logic is decoupled from `App` state or `tui`.
