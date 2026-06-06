use std::path::{Path, PathBuf};
use ropey::Rope;
use ratatui::text::{Text, Line, Span};
use ratatui::style::Style;
use crate::syntax::SyntaxHighlighter;
use crate::theme::Theme;

#[derive(Default, PartialEq, Eq, Clone, Copy)]
pub enum Mode {
    #[default]
    Normal,
    Insert,
    Command,
    Fuzzy,
}

pub struct Editor {
    pub buffer: Rope,
    pub cursor: usize, // Char index in Rope
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
}

impl Editor {
    pub fn new(text: &str, file_path: Option<&Path>, theme: Theme) -> Self {
        let mut highlighter = SyntaxHighlighter::new(theme.clone());
        if let Some(path) = file_path {
            if let Some(path_str) = path.to_str() {
                highlighter.load_language_for_path(path_str);
            }
        }
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
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        if let Some(ref path) = self.current_file {
            std::fs::write(path, self.buffer.to_string())?;
        }
        Ok(())
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

    pub fn insert_char(&mut self, c: char) {
        self.buffer.insert_char(self.cursor, c);
        self.cursor += 1;
    }

    pub fn delete_char(&mut self) {
        if self.cursor > 0 {
            self.buffer.remove(self.cursor - 1..self.cursor);
            self.cursor -= 1;
        }
    }

    pub fn open_file(&mut self, path: &Path) -> std::io::Result<()> {
        let content = std::fs::read_to_string(path)?;
        self.buffer = Rope::from_str(&content);
        self.cursor = 0;
        self.scroll_x = 0;
        self.scroll_y = 0;
        self.current_file = Some(path.to_path_buf());
        self.mode = Mode::Normal;
        self.pending_g = false;
        self.pending_space = false;
        if let Some(path_str) = path.to_str() {
            self.highlighter.load_language_for_path(path_str);
        }
        Ok(())
    }

    pub fn get_styled_text(&mut self) -> Text<'static> {
        let text = self.buffer.to_string();
        let lang = self.current_file.as_ref()
            .and_then(|p| p.to_str())
            .and_then(|p| self.highlighter.language_for_file(p))
            .unwrap_or_default();

        let highlights = self.highlighter.highlight(&text, &lang);

        if highlights.is_empty() {
            return Text::from(text);
        }

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
        let mut char_styles = vec![Style::default(); num_chars.max(1)];
        for (start, end, style) in &highlights {
            let sci = byte_to_char.get(*start).copied().unwrap_or(0).min(num_chars);
            let eci = byte_to_char.get(*end).copied().unwrap_or(num_chars).min(num_chars);
            for ci in sci..eci {
                char_styles[ci] = *style;
            }
        }

        let mut lines: Vec<Line> = Vec::new();
        let mut spans: Vec<Span> = Vec::new();
        let mut seg_start = 0;

        for i in 0..num_chars {
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

        if seg_start < num_chars {
            let s: String = chars[seg_start..].iter().collect();
            spans.push(Span::styled(s, char_styles[seg_start]));
        }
        if !spans.is_empty() {
            lines.push(Line::from(spans));
        }

        Text::from(lines)
    }

    pub fn get_gutter_width(&self) -> usize {
        let line_count = self.buffer.len_lines();
        line_count.to_string().len() + 1
    }

    pub fn update_scroll(&mut self, width: usize, height: usize) {
        let line_idx = self.buffer.char_to_line(self.cursor);
        let col_idx = self.cursor - self.buffer.line_to_char(line_idx);

        // Vertical scroll
        if line_idx < self.scroll_y {
            self.scroll_y = line_idx;
        } else if height > 0 && line_idx >= self.scroll_y + height {
            self.scroll_y = line_idx - height + 1;
        }

        // Horizontal scroll
        if col_idx < self.scroll_x {
            self.scroll_x = col_idx;
        } else if width > 0 && col_idx >= self.scroll_x + width {
            self.scroll_x = col_idx - width + 1;
        }
    }
}
