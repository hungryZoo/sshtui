use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Clear;

use crate::app::{ConfirmState, KeyPickerState, NoticeState, UpdateState};
use crate::config::{DiffHunk, DiffLine, DiffTag};
use crate::paths::SshPaths;

use super::buttons::{ButtonKind, render_buttons};
use super::layout::{
    below, draw_panel, has_border, inset_horizontal, panel_title, render_lines, row, visible_rows,
};
use super::{labeled_lines, styled_wrapped, theme, wrapped_lines};

/// Draws the file and diff for a pending change.
pub(super) fn confirm(frame: &mut Frame, state: &ConfirmState, paths: &SshPaths, area: Rect) {
    if area.is_empty() {
        return;
    }
    let area = draw_panel(frame, area, Some(panel_title(&state.title())), true);
    let area = inset_horizontal(area);
    let (content, buttons) = split_footer(area);
    render_buttons(
        frame,
        buttons,
        &[
            ("Confirm y", ButtonKind::Primary),
            ("Cancel n", ButtonKind::Secondary),
        ],
    );
    let path = paths.abbreviate_home(state.path());
    let path_lines = styled_wrapped(&path, content.width, content.height.min(2), theme::MUTED);
    let diff = below(render_lines(frame, content, path_lines), 1);
    render_diff(frame, state.hunks(), state.scroll(), diff);
}

fn render_diff(frame: &mut Frame, hunks: &[DiffHunk], mut skip: usize, area: Rect) {
    if area.is_empty() {
        return;
    }
    let number_width = hunks
        .iter()
        .flat_map(|hunk| &hunk.lines)
        .flat_map(|line| [line.old_line, line.new_line])
        .flatten()
        .max()
        .map_or(4, |number| number.to_string().len().max(4));

    let mut rows = area.rows();
    let mut shown_hunk = false;
    for hunk in hunks {
        if skip >= hunk.lines.len() {
            skip -= hunk.lines.len();
            continue;
        }
        if shown_hunk {
            let Some(row) = rows.next() else { return };
            frame.render_widget(
                Span::styled("─".repeat(usize::from(row.width)), theme::MUTED),
                row,
            );
        }
        for change in hunk.lines.iter().skip(skip) {
            let Some(row) = rows.next() else { return };
            frame.render_widget(diff_line(change, number_width), row);
        }
        skip = 0;
        shown_hunk = true;
    }
}

fn diff_line(change: &DiffLine, width: usize) -> Line<'static> {
    let old = change
        .old_line
        .map(|number| number.to_string())
        .unwrap_or_default();
    let new = change
        .new_line
        .map(|number| number.to_string())
        .unwrap_or_default();
    let (marker, style) = match change.tag {
        DiffTag::Context => (' ', theme::PLAIN),
        DiffTag::Removed => ('-', theme::ERROR),
        DiffTag::Added => ('+', theme::SUCCESS),
    };
    Line::from(vec![
        Span::styled(format!("{old:>width$} {new:>width$} "), theme::MUTED),
        Span::styled(format!("{marker} {}", change.text), style),
    ])
}

/// Draws the available keys over the form.
pub(super) fn key_picker(frame: &mut Frame, state: &KeyPickerState, body: Rect) {
    let keys = state.keys();
    let bounds = popup_bounds(body, 74, keys.len());
    let area = draw_popup(frame, bounds, "Select a key file");
    let selection = state.selection();
    for (index, rect) in visible_rows(area, keys.len(), Some(selection)) {
        frame.render_widget(Span::raw(keys[index].label()), rect);
        if index == selection {
            frame.buffer_mut().set_style(rect, theme::SELECTED);
        }
    }
}

/// Draws the acknowledgment message and its actions.
pub(super) fn notice(frame: &mut Frame, state: &NoticeState, body: Rect) {
    const MAX_WIDTH: u16 = 104;
    let width = popup_content_width(body, MAX_WIDTH);
    let lines = styled_wrapped(
        &state.lines().join("\n"),
        width,
        body.height,
        theme::WARNING,
    );
    let bounds = popup_bounds(body, MAX_WIDTH, lines.len().saturating_add(1));
    let area = draw_popup(frame, bounds, state.title());
    let (content, footer) = split_footer(area);
    render_lines(frame, content, lines);
    let buttons: &[_] = if state.connects() {
        &[
            ("Connect anyway", ButtonKind::Primary),
            ("Back", ButtonKind::Secondary),
        ]
    } else {
        &[("OK", ButtonKind::Primary)]
    };
    render_buttons(frame, footer, buttons);
}

/// Draws update details and the available actions.
pub(super) fn update(frame: &mut Frame, state: &UpdateState, body: Rect) {
    const MAX_WIDTH: u16 = 104;
    let width = popup_content_width(body, MAX_WIDTH);
    let label_width = 15.min(width.saturating_sub(1));
    let mut lines = Vec::new();
    for (label, value) in [
        ("Current", state.current()),
        ("Latest", state.latest()),
        ("Installed via", &state.method_description()),
    ] {
        lines.extend(labeled_lines(label, value, width, label_width, body.height));
    }
    if state.can_run() {
        lines.push(Line::from(Span::styled("Command", theme::MUTED)));
        for step in state.display_steps() {
            lines.extend(wrapped_lines(&step, width, body.height));
        }
    } else {
        lines.extend(labeled_lines(
            "Download",
            &state.release_url(),
            width,
            label_width,
            body.height,
        ));
    }

    let bounds = popup_bounds(body, MAX_WIDTH, lines.len().saturating_add(1));
    let area = draw_popup(frame, bounds, concat!("Update ", env!("CARGO_PKG_NAME")));
    let (content, footer) = split_footer(area);
    render_lines(frame, content, lines);
    let buttons: &[_] = if state.can_run() {
        &[
            ("Update and exit", ButtonKind::Primary),
            ("Cancel", ButtonKind::Secondary),
        ]
    } else {
        &[("Close", ButtonKind::Primary)]
    };
    render_buttons(frame, footer, buttons);
}

fn split_footer(area: Rect) -> (Rect, Rect) {
    let content_height = if area.height >= 2 {
        area.height.saturating_sub(1)
    } else {
        area.height
    };
    (
        Rect {
            height: content_height,
            ..area
        },
        row(area, content_height),
    )
}

fn draw_popup(frame: &mut Frame, area: Rect, title: &str) -> Rect {
    frame.render_widget(Clear, area);
    let title = Line::from(Span::styled(format!(" {title} "), theme::BOLD));
    if has_border(area) {
        draw_panel(frame, area, Some(title), true)
    } else if area.height >= 2 {
        frame.render_widget(title, row(area, 0));
        below(area, 1)
    } else {
        area
    }
}

fn popup_content_width(body: Rect, max_width: u16) -> u16 {
    let bounds = popup_bounds(body, max_width, 1);
    if has_border(bounds) {
        bounds.width.saturating_sub(2)
    } else {
        bounds.width
    }
}

fn popup_bounds(body: Rect, max_width: u16, content_rows: usize) -> Rect {
    let width = body.width.saturating_sub(body.width / 8).min(max_width);
    let desired_height = u16::try_from(content_rows.saturating_add(2)).unwrap_or(u16::MAX);
    let height = body.height.min(desired_height);
    Rect::new(
        body.x.saturating_add(body.width.saturating_sub(width) / 2),
        body.y
            .saturating_add(body.height.saturating_sub(height) / 2),
        width,
        height,
    )
}
