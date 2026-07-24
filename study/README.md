# tedii — Project Study

> A comprehensive architectural analysis of **tedii**, a modal terminal text editor written in Rust.

This study maps the entire codebase into structural components, interaction patterns, and behavioral flows. It is intended as a reference for understanding the architecture, extending functionality, or onboarding new contributors.

## Contents

| Document | Description |
|----------|-------------|
| [01-COMPONENTS.md](./01-COMPONENTS.md) | All structural units: modules, types, subsystems |
| [02-INTERACTIONS.md](./02-INTERACTIONS.md) | Communication paths, data flow, and dependency wiring |
| [03-BEHAVIOR.md](./03-BEHAVIOR.md) | State machines, event loops, lifecycle, and mode transitions |
| [04-DATA-MODEL.md](./04-DATA-MODEL.md) | Core data structures, type hierarchy, and invariants |
| [05-UI-MAP.md](./05-UI-MAP.md) | Screen layout, visual hierarchy, popup system, rendering pipeline |
| [06-CONFIGURATION.md](./06-CONFIGURATION.md) | Config files, theme system, language definitions, keybindings |
| [07-LSP-INTEGRATION.md](./07-LSP-INTEGRATION.md) | LSP client architecture, protocol flow, message lifecycle |
| [08-GIT-INTEGRATION.md](./08-GIT-INTEGRATION.md) | Git repository operations, diff engine, popup UI workflows |
| [09-DEPENDENCY-GRAPH.md](./09-DEPENDENCY-GRAPH.md) | Inter-module dependencies, crate dependency tree, external libraries |

## Key Stats

- **Language:** Rust (edition 2021)
- **Source files:** 15 modules in `src/`
- **Lines of code:** ~4,200+ (source) + ~1,500+ (tests)
- **TUI Framework:** ratatui 0.29 + crossterm 0.28
- **Text Buffer:** ropey 1.6 (Rope data structure)
- **Syntax:** tree-sitter 0.25 with dynamically loaded grammars
- **LSP:** Custom JSON-RPC 2.0 client over stdio
- **Git:** gix (gitoxide) 0.84 + git CLI for mutations
- **Build:** Cargo, single-crate layout
