//! Rendering with ratatui.

mod buttons;
mod form;
mod layout;
mod list;
mod overlays;
mod theme;

use ratatui::Frame;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Backend, Mode, TextInput};

use self::layout::{below, render_lines, row};

type KeyHint = (&'static str, &'static str);

/// Draws the whole screen for the current application state. No I/O.
pub fn render<B: Backend>(frame: &mut Frame, app: &App<B>) {
    let area = frame.area();
    if area.is_empty() {
        return;
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
            list::render(frame, app, body, true);
            (list::title(app), help_line(list::help(app)))
        }
        Mode::Form(state) => {
            form::render(frame, app, state, body, true);
            (form::title(state), help_line(form::help(state)))
        }
        Mode::Confirm(state) => {
            overlays::confirm(frame, state, app.backend().paths(), body);
            (
                state.title(),
                help_line([
                    ("y/Enter", "confirm"),
                    ("n/Esc", "cancel"),
                    ("Up/Down", "scroll"),
                ]),
            )
        }
        Mode::KeyPicker(state) => {
            let form = state.form();
            form::render(frame, app, form, body, false);
            overlays::key_picker(frame, state, body);
            (
                form::title(form),
                help_line([("Enter", "select"), ("Esc", "cancel")]),
            )
        }
        Mode::Notice(state) => {
            list::render(frame, app, body, false);
            overlays::notice(frame, state, body);
            let help: &[_] = if state.connects() {
                &[("Enter", "connect anyway"), ("Esc", "back to list")]
            } else {
                &[("Enter/Esc", "back to list")]
            };
            (list::title(app), help_line(help.iter().copied()))
        }
        Mode::Update(state) => {
            list::render(frame, app, body, false);
            overlays::update(frame, state, body);
            let help: &[_] = if state.can_run() {
                &[("Enter", "update and exit"), ("Esc", "cancel")]
            } else {
                &[("Esc", "close")]
            };
            (list::title(app), help_line(help.iter().copied()))
        }
    };

    let update_notice = if matches!(app.mode(), Mode::List) {
        app.update_plan().map(|plan| plan.notice())
    } else {
        None
    };
    frame.render_widget(header(title, update_notice, area.width), row(area, 0));
    if let Some(lines) = status {
        let status_area = below(area, body.bottom().saturating_sub(area.y));
        render_lines(frame, status_area, lines);
    }
    if area.height >= 2 {
        frame.render_widget(help, row(area, area.height.saturating_sub(1)));
    }
}

fn header(title: String, notice: Option<String>, width: u16) -> Line<'static> {
    let badge = if width < 8 { "sshtui" } else { " sshtui " };
    let mut line = Line::from(vec![
        Span::styled(badge, theme::SELECTED),
        Span::raw(" "),
        Span::raw(title),
    ]);
    if let Some(notice) = notice {
        let notice = Span::styled(notice, theme::UPDATE_NOTICE);
        if let Some(gap) = usize::from(width).checked_sub(line.width() + notice.width())
            && gap > 0
        {
            line.push_span(Span::raw(" ".repeat(gap)));
            line.push_span(notice);
        }
    }
    line
}

fn help_line(hints: impl IntoIterator<Item = KeyHint>) -> Line<'static> {
    let mut line = Line::default();
    for (index, (key, description)) in hints.into_iter().enumerate() {
        if index > 0 {
            line.push_span(Span::raw("  "));
        }
        line.push_span(Span::styled(key, theme::EMPHASIS));
        line.push_span(Span::raw(" "));
        line.push_span(Span::styled(description, theme::MUTED));
    }
    line
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

fn input_window(input: &TextInput, width: u16, focused: bool) -> (&str, u16) {
    let value = input.value();
    if !focused || width == 0 {
        return (value, 0);
    }

    let width = usize::from(width);
    let mut column = input.cursor_column();
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
    (&value[start..], column.min(width - 1) as u16)
}

#[cfg(test)]
/// Test fixtures and buffer assertions for the renderer and event dispatcher.
pub(crate) mod tests;
