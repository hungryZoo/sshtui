use crossterm::event::KeyCode;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::app::{App, Backend, ClickTarget, FormField, FormKind, FormState};

use super::buttons::{ButtonKind, after_buttons, render_buttons};
use super::layout::{
    below, clip_cells, draw_panel, has_border, inset_horizontal, left, panel_title, render_lines,
    right_of, row,
};
use super::{HitMap, KeyHint, input_window, styled_wrapped, theme};

/// Returns the title of the connection form.
pub(super) fn title(form: &FormState) -> String {
    match form.kind() {
        FormKind::New => "New connection".into(),
        FormKind::Edit { entry, .. } => format!("Edit {}", entry.name),
    }
}

/// Returns the key hints for the active form.
pub(super) fn help(form: &FormState) -> [KeyHint; 3] {
    let submit = match form.kind() {
        FormKind::New => "connect",
        FormKind::Edit { .. } => "save",
    };
    [
        ("Tab/Shift+Tab", "move", Some(KeyCode::Tab)),
        ("Enter", submit, Some(KeyCode::Enter)),
        ("Esc", "back", Some(KeyCode::Esc)),
    ]
}

/// Draws the form and displays its cursor when active.
pub(super) fn render<B: Backend>(
    frame: &mut Frame,
    app: &App<B>,
    form: &FormState,
    area: Rect,
    active: bool,
    hits: &mut HitMap,
) {
    if area.is_empty() {
        return;
    }
    // On short screens the border would take space away from the fields.
    let full_height = area.height >= FormField::ALL.len() as u16 + 2;
    let area = if full_height {
        draw_panel(frame, area, Some(panel_title(&title(form))), true)
    } else {
        area
    };
    let area = inset_horizontal(area);
    let columns = FieldColumns::new(area.width);
    let mut lines = Vec::new();
    if let Some(notice) = form.shared_notice() {
        lines.extend(styled_wrapped(
            &notice,
            area.width,
            area.height,
            theme::WARNING,
        ));
        lines.push(Line::default());
    }

    let mut focused_row = 0;
    let mut cursor = None;
    let mut field_rows = Vec::with_capacity(FormField::ALL.len());
    for field in FormField::ALL {
        let first_row = lines.len();
        let focused = form.focus() == field;
        let label = columns.label(caption(field), focused);
        if focused {
            focused_row = lines.len();
            if active && columns.input > 0 {
                cursor = Some(columns.label);
            }
        }
        if let Some(original) = form.original_lines(field) {
            let mut original = original
                .iter()
                .flat_map(|line| styled_wrapped(line, columns.input, area.height, theme::MUTED))
                .flat_map(|line| line.spans);
            let first = original.next().unwrap_or_else(|| Span::raw(""));
            lines.push(columns.field_line(
                label,
                first,
                theme::PLAIN,
                Some(Span::styled("read-only", theme::WARNING)),
            ));
            for value in original {
                lines.push(Line::from(vec![columns.label("", false), value]));
            }
            field_rows.push((first_row..lines.len(), field, 0));
            continue;
        }

        let input = form.input(field);
        let (value, column, offset) = input_window(input, columns.input, focused && active);
        if focused && cursor.is_some() {
            cursor = Some(columns.label.saturating_add(column));
        }
        let input_style = if focused {
            theme::FOCUSED_INPUT
        } else {
            theme::INPUT
        };
        let inherited = form.placeholder(field).filter(|_| input.value().is_empty());
        let value = if let Some(inherited) = inherited {
            let value = format!("inherited: {inherited}");
            Span::styled(
                clip_cells(&value, columns.input).into_owned(),
                theme::inherited_input(focused),
            )
        } else {
            Span::styled(clip_cells(value, columns.input).into_owned(), input_style)
        };
        lines.push(columns.field_line(label, value, input_style, requirement(form, field)));
        field_rows.push((first_row..lines.len(), field, offset));
        if !input.value().is_empty()
            && let Some(error) = app.validation_error(field)
        {
            let errors = styled_wrapped(
                &error.to_string(),
                area.width.saturating_sub(columns.label),
                area.height,
                theme::ERROR,
            );
            for mut line in errors {
                line.spans.insert(0, columns.label("", false));
                lines.push(line);
            }
        }
    }

    let height = usize::from(area.height);
    let start = if focused_row >= height {
        focused_row.saturating_sub(height.saturating_sub(2))
    } else {
        0
    };
    let remaining = render_lines(
        frame,
        area,
        lines.into_iter().skip(start).take(height).collect(),
    );
    for (rows, field, column_offset) in field_rows {
        for line in rows.start.max(start)..rows.end.min(start + height) {
            columns.add_hits(hits, row(area, (line - start) as u16), field, column_offset);
        }
    }
    if let Some(column) = cursor
        && let Ok(offset) = u16::try_from(focused_row.saturating_sub(start))
        && offset < area.height
    {
        frame.set_cursor_position((area.x.saturating_add(column), row(area, offset).y));
    }
    if full_height && start == 0 {
        render_guidance(frame, app, form, remaining, hits);
    }
}

struct FieldColumns {
    label: u16,
    input: u16,
    show_note: bool,
}

impl FieldColumns {
    fn new(width: u16) -> Self {
        let label = 10.min(width.saturating_sub(1));
        let remaining = width.saturating_sub(label);
        let show_note = remaining >= 19;
        let input = remaining.saturating_sub(if show_note { 11 } else { 0 });
        Self {
            label,
            input,
            show_note,
        }
    }

