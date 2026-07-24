# 02 — Interactions

> Communication paths, data flow, and dependency wiring between components.

## 1. Main Event Loop Data Flow

```
┌─────────────────────────────────────────────────────────────────────┐
│                         main.rs Event Loop                          │
│                                                                     │
│  ┌─────────────┐     ┌──────────────┐     ┌────────────────────┐   │
│  │ Poll Event   │────>│ Route to     │────>│ Execute Action     │   │
│  │ (100ms t/o)  │     │ Handler      │     │ (mutate state)     │   │
│  └─────────────┘     └──────────────┘     └────────────────────┘   │
│                                                  │                  │
│  ┌─────────────┐     ┌──────────────┐           │                  │
│  │ Set Cursor   │<────│ Render All   │<──────────┘                  │
│  │ Position     │     │ (ratatui)    │                              │
│  └─────────────┘     └──────────────┘                              │
└─────────────────────────────────────────────────────────────────────┘
```

### Phase 1: Editor Pre-frame
1. `editor.refresh_lsp()` — syncs LSP changes, polls responses
2. Sets cursor style based on mode

### Phase 2: Rendering (ratatui `Frame`)
1. Draw editor background block
2. Layout: editor area | status bar
3. Sub-layout: diff gutter | line numbers | content
4. `editor.update_scroll(width, height)` — keep cursor visible
5. `editor.refresh_diff()` — recompute git diff markers
6. Render diff markers per line (+, -, ~)
7. Render line numbers
8. `editor.get_styled_text(start, height) -> (Text, start_line)` — produce styled text
9. Render content with scroll offset
10. Render status bar: mode | git branch | filename | cursor pos | LSP counts
11. Render popups (Priority: Completion > Hover > Diagnostic)
12. Render overlay popups (FileExplorer, FuzzyFinder, GitPicker)
13. Set terminal cursor position

### Phase 3: Event Handling
1. Poll keyboard event (100ms timeout)
2. Check overlay visibility in priority order:
   - FileExplorer visible → route to FileExplorer
   - GitPicker visible → route to GitPicker sub-page
   - FuzzyFinder visible → route to FuzzyFinder
3. If hover visible → handle Alt+J/K scroll, Escape dismiss
4. Check global shortcuts: Ctrl+P (explorer), Ctrl+F (fuzzy finder)
5. Dispatch to editor based on `mode`:
   - Normal → single-key commands
   - Insert → text input, completion, backspace, enter
   - Command → build command buffer, Enter to execute
   - Search → build search query, Enter to execute
   - Visual → movement, yank, delete, change
6. If cursor changed → dismiss hover popup
7. If `should_quit` → break loop

---

## 2. Editor ↔ LSP Communication

```
Editor                          LspSession                    LSP Server
  │                                │                              │
  │  refresh_lsp()                 │                              │
  │  ├── if buffer changed ───────>│  send didChange ────────────>│
  │  │                             │                              │
  │  │ poll() ────────────────────>│                              │
  │  │                             │  ┌─ diagnostics (from ──────>│
  │  │                             │  │  reader thread)           │
  │  │                             │  │                           │
  │  │<── update diagnostics ──────┘  │                           │
  │  │                                │                           │
  │  request_hover() ────────────────>│  send hover request ─────>│
  │  │                                │                           │
  │  │                 (later)        │                           │
  │  │  refresh_lsp() ───────────────>│  poll()                   │
  │  │                                │  ┌─ hover response ───────>│
  │  │<── apply hover response ──────┘  │                           │
  │  │                                │                           │
```

Key interaction points:
- `LspSession` runs a background reader thread that reads LSP stdout
- Reader thread sends `LspEvent::Diagnostics` and `LspEvent::Response` into an `mpsc::Receiver`
- `poll()` drains this channel and populates `DiagnosticState` and `ResponseRegistry`
- `refresh_lsp()` is called every frame, ensuring low-latency response to LSP events
- Hover/completion responses are stored by ID; stale responses (mismatched ID) are ignored via `ResponseRegistry`

---

## 3. Editor ↔ Git Interaction

