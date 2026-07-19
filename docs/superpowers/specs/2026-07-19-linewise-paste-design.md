# Linewise Paste Design

## Goal

Make text copied from a line selection paste immediately below the selected
line or selected block.

## Behavior

- `x`, then `y`, then `p` pastes the yanked line or lines below the last
  selected line.
- `x`, then `Y`, then `P` applies the same placement to the system clipboard.
- Pressing `p` or `P` while a line selection is active pastes below the
  selection without replacing the selected text.
- Direct visual-mode paste exits visual mode after insertion.
- Multiline selections extended upward or downward always paste below the
  selection's lower boundary, regardless of which endpoint holds the cursor.
- Characterwise clipboard content keeps the existing cursor-position paste
  behavior.

## Design

The editor will derive the paste reference position from the current selection
bounds before the selection is cleared. For linewise content, the lower
selection endpoint becomes the reference cursor, allowing the existing
newline-aware paste logic to insert at the start of the following line.

Yanking a line selection will leave the cursor at the selection's lower
endpoint so a subsequent normal-mode paste uses the same placement. Direct
visual-mode paste will use a dedicated editor operation that moves to the lower
selection endpoint, clears the selection, and invokes the existing internal or
system clipboard paste path.

Linewise content continues to be identified by a trailing newline, matching the
editor's current paste behavior. This avoids adding clipboard-type metadata and
preserves existing behavior for characterwise text.

## Error Handling

- Empty clipboard content remains a no-op.
- If the system clipboard cannot be read, the editor remains unchanged.
- Paste at the end of the file appends the linewise content.

## Testing

Unit tests in `src/editor.rs` will cover:

- line selection, yank, then internal clipboard paste;
- upward multiline line selection, yank, then paste below the full selection;
- direct paste while a line selection is active;
- direct visual paste exits visual mode by clearing the selection;
- characterwise clipboard text retains cursor-position insertion;
- system clipboard handling remains delegated to the existing clipboard path,
  since external clipboard availability is environment-dependent.
