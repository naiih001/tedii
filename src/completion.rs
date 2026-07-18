use crate::fuzzy::fuzzy_score;
use crate::lsp::{parse_completion_response, CompletionItem, LspResponse};

pub const MAX_VISIBLE_ITEMS: usize = 10;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct CompletionState {
    pub items: Vec<CompletionItem>,
    pub selected: usize,
    pub visible: bool,
    pub pending_request: Option<u64>,
    pub trigger_offset: usize,
    pub prefix: String,
    pub filtered_indices: Vec<usize>,
    pub scroll_offset: usize,
}

impl CompletionState {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn begin_request(&mut self, request_id: u64, trigger_offset: usize) {
        self.clear();
        self.pending_request = Some(request_id);
        self.trigger_offset = trigger_offset;
    }

    pub fn apply_response(&mut self, request_id: u64, response: LspResponse) -> bool {
        if self.pending_request != Some(request_id) {
            return false;
        }
        self.pending_request = None;
        match parse_completion_response(response) {
            Ok(items) if !items.is_empty() => {
                let preselect_idx = items
                    .iter()
                    .position(|item| item.preselect)
                    .unwrap_or(0);
                self.items = items;
                self.filtered_indices = (0..self.items.len()).collect();
                self.selected = preselect_idx;
                self.scroll_offset = 0;
                self.visible = true;
                true
            }
            Ok(_) | Err(_) => {
                self.clear();
                false
            }
        }
    }

    pub fn active_item(&self) -> Option<&CompletionItem> {
        if self.visible && !self.filtered_indices.is_empty() {
            let idx = self.filtered_indices[self.selected.min(self.filtered_indices.len() - 1)];
            Some(&self.items[idx])
        } else {
            None
        }
    }

    pub fn select_next(&mut self) {
        if !self.visible || self.filtered_indices.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % self.filtered_indices.len();
        self.ensure_selected_visible();
    }

    pub fn select_prev(&mut self) {
        if !self.visible || self.filtered_indices.is_empty() {
            return;
        }
        self.selected = if self.selected == 0 {
            self.filtered_indices.len() - 1
        } else {
            self.selected - 1
        };
        self.ensure_selected_visible();
    }

    fn ensure_selected_visible(&mut self) {
        let visible = self.visible_count();
        if visible == 0 {
            return;
        }
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        } else if self.selected >= self.scroll_offset + visible {
            self.scroll_offset = self.selected + 1 - visible;
        }
    }

    pub fn filter(&mut self, prefix: &str) {
        self.prefix = prefix.to_string();
        self.scroll_offset = 0;
        if prefix.is_empty() {
            self.filtered_indices = (0..self.items.len()).collect();
            let preselect_idx = self.items.iter().position(|item| item.preselect);
            self.selected = preselect_idx.unwrap_or(0);
            return;
        }
        let mut scored: Vec<(usize, i64)> = Vec::new();
        for (i, item) in self.items.iter().enumerate() {
            let match_text = item.filter_text.as_deref().unwrap_or(&item.label);
            if let Some((score, _)) = fuzzy_score(prefix, match_text) {
                scored.push((i, score));
            }
        }
        scored.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        self.filtered_indices = scored.into_iter().map(|(i, _)| i).collect();
        if self.filtered_indices.is_empty() {
            self.visible = false;
            return;
        }
        let preselect_idx = self
            .filtered_indices
            .iter()
            .position(|&i| self.items[i].preselect);
        self.selected = preselect_idx.unwrap_or(0);
    }

    pub fn visible_count(&self) -> usize {
        self.filtered_indices.len().min(MAX_VISIBLE_ITEMS)
    }
}

