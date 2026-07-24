use crate::fuzzy::fuzzy_score;
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
pub struct ScoredEntry {
    pub path: PathBuf,
    pub display: String,
    pub is_dir: bool,
    pub score: i64,
    pub indices: Vec<usize>,
}

pub struct FuzzyFinder {
    list: ListPopup<ScoredEntry>,
    root_dir: PathBuf,
    original_dir: PathBuf,
    all_entries: Vec<ScoredEntry>,
    theme: Theme,
}

impl Deref for FuzzyFinder {
    type Target = ListPopup<ScoredEntry>;

    fn deref(&self) -> &Self::Target {
        &self.list
    }
}

impl DerefMut for FuzzyFinder {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.list
    }
}

impl FuzzyFinder {
    pub fn new(theme: Theme) -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self {
            list: ListPopup::new(PopupConfig {
                title: "Fuzzy Finder".to_string(),
                filter_label: "Query".to_string(),
                width_pct: 0.6,
                height_pct: 0.55,
                min_width: 40,
                min_height: 10,
                wrap: true,
                border_key: "fuzzy_border".to_string(),
                filter_key: "fuzzy_query".to_string(),
            }),
            root_dir: cwd.clone(),
            original_dir: cwd,
            all_entries: Vec::new(),
            theme,
        }
    }

    pub fn set_dir(&mut self, dir: PathBuf) {
        self.original_dir = dir.clone();
        self.root_dir = dir;
    }

    pub fn toggle(&mut self) {
        self.list.visible = !self.list.visible;
        if self.list.visible {
            self.refresh();
        }
    }

    pub fn reset_to_root(&mut self) {
        self.root_dir = self.original_dir.clone();
        self.refresh();
    }

    fn refresh(&mut self) {
        self.all_entries = collect_files(&self.root_dir);
        self.list.reset_filter();
        self.apply_query();
    }

    fn apply_query(&mut self) {
        if self.list.filter.is_empty() {
            self.list.entries = self.all_entries.clone();
        } else {
            let mut scored: Vec<ScoredEntry> = self
                .all_entries
                .iter()
                .filter_map(|e| {
                    fuzzy_score(&self.list.filter, &e.display).map(|(score, indices)| {
                        let mut entry = e.clone();
                        entry.score = score;
                        entry.indices = indices;
                        entry
                    })
                })
                .collect();
            scored.sort_by(|a, b| {
                b.score
                    .cmp(&a.score)
                    .then(a.display.len().cmp(&b.display.len()))
            });
            self.list.entries = scored;
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

    pub fn enter(&mut self) -> Option<PathBuf> {
        if self.list.entries.is_empty() {
            return None;
        }
        let entry = self.list.entries[self.list.selection].clone();
        if entry.is_dir {
            self.root_dir = entry.path;
            self.refresh();
            None
        } else {
            self.list.visible = false;
            Some(entry.path)
        }
    }

    fn go_up(&mut self) {
        if let Some(parent) = self.root_dir.parent() {
            let s = parent.to_string_lossy();
            if !s.is_empty() {
                self.root_dir = parent.to_path_buf();
                self.refresh();
            }
        }
    }

    pub fn add_query_char(&mut self, c: char) {
        if c.is_control() {
            return;
        }
        self.list.add_filter_char(c);
        self.apply_query();
    }

    pub fn remove_query_char(&mut self) {
        if !self.list.filter.is_empty() {
            self.list.remove_filter_char();
            self.apply_query();
        } else {
            self.go_up();
        }
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        self.list
            .render_popup(f, area, &self.theme, render_scored_entry);
    }
}

fn render_scored_entry(
    entry: &ScoredEntry,
    selected: bool,
    theme: &Theme,
    f: &mut Frame,
    area: Rect,
) {
    let prefix = if selected { ">" } else { " " };
    let suffix = if entry.is_dir { "/" } else { "" };
    let base_style = if selected {
        theme.ui_get("fuzzy_selected")
    } else if entry.is_dir {
        theme.ui_get("fuzzy_dir")
    } else {
        Style::default()
    };

    if !entry.indices.is_empty() {
        let mut spans = vec![Span::styled(format!("{} ", prefix), base_style)];
        let display_chars: Vec<char> = entry.display.chars().collect();
        let mut last = 0;
        for &mi in &entry.indices {
            if mi > last {
                spans.push(Span::styled(
                    display_chars[last..mi].iter().collect::<String>(),
                    base_style,
                ));
            }
            spans.push(Span::styled(
                display_chars[mi].to_string(),
                theme.ui_get("fuzzy_match"),
            ));
            last = mi + 1;
        }
        if last < display_chars.len() {
            spans.push(Span::styled(
                display_chars[last..].iter().collect::<String>(),
                base_style,
            ));
        }
        if entry.is_dir {
            spans.push(Span::styled("/", base_style));
        }
        f.render_widget(Paragraph::new(Line::from(spans)), area);
    } else {
        let line = format!(" {} {}{}", prefix, entry.display, suffix);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(line, base_style))),
            area,
        );
    }
}

fn collect_files(root: &Path) -> Vec<ScoredEntry> {
    let mut entries = Vec::new();
    if let Some(parent) = root.parent() {
        entries.push(ScoredEntry {
            display: "..".to_string(),
            path: parent.to_path_buf(),
            is_dir: true,
            score: 0,
            indices: Vec::new(),
        });
    }
    walk_dir(root, root, &mut entries);
    entries.sort_by_key(|a| a.display.to_lowercase());
    entries
}

fn walk_dir(base: &Path, dir: &Path, entries: &mut Vec<ScoredEntry>) {
    if let Ok(read_dir) = fs::read_dir(dir) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let Ok(ftype) = entry.file_type() else {
                continue;
            };
            if ftype.is_symlink() {
                continue;
            }
            let is_dir = ftype.is_dir();
            let display = path
                .strip_prefix(base)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            entries.push(ScoredEntry {
                display,
                path: path.clone(),
                is_dir,
                score: 0,
                indices: Vec::new(),
            });
            if is_dir {
                walk_dir(base, &path, entries);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finder_with_entries(count: usize) -> FuzzyFinder {
        let mut finder = FuzzyFinder::new(Theme::default_theme());
        finder.list.entries = (0..count)
            .map(|i| ScoredEntry {
                path: PathBuf::from(format!("file{i}")),
                display: format!("file{i}"),
                is_dir: false,
                score: 0,
                indices: Vec::new(),
            })
            .collect();
        finder
    }

    #[test]
    fn navigate_up_wraps_to_bottom() {
        let mut finder = finder_with_entries(3);
        finder.selection = 0;

        finder.navigate_up();

        assert_eq!(finder.selection, 2);
    }

    #[test]
    fn navigate_down_wraps_to_top() {
        let mut finder = finder_with_entries(3);
        finder.selection = 2;

        finder.navigate_down();

        assert_eq!(finder.selection, 0);
    }

    #[test]
    fn navigate_wraps_in_single_entry_list() {
        let mut finder = finder_with_entries(1);

        finder.navigate_up();
        assert_eq!(finder.selection, 0);

        finder.navigate_down();
        assert_eq!(finder.selection, 0);
    }
}
