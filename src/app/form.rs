//! Form values, original block fields, and validation.

use crossterm::event::KeyEvent;

use crate::config::{
    BlockFields, Change, FieldState, HostEntry, NewHost, OptionChanges, OptionKey,
};
use crate::ssh::args::ConnectTarget;
use crate::ssh::effective::{EffectiveConfig, ExpectedConfig};
use crate::validate::{
    ValidationError, check_nickname_unique, parse_port, validate_address, validate_key_file,
    validate_nickname, validate_user,
};

use super::TextInput;

/// A field in the connection form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormField {
    Nickname,
    User,
    Address,
    Port,
    KeyFile,
}

impl FormField {
    /// All fields in focus order.
    pub const ALL: [Self; 5] = [
        Self::Nickname,
        Self::User,
        Self::Address,
        Self::Port,
        Self::KeyFile,
    ];

    fn option(self) -> Option<OptionKey> {
        match self {
            Self::Nickname => None,
            Self::User => Some(OptionKey::User),
            Self::Address => Some(OptionKey::HostName),
            Self::Port => Some(OptionKey::Port),
            Self::KeyFile => Some(OptionKey::IdentityFile),
        }
    }
}

/// Whether the form opens a new connection or edits a saved alias.
#[derive(Debug)]
pub enum FormKind {
    New,
    Edit {
        /// The alias being edited.
        entry: HostEntry,
        /// The original editable options in its block.
        original: Box<BlockFields>,
    },
}

#[derive(Debug, Default)]
struct FormInput {
    text: TextInput,
    placeholder: Option<String>,
}

/// The values, focus, and source of a connection form.
#[derive(Debug)]
pub struct FormState {
    kind: FormKind,
    fields: Box<[FormInput; FormField::ALL.len()]>,
    focus: usize,
}

impl FormState {
    pub(super) fn new() -> Self {
        Self {
            kind: FormKind::New,
            fields: Box::default(),
            focus: 0,
        }
    }

    pub(super) fn edit(
        entry: HostEntry,
        original: BlockFields,
        effective: Option<&EffectiveConfig>,
    ) -> Self {
        let mut form = Self::new();
        form.fields[FormField::Nickname as usize]
            .text
            .set(entry.name.clone());
        for field in FormField::ALL {
            let Some(option) = field.option() else {
                continue;
            };
            let input = &mut form.fields[field as usize];
            match original.get(option) {
                FieldState::Value(value) => input.text.set(value.clone()),
                FieldState::Absent => {
                    input.placeholder = effective.and_then(|values| match option {
                        OptionKey::User => values.user.clone(),
                        OptionKey::HostName => values.host_name.clone(),
                        OptionKey::Port => values.port.clone(),
                        OptionKey::IdentityFile => values.identity_files.first().cloned(),
                    });
                }
                FieldState::Unrepresentable { .. } => {}
            }
        }
        form.kind = FormKind::Edit {
            entry,
            original: Box::new(original),
        };
        form
    }

    /// Returns the form kind and original edit data when applicable.
    pub fn kind(&self) -> &FormKind {
        &self.kind
    }

    /// Returns the currently focused field.
    pub fn focus(&self) -> FormField {
        FormField::ALL[self.focus]
    }

    /// Returns the input value and cursor for a field.
    pub fn input(&self, field: FormField) -> &TextInput {
        &self.fields[field as usize].text
    }

    /// Returns the inherited value of a field absent from the block.
    pub fn placeholder(&self, field: FormField) -> Option<&str> {
        self.fields[field as usize].placeholder.as_deref()
    }

    /// Returns whether the block defines a field in a way the editor cannot represent.
    pub fn is_read_only(&self, field: FormField) -> bool {
        self.original_lines(field).is_some()
    }

    /// Returns the original lines of a read-only field.
    pub fn original_lines(&self, field: FormField) -> Option<&[String]> {
        match self.original(field)? {
            FieldState::Unrepresentable { lines } => Some(lines),
            _ => None,
        }
    }

    /// Describes how edits to a shared Host block affect the other patterns.
    pub fn shared_notice(&self) -> Option<String> {
        match &self.kind {
            FormKind::Edit { entry, .. } if !entry.siblings.is_empty() => Some(format!(
                "Option changes also apply to {}. Renaming affects only {}.",
                entry.siblings.join(", "),
                entry.name
            )),
            _ => None,
        }
    }

    pub(super) fn value(&self, field: FormField) -> &str {
        self.input(field).value()
    }

