# 03 — Behavior

> State machines, event flows, lifecycle, and behavioral patterns.

## 1. Modal State Machine

```
                ┌──────────────────────────────────────────────────┐
                │                   NORMAL                         │
                │  h/j/k/l: move     i/a: → INSERT                 │
                │  v: → VISUAL       V: → line VISUAL              │
                │  : (colon): → COMMAND                            │
                │  / (slash): → SEARCH                             │
                │  x: select line    p/P: paste                    │
                │  u: undo           U: redo                       │
                │  c: change         d: delete (cut)               │
                │  y/yY: yank        n/N: next/prev search         │
                │  g prefix: goto    Space: leader                 │
                └───────┬───────────┘ z: center cursor             │
                        │                                          │
          ┌─────────────┼─────────────┐                            │
          ▼             ▼             ▼                            │
    ┌──────────┐ ┌──────────┐ ┌──────────┐                        │
    │  INSERT  │ │ COMMAND  │ │  SEARCH  │                        │
    │          │ │          │ │          │                        │
    │ text     │ │ q → quit │ │ query    │                        │
    │ input    │ │ w → save │ │ Enter→   │                        │
    │          │ │ wq→s+quit│ │ search   │                        │
    │ Esc:→NORM│ │ git→gpop │ │ Esc:→NORM│                        │
    └────┬─────┘ └────┬─────┘ └────┬─────┘                        │
         │            │            │                              │
         └─────┬──────┴──────┬─────┘                              │
               │             │                                    │
               ▼             ▼                                    │
           ┌──────────────────────────────────────┐               │
           │               VISUAL                  │               │
           │  h/j/k/l: move selection             │               │
           │  w/b: word extend                    │               │
           │  x/X: line extend                    │               │
           │  y/yY: yank     d: delete (cut)      │               │
           │  c: change      Esc: → NORMAL        │               │
           └──────────────────────────────────────┘               │
                                                                   │
  ┌────────────────────────────────────────────────────────────────┘
  │
  ▼
FUZZY
  This is a "global" mode set when FuzzyFinder or GitPicker is active.
  During FUZZY mode, the editor does NOT process Normal-mode keys.
  The overlay components handle their own key events in main.rs.
```

### Mode Transition Rules

| From | To | Trigger | Side Effects |
|------|----|---------|--------------|
| Normal | Insert | `i` (before cursor), `I` (line start), `a` (after), `A` (line end), `o`/`O` (new line) | `begin_undo_group()` called |
| Normal | Visual | `v` (char-wise), `V` (line-wise) | `selection_anchor = cursor` |
| Normal | Command | `:` | `command_buffer.clear()` |
| Normal | Search | `/` | `search_query.clear()` |
| Normal | Fuzzy | `Ctrl+F` (finder), `Ctrl+G` (git) | overlay toggle |
| Insert | Normal | `Escape` | - |
| Command | Normal | `Escape` | `command_buffer.clear()` |
| Search | Normal | `Escape` | `search_query.clear()` |
| Visual | Normal | `Escape` | `selection_anchor = None` |
| Visual | Insert | `c` (change) | delete selection, undo group |

---

## 2. Application Lifecycle

```
┌─────────────────────────────────────────────────────────────────┐
│                     STARTUP                                      │
│                                                                  │
│  1. Parse CLI args                                               │
│     ├── --init → write default config, exit                      │
│     ├── --grammar fetch|build|update → run grammar mgmt, exit    │
│     └── path arg → determine file or directory                   │
│                                                                  │
│  2. Load configuration                                           │
│     ├── languages.toml → Config (grammars, LSP configs)          │
│     ├── config.toml → ThemeConfig (syntax + UI overrides)        │
│     └── config.toml → KeybindingsConfig (leader_keys flag)       │
│                                                                  │
│  3. Initialize subsystems                                        │
│     ├── Tui::new() → raw mode, alternate screen                  │
│     ├── Editor::new() → buffer, highlighter, LSP, git discovery  │
│     ├── FileExplorer::new()                                      │
│     ├── FuzzyFinder::new()                                       │
│     └── GitPicker::new()                                         │
│                                                                  │
│  4. Enter main loop                                              │
└─────────────────────────────────────────────────────────────────┘
                                                                  │
                    ┌─────────────────┐                           │
                    │  MAIN LOOP      │                           │
                    │  (100ms poll)    │ ◄────────────────────────┘
                    └────────┬────────┘
                             │
              ┌──────────────┼──────────────┐
              ▼              ▼              ▼
        ┌──────────┐  ┌────────────┐  ┌──────────┐
        │ Pre-     │  │ Render     │  │ Handle   │
        │ frame    │  │ (ratatui)  │  │ Events   │
        └──────────┘  └────────────┘  └──────────┘
              │              │              │
              └──────┬───────┘              │
                     │                      │
                     ▼                      │
               ┌──────────┐                │
               │ should    │──Yes──→ ┌──────────┐
               │ _quit?    │         │ SHUTDOWN │
               └──────────┘         └──────────┘
                     │                      │
                     │ No                   │
                     └──────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                     SHUTDOWN                                     │
│  1. Tui::restore() → leave alternate screen, disable raw mode    │
│  2. Process exits                                                │
└─────────────────────────────────────────────────────────────────┘
```

---

## 3. LSP Lifecycle

