use crossterm::event::KeyCode;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Backend, ClickTarget, HostRow};

use super::layout::{below, left, put, render_lines, right_of, row, rule, visible_rows};
use super::theme::{PLAIN, ROW, bold};
use super::{
    HitMap, HoverFrame, HoverKey, KeyHint, KeyHints, Theme, command_lines, headline, input_window,
    intensity, labeled_lines, styled_wrapped,
};

const fn list_hints(enter: &'static str, escape: &'static str) -> [KeyHint; 7] {
    [
        ("enter", enter, Some(KeyCode::Enter)),
        ("e", "edit", Some(KeyCode::Char('e'))),
        ("d", "delete", Some(KeyCode::Char('d'))),
        ("x", "forget(known_host)", Some(KeyCode::Char('x'))),
        ("/", "filter", Some(KeyCode::Char('/'))),
        ("r", "reload", Some(KeyCode::Char('r'))),
        ("esc", escape, Some(KeyCode::Esc)),
    ]
}

pub(super) fn help<B: Backend>(app: &App<B>) -> KeyHints {
    const CONNECT: &[KeyHint] = &list_hints("connect", "quit");
    const CONNECT_FILTERED: &[KeyHint] = &list_hints("connect", "clear filter");
    const NEW: &[KeyHint] = &list_hints("new", "quit");
    const NEW_FILTERED: &[KeyHint] = &list_hints("new", "clear filter");

    if app.is_filtering() {
        (
            true,
            &[
                ("enter", "apply filter", Some(KeyCode::Enter)),
                ("↑↓", "select", None),
                ("esc", "clear filter", Some(KeyCode::Esc)),
            ],
        )
    } else {
        let new = app.new_row_selected();
        (
            new || app.selection().is_some(),
            match (new, app.filter().value().is_empty()) {
                (true, true) => NEW,
                (true, false) => NEW_FILTERED,
                (false, true) => CONNECT,
                (false, false) => CONNECT_FILTERED,
            },
        )
    }
}

pub(super) fn header<B: Backend>(
    frame: &mut Frame,
    app: &App<B>,
    area: Rect,
    theme: &Theme,
    hits: &mut HitMap,
    hover: HoverFrame<'_>,
) {
    let total = app.host_count();
    let title = if app.is_filtering() || !app.filter().value().is_empty() {
        format!("{} of {total} hosts", app.filtered_hosts().len())
    } else {
        format!("{total} hosts")
    };
    let line = headline(title, theme.muted, theme);
    if app.update_plan().is_some() {
        let t = intensity(hover, HoverKey::UpdateNotice);
        let notice = Line::from(vec![
            Span::styled("update available ", theme.warn),
            Span::styled("u", bold(theme.warn)),
        ])
        .style(theme.hover_text(theme.hover_background(PLAIN, ROW, t), t));
        let width = notice.width() as u16;
        let offset = area.width.saturating_sub(width);
        if usize::from(offset) >= line.width() + 2 {
            let notice_area = right_of(area, offset);
            frame.render_widget(notice, notice_area);
            hits.add(notice_area, ClickTarget::UpdateNotice);
        }
    }
    frame.render_widget(line, area);
}

pub(super) fn render<B: Backend>(
    frame: &mut Frame,
    app: &App<B>,
    body: Rect,
    theme: &Theme,
    hits: &mut HitMap,
    hover: HoverFrame<'_>,
) {
    // A compact screen drops the empty row and rule before it loses the list.
    let filter_row = u16::from(body.height >= 4);
    let show_rule = body.height >= 4;
    render_filter(frame, app, row(body, filter_row), theme, hits, hover);
    if show_rule {
        rule(frame, row(body, 2), theme.rule);
    }

    let content = below(body, filter_row + 1 + u16::from(show_rule));
    let split = frame.area().width >= 64;
    let width = if split {
        ((u32::from(content.width) * 54 + 50) / 100).min(52) as u16
    } else {
        content.width
    };
    render_hosts(frame, app, left(content, width), theme, hits, hover);
    if split {
        let divider = left(right_of(content, width), 1);
        for rect in divider.rows() {
            put(frame, rect, "│", theme.rule);
        }
        let detail = right_of(content, width + 3);
        if let Some(host) = app
            .selection()
            .and_then(|index| app.filtered_hosts().nth(index))
        {
            render_detail(frame, app, host, detail, theme);
        } else if app.new_row_selected() {
            put(frame, row(detail, 0), "New connection", bold(theme.text));
            let description = below(detail, 1);
            render_lines(
                frame,
                description,
                styled_wrapped(
                    "Add a host or connect once without saving.",
                    description.width,
                    description.height,
                    theme.muted,
                ),
            );
        }
    }
}