    pub(super) fn validation_error(
        &self,
        field: FormField,
        hosts: &[HostEntry],
    ) -> Option<ValidationError> {
        if self.is_read_only(field) {
            return None;
        }

        let value = self.value(field);
        match field {
            FormField::Nickname => self.validate_nickname(value, hosts),
            FormField::User => validate_user(value),
            FormField::Address
                if value.is_empty() && self.original(field) == Some(&FieldState::Absent) =>
            {
                Ok(())
            }
            FormField::Address => validate_address(value),
            FormField::Port => parse_port(value).map(|_| ()),
            FormField::KeyFile => validate_key_file(value),
        }
        .err()
    }

    pub(super) fn first_error(&self, hosts: &[HostEntry]) -> Option<(FormField, ValidationError)> {
        FormField::ALL
            .into_iter()
            .find_map(|field| Some((field, self.validation_error(field, hosts)?)))
    }

    pub(super) fn target(&self) -> ConnectTarget {
        let nickname = self.value(FormField::Nickname);
        if nickname.is_empty() && matches!(self.kind, FormKind::New) {
            ConnectTarget::Direct {
                user: self.value(FormField::User).into(),
                address: self.value(FormField::Address).into(),
                port: self.value(FormField::Port).into(),
                key_file: self.value(FormField::KeyFile).into(),
            }
        } else {
            ConnectTarget::Alias(nickname.into())
        }
    }

    pub(super) fn new_host(&self) -> NewHost {
        let optional = |field| {
            let value = self.value(field);
            (!value.is_empty()).then(|| value.to_owned())
        };
        NewHost {
            name: self.value(FormField::Nickname).into(),
            host_name: optional(FormField::Address),
            user: optional(FormField::User),
            port: optional(FormField::Port),
            identity_file: optional(FormField::KeyFile),
        }
    }

    pub(super) fn option_changes(&self, original: &BlockFields) -> OptionChanges {
        let change = |key, field| Change::from_form(original.get(key), self.value(field));
        OptionChanges {
            host_name: change(OptionKey::HostName, FormField::Address),
            user: change(OptionKey::User, FormField::User),
            port: change(OptionKey::Port, FormField::Port),
            identity_file: change(OptionKey::IdentityFile, FormField::KeyFile),
        }
    }

    pub(super) fn expected(&self) -> ExpectedConfig {
        let value = |field| {
            if self.is_read_only(field) {
                String::new()
            } else {
                self.value(field).into()
            }
        };
        ExpectedConfig {
            host_name: value(FormField::Address),
            user: value(FormField::User),
            port: value(FormField::Port),
            identity_file: value(FormField::KeyFile),
        }
    }

    pub(super) fn next_field(&mut self) {
        self.focus = (self.focus + 1) % FormField::ALL.len();
    }

    pub(super) fn previous_field(&mut self) {
        self.focus = (self.focus + FormField::ALL.len() - 1) % FormField::ALL.len();
    }

    pub(super) fn set_focus(&mut self, field: FormField) {
        self.focus = field as usize;
    }

    pub(super) fn click_field(&mut self, field: FormField, column: u16) {
        self.set_focus(field);
        if !self.is_read_only(field) {
            let column = if column == u16::MAX {
                usize::MAX
            } else {
                usize::from(column)
            };
            self.fields[self.focus].text.set_cursor_to_column(column);
        }
    }

    pub(super) fn handle_input(&mut self, key: KeyEvent) {
        if !self.is_read_only(self.focus()) {
            self.fields[self.focus].text.handle_key(key);
        }
    }

    pub(super) fn paste(&mut self, text: &str) {
        if !self.is_read_only(self.focus()) {
            self.fields[self.focus].text.paste(text);
        }
    }

    pub(super) fn set_key_file(&mut self, value: String) {
        if !self.is_read_only(FormField::KeyFile) {
            self.fields[FormField::KeyFile as usize].text.set(value);
        }
    }

    fn original(&self, field: FormField) -> Option<&FieldState> {
        match &self.kind {
            FormKind::New => None,
            FormKind::Edit { original, .. } => Some(original.get(field.option()?)),
        }
    }

    fn validate_nickname(&self, value: &str, hosts: &[HostEntry]) -> Result<(), ValidationError> {
        let edited = match &self.kind {
            FormKind::New if value.is_empty() => return Ok(()),
            FormKind::New => None,
            FormKind::Edit { entry, .. } => Some(entry),
        };
        validate_nickname(value)?;
        if edited.is_some_and(|entry| entry.name == value) {
            return Ok(());
        }
        check_nickname_unique(
            value,
            hosts
                .iter()
                .filter(|host| edited != Some(*host))
                .map(|host| host.name.as_str()),
        )
    }
}
