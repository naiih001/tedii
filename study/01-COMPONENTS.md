# 01 — Components

> All structural parts of the tedii system, organized by layer.

## Layer Overview

```
┌─────────────────────────────────────────────────────┐
│                    main.rs                           │
│         (Orchestrator: Event Loop + Rendering)       │
├──────────┬──────────┬──────────┬──────────┬─────────┤
│ Editor   │ File Exp │ Fuzzy    │ Git      │ Popups  │
│ Core     │ lorer    │ Finder   │ Picker   │ (Hover, │
│          │          │          │          │ Compl.) │
├──────────┴──────────┴──────────┴──────────┴─────────┤
│                  Infrastructure                       │
│  TUI  │  Config  │  Theme  │  Syntax  │  LSP  │  Git │
│ (tui) │ (config) │ (theme) │ (syntax) │ (lsp) │ (git)│
├──────────────────────────────────────────────────────┤
│                   Utilities                          │
│              fuzzy.rs  │  grammar_commands.rs         │
└──────────────────────────────────────────────────────┘
```

---

## 1. Orchestrator — `main.rs`

**Purpose:** Entry point, terminal lifecycle, main event loop, rendering dispatch, keyboard routing.

**Key Types:**
- `HoverPopupMetrics` — bounding rectangle and max scroll for hover popup
- `PopupKind` — enum: `None | Diagnostic | Hover | Completion`

**Key Functions:**
- `main() -> Result<()>` — the entire application lifecycle
- `popup_kind(...) -> PopupKind` — priority-based popup selection
- `cursor_changed(before, after) -> bool` — cursor change detection
- `hover_popup_metrics(text, area) -> Option<HoverPopupMetrics>` — popup sizing

**Responsibilities:**
1. Parse CLI arguments (`--init`, `--grammar`, file/directory path)
2. Load configuration (languages, theme, keybindings)
3. Initialize TUI, Editor, FileExplorer, FuzzyFinder, GitPicker
4. Run render-dispatch loop at ~10 FPS (100ms poll timeout)
5. Route keyboard events based on overlay visibility (FileExplorer > GitPicker > FuzzyFinder > Editor)
6. Shutdown and restore terminal

---

## 2. Editor Core — `editor.rs`

**Purpose:** Pure data/logic core with zero terminal I/O. Manages buffer, cursor, modes, undo/redo, selection, clipboard, LSP integration, git diff, and styled text production.

**Key Types:**
- `Mode` — enum: `Normal | Insert | Command | Search | Fuzzy | Visual`
- `Editor` — the main struct holding all editor state

**Editor Fields:**
| Field | Type | Role |
|-------|------|------|
| `buffer` | `Rope` | Text buffer (rope data structure) |
| `cursor` | `usize` | Character-index cursor position |
| `scroll_x`, `scroll_y` | `usize` | Viewport scroll offset |
| `mode` | `Mode` | Current modal state |
| `should_quit` | `bool` | Exit signal |
| `command_buffer` | `String` | Command-mode input |
| `current_file` | `Option<PathBuf>` | Open file path |
| `highlighter` | `SyntaxHighlighter` | Tree-sitter highlighting |
| `theme` | `Theme` | Color/style configuration |
| `search_query` / `search_results` | `String` / `Vec<usize>` | Search state |
| `selection_anchor` | `Option<usize>` | Visual mode anchor |
| `clipboard` | `String` | Internal clipboard |
| `diff_hunks` | `Vec<DiffHunk>` | Git diff markers |
| `lsp_diagnostics` | `DiagnosticState` | LSP diagnostics |
| `hover` | `HoverState` | Hover popup state |
| `completion` | `CompletionState` | Completion popup state |
| `undo_stack` / `redo_stack` | `Vec<(Rope, usize)>` | Undo/redo history |

**Key Method Categories:**
- **Lifecycle:** `new()`, `save()`, `open_file()`
- **Movement:** `move_left/right/up/down()`, `move_word_forward/backward()`, `move_to_line_start/end()`
- **Editing:** `insert_char()`, `delete_char()`, `insert_tab()`, `split_bracket_pair_at_cursor()`
- **Undo/Redo:** `undo()`, `redo()`, `begin_undo_group()`
- **Selection:** `enter/exit_visual_mode()`, `get_selection_range()`, `yank/delete_selection()`
- **Clipboard:** `paste_clipboard()`, `paste_system_clipboard()`
- **Search:** `perform_search()`, `next_match()`, `prev_match()`
- **LSP:** `refresh_lsp()`, `request_hover/completion()`, `accept_completion()`, `dismiss_hover/completion()`
- **Rendering:** `get_styled_text()`, `update_scroll()`, `center_cursor()`, `refresh_diff()`
- **Git:** `is_dirty()`, `git_repo()`, `refresh_diff()`

---

