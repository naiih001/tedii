# LSP Hover Documentation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add nonblocking `Space k` LSP hover documentation with a reusable bounded JSON-RPC response registry, readable plain-text content, and a scrollable popup.

**Architecture:** `src/lsp.rs` owns generic request-response transport and a bounded response registry. A new `src/hover.rs` owns hover parsing, Markdown normalization, and popup state. `src/editor.rs` coordinates requests and stale-response protection, while `src/main.rs` owns key dispatch and Ratatui layout.

**Tech Stack:** Rust 2021, Ratatui 0.29, Crossterm 0.28, Ropey 1.6, Serde JSON 1.0.

## Global Constraints

- Trigger hover only with `Space k` in Normal mode.
- Keep LSP requests asynchronous; never block the render/input loop waiting for hover.
- Retain at most 128 completed JSON-RPC responses and evict the oldest first.
- Only the response matching the editor's current pending hover request may update hover state.
- Render readable plain text without adding a Markdown dependency.
- `Alt-j` and `Alt-k` scroll; `Esc`, cursor movement, editing, file changes, and LSP restarts dismiss hover.
- Hover replaces the diagnostic popup while visible.
- Popup outer width is capped at 80 columns and the editor content width.
- Popup outer height is capped at half the editor content height.
- Preserve existing formatter baseline; do not reformat unrelated code.

---

### Task 1: Generic JSON-RPC Response Registry

**Files:**
- Modify: `src/lsp.rs`
- Test: inline `src/lsp.rs` tests

**Interfaces:**
- Produces: `pub enum LspResponse { Success(serde_json::Value), Error(serde_json::Value) }`
- Produces: `LspSession::take_response(&mut self, request_id: u64) -> Option<LspResponse>`
- Produces: `LspSession::request_hover(&mut self, line: usize, character: usize) -> anyhow::Result<u64>`
- Keeps: `send_request(&mut self, method: &str, params: Value) -> Result<u64>`

- [ ] **Step 1: Add failing registry tests**

Add a private `ResponseRegistry` test contract to `src/lsp.rs`:

```rust
#[test]
fn response_registry_takes_response_once() {
    let mut registry = ResponseRegistry::default();
    registry.insert(7, LspResponse::Success(json!({"value": 1})));

    assert_eq!(
        registry.take(7),
        Some(LspResponse::Success(json!({"value": 1})))
    );
    assert_eq!(registry.take(7), None);
}

#[test]
fn response_registry_evicts_oldest_response() {
    let mut registry = ResponseRegistry::default();
    for id in 0..=MAX_COMPLETED_RESPONSES as u64 {
        registry.insert(id, LspResponse::Success(json!(id)));
    }

    assert_eq!(registry.take(0), None);
    assert_eq!(
        registry.take(MAX_COMPLETED_RESPONSES as u64),
        Some(LspResponse::Success(json!(MAX_COMPLETED_RESPONSES)))
    );
}

#[test]
fn parses_success_and_error_responses() {
    assert_eq!(
        parse_response(&json!({"id": 4, "result": {"contents": "docs"}})),
        Some((4, LspResponse::Success(json!({"contents": "docs"}))))
    );
    assert_eq!(
        parse_response(&json!({"id": 5, "error": {"code": -32603, "message": "failed"}})),
        Some((
            5,
            LspResponse::Error(json!({"code": -32603, "message": "failed"}))
        ))
    );
    assert_eq!(
        parse_response(&json!({
            "id": 6,
            "method": "workspace/configuration",
            "params": {}
        })),
        None
    );
}
```

- [ ] **Step 2: Run the tests and verify RED**

Run:

```bash
cargo test lsp::tests::response_registry
cargo test lsp::tests::parses_success_and_error_responses
```

Expected: compilation fails because `ResponseRegistry`, `LspResponse`, `MAX_COMPLETED_RESPONSES`, and `parse_response` do not exist.

- [ ] **Step 3: Implement the response types and bounded registry**

Add imports and definitions near the existing LSP event types:

