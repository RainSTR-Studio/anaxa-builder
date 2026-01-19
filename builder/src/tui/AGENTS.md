# TUI MODULE KNOWLEDGE BASE

## OVERVIEW
Interactive terminal interface using `ratatui` and `crossterm`.
Implements a hierarchical menu system with search, editing, and dependency visualization.

## STRUCTURE
```
src/tui/
├── mod.rs      # App state, Event loop, State management
├── ui.rs       # Rendering logic (Widgets, Layouts)
├── handler.rs  # Raw event handling (Key events -> Actions)
└── action.rs   # Abstract Action definitions
```

## KEY COMPONENTS
| Component | File | Role |
|-----------|------|------|
| `App` | `mod.rs` | Root state container. Holds `ConfigNode`, `values`. |
| `UiState` | `mod.rs` | Visual state (lists, search, scroll). |
| `Action` | `action.rs` | Enum of all possible user intents. |
| `Editor` | `mod.rs` | Transient state for editing values. |

## PATTERNS
- **Action-Based Update**: `handler.rs` maps inputs to `Action`. `App::handle_action` updates state.
- **Immediate Mode UI**: `ui::draw` renders entire state every frame.
- **Navigation**: Uses `current_node_path` (stack of indices) to track menu depth.

## CONVENTIONS
- **State Mutation**: All state changes happen in `App` methods.
- **Rendering**: `ui.rs` should be pure rendering logic. No logic there.
- **Input**: Raw events converted to semantic `Action`s immediately.

## SEARCH
- Implemented as recursive traversal in `App::search_recursive`.
- Results stored as `(path, index)` tuples to allow jumping.
