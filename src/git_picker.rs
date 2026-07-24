use std::path::{Path, PathBuf};

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::git::{ChangeSection, FileChange, GitRepo};
use crate::overlay::{centered_popup, render_popup_shell, ListPopup, PopupConfig};
use crate::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitPage {
    Status,
    Commit,
    Log,
    Diff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StageAction {
    Stage,
    Unstage,
}

#[derive(Debug, Clone)]
struct Feedback {
    message: String,
    is_error: bool,
}

#[derive(Debug, Clone)]
enum StatusRow {
    Header(String),
    Entry(usize),
    Text(String),
}

pub struct GitPicker {
    pub visible: bool,
    repo: Option<GitRepo>,
    entries: Vec<FileChange>,
    selection: usize,
    page: GitPage,
    page_title: String,
    page_lines: Vec<String>,
    page_scroll: usize,
    viewport_rows: usize,
    commit_message: String,
    feedback: Option<Feedback>,
    theme: Theme,
    list: ListPopup<StatusRow>,
    config: PopupConfig,
}

impl GitPicker {
    pub fn new(theme: Theme) -> Self {
        Self {
            visible: false,
            repo: None,
            entries: Vec::new(),
            selection: 0,
            page: GitPage::Status,
            page_title: String::new(),
            page_lines: Vec::new(),
            page_scroll: 0,
            viewport_rows: 1,
            commit_message: String::new(),
            feedback: None,
            theme,
            list: ListPopup::new(PopupConfig {
                title: "Git Status".to_string(),
                filter_label: String::new(),
                width_pct: 0.72,
                height_pct: 0.68,
                min_width: 40,
                min_height: 12,
                wrap: false,
                border_key: "git_border".to_string(),
                filter_key: "git_query".to_string(),
            }),
            config: PopupConfig {
                title: "Git Status".to_string(),
                filter_label: String::new(),
                width_pct: 0.72,
                height_pct: 0.68,
                min_width: 40,
                min_height: 12,
                wrap: false,
                border_key: "git_border".to_string(),
                filter_key: "git_query".to_string(),
            },
        }
    }

    pub fn open(&mut self, context: &Path) -> bool {
        let Some(repo) = GitRepo::discover(context) else {
            return false;
        };
        self.repo = Some(repo);
        self.visible = true;
        self.page = GitPage::Status;
        self.commit_message.clear();
        self.page_lines.clear();
        self.page_scroll = 0;
        self.feedback = None;
        self.refresh_entries(None);
        true
    }

    pub fn page(&self) -> GitPage {
        self.page
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.page = GitPage::Status;
        self.commit_message.clear();
        self.page_lines.clear();
        self.page_scroll = 0;
    }

    pub fn navigate_up(&mut self) {
        self.list
            .navigate_up_skip(|row| !matches!(row, StatusRow::Entry(_)));
        if let Some(idx) = self.current_entry_index() {
            self.selection = idx;
        }
    }

    pub fn navigate_down(&mut self) {
        self.list
            .navigate_down_skip(|row| !matches!(row, StatusRow::Entry(_)));
        if let Some(idx) = self.current_entry_index() {
            self.selection = idx;
        }
    }

    pub fn enter(&mut self) -> Option<PathBuf> {
        let path = self.entries.get(self.selection)?.path.clone();
        if !path.is_file() {
            self.set_error(format!("File does not exist: {}", self.display_path(&path)));
            return None;
        }
        self.close();
        Some(path)
    }

    pub fn toggle_stage(&mut self) {
        let Some(change) = self.entries.get(self.selection).cloned() else {
            self.set_error("No file selected");
            return;
        };
        let Some(repo) = &self.repo else {
            self.set_error("No Git repository is open");
            return;
        };

        let target_section = match change.section {
            ChangeSection::Staged => ChangeSection::Unstaged,
            ChangeSection::Unstaged => ChangeSection::Staged,
        };
        let action = self.selected_stage_action();
        let result = match action {
            Some(StageAction::Stage) => repo.stage(&change),
            Some(StageAction::Unstage) => repo.unstage(&change),
            None => return,
        };

        match result {
            Ok(()) => {
                let verb = match action {
                    Some(StageAction::Stage) => "Staged",
                    Some(StageAction::Unstage) => "Unstaged",
                    None => unreachable!(),
                };
                self.feedback = Some(Feedback {
                    message: format!("{verb} {}", self.display_path(&change.path)),
                    is_error: false,
                });
                self.refresh_entries(Some((&change.path, target_section)));
            }
            Err(error) => self.set_error(error),
        }
    }

    pub fn begin_commit(&mut self) {
        self.page = GitPage::Commit;
        self.commit_message.clear();
        self.feedback = None;
    }

    pub fn add_commit_char(&mut self, c: char) {
        if !c.is_control() {
            self.commit_message.push(c);
        }
    }

    pub fn remove_commit_char(&mut self) {
        self.commit_message.pop();
    }

    pub fn submit_commit(&mut self) {
        if self.commit_message.trim().is_empty() {
            self.set_error("Commit message cannot be empty");
            return;
        }
        let Some(repo) = &self.repo else {
            self.set_error("No Git repository is open");
            return;
        };

        match repo.commit(self.commit_message.trim()) {
            Ok(()) => {
                let message = self.commit_message.trim().to_string();
                self.page = GitPage::Status;
                self.commit_message.clear();
                self.feedback = Some(Feedback {
                    message: format!("Committed: {message}"),
                    is_error: false,
                });
                self.refresh_entries(None);
            }
            Err(error) => self.set_error(error),
        }
    }

    pub fn open_log(&mut self) {
        let Some(repo) = &self.repo else {
            self.set_error("No Git repository is open");
            return;
        };
        match repo.log(100) {
            Ok(entries) => {
                self.page = GitPage::Log;
                self.page_title = "Git Log".to_string();
                self.page_lines = entries
                    .into_iter()
                    .map(|entry| {
                        format!(
                            "{}  {}  {}  {}",
                            entry.short_hash, entry.subject, entry.author, entry.date
                        )
                    })
                    .collect();
                if self.page_lines.is_empty() {
                    self.page_lines.push("No commits yet.".to_string());
                }
                self.page_scroll = 0;
                self.feedback = None;
            }
            Err(error) => self.set_error(error),
        }
    }

    pub fn open_diff(&mut self) {
        let Some(change) = self.entries.get(self.selection).cloned() else {
            self.set_error("Select a changed file before opening a diff");
            return;
        };
        let Some(repo) = &self.repo else {
            self.set_error("No Git repository is open");
            return;
        };
        match repo.diff_from_head(&change.path) {
            Ok(diff) => {
                self.page = GitPage::Diff;
                self.page_title = format!("Diff: {}", self.display_path(&change.path));
                self.page_lines = diff.lines().map(str::to_string).collect();
                if self.page_lines.is_empty() {
                    self.page_lines
                        .push("No changes against HEAD for this file.".to_string());
                }
                self.page_scroll = 0;
                self.feedback = None;
            }
            Err(error) => self.set_error(error),
        }
    }

    pub fn back_to_status(&mut self) {
        self.page = GitPage::Status;
        self.commit_message.clear();
        self.page_lines.clear();
        self.page_scroll = 0;
        self.feedback = None;
    }

    pub fn scroll_page(&mut self, delta: isize) {
        let max_scroll = self.page_lines.len().saturating_sub(self.viewport_rows);
        self.page_scroll = self
            .page_scroll
            .saturating_add_signed(delta)
            .min(max_scroll);
    }

    pub fn scroll_viewport(&mut self, direction: isize) {
        let amount = self.viewport_rows.saturating_sub(1).max(1) as isize;
        self.scroll_page(direction.saturating_mul(amount));
    }

    fn current_entry_index(&self) -> Option<usize> {
        match self.list.entries.get(self.list.selection)? {
            StatusRow::Entry(idx) => Some(*idx),
            _ => None,
        }
    }

    fn selected_stage_action(&self) -> Option<StageAction> {
        match self.entries.get(self.selection)?.section {
            ChangeSection::Staged => Some(StageAction::Unstage),
            ChangeSection::Unstaged => Some(StageAction::Stage),
        }
    }

    fn refresh_entries(&mut self, preferred: Option<(&Path, ChangeSection)>) {
        let Some(repo) = &self.repo else {
            self.entries.clear();
            self.list.entries.clear();
            self.selection = 0;
            return;
        };
        match repo.status() {
            Ok(entries) => {
                self.entries = entries;
                self.selection = preferred
                    .and_then(|(path, section)| {
                        self.entries.iter().position(|entry| {
                            entry.path == path && entry.section == section
                        })
                    })
                    .unwrap_or_else(|| {
                        self.selection.min(self.entries.len().saturating_sub(1))
                    });
                self.rebuild_status_rows();
            }
            Err(error) => self.set_error(error),
        }
    }

    fn set_error(&mut self, error: impl Into<String>) {
        self.feedback = Some(Feedback {
            message: error
                .into()
                .lines()
                .next()
                .unwrap_or("Git command failed")
                .to_string(),
            is_error: true,
        });
    }

    fn display_path(&self, path: &Path) -> String {
        self.repo
            .as_ref()
            .and_then(|repo| path.strip_prefix(repo.work_dir()).ok())
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned()
    }

    fn status_rows(&self) -> Vec<StatusRow> {
        let staged_count = self
            .entries
            .iter()
            .filter(|entry| entry.section == ChangeSection::Staged)
            .count();
        let unstaged_count = self.entries.len().saturating_sub(staged_count);
        let mut rows = vec![StatusRow::Header(format!("Staged ({staged_count})"))];
        rows.extend(
            self.entries
                .iter()
                .enumerate()
                .filter(|(_, entry)| entry.section == ChangeSection::Staged)
                .map(|(index, _)| StatusRow::Entry(index)),
        );
        rows.push(StatusRow::Header(format!("Unstaged ({unstaged_count})")));
        rows.extend(
            self.entries
                .iter()
                .enumerate()
                .filter(|(_, entry)| entry.section == ChangeSection::Unstaged)
                .map(|(index, _)| StatusRow::Entry(index)),
        );
        if self.entries.is_empty() {
            rows.push(StatusRow::Text("Working tree clean.".to_string()));
        }
        rows
    }

    fn rebuild_status_rows(&mut self) {
        self.list.entries = self.status_rows();
        let row_pos = self
            .list
            .entries
            .iter()
            .position(|row| matches!(row, StatusRow::Entry(idx) if *idx == self.selection));
        if let Some(pos) = row_pos {
            self.list.selection = pos;
        } else if let Some(first_entry) =
            self.list
                .entries
                .iter()
                .position(|row| matches!(row, StatusRow::Entry(_)))
        {
            self.list.selection = first_entry;
            if let StatusRow::Entry(idx) = &self.list.entries[first_entry] {
                self.selection = *idx;
            }
        } else {
            self.list.selection = 0;
        }
        self.list.scroll = 0;
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        if !self.visible || area.width < 4 || area.height < 4 {
            return;
        }

        if self.page == GitPage::Status {
            self.rebuild_status_rows();
        }

        let render_config = PopupConfig {
            title: match self.page {
                GitPage::Status | GitPage::Commit => "Git Status".to_string(),
                GitPage::Log | GitPage::Diff => self.page_title.clone(),
            },
            ..self.config.clone()
        };

        let Some(popup) = centered_popup(area, &render_config) else {
            return;
        };
        render_popup_shell(f, &popup, &render_config, &self.theme);

        let feedback_height = u16::from(self.feedback.is_some());
        let chunks = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(feedback_height),
        ])
        .split(popup.inner);

        let hint = match self.page {
            GitPage::Status => {
                " Space stage/unstage  c commit  l log  d diff  Enter open  Esc close "
            }
            GitPage::Commit => " Enter commit  Esc cancel ",
            GitPage::Log | GitPage::Diff => {
                " j/k or arrows scroll  Ctrl+d/u or PgDn/PgUp page  Esc back "
            }
        };
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                hint,
                self.theme.ui_get("git_query"),
            ))),
            chunks[0],
        );

        self.viewport_rows = chunks[1].height as usize;
        match self.page {
            GitPage::Status => self.render_status(f, chunks[1]),
            GitPage::Commit => self.render_commit(f, chunks[1]),
            GitPage::Log | GitPage::Diff => self.render_page(f, chunks[1]),
        }

        if let Some(feedback) = &self.feedback {
            let style = if feedback.is_error {
                self.theme.ui_get("git_error")
            } else {
                self.theme.ui_get("git_success")
            };
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    format!(" {}", feedback.message),
                    style,
                ))),
                chunks[2],
            );
        }
    }

    fn render_status(&mut self, f: &mut Frame, area: Rect) {
        let entries = &self.entries;
        let work_dir = self.repo.as_ref().map(|r| r.work_dir().to_path_buf());
        self.list.render_entries(f, area, &self.theme, |row, selected, theme, f, line_area| {
            match row {
                StatusRow::Header(title) => {
                    f.render_widget(
                        Paragraph::new(Line::from(Span::styled(
                            format!(" {title}"),
                            theme.ui_get("git_section"),
                        ))),
                        line_area,
                    );
                }
                StatusRow::Text(text) => {
                    f.render_widget(
                        Paragraph::new(Line::from(Span::styled(
                            format!(" {text}"),
                            theme.ui_get("git_page_text"),
                        ))),
                        line_area,
                    );
                }
                StatusRow::Entry(idx) => {
                    let entry = &entries[*idx];
                    let base_style = if selected {
                        theme.ui_get("git_selected")
                    } else {
                        Style::default()
                    };
                    let label = entry.label();
                    let display = entries_display_path(&work_dir, entries, idx, &entry.original_path);
                    f.render_widget(
                        Paragraph::new(Line::from(vec![
                            Span::styled(if selected { " > " } else { "   " }, base_style),
                            Span::styled(format!("{label} "), git_status_style(theme, label)),
                            Span::styled(display, base_style),
                        ])),
                        line_area,
                    );
                }
            }
        });
    }

    fn render_commit(&self, f: &mut Frame, area: Rect) {
        let prompt = Line::from(vec![
            Span::styled(" Commit message: ", self.theme.ui_get("git_section")),
            Span::styled(
                self.commit_message.as_str(),
                self.theme.ui_get("git_page_text"),
            ),
            Span::styled("_", self.theme.ui_get("git_query")),
        ]);
        f.render_widget(Paragraph::new(prompt), area);
    }

    fn render_page(&mut self, f: &mut Frame, area: Rect) {
        let visible_height = area.height as usize;
        let max_scroll = self.page_lines.len().saturating_sub(visible_height);
        self.page_scroll = self.page_scroll.min(max_scroll);
        let lines = self
            .page_lines
            .iter()
            .skip(self.page_scroll)
            .take(visible_height)
            .map(|line| Line::from(Span::styled(line.as_str(), self.page_line_style(line))))
            .collect::<Vec<_>>();
        f.render_widget(Paragraph::new(lines), area);
    }

    fn page_line_style(&self, line: &str) -> Style {
        if self.page == GitPage::Diff {
            if line.starts_with('+') && !line.starts_with("+++") {
                return self.theme.ui_get("git_status_added");
            }
            if line.starts_with('-') && !line.starts_with("---") {
                return self.theme.ui_get("git_status_deleted");
            }
            if line.starts_with("@@") {
                return self.theme.ui_get("git_query");
            }
        }
        self.theme.ui_get("git_page_text")
    }
}