```rust
use std::collections::{HashMap, VecDeque};

const MAX_COMPLETED_RESPONSES: usize = 128;

#[derive(Debug, Clone, PartialEq)]
pub enum LspResponse {
    Success(serde_json::Value),
    Error(serde_json::Value),
}

#[derive(Default)]
struct ResponseRegistry {
    responses: HashMap<u64, LspResponse>,
    order: VecDeque<u64>,
}

impl ResponseRegistry {
    fn insert(&mut self, id: u64, response: LspResponse) {
        if self.responses.insert(id, response).is_some() {
            self.order.retain(|stored_id| *stored_id != id);
        }
        self.order.push_back(id);

        while self.responses.len() > MAX_COMPLETED_RESPONSES {
            if let Some(oldest_id) = self.order.pop_front() {
                self.responses.remove(&oldest_id);
            }
        }
    }

    fn take(&mut self, id: u64) -> Option<LspResponse> {
        let response = self.responses.remove(&id)?;
        self.order.retain(|stored_id| *stored_id != id);
        Some(response)
    }
}

fn parse_response(value: &serde_json::Value) -> Option<(u64, LspResponse)> {
    let id = value.get("id")?.as_u64()?;
    if let Some(error) = value.get("error") {
        return Some((id, LspResponse::Error(error.clone())));
    }
    let result = value.get("result")?;
    Some((
        id,
        LspResponse::Success(result.clone()),
    ))
}
```

Change the event to the following and add `responses: ResponseRegistry` after `request_id` in the current `LspSession` struct:

```rust
enum LspEvent {
    Diagnostics(Vec<Diagnostic>),
    Response(u64, LspResponse),
}

// Add to LspSession immediately after request_id.
responses: ResponseRegistry,
```

Initialize `responses: ResponseRegistry::default()`. In `poll`, insert every response into the registry. In `wait_for_response`, return success only for the expected `LspResponse::Success`, and return `anyhow::bail!("LSP request failed: {}", error)` for the expected error response. Insert unrelated response IDs into the registry so startup cannot discard responses owned by another request. The expected initialization response is consumed directly and never inserted.

Update `read_messages` to use:

```rust
if let Some((id, response)) = parse_response(&value) {
    let _ = tx.send(LspEvent::Response(id, response));
}
```

Expose:

```rust
pub fn take_response(&mut self, request_id: u64) -> Option<LspResponse> {
    self.responses.take(request_id)
}

pub fn request_hover(&mut self, line: usize, character: usize) -> Result<u64> {
    self.send_request(
        "textDocument/hover",
        json!({
            "textDocument": { "uri": self.current_uri },
            "position": {
                "line": line,
                "character": character,
            }
        }),
    )
}
```

- [ ] **Step 4: Run focused and full tests**

Run:

```bash
cargo test lsp::tests
cargo test
```

Expected: all registry/parser tests and the existing suite pass.

- [ ] **Step 5: Commit Task 1**

```bash
git add src/lsp.rs
git commit -m "refactor: retain LSP request responses"
```

---

### Task 2: Hover Content Parser and State

**Files:**
- Create: `src/hover.rs`
- Modify: `src/main.rs`
- Test: inline `src/hover.rs` tests

**Interfaces:**
- Consumes: `crate::lsp::LspResponse`
- Produces: `pub struct HoverState`
- Produces: `pub fn parse_hover_response(response: LspResponse) -> Result<Option<String>, serde_json::Value>`
- Produces: `HoverState::{clear, begin_request, apply_response, scroll_by}`

- [ ] **Step 1: Add `mod hover` and failing parser/state tests**