```
Editor                    GitRepo                    GitPicker
  │                        │                            │
  │  new(file_path)        │                            │
  │  ├── discover() ──────>│                            │
  │  │                     │  gitoxide: walk up dirs    │
  │  │<── Option<Repo> ────│                            │
  │  │                     │                            │
  │  ├── diff_base() ─────>│                            │
  │  │                     │  gix: HEAD tree lookup     │
  │  │<── Option<Vec<u8>>  │                            │
  │  │                     │                            │
  │  refresh_diff()        │                            │
  │  ├── compute_diff()    │                            │
  │  │  (imara_diff)       │                            │
  │  │<── Vec<DiffHunk>    │                            │
  │  │                     │                            │
  │  Ctrl+G (GitPicker)    │                            │
  │  ├── open(context) ────────────────────────────────>│
  │  │                     │  discover() ──────────────>│
  │  │                     │                            │
  │  │  Space (stage)      │                            │
  │  │  ├── toggle_stage() ────────────────────────────>│
  │  │  │                  │  stage/unstage() ─────────>│
  │  │  │                  │  (git CLI)                 │
  │  │  │                  │<── Result                  │
  │  │  │<── refresh ──────│                            │
  │  │  │                  │                            │
  │  │  c (commit)         │                            │
  │  │  ├── begin_commit() ────────────────────────────>│
  │  │  │  Enter ──────────│  submit_commit() ─────────>│
  │  │  │                  │  commit(msg) ─────────────>│
  │  │  │                  │  (git CLI)                 │
```

---

## 4. Editor ↔ Syntax Highlighting

```
Editor                           SyntaxHighlighter
  │                                    │
  │  new(text, file_path, theme)       │
  │  ├── load_language_for_path() ────>│
  │  │                                 │  detect extension → language
  │  │                                 │  load grammar .so
  │  │                                 │  compile highlights.scm query
  │  │<── Option<String>               │
  │                                    │
  │  get_styled_text(start, height)    │
  │  ├── if buffer changed:            │
  │  │   highlight(buffer, lang) ─────>│
  │  │   │                             │  tree-sitter parse
  │  │   │                             │  query → captures
  │  │   │                             │  resolve capture → Style
  │  │   │<── Vec<(byte_start,         │
  │  │   │      byte_end, Style)>      │
  │  │                                 │
  │  ├── build char_styles[]           │
  │  │    syntax → search → selection →│
  │  │    diagnostics (underlines)     │
  │  │<── Text (ratatui)               │
```

---

## 5. Popup Priority & Dismissal

```
Rendering Priority (highest to lowest):
1. FileExplorer (Ctrl+P toggle)
2. GitPicker (Ctrl+G toggle)
3. FuzzyFinder (Ctrl+F toggle)
4. Completion popup (auto)
5. Hover popup (auto)
6. Diagnostic popup (auto)

Dismissal Rules:
- Hover dismissed on: cursor movement, Escape key, any edit
- Completion dismissed on: Escape, newline (if accepting), cursor movement
- FileExplorer dismissed on: file selection (enter), Escape
- FuzzyFinder dismissed on: file selection (enter), Escape
- GitPicker dismissed on: file selection (enter on file), Escape
- Diagnostic cycled with: Alt+J/K (next/prev on cursor line)
```

---

## 6. Keyboard Event Routing

```
                    ┌──────────────┐
                    │  Key Event   │
                    └──────┬───────┘
                           │
               ┌───────────┼───────────┐
               │           │           │
               ▼           ▼           ▼
        ┌──────────┐ ┌──────────┐ ┌──────────┐
        │Overlay   │ │Overlay   │ │Overlay   │
        │Visible?  │ │Hover?    │ │Fuzzy?    │
        └──────────┘ └──────────┘ └──────────┘
               │           │           │
        ┌──────┴────┐      │           │
        ▼           ▼      ▼           ▼
  ┌─────────┐ ┌────────┐ ┌──────┐ ┌──────────┐
  │FileExp  │ │Git     │ │Hover│ │Fuzzy     │
  │Routes   │ │Picker  │ │Cmds │ │Finder    │
  └─────────┘ └────────┘ └──────┘ └──────────┘
                           │
                           ▼
                    ┌──────────────┐
                    │ Editor Mode  │
                    │ Dispatch     │
                    └──────┬───────┘
                           │
               ┌───────┬───┼───┬───────┐
               ▼       ▼   ▼   ▼       ▼
           ┌──────┐ ┌────┐ ┌──┐ ┌──┐ ┌──────┐
           │Normal│ │Ins.│ │Cmd│ │Srch│ │Visual│
           └──────┘ └────┘ └──┘ └──┘ └──────┘
```

---

## 7. Popup Kind Priority

The `popup_kind()` function determines which info popup to show:

```rust
fn popup_kind(completion_visible, hover_visible, diagnostic_present) -> PopupKind {
    if completion_visible { PopupKind::Completion }
    else if hover_visible { PopupKind::Hover }
    else if diagnostic_present { PopupKind::Diagnostic }
    else { PopupKind::None }
}
```

This means only ONE info popup is shown at a time, with completion taking highest priority.
