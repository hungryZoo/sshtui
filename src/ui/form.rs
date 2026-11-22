use crossterm::event::KeyCode;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::app::{App, Backend, ClickTarget, FormField, FormKind, FormState};
use crate::config::{FieldState, OptionKey};
use crate::paths::SshPaths;
use crate::validate::ValidationError;

use super::layout::{below, clip_cells, left, render_lines, right_of, row, rule};
use super::theme::bold;
use super::{
    HitMap, HoverFrame, HoverKey, KeyHint, KeyHints, Theme, command_lines, input_window, intensity,
    styled_wrapped,
};

const TAB: KeyHint = ("tab", "next", Some(KeyCode::Tab));
const BACK: KeyHint = ("esc", "back", Some(KeyCode::Esc));
const CLEAR: KeyHint = ("ctrl+u", "clear field", None);
const PICK: KeyHint = ("ctrl+l", "pick key", Some(KeyCode::F(2)));
const BROWSE: KeyHint = ("ctrl+o", "browse", Some(KeyCode::F(3)));
const CONNECT: KeyHint = ("enter", "connect", Some(KeyCode::Enter));
const SAVE: KeyHint = ("enter", "save & connect", Some(KeyCode::Enter));
const REVIEW: KeyHint = ("enter", "review changes", Some(KeyCode::Enter));

pub(super) fn heading(form: &FormState, paths: &SshPaths) -> (String, String) {
    match form.kind() {
        FormKind::New => ("New connection".into(), String::new()),
        FormKind::Edit { entry, .. } => (
            format!("Edit {}", entry.name),
            format!(
                "{}:{}",
                paths.abbreviate_home(&entry.source),
                entry.line_number
            ),
        ),
    }
}

pub(super) fn help<B: Backend>(app: &App<B>, form: &FormState) -> KeyHints {
    let key_file = form.focus() == FormField::KeyFile;
    let hints: &'static [KeyHint] = match (
        form.kind(),
        form.input(FormField::Nickname).value().is_empty(),
        key_file,
    ) {
        (FormKind::Edit { .. }, _, false) => &[REVIEW, TAB, CLEAR, BACK],
        (FormKind::Edit { .. }, _, true) => &[REVIEW, TAB, PICK, BROWSE, BACK],
        (FormKind::New, true, false) => &[CONNECT, TAB, CLEAR, BACK],
        (FormKind::New, true, true) => &[CONNECT, TAB, PICK, BROWSE, BACK],
        (FormKind::New, false, false) => &[SAVE, TAB, CLEAR, BACK],
        (FormKind::New, false, true) => &[SAVE, TAB, PICK, BROWSE, BACK],
    };
    (app.form_first_error().is_none(), hints)
}

