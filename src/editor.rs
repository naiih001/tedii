use crate::completion::{completion_insert_text, CompletionState};
use crate::config::Config;
use crate::git::{compute_diff, DiffHunk, GitRepo};
use crate::hover::HoverState;
use crate::lsp::{DiagnosticSeverity, DiagnosticState, LspResponse, LspSession};
use crate::plugin::{EventKind, PluginRuntime};
use crate::syntax::SyntaxHighlighter;
use crate::theme::Theme;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ropey::Rope;
use std::path::{Path, PathBuf};

fn matching_pair(c: char) -> Option<char> {
    match c {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        ')' => Some('('),
        ']' => Some('['),
        '}' => Some('{'),
        '"' => Some('"'),
        '\'' => Some('\''),
        '`' => Some('`'),
        _ => None,
    }
}

fn lsp_root_dir(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn lsp_position_to_char(buffer: &Rope, line: usize, utf16_character: usize) -> usize {
    if line >= buffer.len_lines() {
        return buffer.len_chars();
    }

    let line_start = buffer.line_to_char(line);
    let mut utf16_offset = 0;
    let mut char_offset = 0;
    for character in buffer.line(line).chars() {
        if matches!(character, '\r' | '\n') {
            break;
        }
        let next_offset = utf16_offset + character.len_utf16();
        if next_offset > utf16_character {
            break;
        }
        utf16_offset = next_offset;
        char_offset += 1;
    }
    line_start + char_offset
}

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

fn diagnostic_theme_key(severity: DiagnosticSeverity) -> &'static str {
    match severity {
        DiagnosticSeverity::Error => "diagnostic_error",
        DiagnosticSeverity::Warning => "diagnostic_warning",
        DiagnosticSeverity::Information => "diagnostic_information",
        DiagnosticSeverity::Hint => "diagnostic_hint",
    }
}

fn apply_diagnostic_underlines(
    buffer: &Rope,
    diagnostics: &DiagnosticState,
    char_styles: &mut [Style],
    theme: &Theme,
) {
    let text_len = buffer.len_chars().min(char_styles.len());
    let mut items = diagnostics
        .diagnostics_by_line
        .values()
        .flatten()
        .collect::<Vec<_>>();
    items.sort_by_key(|diagnostic| std::cmp::Reverse(diagnostic.severity));

    for diagnostic in items {
        let start =
            lsp_position_to_char(buffer, diagnostic.line, diagnostic.character).min(text_len);
        let mut end = lsp_position_to_char(buffer, diagnostic.end_line, diagnostic.end_character)
            .min(text_len);
        if end <= start && start < text_len {
            end = start + 1;
        }

        let color = theme
            .ui_get(diagnostic_theme_key(diagnostic.severity))
            .fg
            .unwrap_or(Color::Reset);
        let underline = Style::default()
            .underline_color(color)
            .add_modifier(Modifier::UNDERLINED);
        for (index, char_style) in char_styles.iter_mut().enumerate().take(end).skip(start) {
            if !matches!(buffer.char(index), '\r' | '\n') {
                *char_style = char_style.patch(underline);
            }
        }
    }
}

#[derive(Default, PartialEq, Eq, Clone, Copy)]
pub enum Mode {
    #[default]
    Normal,
    Insert,
    Command,
    Search,
    Fuzzy,
    Visual,
}

pub struct Editor {
    pub buffer: Rope,
    pub cursor: usize,
    pub scroll_x: usize,
    pub scroll_y: usize,
    pub mode: Mode,
    pub should_quit: bool,
    pub pending_g: bool,
    pub pending_space: bool,
    pub pending_z: bool,
    pub command_buffer: String,
    pub current_file: Option<PathBuf>,
    pub highlighter: SyntaxHighlighter,
    pub theme: Theme,
    pub search_query: String,
    pub search_results: Vec<usize>,
    pub search_active: bool,
    pub search_idx: usize,
    pub selection_anchor: Option<usize>,
    pub clipboard: String,
    pub git_branch: Option<String>,
    pub diff_hunks: Vec<DiffHunk>,
    pub lsp_diagnostics: DiagnosticState,
    pub lsp_cursor_index: usize,
    pub hover: HoverState,
    pub completion: CompletionState,
    diff_base: Option<String>,
    git_repo: Option<GitRepo>,
    language_config: Option<Config>,
    lsp_session: Option<LspSession>,
    pub(crate) buffer_version: u64,
    saved_buffer_version: u64,
    last_lsp_sync_version: u64,
    cached_text: String,
    cached_highlights: Vec<(usize, usize, Style)>,
    cached_highlight_version: u64,
    cached_char_styles: Vec<Style>,
    undo_stack: Vec<(Rope, usize)>,
    redo_stack: Vec<(Rope, usize)>,
    plugin_runtime: Option<*mut PluginRuntime>,
}

impl Editor {
    pub fn new(
        text: &str,
        file_path: Option<&Path>,
        theme: Theme,
        language_config: Option<Config>,
    ) -> Self {
        let mut highlighter = SyntaxHighlighter::new(theme.clone());
        if let Some(path) = file_path {
            if let Some(path_str) = path.to_str() {
                highlighter.load_language_for_path(path_str);
            }
        }
        let (git_repo, git_branch, diff_base) = file_path
            .and_then(|p| {
                let repo = GitRepo::discover(p)?;
                let branch = repo.current_branch();
                let base = repo
                    .diff_base(p)
                    .map(|b| String::from_utf8_lossy(&b).to_string());
                Some((Some(repo), branch, base))
            })
            .unwrap_or((None, None, None));
        let mut editor = Self {
            buffer: Rope::from_str(text),
            cursor: 0,
            scroll_x: 0,
            scroll_y: 0,
            mode: Mode::Normal,
            should_quit: false,
            pending_g: false,
            pending_space: false,
            pending_z: false,
            command_buffer: String::new(),
            current_file: file_path.map(|p| p.to_path_buf()),
            highlighter,
            theme,
            search_query: String::new(),
            search_results: Vec::new(),
            search_active: false,
            search_idx: 0,
            selection_anchor: None,
            clipboard: String::new(),
            git_branch,
            diff_hunks: Vec::new(),
            lsp_diagnostics: DiagnosticState::default(),
            lsp_cursor_index: 0,
            hover: HoverState::default(),
            completion: CompletionState::default(),
            diff_base,
            git_repo,
            language_config,
            lsp_session: None,
            buffer_version: 0,
            saved_buffer_version: 0,
            last_lsp_sync_version: 0,
            cached_text: text.to_string(),
            cached_highlights: Vec::new(),
            cached_highlight_version: 1, // Force initial highlight on first render
            cached_char_styles: Vec::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            plugin_runtime: None,
        };

        if let Some(path) = file_path {
            editor.restart_lsp(path);
        }

        editor
    }

