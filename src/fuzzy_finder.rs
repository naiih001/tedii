use std::path::{Path, PathBuf};
use std::fs;
use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Clear, Paragraph},
    Frame,
};
use crate::theme::Theme;

#[derive(Clone)]
pub struct ScoredEntry {
    pub path: PathBuf,
    pub display: String,
    pub is_dir: bool,
    pub score: i64,
    pub indices: Vec<usize>,
}

pub struct FuzzyFinder {
    pub visible: bool,
    root_dir: PathBuf,
    original_dir: PathBuf,
    all_entries: Vec<ScoredEntry>,
    entries: Vec<ScoredEntry>,
    selection: usize,
    query: String,
    scroll: usize,
    theme: Theme,
}

impl FuzzyFinder {
    pub fn new(theme: Theme) -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self {
            visible: false,
            root_dir: cwd.clone(),
            original_dir: cwd,
            all_entries: Vec::new(),
            entries: Vec::new(),
            selection: 0,
            query: String::new(),
            scroll: 0,
            theme,
        }
    }

    pub fn set_dir(&mut self, dir: PathBuf) {
        self.original_dir = dir.clone();
        self.root_dir = dir;
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if self.visible {
            self.refresh();
        }
    }

    pub fn reset_to_root(&mut self) {
        self.root_dir = self.original_dir.clone();
        self.refresh();
    }

    fn refresh(&mut self) {
        self.all_entries = collect_files(&self.root_dir);
        self.query.clear();
        self.selection = 0;
        self.scroll = 0;
        self.apply_query();
    }

    fn apply_query(&mut self) {
        if self.query.is_empty() {
            self.entries = self.all_entries.clone();
        } else {
            let mut scored: Vec<ScoredEntry> = self.all_entries
                .iter()
                .filter_map(|e| {
                    fuzzy_score(&self.query, &e.display).map(|(score, indices)| {
                        let mut entry = e.clone();
                        entry.score = score;
                        entry.indices = indices;
                        entry
                    })
                })
                .collect();
            scored.sort_by(|a, b| {
                b.score.cmp(&a.score)
                    .then(a.display.len().cmp(&b.display.len()))
            });
            self.entries = scored;
        }
        self.selection = self.selection.min(self.entries.len().saturating_sub(1));
        self.scroll = self.scroll.min(self.entries.len().saturating_sub(1));
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

    pub fn enter(&mut self) -> Option<PathBuf> {
        if self.entries.is_empty() {
            return None;
        }
        let entry = self.entries[self.selection].clone();
        if entry.is_dir {
            self.root_dir = entry.path;
            self.refresh();
            None
        } else {
            self.visible = false;
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
        self.query.push(c);
        self.apply_query();
    }

    pub fn remove_query_char(&mut self) {
        if !self.query.is_empty() {
            self.query.pop();
            self.apply_query();
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
            .title(" Fuzzy Finder ")
            .title_alignment(Alignment::Center)
            .border_style(self.theme.ui_get("fuzzy_border"));
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

        let query_text = if self.query.is_empty() {
            " Query: ".to_string()
        } else {
            format!(" Query: {}", self.query)
        };
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                query_text,
                self.theme.ui_get("fuzzy_query"),
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
            let suffix = if entry.is_dir { "/" } else { "" };

            let base_style = if selected {
                self.theme.ui_get("fuzzy_selected")
            } else if entry.is_dir {
                self.theme.ui_get("fuzzy_dir")
            } else {
                Style::default()
            };

            if !self.query.is_empty() && !entry.indices.is_empty() {
                let mut spans = vec![
                    if selected {
                        Span::styled(">", base_style)
                    } else {
                        Span::raw(" ")
                    },
                    Span::raw(" "),
                ];
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
                        self.theme.ui_get("fuzzy_match"),
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

                let line_area = Rect::new(
                    entries_area.x,
                    entries_area.y + i as u16,
                    entries_area.width,
                    1,
                );
                f.render_widget(Paragraph::new(Line::from(spans)), line_area);
            } else {
                let line = format!(" {} {}{}", prefix, entry.display, suffix);
                let line_area = Rect::new(
                    entries_area.x,
                    entries_area.y + i as u16,
                    entries_area.width,
                    1,
                );
                f.render_widget(
                    Paragraph::new(Line::from(Span::styled(line, base_style))),
                    line_area,
                );
            }
        }
    }
}

fn fuzzy_score(query: &str, target: &str) -> Option<(i64, Vec<usize>)> {
    let query = query.to_lowercase();
    let target = target.to_lowercase();
    let qchars: Vec<char> = query.chars().collect();
    let tchars: Vec<char> = target.chars().collect();

    if qchars.is_empty() {
        return None;
    }

    let mut score: i64 = 0;
    let mut indices = Vec::new();
    let mut qi = 0;

    for (ti, tc) in tchars.iter().enumerate() {
        if qi < qchars.len() && *tc == qchars[qi] {
            indices.push(ti);
            if qi > 0 && indices[qi - 1] == ti - 1 {
                score += 15;
            }
            if ti == 0 || matches!(tchars[ti - 1], '/' | '-' | '_' | '.' | ' ') {
                score += 20;
            }
            score += 10;
            qi += 1;
        }
    }

    if qi == qchars.len() {
        if indices.len() > 1 {
            let gap =
                *indices.last().unwrap() as i64 - *indices.first().unwrap() as i64 - indices.len() as i64 + 1;
            score -= 3 * gap;
        }
        Some((score, indices))
    } else {
        None
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
            let Ok(ftype) = entry.file_type() else { continue };
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
