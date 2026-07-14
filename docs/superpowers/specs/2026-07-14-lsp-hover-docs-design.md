# LSP Hover Documentation Design

## Goal

Add keyboard-triggered LSP hover documentation to the editor without blocking input or coupling hover handling directly to the LSP reader. The implementation will also establish a reusable request-response path for future LSP features.

## User Interaction

- `Space K` in Normal mode requests hover documentation for the symbol under the cursor.
- Hover content appears in a bordered popup in the lower-right of the editor viewport.
- The popup takes precedence over the existing diagnostic popup. Closing hover allows the diagnostic popup to appear again.
- `Alt-j` and `Alt-k` scroll the hover content by one rendered line.
- `Esc` closes the popup without changing editor mode.
- Any action that changes the cursor closes the popup so documentation cannot remain visible for the wrong symbol.
- Editing, opening another file, restarting the LSP session, and sending a new hover request also clear the current hover content.

## LSP Response Registry

`LspEvent::Response` will carry the response ID and a typed `LspResponse` value with `Success(serde_json::Value)` and `Error(serde_json::Value)` variants. `LspSession` will store completed responses in a `HashMap<u64, LspResponse>` and track insertion order in a `VecDeque<u64>`.

The request API will continue returning the generated ID. Consumers will retain IDs for requests they own and retrieve completed responses with `take_response(request_id: u64) -> Option<LspResponse>`. Retrieval removes the response ID from both the map and insertion-order queue.

The registry will retain at most 128 completed responses. When inserting beyond the limit, it will evict the oldest response. This prevents forgotten or abandoned requests from causing unbounded memory growth while keeping the mechanism generic enough for later completion, definition, references, and signature-help features.

Initialization will use the same response representation but will consume its expected response while waiting for startup. Initialization responses must not leak into the completed-response registry.

## Hover Request Lifecycle

The editor will track one pending hover request ID. `Space K` will:

1. Clear the currently displayed hover and reset its scroll offset.
2. Convert the current Rope cursor position to a zero-based LSP line and UTF-16 character offset.
3. Send `textDocument/hover` with the current document URI and position.
4. Store the returned request ID.

During the existing LSP refresh cycle, the editor will check the registry for its pending ID. Only that exact response may update hover state. A newer hover request replaces the pending ID, so late responses from older requests cannot reopen stale documentation.

A successful non-empty response opens the popup. A `null` result, JSON-RPC error, malformed payload, unavailable LSP session, or empty normalized content leaves the popup closed. Request failures, response errors, and malformed payloads are written to the existing LSP log.

## Hover Content

The parser will support the standard LSP hover content shapes:

- `MarkupContent` objects with `kind` and `value`.
- A single `MarkedString`.
- Arrays of `MarkedString` values.
- `null` results.

Markdown content will be normalized to readable plain text without adding a Markdown dependency:

- Preserve paragraph boundaries and meaningful line breaks.
- Preserve fenced code contents while removing fence delimiter lines and optional language labels.
- Remove common emphasis markers and inline-code backticks.
- Convert Markdown links to `label (URL)`.
- Join `MarkedString` arrays with blank lines.
- Preserve plain-text content as provided.

Unsupported or malformed fragments will be ignored rather than rendered as raw JSON.

## Popup Layout

The popup will reuse the editor's existing Ratatui rendering path and popup vocabulary.

- The outer width is the normalized content width plus borders, capped at 80 columns and the editor content width.
- The outer height is the wrapped line count plus borders, capped at half the editor content height with a minimum usable height of three rows when the viewport permits it.
- Text wraps within the popup.
- Vertical scrolling is clamped to the available wrapped content.
- If all content fits, `Alt-j` and `Alt-k` are no-ops.
- The popup is rendered instead of, not alongside, the diagnostic popup.

## Cursor Dismissal

Input handling will record the cursor position before dispatching an editor key and compare it afterward. If the cursor changed, visible hover content and the pending hover ID are cleared.

This central comparison avoids adding hover-specific calls to every movement, search, and navigation method. Explicit non-keyboard transitions such as opening a file or restarting an LSP session will clear hover state directly.

## Theme

The hover popup will use new `hover_border` and `hover_text` UI theme keys. Both default to white foreground with a reset background. Because they are part of the default theme, missing custom overrides automatically retain these defaults.

## Testing

Tests will cover:

- Successful and error JSON-RPC responses retaining their IDs and payloads.
- Destructive response retrieval by ID.
- Eviction of the oldest response when the 128-entry limit is exceeded.
- Initialization consuming its own response without registry leakage.
- Hover request payloads using the current URI and correct UTF-16 positions, including non-BMP characters.
- Parsing each supported hover content shape.
- Markdown normalization for paragraphs, fenced code, inline code, emphasis, and links.
- Empty, `null`, malformed, and error responses leaving hover closed.
- A stale response not replacing the latest pending hover.
- Scroll bounds and no-op scrolling when content fits.
- `Esc`, cursor movement, editing, file changes, and LSP restarts clearing hover state.
- Hover popup precedence over diagnostics.

## Out of Scope

- Mouse hover.
- Automatic requests after cursor idle time.
- Full Markdown rendering or syntax highlighting inside the popup.
- Multiple simultaneous hover popups.
- Pinning hover documentation while navigating.
