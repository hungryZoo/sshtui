use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Backend, HostRow};

use super::layout::{
    below, draw_panel, inset_horizontal, left, panel_title, render_lines, right_of, row,
    visible_rows,
};
use super::{KeyHint, input_window, labeled_lines, styled_wrapped, theme};

/// Returns the list key hints for this session.
pub(super) fn help<B: Backend>(app: &App<B>) -> impl Iterator<Item = KeyHint> {
    [
        ("Enter", "connect"),
        ("n", "new"),
        ("e", "edit"),
        ("d", "delete"),
        ("/", "filter"),
        ("r", "reload"),
    ]
    .into_iter()
    .chain(app.update_plan().map(|_| ("u", "update")))
    .chain([("q", "quit")])
}

/// Returns the counts displayed in the list header.
pub(super) fn title<B: Backend>(app: &App<B>) -> String {
    let total = app.host_count();
    if app.is_filtering() || !app.filter().value().is_empty() {
        format!("{} of {total} hosts", app.filtered_hosts().len())
    } else {
        format!("{total} hosts")
    }
}

/// Draws the host list and the details of the selected host.
pub(super) fn render<B: Backend>(frame: &mut Frame, app: &App<B>, body: Rect, show_cursor: bool) {
    if body.is_empty() {
        return;
    }
    let names_width = if body.width < 50 {
        body.width
    } else {
        let longest = app
            .filtered_hosts()
            .map(|host| host.entry.name.width() + if host.read_only { 6 } else { 4 })
            .max()
            .unwrap_or(0);
        (longest.clamp(24, 40) as u16).min(body.width / 2)
    };
    let selection = app.selection();
    let names = draw_panel(
        frame,
        left(body, names_width),
        Some(panel_title("Hosts")),
        true,
    );
    render_hosts(frame, app, names, show_cursor);

    let detail = right_of(body, names_width);
    if !detail.is_empty() {
        let host = selection.and_then(|index| app.filtered_hosts().nth(index));
        let title = host.map(|host| panel_title(&host.entry.name));
        let detail = draw_panel(frame, detail, title, false);
        if let Some(host) = host {
            render_detail(frame, app, host, inset_horizontal(detail));
        }
    }
}

fn render_hosts<B: Backend>(frame: &mut Frame, app: &App<B>, mut area: Rect, show_cursor: bool) {
    let filter = app.filter();
    if area.is_empty() {
        return;
    }
    if app.is_filtering() || !filter.value().is_empty() {
        let (value, column) =
            input_window(filter, area.width.saturating_sub(1), app.is_filtering());
        frame.render_widget(
            Line::from(vec![Span::styled("/", theme::ACCENT), Span::raw(value)]),
            row(area, 0),
        );
        if show_cursor && app.is_filtering() {
            let column = column.saturating_add(1).min(area.width.saturating_sub(1));
            frame.set_cursor_position((area.x.saturating_add(column), area.y));
        }
        area = below(area, 1);
    }
    if area.is_empty() {
        return;
    }
    let hosts = app.filtered_hosts();
    let count = hosts.len();
    if count == 0 {
        let line = Line::from(Span::styled("No matching hosts", theme::MUTED)).centered();
        frame.render_widget(line, row(area, area.height.saturating_sub(1) / 2));
        return;
    }
    let selection = app.selection();
    let rows = visible_rows(area, count, selection);
    let mut rows = rows.peekable();
    if let Some(&(first, _)) = rows.peek() {
        for ((index, area), host) in rows.zip(hosts.skip(first)) {
            render_host(frame, area, host, selection == Some(index));
        }
    }
}

fn render_host(frame: &mut Frame, area: Rect, host: HostRow<'_>, selected: bool) {
    let marker_width = if host.read_only && area.width >= 4 {
        2
    } else {
        0
    };
    let gap = u16::from(marker_width > 0);
    let indent = u16::from(area.width > 1);
    let name = left(
        right_of(area, indent),
        area.width.saturating_sub(indent + gap + marker_width),
    );
    frame.render_widget(Span::raw(host.entry.name.as_str()), name);
    if selected {
        frame.buffer_mut().set_style(area, theme::SELECTED);
    }
    if marker_width > 0 {
        frame.render_widget(
            Span::styled("ro", theme::WARNING),
            right_of(area, area.width.saturating_sub(marker_width)),
        );
    }
}

fn render_detail<B: Backend>(frame: &mut Frame, app: &App<B>, host: HostRow<'_>, area: Rect) {
    let mut lines = Vec::new();
    match app.selected_effective() {
        Some(Ok(config)) => {
            for (label, value) in [
                ("User", config.user.as_deref().unwrap_or("-")),
                ("Address", config.host_name.as_deref().unwrap_or("-")),
                ("Port", config.port.as_deref().unwrap_or("-")),
            ] {
                lines.extend(labeled_lines(label, value, area.width, 10, area.height));
            }
            if config.identity_files.is_empty() {
                lines.extend(labeled_lines("Key file", "-", area.width, 10, area.height));
            }
            for (index, key) in config.identity_files.iter().enumerate() {
                let label = if index == 0 { "Key file" } else { "" };
                lines.extend(labeled_lines(label, key, area.width, 10, area.height));
            }
        }
        Some(Err(error)) => lines.extend(styled_wrapped(
            &format!("Effective settings: {error}"),
            area.width,
            area.height,
            theme::ERROR,
        )),
        None => {}
    }
    let source = app.backend().paths().abbreviate_home(&host.entry.source);
    lines.push(Line::default());
    lines.extend(labeled_lines(
        "Source",
        &format!("{source}:{}", host.entry.line_number),
        area.width,
        10,
        area.height,
    ));
    if host.read_only {
        lines.push(Line::from(Span::styled("Read-only file", theme::WARNING)));
    }
    render_lines(frame, area, lines);
}