Add `mod hover;` beside the other modules in `src/main.rs`. Create `src/hover.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::LspResponse;
    use serde_json::json;

    #[test]
    fn parses_markup_and_marked_string_hover_content() {
        assert_eq!(
            parse_hover_response(LspResponse::Success(json!({
                "contents": {
                    "kind": "markdown",
                    "value": "**Vec** stores `snake_case` items.\n\n```rust\nlet v = Vec::new();\n```"
                }
            }))),
            Ok(Some("Vec stores snake_case items.\n\nlet v = Vec::new();".into()))
        );

        assert_eq!(
            parse_hover_response(LspResponse::Success(json!({
                "contents": [
                    {"language": "rust", "value": "fn len(&self) -> usize"},
                    "Returns [the length](https://example.test/len)."
                ]
            }))),
            Ok(Some(
                "fn len(&self) -> usize\n\nReturns the length (https://example.test/len).".into()
            ))
        );
    }

    #[test]
    fn null_empty_and_error_hover_responses_do_not_open() {
        assert_eq!(
            parse_hover_response(LspResponse::Success(serde_json::Value::Null)),
            Ok(None)
        );
        assert_eq!(
            parse_hover_response(LspResponse::Success(json!({"contents": ""}))),
            Ok(None)
        );
        assert!(parse_hover_response(LspResponse::Error(json!({
            "code": -32603,
            "message": "failed"
        })))
        .is_err());
    }

    #[test]
    fn stale_response_is_ignored_and_scrolling_is_clamped() {
        let mut hover = HoverState::default();
        hover.begin_request(9);

        assert!(!hover.apply_response(
            8,
            LspResponse::Success(json!({"contents": "old"}))
        ));
        assert!(!hover.visible);

        assert!(hover.apply_response(
            9,
            LspResponse::Success(json!({"contents": "new"}))
        ));
        assert_eq!(hover.text, "new");
        assert!(hover.visible);

        hover.max_scroll = 3;
        hover.scroll_by(5);
        assert_eq!(hover.scroll, 3);
        hover.scroll_by(-2);
        assert_eq!(hover.scroll, 1);
        hover.clear();
        assert_eq!(hover, HoverState::default());
    }
}
```

- [ ] **Step 2: Run the hover tests and verify RED**

Run:

```bash
cargo test hover::tests
```

Expected: compilation fails because hover parsing and state APIs are not implemented.

- [ ] **Step 3: Implement plain-text normalization**

Implement these exact public types and methods:

```rust
use crate::lsp::LspResponse;
use serde_json::Value;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct HoverState {
    pub text: String,
    pub scroll: u16,
    pub max_scroll: u16,
    pub visible: bool,
    pub pending_request: Option<u64>,
}

impl HoverState {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn begin_request(&mut self, request_id: u64) {
        self.clear();
        self.pending_request = Some(request_id);
    }

    pub fn apply_response(&mut self, request_id: u64, response: LspResponse) -> bool {
        if self.pending_request != Some(request_id) {
            return false;
        }
        self.pending_request = None;
        match parse_hover_response(response) {
            Ok(Some(text)) => {
                self.text = text;
                self.visible = true;
                true
            }
            Ok(None) | Err(_) => {
                self.clear();
                false
            }
        }
    }

    pub fn scroll_by(&mut self, delta: i16) {
        self.scroll = (self.scroll as i32 + delta as i32)
            .clamp(0, self.max_scroll as i32) as u16;
    }
}
```

Implement `parse_hover_response` by extracting `contents` from success values. Support strings, `{ "language", "value" }`, `{ "kind", "value" }`, and arrays. Join array entries with blank lines.

Implement normalization without a dependency:

```rust
pub fn parse_hover_response(response: LspResponse) -> Result<Option<String>, Value> {
    let value = match response {
        LspResponse::Success(value) => value,
        LspResponse::Error(error) => return Err(error),
    };
    if value.is_null() {
        return Ok(None);
    }
    let Some(contents) = value.get("contents") else {
        return Ok(None);
    };
    let normalized = parse_contents(contents)
        .map(|text| normalize_markdown(&text))
        .unwrap_or_default();
    let normalized = normalized.trim().to_string();
    Ok((!normalized.is_empty()).then_some(normalized))
}

fn parse_contents(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Array(items) => {
            let parts = items
                .iter()
                .filter_map(parse_marked_string)
                .filter(|part| !part.trim().is_empty())
                .collect::<Vec<_>>();
            (!parts.is_empty()).then(|| parts.join("\n\n"))
        }
        Value::Object(_) => parse_marked_string(value),
        _ => None,
    }
}

fn parse_marked_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Object(object) => object
            .get("value")
            .and_then(Value::as_str)
            .map(str::to_string),
        _ => None,
    }
}

fn normalize_markdown(input: &str) -> String {
    input
        .lines()
        .filter(|line| !line.trim_start().starts_with("```"))
        .map(normalize_inline)
        .collect::<Vec<_>>()
        .join("\n")
}

