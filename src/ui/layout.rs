//! Shared row and panel geometry.

use std::borrow::Cow;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use super::theme;

/// Returns a row within the area or an empty rectangle.
pub(super) fn row(area: Rect, offset: u16) -> Rect {
    Rect::new(
        area.x,
        area.y.saturating_add(offset.min(area.height)),
        area.width,
        u16::from(offset < area.height),
    )
}

/// Returns the area below the given number of rows.
pub(super) fn below(area: Rect, rows: u16) -> Rect {
    Rect::new(
        area.x,
        area.y.saturating_add(rows.min(area.height)),
        area.width,
        area.height.saturating_sub(rows),
    )
}

/// Returns the first columns of the area.
pub(super) fn left(area: Rect, width: u16) -> Rect {
    Rect {
        width: width.min(area.width),
        ..area
    }
}

/// Returns the columns after the given horizontal offset.
pub(super) fn right_of(area: Rect, width: u16) -> Rect {
    Rect::new(
        area.x.saturating_add(width.min(area.width)),
        area.y,
        area.width.saturating_sub(width),
        area.height,
    )
}

/// Checks whether a border can enclose at least one content cell.
pub(super) fn has_border(area: Rect) -> bool {
    area.width >= 3 && area.height >= 3
}

/// Draws a border when it fits and returns the content area.
pub(super) fn draw_panel(
    frame: &mut Frame,
    area: Rect,
    title: Option<Line<'_>>,
    active: bool,
) -> Rect {
    if !has_border(area) {
        return area;
    }
    let block = theme::panel(title, active);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    inner
}

/// Adds horizontal spacing around the panel title.
pub(super) fn panel_title(title: &str) -> Line<'static> {
    Line::from(format!(" {title} "))
}

/// Adds a column of padding on each side when space permits.
pub(super) fn inset_horizontal(area: Rect) -> Rect {
    if area.width < 8 {
        area
    } else {
        left(right_of(area, 1), area.width.saturating_sub(2))
    }
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
        span.styled_graphemes(theme::PLAIN)
            .map_while(|grapheme| {
                remaining = remaining.checked_sub(grapheme.symbol.width())?;
                Some(grapheme.symbol)
            })
            .collect(),
    )
}
