# PROJECT KNOWLEDGE BASE (src/tui)

**Generated:** 2026-01-17
**Commit:** 02dcb34
**Branch:** master

## OVERVIEW
Implements the Cursive-based Terminal User Interface (TUI) for interactive configuration.

## STRUCTURE
```
tui/
├── mod.rs      # Main TUI event loop, view construction, and data mapping
└── state.rs    # Manages UI-specific state (e.g., selection, expanded nodes)
```

## WHERE TO LOOK
| Task | Location | Notes |
|------|----------|-------|
| Modify TUI layout | `mod.rs` | See `build_view` and related functions |
| Change event handling | `mod.rs` | Main `run` function contains the event loop |
| Add UI-local state | `state.rs` | Extend `TuiState` struct |
| Update tree rendering | `mod.rs` | Logic maps flat config list to `TreeView` |

## CONVENTIONS
- **State Separation**: All UI-specific state (selection, tree expansion) is managed in `state.rs`, separate from the global application configuration.
- **Cursive Backend**: The entire TUI is built on the `cursive` and `cursive-tree-view` crates.
- **Hierarchical Mapping**: A key task of this module is to transform the flat list of configuration items into a hierarchical `TreeView` structure based on their filesystem paths.

## ANTI-PATTERNS
- **Do not** embed application business logic here. This crate is for presentation only.
- **Do not** directly modify the global configuration state; interact with it via the provided data structures.
