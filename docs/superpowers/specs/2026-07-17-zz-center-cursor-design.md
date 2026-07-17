# Design: `zz` — Center Cursor Vertically

## Summary

Add a `zz` command in normal mode that repositions the viewport so the cursor's line is vertically centered in the window. This mirrors Vim's `zz` scroll command.

## Behavior

- Pressing `zz` in normal mode scrolls the viewport vertically so the cursor's line appears at the vertical center of the editor window.
- The cursor does **not** move — only the viewport scroll position (`scroll_y`) changes.
- Horizontal scrolling (`scroll_x`) is unaffected.
- Does not accept a count prefix (kept simple for now).

## Vim Reference

From `:help scroll.txt`:

> **`z.`** — Redraw, line `[count]` at center of window (default cursor line). Put cursor at first non-blank in the line.
> **`zz`** — Like `z.`, but leave the cursor in the same column.

Related commands (not implemented in this design, but for reference):
- `zt` — cursor line at top of window
- `zb` — cursor line at bottom of window

## Architecture

### Changes to `src/editor.rs`

1. **New field on `Editor`:**
   - `pending_z: bool` — tracks whether the user has pressed `z` and is waiting for the second key in normal mode. Initialized to `false`.

2. **New method `center_cursor(&mut self, height: usize)`:**
   - Computes cursor line: `self.buffer.char_to_line(self.cursor)`
   - Sets `scroll_y = cursor_line.saturating_sub(height / 2)`
   - Clamps `scroll_y` so `scroll_y + height <= line_count` (with a floor of 0 via `saturating_sub`)
   - Does not modify `scroll_x` or `cursor`

3. **Tests:**
   - Test that `center_cursor` sets `scroll_y` correctly for a cursor in the middle of a file
   - Test that `center_cursor` clamps to 0 for a cursor near the top
   - Test that `center_cursor` clamps so the viewport doesn't extend past the end
   - Test that `center_cursor` does not change the cursor position

### Changes to `src/main.rs`

1. **Normal mode key dispatch:**
   - Add `KeyCode::Char('z') => { editor.pending_z = true; }` in the normal mode key handler

2. **Pending z handling:**
   - Add a `pending_z` branch in the pending keys section (similar to `pending_g` and `pending_space`):
     - When `pending_z` is true and `z` is pressed again → call `editor.center_cursor(viewport_height)`, reset `pending_z = false`
     - Any other key resets `pending_z = false` (no action taken for non-z second key)

## Edge Cases

- **Cursor at top of file:** `scroll_y` clamps to 0; cursor appears below center. This is correct — there are not enough lines above to center.
- **Cursor at bottom of file:** `scroll_y` clamps so the viewport does not extend past the last line. Cursor appears above center.
- **File smaller than viewport:** `scroll_y` stays 0; the entire content is visible.
- **After `zz`, cursor moves:** `update_scroll()` resumes minimal edge-based scrolling, which is the expected Vim behavior — `zz` is a one-shot viewport reposition.

## What This Design Does Not Cover

- `zt` / `zb` (top/bottom centering) — future work
- `z.` / `z<CR>` / `z-` (center/top/bottom with cursor reposition) — future work
- Count prefix (`5zz`) — future work
- Horizontal scroll centering — out of scope