fn render_filter<B: Backend>(
    frame: &mut Frame,
    app: &App<B>,
    area: Rect,
    theme: &Theme,
    hits: &mut HitMap,
    hover: HoverFrame<'_>,
) {
    if area.is_empty() {
        return;
    }
    let filtering = app.is_filtering();
    let t = intensity(hover, HoverKey::Key(KeyCode::Char('/')));
    frame
        .buffer_mut()
        .set_style(area, theme.hover_input(filtering, t));
    put(
        frame,
        left(right_of(area, 1), 1),
        "/",
        if filtering {
            bold(theme.accent)
        } else {
            theme.muted
        },
    );
    let input = right_of(area, 3);
    let (value, column, _) = input_window(app.filter(), input.width, filtering);
    if !filtering && value.is_empty() {
        put(frame, input, "filter hosts", theme.muted);
    } else {
        put(frame, input, value, theme.text);
    }
    if filtering && !theme.dim && !input.is_empty() {
        frame.set_cursor_position((input.x.saturating_add(column), input.y));
    }
    if !filtering {
        hits.add(area, ClickTarget::Key(KeyCode::Char('/')));
    }
}

fn render_hosts<B: Backend>(
    frame: &mut Frame,
    app: &App<B>,
    area: Rect,
    theme: &Theme,
    hits: &mut HitMap,
    hover: HoverFrame<'_>,
) {
    let hosts = app.filtered_hosts();
    let new_index = hosts.len().max(1);
    let name_width = app
        .filtered_hosts()
        .map(|host| host.entry.name.width().saturating_add(2))
        .max()
        .unwrap_or(0)
        .clamp(16, 28) as u16;
    let selection = app
        .selection()
        .or_else(|| app.new_row_selected().then_some(new_index));
    let mut rows = visible_rows(area, new_index + 1, selection).peekable();
    let Some(&(first, _)) = rows.peek() else {
        return;
    };
    let filter = app.filter().value().to_lowercase();
    let mut hosts = hosts.skip(first);
    for (index, rect) in rows {
        let selected = selection == Some(index);
        if index == new_index {
            let t = intensity(hover, HoverKey::NewRow);
            render_row_selection(frame, rect, selected, t, theme);
            let label = "+ New connection";
            let (name, summary) = row_columns(rect, name_width);
            put(
                frame,
                name,
                label,
                theme.hover_text(theme.selected(bold(theme.accent), selected), t),
            );
            let padding = u16::from(label.width() == usize::from(name.width) && summary.width > 1);
            put(
                frame,
                right_of(summary, padding),
                "n",
                theme.selected(PLAIN, selected).patch(theme.hover_muted(t)),
            );
            hits.add(rect, ClickTarget::NewRow);
        } else if let Some(host) = hosts.next() {
            let t = intensity(hover, HoverKey::HostRow(index));
            render_row_selection(frame, rect, selected, t, theme);
            render_host(frame, rect, host, (selected, t), name_width, &filter, theme);
            hits.add(rect, ClickTarget::HostRow(index));
        } else {
            put(frame, rect, "No matching hosts", theme.muted);
        }
    }
}

fn render_row_selection(frame: &mut Frame, area: Rect, selected: bool, t: f32, theme: &Theme) {
    frame
        .buffer_mut()
        .set_style(area, theme.hover_row(selected, t));
    if selected {
        put(
            frame,
            left(area, 1),
            "›",
            theme.selected(bold(theme.accent), true),
        );
    }
}

fn row_columns(area: Rect, name_width: u16) -> (Rect, Rect) {
    let content = right_of(left(area, area.width.saturating_sub(1)), 2);
    (left(content, name_width), right_of(content, name_width))
}

