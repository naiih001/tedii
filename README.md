# Helix-like Editor Plan

## Architecture
- **TUI**: Ratatui.
- **Data**: Rope data structure.
- **Modal**: Kakoune/Helix keybindings.
- **LSP**: Standard protocol support.

## Beta Roadmap
### Phase 1: MVP
- Basic file I/O.
- Buffer editing.
- Modal movement/selection.

### Phase 2: Core
- Syntax highlighting (tree-sitter).
- Basic LSP diagnostics.
- Undo/redo.

### Phase 3: Polish
- Configuration (TOML).
- Plugin system (WASM).

## Releases
- Releases are managed from the `release` branch.
- `release-plz` opens and updates release PRs, generates changelog entries, and tags releases.
- Tagged releases build Linux and macOS release archives automatically.
