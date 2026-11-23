use crossterm::event::KeyCode;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Backend, ClickTarget, HostRow, ListAction};

use super::layout::{
    below, column_width, left, put, render_lines, right_of, row, rule, visible_rows,
};
use super::theme::{PLAIN, ROW, bold};
use super::{
    HitMap, HoverFrame, HoverKey, KeyHints, Theme, command_lines, headline, input_window,
    intensity, labeled_lines, styled_wrapped,
};

pub(super) fn help<B: Backend>(app: &App<B>) -> KeyHints {
    let text = app.lang().text();
    if app.is_filtering() {
        return (
            true,
            vec![
                ("enter", text.apply_filter, Some(KeyCode::Enter)),
                ("↑↓", text.select, None),
                ("esc", text.clear_filter, Some(KeyCode::Esc)),
            ],
        );
    }
    let new = app.new_row_selected();
    let enter = if new { text.new } else { text.connect };
    let escape = if app.filter().value().is_empty() {
        text.quit
    } else {
        text.clear_filter
    };
    (
        new || app.selection().is_some(),
        ListAction::ALL
            .into_iter()
            .map(|action| {
                let (key, description) = match action {
                    ListAction::Open => ("enter", enter),
                    ListAction::Edit => ("e", text.edit),
                    ListAction::Delete => ("d", text.delete),
                    ListAction::Forget => ("x", text.forget_known_host),
                    ListAction::Filter => ("/", text.filter),
                    ListAction::Reload => ("r", text.reload),
                    ListAction::Leave => ("esc", escape),
                };
                (key, description, Some(action.shortcut()))
            })
            .collect(),
    )
}

pub(super) fn header<B: Backend>(
    frame: &mut Frame,
    app: &App<B>,
    area: Rect,
    theme: &Theme,
    hits: &mut HitMap,
    hover: HoverFrame<'_>,
) {
    let lang = app.lang();
    let matched = (app.is_filtering() || !app.filter().value().is_empty())
        .then(|| app.filtered_hosts().len());
    let title = lang.hosts(app.host_count(), matched);
    let line = headline(title, theme.muted, theme);
    let min_offset = line.width() + 2;
    let mut end = area.width;
    if app.update_plan().is_some() {
        let t = intensity(hover, HoverKey::UpdateNotice);
        let notice = Line::from(vec![
            Span::styled(lang.text().update_available, theme.warn),
            Span::styled("u", bold(theme.warn)),
        ])
        .style(theme.hover_text(theme.hover_background(PLAIN, ROW, t), t));
        let width = notice.width() as u16;
        let offset = area.width.saturating_sub(width);
        if usize::from(offset) >= min_offset {
            let notice_area = right_of(area, offset);
            frame.render_widget(notice, notice_area);
            hits.add(notice_area, ClickTarget::UpdateNotice);
        }
        end = offset.saturating_sub(3);
    }
    let count = app.suggestions().len();
    if count > 0 {
        let label = format!("{} ", lang.suggestions(count));
        let key = KeyCode::Char('i');
        let t = intensity(hover, HoverKey::Key(key));
        let notice = Line::from(vec![
            Span::styled(label, theme.accent),
            Span::styled("i", bold(theme.accent)),
        ])
        .style(theme.hover_text(theme.hover_background(PLAIN, ROW, t), t));
        let width = notice.width() as u16;
        let offset = end.saturating_sub(width);
        if usize::from(offset) >= min_offset {
            let notice_area = left(right_of(area, offset), width);
            frame.render_widget(notice, notice_area);
            hits.add(notice_area, ClickTarget::Key(key));
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
            put(
                frame,
                row(detail, 0),
                app.lang().text().new_connection,
                bold(theme.text),
            );
            let description = below(detail, 1);
            render_lines(
                frame,
                description,
                styled_wrapped(
                    app.lang().text().new_description,
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
        put(frame, input, app.lang().text().filter_hosts, theme.muted);
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
    let text = app.lang().text();
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
            put(
                frame,
                row_columns(rect, name_width).0,
                text.new_row,
                theme.hover_text(theme.selected(bold(theme.accent), selected), t),
            );
            hits.add(rect, ClickTarget::NewRow);
        } else if let Some(host) = hosts.next() {
            let t = intensity(hover, HoverKey::HostRow(index));
            render_row_selection(frame, rect, selected, t, theme);
            render_host(
                frame,
                row_columns(rect, name_width),
                host,
                (selected, t),
                text.read_only,
                &filter,
                theme,
            );
            hits.add(rect, ClickTarget::HostRow(index));
        } else {
            put(frame, rect, text.no_matching_hosts, theme.muted);
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
    (names, summary): (Rect, Rect),
    host: HostRow<'_>,
    (selected, t): (bool, f32),
    readonly: &str,
    filter: &str,
    theme: &Theme,
) {
    let width = readonly.width() as u16;
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
    let lang = app.lang();
    let text = lang.text();
    let label_width = column_width(
        11,
        [
            text.user,
            text.address,
            text.port,
            text.key_file,
            text.source,
        ],
    );
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
                (text.user, config.user.as_deref().unwrap_or("-")),
                (text.address, address),
                (text.port, config.port.as_deref().unwrap_or("-")),
            ] {
                content = detail_value(frame, content, label_width, label, value, theme);
            }
            if config.identity_files.is_empty() {
                content = detail_value(frame, content, label_width, text.key_file, "-", theme);
            }
            for (index, file) in config.identity_files.iter().enumerate() {
                if content.is_empty() {
                    break;
                }
                let label = if index == 0 { text.key_file } else { "" };
                let file = app
                    .backend()
                    .paths()
                    .abbreviate_home(std::path::Path::new(file));
                content = detail_value(frame, content, label_width, label, &file, theme);
            }
        }
        Some(Err(error)) => {
            let lines = styled_wrapped(
                &lang.effective_settings(error),
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
    content = detail_value(frame, content, label_width, text.source, &source, theme);
    if host.read_only {
        put(frame, row(content, 0), text.read_only_file, theme.warn);
        content = below(content, 1);
    }
    if !app.is_filtering()
        && !theme.dim
        && let Some(command) = app.selected_command()
    {
        put(frame, row(content, 1), text.enter_runs, theme.muted);
        let preview = below(content, 2);
        render_lines(
            frame,
            preview,
            command_lines(&command, preview.width, preview.height.min(3), theme),
        );
    }
}

fn detail_value(
    frame: &mut Frame,
    area: Rect,
    label_width: u16,
    label: &str,
    value: &str,
    theme: &Theme,
) -> Rect {
    render_lines(
        frame,
        area,
        labeled_lines(label, value, area.width, label_width, area.height, theme),
    )
}