fn normalize_inline(input: &str) -> String {
    let chars = input.chars().collect::<Vec<_>>();
    let mut output = String::new();
    let mut index = 0;

    while index < chars.len() {
        if chars[index] == '[' {
            if let Some(label_end_offset) = chars[index + 1..].iter().position(|ch| *ch == ']') {
                let label_end = index + 1 + label_end_offset;
                if chars.get(label_end + 1) == Some(&'(') {
                    if let Some(url_end_offset) =
                        chars[label_end + 2..].iter().position(|ch| *ch == ')')
                    {
                        let url_end = label_end + 2 + url_end_offset;
                        output.extend(chars[index + 1..label_end].iter());
                        output.push_str(" (");
                        output.extend(chars[label_end + 2..url_end].iter());
                        output.push(')');
                        index = url_end + 1;
                        continue;
                    }
                }
            }
        }

        if chars[index] == '`' {
            index += 1;
            continue;
        }

        if index + 1 < chars.len()
            && matches!(
                (chars[index], chars[index + 1]),
                ('*', '*') | ('_', '_')
            )
        {
            index += 2;
            continue;
        }

        if chars[index] == '*' {
            index += 1;
            continue;
        }

        if chars[index] == '_' {
            let previous_is_alphanumeric = index
                .checked_sub(1)
                .and_then(|previous| chars.get(previous))
                .is_some_and(|ch| ch.is_alphanumeric());
            let next_is_alphanumeric = chars
                .get(index + 1)
                .is_some_and(|ch| ch.is_alphanumeric());
            if !(previous_is_alphanumeric && next_is_alphanumeric) {
                index += 1;
                continue;
            }
        }

        output.push(chars[index]);
        index += 1;
    }

    output
}
```

`normalize_markdown` must:

- Drop lines whose trimmed value starts with `` ``` ``.
- Remove backticks and paired `**` / `__` delimiters.
- Remove a single `*` or `_` only when it is an emphasis boundary: one adjacent character is alphanumeric and the other is absent, whitespace, or punctuation.
- Preserve `_` when both adjacent characters are alphanumeric, so identifiers such as `snake_case` remain intact.
- Convert `[label](url)` to `label (url)` using a character scan, not a regex dependency.
- Preserve all other newlines.

- [ ] **Step 4: Run focused and full tests**

Run:

```bash
cargo test hover::tests
cargo test
```

Expected: all hover parser/state tests and the existing suite pass.

- [ ] **Step 5: Commit Task 2**

```bash
git add src/hover.rs src/main.rs
git commit -m "feat: parse LSP hover content"
```

---

### Task 3: Editor Hover Request Lifecycle

**Files:**
- Modify: `src/editor.rs`
- Modify: `src/lsp.rs`
- Test: inline `src/editor.rs` and `src/lsp.rs` tests

**Interfaces:**
- Consumes: `HoverState`, `LspSession::request_hover`, `LspSession::take_response`
- Produces: `Editor::request_hover(&mut self)`
- Produces: `Editor::dismiss_hover(&mut self)`
- Produces: `Editor::scroll_hover(&mut self, delta: i16)`
- Produces: private `cursor_lsp_position(buffer: &Rope, cursor: usize) -> (usize, usize)`

- [ ] **Step 1: Add failing UTF-16 and lifecycle tests**

Add to `src/editor.rs` tests:

```rust
#[test]
fn cursor_lsp_position_uses_utf16_code_units() {
    let buffer = Rope::from_str("a😀b\n");

    assert_eq!(cursor_lsp_position(&buffer, 0), (0, 0));
    assert_eq!(cursor_lsp_position(&buffer, 1), (0, 1));
    assert_eq!(cursor_lsp_position(&buffer, 2), (0, 3));
    assert_eq!(cursor_lsp_position(&buffer, 3), (0, 4));
}

#[test]
fn dismiss_hover_clears_visible_and_pending_state() {
    let mut editor = Editor::new("", None, Theme::default_theme(), None);
    editor.hover.text = "docs".into();
    editor.hover.visible = true;
    editor.hover.pending_request = Some(12);

    editor.dismiss_hover();

    assert_eq!(editor.hover, HoverState::default());
}

#[test]
fn editing_clears_hover_before_lsp_refresh() {
    let mut editor = Editor::new("a", None, Theme::default_theme(), None);
    editor.hover.text = "docs".into();
    editor.hover.visible = true;
    editor.insert_char('b');

    assert!(!editor.hover.visible);
    assert_eq!(editor.hover.pending_request, None);
}

#[test]
fn lsp_restart_and_file_open_clear_hover() {
    let mut editor = Editor::new("", None, Theme::default_theme(), None);
    editor.hover.text = "docs".into();
    editor.hover.visible = true;
    editor.restart_lsp(Path::new("main.rs"));
    assert_eq!(editor.hover, HoverState::default());

    let path = std::env::temp_dir().join(format!(
        "tedii-hover-{}-{}.rs",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    std::fs::write(&path, "fn main() {}\n").unwrap();
    editor.hover.text = "docs".into();
    editor.hover.visible = true;
    editor.open_file(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(editor.hover, HoverState::default());
}
```

