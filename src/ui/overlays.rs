use crossterm::event::KeyCode;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear};
use unicode_width::UnicodeWidthStr;

use crate::app::{
    ClickTarget, ConfirmChoice, ConfirmKind, ConfirmState, ImportState, KeyPickerState,
    NoticeState, UpdateState,
};
use crate::config::{DiffHunk, DiffLine, DiffTag, backup_file_name};
use crate::history::Suggestion;
use crate::paths::SshPaths;

use super::layout::{
    below, clip_cells, header_line, inset, left, put, put_right, render_lines, right_of, row, rule,
    visible_rows,
};
use super::theme::bold;
use super::{
    HitMap, HoverFrame, HoverKey, KeyHint, Theme, command_lines, intensity, labeled_lines,
    styled_wrapped,
};

const POPUP_WIDTH: u16 = 60;

pub(super) fn confirm_help(state: &ConfirmState) -> &'static [KeyHint] {
    const CANCEL_DELETE: &[KeyHint] = &destructive_help("cancel", "delete");
    const DELETE: &[KeyHint] = &destructive_help("delete", "delete");
    const CANCEL_FORGET: &[KeyHint] = &destructive_help("cancel", "forget");
    const FORGET: &[KeyHint] = &destructive_help("forget", "forget");

    match (state.kind(), state.choice()) {
        (ConfirmKind::Delete, Some(ConfirmChoice::Cancel)) => CANCEL_DELETE,
        (ConfirmKind::Delete, _) => DELETE,
        (ConfirmKind::Forget, Some(ConfirmChoice::Cancel)) => CANCEL_FORGET,
        (ConfirmKind::Forget, _) => FORGET,
        (ConfirmKind::Save, _) => &[
            ("y", "save", Some(KeyCode::Char('y'))),
            ("n", "back to form", Some(KeyCode::Char('n'))),
            ("↑↓", "scroll", None),
        ],
    }
}

const fn destructive_help(enter: &'static str, proceed: &'static str) -> [KeyHint; 5] {
    [
        ("enter", enter, Some(KeyCode::Enter)),
        ("←→", "choose", None),
        ("y", proceed, Some(KeyCode::Char('y'))),
        ("n", "back to list", Some(KeyCode::Char('n'))),
        ("↑↓", "scroll", None),
    ]
}