pub(super) fn render<B: Backend>(
    frame: &mut Frame,
    app: &App<B>,
    form: &FormState,
    area: Rect,
    theme: &Theme,
    hits: &mut HitMap,
    hover: HoverFrame<'_>,
) {
    if area.is_empty() {
        return;
    }
    let header_rows = area.height.saturating_sub(5).min(2);
    if header_rows > 0 {
        rule(frame, row(area, 0), theme.rule);
    }
    let area = below(area, header_rows);
    let columns = FieldColumns::new(area.width);
    let mut lines = Vec::new();
    let mut field_rows = Vec::new();
    let mut focused_row = 0;
    let mut cursor = None;
    for field in FormField::ALL {
        if !lines.is_empty() && area.height >= 9 {
            lines.push(Line::default());
        }
        let first = lines.len();
        let input = form.input(field);
        let focused = form.focus() == field;
        let t = if focused {
            0.0
        } else {
            intensity(hover, HoverKey::FormField(field))
        };
        if focused {
            focused_row = first;
        }
        let error = if theme.dim || input.value().is_empty() {
            None
        } else {
            app.validation_error(field)
        };
        let label = columns.label(field, focused, error.is_some(), theme, t);
        let note = (!theme.dim).then(|| requirement(form, field, theme));
        let editable = !form.is_read_only(field);
        let mut scroll = 0;
        if let Some(original) = form.original_lines(field) {
            let mut original = styled_wrapped(
                &original.join("\n"),
                columns.input,
                area.height,
                theme.muted,
            )
            .into_iter();
            let first = original
                .next()
                .and_then(|line| line.spans.into_iter().next());
            lines.push(columns.field_line(label, first.unwrap_or_default(), super::PLAIN, note));
            for line in original {
                lines.push(indented(columns.label, line));
            }
        } else {
            let value_width = columns.input.saturating_sub(2);
            let (value, column, offset) = input_window(input, value_width, focused);
            scroll = offset;
            if focused && !theme.dim && columns.input > 0 {
                cursor = Some(columns.label + u16::from(columns.input >= 2) + column);
            }
            let inherited = form.placeholder(field).filter(|_| input.value().is_empty());
            let value = match inherited {
                Some(value) => Span::styled(format!("inherited: {value}"), theme.muted),
                None if value.is_empty()
                    && field == FormField::Port
                    && matches!(form.kind(), FormKind::New)
                    && !theme.dim =>
                {
                    // Show the standard SSH port hint while leaving the port unspecified.
                    Span::styled("22", theme.muted)
                }
                None => Span::styled(value.to_owned(), theme.text),
            };
            let style = theme.hover_input(focused, t);
            let value = Span::styled(
                format!(" {} ", clip_cells(&value.content, value_width)),
                style.patch(value.style),
            );
            lines.push(columns.field_line(label, value, style, note));
        }
        field_rows.push((first..lines.len(), field, editable, scroll));
        if let Some(error) = error {
            let mut text = match error {
                ValidationError::InvalidPort => "Port must be a number from 1 to 65535".into(),
                _ => error.to_string(),
            };
            if let Some(first) = text.get_mut(..1) {
                first.make_ascii_uppercase();
            }
            for line in styled_wrapped(
                &text,
                area.width.saturating_sub(columns.label),
                area.height,
                theme.error,
            ) {
                lines.push(indented(columns.label, line));
            }
        }
    }

    let height = usize::from(area.height);
    let start = if focused_row >= height {
        focused_row.saturating_sub(height.saturating_sub(2))
    } else {
        0
    };
    let remaining = render_lines(frame, area, lines.into_iter().skip(start).collect());
    for (rows, field, editable, scroll) in field_rows {
        for line in rows.start.max(start)..rows.end.min(start + height) {
            let rect = row(area, (line - start) as u16);
            columns.add_hits(hits, rect, field, editable, scroll);
        }
    }
    if let Some(column) = cursor {
        let rect = row(area, (focused_row - start) as u16);
        if !rect.is_empty() {
            frame.set_cursor_position((rect.x + column, rect.y));
        }
    }
    if !theme.dim && start == 0 && remaining.height >= 2 {
        let show_gap = u16::from(remaining.height >= 4);
        rule(frame, row(remaining, show_gap), theme.rule);
        render_guidance(frame, app, form, below(remaining, show_gap + 1), theme);
    }
}

struct FieldColumns {
    label: u16,
    input: u16,
}

impl FieldColumns {
    fn new(width: u16) -> Self {
        let label = 13.min(width.saturating_sub(10));
        Self {
            label,
            input: width.saturating_sub(label).min(36),
        }
    }

    fn label(
        &self,
        field: FormField,
        focused: bool,
        invalid: bool,
        theme: &Theme,
        t: f32,
    ) -> Span<'static> {
        let style = match (focused, invalid) {
            (true, true) => bold(theme.error),
            (false, true) => theme.error,
            (true, false) => bold(theme.accent),
            (false, false) => theme.hover_muted(t),
        };
        let marker = if focused { '›' } else { ' ' };
        let caption = format!("{marker} {}", caption(field));
        let width = usize::from(self.label);
        Span::styled(
            format!("{caption:<width$.width$}"),
            theme.hover_text(style, t),
        )
    }

    fn field_line(
        &self,
        label: Span<'static>,
        value: Span<'static>,
        padding: Style,
        note: Option<Span<'static>>,
    ) -> Line<'static> {
        let value_style = value.style;
        let value = clip_cells(&value.content, self.input).into_owned();
        let value = Span::styled(value, value_style);
        let rest = usize::from(self.input).saturating_sub(value.width());
        let mut line = Line::from(vec![label, value, Span::styled(" ".repeat(rest), padding)]);
        if let Some(note) = note {
            line.push_span("  ");
            line.push_span(note);
        }
        line
    }

    fn add_hits(
        &self,
        hits: &mut HitMap,
        area: Rect,
        field: FormField,
        editable: bool,
        scroll: usize,
    ) {
        hits.add(
            area,
            ClickTarget::FormField {
                field,
                column: u16::MAX,
            },
        );
        if editable {
            let input = left(right_of(area, self.label), self.input);
            for cell in input.columns() {
                let offset = cell.x.saturating_sub(input.x).saturating_sub(1);
                let column = u16::try_from(scroll + usize::from(offset)).unwrap_or(u16::MAX);
                hits.add(cell, ClickTarget::FormField { field, column });
            }
        }
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

fn requirement(form: &FormState, field: FormField, theme: &Theme) -> Span<'static> {
    if form.is_read_only(field) {
        return Span::styled("read-only", theme.warn);
    }
    match (form.kind(), field) {
        (FormKind::New, FormField::Nickname) => {
            Span::styled("optional · fill in to save this host", theme.muted)
        }
        (FormKind::New, FormField::Address) | (FormKind::Edit { .. }, FormField::Nickname) => {
            Span::styled("required", theme.warn)
        }
        (FormKind::Edit { original, .. }, FormField::Address)
            if !matches!(original.get(OptionKey::HostName), FieldState::Absent) =>
        {
            Span::styled("required", theme.warn)
        }
        _ => Span::styled("optional", theme.muted),
    }
}

