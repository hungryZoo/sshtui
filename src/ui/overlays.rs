use crossterm::event::KeyCode;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear};
use unicode_width::UnicodeWidthStr;

use crate::app::{
    ClickTarget, ConfirmKind, ConfirmState, KeyPickerState, NoticeState, UpdateState,
};
use crate::config::{DiffHunk, DiffLine, DiffTag, backup_file_name};
use crate::paths::SshPaths;

use super::layout::{
    below, header_line, inset, left, put, put_right, render_lines, right_of, row, rule,
    visible_rows,
};
use super::theme::bold;
use super::{
    HitMap, HoverFrame, HoverKey, KeyHint, Theme, command_lines, intensity, labeled_lines,
    styled_wrapped,
};

pub(super) fn confirm_help(state: &ConfirmState) -> &'static [KeyHint] {
    match state.kind() {
        ConfirmKind::Delete => &[
            ("y", "delete", Some(KeyCode::Char('y'))),
            ("n", "back to list", Some(KeyCode::Char('n'))),
            ("↑↓", "scroll", None),
        ],
        ConfirmKind::Save => &[
            ("y", "save", Some(KeyCode::Char('y'))),
            ("n", "back to form", Some(KeyCode::Char('n'))),
            ("↑↓", "scroll", None),
        ],
        ConfirmKind::Forget => &[
            ("y", "forget", Some(KeyCode::Char('y'))),
            ("n", "back to list", Some(KeyCode::Char('n'))),
            ("↑↓", "scroll", None),
        ],
    }
}

pub(super) fn confirm(
    frame: &mut Frame,
    state: &ConfirmState,
    paths: &SshPaths,
    body: Rect,
    theme: &Theme,
) {
    rule(frame, row(body, 0), theme.rule);
    let mut added = 0;
    let mut removed = 0;
    for change in state.hunks().iter().flat_map(|hunk| &hunk.lines) {
        match change.tag {
            DiffTag::Added => added += 1,
            DiffTag::Removed => removed += 1,
            DiffTag::Context => {}
        }
    }
    let count = added.max(removed);
    let summary_text = match state.forget_name() {
        Some(name) if removed == 1 => format!("1 entry for {name} will be removed"),
        Some(name) => format!("{removed} entries for {name} will be removed"),
        None if count == 1 => "1 line will change".into(),
        None => format!("{count} lines will change"),
    };
    let mut summary = Line::from(Span::styled(summary_text, theme.text));
    if removed > 0 {
        summary.push_span(Span::styled(format!("  -{removed}"), theme.error));
    }
    if added > 0 {
        summary.push_span(Span::styled(format!("  +{added}"), theme.ok));
    }
    frame.render_widget(summary, row(body, 1));
    let mut diff = below(body, if body.height >= 4 { 3 } else { 2 });
    if state.has_original_file()
        && diff.height >= 2
        && let Some(file_name) = state.path().file_name()
    {
        diff.height = diff.height.saturating_sub(1);
        let backup = state.path().with_file_name(backup_file_name(file_name));
        let backup = paths.abbreviate_home(&backup);
        put(
            frame,
            row(body, body.height.saturating_sub(1)),
            &format!("The current file is kept as {backup}"),
            theme.muted,
        );
    }
    render_diff(frame, state.hunks(), state.scroll(), diff, theme);
}

fn render_diff(frame: &mut Frame, hunks: &[DiffHunk], mut skip: usize, area: Rect, theme: &Theme) {
    let mut rows = area.rows();
    let mut shown_hunk = false;
    for hunk in hunks {
        if skip >= hunk.lines.len() {
            skip -= hunk.lines.len();
            continue;
        }
        if shown_hunk {
            let Some(rect) = rows.next() else { return };
            put(frame, rect, "⋯", theme.muted);
        }
        for change in review_lines(hunk).skip(skip) {
            let Some(rect) = rows.next() else { return };
            diff_line(frame, rect, change, theme);
        }
        skip = 0;
        shown_hunk = true;
    }
}