    pub fn save(&mut self) -> anyhow::Result<()> {
        if let Some(ref path) = self.current_file {
            std::fs::write(path, self.buffer.to_string())?;
            self.saved_buffer_version = self.buffer_version;
            self.refresh_lsp();
        }
        self.fire_event(EventKind::SaveFile);
        Ok(())
    }

    pub fn refresh_lsp(&mut self) {
        let buffer_changed = self.buffer_version != self.last_lsp_sync_version;
        if buffer_changed {
            self.hover.clear();
        }
        if let Some(session) = self.lsp_session.as_mut() {
            if buffer_changed {
                session.did_change(&self.buffer.to_string());
                self.last_lsp_sync_version = self.buffer_version;
            }
            session.poll();
            if let Some(request_id) = self.hover.pending_request {
                if let Some(response) = session.take_response(request_id) {
                    if let LspResponse::Error(error) = &response {
                        crate::lsp::log_line(format!("[editor] hover response failed: {}", error));
                    }
                    self.hover.apply_response(request_id, response);
                }
            }
            if let Some(request_id) = self.completion.pending_request {
                if let Some(response) = session.take_response(request_id) {
                    if let LspResponse::Error(error) = &response {
                        crate::lsp::log_line(format!(
                            "[editor] completion response failed: {}",
                            error
                        ));
                    }
                    self.completion.apply_response(request_id, response);
                }
            }
            self.lsp_diagnostics = session.diagnostics.clone();
        }
    }

    fn restart_lsp(&mut self, path: &Path) {
        self.hover.clear();
        self.completion.clear();
        self.lsp_session = None;
        self.lsp_diagnostics.clear();
        self.lsp_cursor_index = 0;
        self.last_lsp_sync_version = self.buffer_version;
        crate::lsp::log_line(format!(
            "[editor] selecting LSP for file={} buffer_version={}",
            path.display(),
            self.buffer_version
        ));

        let Some(config) = self.language_config.as_ref() else {
            crate::lsp::log_line("[editor] no languages.toml loaded; skipping LSP");
            return;
        };
        let Some(file_path) = path.to_str() else {
            crate::lsp::log_line(format!(
                "[editor] file path is not valid UTF-8; skipping LSP for {}",
                path.display()
            ));
            return;
        };
        let Some(language) = config.language_for_file(file_path) else {
            crate::lsp::log_line(format!(
                "[editor] no language matched file={} from {} configured languages",
                path.display(),
                config.languages.len()
            ));
            return;
        };
        let Some(lsp) = language.lsp.as_ref() else {
            crate::lsp::log_line(format!(
                "[editor] language={} matched file={} but has no LSP config",
                language.name,
                path.display()
            ));
            return;
        };
        let root_dir = lsp_root_dir(path);
        crate::lsp::log_line(format!(
            "[editor] matched language={} command={} args={:?}",
            language.name, lsp.command, lsp.args
        ));
        match LspSession::start(lsp, root_dir, &language.name, path, &self.buffer.to_string()) {
            Ok(session) => {
                self.lsp_session = Some(session);
                crate::lsp::log_line(format!(
                    "[editor] LSP session attached for file={} language={}",
                    path.display(),
                    language.name
                ));
            }
            Err(err) => {
                crate::lsp::log_line(format!(
                    "[editor] LSP startup failed for file={} language={} error={}",
                    path.display(),
                    language.name,
                    err
                ));
                eprintln!("LSP startup failed for {}: {}", language.name, err);
            }
        }
    }

    pub fn diagnostic_on_cursor_line(&self) -> Option<&[crate::lsp::Diagnostic]> {
        let line = self.buffer.char_to_line(self.cursor);
        let diagnostics = self.lsp_diagnostics.diagnostics_at(line);
        if diagnostics.is_empty() {
            None
        } else {
            Some(diagnostics)
        }
    }

    pub fn active_diagnostic(&self) -> Option<&crate::lsp::Diagnostic> {
        let diagnostics = self.diagnostic_on_cursor_line()?;
        diagnostics.get(self.lsp_cursor_index % diagnostics.len())
    }

    pub fn active_diagnostic_with_position(
        &self,
    ) -> Option<(&crate::lsp::Diagnostic, usize, usize)> {
        let diagnostics = self.diagnostic_on_cursor_line()?;
        let index = self.lsp_cursor_index % diagnostics.len();
        Some((&diagnostics[index], index + 1, diagnostics.len()))
    }

