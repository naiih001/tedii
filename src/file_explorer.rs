use crate::theme::Theme;
use crate::overlay::{ListPopup, PopupConfig};
use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use std::fs;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct FileEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
}

pub struct FileExplorer {
    list: ListPopup<FileEntry>,
    current_dir: PathBuf,
    root_dir: PathBuf,
    all_entries: Vec<FileEntry>,
    theme: Theme,
}

impl Deref for FileExplorer {
    type Target = ListPopup<FileEntry>;

    fn deref(&self) -> &Self::Target {
        &self.list
    }
}

impl DerefMut for FileExplorer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.list
    }
}

impl FileExplorer {
    pub fn new(theme: Theme) -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self {
            list: ListPopup::new(PopupConfig {
                title: "File Explorer".to_string(),
                filter_label: "Filter".to_string(),
                width_pct: 0.6,
                height_pct: 0.55,
                min_width: 40,
                min_height: 10,
                wrap: true,
                border_key: "overlay_border".to_string(),
                filter_key: "overlay_filter".to_string(),
            }),
            root_dir: cwd.clone(),
            current_dir: cwd,
            all_entries: Vec::new(),
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
        self.list.reset_filter();
        self.list.entries = self.all_entries.clone();
    }

    fn apply_filter(&mut self) {
        if self.filter.is_empty() {
            self.list.entries = self.all_entries.clone();
        } else {
            let lower = self.filter.to_lowercase();
            self.list.entries = self
                .all_entries
                .iter()
                .filter(|e| e.name.to_lowercase().contains(&lower))
                .cloned()
                .collect();
        }
        self.list.selection = self
            .list
            .selection
            .min(self.list.entries.len().saturating_sub(1));
        self.list.scroll = self
            .list
            .scroll
            .min(self.list.entries.len().saturating_sub(1));
    }

    pub fn navigate_up(&mut self) {
        self.list.navigate_up();
    }

    pub fn navigate_down(&mut self) {
        self.list.navigate_down();
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
        self.list.add_filter_char(c);
        self.apply_filter();
    }

    pub fn remove_filter_char(&mut self) {
        if !self.filter.is_empty() {
            self.list.remove_filter_char();
            self.apply_filter();
        } else {
            self.go_up();
        }
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        self.list
            .render_popup(f, area, &self.theme, render_file_entry);
    }
}

fn render_file_entry(entry: &FileEntry, selected: bool, theme: &Theme, f: &mut Frame, area: Rect) {
    let prefix = if selected { ">" } else { " " };
    let suffix = if entry.is_dir { "/" } else { "" };
    let style = if selected {
        theme.ui_get("overlay_selected")
    } else if entry.is_dir {
        theme.ui_get("overlay_dir")
    } else {
        Style::default()
    };
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(" {} {}{}", prefix, entry.name, suffix),
            style,
        ))),
        area,
    );
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
        explorer.list.entries = (0..count)
            .map(|i| FileEntry {
                name: format!("file{i}"),
                path: PathBuf::from(format!("file{i}")),
                is_dir: false,
            })
            .collect();
        explorer
    }

    #[test]
    fn filter_matches_names_case_insensitively() {
        let mut explorer = explorer_with_entries(0);
        explorer.all_entries = vec![
            FileEntry {
                name: "README.md".to_string(),
                path: PathBuf::from("README.md"),
                is_dir: false,
            },
            FileEntry {
                name: "src".to_string(),
                path: PathBuf::from("src"),
                is_dir: true,
            },
        ];

        explorer.add_filter_char('r');

        assert_eq!(explorer.list.entries.len(), 2);
        explorer.add_filter_char('e');
        assert_eq!(explorer.list.entries.len(), 1);
        assert_eq!(explorer.list.entries[0].name, "README.md");
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
