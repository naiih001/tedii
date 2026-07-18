use ratatui::style::{Color, Modifier, Style};
use std::collections::HashMap;

use crate::config::{ColorDef, ThemeConfig};

static DEFAULT_THEME: &[(&str, Color, Modifier)] = &[
    ("keyword", Color::Magenta, Modifier::empty()),
    ("keyword.control", Color::Magenta, Modifier::empty()),
    (
        "keyword.control.conditional",
        Color::Magenta,
        Modifier::ITALIC,
    ),
    ("keyword.control.import", Color::Magenta, Modifier::empty()),
    ("keyword.control.repeat", Color::Magenta, Modifier::empty()),
    ("function", Color::Yellow, Modifier::empty()),
    ("function.method", Color::Yellow, Modifier::empty()),
    ("function.builtin", Color::LightYellow, Modifier::empty()),
    ("string", Color::Green, Modifier::empty()),
    ("string.quoted", Color::Green, Modifier::empty()),
    ("comment", Color::DarkGray, Modifier::ITALIC),
    ("type", Color::Cyan, Modifier::empty()),
    ("type.builtin", Color::LightCyan, Modifier::empty()),
    ("constant", Color::LightRed, Modifier::empty()),
    ("constant.builtin", Color::LightRed, Modifier::empty()),
    ("number", Color::LightCyan, Modifier::empty()),
    ("operator", Color::LightBlue, Modifier::empty()),
    ("punctuation", Color::Gray, Modifier::empty()),
    ("punctuation.delimiter", Color::Gray, Modifier::empty()),
    ("variable", Color::White, Modifier::empty()),
    ("property", Color::LightYellow, Modifier::empty()),
    ("constructor", Color::LightGreen, Modifier::empty()),
    ("attribute", Color::LightMagenta, Modifier::empty()),
    ("label", Color::LightRed, Modifier::empty()),
    ("string.special", Color::LightGreen, Modifier::empty()),
    ("embedded", Color::LightBlue, Modifier::empty()),
    ("parameter", Color::White, Modifier::empty()),
];

fn default_ui() -> Vec<(&'static str, Color, Color)> {
    vec![
        ("mode_normal", Color::White, Color::Rgb(0, 0, 150)),
        ("mode_insert", Color::White, Color::Rgb(0, 100, 0)),
        ("mode_command", Color::Black, Color::Rgb(200, 200, 0)),
        ("mode_fuzzy", Color::White, Color::Rgb(150, 0, 150)),
        ("mode_visual", Color::White, Color::Rgb(100, 0, 100)),
        ("status_bar_filename", Color::Yellow, Color::Reset),
        (
            "status_bar_cursor_pos",
            Color::White,
            Color::Rgb(0, 100, 100),
        ),
        ("gutter_current_line", Color::Yellow, Color::Rgb(30, 30, 30)),
        ("gutter_line", Color::DarkGray, Color::Rgb(30, 30, 30)),
        ("explorer_border", Color::White, Color::Reset),
        ("explorer_filter", Color::Cyan, Color::Reset),
        ("explorer_selected", Color::Black, Color::Gray),
        ("explorer_dir", Color::Cyan, Color::Reset),
        ("fuzzy_border", Color::White, Color::Reset),
        ("fuzzy_query", Color::Cyan, Color::Reset),
        ("fuzzy_selected", Color::Black, Color::Gray),
        ("fuzzy_dir", Color::Cyan, Color::Reset),
        ("fuzzy_match", Color::Yellow, Color::Reset),
        ("search_match", Color::Yellow, Color::Reset),
        ("visual_selection", Color::White, Color::Rgb(40, 40, 140)),
        ("diagnostic_error", Color::Red, Color::Reset),
        ("diagnostic_warning", Color::Yellow, Color::Reset),
        ("diagnostic_information", Color::Cyan, Color::Reset),
        ("diagnostic_hint", Color::DarkGray, Color::Reset),
        ("hover_border", Color::White, Color::Reset),
        ("hover_text", Color::White, Color::Reset),
        ("status_bar_branch", Color::Cyan, Color::Reset),
        ("gutter_diff_added", Color::Green, Color::Rgb(30, 30, 30)),
        (
            "gutter_diff_modified",
            Color::Yellow,
            Color::Rgb(30, 30, 30),
        ),
        ("gutter_diff_deleted", Color::Red, Color::Rgb(30, 30, 30)),
        ("git_border", Color::White, Color::Reset),
        ("git_query", Color::Cyan, Color::Reset),
        ("git_selected", Color::Black, Color::Gray),
        ("git_status_modified", Color::Yellow, Color::Reset),
        ("git_status_added", Color::Green, Color::Reset),
        ("git_status_deleted", Color::Red, Color::Reset),
        ("git_status_untracked", Color::DarkGray, Color::Reset),
        ("git_status_conflict", Color::LightRed, Color::Reset),
        ("git_status_renamed", Color::Cyan, Color::Reset),
        ("completion_border", Color::White, Color::Reset),
        ("completion_selected", Color::Black, Color::Gray),
        ("completion_label", Color::White, Color::Reset),
        ("completion_detail", Color::DarkGray, Color::Reset),
    ]
}

#[derive(Clone)]
pub struct Theme {
    scopes: HashMap<String, Style>,
    ui: HashMap<String, Style>,
}

fn parse_hex(hex: &str) -> Option<Color> {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(Color::Rgb(r, g, b))
}

fn parse_modifiers(modifiers: &[String]) -> Modifier {
    let mut m = Modifier::empty();
    for s in modifiers {
        if s.as_str() == "italic" {
            m |= Modifier::ITALIC;
        }
    }
    m
}

fn color_def_to_style(def: &ColorDef) -> Style {
    let fg = parse_hex(&def.fg).unwrap_or(Color::Reset);
    let bg = def
        .bg
        .as_ref()
        .and_then(|h| parse_hex(h))
        .unwrap_or(Color::Reset);
    let modifiers = parse_modifiers(&def.modifiers);
    Style::default().fg(fg).bg(bg).add_modifier(modifiers)
}

impl Theme {
    pub fn default_theme() -> Self {
        let mut scopes = HashMap::new();
        for (name, color, modifier) in DEFAULT_THEME {
            scopes.insert(
                name.to_string(),
                Style::default().fg(*color).add_modifier(*modifier),
            );
        }

        let mut ui = HashMap::new();
        for (name, fg, bg) in default_ui() {
            ui.insert(name.to_string(), Style::default().fg(fg).bg(bg));
        }

        Self { scopes, ui }
    }

    pub fn apply_config(&mut self, config: ThemeConfig) {
        for (key, def) in config.syntax {
            self.scopes.insert(key, color_def_to_style(&def));
        }
        for (key, def) in config.ui {
            self.ui.insert(key, color_def_to_style(&def));
        }
    }

    pub fn style_for_capture(&self, capture: &str) -> Style {
        let mut best = capture;
        loop {
            if let Some(style) = self.scopes.get(best) {
                return *style;
            }
            if let Some(pos) = best.rfind('.') {
                best = &best[..pos];
            } else {
                return Style::default();
            }
        }
    }

    pub fn ui_get(&self, key: &str) -> Style {
        self.ui.get(key).copied().unwrap_or_default()
    }
}
