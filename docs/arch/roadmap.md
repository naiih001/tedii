# Beta Roadmap

## Phase 1: MVP (Basic Editing)
- **Buffer/Rope Implementation:** Core text storage.
- **TUI Rendering:** Basic frame rendering.
- **Input Handling:** Handling keyboard events.
- **Basic Modal Editing:**
  - Navigation (h, j, k, l).
  - Mode switching (Normal <-> Insert).
  - Text insertion/deletion.

## Phase 2: Core Editing
- **Syntax Highlighting:** Tree-sitter integration.
- **LSP Diagnostics:** Show errors/warnings.
- **Persistent State:** Undo/redo stack.
- **File System:** Open/save/close buffers.

## Phase 3: Polish & Extensibility
- **Configuration:** TOML-based user configuration.
- **Plugin System:** WASM plugin lifecycle management.
- **Command Palette:** Fuzzy matching for commands.