fn indented(width: u16, mut line: Line<'static>) -> Line<'static> {
    line.spans
        .insert(0, Span::raw(" ".repeat(usize::from(width))));
    line
}

fn render_guidance<B: Backend>(
    frame: &mut Frame,
    app: &App<B>,
    form: &FormState,
    area: Rect,
    theme: &Theme,
) {
    let mut lines = vec![Line::from(Span::styled("On enter", theme.muted))];
    let paths = app.backend().paths();
    if let Some((field, error)) = app.form_first_error() {
        let action = if matches!(error, ValidationError::Empty { .. }) {
            "Fill in"
        } else {
            "Fix"
        };
        lines.push(Line::from(Span::styled(
            format!("  {action} {} to continue", caption(field)),
            theme.error,
        )));
    } else if let FormKind::Edit { entry, .. } = form.kind() {
        let source = paths.abbreviate_home(&entry.source);
        let review = styled_wrapped(
            &format!("Review changes to {source}"),
            area.width.saturating_sub(4),
            area.height,
            theme.text,
        );
        lines.extend(review.into_iter().enumerate().map(|(index, mut line)| {
            let prefix = if index == 0 { "  1 " } else { "    " };
            line.spans.insert(0, Span::styled(prefix, theme.muted));
            line
        }));
    } else {
        let nickname = form.input(FormField::Nickname).value();
        let save = !nickname.is_empty();
        let path = paths.abbreviate_home(&paths.config_file());
        if save {
            let content = format!("{nickname} to {path}");
            let mut remaining_nickname = nickname;
            let mut save_lines = styled_wrapped(
                &content,
                area.width.saturating_sub(10),
                area.height,
                theme.text,
            );
            for line in &mut save_lines {
                if let Some(value) = line.spans.first_mut() {
                    let length = remaining_nickname.len().min(value.content.len());
                    if value.content.is_char_boundary(length)
                        && remaining_nickname.is_char_boundary(length)
                        && let Some(rest) =
                            remaining_nickname.strip_prefix(&value.content[..length])
                    {
                        let name = value.content[..length].to_owned();
                        value.content = value.content[length..].to_owned().into();
                        line.spans.insert(0, Span::styled(name, bold(theme.text)));
                        remaining_nickname = rest;
                    }
                }
            }
            lines.extend(step(1, "Save", save_lines, theme));
        }
        if let Some(preview) = app.loaded_form_preview() {
            match preview {
                Ok(command) => {
                    let command =
                        command_lines(&command, area.width.saturating_sub(10), area.height, theme);
                    lines.extend(step(if save { 2 } else { 1 }, "Run", command, theme));
                }
                Err(error) => {
                    let error = styled_wrapped(
                        &error,
                        area.width.saturating_sub(2),
                        area.height,
                        theme.error,
                    );
                    lines.extend(error.into_iter().map(|line| indented(2, line)));
                }
            }
        }
        lines.push(Line::default());
        let hint = if save {
            "Leave Nickname empty to connect once without saving.".into()
        } else {
            format!("Fill in Nickname to save this host to {path}.")
        };
        lines.extend(styled_wrapped(&hint, area.width, area.height, theme.muted));
    }
    if let Some(notice) = form.shared_notice() {
        lines.push(Line::default());
        lines.extend(styled_wrapped(&notice, area.width, area.height, theme.warn));
    }
    if form.focus() == FormField::KeyFile {
        for hint in [app.picker_unavailable_reason(), app.hidden_files_hint()]
            .into_iter()
            .flatten()
        {
            lines.push(Line::from(Span::styled(hint, theme.muted)));
        }
    }
    render_lines(frame, area, lines);
}

fn step(
    number: u8,
    verb: &str,
    lines: Vec<Line<'static>>,
    theme: &Theme,
) -> impl Iterator<Item = Line<'static>> {
    let prefix = [
        Span::styled(format!("  {number} "), theme.muted),
        Span::styled(format!("{verb:<5} "), theme.text),
    ];
    lines.into_iter().enumerate().map(move |(index, mut line)| {
        if index == 0 {
            line.spans.splice(0..0, prefix.clone());
            line
        } else {
            indented(10, line)
        }
    })
}