fn review_lines(hunk: &DiffHunk) -> impl Iterator<Item = &DiffLine> {
    hunk.lines
        .chunk_by(|left, right| (left.tag == DiffTag::Context) == (right.tag == DiffTag::Context))
        .flat_map(|group| {
            // Pair adjacent removals and additions in the preview while preserving file order.
            let mut old = group.iter().filter(|line| line.tag != DiffTag::Added);
            let mut new = group.iter().filter(|line| line.tag == DiffTag::Added);
            std::iter::from_fn(move || {
                let pair = [old.next(), new.next()];
                pair.iter().any(Option::is_some).then_some(pair)
            })
            .flatten()
            .flatten()
        })
}

fn diff_line(frame: &mut Frame, area: Rect, change: &DiffLine, theme: &Theme) {
    let (marker, style) = match change.tag {
        DiffTag::Context => ("", theme.muted),
        DiffTag::Removed => ("-", theme.removed),
        DiffTag::Added => ("+", theme.added),
    };
    frame.buffer_mut().set_style(area, style);
    for (offset, number) in [(0, change.old_line), (5, change.new_line)] {
        if let Some(number) = number {
            let col = left(right_of(area, offset), 5);
            put_right(frame, col, &number.to_string(), style);
        }
    }
    put(frame, left(right_of(area, 12), 1), marker, bold(style));
    let text = change.text.replace('\t', "    ");
    put(frame, right_of(area, 14), &text, style);
}

pub(super) fn key_picker(
    frame: &mut Frame,
    state: &KeyPickerState,
    paths: &SshPaths,
    body: Rect,
    theme: &Theme,
    hits: &mut HitMap,
    hover: HoverFrame<'_>,
) -> &'static [KeyHint] {
    let count = state.keys().len();
    let directory = paths.abbreviate_home(paths.ssh_dir());
    let count_text = if count == 1 {
        format!("1 key in {directory}")
    } else {
        format!("{count} keys in {directory}")
    };
    let area = popup(frame, body, count.saturating_add(2), theme, hits);
    popup_heading(frame, area, "Select a key file", &count_text, theme);
    let selection = state.selection();
    for (index, rect) in visible_rows(below(area, 2), count, Some(selection)) {
        let key = &state.keys()[index];
        let selected = index == selection;
        let t = intensity(hover, HoverKey::KeyRow(index));
        frame
            .buffer_mut()
            .set_style(rect, theme.hover_row(selected, t));
        if selected {
            put(
                frame,
                left(rect, 1),
                "›",
                theme.selected(bold(theme.accent), true),
            );
        }
        let content = right_of(rect, 2);
        let path = paths.abbreviate_home(&key.path);
        let style = if selected {
            bold(theme.text)
        } else {
            theme.text
        };
        put(
            frame,
            content,
            &path,
            theme.hover_text(theme.selected(style, selected), t),
        );
        if let Some(public) = &key.public {
            let metadata = match &public.comment {
                Some(comment) if !comment.is_empty() => format!("{} {comment}", public.key_type),
                _ => public.key_type.clone(),
            };
            let path_width = u16::try_from(path.width()).unwrap_or(u16::MAX);
            let metadata_area = right_of(content, path_width.saturating_add(1));
            let metadata = super::layout::clip_cells(&metadata, metadata_area.width);
            put_right(
                frame,
                metadata_area,
                &metadata,
                theme.selected(theme.muted, selected),
            );
        }
        hits.add(rect, ClickTarget::KeyRow(index));
    }
    &[
        ("enter", "use this key", Some(KeyCode::Enter)),
        ("↑↓", "select", None),
        ("esc", "cancel", Some(KeyCode::Esc)),
    ]
}

