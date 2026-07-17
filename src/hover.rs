use crate::lsp::LspResponse;
use serde_json::Value;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct HoverState {
    pub text: String,
    pub scroll: u16,
    pub max_scroll: u16,
    pub visible: bool,
    pub pending_request: Option<u64>,
}

impl HoverState {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn begin_request(&mut self, request_id: u64) {
        self.clear();
        self.pending_request = Some(request_id);
    }

    pub fn apply_response(&mut self, request_id: u64, response: LspResponse) -> bool {
        if self.pending_request != Some(request_id) {
            return false;
        }
        self.pending_request = None;
        match parse_hover_response(response) {
            Ok(Some(text)) => {
                self.text = text;
                self.visible = true;
                true
            }
            Ok(None) | Err(_) => {
                self.clear();
                false
            }
        }
    }

    pub fn scroll_by(&mut self, delta: i16) {
        self.scroll = (self.scroll as i32 + delta as i32).clamp(0, self.max_scroll as i32) as u16;
    }
}

pub fn parse_hover_response(response: LspResponse) -> Result<Option<String>, Value> {
    let value = match response {
        LspResponse::Success(value) => value,
        LspResponse::Error(error) => return Err(error),
    };
    if value.is_null() {
        return Ok(None);
    }
    let Some(contents) = value.get("contents") else {
        return Ok(None);
    };
    let normalized = parse_contents(contents).unwrap_or_default();
    let normalized = normalized.trim().to_string();
    Ok((!normalized.is_empty()).then_some(normalized))
}

fn parse_contents(value: &Value) -> Option<String> {
    match value {
        Value::String(_) => parse_marked_string(value),
        Value::Array(items) => {
            let parts = items
                .iter()
                .filter_map(parse_marked_string)
                .filter(|part| !part.trim().is_empty())
                .collect::<Vec<_>>();
            (!parts.is_empty()).then(|| parts.join("\n\n"))
        }
        Value::Object(_) => parse_marked_string(value),
        _ => None,
    }
}

fn parse_marked_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(normalize_markdown(text)),
        Value::Object(object) => {
            let text = object.get("value").and_then(Value::as_str)?;
            match object.get("kind").and_then(Value::as_str) {
                Some("markdown") => Some(normalize_markdown(text)),
                Some("plaintext") => Some(text.to_string()),
                Some(_) => None,
                None if object.get("language").and_then(Value::as_str).is_some() => {
                    Some(text.to_string())
                }
                None => None,
            }
        }
        _ => None,
    }
}

fn normalize_markdown(input: &str) -> String {
    input
        .lines()
        .filter(|line| !line.trim_start().starts_with("```"))
        .map(normalize_inline)
        .collect::<Vec<_>>()
        .join("\n")
}

fn is_emphasis_boundary(character: Option<&char>) -> bool {
    match character {
        None => true,
        Some(character) => character.is_whitespace() || character.is_ascii_punctuation(),
    }
}

