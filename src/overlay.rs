use std::ops::Range;

use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Clear, Paragraph},
    Frame,
};

use crate::theme::Theme;

#[derive(Clone)]
pub struct PopupConfig {
    pub title: String,
    pub width_pct: f32,
    pub height_pct: f32,
    pub min_width: u16,
    pub min_height: u16,
    pub wrap: bool,
    pub border_key: String,
    pub filter_key: String,
    pub filter_label: String,
}

pub struct PopupArea {
    pub outer: Rect,
    pub inner: Rect,
}

pub fn centered_popup(area: Rect, config: &PopupConfig) -> Option<PopupArea> {
    if area.width < 4 || area.height < 4 {
        return None;
    }
    let popup_width = ((area.width as f32 * config.width_pct) as u16)
        .max(config.min_width)
        .min(area.width.saturating_sub(2).max(1));
    let popup_height = ((area.height as f32 * config.height_pct) as u16)
        .max(config.min_height)
        .min(area.height.saturating_sub(2).max(1));
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
    let outer = horiz[1];
    let inner = {
        let block = Block::bordered()
            .title(format!(" {} ", config.title))
            .title_alignment(Alignment::Center);
        block.inner(outer)
    };
    Some(PopupArea { outer, inner })
}

pub fn render_popup_shell(f: &mut Frame, popup: &PopupArea, config: &PopupConfig, theme: &Theme) {
    f.render_widget(Clear, popup.outer);
    let block = Block::bordered()
        .title(format!(" {} ", config.title))
        .title_alignment(Alignment::Center)
        .style(theme.ui_get("editor_bg"))
        .border_style(theme.ui_get(&config.border_key));
    f.render_widget(block, popup.outer);
}

pub fn render_filter_bar(
    f: &mut Frame,
    area: Rect,
    filter: &str,
    label: &str,
    theme: &Theme,
    style_key: &str,
) {
    let filter_text = if filter.is_empty() {
        format!(" {}: ", label)
    } else {
        format!(" {}: {}", label, filter)
    };
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            filter_text,
            theme.ui_get(style_key),
        ))),
        area,
    );
}

pub struct ListPopup<T> {
    pub visible: bool,
    pub entries: Vec<T>,
    pub selection: usize,
    pub scroll: usize,
    pub filter: String,
    pub config: PopupConfig,
}

impl<T> ListPopup<T> {
    pub fn new(config: PopupConfig) -> Self {
        Self {
            visible: false,
            entries: Vec::new(),
            selection: 0,
            scroll: 0,
            filter: String::new(),
            config,
        }
    }

    pub fn navigate_up(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        if self.selection == 0 {
            if self.config.wrap {
                self.selection = self.entries.len() - 1;
            }
        } else {
            self.selection -= 1;
        }
    }

    pub fn navigate_down(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        if self.selection + 1 < self.entries.len() {
            self.selection += 1;
        } else if self.config.wrap {
            self.selection = 0;
        }
    }

    pub fn navigate_up_skip(&mut self, skip: impl Fn(&T) -> bool) {
        if self.entries.is_empty() {
            return;
        }
        let start = self.selection;
        if self.selection == 0 {
            if self.config.wrap {
                self.selection = self.entries.len() - 1;
            } else {
                return;
            }
        } else {
            self.selection -= 1;
        }
        while skip(&self.entries[self.selection]) {
            if self.selection == start {
                return;
            }
            if self.selection == 0 {
                if self.config.wrap {
                    self.selection = self.entries.len() - 1;
                } else {
                    self.selection = start;
                    return;
                }
            } else {
                self.selection -= 1;
            }
        }
    }

    pub fn navigate_down_skip(&mut self, skip: impl Fn(&T) -> bool) {
        if self.entries.is_empty() {
            return;
        }
        let start = self.selection;
        if self.selection + 1 >= self.entries.len() {
            if self.config.wrap {
                self.selection = 0;
            } else {
                return;
            }
        } else {
            self.selection += 1;
        }
        while skip(&self.entries[self.selection]) {
            if self.selection == start {
                return;
            }
            if self.selection + 1 >= self.entries.len() {
                if self.config.wrap {
                    self.selection = 0;
                } else {
                    self.selection = start;
                    return;
                }
            } else {
                self.selection += 1;
            }
        }
    }

    pub fn add_filter_char(&mut self, c: char) {
        if !c.is_control() {
            self.filter.push(c);
        }
    }

    pub fn remove_filter_char(&mut self) {
        self.filter.pop();
    }

    pub fn reset_filter(&mut self) {
        self.filter.clear();
        self.selection = 0;
        self.scroll = 0;
    }

    pub fn selection_range(&self, visible_height: usize) -> Range<usize> {
        self.scroll..(self.scroll + visible_height).min(self.entries.len())
    }

    pub fn scroll_to_selection(&mut self, visible_height: usize) {
        if visible_height == 0 {
            self.scroll = self.selection;
            return;
        }
        if self.selection >= self.scroll + visible_height {
            self.scroll = self.selection - visible_height + 1;
        } else if self.selection < self.scroll {
            self.scroll = self.selection;
        }
        self.scroll = self
            .scroll
            .min(self.entries.len().saturating_sub(visible_height));
    }