    pub fn cycle_active_diagnostic(&mut self, delta: isize) {
        let Some(diagnostics) = self.diagnostic_on_cursor_line() else {
            self.lsp_cursor_index = 0;
            return;
        };
        if diagnostics.is_empty() {
            self.lsp_cursor_index = 0;
            return;
        }
        let len = diagnostics.len() as isize;
        self.lsp_cursor_index = ((self.lsp_cursor_index as isize + delta).rem_euclid(len)) as usize;
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

    pub fn request_completion(&mut self) {
        let (line, character) = cursor_lsp_position(&self.buffer, self.cursor);
        let trigger_offset = self.cursor;
        let Some(session) = self.lsp_session.as_mut() else {
            self.completion.clear();
            return;
        };
        match session.request_completion(line, character) {
            Ok(request_id) => self.completion.begin_request(request_id, trigger_offset),
            Err(error) => {
                crate::lsp::log_line(format!("[editor] completion request failed: {}", error));
                self.completion.clear();
            }
        }
    }

    pub fn accept_completion(&mut self) -> bool {
        let Some(item) = self.completion.active_item() else {
            self.completion.clear();
            return false;
        };
        let text = completion_insert_text(item);
        let text_edit_range = item.text_edit_range;
        let word_start = {
            let mut pos = self.cursor;
            while pos > 0 {
                let ch = self.buffer.char(pos - 1);
                if ch.is_alphanumeric() || ch == '_' {
                    pos -= 1;
                } else {
                    break;
                }
            }
            pos
        };
        self.completion.clear();
        self.begin_undo_group();
        if let Some((sl, sc, _el, _ec)) = text_edit_range {
            let start = lsp_position_to_char(&self.buffer, sl, sc);
            if start <= self.cursor && self.cursor <= self.buffer.len_chars() {
                self.buffer.remove(start..self.cursor);
                self.buffer.insert(start, &text);
                self.cursor = start + text.chars().count();
            } else {
                self.buffer.remove(word_start..self.cursor);
                self.buffer.insert(word_start, &text);
                self.cursor = word_start + text.chars().count();
            }
        } else {
            self.buffer.remove(word_start..self.cursor);
            self.buffer.insert(word_start, &text);
            self.cursor = word_start + text.chars().count();
        }
        self.buffer_version = self.buffer_version.wrapping_add(1);
        self.refresh_lsp();
        true
    }

    pub fn dismiss_completion(&mut self) {
        self.completion.clear();
    }

    pub fn filter_completion(&mut self, prefix: &str) {
        self.completion.filter(prefix);
    }

    pub fn is_dirty(&self) -> bool {
        self.buffer_version != self.saved_buffer_version
    }

    pub fn begin_undo_group(&mut self) {
        self.undo_stack.push((self.buffer.clone(), self.cursor));
        self.redo_stack.clear();
    }

    pub fn undo(&mut self) {
        if let Some((buffer, _cursor)) = self.undo_stack.pop() {
            self.redo_stack.push((self.buffer.clone(), self.cursor));
            self.buffer = buffer;
            self.cursor = self.cursor.min(self.buffer.len_chars());
            self.buffer_version = self.buffer_version.wrapping_add(1);
        }
    }

    pub fn redo(&mut self) {
        if let Some((buffer, cursor)) = self.redo_stack.pop() {
            self.undo_stack.push((self.buffer.clone(), self.cursor));
            self.buffer = buffer;
            self.cursor = cursor;
            self.buffer_version = self.buffer_version.wrapping_add(1);
        }
    }

    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn move_right(&mut self) {
        let line_idx = self.buffer.char_to_line(self.cursor);
        let line = self.buffer.line(line_idx);
        let line_len = line.len_chars();
        let line_start = self.buffer.line_to_char(line_idx);
        let col = self.cursor - line_start;

        let is_at_line_end = if line_len > 0 && line.char(line_len - 1) == '\n' {
            col >= line_len - 1
        } else {
            col >= line_len
        };

        if !is_at_line_end {
            self.cursor += 1;
        } else if line_idx < self.buffer.len_lines() - 1 {
            self.cursor = self.buffer.line_to_char(line_idx + 1);
        }
    }

    pub fn move_up(&mut self) {
        let line = self.buffer.char_to_line(self.cursor);
        if line > 0 {
            let col = self.cursor - self.buffer.line_to_char(line);
            let prev_line = self.buffer.line(line - 1);
            let prev_line_len = prev_line.len_chars();

            let max_col = if prev_line_len > 0 && prev_line.char(prev_line_len - 1) == '\n' {
                prev_line_len - 1
            } else {
                prev_line_len
            };

            let new_col = col.min(max_col);
            self.cursor = self.buffer.line_to_char(line - 1) + new_col;
        }
    }

    pub fn move_down(&mut self) {
        let line = self.buffer.char_to_line(self.cursor);
        if line < self.buffer.len_lines() - 1 {
            let col = self.cursor - self.buffer.line_to_char(line);
            let next_line = self.buffer.line(line + 1);
            let next_line_len = next_line.len_chars();

            let max_col = if next_line_len > 0 && next_line.char(next_line_len - 1) == '\n' {
                next_line_len - 1
            } else {
                next_line_len
            };

            let new_col = col.min(max_col);
            self.cursor = self.buffer.line_to_char(line + 1) + new_col;
        }
    }

    pub fn move_to_start(&mut self) {
        self.cursor = 0;
    }

    pub fn move_to_end(&mut self) {
        if self.buffer.len_chars() > 0 {
            self.cursor = self.buffer.len_chars() - 1;
        } else {
            self.cursor = 0;
        }
    }

    pub fn move_to_line_start(&mut self) {
        let line_idx = self.buffer.char_to_line(self.cursor);
        self.cursor = self.buffer.line_to_char(line_idx);
    }

    pub fn move_word_forward(&mut self) {
        let len = self.buffer.len_chars();
        if len == 0 || self.cursor >= len {
            return;
        }

        let is_word = |c: char| c.is_alphanumeric() || c == '_';
        let mut i = self.cursor;

        let cur = self.buffer.char(i);

        if cur.is_whitespace() {
            while i < len && self.buffer.char(i).is_whitespace() {
                i += 1;
            }
        } else if is_word(cur) {
            while i < len && is_word(self.buffer.char(i)) {
                i += 1;
            }
        } else {
            while i < len && !self.buffer.char(i).is_whitespace() && !is_word(self.buffer.char(i)) {
                i += 1;
            }
        }

        while i < len && self.buffer.char(i).is_whitespace() {
            i += 1;
        }

        if i < len {
            self.cursor = i;
        }
    }

    pub fn move_word_backward(&mut self) {
        let len = self.buffer.len_chars();
        if len == 0 || self.cursor == 0 {
            return;
        }

        let is_word = |c: char| c.is_alphanumeric() || c == '_';
        let mut i = self.cursor.min(len - 1);

        while i > 0 && self.buffer.char(i - 1).is_whitespace() {
            i -= 1;
        }

        if i == 0 {
            self.cursor = 0;
            return;
        }

        let cur = self.buffer.char(i - 1);
        if is_word(cur) {
            while i > 0 && is_word(self.buffer.char(i - 1)) {
                i -= 1;
            }
        } else {
            while i > 0
                && !self.buffer.char(i - 1).is_whitespace()
                && !is_word(self.buffer.char(i - 1))
            {
                i -= 1;
            }
        }

        self.cursor = i;
    }

    pub fn move_to_line_end(&mut self) {
        let line_idx = self.buffer.char_to_line(self.cursor);
        let line = self.buffer.line(line_idx);
        let line_len = line.len_chars();
        let line_start = self.buffer.line_to_char(line_idx);

        if line_len > 0 && line.char(line_len - 1) == '\n' {
            self.cursor = line_start + line_len - 1;
        } else {
            self.cursor = line_start + line_len;
        }
    }

    fn line_selection_end(&self, line_idx: usize) -> usize {
        let line_start = self.buffer.line_to_char(line_idx);
        let line_len = self.buffer.line(line_idx).len_chars();

        if line_len == 0 {
            line_start
        } else {
            line_start + line_len - 1
        }
    }

    pub fn select_line(&mut self) {
        if self.buffer.len_chars() == 0 {
            return;
        }

        let line_idx = self.buffer.char_to_line(self.cursor);
        let line_start = self.buffer.line_to_char(line_idx);

        self.selection_anchor = Some(line_start);
        self.cursor = self.line_selection_end(line_idx);
    }

    pub fn extend_selection_down(&mut self) {
        let Some((selection_start, selection_end)) = self.get_selection_range() else {
            return;
        };
        if selection_start == selection_end {
            return;
        }

        let last_selected = selection_end - 1;
        let next_line = self.buffer.char_to_line(last_selected) + 1;
        if next_line >= self.buffer.len_lines() {
            return;
        }

        self.selection_anchor = Some(selection_start);
        self.cursor = self.line_selection_end(next_line);
    }

    pub fn extend_selection_up(&mut self) {
        let Some((selection_start, selection_end)) = self.get_selection_range() else {
            return;
        };
        if selection_start == selection_end {
            return;
        }

        let first_line = self.buffer.char_to_line(selection_start);
        if first_line == 0 {
            return;
        }

        self.selection_anchor = Some(selection_end - 1);
        self.cursor = self.buffer.line_to_char(first_line - 1);
    }

    pub fn insert_char(&mut self, c: char) {
        let changed = match c {
            '\n' => {
                let indent = self.current_line_indent();
                self.buffer.insert(self.cursor, "\n");
                self.cursor += 1;
                self.buffer.insert(self.cursor, &indent);
                self.cursor += indent.chars().count();
                true
            }
            _ if let Some(close) = matching_pair(c) => {
                let pair_ok = if c == close {
                    self.should_autopair_quote()
                } else {
                    true
                };
                if pair_ok {
                    self.buffer.insert_char(self.cursor, c);
                    self.buffer.insert_char(self.cursor + 1, close);
                    self.cursor += 1;
                    true
                } else if self.cursor < self.buffer.len_chars()
                    && self.buffer.char(self.cursor) == c
                {
                    self.cursor += 1;
                    false
                } else {
                    self.buffer.insert_char(self.cursor, c);
                    self.cursor += 1;
                    true
                }
            }
            _ => {
                self.buffer.insert_char(self.cursor, c);
                self.cursor += 1;
                true
            }
        };
        if changed {
            self.buffer_version = self.buffer_version.wrapping_add(1);
            self.refresh_lsp();
            self.fire_event(EventKind::BufferChanged);
        }
    }

    pub fn insert_tab(&mut self) {
        for _ in 0..4 {
            self.insert_char(' ');
        }
    }

    fn current_line_indent(&self) -> String {
        let line_idx = self.buffer.char_to_line(self.cursor);
        let line = self.buffer.line(line_idx);
        let mut indent = String::new();
        for ch in line.chars() {
            if ch == ' ' || ch == '\t' {
                indent.push(ch);
            } else {
                break;
            }
        }
        indent
    }

    pub fn split_bracket_pair_at_cursor(&mut self) -> bool {
        if self.cursor >= self.buffer.len_chars() {
            return false;
        }

        let next = self.buffer.char(self.cursor);
        let Some(prev_idx) = self.cursor.checked_sub(1) else {
            return false;
        };
        let prev = self.buffer.char(prev_idx);

        let expected_close = match prev {
            '(' => ')',
            '[' => ']',
            '{' => '}',
            _ => return false,
        };

        if next != expected_close {
            return false;
        }

        self.begin_undo_group();
        self.buffer.remove(self.cursor..self.cursor + 1);
        self.buffer.insert(self.cursor, "\n    \n)");
        self.cursor += 5;
        self.buffer_version = self.buffer_version.wrapping_add(1);
        true
    }

    pub fn delete_char(&mut self) {
        if self.cursor > 0 {
            let prev = self.buffer.char(self.cursor - 1);
            if self.cursor < self.buffer.len_chars() {
                let next = self.buffer.char(self.cursor);
                if matching_pair(prev) == Some(next) {
                    self.buffer.remove(self.cursor - 1..self.cursor + 1);
                    self.cursor -= 1;
                    self.buffer_version = self.buffer_version.wrapping_add(1);
                    self.fire_event(EventKind::BufferChanged);
                    return;
                }
            }
            self.buffer.remove(self.cursor - 1..self.cursor);
            self.cursor -= 1;
            self.buffer_version = self.buffer_version.wrapping_add(1);
            self.fire_event(EventKind::BufferChanged);
        }
    }

    fn should_autopair_quote(&self) -> bool {
        if self.cursor == 0 {
            return true;
        }
        let prev = self.buffer.char(self.cursor - 1);
        matches!(
            prev,
            ' ' | '\t' | '\n' | '(' | '[' | '{' | ',' | ':' | ';' | '"' | '\'' | '`'
        )
    }

    pub fn perform_search(&mut self) {
        self.search_results.clear();
        self.search_active = false;
        self.selection_anchor = None;
        if self.search_query.is_empty() {
            return;
        }

        let text = self.buffer.to_string();
        let text_lower = text.to_lowercase();
        let query_lower = self.search_query.to_lowercase();

        let byte_to_char: Vec<usize> = {
            let mut map = Vec::with_capacity(text.len() + 1);
            let mut ci = 0;
            for (bi, _) in text.char_indices() {
                while map.len() <= bi {
                    map.push(ci);
                }
                ci += 1;
            }
            while map.len() <= text.len() {
                map.push(ci);
            }
            map
        };

        let mut pos = 0;
        while let Some(byte_start) = text_lower[pos..].find(&query_lower) {
            let abs_byte = pos + byte_start;
            let ci = byte_to_char.get(abs_byte).copied().unwrap_or(0);
            self.search_results.push(ci);
            pos = abs_byte + query_lower.len();
            if pos >= text.len() {
                break;
            }
        }

        if self.search_results.is_empty() {
            return;
        }

        self.search_active = true;
        self.search_idx = 0;

        let num_chars = text.chars().count();
        if num_chars == 0 {
            return;
        }

        for (i, &ci) in self.search_results.iter().enumerate() {
            if ci >= self.cursor {
                self.search_idx = i;
                self.cursor = ci;
                return;
            }
        }

        self.search_idx = 0;
        self.cursor = self.search_results[0];
    }

    pub fn next_match(&mut self) {
        if !self.search_active || self.search_results.is_empty() {
            return;
        }
        self.search_idx = (self.search_idx + 1) % self.search_results.len();
        self.cursor = self.search_results[self.search_idx];
    }

    pub fn prev_match(&mut self) {
        if !self.search_active || self.search_results.is_empty() {
            return;
        }
        self.search_idx = if self.search_idx == 0 {
            self.search_results.len() - 1
        } else {
            self.search_idx - 1
        };
        self.cursor = self.search_results[self.search_idx];
    }

    pub fn refresh_diff(&mut self) {
        if self.buffer_version != self.cached_highlight_version {
            self.cached_text = self.buffer.to_string();
        }
        if let Some(ref base) = self.diff_base {
            self.diff_hunks = compute_diff(base, &self.cached_text);
        } else {
            self.diff_hunks.clear();
        }
    }

    #[allow(dead_code)]
    pub fn git_repo(&self) -> Option<&GitRepo> {
        self.git_repo.as_ref()
    }

    pub fn open_file(&mut self, path: &Path) -> std::io::Result<()> {
        self.hover.clear();
        self.completion.clear();
        let content = std::fs::read_to_string(path)?;
        self.buffer = Rope::from_str(&content);
        self.buffer_version = self.buffer_version.wrapping_add(1);
        self.saved_buffer_version = self.buffer_version;
        self.cursor = 0;
        self.scroll_x = 0;
        self.scroll_y = 0;
        self.current_file = Some(path.to_path_buf());
        self.mode = Mode::Normal;
        self.pending_g = false;
        self.pending_space = false;
        self.selection_anchor = None;
        self.search_query.clear();
        self.search_results.clear();
        self.search_active = false;
        self.undo_stack.clear();
        self.redo_stack.clear();
        if let Some(path_str) = path.to_str() {
            self.highlighter.load_language_for_path(path_str);
        }
        self.restart_lsp(path);
        let repo = GitRepo::discover(path);
        self.git_repo = repo;
        self.git_branch = self.git_repo.as_ref().and_then(|r| r.current_branch());
        self.diff_base = self
            .git_repo
            .as_ref()
            .and_then(|r| r.diff_base(path))
            .map(|b| String::from_utf8_lossy(&b).to_string());
        self.buffer_version = self.buffer_version.wrapping_add(1);
        self.saved_buffer_version = self.buffer_version;
        self.refresh_diff();
        self.fire_event(EventKind::OpenFile);
        Ok(())
    }

    pub fn enter_visual_mode(&mut self) {
        self.selection_anchor = Some(self.cursor);
    }

    pub fn exit_visual_mode(&mut self) {
        self.selection_anchor = None;
    }

    pub fn get_selection_range(&self) -> Option<(usize, usize)> {
        self.selection_anchor.map(|anchor| {
            let start = anchor.min(self.cursor);
            let end = anchor.max(self.cursor);
            (start, (end + 1).min(self.buffer.len_chars()))
        })
    }

    fn get_selected_text(&self) -> Option<String> {
        self.get_selection_range()
            .map(|(start, end)| self.buffer.slice(start..end).to_string())
    }

    fn position_linewise_paste_after_selection(&mut self, text: &str) {
        if !text.ends_with('\n') {
            return;
        }

        if let Some((_, selection_end)) = self.get_selection_range() {
            self.cursor = selection_end.saturating_sub(1);
        }
    }

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

    pub fn delete_selection(&mut self) {
        if let Some((start, end)) = self.get_selection_range() {
            if start < end {
                self.begin_undo_group();
                self.clipboard = self.buffer.slice(start..end).to_string();
                self.buffer.remove(start..end);
                self.cursor = start;
                self.buffer_version = self.buffer_version.wrapping_add(1);
                self.refresh_lsp();
            }
        }
        self.exit_visual_mode();
    }

    pub fn begin_change(&mut self) {
        self.begin_undo_group();

        if let Some((start, end)) = self.get_selection_range() {
            if start < end {
                self.clipboard = self.buffer.slice(start..end).to_string();
                self.buffer.remove(start..end);
                self.cursor = start;
                self.buffer_version = self.buffer_version.wrapping_add(1);
                self.refresh_lsp();
            }
        } else if self.cursor < self.buffer.len_chars() {
            let character = self.buffer.char(self.cursor);
            if character != '\n' {
                self.clipboard = character.to_string();
                self.buffer.remove(self.cursor..self.cursor + 1);
                self.buffer_version = self.buffer_version.wrapping_add(1);
                self.refresh_lsp();
            }
        }

        self.exit_visual_mode();
    }

    fn paste_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.begin_undo_group();
        if text.ends_with('\n') {
            let line_idx = self.buffer.char_to_line(self.cursor);
            let next_line = line_idx + 1;
            if next_line < self.buffer.len_lines() {
                self.cursor = self.buffer.line_to_char(next_line);
            } else {
                self.cursor = self.buffer.len_chars();
            }
        }
        self.buffer.insert(self.cursor, text);
        self.cursor += text.chars().count();
        self.buffer_version = self.buffer_version.wrapping_add(1);
        self.refresh_lsp();
    }