Add a request-body unit test in `src/lsp.rs` by extracting:

```rust
fn hover_params(uri: &str, line: usize, character: usize) -> serde_json::Value
```

and asserting:

```rust
#[test]
fn hover_params_include_uri_and_utf16_position() {
    assert_eq!(
        hover_params("file:///tmp/main.rs", 3, 7),
        json!({
            "textDocument": { "uri": "file:///tmp/main.rs" },
            "position": { "line": 3, "character": 7 }
        })
    );
}
```

- [ ] **Step 2: Run tests and verify RED**

Run:

```bash
cargo test editor::tests::cursor_lsp_position
cargo test editor::tests::dismiss_hover
cargo test editor::tests::editing_clears_hover
cargo test editor::tests::lsp_restart_and_file_open_clear_hover
cargo test lsp::tests::hover_params
```

Expected: compilation fails because the new editor state and helper methods do not exist.

- [ ] **Step 3: Integrate hover state into the editor**

Import `HoverState` and add `pub hover: HoverState` immediately after `pub lsp_cursor_index` in the current `Editor` struct:

```rust
use crate::hover::HoverState;

// Add to Editor immediately after lsp_cursor_index.
pub hover: HoverState,
```

Initialize `hover: HoverState::default()`.

Add:

```rust
fn cursor_lsp_position(buffer: &Rope, cursor: usize) -> (usize, usize) {
    let cursor = cursor.min(buffer.len_chars());
    let line = buffer.char_to_line(cursor);
    let line_start = buffer.line_to_char(line);
    let utf16_character = buffer
        .slice(line_start..cursor)
        .chars()
        .map(char::len_utf16)
        .sum();
    (line, utf16_character)
}

pub fn request_hover(&mut self) {
    let (line, character) = cursor_lsp_position(&self.buffer, self.cursor);
    let Some(session) = self.lsp_session.as_mut() else {
        self.hover.clear();
        return;
    };
    match session.request_hover(line, character) {
        Ok(request_id) => self.hover.begin_request(request_id),
        Err(error) => {
            crate::lsp::log_line(format!("[editor] hover request failed: {}", error));
            self.hover.clear();
        }
    }
}

pub fn dismiss_hover(&mut self) {
    self.hover.clear();
}

pub fn scroll_hover(&mut self, delta: i16) {
    self.hover.scroll_by(delta);
}
```

In `refresh_lsp`, clear hover before sending `didChange`. After `session.poll()`, if `hover.pending_request` is present, call `session.take_response(id)` and pass a returned response to `hover.apply_response(id, response)`. Log error responses before applying them.

Call `self.hover.clear()` in `restart_lsp` and `open_file`. This also covers LSP restarts and file changes.

Update `request_hover` in `src/lsp.rs` to call a tested helper:

```rust
fn hover_params(uri: &str, line: usize, character: usize) -> serde_json::Value {
    json!({
        "textDocument": { "uri": uri },
        "position": { "line": line, "character": character }
    })
}
```

- [ ] **Step 4: Run focused and full tests**

Run:

```bash
cargo test editor::tests
cargo test lsp::tests
cargo test
```

Expected: all tests pass.

- [ ] **Step 5: Commit Task 3**

```bash
git add src/editor.rs src/lsp.rs
git commit -m "feat: request hover documentation"
```

---

### Task 4: Hover Popup, Keybindings, and Theme

**Files:**
- Modify: `src/main.rs`
- Modify: `src/theme.rs`
- Modify: `README.md`
- Test: inline `src/main.rs` tests or extracted pure layout helpers

**Interfaces:**
- Consumes: `Editor::{request_hover, dismiss_hover, scroll_hover}` and `Editor::hover`
- Produces: `hover_popup_metrics(text: &str, area: Rect) -> Option<HoverPopupMetrics>`
- Produces: `hover_border` and `hover_text` theme keys

- [ ] **Step 1: Add failing popup metric tests**

Extract a pure helper in `src/main.rs` and add tests:

