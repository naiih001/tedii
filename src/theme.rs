use ratatui::style::{Color, Modifier, Style};
use std::collections::HashMap;

static DEFAULT_THEME: &[(&str, Color, Modifier)] = &[
    ("keyword", Color::Magenta, Modifier::empty()),
    ("keyword.control", Color::Magenta, Modifier::empty()),
    ("keyword.control.conditional", Color::Magenta, Modifier::ITALIC),
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

pub struct Theme {
    scopes: HashMap<String, Style>,
}

impl Theme {
    pub fn default_theme() -> Self {
        let mut scopes = HashMap::new();
        for (name, color, modifier) in DEFAULT_THEME {
            scopes.insert(name.to_string(), Style::default().fg(*color).add_modifier(*modifier));
        }
        Self { scopes }
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
}
