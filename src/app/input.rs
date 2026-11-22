//! Single-line text input shared by the form and filter.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{style::Style, text::Span};
use unicode_width::UnicodeWidthStr;

/// Single-line input with a cursor indexed by Unicode scalar values.
#[derive(Debug, Default)]
pub struct TextInput {
    value: String,
    cursor: usize,
}

impl TextInput {
    /// Returns the current text.
    pub fn value(&self) -> &str {
        &self.value
    }

    /// Returns the cursor's character index.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Returns the terminal column of the cursor within the field.
    pub fn cursor_column(&self) -> usize {
        self.value[..self.byte_index(self.cursor)].width()
    }

    pub(super) fn set_cursor_to_column(&mut self, mut column: usize) {
        let mut rest = self.value.as_str();
        for grapheme in Span::raw(&self.value).styled_graphemes(Style::default()) {
            let symbol = grapheme.symbol;
            // The renderer skips control characters when drawing graphemes.
            let Some(start) = rest.find(symbol) else {
                break;
            };
            rest = &rest[start..];
            let Some(remaining) = column.checked_sub(symbol.width()) else {
                self.cursor = self.value[..self.value.len() - rest.len()].chars().count();
                return;
            };
            rest = &rest[symbol.len()..];
            column = remaining;
        }
        self.cursor = self.value.chars().count();
    }

    pub(super) fn set(&mut self, value: String) {
        self.cursor = value.chars().count();
        self.value = value;
    }

    pub(super) fn handle_key(&mut self, key: KeyEvent) {
        if is_ctrl(key, 'u') {
            self.set(String::new());
            return;
        }
        if !is_plain(key) {
            return;
        }

        match key.code {
            KeyCode::Char(ch) if !ch.is_control() => {
                self.value.insert(self.byte_index(self.cursor), ch);
                self.cursor += 1;
            }
            KeyCode::Backspace if self.cursor > 0 => {
                self.cursor -= 1;
                self.value.remove(self.byte_index(self.cursor));
            }
            KeyCode::Delete if self.byte_index(self.cursor) < self.value.len() => {
                self.value.remove(self.byte_index(self.cursor));
            }
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => self.cursor = (self.cursor + 1).min(self.value.chars().count()),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.value.chars().count(),
            _ => {}
        }
    }

    pub(super) fn paste(&mut self, text: &str) {
        let line: String = text
            .chars()
            .take_while(|ch| !matches!(ch, '\r' | '\n'))
            .filter(|ch| !ch.is_control())
            .collect();
        self.value.insert_str(self.byte_index(self.cursor), &line);
        self.cursor += line.chars().count();
    }

    fn byte_index(&self, cursor: usize) -> usize {
        self.value
            .char_indices()
            .nth(cursor)
            .map_or(self.value.len(), |(index, _)| index)
    }
}

pub(super) fn is_plain(key: KeyEvent) -> bool {
    (key.modifiers - KeyModifiers::SHIFT).is_empty()
}

pub(super) fn is_ctrl(key: KeyEvent, ch: char) -> bool {
    key.modifiers - KeyModifiers::SHIFT == KeyModifiers::CONTROL
        && matches!(key.code, KeyCode::Char(input) if input.eq_ignore_ascii_case(&ch))
}
