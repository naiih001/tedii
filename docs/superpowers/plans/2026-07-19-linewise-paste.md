# Linewise Paste Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Paste linewise clipboard content below the full selected line or block in both normal and visual modes.

**Architecture:** Keep paste placement inside `Editor`. Before linewise content is pasted, derive the lower endpoint from `get_selection_range()` and move the cursor there; then reuse the existing trailing-newline-aware `paste_text()` path. Key dispatch only selects the internal or system clipboard operation and changes modes.

**Tech Stack:** Rust 2021, Ropey, Crossterm, Ratatui, inline Rust unit tests

## Global Constraints

- Direct visual paste does not replace or delete the selected text.
- Direct visual paste clears the selection and returns to normal mode.
- Upward and downward multiline selections paste below the lower selected line.
- Characterwise clipboard content retains cursor-position insertion.
- Empty or unavailable system clipboard content remains a no-op.
- Do not add clipboard-type metadata or new dependencies.

---

### Task 1: Selection-Aware Editor Paste

**Files:**
- Modify: `src/editor.rs:990-1090`
- Test: `src/editor.rs` inline `tests` module

**Interfaces:**
- Consumes: `Editor::get_selection_range() -> Option<(usize, usize)>`, `Editor::paste_text(&mut self, text: &str)`
- Produces: `Editor::paste_clipboard_after_selection(&mut self)`, `Editor::paste_system_clipboard_after_selection(&mut self)`

- [ ] **Step 1: Write failing tests for yank-then-paste placement**

Add these tests to the `src/editor.rs` test module:

```rust
#[test]
fn yanked_line_pastes_below_selected_line() {
    let theme = Theme::default_theme();
    let mut editor = Editor::new("first\nsecond", None, theme, None);
    editor.cursor = 2;
    editor.select_line();

    editor.yank_selection();
    editor.paste_clipboard();

    assert_eq!(editor.buffer.to_string(), "first\nfirst\nsecond");
}

#[test]
fn upward_line_selection_pastes_below_entire_selection() {
    let theme = Theme::default_theme();
    let mut editor = Editor::new("first\nsecond\nthird\nfourth", None, theme, None);
    editor.cursor = 15;
    editor.select_line();
    editor.extend_selection_up();

    editor.yank_selection();
    editor.paste_clipboard();

    assert_eq!(
        editor.buffer.to_string(),
        "first\nsecond\nthird\nsecond\nthird\nfourth"
    );
}
```

- [ ] **Step 2: Run the yank-then-paste tests and verify they fail**

Run:

```bash
cargo test yanked_line_pastes_below_selected_line -- --nocapture
cargo test upward_line_selection_pastes_below_entire_selection -- --nocapture
```

Expected: both fail because `yank_selection()` leaves the cursor at the active endpoint instead of the selection's lower endpoint.

- [ ] **Step 3: Write failing tests for direct visual paste**

Add these tests:

```rust
#[test]
fn direct_linewise_paste_inserts_below_selection_and_clears_it() {
    let theme = Theme::default_theme();
    let mut editor = Editor::new("first\nsecond", None, theme, None);
    editor.cursor = 2;
    editor.select_line();
    editor.clipboard = "copied\n".into();

    editor.paste_clipboard_after_selection();

    assert_eq!(editor.buffer.to_string(), "first\ncopied\nsecond");
    assert_eq!(editor.selection_anchor, None);
}

#[test]
fn direct_characterwise_paste_keeps_active_cursor_position() {
    let theme = Theme::default_theme();
    let mut editor = Editor::new("first\nsecond", None, theme, None);
    editor.cursor = 2;
    editor.select_line();
    editor.clipboard = "X".into();

    editor.paste_clipboard_after_selection();

    assert_eq!(editor.buffer.to_string(), "firstX\nsecond");
    assert_eq!(editor.selection_anchor, None);
}
```

- [ ] **Step 4: Run the direct-paste tests and verify they fail to compile**

Run:

```bash
cargo test direct_linewise_paste -- --nocapture
cargo test direct_characterwise_paste -- --nocapture
```

Expected: compilation fails because `paste_clipboard_after_selection()` does not exist.

- [ ] **Step 5: Implement selection-aware paste positioning**

Add the following helpers and public operations around the existing clipboard methods:

```rust
fn position_linewise_paste_after_selection(&mut self, text: &str) {
    if !text.ends_with('\n') {
        return;
    }

    if let Some((_, selection_end)) = self.get_selection_range() {
        self.cursor = selection_end.saturating_sub(1);
    }
}

fn paste_text_after_selection(&mut self, text: &str) {
    self.position_linewise_paste_after_selection(text);
    self.exit_visual_mode();
    self.paste_text(text);
}
```

Update both yank methods so linewise text leaves the cursor at the selection's lower endpoint before clearing the selection:

```rust
pub fn yank_selection(&mut self) {
    if let Some(text) = self.get_selected_text() {
        self.position_linewise_paste_after_selection(&text);
        self.clipboard = text;
    }
    self.exit_visual_mode();
}

pub fn yank_selection_system(&mut self) {
    if let Some(text) = self.get_selected_text() {
        self.position_linewise_paste_after_selection(&text);
        self.clipboard = text.clone();
        if let Ok(mut clipboard) = arboard::Clipboard::new() {
            let _ = clipboard.set_text(text);
        }
    }
    self.exit_visual_mode();
}
```

Add the direct visual paste methods:

```rust
pub fn paste_clipboard_after_selection(&mut self) {
    let text = self.clipboard.clone();
    self.paste_text_after_selection(&text);
}

pub fn paste_system_clipboard_after_selection(&mut self) {
    if let Ok(mut clipboard) = arboard::Clipboard::new() {
        if let Ok(text) = clipboard.get_text() {
            self.clipboard = text.clone();
            self.paste_text_after_selection(&text);
        }
    }
}
```

- [ ] **Step 6: Run focused editor tests**

Run:

```bash
cargo test yanked_line_pastes_below_selected_line -- --nocapture
cargo test upward_line_selection_pastes_below_entire_selection -- --nocapture
cargo test direct_linewise_paste -- --nocapture
cargo test direct_characterwise_paste -- --nocapture
```

Expected: all focused tests pass.

- [ ] **Step 7: Commit the editor behavior**

```bash
git add src/editor.rs
git commit -m "fix: paste line selections below selected lines"
```

### Task 2: Visual-Mode Paste Key Routing

**Files:**
- Modify: `src/main.rs:920-950`

**Interfaces:**
- Consumes: `Editor::paste_clipboard_after_selection()`, `Editor::paste_system_clipboard_after_selection()`
- Produces: visual-mode `p` and `P` key behavior

- [ ] **Step 1: Route visual-mode paste keys**

Add these match arms before the visual-mode delete/change operations:

```rust
KeyCode::Char('p') => {
    editor.paste_clipboard_after_selection();
    editor.mode = Mode::Normal;
}
KeyCode::Char('P') => {
    editor.paste_system_clipboard_after_selection();
    editor.mode = Mode::Normal;
}
```

- [ ] **Step 2: Run the complete test suite**

Run:

```bash
cargo test
```

Expected: all tests pass with zero failures.

- [ ] **Step 3: Check formatting and whitespace**

Run:

```bash
git diff --check
cargo fmt --check
```

Expected: `git diff --check` passes. If `cargo fmt --check` reports the repository's existing unrelated formatting drift, confirm the new or modified lines introduce no additional formatter differences and do not mass-format unrelated files.

- [ ] **Step 4: Commit key routing**

```bash
git add src/main.rs
git commit -m "feat: paste directly from line selection"
```