    fn paste_text_after_selection(&mut self, text: &str) {
        self.position_linewise_paste_after_selection(text);
        self.exit_visual_mode();
        self.paste_text(text);
    }

    pub fn paste_clipboard(&mut self) {
        let text = self.clipboard.clone();
        self.paste_text(&text);
    }

    pub fn paste_clipboard_after_selection(&mut self) {
        let text = self.clipboard.clone();
        self.paste_text_after_selection(&text);
    }

    pub fn paste_system_clipboard(&mut self) {
        if let Ok(mut clipboard) = arboard::Clipboard::new() {
            if let Ok(text) = clipboard.get_text() {
                let text = text.to_string();
                self.clipboard = text.clone();
                self.paste_text(&text);
            }
        }
    }

    pub fn paste_system_clipboard_after_selection(&mut self) {
        let text = arboard::Clipboard::new()
            .ok()
            .and_then(|mut clipboard| clipboard.get_text().ok());

        if let Some(text) = text {
            self.clipboard = text.clone();
            self.paste_text_after_selection(&text);
        } else {
            self.exit_visual_mode();
        }
    }

    pub fn get_styled_text(
        &mut self,
        visible_start_line: usize,
        visible_height: usize,
    ) -> (Text<'static>, usize) {
        const CONTEXT_LINES: usize = 50;

        if self.buffer_version != self.cached_highlight_version {
            self.cached_text = self.buffer.to_string();
            let lang = self
                .current_file
                .as_ref()
                .and_then(|p| p.to_str())
                .and_then(|p| self.highlighter.language_for_file(p))
                .unwrap_or_default();
            self.cached_highlights = self.highlighter.highlight(&self.cached_text, &lang);
            self.cached_highlight_version = self.buffer_version;
        }

        let text = &self.cached_text;
        let highlights = &self.cached_highlights;

        let byte_to_char: Vec<usize> = {
            let mut map = Vec::with_capacity(text.len() + 1);
            let mut ci = 0;
            for (bi, _) in text.char_indices() {
                while map.len() <= bi {
                    map.push(ci);
                }
                ci += 1;
            }
            while map.len() <= text.len() {
                map.push(ci);
            }
            map
        };

        let chars: Vec<char> = text.chars().collect();
        let num_chars = chars.len();

        let search_active = self.search_active;
        let search_hl_style = self.theme.ui_get("search_match");
        let match_len = self.search_query.chars().count();
        let search_results: Vec<usize> = self.search_results.clone();

        let sel_range = self.get_selection_range();
        let sel_style = self.theme.ui_get("visual_selection");

        let line_count = self.buffer.len_lines();
        let context_start = visible_start_line.saturating_sub(CONTEXT_LINES);
        let visible_end_line = (visible_start_line + visible_height).min(line_count);
        let context_start_char = self.buffer.line_to_char(context_start);
        let visible_end_char = self.buffer.line_to_char(visible_end_line);

        self.cached_char_styles.clear();
        self.cached_char_styles
            .resize(num_chars.max(1), Style::default());
        let char_styles = &mut self.cached_char_styles;

        for (start, end, style) in highlights {
            let sci = byte_to_char
                .get(*start)
                .copied()
                .unwrap_or(0)
                .min(num_chars);
            let eci = byte_to_char
                .get(*end)
                .copied()
                .unwrap_or(num_chars)
                .min(num_chars);
            for char_style in char_styles.iter_mut().take(eci).skip(sci) {
                *char_style = *style;
            }
        }

        if search_active {
            for &start in &search_results {
                let end = (start + match_len).min(num_chars);
                for char_style in char_styles.iter_mut().take(end).skip(start) {
                    *char_style = search_hl_style;
                }
            }
        }

        if let Some((sel_start, sel_end)) = sel_range {
            for char_style in char_styles.iter_mut().take(sel_end).skip(sel_start) {
                *char_style = sel_style;
            }
        }

        apply_diagnostic_underlines(
            &self.buffer,
            &self.lsp_diagnostics,
            char_styles,
            &self.theme,
        );

        let mut lines: Vec<Line> = Vec::new();
        let mut spans: Vec<Span> = Vec::new();
        let mut seg_start = context_start_char;

        for i in context_start_char..visible_end_char {
            if chars[i] == '\n' {
                if i > seg_start {
                    let s: String = chars[seg_start..i].iter().collect();
                    spans.push(Span::styled(s, char_styles[seg_start]));
                }
                lines.push(Line::from(std::mem::take(&mut spans)));
                seg_start = i + 1;
            } else if i > seg_start && char_styles[i] != char_styles[i - 1] {
                let s: String = chars[seg_start..i].iter().collect();
                spans.push(Span::styled(s, char_styles[seg_start]));
                seg_start = i;
            }
        }

        if seg_start < visible_end_char {
            let s: String = chars[seg_start..visible_end_char].iter().collect();
            spans.push(Span::styled(s, char_styles[seg_start]));
        }
        if !spans.is_empty() {
            lines.push(Line::from(spans));
        }

        (Text::from(lines), context_start)
    }

