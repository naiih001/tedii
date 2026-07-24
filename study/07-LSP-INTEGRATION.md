# 07 — LSP Integration

> LSP client architecture, protocol flow, message lifecycle.

## 1. Architecture Overview

```
┌─────────────┐     stdio      ┌──────────────┐
│  LspSession │◄─── stdin/stdout ──►│  LSP Server   │
│  (main      │                │  (subprocess) │
│   thread)   │                │               │
└──────┬──────┘                └──────────────┘
       │
       │ mpsc::channel
       │
       ▼
┌──────────────────┐
│  Reader Thread   │
│  (reads stdout)  │
│  → dispatches    │
│    LspEvent      │
│  → Diagnostics   │
│  → Responses     │
└──────────────────┘
```

---

## 2. Session Lifecycle

### Initialization Sequence

1. **Spawn**: `Command::new(command).args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()`
2. **Initialize request** (wait up to 5s for response):
   ```json
   {"jsonrpc":"2.0", "id":1, "method":"initialize",
    "params":{
      "processId": PID,
      "capabilities":{
        "textDocument":{
          "publishDiagnostics":{},
          "completion":{"completionItem":{"documentationFormat":["markdown","plaintext"]}}
        }
      },
      "rootUri":"file:///workspace/root"
    }}
   ```
3. **Initialized notification**:
   ```json
   {"jsonrpc":"2.0", "method":"initialized"}
   ```
4. **DidOpen notification**:
   ```json
   {"jsonrpc":"2.0", "method":"textDocument/didOpen",
    "params":{
      "textDocument":{"uri":"file:///path/to/file", "languageId":"rust", "version":1, "text":"..."}
    }}
   ```

### Ongoing Communication

- **DidChange** (full text sync on every buffer change):
  ```json
  {"jsonrpc":"2.0", "method":"textDocument/didChange",
   "params":{
     "textDocument":{"uri":"...", "version":2},
     "contentChanges":[{"text":"..."}]  // full text
   }}
  ```

- **Hover request**:
  ```json
  {"jsonrpc":"2.0", "id":2, "method":"textDocument/hover",
   "params":{
     "textDocument":{"uri":"..."},
     "position":{"line":5, "character":12}
   }}
  ```

- **Completion request**:
  ```json
  {"jsonrpc":"2.0", "id":3, "method":"textDocument/completion",
   "params":{
     "textDocument":{"uri":"..."},
     "position":{"line":5, "character":14}
   }}
  ```

### Shutdown Sequence

```rust
impl Drop for LspSession {
    fn drop(&mut self) {
        // Send "exit" notification
        // Kill child process
    }
}
```

---

## 3. Message Protocol

### Wire Format

```
Content-Length: N\r\n
\r\n
{json_payload}
```

### Background Reader Thread

```rust
// Spawned in LspSession::start()
fn read_messages(stdout, channel_tx) {
    loop {
        // Read Content-Length header
        // Read JSON body
        // Parse JSON-RPC 2.0 message
        // If method == "textDocument/publishDiagnostics" → LspEvent::Diagnostics
        // If has "id" field → LspEvent::Response(id, result/error)
        // Send to channel
    }
}
```

### Response Registry

```rust
struct ResponseRegistry {
    map: HashMap<u64, LspResponse>,
    order: VecDeque<u64>,
}

// Capacity: 128 entries (MAX_COMPLETED_RESPONSES)
// On insert beyond capacity: evicts oldest entry
// take(id): removes and returns the response
```

---

## 4. Request/Response Matching

```
Editor                    LspSession
  │                          │
  │ request_hover(line, char)│
  │─────────────────────────>│
  │                          │  id = ++self.request_id
  │                          │  send JSON-RPC request with id
  │<── Ok(id) ───────────────│
  │                          │
  │ [later frames]           │
  │ refresh_lsp()            │
  │─────────────────────────>│
  │                          │  poll() → drains channel
  │                          │  reader thread received response
  │                          │  stored in ResponseRegistry{id: ...}
  │ take_response(id) ──────>│
  │<── Option<LspResponse> ──│
  │                          │
  │ (HoverState: apply_response(id, response))
  │   ✓ if pending_request == id → accept
  │   ✗ if ids don't match → stale response, ignore
```

---

## 5. Hover Response Parsing

```rust
// Entry point
parse_hover_response(response: LspResponse) -> Result<Option<String>, Value>

// JSON path: result.contents
// contents can be:
//   String → parse_marked_string(string)
//   Object → {
//     kind: "markdown" → normalize_markdown(value)
//     kind: "plaintext" → value as-is
//     language: "..." → value as-is (MarkedString)
//   }
//   Array → join each element with "\n\n"
//   Null → Ok(None)

normalize_markdown(input):
  - Strip lines starting with ```
  - Convert [label](url) → label (url)
  - Strip backticks
  - Strip **/__ (bold)
  - Strip */_ (italic) only at emphasis boundaries
```

### Emphasis Boundary Detection

```rust
fn is_emphasis_boundary(c: Option<&char>) -> bool {
    c.map_or(true, |c| c.is_whitespace() || c.is_ascii_punctuation())
}
// Preserves: a*b, name_♥, snake_case, x*√y
// Strips: *bold*, _italic_, `code`
```

---

## 6. Completion Response Parsing

```rust
parse_completion_response(response) -> Result<Vec<CompletionItem>, Value>

// Accepts:
//   Result array: [item1, item2, ...]
//   Result object: { isIncomplete: bool, items: [item1, ...] }
// Skips items without a label
// Sorts by sortText (case-insensitive), then original_index

// CompletionItem fields extracted:
//   label, kind, detail, insertText, sortText, filterText
//   textEdit: { range: {start: {line,character}, end: {line,character}},
//               newText: string }
//   preselect
```

---

## 7. Diagnostic Handling

```rust
// Incoming: textDocument/publishDiagnostics notification
// Parsed into Vec<Diagnostic>

// DiagnosticState stores diagnostics_by_line: HashMap<usize, Vec<Diagnostic>>
// Sorted per-line by severity (Error < Warning < Info < Hint), then character

// Used for:
// - Status bar counts: E:N W:N
// - Inline underlines via apply_diagnostic_underlines()
// - Diagnostic popup: active_diagnostic() returns current-line diagnostic
// - Cycling: cycle_active_diagnostic(delta) moves through multi-diagnostic lines
```

---

## 8. LSP Configuration Reference

Language configs in `languages.toml`:

```toml
[[languages]]
name = "Rust"
file_types = ["rs"]
grammar = "rust"

[languages.lsp]
command = "rust-analyzer"
args = []

[[languages]]
name = "Python"
file_types = ["py"]
grammar = "python"

[languages.lsp]
command = "pyright-langserver"
args = ["--stdio"]

[[languages]]
name = "TypeScript"
file_types = ["ts", "tsx"]
grammar = "typescript"

[languages.lsp]
command = "typescript-language-server"
args = ["--stdio"]
```