pub(super) fn confirm(
    frame: &mut Frame,
    state: &ConfirmState,
    paths: &SshPaths,
    mut body: Rect,
    theme: &Theme,
    hits: &mut HitMap,
) {
    if let (Some(choice), Some(question)) = (
        state.choice(),
        state.question(&paths.abbreviate_home(state.path())),
    ) {
        let area = row(body, body.height.saturating_sub(1));
        let proceed = if state.kind() == ConfirmKind::Forget {
            "Forget"
        } else {
            "Delete"
        };
        render_question(frame, area, &question, proceed, choice, theme, hits);
        body.height = body.height.saturating_sub(1);
    }
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

fn render_question(
    frame: &mut Frame,
    area: Rect,
    question: &str,
    proceed: &str,
    choice: ConfirmChoice,
    theme: &Theme,
    hits: &mut HitMap,
) {
    const GAP: u16 = 3;
    const CANCEL: &str = "Cancel";
    let labels_width = (CANCEL.width() + proceed.width()) as u16;
    let padding = u16::from(area.width >= labels_width + 2);
    let choices_width = labels_width + 2 * padding;
    let choice_gap = area.width.saturating_sub(choices_width).min(GAP);
    let room = area.width.saturating_sub(choices_width + choice_gap + GAP);
    let prompt = clip_cells(question, room);
    let prompt_width = prompt.width() as u16;
    put(frame, left(area, prompt_width), &prompt, theme.text);
    let mut buttons = right_of(area, prompt_width + if prompt_width == 0 { 0 } else { GAP });
    let badge = theme.badge(true);
    for (answer, label, code, highlight) in [
        (ConfirmChoice::Cancel, CANCEL, 'n', badge),
        (
            ConfirmChoice::Proceed,
            proceed,
            'y',
            badge.bg(theme.error.fg.unwrap_or(ratatui::style::Color::Red)),
        ),
    ] {
        let selected = choice == answer;
        let pad = if selected && padding != 0 { " " } else { "" };
        let text = format!("{pad}{label}{pad}");
        let rect = left(buttons, text.width() as u16);
        put(
            frame,
            rect,
            &text,
            if selected { highlight } else { theme.muted },
        );
        hits.add(rect, ClickTarget::Key(KeyCode::Char(code)));
        buttons = right_of(buttons, rect.width + choice_gap);
    }
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
    let area = popup(
        frame,
        body,
        POPUP_WIDTH,
        count.saturating_add(2),
        theme,
        hits,
    );
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

pub(super) fn import(
    frame: &mut Frame,
    state: &ImportState,
    suggestions: &[Suggestion],
    body: Rect,
    theme: &Theme,
    hits: &mut HitMap,
    hover: HoverFrame<'_>,
) -> &'static [KeyHint] {
    const GUIDANCE: &str = "Pick one to open the new connection form with these values.";
    const MAX_ROWS: usize = 10;

    let count = suggestions.len();
    let area = popup(
        frame,
        body,
        GUIDANCE.len() as u16 + 6,
        count.min(MAX_ROWS) + 3,
        theme,
        hits,
    );
    popup_heading(
        frame,
        area,
        "Import from shell history",
        &format!("{count} found"),
        theme,
    );
    let mut rows = below(area, 2);
    rows.height = rows.height.saturating_sub(1);
    let selection = state.selection();
    for (index, rect) in visible_rows(rows, count, Some(selection)) {
        let suggestion = &suggestions[index];
        let invocation = &suggestion.invocation;
        let selected = index == selection;
        let t = intensity(hover, HoverKey::SuggestionRow(index));
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

        let mut destination = match &invocation.user {
            Some(user) => format!("{user}@{}", invocation.address),
            None => invocation.address.clone(),
        };
        if let Some(port) = &invocation.port {
            destination.push(':');
            destination.push_str(port);
        }
        let content = right_of(rect, 2);
        let occurrences = format!("×{}", suggestion.count);
        let count_offset = content.width.saturating_sub(occurrences.width() as u16);
        let destination_area = left(content, count_offset.saturating_sub(2));
        let style = if selected {
            bold(theme.text)
        } else {
            theme.text
        };
        put(
            frame,
            destination_area,
            &destination,
            theme.hover_text(theme.selected(style, selected), t),
        );
        let muted = theme.selected(theme.muted, selected);
        if let Some(key_file) = &invocation.key_file {
            let offset = u16::try_from(destination.width()).unwrap_or(u16::MAX);
            put(
                frame,
                right_of(destination_area, offset.saturating_add(2)),
                key_file,
                muted,
            );
        }
        put_right(frame, right_of(content, count_offset), &occurrences, muted);
        hits.add(rect, ClickTarget::SuggestionRow(index));
    }
    put(frame, row(area, 2 + rows.height), GUIDANCE, theme.muted);
    &[
        ("enter", "fill form", Some(KeyCode::Enter)),
        ("↑↓", "select", None),
        ("esc", "close", Some(KeyCode::Esc)),
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
    let area = popup(
        frame,
        body,
        POPUP_WIDTH,
        lines.len().saturating_add(2),
        theme,
        hits,
    );
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
    let area = popup(
        frame,
        body,
        POPUP_WIDTH,
        lines.len().saturating_add(2),
        theme,
        hits,
    );
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

fn popup_width(body: Rect, preferred_width: u16) -> u16 {
    if body.width >= preferred_width {
        preferred_width
    } else {
        body.width.saturating_sub(4)
    }
}

fn popup_content_width(body: Rect) -> u16 {
    let width = popup_width(body, POPUP_WIDTH);
    width.saturating_sub(if width >= 7 { 6 } else { 2 })
}

fn popup(
    frame: &mut Frame,
    body: Rect,
    preferred_width: u16,
    rows: usize,
    theme: &Theme,
    hits: &mut HitMap,
) -> Rect {
    let width = popup_width(body, preferred_width);
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