    fn add_hits(&self, hits: &mut HitMap, area: Rect, field: FormField, offset: usize) {
        hits.add(
            area,
            ClickTarget::FormField {
                field,
                column: u16::MAX,
            },
        );
        let input = left(right_of(area, self.label), self.input);
        for cell in input.columns() {
            let column = offset + usize::from(cell.x - input.x);
            let column = u16::try_from(column).unwrap_or(u16::MAX);
            hits.add(cell, ClickTarget::FormField { field, column });
        }
    }

    fn label(&self, caption: &str, focused: bool) -> Span<'static> {
        let width = usize::from(self.label);
        let style = if focused {
            theme::EMPHASIS
        } else {
            theme::PLAIN
        };
        Span::styled(format!("{caption:<width$.width$}"), style)
    }

    fn field_line(
        &self,
        label: Span<'static>,
        value: Span<'static>,
        padding_style: Style,
        note: Option<Span<'static>>,
    ) -> Line<'static> {
        let empty = usize::from(self.input).saturating_sub(value.width());
        let mut line = Line::from(vec![
            label,
            value,
            Span::styled(" ".repeat(empty), padding_style),
        ]);
        if self.show_note
            && let Some(note) = note
        {
            line.push_span(Span::raw("  "));
            line.push_span(note);
        }
        line
    }
}

fn caption(field: FormField) -> &'static str {
    match field {
        FormField::Nickname => "Nickname",
        FormField::User => "User",
        FormField::Address => "Address",
        FormField::Port => "Port",
        FormField::KeyFile => "Key file",
    }
}

fn requirement(form: &FormState, field: FormField) -> Option<Span<'static>> {
    let note = match (form.kind(), field) {
        (FormKind::New, FormField::Address) | (FormKind::Edit { .. }, FormField::Nickname) => {
            "required"
        }
        (FormKind::New, _) => "optional",
        (FormKind::Edit { .. }, _) => return None,
    };
    Some(Span::styled(note, theme::MUTED))
}

fn render_guidance<B: Backend>(
    frame: &mut Frame,
    app: &App<B>,
    form: &FormState,
    mut area: Rect,
    hits: &mut HitMap,
) {
    if area.is_empty() {
        return;
    }
    let unavailable = app.picker_unavailable_reason();
    let browse_kind = if unavailable.is_some() {
        ButtonKind::Disabled
    } else {
        ButtonKind::Secondary
    };
    let keys = [
        ("Keys F2", ButtonKind::Secondary, KeyCode::F(2)),
        ("Browse F3", browse_kind, KeyCode::F(3)),
    ];
    let reason_bounds = after_buttons(row(area, 0), &keys);
    let reason = unavailable.map_or_else(Vec::new, |reason| {
        styled_wrapped(&reason, reason_bounds.width, area.height, theme::MUTED)
    });
    let hint = app.hidden_files_hint();
    let reason_height = 1.max(reason.len() as u16);
    let key_height = (reason_height.saturating_add(u16::from(hint.is_some()))).min(area.height);
    let remaining = area.height.saturating_sub(key_height);
    let command_width = if area.width >= 3 {
        area.width.saturating_sub(2)
    } else {
        area.width
    };
    let command_width = inset_horizontal(Rect {
        width: command_width,
        ..area
    })
    .width;
    let preview = app.loaded_form_preview().map_or_else(Vec::new, |preview| {
        let (command, style) = match preview {
            Ok(command) => (command, theme::PLAIN),
            Err(error) => (format!("({error})"), theme::ERROR),
        };
        styled_wrapped(
            &command,
            command_width,
            remaining.saturating_sub(2).min(3),
            style,
        )
    });
    let command_height = if preview.is_empty() {
        0
    } else {
        preview.len() as u16 + 2
    };
    let extra = remaining.saturating_sub(command_height);
    let show_actions = command_height > 0 && extra > 0;
    let mut gaps = extra.saturating_sub(u16::from(show_actions));

    area = guidance_gap(area, &mut gaps);
    render_buttons(frame, row(area, 0), &keys, hits);
    render_lines(
        frame,
        Rect {
            y: area.y,
            height: reason_height,
            ..reason_bounds
        },
        reason,
    );
    if let Some(hint) = hint
        && key_height > reason_height
    {
        frame.render_widget(Span::styled(hint, theme::MUTED), row(area, reason_height));
    }
    area = below(area, key_height);

    if command_height > 0 {
        area = guidance_gap(area, &mut gaps);
        let command = Rect {
            height: command_height,
            ..area
        };
        let title = panel_title("Command");
        let content = if has_border(command) {
            draw_panel(frame, command, Some(title), false)
        } else {
            frame.render_widget(title, row(command, 0));
            below(command, 1)
        };
        render_lines(frame, inset_horizontal(content), preview);
        area = below(area, command_height);
    }
    if show_actions {
        area = guidance_gap(area, &mut gaps);
        let submit = if matches!(form.kind(), FormKind::New) {
            "Connect"
        } else {
            "Save"
        };
        render_buttons(
            frame,
            row(area, 0),
            &[
                (submit, ButtonKind::Primary, KeyCode::Enter),
                ("Cancel", ButtonKind::Secondary, KeyCode::Esc),
            ],
            hits,
        );
    }
}

fn guidance_gap(area: Rect, gaps: &mut u16) -> Rect {
    let gap = u16::from(*gaps > 0);
    *gaps = gaps.saturating_sub(gap);
    below(area, gap)
}
