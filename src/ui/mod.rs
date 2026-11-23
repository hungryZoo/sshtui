//! Terminal screens and mouse hit testing.

mod form;
mod hover;
mod layout;
mod list;
mod overlays;
mod theme;

use std::time::Instant;

use crossterm::event::KeyCode;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Backend, ClickTarget, Mode, TextInput};

pub use self::hover::{Hover, HoverKey};
pub use self::theme::{ColorMode, Theme};

use self::layout::{
    below, header_line, inset, left, padded_cells, render_lines, right_of, row, rule,
};
use self::theme::{PLAIN, bold};

type KeyHint = (&'static str, &'static str, Option<KeyCode>);
type KeyHints = (bool, Vec<KeyHint>);
type HoverFrame<'a> = Option<(&'a Hover, Instant)>;

fn intensity(hover: HoverFrame<'_>, key: HoverKey) -> f32 {
    hover.map_or(0.0, |(hover, now)| hover.intensity(&key, now))
}

/// Clickable regions of the last rendered frame.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HitMap {
    regions: Vec<(Rect, ClickTarget)>,
}

impl HitMap {
    /// Returns the target at a terminal cell, preferring later regions.
    pub fn hit(&self, column: u16, row: u16) -> Option<&ClickTarget> {
        self.regions
            .iter()
            .rev()
            .find(|(area, _)| area.contains((column, row).into()))
            .map(|(_, target)| target)
    }

    fn add(&mut self, area: Rect, target: ClickTarget) {
        if !area.is_empty() {
            self.regions.push((area, target));
        }
    }
}

/// Draws the screen and returns its clickable regions. No I/O.
pub fn render<B: Backend>(
    frame: &mut Frame,
    app: &App<B>,
    theme: &Theme,
    hover: Option<(&Hover, Instant)>,
) -> HitMap {
    let screen = frame.area();
    let mut hits = HitMap::default();
    if screen.is_empty() {
        return hits;
    }
    let modal = matches!(
        app.mode(),
        Mode::KeyPicker(_) | Mode::Import(_) | Mode::Notice(_) | Mode::Update(_)
    );
    let backdrop = if modal { &theme.dimmed() } else { theme };
    let backdrop_hover = if modal { None } else { hover };
    frame.buffer_mut().set_style(screen, backdrop.base);
    let area = inset(
        screen,
        if screen.width >= 64 { 2 } else { 0 },
        u16::from(screen.height >= 18),
    );
    let header = row(area, 0);
    let footer = row(area, area.height.saturating_sub(1));
    let footer_rule = row(area, area.height.saturating_sub(2));
    let mut body = below(area, 1);
    body.height = body.height.saturating_sub(2);
    let status = app.status().map(|status| {
        styled_wrapped(
            &status.text,
            area.width,
            body.height.min(3),
            backdrop.status(status.level),
        )
    });
    if let Some(lines) = status {
        body.height = body.height.saturating_sub(lines.len() as u16);
        render_lines(
            frame,
            below(area, body.bottom().saturating_sub(area.y)),
            lines,
        );
    }
    if area.height >= 3 {
        rule(frame, footer_rule, backdrop.rule);
    }

    let paths = app.backend().paths();
    let lang = app.lang();
    let base_help = match app.mode() {
        Mode::List | Mode::Import(_) | Mode::Notice(_) | Mode::Update(_) => {
            list::header(frame, app, header, backdrop, &mut hits, backdrop_hover);
            list::render(frame, app, body, backdrop, &mut hits, backdrop_hover);
            list::help(app)
        }
        Mode::Form(_) | Mode::KeyPicker(_) => {
            let Some(state) = app.form() else { return hits };
            let (title, source) = form::heading(state, paths, lang);
            header_line(
                frame,
                header,
                headline(title, bold(backdrop.text), backdrop),
                &source,
                backdrop.muted,
            );
            form::render(frame, app, state, body, backdrop, &mut hits, backdrop_hover);
            form::help(app, state)
        }
        Mode::Confirm(state) => {
            let path = paths.abbreviate_home(state.path());
            header_line(
                frame,
                header,
                headline(state.title(), bold(theme.text), theme),
                &path,
                theme.muted,
            );
            overlays::confirm(frame, state, paths, body, theme, &mut hits);
            (true, overlays::confirm_help(state))
        }
    };
    let help = match app.mode() {
        Mode::KeyPicker(state) => (
            true,
            overlays::key_picker(frame, state, paths, body, theme, &mut hits, hover),
        ),
        Mode::Import(state) => (
            true,
            overlays::import(frame, state, app, body, theme, &mut hits, hover),
        ),
        Mode::Notice(state) => (true, overlays::notice(frame, state, body, theme, &mut hits)),
        Mode::Update(state) => (true, overlays::update(frame, state, body, theme, &mut hits)),
        _ => base_help,
    };
    if area.height >= 2 {
        render_help(frame, help, footer, theme, &mut hits, app, hover);
    }
    hits
}

