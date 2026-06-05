# Data Structures

## Buffer Management (Rope)
- Use `ropey` or equivalent crate for underlying text representation.
- Provides efficient editing of large files.
- Metadata: Cursor position, selection range, change tracking.

## Undo/Redo Stack
- Command-based undo/redo (action history).
- Snapshotting vs. operation logs: Operation logs preferred for memory efficiency.
