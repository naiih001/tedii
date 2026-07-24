# 06 — Configuration

> Config files, theme system, language definitions, keybindings.

## 1. Configuration Files

All config files live under `~/.config/tedii/`.

| File | Format | Purpose | Loaded By |
|------|--------|---------|-----------|
| `languages.toml` | TOML | Language definitions, grammars, LSP configs | `config::load_config()` |
| `config.toml` | TOML | Theme overrides, keybinding settings | `load_theme_config()`, `load_keybindings_config()` |

---

## 2. languages.toml Structure

```toml
[[grammars]]
name = "rust"
source = "tree-sitter/tree-sitter-rust"

[[languages]]
name = "Rust"
file_types = ["rs"]
grammar = "rust"
highlights = "queries/rust/highlights.scm"  # optional

[languages.lsp]
command = "rust-analyzer"
args = []
```

### Grammar Section
Each grammar has:
- `name` — identifier used to reference the grammar
- `source` — GitHub `owner/repo` or full URL for fetching/building

### Language Section
Each language has:
- `name` — display name
- `file_types` — file extensions (without dot)
- `grammar` — references a grammar name above
- `highlights` — optional custom path to highlights.scm
- `lsp` — optional LSP server configuration (command + args)

---

## 3. config.toml Structure

```toml
[theme]
[theme.syntax]
"keyword" = { fg = "#ff79c6" }
"string" = { fg = "#f1fa8c", modifiers = ["italic"] }
"function" = { fg = "#50fa7b", bg = "#282a36" }

[theme.ui]
"mode_normal" = { fg = "#6272a4", bg = "#44475a" }
"editor_bg" = { fg = "#f8f8f2", bg = "#282a36" }

[keybindings]
leader_keys = true
```

---

## 4. Theme Resolution

```rust
// 1. Start with DEFAULT_THEME (27 syntax scopes + 39 UI elements)
// 2. Apply user overrides from config.toml [theme]
// 3. Result: Theme { scopes: HashMap, ui: HashMap }
```

### Scope Resolution (Hierarchical Fallback)

```
Query: style_for_capture("keyword.control.import")
  1. Lookup "keyword.control.import" → found? return it
  2. Lookup "keyword.control" → found? return it
  3. Lookup "keyword" → found? return it
  4. Return Style::default()
```

### Default Syntax Scopes

27 scopes defined with 8/16 ANSI colors:
`attribute`, `comment`, `constant.builtin`, `constant`, `constructor`, `embedded`, `error`, `escape`, `function`, `function.builtin`, `keyword`, `keyword.control`, `keyword.function`, `label`, `module`, `number`, `operator`, `property`, `punctuation`, `punctuation.bracket`, `punctuation.delimiter`, `string`, `string.special`, `tag`, `type`, `type.builtin`, `variable`, `variable.builtin`, `variable.member`, `variable.parameter`

### Default UI Elements

39 UI element styles for:
- Mode indicators: `mode_normal`, `mode_insert`, `mode_visual`, `mode_command`, `mode_search`
- Editor: `editor_bg`, `status_bar`, `line_numbers`, `cursor_line`
- Popups: `explorer_border/selected/dir/filter`, `fuzzy_border/query/selected/dir/match`
- Git: `git_border/selected/success/error/section/page_text/query`, `git_status_modified/added/untracked/deleted/renamed/copied`
- Diagnostics: `diagnostic_error/warning/information/hint`
- Search: `search_match`, `selection`
- Diff: `diff_added/removed/modified`

---

## 5. Color Format

Colors can be:
- **Named ANSI**: `black`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan`, `gray`, `dark_gray`, `light_red`, `light_green`, `light_yellow`, `light_blue`, `light_magenta`, `light_cyan`, `white`
- **Hex RGB**: `#RRGGBB` (parsed in `parse_hex()`)
- **Default** (no color): when omitted or invalid

### Style Modifiers
Only `"italic"` is supported currently (parsed by `parse_modifiers()`).

---

## 6. Keybindings

Currently very simple:
- `leader_keys: bool` (default: `true`)
- Controls whether Space leader-key is enabled
- All other keybindings are hardcoded in `main.rs` based on `Mode`
