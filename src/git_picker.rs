use std::path::PathBuf;

use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Clear, Paragraph},
    Frame,
};

use crate::git::FileChange;
use crate::theme::Theme;

pub struct GitPicker {
    pub visible: bool,
    entries: Vec<FileChange>,
    selection: usize,
    scroll: usize,
    theme: Theme,
}

impl GitPicker {
    pub fn new(theme: Theme) -> Self {
        Self {
            visible: false,
            entries: Vec::new(),
            selection: 0,
            scroll: 0,
            theme,
        }
    }

    pub fn set_entries(&mut self, entries: Vec<FileChange>) {
        self.entries = entries;
        self.selection = 0;
        self.scroll = 0;
    }

    pub fn navigate_up(&mut self) {
        if self.selection > 0 {
            self.selection -= 1;
        }
    }

    pub fn navigate_down(&mut self) {
        if !self.entries.is_empty() && self.selection < self.entries.len() - 1 {
            self.selection += 1;
        }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn enter(&mut self) -> Option<PathBuf> {
        if self.entries.is_empty() {
            return None;
        }
        self.visible = false;
        Some(self.entries[self.selection].path().to_path_buf())
    }

    fn status_style(&self, label: &str) -> Style {
        match label {
            "M" => self.theme.ui_get("git_status_modified"),
            "?" => self.theme.ui_get("git_status_untracked"),
            "D" => self.theme.ui_get("git_status_deleted"),
            "R" => self.theme.ui_get("git_status_renamed"),
            "C" => self.theme.ui_get("git_status_conflict"),
            _ => Style::default(),
        }
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        if !self.visible {
            return;
        }

        let popup_width = (area.width as f32 * 0.6).max(40.0) as u16;
        let popup_height = (area.height as f32 * 0.55).max(10.0) as u16;

        let vert = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(popup_height),
            Constraint::Fill(1),
        ])
        .split(area);
        let horiz = Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Length(popup_width),
            Constraint::Fill(1),
        ])
        .split(vert[1]);
        let popup_area = horiz[1];

        f.render_widget(Clear, popup_area);

        let block = Block::bordered()
            .title(" Git Status ")
            .title_alignment(Alignment::Center)
            .border_style(self.theme.ui_get("git_border"));
        let inner = block.inner(popup_area);
        f.render_widget(block, popup_area);

        let chunks = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).split(inner);
        let query_area = chunks[0];
        let entries_area = chunks[1];

        let visible_height = entries_area.height as usize;

        if self.selection >= self.scroll + visible_height {
            self.scroll = self.selection - visible_height + 1;
        } else if self.selection < self.scroll {
            self.scroll = self.selection;
        }

        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                " Use arrows to navigate, Enter to open, Esc to close ",
                self.theme.ui_get("git_query"),
            ))),
            query_area,
        );

        let scroll = self.scroll;
        for i in 0..visible_height {
            let idx = scroll + i;
            if idx >= self.entries.len() {
                break;
            }
            let entry = &self.entries[idx];
            let selected = idx == self.selection;

            let prefix = if selected { ">" } else { " " };
            let base_style = if selected {
                self.theme.ui_get("git_selected")
            } else {
                Style::default()
            };
            let label = entry.label();
            let status_style = self.status_style(label);

            let label = format!(" {}", label);

            let path_str = entry.path().to_string_lossy();

            let line_area = Rect::new(
                entries_area.x,
                entries_area.y + i as u16,
                entries_area.width,
                1,
            );

            if selected {
                f.render_widget(
                    Paragraph::new(Line::from(vec![
                        Span::raw(" "),
                        Span::styled(">", base_style),
                        Span::raw(" "),
                        Span::styled(label, status_style),
                        Span::raw(" "),
                        Span::styled(path_str, base_style),
                    ])),
                    line_area,
                );
            } else {
                f.render_widget(
                    Paragraph::new(Line::from(vec![
                        Span::raw("   "),
                        Span::styled(label, status_style),
                        Span::raw(" "),
                        Span::raw(path_str),
                    ])),
                    line_area,
                );
            }
        }
    }
}
