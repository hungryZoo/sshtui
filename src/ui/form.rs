use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::app::{App, Backend, FormField, FormKind, FormState};

use super::{below, input_window, row};

pub(super) fn title(form: &FormState) -> String {
    match form.kind() {
        FormKind::New => "New connection".into(),
        FormKind::Edit { entry, .. } => format!("Edit {}", entry.name),
    }
}

pub(super) fn help(form: &FormState) -> &'static str {
    match form.kind() {
        FormKind::New => "Tab/Shift+Tab move  Enter connect  Esc back",
        FormKind::Edit { .. } => "Tab/Shift+Tab move  Enter save  Esc back",
    }
}

pub(super) fn render<B: Backend>(
    frame: &mut Frame,
    app: &App<B>,
    form: &FormState,
    area: Rect,
    active: bool,
) {
    if area.is_empty() {
        return;
    }
    let new = matches!(form.kind(), FormKind::New);
    let label_width =
        (if new { 21 } else { 10 }).min((area.width / 2).max(area.width.saturating_sub(8)));
    let input_width = area.width - label_width;
    let mut lines = Vec::new();
    let mut focused_row = 0;
    let mut cursor = None;

    for field in FormField::ALL {
        let focused = form.focus() == field;
        let label_style = if focused {
            Style::default().cyan().bold()
        } else {
            Style::default()
        };
        let caption = label(field, new, label_width);
        let label = Span::styled(
            format!("{caption:<width$}", width = usize::from(label_width)),
            label_style,
        );
        if focused {
            focused_row = lines.len();
        }
        let input = form.input(field);
        let (value, column) = input_window(input, input_width, focused && active);
        if focused && active {
            cursor = Some(label_width + column);
        }
        if let Some(original) = form.original_lines(field) {
            let mut original = original.iter();
            let first = original.next().map_or("", String::as_str);
            lines.push(Line::from(vec![
                label,
                Span::raw(format!("[read-only] {first}")).dim(),
            ]));
            for line in original {
                lines.push(Line::from(vec![
                    Span::raw(" ".repeat(usize::from(label_width))),
                    Span::raw(line).dim(),
                ]));
            }
            continue;
        }

        let value = match form.placeholder(field) {
            Some(inherited) if input.value().is_empty() => {
                Span::raw(format!("inherited: {inherited}")).dim()
            }
            _ => Span::raw(value),
        };
        lines.push(Line::from(vec![label, value]));
        if let Some(error) = app.validation_error(field) {
            lines.push(Line::from(error.to_string()).red());
        }
    }

    // Keep the focused field and its following line visible as the terminal shrinks.
    let height = usize::from(area.height);
    let start = if focused_row >= height {
        focused_row.saturating_sub(height.saturating_sub(2))
    } else {
        0
    };
    let mut used = 0;
    for (line, rect) in lines.into_iter().skip(start).zip(area.rows()) {
        frame.render_widget(line, rect);
        used += 1;
    }
    if let Some(column) = cursor
        && let Ok(offset) = u16::try_from(focused_row - start)
        && offset < area.height
    {
        let field_row = row(area, offset);
        frame.set_cursor_position((field_row.x.saturating_add(column), field_row.y));
    }
    render_guidance(frame, app, form, below(area, used));
}

fn label(field: FormField, new: bool, width: u16) -> &'static str {
    let short = match field {
        FormField::Nickname => "Nickname:",
        FormField::User => "User:",
        FormField::Address => "Address:",
        FormField::Port => "Port:",
        FormField::KeyFile => "Key file:",
    };
    let full = match field {
        FormField::Nickname => "Nickname (optional):",
        FormField::User => "User (optional):",
        FormField::Address => "Address (required):",
        FormField::Port => "Port (optional):",
        FormField::KeyFile => "Key file (optional):",
    };
    let width = usize::from(width);
    if new && full.len() < width {
        full
    } else {
        &short[..short.len().min(width)]
    }
}

fn render_guidance<B: Backend>(frame: &mut Frame, app: &App<B>, form: &FormState, area: Rect) {
    if area.is_empty() {
        return;
    }
    let mut lines = Vec::new();
    if let Some(notice) = form.shared_notice() {
        lines.push(Line::from(notice).yellow());
    }
    let browse = match app.picker_unavailable_reason() {
        Some(reason) => Span::raw(reason).dim(),
        None => Span::raw("Ctrl+O / F3: browse"),
    };
    lines.push(Line::from(vec![
        Span::raw("Ctrl+L / F2: pick from ~/.ssh   "),
        browse,
    ]));
    if let Some(hint) = app.hidden_files_hint() {
        lines.push(Line::from(hint).dim());
    }
    if let Some(preview) = app.loaded_form_preview() {
        lines.push(match preview {
            Ok(command) => Line::from(format!("Command: {command}")),
            Err(error) => Line::from(format!("Command: ({error})")).red(),
        });
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}
