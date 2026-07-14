use anyhow::{Context, Result};
use serde_json::json;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

use crate::config::LspServerConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Information,
    Hint,
}

impl DiagnosticSeverity {
    fn from_lsp(value: Option<u64>) -> Self {
        match value {
            Some(1) => Self::Error,
            Some(2) => Self::Warning,
            Some(3) => Self::Information,
            Some(4) => Self::Hint,
            _ => Self::Information,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub source: Option<String>,
    pub line: usize,
    pub character: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiagnosticState {
    pub diagnostics_by_line: HashMap<usize, Vec<Diagnostic>>,
    pub error_count: usize,
    pub warning_count: usize,
}

impl DiagnosticState {
    pub fn clear(&mut self) {
        self.diagnostics_by_line.clear();
        self.error_count = 0;
        self.warning_count = 0;
    }

    pub fn update(&mut self, diagnostics: Vec<Diagnostic>) {
        self.clear();
        for diagnostic in diagnostics {
            match diagnostic.severity {
                DiagnosticSeverity::Error => self.error_count += 1,
                DiagnosticSeverity::Warning => self.warning_count += 1,
                DiagnosticSeverity::Information | DiagnosticSeverity::Hint => {}
            }
            self.diagnostics_by_line
                .entry(diagnostic.line)
                .or_default()
                .push(diagnostic);
        }
        for values in self.diagnostics_by_line.values_mut() {
            values.sort_by(|a, b| a.severity.cmp(&b.severity).then(a.character.cmp(&b.character)));
        }
    }

    pub fn diagnostics_at(&self, line: usize) -> &[Diagnostic] {
        self.diagnostics_by_line
            .get(&line)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }
}

enum LspEvent {
    Diagnostics(Vec<Diagnostic>),
}

pub struct LspSession {
    stdin: Arc<Mutex<ChildStdin>>,
    rx: mpsc::Receiver<LspEvent>,
    child: Child,
    pub diagnostics: DiagnosticState,
    current_uri: String,
    request_id: u64,
}

impl LspSession {
    pub fn start(
        config: &LspServerConfig,
        root_dir: &Path,
        language_id: &str,
        file_path: &Path,
        text: &str,
    ) -> Result<Self> {
        let mut child = Command::new(&config.command)
            .args(&config.args)
            .current_dir(root_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("Failed to start LSP server {}", config.command))?;

        let stdin = Arc::new(Mutex::new(
            child.stdin.take().context("Failed to open LSP stdin")?,
        ));
        let stdout = child.stdout.take().context("Failed to open LSP stdout")?;
        let (tx, rx) = mpsc::channel();
        let reader_tx = tx.clone();

        thread::spawn(move || read_messages(stdout, reader_tx));

        let mut session = Self {
            stdin,
            rx,
            child,
            diagnostics: DiagnosticState::default(),
            current_uri: to_file_uri(file_path),
            request_id: 1,
        };
        session.initialize(language_id, root_dir)?;
        session.did_open(language_id, text);
        Ok(session)
    }

    fn initialize(&mut self, language_id: &str, root_dir: &Path) -> Result<()> {
        let params = json!({
            "processId": std::process::id(),
            "rootUri": to_file_uri(root_dir),
            "rootPath": root_dir.to_string_lossy(),
            "capabilities": {
                "textDocument": {
                    "publishDiagnostics": {
                        "relatedInformation": false
                    }
                }
            },
            "workspaceFolders": [{
                "uri": to_file_uri(root_dir),
                "name": root_dir.file_name().and_then(|s| s.to_str()).unwrap_or(language_id),
            }]
        });
        self.send_request("initialize", params)?;
        self.send_notification("initialized", json!({}))?;
        Ok(())
    }

    pub fn did_open(&mut self, language_id: &str, text: &str) {
        let params = json!({
            "textDocument": {
                "uri": self.current_uri,
                "languageId": language_id,
                "version": 1,
                "text": text,
            }
        });
        let _ = self.send_notification("textDocument/didOpen", params);
    }

    pub fn did_change(&mut self, text: &str) {
        let params = json!({
            "textDocument": {
                "uri": self.current_uri,
                "version": self.request_id as i32,
            },
            "contentChanges": [{
                "text": text
            }]
        });
        let _ = self.send_notification("textDocument/didChange", params);
    }

    pub fn poll(&mut self) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                LspEvent::Diagnostics(items) => self.diagnostics.update(items),
            }
        }
    }

