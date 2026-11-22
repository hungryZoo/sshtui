//! Shared geometry and cell-aware text rendering.

use std::borrow::Cow;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use super::theme::PLAIN;

pub(super) fn inset(area: Rect, horizontal: u16, vertical: u16) -> Rect {
    let area = below(right_of(area, horizontal), vertical);
    Rect {
        width: area.width.saturating_sub(horizontal),
        height: area.height.saturating_sub(vertical),
        ..area
    }
}

pub(super) fn row(area: Rect, offset: u16) -> Rect {
    Rect {
        height: u16::from(offset < area.height),
        ..below(area, offset)
    }
}

pub(super) fn below(area: Rect, rows: u16) -> Rect {
    Rect::new(
        area.x,
        area.y.saturating_add(rows.min(area.height)),
        area.width,
        area.height.saturating_sub(rows),
    )
}

pub(super) fn left(area: Rect, width: u16) -> Rect {
    Rect {
        width: width.min(area.width),
        ..area
    }
}

pub(super) fn right_of(area: Rect, width: u16) -> Rect {
    Rect::new(
        area.x.saturating_add(width.min(area.width)),
        area.y,
        area.width.saturating_sub(width),
        area.height,
    )
}

pub(super) fn rule(frame: &mut Frame, area: Rect, style: Style) {
    put(frame, area, &"─".repeat(usize::from(area.width)), style);
}

pub(super) fn put(frame: &mut Frame, area: Rect, text: &str, style: Style) {
    frame.render_widget(Span::styled(clip_cells(text, area.width), style), area);
}

pub(super) fn put_right(frame: &mut Frame, area: Rect, text: &str, style: Style) {
    let text = tail_cells(text, area.width);
    let offset = area.width.saturating_sub(text.width() as u16);
    put(frame, right_of(area, offset), text, style);
}

fn tail_cells(text: &str, width: u16) -> &str {
    let mut remaining = text.width().saturating_sub(usize::from(width));
    let mut start = 0;
    for grapheme in Span::raw(text).styled_graphemes(PLAIN) {
        if remaining == 0 {
            break;
        }
        remaining = remaining.saturating_sub(grapheme.symbol.width());
        // Use the source offset because styled_graphemes skips control characters.
        if let Some(offset) = text[start..].find(grapheme.symbol) {
            start += offset + grapheme.symbol.len();
        }
    }
    &text[start..]
}

pub(super) fn header_line(
    frame: &mut Frame,
    area: Rect,
    title: Line<'_>,
    note: &str,
    note_style: Style,
) {
    if note.is_empty() {
        frame.render_widget(title, area);
        return;
    }
    let title_width = u16::try_from(title.width()).unwrap_or(u16::MAX);
    let note_area = right_of(area, title_width.saturating_add(2));
    put_right(frame, note_area, note, note_style);
    frame.render_widget(title, left(area, title_width));
}

/// Draws text and returns the area below its visible rows.
pub(super) fn render_lines(frame: &mut Frame, area: Rect, lines: Vec<Line<'_>>) -> Rect {
    let height = u16::try_from(lines.len())
        .unwrap_or(u16::MAX)
        .min(area.height);
    frame.render_widget(Paragraph::new(lines), Rect { height, ..area });
    below(area, height)
}

/// Returns the visible indices and their screen rectangles with the selection in view.
pub(super) fn visible_rows(
    area: Rect,
    count: usize,
    selection: Option<usize>,
) -> impl Iterator<Item = (usize, Rect)> {
    let offset = selection.filter(|&index| index < count).map_or(0, |index| {
        (index + 1).saturating_sub(usize::from(area.height))
    });
    area.rows()
        .zip(offset..count)
        .map(|(rect, index)| (index, rect))
}

/// Returns the longest prefix that fits without splitting a grapheme.
pub(super) fn clip_cells(value: &str, width: u16) -> Cow<'_, str> {
    let span = Span::raw(value);
    if span.width() <= usize::from(width) {
        return Cow::Borrowed(value);
    }
    let mut remaining = usize::from(width);
    Cow::Owned(
        span.styled_graphemes(PLAIN)
            .map_while(|grapheme| {
                remaining = remaining.checked_sub(grapheme.symbol.width())?;
                Some(grapheme.symbol)
            })
            .collect(),
    )
}
