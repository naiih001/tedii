# 04 — Data Model

> Core data structures, type hierarchy, and invariants.

## 1. Text Buffer

```rust
// ropey::Rope — the underlying text storage
// - Provides O(log n) insertion, deletion, and indexing
// - Chunk-based representation (B-tree of string chunks)

pub struct Editor {
    pub buffer: Rope,           // The text buffer
    pub cursor: usize,          // Character index into buffer
    pub scroll_x: usize,        // Horizontal scroll (characters)
    pub scroll_y: usize,        // Vertical scroll (lines)
    pub buffer_version: u64,    // Monotonic version counter
    pub saved_buffer_version: u64, // Version at last save
    pub disk_changed: bool,     // External modification flag
    pub file_mtime: Option<SystemTime>, // File timestamp
}
```

### Version Counter Invariants
- `buffer_version` increments on every `insert_char()`, `delete_char()`, `insert_tab()`, undo, redo, `open_file()`
- `saved_buffer_version` = `buffer_version` after `save()`
- `is_dirty()` = `buffer_version != saved_buffer_version`
- Cached highlights and diffs are recomputed only when `buffer_version` changes

---

## 2. Cursor & Selection

```rust
pub struct Editor {
    pub cursor: usize,                // 0-based char index
    pub selection_anchor: Option<usize>, // Visual mode anchor point
    pub mode: Mode,                   // Current modal state
}

// Selection range computed as:
pub fn get_selection_range(&self) -> Option<(usize, usize)> {
    self.selection_anchor.map(|anchor| {
        let start = anchor.min(self.cursor);
        let end = anchor.max(self.cursor);
        // end is exclusive (end+1 of last selected char)
        (start, end)
    })
}
```

---

## 3. Undo/Redo Stacks

```rust
pub struct Editor {
    pub undo_stack: Vec<(Rope, usize)>,  // (buffer_snapshot, cursor)
    pub redo_stack: Vec<(Rope, usize)>,  // (buffer_snapshot, cursor)
}
```

Snapshot-based: each entry copies the entire rope and cursor position.

---

## 4. Search State

```rust
pub struct Editor {
    pub search_query: String,       // Current search text
    pub search_results: Vec<usize>, // Char indices of all matches
    pub search_active: bool,        // Show/hide search highlights
    pub search_idx: usize,          // Current match index
}
```

---

## 5. Clipboard

```rust
pub struct Editor {
    pub clipboard: String, // Internal clipboard for yank/paste
}
// System clipboard via arboard crate (optional, fallible)
```

---

## 6. Completion State

```rust
pub struct CompletionState {
    pub items: Vec<CompletionItem>,         // All items from LSP
    pub selected: usize,                    // Selected index in filtered_indices
    pub visible: bool,                      // Show popup
    pub pending_request: Option<u64>,       // Outstanding request ID
    pub trigger_offset: usize,              // Buffer offset at trigger
    pub prefix: String,                     // Current filter prefix
    pub filtered_indices: Vec<usize>,       // Indices into items
    pub scroll_offset: usize,               // Scroll in filtered list
}

pub struct CompletionItem {
    pub label: String,
    pub kind: Option<u64>,
    pub detail: Option<String>,
    pub insert_text: Option<String>,
    pub sort_text: Option<String>,
    pub filter_text: Option<String>,
    pub text_edit_range: Option<(usize, usize, usize, usize)>,
    pub text_edit_new_text: Option<String>,
    pub preselect: bool,
    pub original_index: usize,
}
```

---

## 7. Hover State

```rust
pub struct HoverState {
    pub text: String,              // Parsed hover content
    pub scroll: u16,               // Current scroll offset
    pub max_scroll: u16,           // Maximum scroll (set externally)
    pub visible: bool,             // Show popup
    pub pending_request: Option<u64>, // Outstanding request ID
}
```

---

## 8. LSP State

