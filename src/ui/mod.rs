//! Rendering with ratatui.

mod form;
mod list;
mod overlays;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListState};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Backend, Mode, StatusLevel, TextInput};

/// Draws the whole screen for the current application state. No I/O.
pub fn render<B: Backend>(frame: &mut Frame, app: &App<B>) {
    let area = frame.area();
    if area.is_empty() {
        return;
    }
    let body = Rect::new(
        area.x,
        area.y.saturating_add(1),
        area.width,
        area.height.saturating_sub(3),
    );

    let (title, help) = match app.mode() {
        Mode::List => {
            list::render(frame, app, body, true);
            (list::title(app), list::HELP)
        }
        Mode::Form(state) => {
            form::render(frame, app, state, body, true);
            (form::title(state), form::help(state))
        }
        Mode::Confirm(state) => {
            overlays::confirm(frame, state, body);
            (
                state.title(),
                "y/Enter confirm  n/Esc cancel  Up/Down scroll",
            )
        }
        Mode::KeyPicker(state) => {
            let form = state.form();
            form::render(frame, app, form, body, false);
            overlays::key_picker(frame, state, body);
            (form::title(form), "Enter select  Esc cancel")
        }
        Mode::Notice(state) => {
            list::render(frame, app, body, false);
            overlays::notice(frame, state, body);
            let help = if state.connects() {
                "Enter connect anyway  Esc back to list"
            } else {
                "Enter/Esc back to list"
            };
            (list::title(app), help)
        }
    };

    frame.render_widget(Line::from(title).bold(), row(area, 0));
    if area.height >= 3
        && let Some(status) = app.status()
    {
        let style = match status.level {
            StatusLevel::Info => Style::default(),
            StatusLevel::Warning => Style::default().fg(Color::Yellow),
            StatusLevel::Error => Style::default().fg(Color::Red),
        };
        frame.render_widget(
            Line::from(status.text.as_str()).style(style),
            row(area, area.height - 2),
        );
    }
    if area.height >= 2 {
        frame.render_widget(Line::from(help).dim(), row(area, area.height - 1));
    }
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