    pub fn get_gutter_width(&self) -> usize {
        let line_count = self.buffer.len_lines();
        line_count.to_string().len() + 1
    }

    pub fn update_scroll(&mut self, width: usize, height: usize) {
        let line_idx = self.buffer.char_to_line(self.cursor);
        let col_idx = self.cursor - self.buffer.line_to_char(line_idx);

        if line_idx < self.scroll_y {
            self.scroll_y = line_idx;
        } else if height > 0 && line_idx >= self.scroll_y + height {
            self.scroll_y = line_idx - height + 1;
        }

        if col_idx < self.scroll_x {
            self.scroll_x = col_idx;
        } else if width > 0 && col_idx >= self.scroll_x + width {
            self.scroll_x = col_idx - width + 1;
        }
    }

    pub fn set_plugin_runtime(&mut self, rt: *mut PluginRuntime) {
        self.plugin_runtime = Some(rt);
    }

    pub fn fire_event(&self, kind: EventKind) {
        if let Some(ptr) = self.plugin_runtime {
            // SAFETY: single-threaded, RT lives for program lifetime
            let rt = unsafe { &mut *ptr };
            rt.fire_event(kind);
        }
    }

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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hover::HoverState;
    use crate::lsp::{Diagnostic, DiagnosticSeverity};