## 3. TUI — `tui.rs`

**Purpose:** Terminal initialization and teardown wrapper.

**Key Type:**
- `Tui` — wraps `Terminal<CrosstermBackend<Stdout>>`

**Key Functions:**
- `Tui::new() -> Result<Self>` — enables raw mode, alternate screen
- `Tui::restore() -> Result<()>` — leaves alternate screen, disables raw mode (static)

---

## 4. Configuration — `config.rs`

**Purpose:** Load and parse TOML configuration files.

**Key Types:**
- `Config` — language definitions and grammar sources
- `LanguageConfig` — per-language: name, file_types, grammar, highlights, LSP
- `LspServerConfig` — LSP command and args
- `GrammarDef` — grammar name + GitHub source
- `ColorDef` — hex fg/bg + modifier list
- `ThemeConfig` — syntax + UI style overrides
- `KeybindingsConfig` — leader_keys flag

**Key Functions:**
- `load_config() -> Result<Config>` — loads `languages.toml`
- `load_theme_config() -> Option<ThemeConfig>` — loads `[theme]` from `config.toml`
- `load_keybindings_config() -> Option<KeybindingsConfig>` — loads `[keybindings]` from `config.toml`

---

## 5. Theme — `theme.rs`

**Purpose:** Define and resolve color/style definitions for syntax highlighting and UI chrome.

**Key Type:**
- `Theme` — `scopes: HashMap<String, Style>` + `ui: HashMap<String, Style>`

**Key Functions:**
- `Theme::default_theme() -> Self` — built-in defaults
- `Theme::apply_config(config: ThemeConfig)` — merge user overrides
- `Theme::style_for_capture(capture: &str) -> Style` — hierarchical scope resolution
- `Theme::ui_get(key: &str) -> Style` — UI element lookup

---

## 6. Syntax Highlighting — `syntax.rs`

**Purpose:** Tree-sitter based syntax highlighting engine.

**Key Types:**
- `GrammarLoader` — manages loaded .so grammar libraries
- `SyntaxHighlighter` — parser + queries + theme

**Key Functions:**
- `SyntaxHighlighter::new(theme) -> Self`
- `SyntaxHighlighter::load_language(name, path, query_source) -> Result<()>`
- `SyntaxHighlighter::load_language_for_path(file_path) -> Option<String>`
- `SyntaxHighlighter::highlight(source, lang) -> Vec<(usize, usize, Style)>`

---

## 7. LSP Client — `lsp.rs`

**Purpose:** Manage LSP server process lifecycle, JSON-RPC 2.0 messaging, diagnostics, and request/response handling.

**Key Types:**
- `LspSession` — active LSP server session
- `LspResponse` — enum: `Success(Value) | Error(Value)`
- `DiagnosticState` — diagnostics_by_line + counts
- `Diagnostic` — severity, message, source, positions
- `DiagnosticSeverity` — enum: `Error | Warning | Information | Hint`
- `CompletionItem` — LSP completion candidate
- `ResponseRegistry` — capped response cache (128 entries)
- `LspEvent` — internal: `Diagnostics(Vec<Diagnostic>) | Response(u64, LspResponse)`

**Key Functions:**
- `LspSession::start(config, root_dir, language_id, file_path, text) -> Result<Self>`
- `LspSession::did_change(text)` — sends full-text sync
- `LspSession::request_hover/completion(line, character) -> Result<u64>`
- `LspSession::take_response(request_id) -> Option<LspResponse>`
- `LspSession::poll()` — drain receiver channel
- `parse_completion_response(response) -> Result<Vec<CompletionItem>>`

---

## 8. Hover — `hover.rs`

**Purpose:** Hover documentation popup state and response parsing.

**Key Type:**
- `HoverState` — text, scroll, visible, pending_request

**Key Functions:**
- `HoverState::clear()`, `begin_request(id)`, `apply_response(id, response) -> bool`, `scroll_by(delta)`
- `parse_hover_response(response) -> Result<Option<String>, Value>`
- `normalize_markdown(input) -> String` — strips fenced blocks, formats links

---

## 9. Completion — `completion.rs`

**Purpose:** Autocompletion popup state and filtering.

**Key Type:**
- `CompletionState` — items, selected, visible, filtered_indices, scroll

**Constants:**
- `MAX_VISIBLE_ITEMS: usize = 10`

**Key Functions:**
- `CompletionState::clear()`, `begin_request(id, offset)`, `apply_response(id, response) -> bool`
- `CompletionState::select_next/prev()`, `filter(prefix)`, `active_item()`
- `strip_snippet(text) -> String` — removes LSP snippet placeholders
- `completion_insert_text(item) -> String` — resolves text to insert

---

## 10. Git — `git.rs`

**Purpose:** Git repository discovery, status, staging, committing, log, diff, and line-level diff computation.

