use crate::git::{compute_diff, DiffHunk, GitRepo};
use crate::syntax::SyntaxHighlighter;
use crate::theme::Theme;
use ratatui::style::Style;
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
    diff_base: Option<String>,
    git_repo: Option<GitRepo>,
    buffer_version: u64,
    saved_buffer_version: u64,
    cached_text: String,
    cached_highlights: Vec<(usize, usize, Style)>,
    cached_highlight_version: u64,
    cached_char_styles: Vec<Style>,
    undo_stack: Vec<(Rope, usize)>,
    redo_stack: Vec<(Rope, usize)>,
}

impl Editor {
    pub fn new(text: &str, file_path: Option<&Path>, theme: Theme) -> Self {
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
        Self {
            buffer: Rope::from_str(text),
            cursor: 0,
            scroll_x: 0,
            scroll_y: 0,
            mode: Mode::Normal,
            should_quit: false,
            pending_g: false,
            pending_space: false,
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
            diff_base,
            git_repo,
            buffer_version: 0,
            saved_buffer_version: 0,
            cached_text: text.to_string(),
            cached_highlights: Vec::new(),
            cached_highlight_version: 1, // Force initial highlight on first render
            cached_char_styles: Vec::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    pub fn save(&mut self) -> anyhow::Result<()> {
        if let Some(ref path) = self.current_file {
            std::fs::write(path, self.buffer.to_string())?;
            self.saved_buffer_version = self.buffer_version;
        }
        Ok(())
    }

    pub fn is_dirty(&self) -> bool {
        self.buffer_version != self.saved_buffer_version
    }

    pub fn begin_undo_group(&mut self) {
        self.undo_stack.push((self.buffer.clone(), self.cursor));
        self.redo_stack.clear();
    }

    pub fn undo(&mut self) {
        if let Some((buffer, cursor)) = self.undo_stack.pop() {
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

    pub fn select_line(&mut self) {
        let len = self.buffer.len_chars();
        if len == 0 {
            return;
        }

        let line_idx = self.buffer.char_to_line(self.cursor);
        let line = self.buffer.line(line_idx);
        let line_len = line.len_chars();
        let line_start = self.buffer.line_to_char(line_idx);

        self.selection_anchor = Some(line_start);
        self.cursor = (line_start + line_len).min(len);
    }

    pub fn extend_selection_down(&mut self) {
        let len = self.buffer.len_chars();
        if len == 0 {
            return;
        }
        let line_idx = self.buffer.char_to_line(self.cursor);
        let next_line = line_idx + 1;
        if next_line >= self.buffer.len_lines() {
            return;
        }
        let next = self.buffer.line(next_line);
        let next_len = next.len_chars();
        let next_start = self.buffer.line_to_char(next_line);
        self.cursor = (next_start + next_len).min(len);
    }

    pub fn extend_selection_up(&mut self) {
        let len = self.buffer.len_chars();
        if len == 0 {
            return;
        }
        let line_idx = self.buffer.char_to_line(self.cursor);
        if line_idx == 0 {
            return;
        }
        let prev_line = line_idx - 1;
        let prev = self.buffer.line(prev_line);
        let prev_len = prev.len_chars();
        let prev_start = self.buffer.line_to_char(prev_line);
        self.cursor = (prev_start + prev_len).min(len);
    }

    pub fn insert_char(&mut self, c: char) {
        // Autopair: if typing an opener, insert closer too
        if let Some(close) = matching_pair(c) {
            let pair_ok = if c == close {
                self.should_autopair_quote()
            } else {
                true
            };
            if pair_ok {
                self.buffer.insert_char(self.cursor, c);
                self.buffer.insert_char(self.cursor + 1, close);
                self.cursor += 1;
                self.buffer_version = self.buffer_version.wrapping_add(1);
                return;
            }
        }

        // Skip over: if typing a closer and next char matches, just advance
        if self.cursor < self.buffer.len_chars()
            && self.buffer.char(self.cursor) == c
            && matching_pair(c) == Some(c)
        {
            self.cursor += 1;
            return;
        }

        // Normal insert
        self.buffer.insert_char(self.cursor, c);
        self.cursor += 1;
        self.buffer_version = self.buffer_version.wrapping_add(1);
    }

    pub fn insert_tab(&mut self) {
        for _ in 0..4 {
            self.insert_char(' ');
        }
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
                    return;
                }
            }
            self.buffer.remove(self.cursor - 1..self.cursor);
            self.cursor -= 1;
            self.buffer_version = self.buffer_version.wrapping_add(1);
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

    pub fn yank_selection(&mut self) {
        if let Some(text) = self.get_selected_text() {
            self.clipboard = text;
        }
        self.exit_visual_mode();
    }

    pub fn yank_selection_system(&mut self) {
        if let Some(text) = self.get_selected_text() {
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
    }

    pub fn paste_clipboard(&mut self) {
        let text = self.clipboard.clone();
        self.paste_text(&text);
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_tab_expands_to_spaces() {
        let theme = Theme::default_theme();
        let mut editor = Editor::new("", None, theme);

        editor.insert_tab();

        assert_eq!(editor.buffer.to_string(), "    ");
        assert_eq!(editor.cursor, 4);
    }

    #[test]
    fn split_bracket_pair_inserts_multiline_block() {
        let theme = Theme::default_theme();
        let mut editor = Editor::new("()", None, theme);
        editor.cursor = 1;

        assert!(editor.split_bracket_pair_at_cursor());
        assert_eq!(editor.buffer.to_string(), "(\n    \n)");
        assert_eq!(editor.cursor, 6);
    }

    #[test]
    fn split_bracket_pair_returns_false_outside_pair() {
        let theme = Theme::default_theme();
        let mut editor = Editor::new("abc", None, theme);
        editor.cursor = 1;

        assert!(!editor.split_bracket_pair_at_cursor());
        assert_eq!(editor.buffer.to_string(), "abc");
        assert_eq!(editor.cursor, 1);
    }
}
