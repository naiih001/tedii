mod completion;
mod config;
mod editor;
mod file_explorer;
mod fuzzy;
mod fuzzy_finder;
mod git;
mod git_picker;
mod grammar_commands;
mod hover;
mod lsp;
mod syntax;
mod theme;
mod tui;

use anyhow::Result;
use config::{load_config, load_keybindings_config, load_theme_config};
use crossterm::{
    cursor::SetCursorStyle,
    event::{self, Event, KeyCode, KeyModifiers},
    ExecutableCommand,
};
use editor::{Editor, Mode};
use file_explorer::FileExplorer;
use fuzzy_finder::FuzzyFinder;
use git::{DiffKind, GitRepo};
use git_picker::GitPicker;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout},
    text::{Line, Span},
    widgets::Paragraph,
};
use std::io::stdout;
use std::path::PathBuf;
use theme::Theme;
use tui::Tui;

#[derive(Debug, PartialEq, Eq)]
struct HoverPopupMetrics {
    area: ratatui::layout::Rect,
    max_scroll: u16,
}

#[derive(Debug, PartialEq, Eq)]
enum PopupKind {
    None,
    Diagnostic,
    Hover,
    Completion,
}

fn popup_kind(
    completion_visible: bool,
    hover_visible: bool,
    diagnostic_present: bool,
) -> PopupKind {
    if completion_visible {
        PopupKind::Completion
    } else if hover_visible {
        PopupKind::Hover
    } else if diagnostic_present {
        PopupKind::Diagnostic
    } else {
        PopupKind::None
    }
}

fn cursor_changed(before: usize, after: usize) -> bool {
    before != after
}

