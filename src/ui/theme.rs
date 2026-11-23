//! Terminal styles shared by all screens.

use ratatui::style::{Color, Modifier, Style};

use crate::app::StatusLevel;

pub(super) const PLAIN: Style = Style::new();
pub(super) const ROW: Color = Color::Rgb(38, 43, 52);
pub(super) const HOVER_ROW: Color = Color::Rgb(30, 33, 40);
pub(super) const HOVER_SELECTED: Color = Color::Rgb(46, 52, 63);
pub(super) const HOVER_ACCENT: Color = Color::Rgb(155, 229, 214);

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
                PLAIN.bg(ROW),
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
            badge: PLAIN.bg(accent).fg(badge_text),
            disabled_badge: match mode {
                ColorMode::TrueColor => row.fg(muted),
                ColorMode::Ansi => PLAIN.fg(muted),
            },
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

    pub(super) fn hover_input(&self, focused: bool, intensity: f32) -> Style {
        let style = self.input(focused);
        match self.focus.bg {
            Some(color) if !focused => self.hover_background(style, color, intensity),
            _ => style,
        }
    }

    pub(super) fn hover_row(&self, selected: bool, intensity: f32) -> Style {
        let (style, color) = if selected {
            (self.row, HOVER_SELECTED)
        } else {
            (PLAIN, HOVER_ROW)
        };
        self.hover_background(style, color, intensity)
    }

    pub(super) fn hover_background(&self, style: Style, to: Color, intensity: f32) -> Style {
        if self.mode == ColorMode::TrueColor && intensity > 0.0 {
            let from = style.bg.or(self.base.bg).unwrap_or(Color::Reset);
            style.bg(mix(from, to, intensity))
        } else {
            style
        }
    }

    pub(super) fn hover_muted(&self, intensity: f32) -> Style {
        if self.mode == ColorMode::TrueColor && intensity > 0.0 {
            let from = self.muted.fg.unwrap_or(Color::Reset);
            let to = self.text.fg.unwrap_or(Color::Reset);
            self.muted.fg(mix(from, to, intensity))
        } else {
            self.muted
        }
    }

    pub(super) fn hover_text(&self, style: Style, intensity: f32) -> Style {
        if self.mode == ColorMode::Ansi && intensity >= 0.5 {
            style.add_modifier(Modifier::UNDERLINED)
        } else {
            style
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
        bold(if enabled {
            self.badge
        } else {
            self.disabled_badge
        })
    }

    pub(super) fn help_styles(
        &self,
        focused: bool,
        enabled: bool,
        key: Style,
        t: f32,
    ) -> (Style, Style, Style) {
        if focused || (self.mode == ColorMode::Ansi && t >= 0.5) {
            let button = if enabled {
                self.hover_background(self.badge, HOVER_ACCENT, t)
            } else {
                self.disabled_badge
            };
            return (button, bold(button), button);
        }
        if t == 0.0 || self.mode == ColorMode::Ansi {
            return (PLAIN, bold(key), self.muted);
        }

        let background = self.badge.bg.unwrap_or(Color::Reset);
        let foreground = self.badge.fg.unwrap_or(Color::Reset);
        let fade = |style: Style| style.fg(mix(style.fg.unwrap_or(Color::Reset), foreground, t));
        (
            self.hover_background(fade(self.text), background, t),
            bold(fade(key)),
            fade(self.muted),
        )
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

fn mix(from: Color, to: Color, t: f32) -> Color {
    match (from, to) {
        (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => {
            let channel = |a: u8, b: u8| {
                let a = f32::from(a);
                (a + (f32::from(b) - a) * t).round() as u8
            };
            Color::Rgb(channel(r1, r2), channel(g1, g2), channel(b1, b2))
        }
        _ if t >= 0.5 => to,
        _ => from,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mix_interpolates_and_rounds_each_rgb_channel() {
        let from = Color::Rgb(0, 40, 71);
        let to = Color::Rgb(11, 42, 60);
        assert_eq!(mix(from, to, 0.0), from);
        assert_eq!(mix(from, to, 0.5), Color::Rgb(6, 41, 66));
        assert_eq!(mix(from, to, 1.0), to);
    }

    #[test]
    fn mix_switches_at_half_strength_if_either_color_is_not_rgb() {
        let rgb = Color::Rgb(0, 35, 255);
        for (from, to) in [
            (Color::Cyan, rgb),
            (rgb, Color::Cyan),
            (Color::Cyan, Color::Red),
        ] {
            assert_eq!(mix(from, to, 0.0), from);
            assert_eq!(mix(from, to, 0.49), from);
            assert_eq!(mix(from, to, 0.5), to);
            assert_eq!(mix(from, to, 1.0), to);
        }
    }
}
