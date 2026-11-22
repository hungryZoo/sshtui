//! Rendering with ratatui.

mod buttons;
mod form;
mod layout;
mod list;
mod overlays;
mod theme;

use crossterm::event::KeyCode;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Backend, ClickTarget, Mode, TextInput};

use self::layout::{below, left, render_lines, right_of, row};

type KeyHint = (&'static str, &'static str, Option<KeyCode>);

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
pub fn render<B: Backend>(frame: &mut Frame, app: &App<B>) -> HitMap {
    let area = frame.area();
    let mut hits = HitMap::default();
    if area.is_empty() {
        return hits;
    }
    let mut body = below(area, 1);
    body.height = body.height.saturating_sub(1);
    let status = app.status().map(|status| {
        let lines = styled_wrapped(
            &status.text,
            area.width,
            body.height.min(3),
            theme::status(status.level),
        );
        body.height = body.height.saturating_sub(lines.len() as u16);
        lines
    });

    let (title, help) = match app.mode() {
        Mode::List => {
            list::render(frame, app, body, true, &mut hits);
            (list::title(app), list::help(app).collect())
        }
        Mode::Form(state) => {
            form::render(frame, app, state, body, true, &mut hits);
            (form::title(state), form::help(state).to_vec())
        }
        Mode::Confirm(state) => {
            overlays::confirm(frame, state, app.backend().paths(), body, &mut hits);
            (
                state.title(),
                vec![
                    ("y/Enter", "confirm", Some(KeyCode::Char('y'))),
                    ("n/Esc", "cancel", Some(KeyCode::Char('n'))),
                    ("Up/Down", "scroll", None),
                ],
            )
        }
        Mode::KeyPicker(state) => {
            let form = state.form();
            form::render(frame, app, form, body, false, &mut hits);
            overlays::key_picker(frame, state, body, &mut hits);
            (
                form::title(form),
                vec![
                    ("Enter", "select", Some(KeyCode::Enter)),
                    ("Esc", "cancel", Some(KeyCode::Esc)),
                ],
            )
        }
        Mode::Notice(state) => {
            list::render(frame, app, body, false, &mut hits);
            overlays::notice(frame, state, body, &mut hits);
            let help = if state.connects() {
                vec![
                    ("Enter", "connect anyway", Some(KeyCode::Enter)),
                    ("Esc", "back to list", Some(KeyCode::Esc)),
                ]
            } else {
                vec![("Enter/Esc", "back to list", Some(KeyCode::Enter))]
            };
            (list::title(app), help)
        }
        Mode::Update(state) => {
            list::render(frame, app, body, false, &mut hits);
            overlays::update(frame, state, body, &mut hits);
            let help = if state.can_run() {
                vec![
                    ("Enter", "update and exit", Some(KeyCode::Enter)),
                    ("Esc", "cancel", Some(KeyCode::Esc)),
                ]
            } else {
                vec![("Esc", "close", Some(KeyCode::Esc))]
            };
            (list::title(app), help)
        }
    };

    let update_notice = if matches!(app.mode(), Mode::List) {
        app.update_plan().map(|plan| plan.notice())
    } else {
        None
    };
    render_header(frame, title, update_notice, row(area, 0), &mut hits);
    if let Some(lines) = status {
        let status_area = below(area, body.bottom().saturating_sub(area.y));
        render_lines(frame, status_area, lines);
    }
    if area.height >= 2 {
        render_help(frame, help, row(area, area.height - 1), &mut hits);
    }
    hits
}

fn render_header(
    frame: &mut Frame,
    title: String,
    notice: Option<String>,
    area: Rect,
    hits: &mut HitMap,
) {
    let badge = if area.width < 8 { "sshtui" } else { " sshtui " };
    let mut line = Line::from(vec![
        Span::styled(badge, theme::SELECTED),
        Span::raw(" "),
        Span::raw(title),
    ]);
    if let Some(notice) = notice {
        let notice = Span::styled(notice, theme::UPDATE_NOTICE);
        if let Some(gap) = usize::from(area.width).checked_sub(line.width() + notice.width())
            && gap > 0
        {
            hits.add(
                right_of(area, area.width - notice.width() as u16),
                ClickTarget::UpdateNotice,
            );
            line.push_span(Span::raw(" ".repeat(gap)));
            line.push_span(notice);
        }
    }
    frame.render_widget(line, area);
}

fn render_help(frame: &mut Frame, hints: Vec<KeyHint>, area: Rect, hits: &mut HitMap) {
    let mut line = Line::default();
    let mut offset = 0;
    for (index, (key, description, code)) in hints.into_iter().enumerate() {
        if index > 0 {
            line.push_span(Span::raw("  "));
            offset += 2;
        }
        let width = key.width() + 1 + description.width();
        if let Some(code) = code
            && offset + width <= usize::from(area.width)
        {
            hits.add(
                left(right_of(area, offset as u16), width as u16),
                ClickTarget::Key(code),
            );
        }
        line.push_span(Span::styled(key, theme::EMPHASIS));
        line.push_span(Span::raw(" "));
        line.push_span(Span::styled(description, theme::MUTED));
        offset += width;
    }
    frame.render_widget(line, area);
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
) -> Vec<Line<'static>> {
    let mut lines = wrapped_lines(value, width.saturating_sub(label_width), max_rows);
    if lines.is_empty() && width > 0 && max_rows > 0 {
        lines.push(Line::default());
    }
    for (index, line) in lines.iter_mut().enumerate() {
        let label = if index == 0 { label } else { "" };
        let width = usize::from(label_width);
        line.spans.insert(
            0,
            Span::styled(format!("{label:<width$.width$}"), theme::MUTED),
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
        'graphemes: for grapheme in Span::raw(logical_line).styled_graphemes(theme::PLAIN) {
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
    for grapheme in Span::raw(value).styled_graphemes(theme::PLAIN) {
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

#[cfg(test)]
/// Test fixtures and buffer assertions for the renderer and event dispatcher.
pub(crate) mod tests;