    fn diagnostic(
        severity: DiagnosticSeverity,
        line: usize,
        character: usize,
        end_line: usize,
        end_character: usize,
    ) -> Diagnostic {
        Diagnostic {
            severity,
            message: "diagnostic".into(),
            source: None,
            line,
            character,
            end_line,
            end_character,
        }
    }

    #[test]
    fn bare_file_name_uses_current_directory_as_lsp_root() {
        assert_eq!(lsp_root_dir(Path::new("main.ts")), Path::new("."));
    }

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

    #[test]
    fn insert_tab_expands_to_spaces() {
        let theme = Theme::default_theme();
        let mut editor = Editor::new("", None, theme, None);

        editor.insert_tab();

        assert_eq!(editor.buffer.to_string(), "    ");
        assert_eq!(editor.cursor, 4);
    }

    #[test]
    fn insert_newline_preserves_indentation() {
        let theme = Theme::default_theme();
        let mut editor = Editor::new("    hello", None, theme, None);
        editor.cursor = 9; // end of line

        editor.insert_char('\n');

        assert_eq!(editor.buffer.to_string(), "    hello\n    ");
        assert_eq!(editor.cursor, 14);
    }

    #[test]
    fn insert_newline_on_empty_line_does_nothing_extra() {
        let theme = Theme::default_theme();
        let mut editor = Editor::new("\n", None, theme, None);
        editor.cursor = 0;

        editor.insert_char('\n');

        assert_eq!(editor.buffer.to_string(), "\n\n");
        assert_eq!(editor.cursor, 1);
    }

