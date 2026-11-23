use crossterm::event::KeyCode;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Backend, ClickTarget, FormField, FormKind, FormState};
use crate::config::{FieldState, OptionKey};
use crate::i18n::{Lang, Text};
use crate::paths::SshPaths;
use crate::ssh::keysetup;

use super::layout::{
    below, clip_cells, column_width, left, padded_cells, render_lines, right_of, row, rule,
};
use super::theme::bold;
use super::{
    HitMap, HoverFrame, HoverKey, KeyHints, Theme, command_lines, input_window, intensity,
    styled_wrapped,
};

const GUIDANCE_ROWS: usize = 5;
const MAX_KEY_ROWS: usize = 4;

pub(super) fn heading(form: &FormState, paths: &SshPaths, lang: Lang) -> (String, String) {
    match form.kind() {
        FormKind::New => (lang.text().new_connection.into(), String::new()),
        FormKind::Edit { entry, .. } => (
            lang.edit_title(&entry.name),
            format!(
                "{}:{}",
                paths.abbreviate_home(&entry.source),
                entry.line_number
            ),
        ),
    }
}

pub(super) fn help<B: Backend>(app: &App<B>, form: &FormState) -> KeyHints {
    let text = app.lang().text();
    let enter = match form.kind() {
        FormKind::Edit { .. } => text.review_changes,
        FormKind::New if form.input(FormField::Nickname).value().is_empty() => text.connect,
        FormKind::New => text.save_and_connect,
    };
    let mut hints = vec![
        ("enter", enter, Some(KeyCode::Enter)),
        ("tab", text.next, Some(KeyCode::Tab)),
    ];
    match form.focus() {
        FormField::KeyFile => hints.extend([
            ("↑↓", text.pick_key, None),
            ("ctrl+o", text.browse, Some(KeyCode::F(3))),
        ]),
        field if field.is_checkbox() => {
            hints.push(("space", text.toggle, Some(KeyCode::Char(' '))))
        }
        _ => hints.push(("ctrl+u", text.clear_field, None)),
    }
    hints.push(("esc", text.back, Some(KeyCode::Esc)));
    (app.form_first_error().is_none(), hints)
}

