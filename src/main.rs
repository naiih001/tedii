mod config;
mod editor;
mod file_explorer;
mod fuzzy_finder;
mod grammar_commands;
mod syntax;
mod theme;
mod tui;

use anyhow::Result;
use config::load_theme_config;
use crossterm::{
    cursor::SetCursorStyle,
    event::{self, Event, KeyCode, KeyModifiers},
    ExecutableCommand,
};
use editor::{Editor, Mode};
use file_explorer::FileExplorer;
use fuzzy_finder::FuzzyFinder;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout},
    text::{Line, Span},
    widgets::Paragraph,
};
use std::io::stdout;
use std::path::PathBuf;
use theme::Theme;
use tui::Tui;

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

    let mut theme = Theme::default_theme();
    if let Some(theme_config) = load_theme_config() {
        theme.apply_config(theme_config);
    }

    let file_content = if let Some(ref path) = file_path {
        match std::fs::read_to_string(path) {
            Ok(content) => content,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
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
        "Welcome to tedii!\nModal editing implemented.\nh,j,k,l to move.\ni for insert.\nEsc for Normal.".to_string()
    };

    let mut tui = Tui::new()?;
    let mut editor = Editor::new(&file_content, file_path.as_deref(), theme.clone());

    let mut file_explorer = FileExplorer::new(theme.clone());
    let mut fuzzy_finder = FuzzyFinder::new(theme);
    if let Some(dir) = start_dir {
        file_explorer.set_dir(dir.clone());
        fuzzy_finder.set_dir(dir);
        file_explorer.toggle();
    }

    while !editor.should_quit {
        let cursor_style = match editor.mode {
            Mode::Normal => SetCursorStyle::SteadyBlock,
            Mode::Insert => SetCursorStyle::SteadyBar,
            _ => SetCursorStyle::SteadyBlock,
        };
        stdout().execute(cursor_style)?;

        tui.terminal.draw(|f| {
            let area = f.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(1),
                    Constraint::Length(1),
                ])
                .split(area);

            let editor_area = chunks[0];

            let gutter_width = editor.get_gutter_width() + 1;
            let editor_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(gutter_width as u16),
                    Constraint::Min(1),
                ])
                .split(editor_area);

            let gutter_area = editor_chunks[0];
            let content_area = editor_chunks[1];

            let viewport_width = content_area.width as usize;
            let viewport_height = content_area.height as usize;

            editor.update_scroll(viewport_width, viewport_height);

            let scroll_y = editor.scroll_y;
            let scroll_x = editor.scroll_x;

            let line_count = editor.buffer.len_lines();
            let mut line_numbers = Vec::new();
            for i in 0..line_count {
                let style = if i == editor.buffer.char_to_line(editor.cursor) {
                    editor.theme.ui_get("gutter_current_line")
                } else {
                    editor.theme.ui_get("gutter_line")
                };
                line_numbers.push(Line::from(vec![Span::styled(format!("{:>width$} ", i + 1, width = gutter_width - 1), style)]));
            }
            
            let gutter_widget = Paragraph::new(line_numbers)
                .alignment(Alignment::Right)
                .scroll((scroll_y as u16, 0));

            let text_widget = Paragraph::new(editor.get_styled_text())
                .style(editor.theme.ui_get("editor_bg"))
                .scroll((scroll_y as u16, scroll_x as u16));

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
                Mode::Command => Span::styled(format!(" COMMAND :{} ", editor.command_buffer), editor.theme.ui_get("mode_command")),
                Mode::Search => Span::styled(format!(" SEARCH /{}/ ", editor.search_query), editor.theme.ui_get("mode_command")),
                Mode::Fuzzy => Span::styled(" FUZZY ", editor.theme.ui_get("mode_fuzzy")),
            };

            let filename = editor.current_file
                .as_ref()
                .and_then(|p| p.to_str())
                .unwrap_or("Untitled");
            let left_text = Line::from(vec![
                mode_span,
                Span::raw(" "),
                Span::styled(filename, editor.theme.ui_get("status_bar_filename")),
            ]);
            let right_text = Line::from(vec![
                Span::styled(format!(" {}:{} ", line_idx + 1, col_idx + 1), editor.theme.ui_get("status_bar_cursor_pos")),
            ]);

            f.render_widget(Paragraph::new(left_text), status_chunks[0]);
            f.render_widget(Paragraph::new(right_text).alignment(Alignment::Right), status_chunks[1]);

            file_explorer.render(f, area);
            fuzzy_finder.render(f, area);

            if !file_explorer.visible && !fuzzy_finder.visible {
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
                    match editor.mode {
                        Mode::Normal => {
                            if editor.pending_g {
                                match key.code {
                                    KeyCode::Char('g') | KeyCode::Char('k') => editor.move_to_start(),
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
                                    _ => {}
                                }
                            } else {
                                match key.code {
                                    KeyCode::Char('/') => {
                                        editor.mode = Mode::Search;
                                        editor.search_query.clear();
                                    }
                                    KeyCode::Char('n') => {
                                        if editor.search_active { editor.next_match(); }
                                    }
                                    KeyCode::Char('N') => {
                                        if editor.search_active { editor.prev_match(); }
                                    }
                                    KeyCode::Char(':') => {
                                        editor.mode = Mode::Command;
                                        editor.command_buffer.clear();
                                    }
                                    KeyCode::Char('i') => editor.mode = Mode::Insert,
                                    KeyCode::Char('I') => {
                                        editor.move_to_line_start();
                                        editor.mode = Mode::Insert;
                                    }
                                    KeyCode::Char('a') => {
                                        editor.move_right();
                                        editor.mode = Mode::Insert;
                                    }
                                    KeyCode::Char('A') => {
                                        editor.move_to_line_end();
                                        editor.mode = Mode::Insert;
                                    }
                                    KeyCode::Char('o') => {
                                        editor.move_to_line_end();
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
                                    KeyCode::Char('g') => editor.pending_g = true,
                                    KeyCode::Char(' ') => editor.pending_space = true,
                                    _ => {}
                                }
                            }
                        }
                        Mode::Insert => match key.code {
                            KeyCode::Esc => editor.mode = Mode::Normal,
                            KeyCode::Char(c) => editor.insert_char(c),
                            KeyCode::Backspace => editor.delete_char(),
                            KeyCode::Enter => editor.insert_char('\n'),
                            _ => {}
                        },
                        Mode::Command => match key.code {
                            KeyCode::Esc => editor.mode = Mode::Normal,
                            KeyCode::Enter => {
                                match editor.command_buffer.as_str() {
                                    "q" => editor.should_quit = true,
                                    "w" => {
                                        let _ = editor.save();
                                        editor.mode = Mode::Normal;
                                    }
                                    "wq" => {
                                        let _ = editor.save();
                                        editor.should_quit = true;
                                    }
                                    _ => editor.mode = Mode::Normal,
                                }
                            }
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
                        Mode::Fuzzy => match key.code {
                            KeyCode::Esc => {
                                fuzzy_finder.visible = false;
                                editor.mode = Mode::Normal;
                            }
                            _ => {}
                        },
                    }
                }
            }
        }
    }

    Tui::restore()?;
    Ok(())
}