**Key Types:**
- `GitRepo` — wraps `gix::Repository`
- `FileChange` — path, status, section (staged/unstaged)
- `ChangeStatus` — enum: Modified, Added, Untracked, Deleted, Renamed, Copied, Conflict, TypeChanged
- `ChangeSection` — enum: Staged, Unstaged
- `GitLogEntry` — short_hash, subject, author, date
- `DiffHunk` — line number + kind (Added/Removed/Modified)
- `DiffKind` — enum: Added, Removed, Modified

**Key Functions:**
- `GitRepo::discover(path) -> Option<Self>`
- `GitRepo::status() -> Result<Vec<FileChange>>`
- `GitRepo::stage/unstage(change)`, `commit(message)`
- `GitRepo::log(limit) -> Result<Vec<GitLogEntry>>`
- `GitRepo::diff_from_head(path) -> Result<String>`
- `GitRepo::diff_base(file) -> Option<Vec<u8>>`
- `compute_diff(base, modified) -> Vec<DiffHunk>` — line diff using imara_diff

---

## 11. Git Picker UI — `git_picker.rs`

**Purpose:** Terminal UI popup for interactive git operations.

**Key Types:**
- `GitPicker` — state machine for git popup
- `GitPage` — enum: `Status | Commit | Log | Diff`
- `StatusRow` — enum: `Header(String) | Entry(usize) | Text(String)`
- `Feedback` — message + is_error

**Key Functions:**
- `GitPicker::open(context_path) -> bool`
- `GitPicker::navigate_up/down()`, `enter() -> Option<PathBuf>`
- `GitPicker::toggle_stage()`, `begin_commit()`, `submit_commit()`
- `GitPicker::open_log()`, `open_diff()`, `back_to_status()`
- `GitPicker::scroll_page(delta)`, `scroll_viewport(direction)`
- `GitPicker::render(f, area)` — main render entry

---

## 12. File Explorer — `file_explorer.rs`

**Purpose:** Directory tree browser popup.

**Key Types:**
- `FileEntry` — name, path, is_dir
- `FileExplorer` — visible, current_dir, entries, filter, selection, scroll

**Key Functions:**
- `FileExplorer::toggle()`, `navigate_up/down()`, `enter() -> Option<PathBuf>`
- `FileExplorer::go_up()`, `add_filter_char(c)`, `remove_filter_char()`
- `FileExplorer::render(f, area)`

---

## 13. Fuzzy Finder — `fuzzy_finder.rs`

**Purpose:** Recursive fuzzy file search popup.

**Key Types:**
- `ScoredEntry` — path, display, is_dir, score, indices
- `FuzzyFinder` — visible, root_dir, all_entries, query, selection, scroll

**Key Functions:**
- `FuzzyFinder::toggle()`, `navigate_up/down()`, `enter() -> Option<PathBuf>`
- `FuzzyFinder::add_query_char(c)`, `remove_query_char()`, `go_up()`
- `FuzzyFinder::render(f, area)`

---

## 14. Fuzzy Matching — `fuzzy.rs`

**Purpose:** Pure fuzzy string matching algorithm.

**Key Function:**
- `fuzzy_score(query: &str, target: &str) -> Option<(i64, Vec<usize>)>`

**Scoring:**
- +10 per matched character
- +15 consecutive match bonus
- +20 separator/word-start bonus (/ - _ . space)
- -3 * gap_size penalty

---

## 15. Grammar Commands — `grammar_commands.rs`

**Purpose:** Tree-sitter grammar lifecycle management (CLI).

**Key Functions:**
- `fetch_grammars(config, runtime_path)` — downloads from GitHub
- `build_grammars(config, runtime_path)` — compiles .so files
- `update_grammars(config, runtime_path)` — fetch + build
- `create_default_config()` — writes default `languages.toml`
- `find_or_create_runtime() -> Result<PathBuf>`

---

## Component Dependency Hierarchy

```
main.rs (orchestrator)
├── tui.rs (terminal lifecycle)
├── editor.rs (core logic)
│   ├── syntax.rs (highlighting)
│   │   └── theme.rs (style resolution)
│   │       └── config.rs (user overrides)
│   ├── lsp.rs (LSP client)
│   ├── hover.rs (hover parsing)
│   ├── completion.rs (completion filtering)
│   ├── git.rs (repo ops + diff)
│   │   └── config.rs (LspServerConfig)
│   └── theme.rs (colors)
├── file_explorer.rs (popup)
│   └── theme.rs
├── fuzzy_finder.rs (popup)
│   └── fuzzy.rs (scoring)
│       └── theme.rs
├── git_picker.rs (popup)
│   ├── git.rs
│   └── theme.rs
├── config.rs (loading)
├── theme.rs (defaults)
└── grammar_commands.rs (CLI)
    └── config.rs
```
