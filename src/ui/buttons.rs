//! Button rendering and hit rectangles.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Span;
use unicode_width::UnicodeWidthStr;

use super::layout::{left, right_of};
use super::theme;

/// The visual role of a button in its row.
#[derive(Clone, Copy)]
pub(super) enum ButtonKind {
    Primary,
    Secondary,
    Disabled,
}

impl ButtonKind {
    fn style(self) -> Style {
        match self {
            Self::Primary => theme::SELECTED,
            Self::Secondary => theme::PLAIN,
            Self::Disabled => theme::MUTED,
        }
    }
}

/// The label and visual role of a button.
pub(super) type Button<'a> = (&'a str, ButtonKind);

fn after_button(area: Rect, button: Rect) -> Rect {
    right_of(
        area,
        button.right().saturating_sub(area.x).saturating_add(2),
    )
}

/// Returns the rectangle occupied by each button, in display order.
pub(super) fn button_areas<'a>(
    mut area: Rect,
    buttons: &'a [Button<'a>],
) -> impl Iterator<Item = (Button<'a>, Rect)> {
    buttons.iter().copied().map_while(move |button| {
        let width = u16::try_from(button.0.width().saturating_add(4)).ok()?;
        if area.is_empty() || width > area.width {
            return None;
        }
        let bounds = left(area, width);
        area = after_button(area, bounds);
        Some((button, bounds))
    })
}

/// Returns the space after the last button and its spacing.
pub(super) fn after_buttons(area: Rect, buttons: &[Button<'_>]) -> Rect {
    button_areas(area, buttons)
        .last()
        .map_or(area, |(_, bounds)| after_button(area, bounds))
}

/// Draws a row of buttons at their computed rectangles.
pub(super) fn render_buttons(frame: &mut Frame, area: Rect, buttons: &[Button<'_>]) {
    for ((label, kind), bounds) in button_areas(area, buttons) {
        frame.render_widget(Span::styled(format!("[ {label} ]"), kind.style()), bounds);
    }
}