```rust
pub struct DiagnosticState {
    pub diagnostics_by_line: HashMap<usize, Vec<Diagnostic>>,
    pub error_count: usize,
    pub warning_count: usize,
}

pub struct Diagnostic {
    pub severity: DiagnosticSeverity, // Error/Warning/Information/Hint
    pub message: String,
    pub source: Option<String>,
    pub line: usize,
    pub character: usize,
    pub end_line: usize,
    pub end_character: usize,
}

pub struct LspSession {
    pub session_id: u64,
    pub language_id: String,
    pub command: String,           // LSP server binary
    pub root_dir: PathBuf,
    pub file_path: PathBuf,
    pub stdin: Arc<Mutex<ChildStdin>>,
    pub rx: mpsc::Receiver<LspEvent>,
    pub child: Child,
    pub diagnostics: DiagnosticState,
    pub current_uri: String,
    pub request_id: u64,
    pub responses: ResponseRegistry, // Capped at 128
    pub document_version: i32,
}

enum LspEvent {
    Diagnostics(Vec<Diagnostic>),
    Response(u64, LspResponse),
}

pub enum LspResponse {
    Success(Value),
    Error(Value),
}
```

---

## 9. Git State

```rust
pub struct GitRepo {
    pub repo: gix::Repository,
    pub work_dir: PathBuf,
}

pub struct FileChange {
    pub path: PathBuf,
    pub original_path: Option<PathBuf>,
    pub section: ChangeSection,  // Staged | Unstaged
    pub status: ChangeStatus,
}

pub enum ChangeStatus {
    Modified, Added, Untracked, Deleted,
    Renamed, Copied, Conflict, TypeChanged,
}

pub struct GitLogEntry {
    pub short_hash: String,
    pub subject: String,
    pub author: String,
    pub date: String,
}

pub struct DiffHunk {
    pub line: u32,        // 0-based line number in modified text
    pub kind: DiffKind,   // Added | Removed | Modified
}
```

---

## 10. Git Picker State

```rust
pub struct GitPicker {
    pub visible: bool,
    pub repo: Option<GitRepo>,
    pub entries: Vec<FileChange>,
    pub selection: usize,
    pub scroll: usize,
    pub page: GitPage,           // Status | Commit | Log | Diff
    pub page_title: String,
    pub page_lines: Vec<String>,
    pub page_scroll: usize,
    pub viewport_rows: usize,
    pub commit_message: String,
    pub feedback: Option<Feedback>,
    pub theme: Theme,
}
```

---

## 11. Cached Rendering State

```rust
pub struct Editor {
    // Cached string for highlight/diff
    pub cached_text: String,
    
    // Syntax highlight cache
    pub cached_highlights: Vec<(usize, usize, Style)>,
    pub cached_highlight_version: u64,
    
    // Per-character style array (reused across renders)
    pub cached_char_styles: Vec<Style>,
}
```

Invariant: `cached_highlight_version == buffer_version` implies `cached_highlights` and `cached_char_styles` are up to date.

---

## 12. Theme Data Model

```rust
pub struct Theme {
    pub scopes: HashMap<String, Style>,  // syntax capture → Style
    pub ui: HashMap<String, Style>,      // UI element → Style
}

// Scope resolution: "keyword.control.import" → "keyword.control" → "keyword"
pub fn style_for_capture(&self, capture: &str) -> Style
```

---

## 13. Configuration Data Model

```rust
pub struct Config {
    pub languages: Vec<LanguageConfig>,
    pub grammars: Vec<GrammarDef>,
}

pub struct LanguageConfig {
    pub name: String,
    pub file_types: Vec<String>,
    pub grammar: String,
    pub highlights: Option<PathBuf>,
    pub lsp: Option<LspServerConfig>,
}

pub struct LspServerConfig {
    pub command: String,
    pub args: Vec<String>,
}

pub struct GrammarDef {
    pub name: String,
    pub source: String,  // GitHub "owner/repo" or URL
}
```

---

## 14. Popup Types (External)

```rust
// FileExplorer
pub struct FileExplorer {
    pub visible: bool,
    pub current_dir: PathBuf,
    pub selection: usize,
    pub filter: String,
    // ...
}

// FuzzyFinder
pub struct FuzzyFinder {
    pub visible: bool,
    pub root_dir: PathBuf,
    pub selection: usize,
    pub query: String,
    // ...
}

// main.rs - Popup routing
pub enum PopupKind {
    None,
    Diagnostic,
    Hover,
    Completion,
}
```