    #[test]
    fn insert_newline_on_line_with_no_indent_just_inserts_newline() {
        let theme = Theme::default_theme();
        let mut editor = Editor::new("hello", None, theme, None);
        editor.cursor = 5;

        editor.insert_char('\n');

        assert_eq!(editor.buffer.to_string(), "hello\n");
        assert_eq!(editor.cursor, 6);
    }

    #[test]
    fn insert_newline_with_tab_indentation() {
        let theme = Theme::default_theme();
        let mut editor = Editor::new("\thello", None, theme, None);
        editor.cursor = 6;

        editor.insert_char('\n');

        assert_eq!(editor.buffer.to_string(), "\thello\n\t");
        assert_eq!(editor.cursor, 8);
    }

    #[test]
    fn insert_newline_in_middle_of_line_copies_indent_from_current_line() {
        let theme = Theme::default_theme();
        let mut editor = Editor::new("    if(x) {", None, theme, None);
        editor.cursor = 6; // middle of line

        editor.insert_char('\n');

        assert_eq!(editor.buffer.to_string(), "    if\n    (x) {");
        assert_eq!(editor.cursor, 11);
    }

    #[test]
    fn insert_newline_on_indent_only_line_copies_whitespace() {
        let theme = Theme::default_theme();
        let mut editor = Editor::new("        ", None, theme, None);
        editor.cursor = 8;

        editor.insert_char('\n');

        assert_eq!(editor.buffer.to_string(), "        \n        ");
    }

    #[test]
    fn split_bracket_pair_inserts_multiline_block() {
        let theme = Theme::default_theme();
        let mut editor = Editor::new("()", None, theme, None);
        editor.cursor = 1;

        assert!(editor.split_bracket_pair_at_cursor());
        assert_eq!(editor.buffer.to_string(), "(\n    \n)");
        assert_eq!(editor.cursor, 6);
    }

    #[test]
    fn split_bracket_pair_returns_false_outside_pair() {
        let theme = Theme::default_theme();
        let mut editor = Editor::new("abc", None, theme, None);
        editor.cursor = 1;

        assert!(!editor.split_bracket_pair_at_cursor());
        assert_eq!(editor.buffer.to_string(), "abc");
        assert_eq!(editor.cursor, 1);
    }

    #[test]
    fn diagnostic_underlines_exact_range_without_replacing_text_style() {
        let buffer = Rope::from_str("let value = 1;\n");
        let theme = Theme::default_theme();
        let base_style = Style::default().fg(Color::Green).bg(Color::Black);
        let mut styles = vec![base_style; buffer.len_chars()];
        let mut diagnostics = DiagnosticState::default();
        diagnostics.update(vec![diagnostic(DiagnosticSeverity::Warning, 0, 4, 0, 9)]);

        apply_diagnostic_underlines(&buffer, &diagnostics, &mut styles, &theme);

        for (index, style) in styles.iter().enumerate() {
            assert_eq!(style.fg, Some(Color::Green));
            assert_eq!(style.bg, Some(Color::Black));
            if (4..9).contains(&index) {
                assert!(style.add_modifier.contains(Modifier::UNDERLINED));
                assert_eq!(style.underline_color, Some(Color::Yellow));
            } else {
                assert!(!style.add_modifier.contains(Modifier::UNDERLINED));
            }
        }
    }