pub(super) fn notice(
    frame: &mut Frame,
    state: &NoticeState,
    body: Rect,
    theme: &Theme,
    hits: &mut HitMap,
) -> &'static [KeyHint] {
    let width = popup_content_width(body);
    let lines = styled_wrapped(&state.lines().join("\n"), width, body.height, theme.warn);
    let area = popup(frame, body, lines.len().saturating_add(2), theme, hits);
    popup_heading(frame, area, state.title(), "", theme);
    render_lines(frame, below(area, 2), lines);
    if state.connects() {
        &[
            ("enter", "connect anyway", Some(KeyCode::Enter)),
            ("esc", "back to list", Some(KeyCode::Esc)),
        ]
    } else {
        &[("enter", "ok", Some(KeyCode::Enter))]
    }
}

pub(super) fn update(
    frame: &mut Frame,
    state: &UpdateState,
    body: Rect,
    theme: &Theme,
    hits: &mut HitMap,
) -> &'static [KeyHint] {
    let width = popup_content_width(body);
    let label_width = 15.min(width.saturating_sub(1));
    let mut lines = labeled_lines(
        "Installed via",
        &state.method_description(),
        width,
        label_width,
        body.height,
        theme,
    );
    if state.can_run() {
        for (step, command) in state.display_steps().iter().enumerate() {
            let command = command_lines(
                command,
                width.saturating_sub(label_width),
                body.height,
                theme,
            );
            for (index, mut line) in command.into_iter().enumerate() {
                let label = if step == 0 && index == 0 {
                    "Command"
                } else {
                    ""
                };
                line.spans.insert(
                    0,
                    Span::styled(
                        format!(
                            "{label:<label_width$}",
                            label_width = usize::from(label_width)
                        ),
                        theme.muted,
                    ),
                );
                lines.push(line);
            }
        }
    } else {
        lines.extend(labeled_lines(
            "Download",
            &state.release_url(),
            width,
            label_width,
            body.height,
            theme,
        ));
    }
    let area = popup(frame, body, lines.len().saturating_add(2), theme, hits);
    let versions = format!("{} → {}", state.current(), state.latest());
    popup_heading(frame, area, "Update sshtui", &versions, theme);
    render_lines(frame, below(area, 2), lines);
    if state.can_run() {
        &[
            ("enter", "update and exit", Some(KeyCode::Enter)),
            ("esc", "cancel", Some(KeyCode::Esc)),
        ]
    } else {
        &[("esc", "close", Some(KeyCode::Esc))]
    }
}

fn popup_width(body: Rect) -> u16 {
    if body.width >= 60 {
        60
    } else {
        body.width.saturating_sub(4)
    }
}

fn popup_content_width(body: Rect) -> u16 {
    let width = popup_width(body);
    width.saturating_sub(if width >= 7 { 6 } else { 2 })
}

fn popup(frame: &mut Frame, body: Rect, rows: usize, theme: &Theme, hits: &mut HitMap) -> Rect {
    let width = popup_width(body);
    let height = u16::try_from(rows.saturating_add(2))
        .unwrap_or(u16::MAX)
        .min(body.height);
    let remaining = body.height.saturating_sub(height);
    let top = if remaining >= 6 { 6 } else { remaining / 2 };
    let area = Rect::new(
        body.x + body.width.saturating_sub(width) / 2,
        body.y + top,
        width,
        height,
    );
    hits.add(frame.area(), ClickTarget::Outside);
    hits.add(area, ClickTarget::Inert);
    if area.is_empty() {
        return area;
    }
    frame.render_widget(Clear, area);
    frame.buffer_mut().set_style(area, theme.base);
    if area.width < 3 || area.height < 3 {
        return area;
    }
    let block = Block::bordered()
        .border_type(BorderType::Plain)
        .border_style(theme.text);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    inset(inner, if area.width >= 7 { 2 } else { 0 }, 0)
}

fn popup_heading(frame: &mut Frame, area: Rect, title: &str, right: &str, theme: &Theme) {
    header_line(
        frame,
        row(area, 0),
        Line::from(Span::styled(title, bold(theme.text))),
        right,
        theme.muted,
    );
}