    fn send_request(&mut self, method: &str, params: serde_json::Value) -> Result<()> {
        let id = self.request_id;
        self.request_id += 1;
        let body = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        self.write_message(&body.to_string())
    }

    fn send_notification(&mut self, method: &str, params: serde_json::Value) -> Result<()> {
        let body = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        self.write_message(&body.to_string())
    }

    fn write_message(&mut self, payload: &str) -> Result<()> {
        let mut stdin = self.stdin.lock().expect("LSP stdin poisoned");
        write!(stdin, "Content-Length: {}\r\n\r\n{}", payload.len(), payload)?;
        stdin.flush()?;
        Ok(())
    }
}

impl Drop for LspSession {
    fn drop(&mut self) {
        let _ = self.send_notification("exit", json!({}));
        let _ = self.child.kill();
    }
}

fn read_messages(stdout: impl Read, tx: mpsc::Sender<LspEvent>) {
    let mut reader = BufReader::new(stdout);
    loop {
        let mut content_length = None;
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => return,
                Ok(_) => {
                    let trimmed = line.trim_end_matches(['\r', '\n']);
                    if trimmed.is_empty() {
                        break;
                    }
                    if let Some(rest) = trimmed.strip_prefix("Content-Length: ") {
                        content_length = rest.parse::<usize>().ok();
                    }
                }
                Err(_) => return,
            }
        }
        let Some(len) = content_length else {
            continue;
        };
        let mut body = vec![0u8; len];
        if reader.read_exact(&mut body).is_err() {
            return;
        }
        if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&body) {
            if value.get("method").and_then(|m| m.as_str()) == Some("textDocument/publishDiagnostics") {
                if let Some(params) = value.get("params") {
                    let diagnostics = parse_diagnostics(params);
                    let _ = tx.send(LspEvent::Diagnostics(diagnostics));
                }
            }
        }
    }
}

fn parse_diagnostics(params: &serde_json::Value) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let items = params
        .get("diagnostics")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    for item in items {
        let severity = DiagnosticSeverity::from_lsp(item.get("severity").and_then(|v| v.as_u64()));
        let range = item.get("range").cloned().unwrap_or_default();
        let start = range.get("start").cloned().unwrap_or_default();
        diagnostics.push(Diagnostic {
            severity,
            message: item
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            source: item.get("source").and_then(|v| v.as_str()).map(|s| s.to_string()),
            line: start.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
            character: start.get("character").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
        });
    }
    diagnostics
}

fn to_file_uri(path: &Path) -> String {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::fs::canonicalize(path).unwrap_or_else(|_| PathBuf::from(path))
    };
    format!("file://{}", path.to_string_lossy().replace(' ', "%20"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_state_counts_severities_by_line() {
        let mut state = DiagnosticState::default();
        state.update(vec![
            Diagnostic {
                severity: DiagnosticSeverity::Error,
                message: "boom".into(),
                source: None,
                line: 4,
                character: 9,
            },
            Diagnostic {
                severity: DiagnosticSeverity::Warning,
                message: "careful".into(),
                source: None,
                line: 4,
                character: 2,
            },
            Diagnostic {
                severity: DiagnosticSeverity::Hint,
                message: "note".into(),
                source: None,
                line: 7,
                character: 1,
            },
        ]);

        assert_eq!(state.error_count, 1);
        assert_eq!(state.warning_count, 1);
        assert_eq!(state.diagnostics_at(4).len(), 2);
        assert_eq!(state.diagnostics_at(7).len(), 1);
    }

    #[test]
    fn parse_publish_diagnostics_payload() {
        let payload = json!({
            "diagnostics": [{
                "severity": 1,
                "message": "bad thing",
                "source": "rustc",
                "range": {
                    "start": { "line": 3, "character": 5 },
                    "end": { "line": 3, "character": 10 }
                }
            }]
        });

        let diagnostics = parse_diagnostics(&payload);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, DiagnosticSeverity::Error);
        assert_eq!(diagnostics[0].line, 3);
        assert_eq!(diagnostics[0].character, 5);
        assert_eq!(diagnostics[0].message, "bad thing");
    }
}
