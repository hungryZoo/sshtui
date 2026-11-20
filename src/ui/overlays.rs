use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};
use unicode_width::UnicodeWidthStr;

use crate::app::{ConfirmState, KeyPickerState, NoticeState};
use crate::config::{DiffLine, DiffTag};

use super::{below, row, selectable_list};

pub(super) fn confirm(frame: &mut Frame, state: &ConfirmState, area: Rect) {
    if area.is_empty() {
        return;
    }
    frame.render_widget(Line::from(state.path().to_string_lossy()), row(area, 0));

    let mut rows = below(area, 1).rows();
    let mut skip = state.scroll();
    let mut shown_hunk = false;
    for hunk in state.hunks() {
        if skip >= hunk.lines.len() {
            skip -= hunk.lines.len();
            continue;
        }
        if shown_hunk {
            let Some(row) = rows.next() else { return };
            frame.render_widget(Line::from("─".repeat(usize::from(row.width))).dim(), row);
        }
        for change in hunk.lines.iter().skip(skip) {
            let Some(row) = rows.next() else { return };
            frame.render_widget(diff_line(change), row);
        }
        skip = 0;
        shown_hunk = true;
    }
}

fn diff_line(change: &DiffLine) -> Line<'_> {
    let old = change
        .old_line
        .map(|number| number.to_string())
        .unwrap_or_default();
    let new = change
        .new_line
        .map(|number| number.to_string())
        .unwrap_or_default();
    let marker = match change.tag {
        DiffTag::Context => ' ',
        DiffTag::Removed => '-',
        DiffTag::Added => '+',
    };
    let line = Line::from(format!("{old:>4} {new:>4} {marker} {}", change.text));
    match change.tag {
        DiffTag::Context => line.dim(),
        DiffTag::Removed => line.red(),
        DiffTag::Added => line.green(),
    }
}

pub(super) fn key_picker(frame: &mut Frame, state: &KeyPickerState, body: Rect) {
    let area = popup(frame, body, "Select a key file", state.keys().len(), 74);
    selectable_list(
        frame,
        area,
        state.keys().iter().map(|key| Line::from(key.label())),
        Some(state.selection()),
    );
}

pub(super) fn notice(frame: &mut Frame, state: &NoticeState, body: Rect) {
    let width = usize::from(popup_width(body, 104).saturating_sub(2).max(1));
    let rows = state
        .lines()
        .iter()
        .map(|line| 1 + line.width().div_ceil(width))
        .sum();
    let area = popup(frame, body, state.title(), rows, 104);
    let lines: Vec<_> = state
        .lines()
        .iter()
        .map(|line| Line::from(line.as_str()))
        .collect();
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

fn popup(frame: &mut Frame, body: Rect, title: &str, content_rows: usize, max_width: u16) -> Rect {
    if body.is_empty() {
        return body;
    }
    let width = popup_width(body, max_width);
    let desired_height = u16::try_from(content_rows.saturating_add(2)).unwrap_or(u16::MAX);
    let height = body.height.min(desired_height);
    let area = Rect::new(
        body.x.saturating_add((body.width - width) / 2),
        body.y.saturating_add((body.height - height) / 2),
        width,
        height,
    );
    frame.render_widget(Clear, area);
    if width >= 3 && height >= 3 {
        let block = Block::bordered().title(Line::from(title).bold());
        let inner = block.inner(area);
        frame.render_widget(block, area);
        inner
    } else if height >= 2 {
        frame.render_widget(Line::from(title).bold(), row(area, 0));
        below(area, 1)
    } else {
        area
    }
}

fn popup_width(body: Rect, max_width: u16) -> u16 {
    (body.width - body.width / 8).min(max_width)
}
