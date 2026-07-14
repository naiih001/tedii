use anyhow::{Context, Result};
use serde_json::json;
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

use crate::config::LspServerConfig;

static NEXT_SESSION_ID: AtomicU64 = AtomicU64::new(1);

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
    Response(u64),
}

pub struct LspSession {
    session_id: u64,
    language_id: String,
    command: String,
    root_dir: PathBuf,
    file_path: PathBuf,
    stdin: Arc<Mutex<ChildStdin>>,
    rx: mpsc::Receiver<LspEvent>,
    child: Child,
    pub diagnostics: DiagnosticState,
    current_uri: String,
    request_id: u64,
    document_version: i32,
}

impl LspSession {
    pub fn start(
        config: &LspServerConfig,
        root_dir: &Path,
        language_id: &str,
        file_path: &Path,
        text: &str,
    ) -> Result<Self> {
        let session_id = NEXT_SESSION_ID.fetch_add(1, Ordering::Relaxed);
        let file_path_buf = file_path.to_path_buf();
        let root_dir_buf = root_dir.to_path_buf();
        let language_id_buf = language_id.to_string();
        let command = config.command.clone();

        log_line(format!(
            "[session {}] starting LSP command={} args={:?} cwd={} file={} language={}",
            session_id,
            config.command,
            config.args,
            root_dir.display(),
            file_path.display(),
            language_id
        ));

        let mut child = Command::new(&config.command)
            .args(&config.args)
            .current_dir(root_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("Failed to start LSP server {}", config.command))?;

        let stdin = Arc::new(Mutex::new(
            child.stdin.take().context("Failed to open LSP stdin")?,
        ));
        let stdout = child.stdout.take().context("Failed to open LSP stdout")?;
        if let Some(stderr) = child.stderr.take() {
            thread::spawn(move || read_stderr(stderr));
        }
        let (tx, rx) = mpsc::channel();
        let reader_tx = tx.clone();

        thread::spawn(move || read_messages(stdout, reader_tx));

        let mut session = Self {
            session_id,
            language_id: language_id_buf,
            command,
            root_dir: root_dir_buf,
            file_path: file_path_buf,
            stdin,
            rx,
            child,
            diagnostics: DiagnosticState::default(),
            current_uri: to_file_uri(file_path),
            request_id: 1,
            document_version: 0,
        };
        let init_id = session.send_initialize(language_id, root_dir)?;
        session.wait_for_response(init_id)?;
        session.send_notification("initialized", json!({}))?;
        session.did_open(language_id, text);
        log_line(format!(
            "[session {}] ready uri={} language_id={} command={}",
            session.session_id, session.current_uri, language_id, session.command
        ));
        Ok(session)
    }

    fn send_initialize(&mut self, language_id: &str, root_dir: &Path) -> Result<u64> {
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
        log_line(format!(
            "[session {}] -> initialize language_id={} root={} params={}",
            self.session_id,
            language_id,
            root_dir.display(),
            pretty_json(&params)
        ));
        self.send_request("initialize", params)
    }

    pub fn did_open(&mut self, language_id: &str, text: &str) {
        self.document_version = 1;
        log_line(format!(
            "[session {}] -> didOpen uri={} language_id={} bytes={} text={}",
            self.session_id,
            self.current_uri,
            language_id,
            text.len(),
            truncate(text, 2048)
        ));
        let params = json!({
            "textDocument": {
                "uri": self.current_uri,
                "languageId": language_id,
                "version": self.document_version,
                "text": text,
            }
        });
        let _ = self.send_notification("textDocument/didOpen", params);
    }