#[derive(Clone, Copy)]
enum RowTarget {
    Field(FormField, bool, usize),
    Key(usize),
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
    let header_rows = area
        .height
        .saturating_sub(FormField::ALL.len() as u16)
        .min(2);
    if header_rows > 0 {
        rule(frame, row(area, 0), theme.rule);
    }
    let area = below(area, header_rows);
    let lang = app.lang();
    let text = lang.text();
    let columns = FieldColumns::new(area.width, text);
    let mut rows = Vec::new();
    let mut gaps = Vec::new();
    let mut focused_row = 0;
    let mut cursor = None;
    for field in FormField::ALL {
        if !rows.is_empty() && field != FormField::DisablePassword {
            gaps.push(rows.len());
        }
        let first = rows.len();
        let mut lines = Vec::new();
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
        let label = columns.label(text.caption(field), focused, error.is_some(), theme, t);
        let note = (!theme.dim).then(|| requirement(form, field, theme, text));
        let editable = !field.is_checkbox() && !form.is_read_only(field);
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
        } else if field.is_checkbox() {
            lines.push(checkbox_line(
                form, field, label, area.width, theme, t, text,
            ));
        } else {
            let value_width = columns.input.saturating_sub(2);
            let (value, column, offset) = input_window(input, value_width, focused);
            scroll = offset;
            if focused && !theme.dim && columns.input > 0 && form.key_cursor().is_none() {
                cursor = Some(columns.label + u16::from(columns.input >= 2) + column);
            }
            let inherited = form.placeholder(field).filter(|_| input.value().is_empty());
            let value = match inherited {
                Some(value) => Span::styled(lang.inherited(value), theme.muted),
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
        let target = RowTarget::Field(field, editable, scroll);
        rows.extend(lines.into_iter().map(|line| (line, Some(target))));
        if let Some(error) = error {
            for line in styled_wrapped(
                &lang.inline_validation_error(&error),
                area.width.saturating_sub(columns.label),
                area.height,
                theme.error,
            ) {
                rows.push((indented(columns.label, line), None));
            }
        }
    }

    let height = usize::from(area.height);
    let available = height.saturating_sub(rows.len());
    let key_rows = if theme.dim || form.is_read_only(FormField::KeyFile) {
        Vec::new()
    } else {
        key_rows(
            form,
            app.backend().paths(),
            Rect {
                height: available.saturating_sub(GUIDANCE_ROWS) as u16,
                ..area
            },
            columns.label,
            theme,
            hover,
            lang,
        )
    };
    let guidance_rows = if theme.dim { 0 } else { GUIDANCE_ROWS };
    let spaced = available >= guidance_rows + key_rows.len() + gaps.len();
    let key_start = gaps.last().copied().unwrap_or(0);
    let key_count = key_rows.len();
    rows.splice(key_start..key_start, key_rows);
    if focused_row >= key_start {
        focused_row += key_count;
    }
    if spaced {
        for gap in gaps.into_iter().rev() {
            let gap = if gap == key_start {
                gap + key_count
            } else {
                gap
            };
            rows.insert(gap, (Line::default(), None));
            focused_row += usize::from(focused_row >= gap);
        }
    }

    let start = if focused_row >= height {
        focused_row.saturating_sub(height.saturating_sub(2))
    } else {
        0
    };
    let lines = rows
        .into_iter()
        .skip(start)
        .take(height)
        .enumerate()
        .map(|(i, (line, target))| {
            let rect = row(area, i as u16);
            match target {
                Some(RowTarget::Field(field, editable, scroll)) => {
                    columns.add_hits(hits, rect, field, editable, scroll);
                }
                Some(RowTarget::Key(index)) => {
                    let rect = right_of(rect, columns.label);
                    let selected = form.key_cursor() == Some(index);
                    let t = intensity(hover, HoverKey::KeyRow(index));
                    frame
                        .buffer_mut()
                        .set_style(rect, theme.hover_row(selected, t));
                    hits.add(rect, ClickTarget::KeyRow(index));
                }
                None => {}
            }
            line
        })
        .collect();
    let remaining = render_lines(frame, area, lines);
    if let Some(column) = cursor {
        let rect = row(area, (focused_row - start) as u16);
        if !rect.is_empty() {
            frame.set_cursor_position((rect.x + column, rect.y));
        }
    }
    if !theme.dim && start == 0 && remaining.height >= 2 {
        let show_gap = u16::from(spaced && usize::from(remaining.height) > GUIDANCE_ROWS);
        rule(frame, row(remaining, show_gap), theme.rule);
        render_guidance(frame, app, form, below(remaining, show_gap + 1), theme);
    }
}

fn key_rows(
    form: &FormState,
    paths: &SshPaths,
    area: Rect,
    indent: u16,
    theme: &Theme,
    hover: HoverFrame<'_>,
    lang: Lang,
) -> Vec<(Line<'static>, Option<RowTarget>)> {
    let keys = form.keys();
    let available = usize::from(area.height);
    let mut count = available.min(MAX_KEY_ROWS).min(keys.len());
    if count == 0 {
        return Vec::new();
    }
    let value = form.input(FormField::KeyFile).value();
    let expanded = paths.expand_tilde(value);
    let matching = keys
        .iter()
        .position(|key| key.path == expanded || value == paths.abbreviate_home(&key.path));
    let selected = form.key_cursor().or(matching);
    let offset = |count| selected.map_or(0, |index| (index + 1).saturating_sub(count));
    if count == available && count > 1 && offset(count) + count < keys.len() {
        count -= 1;
    }
    let start = offset(count);
    let end = (start + count).min(keys.len());
    let mut lines = Vec::with_capacity(count + 1);
    for (index, key) in keys.iter().enumerate().take(end).skip(start) {
        let focused = form.key_cursor() == Some(index);
        let current = matching == Some(index);
        let (marker, marker_style) = if focused {
            ("› ", bold(theme.accent))
        } else if current && form.key_cursor().is_none() {
            ("● ", theme.text)
        } else {
            ("  ", theme.muted)
        };
        let style = if current { theme.text } else { theme.muted };
        let t = intensity(hover, HoverKey::KeyRow(index));
        let path = paths.abbreviate_home(&key.path);
        let mut line = Line::from(vec![
            Span::styled(marker, theme.selected(marker_style, focused)),
            Span::styled(path, theme.hover_text(theme.selected(style, focused), t)),
        ]);
        line = indented(indent, line);
        let remaining = usize::from(area.width).saturating_sub(line.width());
        if let Some(public) = &key.public
            && remaining > 1
        {
            let metadata = match public
                .comment
                .as_deref()
                .filter(|comment| !comment.is_empty())
            {
                Some(comment) => format!("{} {comment}", public.key_type),
                None => public.key_type.clone(),
            };
            let metadata = clip_cells(&metadata, (remaining - 1) as u16);
            line.push_span(" ".repeat(remaining - metadata.width()));
            line.push_span(Span::styled(
                metadata.into_owned(),
                theme.selected(theme.muted, focused),
            ));
        }
        lines.push((line, Some(RowTarget::Key(index))));
    }
    if end < keys.len() && lines.len() < available {
        let line = Line::from(Span::styled(lang.more_keys(keys.len() - end), theme.muted));
        lines.push((indented(indent, line), None));
    }
    lines
}

fn checkbox_line(
    form: &FormState,
    field: FormField,
    label: Span<'static>,
    width: u16,
    theme: &Theme,
    hover: f32,
    text: &Text,
) -> Line<'static> {
    let checked = form.is_checked(field);
    let (value, note) = if field == FormField::InstallKey {
        (
            text.install_public_key,
            Some((text.password_once, theme.muted)),
        )
    } else {
        (
            text.disable_password_prompt,
            (checked && !form.is_checked(FormField::InstallKey))
                .then_some((text.check_key_login, theme.warn)),
        )
    };
    let marker = if checked { "[x]" } else { "[ ]" };
    let mut line = Line::from(vec![
        label,
        Span::styled(
            format!("{marker} {value}"),
            theme.hover_text(theme.text, hover),
        ),
    ]);
    if !theme.dim
        && let Some((note, style)) = note
        && line.width() + 2 + note.width() <= usize::from(width)
    {
        line.push_span("  ");
        line.push_span(Span::styled(note, style));
    }
    line
}

