use crate::theme::Theme;
use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Clear, Paragraph},
    Frame,
};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct FileEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
}

pub struct FileExplorer {
    pub visible: bool,
    current_dir: PathBuf,
    root_dir: PathBuf,
    all_entries: Vec<FileEntry>,
    pub entries: Vec<FileEntry>,
    pub selection: usize,
    pub filter: String,
    scroll: usize,
    theme: Theme,
}

impl FileExplorer {
    pub fn new(theme: Theme) -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self {
            visible: false,
            root_dir: cwd.clone(),
            current_dir: cwd,
            all_entries: Vec::new(),
            entries: Vec::new(),
            selection: 0,
            filter: String::new(),
            scroll: 0,
            theme,
        }
    }

    pub fn set_dir(&mut self, dir: PathBuf) {
        self.root_dir = dir.clone();
        self.current_dir = dir;
    }

    pub fn reset_to_root(&mut self) {
        self.current_dir = self.root_dir.clone();
        self.refresh();
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if self.visible {
            self.refresh();
        }
    }

    fn refresh(&mut self) {
        self.all_entries = read_directory(&self.current_dir);
        self.filter.clear();
        self.selection = 0;
        self.scroll = 0;
        self.entries = self.all_entries.clone();
    }

    fn apply_filter(&mut self) {
        if self.filter.is_empty() {
            self.entries = self.all_entries.clone();
        } else {
            let lower = self.filter.to_lowercase();
            self.entries = self
                .all_entries
                .iter()
                .filter(|e| e.name.to_lowercase().contains(&lower))
                .cloned()
                .collect();
        }
        self.selection = self.selection.min(self.entries.len().saturating_sub(1));
        self.scroll = self.scroll.min(self.entries.len().saturating_sub(1));
    }

    pub fn navigate_up(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        self.selection = if self.selection == 0 {
            self.entries.len() - 1
        } else {
            self.selection - 1
        };
    }

    pub fn navigate_down(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        self.selection = if self.selection + 1 >= self.entries.len() {
            0
        } else {
            self.selection + 1
        };
    }

    pub fn enter(&mut self) -> Option<PathBuf> {
        if self.entries.is_empty() {
            return None;
        }
        let entry = self.entries[self.selection].clone();
        if entry.is_dir {
            self.current_dir = entry.path;
            self.refresh();
            None
        } else {
            self.visible = false;
            Some(entry.path)
        }
    }

    pub fn go_up(&mut self) {
        if let Some(parent) = self.current_dir.parent() {
            let s = parent.to_string_lossy();
            if !s.is_empty() {
                self.current_dir = parent.to_path_buf();
                self.refresh();
            }
        }
    }

    pub fn add_filter_char(&mut self, c: char) {
        if c.is_control() {
            return;
        }
        self.filter.push(c);
        self.apply_filter();
    }

    pub fn remove_filter_char(&mut self) {
        if !self.filter.is_empty() {
            self.filter.pop();
            self.apply_filter();
        } else {
            self.go_up();
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
            .title(" File Explorer ")
            .title_alignment(Alignment::Center)
            .border_style(self.theme.ui_get("explorer_border"));
        let inner = block.inner(popup_area);
        f.render_widget(block, popup_area);

        let chunks = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).split(inner);
        let filter_area = chunks[0];
        let entries_area = chunks[1];

        let visible_height = entries_area.height as usize;

        if self.selection >= self.scroll + visible_height {
            self.scroll = self.selection - visible_height + 1;
        } else if self.selection < self.scroll {
            self.scroll = self.selection;
        }

        let filter_text = if self.filter.is_empty() {
            " Filter: ".to_string()
        } else {
            format!(" Filter: {}", self.filter)
        };
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                filter_text,
                self.theme.ui_get("explorer_filter"),
            ))),
            filter_area,
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
            let suffix = if entry.is_dir { "/" } else { "" };
            let line = format!(" {} {}{}", prefix, entry.name, suffix);

            let style = if selected {
                self.theme.ui_get("explorer_selected")
            } else if entry.is_dir {
                self.theme.ui_get("explorer_dir")
            } else {
                Style::default()
            };

            let y = entries_area.y + i as u16;
            if y < area.y + area.height {
                let line_area = Rect::new(entries_area.x, y, entries_area.width, 1);
                f.render_widget(
                    Paragraph::new(Line::from(Span::styled(line, style))),
                    line_area,
                );
            }
        }
    }
}

fn read_directory(path: &Path) -> Vec<FileEntry> {
    let mut entries = Vec::new();

    if let Ok(read_dir) = fs::read_dir(path) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let is_dir = path.is_dir();
            entries.push(FileEntry { name, path, is_dir });
        }
    }

    entries.sort_by(|a, b| {
        if a.is_dir != b.is_dir {
            if a.is_dir {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            }
        } else {
            a.name.to_lowercase().cmp(&b.name.to_lowercase())
        }
    });

    if let Some(parent) = path.parent() {
        entries.insert(
            0,
            FileEntry {
                name: "..".to_string(),
                path: parent.to_path_buf(),
                is_dir: true,
            },
        );
    }

    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    fn explorer_with_entries(count: usize) -> FileExplorer {
        let mut explorer = FileExplorer::new(Theme::default_theme());
        explorer.entries = (0..count)
            .map(|i| FileEntry {
                name: format!("file{i}"),
                path: PathBuf::from(format!("file{i}")),
                is_dir: false,
            })
            .collect();
        explorer
    }

    #[test]
    fn navigate_up_wraps_to_bottom() {
        let mut explorer = explorer_with_entries(3);
        explorer.selection = 0;

        explorer.navigate_up();

        assert_eq!(explorer.selection, 2);
    }

    #[test]
    fn navigate_down_wraps_to_top() {
        let mut explorer = explorer_with_entries(3);
        explorer.selection = 2;

        explorer.navigate_down();

        assert_eq!(explorer.selection, 0);
    }

    #[test]
    fn navigate_wraps_in_single_entry_list() {
        let mut explorer = explorer_with_entries(1);

        explorer.navigate_up();
        assert_eq!(explorer.selection, 0);

        explorer.navigate_down();
        assert_eq!(explorer.selection, 0);
    }
}