    pub fn did_change(&mut self, text: &str) {
        self.document_version += 1;
        log_line(format!(
            "[session {}] -> didChange uri={} version={} bytes={} text={}",
            self.session_id,
            self.current_uri,
            self.document_version,
            text.len(),
            truncate(text, 2048)
        ));
        let params = json!({
            "textDocument": {
                "uri": self.current_uri,
                "version": self.document_version,
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
                LspEvent::Diagnostics(items) => {
                    log_line(format!(
                        "[session {}] <- publishDiagnostics count={} lines={:?}",
                        self.session_id,
                        items.len(),
                        items.iter().map(|d| d.line).collect::<Vec<_>>()
                    ));
                    self.diagnostics.update(items)
                }
                LspEvent::Response(id) => {
                    log_line(format!("[session {}] <- response id={}", self.session_id, id));
                }
            }
        }
    }

    fn send_request(&mut self, method: &str, params: serde_json::Value) -> Result<u64> {
        let id = self.request_id;
        self.request_id += 1;
        let body = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        log_line(format!(
            "[session {}] -> request id={} method={} body={}",
            self.session_id,
            id,
            method,
            pretty_json(&body)
        ));
        self.write_message(&body.to_string())?;
        Ok(id)
    }

    fn send_notification(&mut self, method: &str, params: serde_json::Value) -> Result<()> {
        let body = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        log_line(format!(
            "[session {}] -> notification method={} body={}",
            self.session_id,
            method,
            pretty_json(&body)
        ));
        self.write_message(&body.to_string())
    }

    fn write_message(&mut self, payload: &str) -> Result<()> {
        let mut stdin = self.stdin.lock().expect("LSP stdin poisoned");
        write!(stdin, "Content-Length: {}\r\n\r\n{}", payload.len(), payload)?;
        stdin.flush()?;
        Ok(())
    }

    fn wait_for_response(&mut self, expected_id: u64) -> Result<()> {
        loop {
            match self.rx.recv_timeout(Duration::from_secs(5))? {
                LspEvent::Diagnostics(items) => self.diagnostics.update(items),
                LspEvent::Response(id) if id == expected_id => {
                    log_line(format!(
                        "[session {}] <- initialize response id={}",
                        self.session_id, id
                    ));
                    return Ok(());
                }
                LspEvent::Response(_) => {}
            }
        }
    }
}

impl Drop for LspSession {
    fn drop(&mut self) {
        log_line(format!(
            "[session {}] dropping session for file={} language={} root={} command={}",
            self.session_id,
            self.file_path.display(),
            self.language_id,
            self.root_dir.display(),
            self.command
        ));
        let _ = self.send_notification("exit", json!({}));
        let _ = self.child.kill();
        log_line(format!("[session {}] child kill requested", self.session_id));
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
            log_line("stream missing Content-Length header");
            continue;
        };
        let mut body = vec![0u8; len];
        if reader.read_exact(&mut body).is_err() {
            log_line("stdout closed while reading body");
            return;
        }
        if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&body) {
            log_line(format!("<- raw message {}", pretty_json(&value)));
            if let Some(id) = value.get("id").and_then(|v| v.as_u64()) {
                let _ = tx.send(LspEvent::Response(id));
            }
            if value.get("method").and_then(|m| m.as_str()) == Some("textDocument/publishDiagnostics") {
                if let Some(params) = value.get("params") {
                    let diagnostics = parse_diagnostics(params);
                    log_line(format!(
                        "<- publishDiagnostics uri={} count={}",
                        params
                            .get("uri")
                            .and_then(|v| v.as_str())
                            .unwrap_or("<missing>"),
                        diagnostics.len()
                    ));
                    let _ = tx.send(LspEvent::Diagnostics(diagnostics));
                }
            }
        } else {
            log_line(format!(
                "failed to parse LSP JSON body: {}",
                String::from_utf8_lossy(&body)
            ));
        }
    }
}

fn read_stderr(stderr: impl Read) {
    let reader = BufReader::new(stderr);
    for line in reader.lines() {
        match line {
            Ok(line) => log_line(format!("stderr {}", line)),
            Err(err) => {
                log_line(format!("stderr read error: {}", err));
                break;
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

pub(crate) fn log_line(message: impl AsRef<str>) {
    let log_path = dirs::config_dir()
        .map(|dir| dir.join("tedii").join("lsp.log"))
        .unwrap_or_else(|| PathBuf::from("tedii-lsp.log"));
    if let Some(parent) = log_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let timestamp = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(duration) => duration.as_secs().to_string(),
        Err(_) => "0".to_string(),
    };
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_path) {
        let _ = writeln!(file, "[{}] {}", timestamp, message.as_ref());
    }
}

fn pretty_json(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

fn truncate(value: &str, max_chars: usize) -> String {
    let char_count = value.chars().count();
    if char_count <= max_chars {
        return value.to_string();
    }
    let mut out = value.chars().take(max_chars).collect::<String>();
    out.push_str("...[truncated]");
    out
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