```
┌──────────┐     ┌──────────────┐     ┌─────────────────┐
│ Editor   │     │ LspSession   │     │ LSP Server      │
│          │     │              │     │ (subprocess)    │
└────┬─────┘     └──────┬───────┘     └────────┬────────┘
     │                  │                      │
     │  start(...)      │                      │
     │─────────────────>│                      │
     │                  │  spawn with args     │
     │                  │─────────────────────>│ (child process)
     │                  │                      │
     │                  │  Content-Length: N   │
     │                  │  {"jsonrpc":"2.0",   │
     │                  │   "method":"init...  │
     │                  │─────────────────────>│
     │                  │                      │
     │                  │  <── initialize result│
     │                  │                      │
     │                  │  "initialized"       │
     │                  │─────────────────────>│
     │                  │                      │
     │                  │  "didOpen"           │
     │                  │─────────────────────>│
     │                  │                      │
     │<──── LspSession  │                      │
     │       ready      │                      │
     │                  │                      │
     │  refresh_lsp()   │                      │
     │  (each frame)    │                      │
     │─────────────────>│                      │
     │                  │  if buffer changed:  │
     │                  │  "didChange" ───────>│
     │                  │                      │
     │                  │  poll()              │
     │                  │  ┌─ reader thread    │
     │                  │  │  reads stdout ───>│
     │                  │  │                   │
     │                  │  │  "publishDiag...  │
     │                  │  │  "─── from thread  │
     │                  │  │                   │
     │                  │  │  Response(id, ...)│
     │<── diagnostics,  │  │                   │
     │     responses    │  │                   │
     │                  │                      │
     │  request_hover() │                      │
     │─────────────────>│  "textDocument/      │
     │                  │   hover" ───────────>│
     │                  │                      │
     │  request_comp()  │                      │
     │─────────────────>│  "textDocument/      │
     │                  │   completion" ──────>│
     │                  │                      │
     │  (next frame)    │                      │
     │  refresh_lsp()   │                      │
     │─────────────────>│  poll() ─────────────│
     │                  │  <── response        │
     │<── apply response│                      │
     │                  │                      │
     │  did_change()    │  "didChange" ───────>│
     │─────────────────>│                      │
     │                  │                      │
     │  [session drops] │                      │
     │                  │  "exit" ────────────>│
     │                  │  kill()              │
     │                  │  (Drop impl)         │
```

---

## 4. Undo/Redo Behavior

```
Editor undo stack: Vec<(Rope, usize)>
Editor redo stack: Vec<(Rope, usize)>

Each entry is a snapshot of (buffer, cursor_position).

begin_undo_group():
  - Pushes current (buffer, cursor) onto undo_stack
  - Clears redo_stack (new action invalidates redo history)

undo():
  - Pops undo_stack → (rope, cursor)
  - Pushes current (buffer, cursor) onto redo_stack
  - Restores (rope, cursor)

redo():
  - Pops redo_stack → (rope, cursor)
  - Pushes current (buffer, cursor) onto undo_stack
  - Restores (rope, cursor)

Note: This is a snapshot-based undo system (full rope copy per entry),
not operation-log based. This is simpler but uses more memory for
large changes.
```

---

## 5. Search Behavior

```
1. User types '/' → mode = Search, search_query starts empty
2. Each character typed appends to search_query
3. Enter triggers perform_search():
   a. Build byte-to-char index map of buffer
   b. Case-insensitive find all occurrences of search_query
   c. Store result positions in search_results
   d. Set search_active = true
   e. Position cursor at first match at or after current cursor
4. n → next_match(): advance search_idx, jump cursor
5. N → prev_match(): recede search_idx, jump cursor
6. Matches are highlighted in rendering via search_highlight style
```

---

## 6. Completion Behavior

```
Trigger conditions (in Insert mode):
  - After a trigger character ('.' typically)
  - Manual trigger via keybinding
  - Each character typed (filter on prefix)

Flow:
1. Editor sends completion request to LSP
2. LSP returns CompletionItems
3. CompletionState stores items, applies fuzzy filter on prefix
4. Popup shows top 10 items (MAX_VISIBLE_ITEMS)
5. Tab/Enter: accept selected item
   - accept_completion() applies text edit or insert_text
6. Escape: dismiss completion
7. Arrow keys: navigate selection
8. Typing characters: re-filters items via fuzzy_score()
```

---

## 7. File Change Detection

```
On each save():
  - Store file modification time (file_mtime)
  - Store saved buffer version (saved_buffer_version)

On open_file():
  - Read current file mtime
  - If mtime differs from stored, set disk_changed = true
  - (UI can then warn user about external modification)
```

---

## 8. Cursor Centering (zz)

```
Behavior:
1. User presses 'z' (pending_z = true)
2. Next key must be 'z' (or timeout/cancel on other key)
3. center_cursor(height) calculates:
   scroll_y = cursor_line.saturating_sub(height / 2)
4. Clamped to [0, max_scroll]
```

---

## 9. Leader Key Behavior

```
Behavior:
1. User presses Space (pending_space = true)
2. Next key determines the leader command
3. Controlled by KeybindingsConfig.leader_keys flag
```

---

## 10. Auto-pairing Behavior

```
insert_char(c) {
    if c is an opening bracket/quote:
        - Find matching closing character
        - If next char is already the closing char → skip-over (cursor++)
        - Otherwise: insert both open + close, cursor between them
    if c is a closing bracket/quote:
        - If next char matches → skip-over
        - Otherwise: normal insert
}

delete_char() {
    - If cursor is between matched pair (e.g. |"" or |()):
      - Delete both characters
    - Otherwise: delete single char before cursor
}

split_bracket_pair_at_cursor() {
    - If cursor is between () or [] or {}:
      - Convert to multiline: (\n    \n)
      - Cursor placed on the new indented line
}
```

---

## 11. Paste Strategies

```
paste_clipboard():
  - If clipboard text ends with '\n' (linewise):
    - Paste after current line
  - Else (characterwise):
    - Insert at cursor position

paste_clipboard_after_selection():
  - Exit visual mode
  - Paste text after the selection end
```
