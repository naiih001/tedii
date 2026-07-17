# `zz` Center Cursor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `zz` command in normal mode that centers the viewport vertically on the cursor line, matching Vim's behavior.

**Architecture:** Add a `center_cursor()` method to the `Editor` struct that sets `scroll_y` to center the cursor line in the viewport. Wire it into key dispatch via a `pending_z` field, following the existing `pending_g`/`pending_space` pattern.

**Tech Stack:** Rust, ratatui, ropey

## Global Constraints

- Rust edition 2021, Cargo build system
- Editor cursor is a character index (`usize`), line/col computed via `buffer.char_to_line()`
- Scroll state lives in `Editor.scroll_x` and `Editor.scroll_y` fields
- All key handling is in `src/main.rs` inside a nested match in the event loop
- Tests use `Editor::new(text, None, theme, None)` pattern with `Theme::default_theme()`

---

### Task 1: Add `center_cursor()` method and tests

**Files:**
- Modify: `src/editor.rs` (struct definition at line 112, new method after `update_scroll` at line 1059, tests at end of test module)

**Interfaces:**
- Consumes: `self.cursor`, `self.buffer`, `self.scroll_y`
- Produces: Sets `self.scroll_y`; called with viewport `height` parameter

- [ ] **Step 1: Add `pending_z` field to the Editor struct**

In `src/editor.rs`, add `pub pending_z: bool,` after line 120 (`pub pending_space: bool,`):

```rust
pub pending_g: bool,
pub pending_space: bool,
pub pending_z: bool,
```

- [ ] **Step 2: Initialize `pending_z` in `Editor::new()`**

In `src/editor.rs`, add `pending_z: false,` after line 181 (`pending_space: false,`):

```rust
pending_g: false,
pending_space: false,
pending_z: false,
```

- [ ] **Step 3: Add `center_cursor()` method**

In `src/editor.rs`, add the method after `update_scroll()` (after line 1059, before the closing `}` of the `impl Editor` block):

```rust
pub fn center_cursor(&mut self, height: usize) {
    let line_idx = self.buffer.char_to_line(self.cursor);
    let line_count = self.buffer.len_lines();

    if height == 0 || line_count == 0 {
        return;
    }

    let half = height / 2;
    let new_scroll_y = line_idx.saturating_sub(half);
    let max_scroll = line_count.saturating_sub(height);
    self.scroll_y = new_scroll_y.min(max_scroll);
}
```

- [ ] **Step 4: Add tests for `center_cursor()`**

Add the following tests at the end of the `#[cfg(test)] mod tests` block in `src/editor.rs` (before the final `}`):

```rust
#[test]
fn center_cursor_sets_scroll_y_to_center() {
    let theme = Theme::default_theme();
    let text = (0..100).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
    let mut editor = Editor::new(&text, None, theme, None);
    editor.cursor = editor.buffer.line_to_char(50);
    editor.scroll_y = 0;

    editor.center_cursor(20);

    // cursor line 50, height 20, half = 10 → scroll_y = 50 - 10 = 40
    assert_eq!(editor.scroll_y, 40);
}

#[test]
fn center_cursor_clamps_to_zero_for_cursor_near_top() {
    let theme = Theme::default_theme();
    let text = (0..100).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
    let mut editor = Editor::new(&text, None, theme, None);
    editor.cursor = editor.buffer.line_to_char(2);
    editor.scroll_y = 0;

    editor.center_cursor(20);

    // cursor line 2, half = 10 → 2 - 10 saturates to 0
    assert_eq!(editor.scroll_y, 0);
}

#[test]
fn center_cursor_clamps_when_near_end_of_file() {
    let theme = Theme::default_theme();
    let text = (0..20).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
    let mut editor = Editor::new(&text, None, theme, None);
    editor.cursor = editor.buffer.line_to_char(19);
    editor.scroll_y = 0;

    editor.center_cursor(20);

    // line_count = 20, height = 20, max_scroll = 0
    assert_eq!(editor.scroll_y, 0);
}

#[test]
fn center_cursor_does_not_change_cursor_position() {
    let theme = Theme::default_theme();
    let text = (0..50).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
    let mut editor = Editor::new(&text, None, theme, None);
    let original_cursor = editor.buffer.line_to_char(25);
    editor.cursor = original_cursor;

    editor.center_cursor(20);

    assert_eq!(editor.cursor, original_cursor);
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --lib editor::tests::center_cursor`
Expected: 4 tests pass

- [ ] **Step 6: Commit**

```bash
git add src/editor.rs
git commit -m "feat: add center_cursor method to Editor"
```

---

### Task 2: Wire `zz` into key dispatch

**Files:**
- Modify: `src/main.rs` (normal mode key handling, lines 419-548)

**Interfaces:**
- Consumes: `editor.pending_z`, `editor.center_cursor(height)`, `viewport_height` (already computed at line 188)
- Produces: Scroll-to-center on `zz` keypress

- [ ] **Step 1: Add `pending_z` handler in the pending keys section**

In `src/main.rs`, after the `pending_space` block (after line 460, before `} else {` on line 461), add a new `else if editor.pending_z` block:

```rust
} else if editor.pending_z {
    editor.pending_z = false;
    match key.code {
        KeyCode::Char('z') => editor.center_cursor(viewport_height),
        _ => {}
    }
} else {
```

The full pending keys section should now read:

```rust
if editor.pending_g {
    match key.code {
        KeyCode::Char('g') | KeyCode::Char('k') => {
            editor.move_to_start()
        }
        KeyCode::Char('e') | KeyCode::Char('j') => editor.move_to_end(),
        KeyCode::Char('h') => editor.move_to_line_start(),
        KeyCode::Char('l') => editor.move_to_line_end(),
        _ => {}
    }
    editor.pending_g = false;
} else if editor.pending_space {
    editor.pending_space = false;
    match key.code {
        KeyCode::Char('e') => {
            fuzzy_finder.visible = false;
            file_explorer.toggle();
        }
        KeyCode::Char('f') => {
            file_explorer.visible = false;
            fuzzy_finder.toggle();
            editor.mode = Mode::Fuzzy;
        }
        KeyCode::Char('g') => {
            file_explorer.visible = false;
            fuzzy_finder.visible = false;
            refresh_git(&mut git_picker);
            if !git_picker.is_empty() {
                git_picker.visible = true;
                editor.mode = Mode::Fuzzy;
            }
        }
        KeyCode::Char('w') if leader_keys_enabled => {
            let _ = editor.save();
        }
        KeyCode::Char('q') if leader_keys_enabled => {
            editor.should_quit = true;
        }
        _ => {}
    }
} else if editor.pending_z {
    editor.pending_z = false;
    match key.code {
        KeyCode::Char('z') => editor.center_cursor(viewport_height),
        _ => {}
    }
} else {
```

- [ ] **Step 2: Add `z` key to the normal mode key match**

In `src/main.rs`, add `KeyCode::Char('z') => editor.pending_z = true,` in the normal mode key match. Place it after line 529 (`KeyCode::Char(' ') => editor.pending_space = true,`):

```rust
KeyCode::Char('g') => editor.pending_g = true,
KeyCode::Char(' ') => editor.pending_space = true,
KeyCode::Char('z') => editor.pending_z = true,
```

- [ ] **Step 3: Build and verify compilation**

Run: `cargo build`
Expected: Builds successfully with no errors

- [ ] **Step 4: Run full test suite**

Run: `cargo test`
Expected: All tests pass (including the 4 new `center_cursor` tests)

- [ ] **Step 5: Commit**

```bash
git add src/main.rs
git commit -m "feat: add zz keybind to center cursor vertically"
```