struct FieldColumns {
    label: u16,
    input: u16,
}

impl FieldColumns {
    fn new(width: u16, text: &Text) -> Self {
        let widest = column_width(11, FormField::ALL.map(|field| text.caption(field)));
        let label = widest.saturating_add(2).min(width.saturating_sub(10));
        Self {
            label,
            input: width.saturating_sub(label).min(36),
        }
    }

    fn label(
        &self,
        caption: &str,
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
        let caption = format!("{marker} {caption}");
        Span::styled(
            padded_cells(&caption, self.label),
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

fn requirement(form: &FormState, field: FormField, theme: &Theme, text: &Text) -> Span<'static> {
    if form.is_read_only(field) {
        return Span::styled(text.read_only, theme.warn);
    }
    match (form.kind(), field) {
        (FormKind::New, FormField::Nickname) => Span::styled(text.optional_save, theme.muted),
        (FormKind::New, FormField::Address) | (FormKind::Edit { .. }, FormField::Nickname) => {
            Span::styled(text.required, theme.warn)
        }
        (FormKind::Edit { original, .. }, FormField::Address)
            if !matches!(original.get(OptionKey::HostName), FieldState::Absent) =>
        {
            Span::styled(text.required, theme.warn)
        }
        _ => Span::styled(text.optional, theme.muted),
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
    let lang = app.lang();
    let text = lang.text();
    let verb_width = column_width(5, [text.save_step, text.key_step, text.run_step]);
    let step_content_width = area.width.saturating_sub(5 + verb_width);
    let mut lines = vec![Line::from(Span::styled(text.on_enter, theme.muted))];
    let paths = app.backend().paths();
    let install = form.is_checked(FormField::InstallKey);
    if let Some((field, error)) = app.form_first_error() {
        let guidance = lang.continue_form(field, &error);
        lines.push(Line::from(Span::styled(
            format!("  {guidance}"),
            theme.error,
        )));
    } else if let FormKind::Edit { entry, .. } = form.kind() {
        let source = paths.abbreviate_home(&entry.source);
        let review = styled_wrapped(
            &lang.review_source(&source),
            area.width.saturating_sub(4),
            if install {
                area.height.saturating_sub(2).max(1)
            } else {
                area.height
            },
            theme.text,
        );
        lines.extend(review.into_iter().enumerate().map(|(index, mut line)| {
            let prefix = if index == 0 { "  1 " } else { "    " };
            line.spans.insert(0, Span::styled(prefix, theme.muted));
            line
        }));
        if install {
            lines.extend(step(
                2,
                text.key_step,
                verb_width,
                vec![key_install_line(
                    form,
                    paths,
                    step_content_width,
                    theme,
                    lang,
                )],
                theme,
            ));
        }
    } else {
        let nickname = form.input(FormField::Nickname).value();
        let save = !nickname.is_empty();
        let path = paths.abbreviate_home(&paths.config_file());
        if save {
            let content = lang.save_destination(nickname, &path);
            let mut remaining_nickname = nickname;
            let mut save_lines = styled_wrapped(
                &content,
                step_content_width,
                if install {
                    area.height.saturating_sub(3).max(1)
                } else {
                    area.height
                },
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
            lines.extend(step(1, text.save_step, verb_width, save_lines, theme));
        }
        let mut next = if save { 2 } else { 1 };
        if install {
            lines.extend(step(
                next,
                text.key_step,
                verb_width,
                vec![key_install_line(
                    form,
                    paths,
                    step_content_width,
                    theme,
                    lang,
                )],
                theme,
            ));
            next += 1;
        }
        if let Some(preview) = app.loaded_form_preview() {
            match preview {
                Ok(command) => {
                    let command = command_lines(&command, step_content_width, area.height, theme);
                    lines.extend(step(next, text.run_step, verb_width, command, theme));
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
            text.connect_without_saving.into()
        } else {
            lang.fill_nickname(&path)
        };
        lines.extend(styled_wrapped(&hint, area.width, area.height, theme.muted));
    }
    if let Some(notice) = form.localized_shared_notice(lang) {
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

fn key_install_line(
    form: &FormState,
    paths: &SshPaths,
    width: u16,
    theme: &Theme,
    lang: Lang,
) -> Line<'static> {
    let path = form.install_key_file().map_or_else(
        || lang.text().public_key_placeholder.into(),
        |key| paths.abbreviate_home(&keysetup::public_key_path(paths, key)),
    );
    let text = lang.install_on_server(&path);
    Line::from(Span::styled(
        clip_cells(&text, width).into_owned(),
        theme.text,
    ))
}

fn step(
    number: u8,
    verb: &str,
    verb_width: u16,
    lines: Vec<Line<'static>>,
    theme: &Theme,
) -> impl Iterator<Item = Line<'static>> {
    let prefix = [
        Span::styled(format!("  {number} "), theme.muted),
        Span::styled(format!("{} ", padded_cells(verb, verb_width)), theme.text),
    ];
    lines.into_iter().enumerate().map(move |(index, mut line)| {
        if index == 0 {
            line.spans.splice(0..0, prefix.clone());
            line
        } else {
            indented(5 + verb_width, line)
        }
    })
}