fn hover_popup_metrics(text: &str, area: ratatui::layout::Rect) -> Option<HoverPopupMetrics> {
    if text.trim().is_empty() || area.width < 3 || area.height < 3 {
        return None;
    }
    let longest = text
        .lines()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);
    let width = ((longest + 2) as u16).clamp(3, area.width.min(80));
    let inner_width = width.saturating_sub(2).max(1) as usize;
    let wrapped_lines = text
        .lines()
        .map(|line| line.chars().count().max(1).div_ceil(inner_width))
        .sum::<usize>()
        .max(1);
    let height_cap = (area.height / 2).max(3).min(area.height);
    let height = ((wrapped_lines + 2) as u16).clamp(3, height_cap);
    let inner_height = height.saturating_sub(2) as usize;
    let max_scroll = wrapped_lines.saturating_sub(inner_height) as u16;
    Some(HoverPopupMetrics {
        area: ratatui::layout::Rect {
            x: area.x + area.width - width,
            y: area.y + area.height - height,
            width,
            height,
        },
        max_scroll,
    })
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let mut file_path: Option<PathBuf> = None;
    let mut start_dir: Option<PathBuf> = None;

    if args.len() > 1 {
        match args[1].as_str() {
            "--init" => {
                grammar_commands::create_default_config()?;
                return Ok(());
            }
            "--grammar" => {
                if args.len() < 3 {
                    eprintln!("Usage: tedii --grammar {{fetch,build,update}}");
                    std::process::exit(1);
                }
                let runtime = grammar_commands::find_or_create_runtime()?;
                let config = config::load_config()?;
                match args[2].as_str() {
                    "fetch" => grammar_commands::fetch_grammars(&config, &runtime)?,
                    "build" => grammar_commands::build_grammars(&config, &runtime)?,
                    "update" => grammar_commands::update_grammars(&config, &runtime)?,
                    other => {
                        eprintln!("Unknown grammar command: {}", other);
                        eprintln!("Usage: tedii --grammar {{fetch,build,update}}");
                        std::process::exit(1);
                    }
                }
                return Ok(());
            }
            _ => {
                let path = std::path::Path::new(&args[1]);
                if path.is_dir() {
                    start_dir = Some(path.to_path_buf());
                } else {
                    file_path = Some(path.to_path_buf());
                }
            }
        }
    }

    crate::lsp::log_line(format!(
        "[main] startup args={:?} file_path={:?} start_dir={:?}",
        args, file_path, start_dir
    ));

    let language_config = match load_config() {
        Ok(config) => {
            crate::lsp::log_line(format!(
                "[main] loaded languages.toml with {} languages and {} grammars",
                config.languages.len(),
                config.grammars.len()
            ));
            Some(config)
        }
        Err(err) => {
            crate::lsp::log_line(format!("[main] failed to load languages.toml: {}", err));
            None
        }
    };
    let mut theme = Theme::default_theme();
    if let Some(theme_config) = load_theme_config() {
        theme.apply_config(theme_config);
    }

    let keybindings_config = load_keybindings_config();
    let leader_keys_enabled = keybindings_config
        .as_ref()
        .map(|k| k.leader_keys)
        .unwrap_or(true);

    let file_content = if let Some(ref path) = file_path {
        crate::lsp::log_line(format!("[main] opening file {}", path.display()));
        match std::fs::read_to_string(path) {
            Ok(content) => content,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                crate::lsp::log_line(format!(
                    "[main] file not found, creating {}",
                    path.display()
                ));
                if let Err(e) = std::fs::File::create(path) {
                    eprintln!("Error creating file {}: {}", path.display(), e);
                    std::process::exit(1);
                }
                String::new()
            }
            Err(e) => {
                eprintln!("Error reading file {}: {}", path.display(), e);
                std::process::exit(1);
            }
        }
    } else {
        crate::lsp::log_line("[main] no file argument supplied; using welcome buffer");
        "Welcome to tedii!\nModal editing implemented.\nh,j,k,l to move.\ni for insert.\nEsc for Normal.".to_string()
    };

    let mut tui = Tui::new()?;
    let mut editor = Editor::new(&file_content, file_path.as_deref(), theme.clone(), language_config);

    let mut file_explorer = FileExplorer::new(theme.clone());
    let mut fuzzy_finder = FuzzyFinder::new(theme.clone());
    let mut git_picker = GitPicker::new(theme);

    fn refresh_git(git_picker: &mut GitPicker) {
        if let Ok(cwd) = std::env::current_dir() {
            if let Some(repo) = GitRepo::discover(&cwd) {
                let changes = repo.status();
                git_picker.set_entries(changes);
            }
        }
    }

    if let Some(dir) = start_dir {
        file_explorer.set_dir(dir.clone());
        fuzzy_finder.set_dir(dir);
        file_explorer.toggle();
    }

    while !editor.should_quit {
        editor.refresh_lsp();

        let cursor_style = match editor.mode {
            Mode::Normal | Mode::Visual => SetCursorStyle::SteadyBlock,
            Mode::Insert => SetCursorStyle::SteadyBar,
            _ => SetCursorStyle::SteadyBlock,
        };
        stdout().execute(cursor_style)?;

        tui.terminal.draw(|f| {
            let area = f.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(1)])
                .split(area);

            let editor_area = chunks[0];

            let diff_width = 1u16;
            let gutter_width = editor.get_gutter_width() + 1;
            let editor_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(diff_width),
                    Constraint::Length(gutter_width as u16),
                    Constraint::Min(1),
                ])
                .split(editor_area);

            let diff_area = editor_chunks[0];
            let gutter_area = editor_chunks[1];
            let content_area = editor_chunks[2];

            let viewport_width = content_area.width as usize;
            let viewport_height = content_area.height as usize;

            editor.update_scroll(viewport_width, viewport_height);
            editor.refresh_diff();

            let scroll_y = editor.scroll_y;
            let scroll_x = editor.scroll_x;

            let line_count = editor.buffer.len_lines();
            let visible_end = (scroll_y + viewport_height).min(line_count);

            let mut diff_markers = Vec::new();
            for i in scroll_y..visible_end {
                let style = if i == editor.buffer.char_to_line(editor.cursor) {
                    editor.theme.ui_get("gutter_current_line")
                } else {
                    editor.theme.ui_get("gutter_line")
                };
                let (marker, diff_style) = editor
                    .diff_hunks
                    .iter()
                    .find(|h| h.line == i as u32)
                    .map(|h| match h.kind {
                        DiffKind::Added => ("+", editor.theme.ui_get("gutter_diff_added")),
                        DiffKind::Removed => ("-", editor.theme.ui_get("gutter_diff_deleted")),
                        DiffKind::Modified => ("~", editor.theme.ui_get("gutter_diff_modified")),
                    })
                    .unwrap_or((" ", style));
                diff_markers.push(Line::from(vec![Span::styled(marker, diff_style)]));
            }

            let mut line_numbers = Vec::new();
            for i in scroll_y..visible_end {
                let style = if i == editor.buffer.char_to_line(editor.cursor) {
                    editor.theme.ui_get("gutter_current_line")
                } else {
                    editor.theme.ui_get("gutter_line")
                };
                line_numbers.push(Line::from(vec![Span::styled(
                    format!("{:>width$} ", i + 1, width = gutter_width as usize - 1),
                    style,
                )]));
            }

            let diff_widget = Paragraph::new(diff_markers);
            let gutter_widget = Paragraph::new(line_numbers).alignment(Alignment::Right);

            let (styled_text, context_start) = editor.get_styled_text(scroll_y, viewport_height);
            let text_widget = Paragraph::new(styled_text)
                .style(editor.theme.ui_get("editor_bg"))
                .scroll(((scroll_y - context_start) as u16, scroll_x as u16));

            f.render_widget(diff_widget, diff_area);
            f.render_widget(gutter_widget, gutter_area);
            f.render_widget(text_widget, content_area);

            let line_idx = editor.buffer.char_to_line(editor.cursor);
            let col_idx = editor.cursor - editor.buffer.line_to_char(line_idx);

            let status_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Min(1), Constraint::Length(20)])
                .split(chunks[1]);

            let mode_span = match editor.mode {
                Mode::Normal => Span::styled(" NORMAL ", editor.theme.ui_get("mode_normal")),
                Mode::Insert => Span::styled(" INSERT ", editor.theme.ui_get("mode_insert")),
                Mode::Command => Span::styled(
                    format!(" COMMAND :{} ", editor.command_buffer),
                    editor.theme.ui_get("mode_command"),
                ),
                Mode::Search => Span::styled(
                    format!(" SEARCH /{}/ ", editor.search_query),
                    editor.theme.ui_get("mode_command"),
                ),
                Mode::Fuzzy => Span::styled(" FUZZY ", editor.theme.ui_get("mode_fuzzy")),
                Mode::Visual => Span::styled(" VISUAL ", editor.theme.ui_get("mode_visual")),
            };

            let filename = editor
                .current_file
                .as_ref()
                .and_then(|p| p.to_str())
                .unwrap_or("Untitled");
            let mut left_spans = vec![mode_span, Span::raw(" ")];
            if let Some(ref branch) = editor.git_branch {
                left_spans.push(Span::styled(
                    format!(" {} ", branch),
                    editor.theme.ui_get("status_bar_branch"),
                ));
                left_spans.push(Span::raw(" "));
            }
            let mut filename_display = filename.to_string();
            if editor.is_dirty() {
                filename_display.push_str(" *");
            }
            left_spans.push(Span::styled(
                filename_display,
                editor.theme.ui_get("status_bar_filename"),
            ));
            let left_text = Line::from(left_spans);
            let right_text = Line::from(vec![
                Span::styled(
                    format!(" {}:{} ", line_idx + 1, col_idx + 1),
                    editor.theme.ui_get("status_bar_cursor_pos"),
                ),
                Span::raw(" "),
                Span::styled(
                    format!(
                        " E:{} W:{} ",
                        editor.lsp_diagnostics.error_count, editor.lsp_diagnostics.warning_count
                    ),
                    editor.theme.ui_get("status_bar_cursor_pos"),
                ),
            ]);

            f.render_widget(Paragraph::new(left_text), status_chunks[0]);
            f.render_widget(
                Paragraph::new(right_text).alignment(Alignment::Right),
                status_chunks[1],
            );

            match popup_kind(
                editor.completion.visible,
                editor.hover.visible,
                editor.active_diagnostic().is_some(),
            ) {
                PopupKind::Completion => {
                    if !editor.completion.filtered_indices.is_empty() {
                        let visible_count = editor.completion.visible_count();
                        let scroll_offset = editor.completion.scroll_offset;
                        let line_idx = editor.buffer.char_to_line(editor.cursor);
                        let col_idx = editor.cursor - editor.buffer.line_to_char(line_idx);
                        let popup_x =
                            content_area.x + (col_idx as u16).saturating_sub(scroll_x as u16);
                        let cursor_screen_y =
                            content_area.y + (line_idx as u16).saturating_sub(scroll_y as u16);
                        let max_label_len = editor
                            .completion
                            .filtered_indices
                            .iter()
                            .skip(scroll_offset)
                            .take(visible_count)
                            .map(|&i| editor.completion.items[i].label.len())
                            .max()
                            .unwrap_or(0)
                            .max(4);
                        let max_detail_len = editor
                            .completion
                            .filtered_indices
                            .iter()
                            .skip(scroll_offset)
                            .take(visible_count)
                            .filter_map(|&i| editor.completion.items[i].detail.as_ref())
                            .map(|d| d.len())
                            .max()
                            .unwrap_or(0);
                        let popup_width = ((max_label_len + max_detail_len + 4) as u16)
                            .clamp(10, content_area.width.saturating_sub(popup_x - content_area.x).max(10));
                        let ideal_height = (visible_count as u16 + 2)
                            .min(completion::MAX_VISIBLE_ITEMS as u16 + 2);

                        let space_below = content_area.y + content_area.height - cursor_screen_y - 1;
                        let space_above = cursor_screen_y - content_area.y;

                        let (popup_y, popup_height) = if space_below >= ideal_height {
                            (cursor_screen_y + 1, ideal_height)
                        } else if space_above >= ideal_height {
                            (cursor_screen_y - ideal_height, ideal_height)
                        } else if space_below >= space_above {
                            (cursor_screen_y + 1, space_below.max(3))
                        } else {
                            (cursor_screen_y - space_above, space_above.max(3))
                        };
                        let popup_area = ratatui::layout::Rect {
                            x: popup_x.min(content_area.x + content_area.width.saturating_sub(1)),
                            y: popup_y.min(content_area.y + content_area.height.saturating_sub(1)),
                            width: popup_width,
                            height: popup_height,
                        };
                        let inner_width = popup_width.saturating_sub(2) as usize;
                        let mut lines: Vec<Line> = Vec::new();
                        for (view_idx, &item_idx) in editor
                            .completion
                            .filtered_indices
                            .iter()
                            .skip(scroll_offset)
                            .take(visible_count)
                            .enumerate()
                        {
                            let item = &editor.completion.items[item_idx];
                            let is_selected = scroll_offset + view_idx == editor.completion.selected;
                            let label_display: String = item.label.chars().take(inner_width).collect();
                            let detail_display: String = item
                                .detail
                                .as_deref()
                                .unwrap_or("")
                                .chars()
                                .take(inner_width.saturating_sub(label_display.len() + 1))
                                .collect();
                            let mut spans = Vec::new();
                            if is_selected {
                                spans.push(Span::styled(
                                    "> ",
                                    editor.theme.ui_get("completion_selected"),
                                ));
                            } else {
                                spans.push(Span::styled("  ", editor.theme.ui_get("completion_label")));
                            }
                            spans.push(Span::styled(
                                label_display,
                                editor.theme.ui_get("completion_label"),
                            ));
                            if !detail_display.is_empty() {
                                spans.push(Span::styled(
                                    format!(" {}", detail_display),
                                    editor.theme.ui_get("completion_detail"),
                                ));
                            }
                            lines.push(Line::from(spans));
                        }
                        let block = ratatui::widgets::Block::bordered()
                            .border_style(editor.theme.ui_get("completion_border"));
                        let popup =
                            Paragraph::new(lines).block(block);
                        f.render_widget(popup, popup_area);
                    }
                }
                PopupKind::Hover => {
                    if let Some(metrics) = hover_popup_metrics(&editor.hover.text, content_area) {
                        editor.hover.max_scroll = metrics.max_scroll;
                        editor.hover.scroll = editor.hover.scroll.min(metrics.max_scroll);
                        let block = ratatui::widgets::Block::bordered()
                            .border_style(editor.theme.ui_get("hover_border"));
                        let popup = Paragraph::new(editor.hover.text.clone())
                            .style(editor.theme.ui_get("hover_text"))
                            .block(block)
                            .wrap(ratatui::widgets::Wrap { trim: false })
                            .scroll((editor.hover.scroll, 0));
                        f.render_widget(popup, metrics.area);
                    }
                }
                PopupKind::Diagnostic => {
                    if let Some((diagnostic, position, total)) = editor.active_diagnostic_with_position() {
                        let popup_text = vec![Line::from(vec![Span::raw(format!(
                            "[{position}/{total}] {:?}: {}",
                            diagnostic.severity, diagnostic.message
                        ))])];
                        let popup_width = popup_text
                            .iter()
                            .map(|line| line.width() as u16)
                            .max()
                            .unwrap_or(0)
                            .min(content_area.width.saturating_sub(2));
                        let popup_height = popup_text.len() as u16 + 2;
                        let popup_area = ratatui::layout::Rect {
                            x: content_area
                                .x
                                .saturating_add(content_area.width.saturating_sub(popup_width + 2)),
                            y: content_area.y.saturating_add(
                                content_area.height.saturating_sub(popup_height + 1),
                            ),
                            width: popup_width + 2,
                            height: popup_height,
                        };
                        f.render_widget(
                            Paragraph::new(popup_text).block(ratatui::widgets::Block::bordered()),
                            popup_area,
                        );
                    }
                }
                PopupKind::None => {}
            }

            file_explorer.render(f, area);
            fuzzy_finder.render(f, area);
            git_picker.render(f, area);

            if !file_explorer.visible && !fuzzy_finder.visible && !git_picker.visible {
                f.set_cursor_position((
                    content_area.x + (col_idx - editor.scroll_x) as u16,
                    content_area.y + (line_idx - editor.scroll_y) as u16,
                ));
            }
        })?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if file_explorer.visible {
                    match key.code {
                        KeyCode::Esc => {
                            file_explorer.reset_to_root();
                            file_explorer.visible = false;
                        }
                        KeyCode::Enter => {
                            if let Some(path) = file_explorer.enter() {
                                if let Err(e) = editor.open_file(&path) {
                                    eprintln!("Error opening file: {}", e);
                                }
                            }
                        }
                        KeyCode::Backspace => file_explorer.remove_filter_char(),
                        KeyCode::Up | KeyCode::BackTab => file_explorer.navigate_up(),
                        KeyCode::Down | KeyCode::Tab => file_explorer.navigate_down(),
                        KeyCode::Char(c) => file_explorer.add_filter_char(c),
                        _ => {}
                    }
                } else if git_picker.visible {
                    match key.code {
                        KeyCode::Esc => {
                            git_picker.visible = false;
                            editor.mode = Mode::Normal;
                        }
                        KeyCode::Enter => {
                            if let Some(path) = git_picker.enter() {
                                if let Err(e) = editor.open_file(&path) {
                                    eprintln!("Error opening file: {}", e);
                                }
                                git_picker.visible = false;
                                editor.mode = Mode::Normal;
                            }
                        }
                        KeyCode::Up | KeyCode::BackTab => git_picker.navigate_up(),
                        KeyCode::Down | KeyCode::Tab => git_picker.navigate_down(),
                        _ => {}
                    }
                } else if fuzzy_finder.visible {
                    match key.code {
                        KeyCode::Esc => {
                            fuzzy_finder.reset_to_root();
                            fuzzy_finder.visible = false;
                            editor.mode = Mode::Normal;
                        }
                        KeyCode::Enter => {
                            if let Some(path) = fuzzy_finder.enter() {
                                if let Err(e) = editor.open_file(&path) {
                                    eprintln!("Error opening file: {}", e);
                                }
                                fuzzy_finder.visible = false;
                                editor.mode = Mode::Normal;
                            }
                        }
                        KeyCode::Backspace => fuzzy_finder.remove_query_char(),
                        KeyCode::Up | KeyCode::BackTab => fuzzy_finder.navigate_up(),
                        KeyCode::Down | KeyCode::Tab => fuzzy_finder.navigate_down(),
                        KeyCode::Char(c) => fuzzy_finder.add_query_char(c),
                        _ => {}
                    }
                } else if key.code == KeyCode::Char('p') && key.modifiers == KeyModifiers::CONTROL {
                    fuzzy_finder.visible = false;
                    file_explorer.toggle();
                } else if key.code == KeyCode::Char('f') && key.modifiers == KeyModifiers::CONTROL {
                    file_explorer.visible = false;
                    fuzzy_finder.toggle();
                    editor.mode = Mode::Fuzzy;
                } else {
                    if editor.hover.visible
                        && key.code == KeyCode::Char('j')
                        && key.modifiers == KeyModifiers::ALT
                    {
                        editor.scroll_hover(1);
                        continue;
                    }
                    if editor.hover.visible
                        && key.code == KeyCode::Char('k')
                        && key.modifiers == KeyModifiers::ALT
                    {
                        editor.scroll_hover(-1);
                        continue;
                    }
                    if editor.hover.visible && key.code == KeyCode::Esc {
                        editor.dismiss_hover();
                        continue;
                    }

                    let cursor_before = editor.cursor;
                    match editor.mode {
                        Mode::Normal => {
                            if editor.pending_g {
                                match key.code {
                                    KeyCode::Char('g') | KeyCode::Char('k') => {
                                        editor.move_to_start()
                                    }
                                    KeyCode::Char('e') | KeyCode::Char('j') => editor.move_to_end(),
                                    KeyCode::Char('h') => editor.move_to_line_start(),
                                    KeyCode::Char('l') => editor.move_to_line_end(),
                                    _ => {}
                                }
                                editor.pending_g = false;
                            } else if editor.pending_space {
                                editor.pending_space = false;
                                match key.code {
                                    KeyCode::Char('e') => {
                                        fuzzy_finder.visible = false;
                                        file_explorer.toggle();
                                    }
                                    KeyCode::Char('f') => {
                                        file_explorer.visible = false;
                                        fuzzy_finder.toggle();
                                        editor.mode = Mode::Fuzzy;
                                    }
                                    KeyCode::Char('g') => {
                                        file_explorer.visible = false;
                                        fuzzy_finder.visible = false;
                                        refresh_git(&mut git_picker);
                                        if !git_picker.is_empty() {
                                            git_picker.visible = true;
                                            editor.mode = Mode::Fuzzy;
                                        }
                                    }
                                    KeyCode::Char('w') if leader_keys_enabled => {
                                        let _ = editor.save();
                                    }
                                    KeyCode::Char('q') if leader_keys_enabled => {
                                        editor.should_quit = true;
                                    }
                                    KeyCode::Char('k') => editor.request_hover(),
                                    _ => {}
                                }
                            } else if editor.pending_z {
                                editor.pending_z = false;
                                match key.code {
                                    KeyCode::Char('z') => {
                                        let term_size = tui.terminal.size().unwrap_or_default();
                                        let viewport_height =
                                            term_size.height.saturating_sub(1) as usize;
                                        editor.center_cursor(viewport_height);
                                    }
                                    _ => {}
                                }
                            } else {
                                match key.code {
                                    KeyCode::Char('j')
                                        if key.modifiers == KeyModifiers::ALT =>
                                    {
                                        editor.cycle_active_diagnostic(1);
                                    }
                                    KeyCode::Char('k')
                                        if key.modifiers == KeyModifiers::ALT =>
                                    {
                                        editor.cycle_active_diagnostic(-1);
                                    }
                                    KeyCode::Char('/') => {
                                        editor.mode = Mode::Search;
                                        editor.search_query.clear();
                                    }
                                    KeyCode::Char('n') => {
                                        if editor.search_active {
                                            editor.next_match();
                                        }
                                    }
                                    KeyCode::Char('N') => {
                                        if editor.search_active {
                                            editor.prev_match();
                                        }
                                    }
                                    KeyCode::Char(':') => {
                                        editor.mode = Mode::Command;
                                        editor.command_buffer.clear();
                                    }
                                    KeyCode::Char('i') => {
                                        editor.begin_undo_group();
                                        editor.mode = Mode::Insert;
                                    }
                                    KeyCode::Char('I') => {
                                        editor.move_to_line_start();
                                        editor.begin_undo_group();
                                        editor.mode = Mode::Insert;
                                    }
                                    KeyCode::Char('a') => {
                                        editor.move_right();
                                        editor.begin_undo_group();
                                        editor.mode = Mode::Insert;
                                    }
                                    KeyCode::Char('A') => {
                                        editor.move_to_line_end();
                                        editor.begin_undo_group();
                                        editor.mode = Mode::Insert;
                                    }
                                    KeyCode::Char('o') => {
                                        editor.move_to_line_end();
                                        editor.begin_undo_group();
                                        editor.insert_char('\n');
                                        editor.mode = Mode::Insert;
                                    }
                                    KeyCode::Char('O') => {
                                        editor.move_to_line_start();
                                        editor.insert_char('\n');
                                        editor.move_up();
                                        editor.mode = Mode::Insert;
                                    }
                                    KeyCode::Char('h') => editor.move_left(),
                                    KeyCode::Char('j') => editor.move_down(),
                                    KeyCode::Char('k') => editor.move_up(),
                                    KeyCode::Char('l') => editor.move_right(),
                                    KeyCode::Char('w') => editor.move_word_forward(),
                                    KeyCode::Char('b') => editor.move_word_backward(),
                                    KeyCode::Char('g') => editor.pending_g = true,
                                    KeyCode::Char(' ') => editor.pending_space = true,
                                    KeyCode::Char('z') => editor.pending_z = true,
                                    KeyCode::Char('c') => {
                                        editor.begin_change();
                                        editor.mode = Mode::Insert;
                                    }
                                    KeyCode::Char('v') => {
                                        editor.enter_visual_mode();
                                        editor.mode = Mode::Visual;
                                    }
                                    KeyCode::Char('x') => {
                                        editor.select_line();
                                        editor.mode = Mode::Visual;
                                    }
                                    KeyCode::Char('p') => editor.paste_clipboard(),
                                    KeyCode::Char('P') => editor.paste_system_clipboard(),
                                    KeyCode::Char('u') => editor.undo(),
                                    KeyCode::Char('U') if key.modifiers == KeyModifiers::CONTROL => editor.redo(),
                                    _ => {}
                                }
                            }
                        }
                        Mode::Insert => {
                            if editor.completion.visible {
                                match key.code {
                                    KeyCode::Esc => {
                                        editor.dismiss_completion();
                                        editor.mode = Mode::Normal;
                                    }
                                    KeyCode::Enter => {
                                        editor.accept_completion();
                                    }
                                    KeyCode::Tab => {
                                        editor.completion.select_next();
                                    }
                                    KeyCode::BackTab => {
                                        editor.completion.select_prev();
                                    }
                                    KeyCode::Up => {
                                        editor.completion.select_prev();
                                    }
                                    KeyCode::Down => {
                                        editor.completion.select_next();
                                    }
                                    KeyCode::Char(c) => {
                                        editor.insert_char(c);
                                        let prefix =
                                            editor.buffer.slice(editor.completion.trigger_offset..editor.cursor).to_string();
                                        editor.filter_completion(&prefix);
                                        if !editor.completion.visible {
                                            editor.request_completion();
                                        }
                                    }
                                    KeyCode::Backspace => {
                                        editor.delete_char();
                                        if editor.cursor < editor.completion.trigger_offset {
                                            editor.dismiss_completion();
                                        } else {
                                            let prefix = editor
                                                .buffer
                                                .slice(editor.completion.trigger_offset..editor.cursor)
                                                .to_string();
                                            editor.filter_completion(&prefix);
                                            if !editor.completion.visible {
                                                editor.dismiss_completion();
                                            }
                                        }
                                    }
                                    _ => {
                                        editor.dismiss_completion();
                                    }
                                }
                            } else {
                                match key.code {
                                    KeyCode::Esc => editor.mode = Mode::Normal,
                                    KeyCode::Char(c)
                                        if key.modifiers == KeyModifiers::CONTROL =>
                                    {
                                        if c == ' ' {
                                            editor.request_completion();
                                        }
                                    }
                                    KeyCode::Char(c) => {
                                        editor.insert_char(c);
                                        if !editor.completion.visible
                                            && (c.is_alphanumeric() || c == '_')
                                        {
                                            editor.request_completion();
                                        }
                                    }
                                    KeyCode::Backspace => editor.delete_char(),
                                    KeyCode::Enter => editor.insert_char('\n'),
                                    KeyCode::Tab => {
                                        if !editor.split_bracket_pair_at_cursor() {
                                            editor.insert_tab();
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        Mode::Command => match key.code {
                            KeyCode::Esc => editor.mode = Mode::Normal,
                            KeyCode::Enter => match editor.command_buffer.as_str() {
                                "q" => editor.should_quit = true,
                                "git" => {
                                    refresh_git(&mut git_picker);
                                    if !git_picker.is_empty() {
                                        git_picker.visible = true;
                                        editor.mode = Mode::Fuzzy;
                                    }
                                    editor.mode = Mode::Normal;
                                }
                                "w" => {
                                    let _ = editor.save();
                                    editor.mode = Mode::Normal;
                                }
                                "wq" => {
                                    let _ = editor.save();
                                    editor.should_quit = true;
                                }
                                _ => editor.mode = Mode::Normal,
                            },
                            KeyCode::Char(c) => editor.command_buffer.push(c),
                            KeyCode::Backspace => {
                                editor.command_buffer.pop();
                            }
                            _ => {}
                        },
                        Mode::Search => match key.code {
                            KeyCode::Esc => {
                                editor.mode = Mode::Normal;
                                editor.search_query.clear();
                            }
                            KeyCode::Enter => {
                                editor.perform_search();
                                editor.mode = Mode::Normal;
                            }
                            KeyCode::Char(c) => editor.search_query.push(c),
                            KeyCode::Backspace => {
                                editor.search_query.pop();
                            }
                            _ => {}
                        },
                        Mode::Fuzzy => {
                            if key.code == KeyCode::Esc {
                                fuzzy_finder.visible = false;
                                editor.mode = Mode::Normal;
                            }
                        }
                        Mode::Visual => {
                            if editor.pending_g {
                                match key.code {
                                    KeyCode::Char('g') | KeyCode::Char('k') => {
                                        editor.move_to_start()
                                    }
                                    KeyCode::Char('e') | KeyCode::Char('j') => editor.move_to_end(),
                                    KeyCode::Char('h') => editor.move_to_line_start(),
                                    KeyCode::Char('l') => editor.move_to_line_end(),
                                    _ => {}
                                }
                                editor.pending_g = false;
                            } else {
                                match key.code {
                                    KeyCode::Esc => {
                                        editor.exit_visual_mode();
                                        editor.mode = Mode::Normal;
                                    }
                                    KeyCode::Char('v') => {
                                        editor.exit_visual_mode();
                                        editor.mode = Mode::Normal;
                                    }
                                    KeyCode::Char('h') => editor.move_left(),
                                    KeyCode::Char('j') => editor.move_down(),
                                    KeyCode::Char('k') => editor.move_up(),
                                    KeyCode::Char('l') => editor.move_right(),
                                    KeyCode::Char('w') => editor.move_word_forward(),
                                    KeyCode::Char('b') => editor.move_word_backward(),
                                    KeyCode::Char('x') => editor.extend_selection_down(),
                                    KeyCode::Char('X') => editor.extend_selection_up(),
                                    KeyCode::Char('g') => editor.pending_g = true,
                                    KeyCode::Char('y') => {
                                        editor.yank_selection();
                                        editor.mode = Mode::Normal;
                                    }
                                    KeyCode::Char('Y') => {
                                        editor.yank_selection_system();
                                        editor.mode = Mode::Normal;
                                    }
                                    KeyCode::Char('p') => {
                                        editor.paste_clipboard_after_selection();
                                        editor.mode = Mode::Normal;
                                    }
                                    KeyCode::Char('P') => {
                                        editor.paste_system_clipboard_after_selection();
                                        editor.mode = Mode::Normal;
                                    }
                                    KeyCode::Char('d') => {
                                        editor.delete_selection();
                                        editor.mode = Mode::Normal;
                                    }
                                    KeyCode::Char('c') => {
                                        editor.begin_change();
                                        editor.mode = Mode::Insert;
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    if cursor_changed(cursor_before, editor.cursor) {
                        editor.dismiss_hover();
                    }
                }
            }
        }

        editor.refresh_lsp();
    }

    Tui::restore()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hover_popup_is_capped_and_reports_scroll_range() {
        let area = ratatui::layout::Rect::new(10, 5, 100, 20);
        let text = (0..20)
            .map(|index| format!("line {index}"))
            .collect::<Vec<_>>()
            .join("\n");

        let metrics = hover_popup_metrics(&text, area).unwrap();

        assert!(metrics.area.width <= 80);
        assert!(metrics.area.height <= 10);
        assert_eq!(metrics.area.x + metrics.area.width, area.x + area.width);
        assert_eq!(metrics.area.y + metrics.area.height, area.y + area.height);
        assert!(metrics.max_scroll > 0);
    }

    #[test]
    fn hover_popup_returns_none_for_unusable_area_or_empty_text() {
        assert_eq!(
            hover_popup_metrics("", ratatui::layout::Rect::new(0, 0, 80, 20)),
            None
        );
        assert_eq!(
            hover_popup_metrics("docs", ratatui::layout::Rect::new(0, 0, 2, 2)),
            None
        );
    }

    #[test]
    fn hover_has_popup_precedence_and_cursor_changes_dismiss_it() {
        assert_eq!(popup_kind(false, true, true), PopupKind::Hover);
        assert_eq!(popup_kind(false, false, true), PopupKind::Diagnostic);
        assert_eq!(popup_kind(false, false, false), PopupKind::None);
        assert_eq!(popup_kind(true, true, true), PopupKind::Completion);
        assert!(cursor_changed(4, 5));
        assert!(!cursor_changed(4, 4));
    }
}