fn git_status_style(theme: &Theme, label: &str) -> Style {
    match label {
        "M" | "T" => theme.ui_get("git_status_modified"),
        "A" => theme.ui_get("git_status_added"),
        "?" => theme.ui_get("git_status_untracked"),
        "D" => theme.ui_get("git_status_deleted"),
        "R" | "C" => theme.ui_get("git_status_renamed"),
        "U" => theme.ui_get("git_status_conflict"),
        _ => Style::default(),
    }
}

fn entries_display_path(
    work_dir: &Option<PathBuf>,
    entries: &[FileChange],
    idx: &usize,
    original_path: &Option<PathBuf>,
) -> String {
    fn strip_work_dir(work_dir: &Option<PathBuf>, path: &Path) -> String {
        work_dir
            .as_ref()
            .and_then(|wd| path.strip_prefix(wd).ok())
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned()
    }
    let repo_path = &entries[*idx].path;
    let display = strip_work_dir(work_dir, repo_path);
    if let Some(original) = original_path {
        format!("{} -> {display}", strip_work_dir(work_dir, original))
    } else {
        display
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::{ChangeSection, ChangeStatus};
    use std::fs;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn change(path: &str, section: ChangeSection) -> FileChange {
        FileChange {
            path: PathBuf::from(path),
            original_path: None,
            section,
            status: ChangeStatus::Modified,
        }
    }

    fn temp_repo() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("tedii-picker-test-{}-{unique}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        for args in [
            vec!["init", "-q"],
            vec!["config", "user.name", "Tedii Test"],
            vec!["config", "user.email", "tedii@example.com"],
        ] {
            let output = Command::new("git")
                .arg("-C")
                .arg(&path)
                .args(args)
                .output()
                .unwrap();
            assert!(output.status.success());
        }
        path
    }

    #[test]
    fn selected_stage_action_matches_the_entry_section() {
        let mut picker = GitPicker::new(Theme::default_theme());
        picker.entries = vec![
            change("staged.txt", ChangeSection::Staged),
            change("unstaged.txt", ChangeSection::Unstaged),
        ];
        picker.rebuild_status_rows();

        assert_eq!(picker.selected_stage_action(), Some(StageAction::Unstage));
        picker.navigate_down();
        assert_eq!(picker.selected_stage_action(), Some(StageAction::Stage));
    }

    #[test]
    fn commit_prompt_collects_text_and_escape_returns_to_status() {
        let mut picker = GitPicker::new(Theme::default_theme());

        picker.begin_commit();
        picker.add_commit_char('f');
        picker.add_commit_char('i');
        picker.remove_commit_char();
        picker.add_commit_char('x');

        assert_eq!(picker.page, GitPage::Commit);
        assert_eq!(picker.commit_message, "fx");

        picker.back_to_status();

        assert_eq!(picker.page, GitPage::Status);
        assert!(picker.commit_message.is_empty());
    }

    #[test]
    fn log_and_diff_pages_return_to_status() {
        let mut picker = GitPicker::new(Theme::default_theme());

        picker.page = GitPage::Log;
        picker.back_to_status();
        assert_eq!(picker.page, GitPage::Status);

        picker.page = GitPage::Diff;
        picker.back_to_status();
        assert_eq!(picker.page, GitPage::Status);
    }

    #[test]
    fn page_scrolling_is_bounded_by_content_and_viewport() {
        let mut picker = GitPicker::new(Theme::default_theme());
        picker.page_lines = (0..20).map(|index| format!("line {index}")).collect();
        picker.viewport_rows = 5;

        picker.scroll_page(100);
        assert_eq!(picker.page_scroll, 15);

        picker.scroll_page(-100);
        assert_eq!(picker.page_scroll, 0);
    }

    #[test]
    fn opening_a_missing_changed_file_keeps_the_popup_open() {
        let mut picker = GitPicker::new(Theme::default_theme());
        picker.visible = true;
        picker.entries = vec![change("missing.txt", ChangeSection::Unstaged)];
        picker.rebuild_status_rows();

        assert_eq!(picker.enter(), None);
        assert!(picker.visible);
        assert!(picker.feedback.as_ref().unwrap().is_error);
    }

    #[test]
    fn picker_stages_commits_and_opens_the_resulting_log() {
        let path = temp_repo();
        fs::write(path.join("new.txt"), "new\n").unwrap();
        let mut picker = GitPicker::new(Theme::default_theme());

        assert!(picker.open(&path));
        assert_eq!(picker.entries[0].section, ChangeSection::Unstaged);

        picker.toggle_stage();
        assert_eq!(picker.entries[0].section, ChangeSection::Staged);

        picker.begin_commit();
        for c in "add new file".chars() {
            picker.add_commit_char(c);
        }
        picker.submit_commit();
        assert_eq!(picker.page, GitPage::Status);
        assert!(picker.entries.is_empty());

        picker.open_log();
        assert_eq!(picker.page, GitPage::Log);
        assert!(picker
            .page_lines
            .iter()
            .any(|line| line.contains("add new file")));

        fs::remove_dir_all(path).unwrap();
    }
}
