# 05 — UI Map

> Screen layout, visual hierarchy, popup system, and rendering pipeline.

## 1. Screen Layout

```
┌─────────────────────────────────────────────────────────────┐
│  ┌───────────────────────────────────────────────────────┐  │
│  │                                                        │  │
│  │                  EDITOR AREA                           │  │
│  │  │ │ 1 │ use std::collections::HashMap;               │  │
│  │  │ │ 2 │                                               │  │
│  │  D│L │ 3 │ fn main() {                                │  │
│  │  I│I │ 4 │     let mut map = HashMap::new();           │  │
│  │  F│N│ 5 │     map.insert("hello", "world");           │  │
│  │  F│E│ 6 │     println!("{:?}", map);                  │  │
│  │    │  │ 7 │ }                                          │  │
│  │  + │ 8 │                                               │  │
│  │    │ 9 │                                               │  │
│  │    │   │                                               │  │
│  │                                                        │  │
│  └───────────────────────────────────────────────────────┘  │
│  ┌───────────────────────────────────────────────────────┐  │
│  │ NORMAL  │ main │ tedii/src/main.rs │ 5:20 │ E:1 W:2  │  │
│  └───────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

### Regions

| Region | Width | Content |
|--------|-------|---------|
| **Diff Gutter** | 1 col | `+`, `-`, `~` git diff markers, or space |
| **Line Numbers** | variable | 1-indexed line numbers, right-aligned, trailing space |
| **Content** | remaining | Styled text with syntax highlighting |
| **Status Bar** | full width | Mode indicator, git branch, filename, cursor pos, LSP counts |
| **Popups** | overlay | Centered/modal popups on top of editor |

---

## 2. Status Bar Layout

```
┌──────────────────────────────────────────────────────────────────┐
│ NORMAL  │ master │ * tedii/src/main.rs │ 42:15 │ E:1 W:2        │
└──────────────────────────────────────────────────────────────────┘

Segments (left to right):
1. Mode indicator: NORMAL | INSERT | VISUAL | COMMAND | SEARCH
2. Git branch (if in repo): main, master, feature-*, etc.
3. Filename with dirty indicator: * prefix if unsaved changes
4. Cursor position: line:col (1-indexed)
5. LSP counts: E:N (errors), W:N (warnings)
```

---

## 3. Popup System

### Popup Dimensions & Positioning

| Popup | Width | Height | Positioning |
|-------|-------|--------|-------------|
| File Explorer | 60% (min 40) | 55% (min 10) | Centered |
| Fuzzy Finder | 60% (min 40) | 55% (min 10) | Centered |
| Git Picker | 72% (min 40) | 68% (min 12) | Centered |
| Completion | auto (~80 chars max) | 10 items | At cursor? (inside editor area) |
| Hover | capped at 80 cols | max half height | Bottom-right anchored |
| Diagnostic | single line | 1 line | At cursor line? (inside editor) |

### Popup Rendering Priority (overlay order)

```
1. FileExplorer    (Ctrl+P)
2. GitPicker       (Ctrl+G)
3. FuzzyFinder     (Ctrl+F)
4. Completion      (auto)
5. Hover           (auto)
6. Diagnostic      (auto - inline, no overlay)
```

### Popup Common Pattern

All popups follow this pattern:
1. `Clear` widget to erase underlying content
2. `Block` with border and title
3. Internal layout (header, content, footer/hints)
4. Scrollable list with selection highlight
5. Status/hint bar at bottom

---

## 4. Completion Popup Layout

```
┌─────────────────────────────────┐
│ ● foo(x: i32)           Method  │  ← selected
│   bar(s: &str)          Function│
│   baz()                 Function│
│   qux(a: i32, b: i32)   Method  │
│   ...more items...              │
└─────────────────────────────────┘

- Max 10 visible items (MAX_VISIBLE_ITEMS)
- Selected item marked with ● or highlighted
- Shows label + detail (kind name)
```

---

## 5. Hover Popup Layout

```
┌──────────────────────────────────────────┐
│  fn foo(x: i32) -> String               │
│                                          │
│  This function takes an integer and      │
│  returns a string representation.        │
│                                          │
│  # Examples                              │
│                                          │
│  let s = foo(42);                        │
│  assert_eq!(s, "42");                    │
└──────────────────────────────────────────┘

- Capped at 80 columns wide
- Max height = terminal_height / 2
- Scrollable with Alt+J/K
- Shows normalized markdown (links converted, code blocks stripped)
```

---

## 6. File Explorer Layout

```
┌─────────────────────────────────────┐
│  File Explorer                      │
├─────────────────────────────────────┤
│  src/                               │  ← filter bar
├─────────────────────────────────────┤
│  ..                                 │  ← parent dir
│  src/                     📁        │  ← directories
│  Cargo.toml              📄         │  ← files (selected)
│  README.md               📄         │
│  target/                 📁         │
│  ─────────────────────────────────   │
│  Space: filter  Enter: open  Esc    │
└─────────────────────────────────────┘
```

---

## 7. Fuzzy Finder Layout

Same as File Explorer but:
- Shows recursive file search results
- Matched characters highlighted with `fuzzy_match` style
- Query typed in the filter bar, re-scores in real-time
- Directories can be descended into

---

## 8. Git Picker Layout

### Status Page
```
┌───────────────────────────────────────┐
│  Git Status                           │
├───────────────────────────────────────┤
│  Staged (2)                           │
│  M  src/main.rs                       │
│  A  src/new_file.rs                   │
│  Unstaged (3)                         │
│  M  src/editor.rs                     │   ← selected
│  ?  untracked.txt                     │
│  D  old_file.rs                       │
│  ───────────────────────────────────── │
│  Space stage/unstage  c commit  l log │
│  d diff  Enter open  Esc close        │
└───────────────────────────────────────┘
```

### Commit Page
```
┌───────────────────────────────────────┐
│  Commit Message                       │
├───────────────────────────────────────┤
│  fix: resolve cursor issue with       │
│  long lines                           │
│  ──── (cursor blinking here) ────     │
│  Enter commit  Esc cancel             │
└───────────────────────────────────────┘
```

### Log/Diff Page
```
┌───────────────────────────────────────┐
│  Log (last 100 commits)               │
├───────────────────────────────────────┤
│  a1b2c3d Fix cursor issue             │
│  e5f6g7a Add git integration          │  ← scrollable
│  h8i9j0k Initial commit               │
│  ...                                  │
│  j/k scroll  Ctrl+d/u page  Esc back  │
└───────────────────────────────────────┘
```

---

## 9. Rendering Pipeline (per frame)

```
1. Draw editor background (Theme::ui_get("editor_bg"))
2. Layout:
   a. Constraint::Length(1)  ← status bar gap
   b. Constraint::Min(0)     ← editor content
   c. Constraint::Length(1)  ← status bar
3. Editor sub-layout (horizontal):
   a. Constraint::Length(1)  ← diff gutter
   b. Constraint::Length(gutter_width) ← line numbers
   c. Constraint::Min(0)     ← content area
4. Update scroll (editor.update_scroll)
5. Refresh diff (editor.refresh_diff)
6. Render diff markers
7. Render line numbers
8. Get styled text (editor.get_styled_text)
9. Render content as Scrollbar with offset
10. Render status bar
11. Render overlay popups (if visible)
12. Set cursor position
```
