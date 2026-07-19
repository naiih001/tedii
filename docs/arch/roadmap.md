# Beta Roadmap

## Phase 1: MVP (Basic Editing)
- **Buffer/Rope Implementation:** Core text storage. x
- **TUI Rendering:** Basic frame rendering. x
- **Input Handling:** Handling keyboard events. x
- **Basic Modal Editing:** x
  - Navigation (h, j, k, l). x
  - Mode switching (Normal <-> Insert). x
  - Text insertion/deletion. x

## Phase 2: Core Editing
- **Syntax Highlighting:** Tree-sitter integration. x
- **LSP Diagnostics:** Show errors/warnings. x
- **Persistent State:** Undo/redo stack. x
- **File System:** Open/save/close buffers. x

## Phase 3: Polish & Extensibility
- **Configuration:** TOML-based user configuration.
- **Plugin System:** WASM plugin lifecycle management.
- **Command Palette:** Fuzzy matching for commands.