fn headline(title: String, style: Style, theme: &Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled("sshtui", bold(theme.accent)),
        Span::raw("  "),
        Span::styled(title, style),
    ])
}

fn render_help<B: Backend>(
    frame: &mut Frame,
    (enabled, mut hints): KeyHints,
    mut area: Rect,
    theme: &Theme,
    hits: &mut HitMap,
    app: &App<B>,
    hover: HoverFrame<'_>,
) {
    const GAP: u16 = 3;

    let pair_width = |index, (key, description, _): &KeyHint| {
        key.width() + description.width() + if index == 0 { 3 } else { 1 }
    };
    let gap = usize::from(GAP);
    let available = usize::from(area.width);
    let last = hints.len().saturating_sub(1);
    let on_list = matches!(app.mode(), Mode::List) && !app.is_filtering();
    let mut focused = if on_list {
        app.list_action().index()
    } else {
        0
    };
    if last >= 2 && {
        let first_with_gap = pair_width(0, &hints[0]) + gap;
        first_with_gap + pair_width(last, &hints[last]) <= available
            || (focused > 0 && first_with_gap + pair_width(focused, &hints[focused]) <= available)
    } {
        // Keep filtered-list hints stable as the selection moves between host and new rows.
        let reserved = if on_list && !app.filter().value().is_empty() {
            let text = app.lang().text();
            text.connect
                .width()
                .max(text.new.width())
                .saturating_sub(hints[0].1.width())
        } else {
            0
        };
        let mut total = hints
            .iter()
            .enumerate()
            .map(|(index, hint)| pair_width(index, hint))
            .sum::<usize>()
            + last * gap
            + reserved;
        for index in (1..last).rev() {
            if total <= available {
                break;
            }
            if index != focused {
                total -= pair_width(index, &hints.remove(index)) + gap;
                focused -= usize::from(index < focused);
            }
        }
    }

    for (index, hint) in hints.iter().enumerate() {
        let width = pair_width(index, hint);
        if width > usize::from(area.width) {
            if index != 0 && index == focused {
                continue;
            }
            break;
        }
        let width = width as u16;
        let &(key, description, code) = hint;
        let enabled = index != 0 || enabled;
        let t = code
            .filter(|_| enabled)
            .map_or(0.0, |code| intensity(hover, HoverKey::Key(code)));
        let key_style = if code == Some(KeyCode::F(3)) && app.picker_unavailable_reason().is_some()
        {
            theme.muted
        } else {
            theme.text
        };
        let (button_style, key_style, description_style) =
            theme.help_styles(index == focused, enabled, key_style, t);
        let rect = left(area, width);
        let (label, key_width) = if index == 0 {
            (format!(" {key}  {description}"), key.width() as u16 + 2)
        } else {
            (format!("{key} {description}"), key.width() as u16)
        };
        let leading = u16::from(index != 0);
        let trailing = u16::from(area.width > width);
        let button = Rect::new(
            rect.x - leading,
            rect.y,
            width + leading + trailing,
            rect.height,
        );
        frame.render_widget(Line::raw(label), rect);
        let buffer = frame.buffer_mut();
        // Apply styles to display cells, including continuation cells of wide glyphs.
        buffer.set_style(
            button,
            Style {
                bg: button_style.bg.or(theme.base.bg),
                ..button_style
            },
        );
        buffer.set_style(left(rect, key_width), key_style);
        buffer.set_style(right_of(rect, key_width + 1), description_style);
        if let Some(code) = code {
            hits.add(button, ClickTarget::Key(code));
        }
        area = right_of(area, width.saturating_add(GAP));
    }
}
fn styled_wrapped(text: &str, width: u16, max_rows: u16, style: Style) -> Vec<Line<'static>> {
    wrapped_lines(text, width, max_rows)
        .into_iter()
        .map(|mut line| {
            for span in &mut line.spans {
                span.style = style;
            }
            line
        })
        .collect()
}