    pub fn render_entries(
        &mut self,
        f: &mut Frame,
        area: Rect,
        theme: &Theme,
        mut render_entry: impl FnMut(&T, bool, &Theme, &mut Frame, Rect),
    ) {
        let visible_height = area.height as usize;
        self.scroll_to_selection(visible_height);
        for (line, index) in self.selection_range(visible_height).enumerate() {
            let line_area = Rect::new(area.x, area.y + line as u16, area.width, 1);
            render_entry(
                &self.entries[index],
                index == self.selection,
                theme,
                f,
                line_area,
            );
        }
    }

    pub fn render_popup(
        &mut self,
        f: &mut Frame,
        area: Rect,
        theme: &Theme,
        render_entry: impl FnMut(&T, bool, &Theme, &mut Frame, Rect),
    ) {
        if !self.visible {
            return;
        }
        let Some(popup) = centered_popup(area, &self.config) else {
            return;
        };
        render_popup_shell(f, &popup, &self.config, theme);
        let chunks = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).split(popup.inner);
        render_filter_bar(
            f,
            chunks[0],
            &self.filter,
            &self.config.filter_label,
            theme,
            &self.config.filter_key,
        );
        self.render_entries(f, chunks[1], theme, render_entry);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn popup(entries: usize, wrap: bool) -> ListPopup<usize> {
        let mut list = ListPopup::new(PopupConfig {
            title: "Test".to_string(),
            filter_label: "Filter".to_string(),
            width_pct: 0.6,
            height_pct: 0.55,
            min_width: 40,
            min_height: 10,
            wrap,
            border_key: "overlay_border".to_string(),
            filter_key: "overlay_filter".to_string(),
        });
        list.entries = (0..entries).collect();
        list
    }

    #[test]
    fn navigation_wraps_when_configured() {
        let mut list = popup(3, true);

        list.navigate_up();
        assert_eq!(list.selection, 2);

        list.navigate_down();
        assert_eq!(list.selection, 0);
    }

    #[test]
    fn navigation_clamps_when_wrapping_is_disabled() {
        let mut list = popup(3, false);

        list.navigate_up();
        assert_eq!(list.selection, 0);

        list.selection = 2;
        list.navigate_down();
        assert_eq!(list.selection, 2);
    }

    #[test]
    fn filter_operations_only_update_filter_state() {
        let mut list = popup(0, true);

        list.add_filter_char('a');
        list.add_filter_char('\n');
        list.remove_filter_char();

        assert!(list.filter.is_empty());
    }

    #[test]
    fn scrolling_keeps_selection_in_the_viewport() {
        let mut list = popup(10, true);
        list.selection = 6;

        list.scroll_to_selection(4);
        assert_eq!(list.scroll, 3);
        assert_eq!(list.selection_range(4), 3..7);

        list.selection = 1;
        list.scroll_to_selection(4);
        assert_eq!(list.scroll, 1);
    }

    #[test]
    fn resetting_filter_resets_navigation() {
        let mut list = popup(3, true);
        list.filter = "abc".to_string();
        list.selection = 2;
        list.scroll = 2;

        list.reset_filter();

        assert!(list.filter.is_empty());
        assert_eq!(list.selection, 0);
        assert_eq!(list.scroll, 0);
    }

    #[test]
    fn navigate_up_skip_passes_over_skippable_entries() {
        let mut list = popup(5, false);
        // entries: 0, 1, 2, 3, 4
        // skip the odd-numbered entries
        list.selection = 4;

        list.navigate_up_skip(|x| *x % 2 == 1);
        assert_eq!(list.selection, 2);

        list.navigate_up_skip(|x| *x % 2 == 1);
        assert_eq!(list.selection, 0);
    }

    #[test]
    fn navigate_up_skip_stays_at_boundary_when_no_valid_entry_above() {
        let mut list = popup(3, false);
        list.selection = 0;

        list.navigate_up_skip(|x| *x % 2 == 1);
        assert_eq!(list.selection, 0);
    }

    #[test]
    fn navigate_down_skip_passes_over_skippable_entries() {
        let mut list = popup(5, false);
        list.selection = 0;

        list.navigate_down_skip(|x| *x % 2 == 1);
        assert_eq!(list.selection, 2);

        list.navigate_down_skip(|x| *x % 2 == 1);
        assert_eq!(list.selection, 4);
    }

    #[test]
    fn navigate_down_skip_stays_at_boundary_when_no_valid_entry_below() {
        let mut list = popup(3, false);
        list.selection = 2;

        list.navigate_down_skip(|x| *x % 2 == 1);
        assert_eq!(list.selection, 2);
    }

    #[test]
    fn navigate_up_skip_wraps_when_configured() {
        let mut list = popup(3, true);
        list.selection = 2;

        list.navigate_up_skip(|x| *x % 2 == 1);
        assert_eq!(list.selection, 0);
    }

    #[test]
    fn navigate_down_skip_wraps_when_configured() {
        let mut list = popup(3, true);
        list.selection = 0;

        list.navigate_down_skip(|x| *x % 2 == 1);
        assert_eq!(list.selection, 2);
    }

    #[test]
    fn navigate_up_skip_all_skippable_stays_put() {
        let mut list = popup(3, false);
        list.selection = 1;

        list.navigate_up_skip(|_| true);
        assert_eq!(list.selection, 1);
    }

    #[test]
    fn navigate_down_skip_all_skippable_stays_put() {
        let mut list = popup(3, false);
        list.selection = 1;

        list.navigate_down_skip(|_| true);
        assert_eq!(list.selection, 1);
    }
}