    #[test]
    fn diagnostic_underlines_utf16_multiline_range_without_newlines() {
        let buffer = Rope::from_str("a😀b\ncd\n");
        let theme = Theme::default_theme();
        let mut styles = vec![Style::default(); buffer.len_chars()];
        let mut diagnostics = DiagnosticState::default();
        diagnostics.update(vec![diagnostic(
            DiagnosticSeverity::Information,
            0,
            1,
            1,
            1,
        )]);

        apply_diagnostic_underlines(&buffer, &diagnostics, &mut styles, &theme);

        for index in [1, 2, 4] {
            assert!(styles[index].add_modifier.contains(Modifier::UNDERLINED));
            assert_eq!(styles[index].underline_color, Some(Color::Cyan));
        }
        assert!(!styles[3].add_modifier.contains(Modifier::UNDERLINED));
        assert!(!styles[5].add_modifier.contains(Modifier::UNDERLINED));
    }

    #[test]
    fn highest_severity_wins_and_empty_ranges_mark_one_character() {
        let buffer = Rope::from_str("abcd");
        let theme = Theme::default_theme();
        let mut styles = vec![Style::default(); buffer.len_chars()];
        let mut diagnostics = DiagnosticState::default();
        diagnostics.update(vec![
            diagnostic(DiagnosticSeverity::Warning, 0, 0, 0, 3),
            diagnostic(DiagnosticSeverity::Error, 0, 1, 0, 2),
            diagnostic(DiagnosticSeverity::Hint, 0, 3, 0, 3),
        ]);

        apply_diagnostic_underlines(&buffer, &diagnostics, &mut styles, &theme);

        assert_eq!(styles[0].underline_color, Some(Color::Yellow));
        assert_eq!(styles[1].underline_color, Some(Color::Red));
        assert_eq!(styles[2].underline_color, Some(Color::Yellow));
        assert_eq!(styles[3].underline_color, Some(Color::DarkGray));
    }

    #[test]
    fn empty_buffer_ignores_zero_length_diagnostic() {
        let buffer = Rope::new();
        let theme = Theme::default_theme();
        let mut styles = vec![Style::default()];
        let mut diagnostics = DiagnosticState::default();
        diagnostics.update(vec![diagnostic(DiagnosticSeverity::Error, 0, 0, 0, 0)]);

        apply_diagnostic_underlines(&buffer, &diagnostics, &mut styles, &theme);

        assert_eq!(styles, vec![Style::default()]);
    }

    #[test]
    fn active_diagnostic_reports_one_based_position_and_wraps() {
        let theme = Theme::default_theme();
        let mut editor = Editor::new("value\n", None, theme, None);
        editor.lsp_diagnostics.update(vec![
            diagnostic(DiagnosticSeverity::Error, 0, 0, 0, 1),
            diagnostic(DiagnosticSeverity::Warning, 0, 1, 0, 2),
        ]);

        let (_, position, total) = editor.active_diagnostic_with_position().unwrap();
        assert_eq!((position, total), (1, 2));

        editor.cycle_active_diagnostic(-1);

        let (_, position, total) = editor.active_diagnostic_with_position().unwrap();
        assert_eq!((position, total), (2, 2));
    }

    #[test]
    fn begin_change_removes_character_under_cursor_and_groups_insert_for_undo() {
        let theme = Theme::default_theme();
        let mut editor = Editor::new("abc", None, theme, None);
        editor.cursor = 1;

        editor.begin_change();
        editor.insert_char('x');

        assert_eq!(editor.buffer.to_string(), "axc");
        assert_eq!(editor.clipboard, "b");
        assert_eq!(editor.cursor, 2);

        editor.undo();

        assert_eq!(editor.buffer.to_string(), "abc");
    }

    #[test]
    fn begin_change_on_newline_preserves_newline_and_groups_insert_for_undo() {
        let theme = Theme::default_theme();
        let mut editor = Editor::new("a\nb", None, theme, None);
        editor.cursor = 1;
        editor.clipboard = "saved".into();

        editor.begin_change();
        editor.insert_char('x');

        assert_eq!(editor.buffer.to_string(), "ax\nb");
        assert_eq!(editor.clipboard, "saved");

        editor.undo();

        assert_eq!(editor.buffer.to_string(), "a\nb");
    }

    #[test]
    fn begin_change_removes_visual_selection_and_clears_selection() {
        let theme = Theme::default_theme();
        let mut editor = Editor::new("abcd", None, theme, None);
        editor.selection_anchor = Some(1);
        editor.cursor = 2;

        editor.begin_change();

        assert_eq!(editor.buffer.to_string(), "ad");
        assert_eq!(editor.clipboard, "bc");
        assert_eq!(editor.cursor, 1);
        assert_eq!(editor.selection_anchor, None);
    }

    #[test]
    fn select_line_stops_before_first_character_of_next_line() {
        let theme = Theme::default_theme();
        let mut editor = Editor::new("first\nsecond", None, theme, None);
        editor.cursor = 2;

        editor.select_line();

        assert_eq!(editor.get_selection_range(), Some((0, 6)));
        assert_eq!(editor.get_selected_text().as_deref(), Some("first\n"));
    }

    #[test]
    fn select_line_preserves_crlf_without_selecting_next_line() {
        let theme = Theme::default_theme();
        let mut editor = Editor::new("first\r\nsecond", None, theme, None);
        editor.cursor = 2;

        editor.select_line();

        assert_eq!(editor.get_selection_range(), Some((0, 7)));
        assert_eq!(editor.get_selected_text().as_deref(), Some("first\r\n"));
    }

    #[test]
    fn extending_line_selection_stops_before_following_line() {
        let theme = Theme::default_theme();
        let mut editor = Editor::new("first\nsecond\nthird", None, theme, None);
        editor.cursor = 2;
        editor.select_line();

        editor.extend_selection_down();

        assert_eq!(editor.get_selection_range(), Some((0, 13)));
        assert_eq!(editor.get_selected_text().as_deref(), Some("first\nsecond\n"));
    }

    #[test]
    fn extending_line_selection_up_includes_only_previous_line() {
        let theme = Theme::default_theme();
        let mut editor = Editor::new("first\nsecond\nthird", None, theme, None);
        editor.cursor = 8;
        editor.select_line();

        editor.extend_selection_up();

        assert_eq!(editor.get_selection_range(), Some((0, 13)));
        assert_eq!(editor.get_selected_text().as_deref(), Some("first\nsecond\n"));
    }

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
}