```rust
#[derive(Debug, PartialEq, Eq)]
struct HoverPopupMetrics {
    area: ratatui::layout::Rect,
    max_scroll: u16,
}

#[derive(Debug, PartialEq, Eq)]
enum PopupKind {
    None,
    Diagnostic,
    Hover,
}

#[test]
fn hover_popup_is_capped_and_reports_scroll_range() {
    let area = ratatui::layout::Rect::new(10, 5, 100, 20);
    let text = (0..20)
        .map(|index| format!("line {index}"))
        .collect::<Vec<_>>()
        .join("\n");

    let metrics = hover_popup_metrics(&text, area).unwrap();

    assert!(metrics.area.width <= 80);
    assert!(metrics.area.height <= 10);
    assert_eq!(metrics.area.x + metrics.area.width, area.x + area.width);
    assert_eq!(metrics.area.y + metrics.area.height, area.y + area.height);
    assert!(metrics.max_scroll > 0);
}

#[test]
fn hover_popup_returns_none_for_unusable_area_or_empty_text() {
    assert_eq!(
        hover_popup_metrics("", ratatui::layout::Rect::new(0, 0, 80, 20)),
        None
    );
    assert_eq!(
        hover_popup_metrics("docs", ratatui::layout::Rect::new(0, 0, 2, 2)),
        None
    );
}

#[test]
fn hover_has_popup_precedence_and_cursor_changes_dismiss_it() {
    assert_eq!(popup_kind(true, true), PopupKind::Hover);
    assert_eq!(popup_kind(false, true), PopupKind::Diagnostic);
    assert_eq!(popup_kind(false, false), PopupKind::None);
    assert!(cursor_changed(4, 5));
    assert!(!cursor_changed(4, 4));
}
```

- [ ] **Step 2: Run popup tests and verify RED**

Run:

```bash
cargo test hover_popup
```

Expected: compilation fails because popup metrics do not exist.

- [ ] **Step 3: Implement popup metrics and rendering**

Implement metrics using character counts:

```rust
fn popup_kind(hover_visible: bool, diagnostic_present: bool) -> PopupKind {
    if hover_visible {
        PopupKind::Hover
    } else if diagnostic_present {
        PopupKind::Diagnostic
    } else {
        PopupKind::None
    }
}

fn cursor_changed(before: usize, after: usize) -> bool {
    before != after
}

fn hover_popup_metrics(text: &str, area: ratatui::layout::Rect) -> Option<HoverPopupMetrics> {
    if text.trim().is_empty() || area.width < 3 || area.height < 3 {
        return None;
    }
    let longest = text.lines().map(|line| line.chars().count()).max().unwrap_or(0);
    let width = ((longest + 2) as u16).clamp(3, area.width.min(80));
    let inner_width = width.saturating_sub(2).max(1) as usize;
    let wrapped_lines = text
        .lines()
        .map(|line| line.chars().count().max(1).div_ceil(inner_width))
        .sum::<usize>()
        .max(1);
    let height_cap = (area.height / 2).max(3).min(area.height);
    let height = ((wrapped_lines + 2) as u16).clamp(3, height_cap);
    let inner_height = height.saturating_sub(2) as usize;
    let max_scroll = wrapped_lines.saturating_sub(inner_height) as u16;
    Some(HoverPopupMetrics {
        area: ratatui::layout::Rect {
            x: area.x + area.width - width,
            y: area.y + area.height - height,
            width,
            height,
        },
        max_scroll,
    })
}
```

Before rendering diagnostics, match on `popup_kind(editor.hover.visible, editor.active_diagnostic().is_some())`. In the `PopupKind::Hover` arm, render:

```rust
editor.hover.max_scroll = metrics.max_scroll;
editor.hover.scroll = editor.hover.scroll.min(metrics.max_scroll);
let block = ratatui::widgets::Block::bordered()
    .border_style(editor.theme.ui_get("hover_border"));
let popup = Paragraph::new(editor.hover.text.clone())
    .style(editor.theme.ui_get("hover_text"))
    .block(block)
    .wrap(ratatui::widgets::Wrap { trim: false })
    .scroll((editor.hover.scroll, 0));
f.render_widget(popup, metrics.area);
```

In the `PopupKind::Diagnostic` arm, render the existing diagnostic popup. `PopupKind::None` renders neither popup.