fn render_host(
    frame: &mut Frame,
    area: Rect,
    host: HostRow<'_>,
    (selected, t): (bool, f32),
    name_width: u16,
    filter: &str,
    theme: &Theme,
) {
    let (names, summary) = row_columns(area, name_width);
    let readonly = "read-only";
    let width = readonly.len() as u16;
    let marker_width = if host.read_only && summary.width >= width {
        width
    } else {
        0
    };

    let regular = theme.hover_text(
        theme.selected(
            if selected {
                bold(theme.text)
            } else {
                theme.text
            },
            selected,
        ),
        t,
    );
    let name = host.entry.name.as_str();
    let line = match match_range(name, filter) {
        Some((start, end)) => Line::from(vec![
            Span::styled(&name[..start], regular),
            Span::styled(
                &name[start..end],
                theme.hover_text(theme.selected(bold(theme.accent), selected), t),
            ),
            Span::styled(&name[end..], regular),
        ]),
        None => Line::from(Span::styled(name, regular)),
    };
    frame.render_widget(line, names);
    let summary_width = summary.width.saturating_sub(marker_width);
    put(
        frame,
        left(summary, summary_width),
        host.summary,
        theme.selected(
            if selected {
                theme.text
            } else {
                theme.hover_muted(t)
            },
            selected,
        ),
    );
    if marker_width > 0 {
        put(
            frame,
            right_of(summary, summary_width),
            readonly,
            theme.selected(theme.warn, selected),
        );
    }
}

fn match_range(name: &str, filter: &str) -> Option<(usize, usize)> {
    if filter.is_empty() {
        return None;
    }
    let start = name.to_lowercase().find(filter)?;
    let end = start + filter.len();
    let mut offset = 0;
    let mut first = None;
    let mut source = 0;
    for grapheme in Span::raw(name).styled_graphemes(PLAIN) {
        let symbol = grapheme.symbol;
        let index = source + name[source..].find(symbol)?;
        source = index + symbol.len();
        offset += symbol
            .chars()
            .flat_map(char::to_lowercase)
            .map(char::len_utf8)
            .sum::<usize>();
        if start < offset {
            first.get_or_insert(index);
        }
        if end <= offset {
            return Some((first?, source));
        }
    }
    None
}

fn render_detail<B: Backend>(
    frame: &mut Frame,
    app: &App<B>,
    host: HostRow<'_>,
    area: Rect,
    theme: &Theme,
) {
    if area.is_empty() {
        return;
    }
    put(frame, row(area, 0), &host.entry.name, bold(theme.text));
    let mut content = below(area, 3);
    match app.selected_effective() {
        Some(Ok(config)) => {
            let address = config.host_name.as_deref().unwrap_or(&host.entry.name);
            let destination = config
                .user
                .as_ref()
                .map_or_else(|| address.to_owned(), |user| format!("{user}@{address}"));
            put(frame, row(area, 1), &destination, theme.muted);
            for (label, value) in [
                ("User", config.user.as_deref().unwrap_or("-")),
                ("Address", address),
                ("Port", config.port.as_deref().unwrap_or("-")),
            ] {
                content = detail_value(frame, content, label, value, theme);
            }
            if config.identity_files.is_empty() {
                content = detail_value(frame, content, "Key file", "-", theme);
            }
            for (index, file) in config.identity_files.iter().enumerate() {
                if content.is_empty() {
                    break;
                }
                let label = if index == 0 { "Key file" } else { "" };
                let file = app
                    .backend()
                    .paths()
                    .abbreviate_home(std::path::Path::new(file));
                content = detail_value(frame, content, label, &file, theme);
            }
        }
        Some(Err(error)) => {
            let lines = styled_wrapped(
                &format!("Effective settings: {error}"),
                content.width,
                content.height,
                theme.error,
            );
            content = render_lines(frame, content, lines);
        }
        None => {}
    }

    let path = app.backend().paths().abbreviate_home(&host.entry.source);
    let source = format!("{path}:{}", host.entry.line_number);
    content = detail_value(frame, content, "Source", &source, theme);
    if host.read_only {
        put(frame, row(content, 0), "Read-only file", theme.warn);
        content = below(content, 1);
    }
    if !app.is_filtering()
        && !theme.dim
        && let Some(command) = app.selected_command()
    {
        put(frame, row(content, 1), "enter runs", theme.muted);
        let preview = below(content, 2);
        render_lines(
            frame,
            preview,
            command_lines(&command, preview.width, preview.height.min(3), theme),
        );
    }
}

fn detail_value(frame: &mut Frame, area: Rect, label: &str, value: &str, theme: &Theme) -> Rect {
    render_lines(
        frame,
        area,
        labeled_lines(label, value, area.width, 11, area.height, theme),
    )
}
