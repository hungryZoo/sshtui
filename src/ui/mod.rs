//! Rendering with ratatui.

mod form;
mod list;
mod overlays;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListState, Paragraph};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Backend, Mode, StatusLevel, TextInput};

/// Draws the whole screen for the current application state. No I/O.
pub fn render<B: Backend>(frame: &mut Frame, app: &App<B>) {
    let area = frame.area();
    if area.is_empty() {
        return;
    }
    let mut body = below(area, 1);
    body.height = body.height.saturating_sub(1);
    let status = app.status().map(|status| {
        let lines = wrapped_lines(&status.text, area.width, body.height.min(3));
        let height = lines.len() as u16;
        body.height -= height;
        let style = match status.level {
            StatusLevel::Info => Style::default(),
            StatusLevel::Warning => Style::default().fg(Color::Yellow),
            StatusLevel::Error => Style::default().fg(Color::Red),
        };
        let status_area = Rect::new(body.x, body.bottom(), body.width, height);
        (Paragraph::new(lines).style(style), status_area)
    });

    let (title, help) = match app.mode() {
        Mode::List => {
            list::render(frame, app, body, true);
            (list::title_line(app, area.width), list::help(app))
        }
        Mode::Form(state) => {
            form::render(frame, app, state, body, true);
            (Line::from(form::title(state)).bold(), form::help(state))
        }
        Mode::Confirm(state) => {
            overlays::confirm(frame, state, app.backend().paths(), body);
            (
                Line::from(state.title()).bold(),
                "y/Enter confirm  n/Esc cancel  Up/Down scroll",
            )
        }
        Mode::KeyPicker(state) => {
            let form = state.form();
            form::render(frame, app, form, body, false);
            overlays::key_picker(frame, state, body);
            (
                Line::from(form::title(form)).bold(),
                "Enter select  Esc cancel",
            )
        }
        Mode::Notice(state) => {
            list::render(frame, app, body, false);
            overlays::notice(frame, state, body);
            let help = if state.connects() {
                "Enter connect anyway  Esc back to list"
            } else {
                "Enter/Esc back to list"
            };
            (Line::from(list::title(app)).bold(), help)
        }
        Mode::Update(state) => {
            list::render(frame, app, body, false);
            overlays::update(frame, state, body);
            let help = if state.can_run() {
                "Enter update and exit  Esc cancel"
            } else {
                "Esc close"
            };
            (Line::from(list::title(app)).bold(), help)
        }
    };

    frame.render_widget(title, row(area, 0));
    if let Some((status, status_area)) = status {
        frame.render_widget(status, status_area);
    }
    if area.height >= 2 {
        frame.render_widget(Line::from(help).dim(), row(area, area.height - 1));
    }
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
        'graphemes: for grapheme in Span::raw(logical_line).styled_graphemes(Style::default()) {
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

fn row(area: Rect, offset: u16) -> Rect {
    Rect::new(
        area.x,
        area.y.saturating_add(offset),
        area.width,
        u16::from(offset < area.height),
    )
}

fn below(area: Rect, rows: u16) -> Rect {
    Rect::new(
        area.x,
        area.y.saturating_add(rows.min(area.height)),
        area.width,
        area.height.saturating_sub(rows),
    )
}

fn selectable_list<'a>(
    frame: &mut Frame,
    area: Rect,
    items: impl IntoIterator<Item = Line<'a>>,
    selection: Option<usize>,
) {
    let list = List::new(items).highlight_style(Style::default().reversed());
    frame.render_stateful_widget(
        list,
        area,
        &mut ListState::default().with_selected(selection),
    );
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
    for grapheme in Span::raw(value).styled_graphemes(Style::default()) {
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