fn strip_snippet(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '$' && i + 1 < chars.len() && chars[i + 1] == '{' {
            let mut depth = 1u32;
            let mut has_colon = None;
            let mut j = i + 2;
            while j < chars.len() && depth > 0 {
                match chars[j] {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            if let Some(colon_pos) = has_colon {
                                let default_text: String =
                                    chars[colon_pos + 1..j].iter().collect();
                                result.push_str(&default_text);
                            }
                        }
                    }
                    ':' if depth == 1 && has_colon.is_none() => {
                        has_colon = Some(j);
                    }
                    _ => {}
                }
                j += 1;
            }
            i = j;
        } else if chars[i] == '$'
            && i + 1 < chars.len()
            && chars[i + 1].is_ascii_digit()
        {
            i += 2;
            if i < chars.len() && chars[i] == ':' {
                i += 1;
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    result
}

pub fn completion_insert_text(item: &CompletionItem) -> String {
    let text = item
        .text_edit_new_text
        .as_deref()
        .or(item.insert_text.as_deref())
        .unwrap_or(&item.label);
    strip_snippet(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::{LspResponse, CompletionItem};
    use serde_json::json;

    fn make_item(label: &str) -> CompletionItem {
        CompletionItem {
            label: label.to_string(),
            kind: None,
            detail: None,
            insert_text: None,
            sort_text: None,
            filter_text: None,
            text_edit_range: None,
            text_edit_new_text: None,
            preselect: false,
            original_index: 0,
        }
    }

    #[test]
    fn apply_response_populates_items() {
        let mut state = CompletionState::default();
        state.begin_request(1, 0);
        let items = vec![
            make_item("foo"),
            make_item("bar"),
            make_item("baz"),
        ];
        let response = LspResponse::Success(json!(items.iter().map(|i| json!({"label": i.label})).collect::<Vec<_>>()));
        assert!(state.apply_response(1, response));
        assert!(state.visible);
        assert_eq!(state.items.len(), 3);
        assert_eq!(state.filtered_indices.len(), 3);
    }

    #[test]
    fn stale_response_is_ignored() {
        let mut state = CompletionState::default();
        state.begin_request(1, 0);
        let response = LspResponse::Success(json!([{"label": "old"}]));
        assert!(!state.apply_response(2, response));
        assert!(!state.visible);
    }

    #[test]
    fn select_next_and_prev_wrap_around() {
        let mut state = CompletionState::default();
        state.begin_request(1, 0);
        state.items = vec![make_item("a"), make_item("b"), make_item("c")];
        state.filtered_indices = vec![0, 1, 2];
        state.visible = true;
        state.selected = 0;

        state.select_next();
        assert_eq!(state.selected, 1);
        state.select_next();
        assert_eq!(state.selected, 2);
        state.select_next();
        assert_eq!(state.selected, 0);

        state.select_prev();
        assert_eq!(state.selected, 2);
    }

    #[test]
    fn filter_reduces_items_by_fuzzy_match() {
        let mut state = CompletionState::default();
        state.begin_request(1, 0);
        state.items = vec![
            CompletionItem {
                filter_text: Some("foo_bar".into()),
                ..make_item("foo_bar")
            },
            CompletionItem {
                filter_text: Some("foo_baz".into()),
                ..make_item("foo_baz")
            },
            CompletionItem {
                filter_text: Some("xyz".into()),
                ..make_item("xyz")
            },
        ];
        state.visible = true;
        state.filter("foo_b");
        assert!(state.visible);
        assert_eq!(state.filtered_indices.len(), 2);

        state.filter("foo_ba");
        assert!(state.visible);
        assert_eq!(state.filtered_indices.len(), 2);

        state.filter("xyz");
        assert!(state.visible);
        assert_eq!(state.filtered_indices.len(), 1);

        state.filter("nonexistent");
        assert!(!state.visible);
    }

    #[test]
    fn filter_fuzzy_matches_non_prefix() {
        let mut state = CompletionState::default();
        state.begin_request(1, 0);
        state.items = vec![
            CompletionItem {
                filter_text: Some("foo_bar".into()),
                ..make_item("foo_bar")
            },
            CompletionItem {
                filter_text: Some("foo_baz".into()),
                ..make_item("foo_baz")
            },
            CompletionItem {
                filter_text: Some("xyz".into()),
                ..make_item("xyz")
            },
        ];
        state.visible = true;
        state.filter("fb");
        assert!(state.visible);
        assert_eq!(state.filtered_indices.len(), 2);
        let labels: Vec<&str> = state
            .filtered_indices
            .iter()
            .map(|&i| state.items[i].label.as_str())
            .collect();
        assert!(labels.contains(&"foo_bar"));
        assert!(labels.contains(&"foo_baz"));
    }

    #[test]
    fn filter_sorts_by_relevance() {
        let mut state = CompletionState::default();
        state.begin_request(1, 0);
        state.items = vec![
            CompletionItem {
                filter_text: Some("xfoob".into()),
                ..make_item("xfoob")
            },
            CompletionItem {
                filter_text: Some("foo_bar".into()),
                ..make_item("foo_bar")
            },
        ];
        state.visible = true;
        state.filter("fb");
        assert!(state.visible);
        assert_eq!(state.filtered_indices.len(), 2);
        assert_eq!(state.items[state.filtered_indices[0]].label, "foo_bar");
    }

    #[test]
    fn filter_empty_prefix_shows_all() {
        let mut state = CompletionState::default();
        state.begin_request(1, 0);
        state.items = vec![make_item("a"), make_item("b"), make_item("c")];
        state.visible = true;
        state.filter("");
        assert!(state.visible);
        assert_eq!(state.filtered_indices.len(), 3);
    }

    #[test]
    fn strip_snippet_removes_placeholders() {
        assert_eq!(strip_snippet("foo($1:$2)"), "foo()");
        assert_eq!(strip_snippet("${1:name}"), "name");
        assert_eq!(strip_snippet("text${1:a}more"), "textamore");
        assert_eq!(strip_snippet("plain text"), "plain text");
    }

    #[test]
    fn completion_insert_text_prefers_text_edit_over_insert_text() {
        let item = CompletionItem {
            text_edit_new_text: Some("edit_text".into()),
            insert_text: Some("insert_text".into()),
            label: "label".into(),
            ..make_item("label")
        };
        assert_eq!(completion_insert_text(&item), "edit_text");

        let item2 = CompletionItem {
            text_edit_new_text: None,
            insert_text: Some("insert_text".into()),
            label: "label".into(),
            ..make_item("label")
        };
        assert_eq!(completion_insert_text(&item2), "insert_text");

        let item3 = make_item("label");
        assert_eq!(completion_insert_text(&item3), "label");
    }

    #[test]
    fn scroll_offset_advances_when_selecting_past_visible_window() {
        let mut state = CompletionState::default();
        state.items = (0..15).map(|i| make_item(&format!("item_{i}"))).collect();
        state.filtered_indices = (0..15).collect();
        state.visible = true;
        state.selected = 0;
        state.scroll_offset = 0;

        for _ in 0..10 {
            state.select_next();
        }
        assert_eq!(state.selected, 10);
        assert_eq!(state.scroll_offset, 1);
    }

    #[test]
    fn scroll_offset_resets_when_wrapping_to_top() {
        let mut state = CompletionState::default();
        state.items = (0..15).map(|i| make_item(&format!("item_{i}"))).collect();
        state.filtered_indices = (0..15).collect();
        state.visible = true;
        state.selected = 0;
        state.scroll_offset = 0;

        for _ in 0..10 {
            state.select_next();
        }
        assert_eq!(state.scroll_offset, 1);
        state.select_next();
        assert_eq!(state.selected, 11);
        assert_eq!(state.scroll_offset, 2);

        state.select_prev();
        assert_eq!(state.selected, 10);
        assert_eq!(state.scroll_offset, 2);

        state.selected = 14;
        state.scroll_offset = 5;
        state.select_next();
        assert_eq!(state.selected, 0);
        assert_eq!(state.scroll_offset, 0);
    }

    #[test]
    fn scroll_offset_resets_on_filter() {
        let mut state = CompletionState::default();
        state.items = (0..15).map(|i| make_item(&format!("item_{i}"))).collect();
        state.filtered_indices = (0..15).collect();
        state.visible = true;
        state.scroll_offset = 5;

        state.filter("item_1");
        assert_eq!(state.scroll_offset, 0);
    }
}