/// Wraps values in their own column and writes the label on the first row.
fn labeled_lines(
    label: &str,
    value: &str,
    width: u16,
    label_width: u16,
    max_rows: u16,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let mut lines = wrapped_lines(value, width.saturating_sub(label_width), max_rows);
    if lines.is_empty() && width > 0 && max_rows > 0 {
        lines.push(Line::default());
    }
    for (index, line) in lines.iter_mut().enumerate() {
        let label = if index == 0 { label } else { "" };
        line.spans.insert(
            0,
            Span::styled(padded_cells(label, label_width), theme.muted),
        );
    }
    lines
}

fn wrapped_lines(text: &str, width: u16, max_rows: u16) -> Vec<Line<'static>> {
    let width = usize::from(width);
    let max_rows = usize::from(max_rows);
    let mut lines = Vec::new();
    if width == 0 || max_rows == 0 {
        return lines;
    }

    for logical_line in text.lines() {
        let first_row = lines.len();
        let mut line = String::new();
        let mut cells = 0;
        let mut word_break = None;
        'graphemes: for grapheme in Span::raw(logical_line).styled_graphemes(PLAIN) {
            let symbol = grapheme.symbol;
            let symbol_width = symbol.width();
            if symbol_width > width {
                continue;
            }
            let whitespace = grapheme.is_whitespace();
            while cells + symbol_width > width {
                // Move the partial word to the next row when there is a word boundary.
                let (offset, prefix_width) = word_break
                    .take()
                    .filter(|_| !whitespace)
                    .unwrap_or((line.len(), cells));
                let tail = line.split_off(offset);
                lines.push(Line::from(std::mem::replace(&mut line, tail)));
                cells -= prefix_width;
                if lines.len() == max_rows {
                    return lines;
                }
                if whitespace {
                    continue 'graphemes;
                }
            }
            line.push_str(symbol);
            cells += symbol_width;
            if whitespace {
                word_break = Some((line.len(), cells));
            }
        }
        if !line.is_empty() || lines.len() == first_row {
            lines.push(Line::from(line));
        }
        if lines.len() == max_rows {
            break;
        }
    }
    lines
}

fn input_window(input: &TextInput, width: u16, focused: bool) -> (&str, u16, usize) {
    let value = input.value();
    if !focused || width == 0 {
        return (value, 0, 0);
    }

    let width = usize::from(width);
    let cursor = input.cursor_column();
    let mut column = cursor;
    let mut start = 0;
    // Drop whole graphemes so horizontal scrolling never exposes a partial character.
    for grapheme in Span::raw(value).styled_graphemes(PLAIN) {
        let cells = grapheme.symbol.width();
        if column < width || cells > column {
            break;
        }
        column -= cells;
        start += grapheme.symbol.len();
    }
    (
        &value[start..],
        column.min(width - 1) as u16,
        cursor - column,
    )
}

fn command_lines(command: &str, width: u16, max_rows: u16, theme: &Theme) -> Vec<Line<'static>> {
    let mut lines = styled_wrapped(command, width.saturating_sub(2), max_rows, theme.accent);
    for (index, line) in lines.iter_mut().enumerate() {
        line.spans.insert(
            0,
            Span::styled(if index == 0 { "$ " } else { "  " }, theme.muted),
        );
    }
    lines
}

#[cfg(test)]
/// Test fixtures and buffer assertions for the renderer and event dispatcher.
pub(crate) mod tests;