- [ ] **Step 4: Add keys and cursor-dismissal**

Before ordinary editor key dispatch, handle visible hover controls:

```rust
if editor.hover.visible
    && key.code == KeyCode::Char('j')
    && key.modifiers == KeyModifiers::ALT
{
    editor.scroll_hover(1);
    continue;
}
if editor.hover.visible
    && key.code == KeyCode::Char('k')
    && key.modifiers == KeyModifiers::ALT
{
    editor.scroll_hover(-1);
    continue;
}
if editor.hover.visible && key.code == KeyCode::Esc {
    editor.dismiss_hover();
    continue;
}
```

Inside the existing `pending_space` Normal-mode branch, add:

```rust
KeyCode::Char('k') => editor.request_hover(),
```

Immediately before dispatching a regular editor-mode key, capture:

```rust
let cursor_before = editor.cursor;
```

After dispatch, use:

```rust
if cursor_changed(cursor_before, editor.cursor) {
    editor.dismiss_hover();
}
```

Do not dismiss for `Alt-j` or `Alt-k`, because those branches continue before normal dispatch.

- [ ] **Step 5: Add theme keys and README documentation**

Add to `default_ui()` in `src/theme.rs`:

```rust
("hover_border", Color::White, Color::Reset),
("hover_text", Color::White, Color::Reset),
```

Add to the README feature list:

```markdown
- LSP hover documentation with `Space k`; scroll with `Alt-j` / `Alt-k`.
```

- [ ] **Step 6: Run focused and full verification**

Run:

```bash
cargo test hover_popup
cargo test
cargo clippy --all-targets -- -D warnings -A clippy::collapsible_match
git diff --check
```

Expected: all tests pass, scoped Clippy passes, and `git diff --check` reports no whitespace errors.

Run:

```bash
cargo fmt -- --check
```

Expected baseline: this may still report the repository's existing unrelated formatting drift. Confirm no newly added lines appear in the formatter diff; do not format unrelated files.

- [ ] **Step 7: Commit Task 4**

```bash
git add README.md src/main.rs src/theme.rs
git commit -m "feat: show scrollable LSP hover docs"
```

---

### Task 5: Final Integration Review

**Files:**
- Review: `src/lsp.rs`
- Review: `src/hover.rs`
- Review: `src/editor.rs`
- Review: `src/main.rs`
- Review: `src/theme.rs`
- Review: `README.md`

**Interfaces:**
- Verifies all interfaces produced by Tasks 1-4.
- Produces no new behavior unless a regression test first demonstrates a defect.

- [ ] **Step 1: Exercise the complete automated suite**

```bash
cargo test
cargo clippy --all-targets -- -D warnings -A clippy::collapsible_match
git diff --check
git status --short
```

Expected: tests and scoped Clippy pass, no whitespace errors exist, and only intended files are modified.

- [ ] **Step 2: Perform manual LSP acceptance checks**

Run Tedii with a configured language server and verify:

1. Place the cursor on a documented symbol and press `Space k`; the popup opens without freezing input.
2. Confirm Markdown markers and fence delimiters are absent while code and paragraphs remain readable.
3. Use `Alt-j` and `Alt-k`; scrolling clamps at both ends.
4. Press `Esc`; hover closes and any diagnostic popup beneath it becomes visible.
5. Reopen hover and move with `h`, `j`, `k`, or `l`; hover closes.
6. Trigger two hover requests quickly on different symbols; only the latest result appears.
7. Trigger hover where the server returns `null`; no empty popup appears.
8. Edit the buffer, open another file, and restart the LSP; stale hover content does not remain.

- [ ] **Step 3: Request code review and fix findings test-first**

Review specifically for:

- Unbounded response retention.
- Initialization responses leaking into the registry.
- Stale hover responses reopening closed content.
- Incorrect UTF-16 positions.
- Popup dimensions under small terminal sizes.
- Input conflicts with existing diagnostic `Alt-j` / `Alt-k` behavior.

For any defect, add a failing regression test, run it to confirm RED, implement the minimal fix, and rerun the full suite.

- [ ] **Step 4: Commit integration fixes only if needed**

If review required changes:

```bash
git add README.md src/lsp.rs src/hover.rs src/editor.rs src/main.rs src/theme.rs
git commit -m "fix: harden LSP hover documentation"
```

If no changes were required, do not create an empty commit.
