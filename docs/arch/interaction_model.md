# Interaction Model

## Modal States
- **Normal:** Commands manipulate buffers, cursors, and selections.
- **Insert:** Direct text entry.
- **Select:** Visual mode for range manipulation (mimics Helix's selection-first model).

## Keybinding Implementation
- Action dispatch: Map keys to functions.
- Command chaining: (e.g., `d` `w` for delete word).
