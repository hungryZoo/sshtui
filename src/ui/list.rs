use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::app::{App, Backend, HostRow};

use super::{below, input_window, row, selectable_list};

pub(super) const HELP: &str = "Enter connect  n new  e edit  d delete  / filter  r reload  q quit";

pub(super) fn title<B: Backend>(app: &App<B>) -> String {
    let total = app.host_count();
    if app.is_filtering() || !app.filter().value().is_empty() {
        format!("sshtui · {} of {total} hosts", app.filtered_hosts().len())
    } else {
        format!("sshtui · {total} hosts")
    }
}

pub(super) fn render<B: Backend>(
    frame: &mut Frame,
    app: &App<B>,
    mut body: Rect,
    show_cursor: bool,
) {
    if body.is_empty() {
        return;
    }
    let filter = app.filter();
    if app.is_filtering() || !filter.value().is_empty() {
        let (value, column) =
            input_window(filter, body.width.saturating_sub(1), app.is_filtering());
        frame.render_widget(
            Line::from(vec![Span::raw("/"), Span::raw(value)]),
            row(body, 0),
        );
        if show_cursor && app.is_filtering() {
            let column = column.saturating_add(1).min(body.width - 1);
            frame.set_cursor_position((body.x.saturating_add(column), body.y));
        }
        body = below(body, 1);
    }
    if body.is_empty() {
        return;
    }

    let mut hosts = app.filtered_hosts();
    if hosts.len() == 0 {
        frame.render_widget(Line::from("No matching hosts"), row(body, 0));
        return;
    }
    let [names, detail] = Layout::horizontal([Constraint::Percentage(35), Constraint::Fill(1)])
        .spacing(1)
        .areas(body);
    let selected = app.selection();
    selectable_list(frame, names, app.filtered_hosts().map(host_name), selected);
    if let Some(host) = selected.and_then(|index| hosts.nth(index)) {
        render_detail(frame, app, host, detail);
    }
}

fn host_name(host: HostRow<'_>) -> Line<'_> {
    let mut label = Line::from(host.entry.name.as_str());
    if host.read_only {
        label.push_span(Span::raw(" [ro]").dim());
    }
    label
}

fn render_detail<B: Backend>(frame: &mut Frame, app: &App<B>, host: HostRow<'_>, area: Rect) {
    let mut lines = vec![Line::from(format!("Alias: {}", host.entry.name))];
    match app.selected_effective() {
        Some(Ok(config)) => {
            lines.extend([
                Line::from(format!("User: {}", config.user.as_deref().unwrap_or("-"))),
                Line::from(format!(
                    "Address: {}",
                    config.host_name.as_deref().unwrap_or("-")
                )),
                Line::from(format!("Port: {}", config.port.as_deref().unwrap_or("-"))),
            ]);
            if config.identity_files.is_empty() {
                lines.push(Line::from("Key file: -"));
            }
            lines.extend(
                config
                    .identity_files
                    .iter()
                    .map(|key| Line::from(format!("Key file: {key}"))),
            );
        }
        Some(Err(error)) => lines.push(Line::from(format!("Effective settings: {error}")).red()),
        None => {}
    }
    let source = app.backend().paths().abbreviate_home(&host.entry.source);
    lines.push(Line::from(format!(
        "Source: {source}:{}",
        host.entry.line_number
    )));
    if host.read_only {
        lines.push(Line::from("Read-only file").yellow());
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}
