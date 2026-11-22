//! Terminal styles shared by all screens.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType};

use crate::app::StatusLevel;

pub(super) const PLAIN: Style = Style::new();
pub(super) const ACCENT: Style = PLAIN.fg(Color::Cyan);
pub(super) const MUTED: Style = PLAIN.fg(Color::DarkGray);
pub(super) const ERROR: Style = PLAIN.fg(Color::Red);
pub(super) const WARNING: Style = PLAIN.fg(Color::Yellow);
pub(super) const SUCCESS: Style = PLAIN.fg(Color::Green);
pub(super) const BOLD: Style = PLAIN.add_modifier(Modifier::BOLD);
pub(super) const EMPHASIS: Style = ACCENT.add_modifier(Modifier::BOLD);
pub(super) const UPDATE_NOTICE: Style = WARNING.add_modifier(Modifier::BOLD);
pub(super) const SELECTED: Style = BOLD.fg(Color::Black).bg(Color::Cyan);
pub(super) const INPUT: Style = PLAIN.add_modifier(Modifier::UNDERLINED);
pub(super) const FOCUSED_INPUT: Style = INPUT.fg(Color::Cyan).underline_color(Color::Cyan);

/// Keeps inherited text muted while indicating focus with the underline.
pub(super) fn inherited_input(focused: bool) -> Style {
    if focused { FOCUSED_INPUT } else { INPUT }.fg(Color::DarkGray)
}

/// Styles a status message according to its severity.
pub(super) fn status(level: StatusLevel) -> Style {
    match level {
        StatusLevel::Info => SUCCESS,
        StatusLevel::Warning => WARNING,
        StatusLevel::Error => ERROR,
    }
}

/// Builds the rounded border for a panel.
pub(super) fn panel(title: Option<Line<'_>>, active: bool) -> Block<'_> {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(if active { ACCENT } else { MUTED });
    match title {
        Some(title) => block.title(title),
        None => block,
    }
}
