//! Terminal styles shared by all screens.

use ratatui::style::{Color, Modifier, Style};

use crate::app::StatusLevel;

pub(super) const PLAIN: Style = Style::new();

/// The color capabilities of the terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    TrueColor,
    Ansi,
}

/// Styles for the terminal and its overlays.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    mode: ColorMode,
    pub(super) dim: bool,
    pub(super) base: Style,
    pub(super) text: Style,
    pub(super) muted: Style,
    pub(super) accent: Style,
    pub(super) warn: Style,
    pub(super) error: Style,
    pub(super) ok: Style,
    pub(super) rule: Style,
    input: Style,
    focus: Style,
    dim_input: Style,
    pub(super) row: Style,
    pub(super) removed: Style,
    pub(super) added: Style,
    badge: Style,
    disabled_badge: Style,
}

impl Theme {
    /// Builds the palette for the terminal's color mode.
    pub fn new(mode: ColorMode) -> Self {
        let (bg, text, muted, accent, warn, error, ok, rule) = match mode {
            ColorMode::TrueColor => (
                Color::Rgb(21, 23, 28),
                Color::Rgb(216, 219, 225),
                Color::Rgb(141, 148, 160),
                Color::Rgb(111, 211, 193),
                Color::Rgb(230, 181, 102),
                Color::Rgb(240, 138, 138),
                Color::Rgb(156, 207, 130),
                Color::Rgb(58, 64, 75),
            ),
            ColorMode::Ansi => (
                Color::Reset,
                Color::Reset,
                Color::DarkGray,
                Color::Cyan,
                Color::Yellow,
                Color::Red,
                Color::Green,
                Color::DarkGray,
            ),
        };
        let (input, focus, dim_input, row, removed, added, badge_text) = match mode {
            ColorMode::TrueColor => (
                PLAIN.bg(Color::Rgb(32, 36, 44)),
                PLAIN.bg(Color::Rgb(43, 49, 59)),
                PLAIN.bg(Color::Rgb(27, 30, 36)),
                PLAIN.bg(Color::Rgb(38, 43, 52)),
                PLAIN.bg(Color::Rgb(58, 34, 38)),
                PLAIN.bg(Color::Rgb(31, 51, 38)),
                bg,
            ),
            ColorMode::Ansi => {
                let underline = PLAIN.add_modifier(Modifier::UNDERLINED);
                (
                    underline,
                    underline.underline_color(accent),
                    underline.underline_color(muted),
                    bold(PLAIN.bg(accent).fg(Color::Black)),
                    PLAIN,
                    PLAIN,
                    Color::Black,
                )
            }
        };

        Self {
            mode,
            dim: false,
            base: PLAIN.bg(bg).fg(text),
            text: PLAIN.fg(text),
            muted: PLAIN.fg(muted),
            accent: PLAIN.fg(accent),
            warn: PLAIN.fg(warn),
            error: PLAIN.fg(error),
            ok: PLAIN.fg(ok),
            rule: PLAIN.fg(rule),
            input,
            focus,
            dim_input,
            row,
            removed: removed.fg(error),
            added: added.fg(ok),
            badge: bold(PLAIN.bg(accent).fg(badge_text)),
            disabled_badge: bold(match mode {
                ColorMode::TrueColor => row.fg(muted),
                ColorMode::Ansi => PLAIN.fg(muted),
            }),
        }
    }

    /// Returns the same palette with muted text and darkened inputs behind a popup.
    pub fn dimmed(&self) -> Self {
        Self {
            dim: true,
            base: self.base.patch(self.muted),
            text: self.muted,
            accent: self.muted,
            warn: self.muted,
            error: self.muted,
            ok: self.muted,
            row: PLAIN,
            removed: self.muted,
            added: self.muted,
            badge: self.disabled_badge,
            ..*self
        }
    }

    pub(super) fn input(&self, focused: bool) -> Style {
        if self.dim {
            self.dim_input
        } else if focused {
            self.focus
        } else {
            self.input
        }
    }

    pub(super) fn selected(&self, style: Style, selected: bool) -> Style {
        if selected && self.mode == ColorMode::Ansi && !self.dim {
            style.patch(self.row)
        } else {
            style
        }
    }

    pub(super) fn badge(&self, enabled: bool) -> Style {
        if enabled {
            self.badge
        } else {
            self.disabled_badge
        }
    }

    pub(super) fn status(&self, level: StatusLevel) -> Style {
        match level {
            StatusLevel::Info => self.ok,
            StatusLevel::Warning => self.warn,
            StatusLevel::Error => self.error,
        }
    }
}

pub(super) fn bold(style: Style) -> Style {
    style.add_modifier(Modifier::BOLD)
}