fn normalize_inline(input: &str) -> String {
    let chars = input.chars().collect::<Vec<_>>();
    let mut output = String::new();
    let mut index = 0;

    while index < chars.len() {
        if chars[index] == '[' {
            if let Some(label_end_offset) = chars[index + 1..].iter().position(|ch| *ch == ']') {
                let label_end = index + 1 + label_end_offset;
                if chars.get(label_end + 1) == Some(&'(') {
                    if let Some(url_end_offset) =
                        chars[label_end + 2..].iter().position(|ch| *ch == ')')
                    {
                        let url_end = label_end + 2 + url_end_offset;
                        output.extend(chars[index + 1..label_end].iter());
                        output.push_str(" (");
                        output.extend(chars[label_end + 2..url_end].iter());
                        output.push(')');
                        index = url_end + 1;
                        continue;
                    }
                }
            }
        }

        if chars[index] == '`' {
            index += 1;
            continue;
        }

        if index + 1 < chars.len()
            && matches!((chars[index], chars[index + 1]), ('*', '*') | ('_', '_'))
        {
            index += 2;
            continue;
        }

        if matches!(chars[index], '*' | '_') {
            let previous = index
                .checked_sub(1)
                .and_then(|previous| chars.get(previous));
            let next = chars.get(index + 1);
            let previous_is_alphanumeric = previous.is_some_and(|ch| ch.is_alphanumeric());
            let next_is_alphanumeric = next.is_some_and(|ch| ch.is_alphanumeric());
            if (previous_is_alphanumeric && is_emphasis_boundary(next))
                || (is_emphasis_boundary(previous) && next_is_alphanumeric)
            {
                index += 1;
                continue;
            }
        }

        output.push(chars[index]);
        index += 1;
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::LspResponse;
    use serde_json::json;

    #[test]
    fn parses_markup_and_marked_string_hover_content() {
        assert_eq!(
            parse_hover_response(LspResponse::Success(json!({
                "contents": {
                    "kind": "markdown",
                    "value": "**Vec** stores `snake_case` items.\n\n```rust\nlet v = Vec::new();\n```"
                }
            }))),
            Ok(Some(
                "Vec stores snake_case items.\n\nlet v = Vec::new();".into()
            ))
        );

        assert_eq!(
            parse_hover_response(LspResponse::Success(json!({
                "contents": [
                    {"language": "rust", "value": "fn len(&self) -> usize"},
                    "Returns [the length](https://example.test/len)."
                ]
            }))),
            Ok(Some(
                "fn len(&self) -> usize\n\nReturns the length (https://example.test/len).".into()
            ))
        );
    }

    #[test]
    fn preserves_plaintext_and_language_marked_strings() {
        assert_eq!(
            parse_hover_response(LspResponse::Success(json!({
                "contents": {
                    "kind": "plaintext",
                    "value": "Keep `ticks` and * literal markers."
                }
            }))),
            Ok(Some("Keep `ticks` and * literal markers.".into()))
        );

        assert_eq!(
            parse_hover_response(LspResponse::Success(json!({
                "contents": {
                    "language": "rust",
                    "value": "fn pointer(value: *mut i32) -> snake_case"
                }
            }))),
            Ok(Some("fn pointer(value: *mut i32) -> snake_case".into()))
        );
    }

    #[test]
    fn null_empty_and_error_hover_responses_do_not_open() {
        assert_eq!(
            parse_hover_response(LspResponse::Success(serde_json::Value::Null)),
            Ok(None)
        );
        assert_eq!(
            parse_hover_response(LspResponse::Success(json!({"contents": ""}))),
            Ok(None)
        );
        assert!(parse_hover_response(LspResponse::Error(json!({
            "code": -32603,
            "message": "failed"
        })))
        .is_err());
    }

    #[test]
    fn single_markers_are_removed_only_at_emphasis_boundaries() {
        assert_eq!(
            parse_hover_response(LspResponse::Success(json!({
                "contents": "Use a*b, a_b, *bold*, and _italic_."
            }))),
            Ok(Some("Use a*b, a_b, bold, and italic.".into()))
        );
    }

    #[test]
    fn symbol_adjacent_markers_are_not_emphasis_boundaries() {
        assert_eq!(
            parse_hover_response(LspResponse::Success(json!({
                "contents": "Keep x*√y, name_♥, and snake_case; strip *bold* and _italic_."
            }))),
            Ok(Some(
                "Keep x*√y, name_♥, and snake_case; strip bold and italic.".into()
            ))
        );
    }

    #[test]
    fn stale_response_is_ignored_and_scrolling_is_clamped() {
        let mut hover = HoverState::default();
        hover.begin_request(9);

        assert!(!hover.apply_response(8, LspResponse::Success(json!({"contents": "old"}))));
        assert!(!hover.visible);

        assert!(hover.apply_response(9, LspResponse::Success(json!({"contents": "new"}))));
        assert_eq!(hover.text, "new");
        assert!(hover.visible);

        hover.max_scroll = 3;
        hover.scroll_by(5);
        assert_eq!(hover.scroll, 3);
        hover.scroll_by(-2);
        assert_eq!(hover.scroll, 1);
        hover.clear();
        assert_eq!(hover, HoverState::default());
    }
}
