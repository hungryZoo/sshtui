//! Pure functions for adding, updating, renaming, and deleting hosts.

use std::ops::Range;

use crate::validate::{ValidationError, validate_nickname};

use super::model::{Arg, Block, BlockKind, Directive, Eol, Line, LineKind, ParsedConfig};
use super::reader::is_listable_pattern;

/// Options supported by the host config editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OptionKey {
    HostName,
    User,
    Port,
    IdentityFile,
    PasswordAuthentication,
}

impl OptionKey {
    /// Editable options in the order used when writing new lines.
    pub const ALL: [Self; 5] = [
        Self::HostName,
        Self::User,
        Self::Port,
        Self::IdentityFile,
        Self::PasswordAuthentication,
    ];

    /// Returns the canonical keyword written for a new line.
    pub fn keyword(self) -> &'static str {
        match self {
            Self::HostName => "HostName",
            Self::User => "User",
            Self::Port => "Port",
            Self::IdentityFile => "IdentityFile",
            Self::PasswordAuthentication => "PasswordAuthentication",
        }
    }
}

/// How one option appears inside a single block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldState {
    /// No directive with this keyword in the block.
    Absent,
    /// Exactly one directive with exactly one argument.
    Value(String),
    /// A read-only field containing all matching lines as lossy UTF-8.
    Unrepresentable { lines: Vec<String> },
}

/// The state of each editable option in a Host block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockFields {
    pub host_name: FieldState,
    pub user: FieldState,
    pub port: FieldState,
    pub identity_file: FieldState,
    /// The state of the `PasswordAuthentication` directive.
    pub password_authentication: FieldState,
}

impl BlockFields {
    /// Returns the field state for the given option.
    pub fn get(&self, key: OptionKey) -> &FieldState {
        match key {
            OptionKey::HostName => &self.host_name,
            OptionKey::User => &self.user,
            OptionKey::Port => &self.port,
            OptionKey::IdentityFile => &self.identity_file,
            OptionKey::PasswordAuthentication => &self.password_authentication,
        }
    }
}

/// Requested change for one option.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Change {
    #[default]
    Keep,
    Set(String),
    Remove,
}

impl Change {
    /// Derives the change from the current state and the form text.
    pub fn from_form(current: &FieldState, text: &str) -> Self {
        match current {
            FieldState::Unrepresentable { .. } => Self::Keep,
            FieldState::Value(value) if value == text => Self::Keep,
            FieldState::Absent if text.is_empty() => Self::Keep,
            FieldState::Value(_) if text.is_empty() => Self::Remove,
            _ => Self::Set(text.to_owned()),
        }
    }
}

/// The requested changes to the editable options of a Host block.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OptionChanges {
    pub host_name: Change,
    pub user: Change,
    pub port: Change,
    pub identity_file: Change,
    /// The requested change to `PasswordAuthentication`.
    pub password_authentication: Change,
}

impl OptionChanges {
    /// Returns the requested change for the given option.
    pub fn get(&self, key: OptionKey) -> &Change {
        match key {
            OptionKey::HostName => &self.host_name,
            OptionKey::User => &self.user,
            OptionKey::Port => &self.port,
            OptionKey::IdentityFile => &self.identity_file,
            OptionKey::PasswordAuthentication => &self.password_authentication,
        }
    }
}

/// A Host block to insert. `None` options are omitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewHost {
    pub name: String,
    pub host_name: Option<String>,
    pub user: Option<String>,
    pub port: Option<String>,
    pub identity_file: Option<String>,
    /// The value to write for `PasswordAuthentication`, if present.
    pub password_authentication: Option<String>,
}

/// An edit of one alias and the options of its block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostEdit {
    pub block_index: usize,
    pub pattern_index: usize,
    /// New alias for the pattern at `pattern_index`; `None` keeps it.
    pub new_name: Option<String>,
    pub changes: OptionChanges,
}

/// An invalid edit request or a target the form cannot edit.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EditError {
    #[error("block {0} does not exist")]
    BlockNotFound(usize),
    #[error("block {0} is not a Host block")]
    NotHostBlock(usize),
    #[error("pattern {pattern_index} does not exist in block {block_index}")]
    PatternNotFound {
        block_index: usize,
        pattern_index: usize,
    },
    #[error(
        "{0:?} cannot be edited because the block defines it in a form the editor cannot represent"
    )]
    Unrepresentable(OptionKey),
    #[error("invalid value: {0}")]
    InvalidValue(String),
    #[error(transparent)]
    InvalidName(#[from] ValidationError),
}

/// Formats one argument for writing with quoting and escapes understood by the lexer.
/// A horizontal tab is accepted inside a quoted argument.
pub fn format_arg(value: &str) -> Result<String, EditError> {
    if value.is_empty() {
        return Err(EditError::InvalidValue("value is empty".into()));
    }
    if value.chars().any(|ch| ch.is_control() && ch != '\t') {
        return Err(EditError::InvalidValue(
            "value contains an unsupported control character".into(),
        ));
    }
    if value.contains('"') {
        return Err(EditError::InvalidValue(
            "double quotes are not allowed".into(),
        ));
    }

    let quoted = value.contains([' ', '\t', '#', '\'']) || value.starts_with('=');
    let mut formatted = String::with_capacity(value.len());
    if quoted {
        formatted.push('"');
    }
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\'
            && (matches!(chars.peek(), Some('\\' | '\'')) || (quoted && chars.peek().is_none()))
        {
            formatted.push('\\');
        }
        formatted.push(ch);
    }
    if quoted {
        formatted.push('"');
    }
    Ok(formatted)
}

/// Reads the editable options of a Host block.
pub fn block_fields(parsed: &ParsedConfig, block_index: usize) -> Result<BlockFields, EditError> {
    let block = host_block(parsed, block_index)?;
    let [
        host_name,
        user,
        port,
        identity_file,
        password_authentication,
    ] = OptionKey::ALL.map(|key| match option_lines(body(parsed, block), key) {
        OptionLines::Absent => FieldState::Absent,
        OptionLines::Value(_, arg) => FieldState::Value(arg.value.clone()),
        OptionLines::Unrepresentable(lines) => FieldState::Unrepresentable {
            lines: lines
                .iter()
                .map(|line| {
                    String::from_utf8_lossy(&parsed.bytes()[line.span.clone()]).into_owned()
                })
                .collect(),
        },
    });
    Ok(BlockFields {
        host_name,
        user,
        port,
        identity_file,
        password_authentication,
    })
}

/// Returns the file with a new Host block inserted.
pub fn add_host(parsed: &ParsedConfig, host: &NewHost) -> Result<Vec<u8>, EditError> {
    validate_nickname(&host.name)?;
    let name = format_arg(&host.name)?;
    let eol = file_eol(parsed);
    let indent = file_indent(parsed);
    let next_header = parsed.blocks().iter().find(|block| {
        block.kind == BlockKind::Match
            || parsed.lines()[block.header]
                .directive()
                .is_some_and(|header| {
                    header
                        .args
                        .iter()
                        .any(|arg| !is_listable_pattern(&arg.value))
                })
    });
    let line_index = next_header.map_or(parsed.lines().len(), |block| {
        attached_comments_start(parsed, block.header)
    });
    let mut insertion = Vec::new();
    if let Some(previous) = line_index
        .checked_sub(1)
        .and_then(|index| parsed.lines().get(index))
    {
        if previous.eol == Eol::None {
            insertion.extend_from_slice(eol);
        }
        if previous.kind != LineKind::Blank {
            insertion.extend_from_slice(eol);
        }
    }
    insertion.extend_from_slice(b"Host ");
    insertion.extend_from_slice(name.as_bytes());
    insertion.extend_from_slice(eol);
    let values = [
        &host.host_name,
        &host.user,
        &host.port,
        &host.identity_file,
        &host.password_authentication,
    ];
    for (key, value) in OptionKey::ALL.into_iter().zip(values) {
        if let Some(value) = value {
            write_option(&mut insertion, indent, key, &format_arg(value)?);
            insertion.extend_from_slice(eol);
        }
    }
    if next_header.is_some() {
        insertion.extend_from_slice(eol);
    }
    let offset = line_start(parsed, line_index);
    Ok(apply_replacements(
        parsed.bytes(),
        vec![(offset..offset, insertion)],
    ))
}

/// Returns the file with the rename and option changes applied.
pub fn apply_host_edit(parsed: &ParsedConfig, edit: &HostEdit) -> Result<Vec<u8>, EditError> {
    let (block, _, pattern) = host_pattern(parsed, edit.block_index, edit.pattern_index)?;
    let header = &parsed.lines()[block.header];
    let mut replacements = Vec::new();
    if let Some(name) = &edit.new_name
        && *name != pattern.value
    {
        validate_nickname(name)?;
        replacements.push(replace_arg(parsed, header, pattern, name)?);
    }

    let body = body(parsed, block);
    let mut additions = Vec::new();
    for key in OptionKey::ALL {
        let change = edit.changes.get(key);
        match change {
            Change::Keep => continue,
            Change::Set(value) if value.is_empty() => {
                return Err(EditError::InvalidValue("value is empty".into()));
            }
            _ => {}
        }
        match (change, option_lines(body, key)) {
            (_, OptionLines::Unrepresentable(_)) => return Err(EditError::Unrepresentable(key)),
            (Change::Set(value), OptionLines::Value(line, arg)) if *value != arg.value => {
                replacements.push(replace_arg(parsed, line, arg, value)?);
            }
            (Change::Set(value), OptionLines::Absent) => additions.push((key, format_arg(value)?)),
            (Change::Remove, OptionLines::Value(line, _)) => {
                replacements.push((line.full_span(), Vec::new()));
            }
            _ => {}
        }
    }

    if !additions.is_empty() {
        let indent = body
            .iter()
            .find_map(|line| line_indent(parsed, line))
            .unwrap_or_else(|| file_indent(parsed));
        let anchor = body
            .iter()
            .rfind(|line| {
                line.directive().is_some()
                    && !replacements
                        .iter()
                        .any(|(range, bytes)| bytes.is_empty() && *range == line.full_span())
            })
            .unwrap_or(header);
        let eol = file_eol(parsed);
        let mut insertion = Vec::new();
        for (key, value) in additions {
            if anchor.eol == Eol::None {
                insertion.extend_from_slice(eol);
            }
            write_option(&mut insertion, indent, key, &value);
            if anchor.eol != Eol::None {
                insertion.extend_from_slice(eol);
            }
        }
        let offset = anchor.full_span().end;
        replacements.push((offset..offset, insertion));
    }
    Ok(apply_replacements(parsed.bytes(), replacements))
}

/// Returns the file with one alias removed (the whole block when it is the only pattern).
pub fn delete_pattern(
    parsed: &ParsedConfig,
    block_index: usize,
    pattern_index: usize,
) -> Result<Vec<u8>, EditError> {
    let (block, header, pattern) = host_pattern(parsed, block_index, pattern_index)?;
    let args = &header.args;
    let span = if let Some(next) = args.get(pattern_index + 1) {
        pattern.span.start..next.span.start
    } else if let Some(previous) = pattern_index
        .checked_sub(1)
        .and_then(|index| args.get(index))
    {
        previous.span.end..pattern.span.end
    } else if let Some(next) = parsed.blocks().get(block_index + 1) {
        line_start(parsed, block.header)
            ..line_start(parsed, attached_comments_start(parsed, next.header))
    } else {
        let first =
            preceding_lines_start(parsed, block.header, |_, line| line.kind == LineKind::Blank);
        line_start(parsed, first)..parsed.bytes().len()
    };
    Ok(apply_replacements(parsed.bytes(), vec![(span, Vec::new())]))
}

fn host_block(parsed: &ParsedConfig, index: usize) -> Result<&Block, EditError> {
    let block = parsed
        .blocks()
        .get(index)
        .ok_or(EditError::BlockNotFound(index))?;
    match block.kind {
        BlockKind::Host => Ok(block),
        BlockKind::Match => Err(EditError::NotHostBlock(index)),
    }
}

fn host_pattern(
    parsed: &ParsedConfig,
    block_index: usize,
    pattern_index: usize,
) -> Result<(&Block, &Directive, &Arg), EditError> {
    let block = host_block(parsed, block_index)?;
    let missing = || EditError::PatternNotFound {
        block_index,
        pattern_index,
    };
    let header = parsed.lines()[block.header]
        .directive()
        .ok_or_else(missing)?;
    let pattern = header.args.get(pattern_index).ok_or_else(missing)?;
    Ok((block, header, pattern))
}

fn body<'a>(parsed: &'a ParsedConfig, block: &Block) -> &'a [Line] {
    &parsed.lines()[block.header + 1..block.end]
}

enum OptionLines<'a> {
    Absent,
    Value(&'a Line, &'a Arg),
    Unrepresentable(Vec<&'a Line>),
}

fn option_lines(lines: &[Line], key: OptionKey) -> OptionLines<'_> {
    let mut matching = lines
        .iter()
        .filter_map(|line| {
            let directive = line.directive()?;
            directive.is(key.keyword()).then_some((line, directive))
        })
        .peekable();
    let Some((first, directive)) = matching.next() else {
        return OptionLines::Absent;
    };
    if let [arg] = directive.args.as_slice()
        && matching.peek().is_none()
    {
        return OptionLines::Value(first, arg);
    }
    OptionLines::Unrepresentable(
        std::iter::once(first)
            .chain(matching.map(|(line, _)| line))
            .collect(),
    )
}

fn file_eol(parsed: &ParsedConfig) -> &'static [u8] {
    parsed
        .lines()
        .iter()
        .find(|line| line.eol != Eol::None)
        .map_or(Eol::Lf, |line| line.eol)
        .as_bytes()
}

fn line_indent<'a>(parsed: &'a ParsedConfig, line: &Line) -> Option<&'a [u8]> {
    let directive = line.directive()?;
    Some(&parsed.bytes()[line.span.start..directive.keyword_span.start])
}

fn file_indent(parsed: &ParsedConfig) -> &[u8] {
    parsed
        .blocks()
        .iter()
        .flat_map(|block| body(parsed, block))
        .filter_map(|line| line_indent(parsed, line))
        .find(|indent| !indent.is_empty())
        .unwrap_or(b"    ")
}

fn line_start(parsed: &ParsedConfig, index: usize) -> usize {
    parsed
        .lines()
        .get(index)
        .map_or(parsed.bytes().len(), |line| line.span.start)
}

fn preceding_lines_start(
    parsed: &ParsedConfig,
    header: usize,
    matches: impl Fn(usize, &Line) -> bool,
) -> usize {
    let mut start = header;
    while start > 0 && matches(start - 1, &parsed.lines()[start - 1]) {
        start -= 1;
    }
    start
}

fn attached_comments_start(parsed: &ParsedConfig, header: usize) -> usize {
    preceding_lines_start(parsed, header, |index, line| {
        line.kind == LineKind::Comment && parsed.line_bytes(index).starts_with(b"#")
    })
}

fn write_option(bytes: &mut Vec<u8>, indent: &[u8], key: OptionKey, formatted: &str) {
    bytes.extend_from_slice(indent);
    bytes.extend_from_slice(key.keyword().as_bytes());
    bytes.push(b' ');
    bytes.extend_from_slice(formatted.as_bytes());
}

type Replacement = (Range<usize>, Vec<u8>);

fn replace_arg(
    parsed: &ParsedConfig,
    line: &Line,
    arg: &Arg,
    value: &str,
) -> Result<Replacement, EditError> {
    let mut formatted = format_arg(value)?;
    let suffix = &parsed.bytes()[arg.span.end..line.span.end];
    // An unquoted final backslash would escape a space before another token or a comment.
    if formatted.ends_with('\\')
        && suffix.starts_with(b" ")
        && !suffix.iter().all(u8::is_ascii_whitespace)
    {
        formatted.push('\\');
    }
    Ok((arg.span.clone(), formatted.into_bytes()))
}

fn apply_replacements(bytes: &[u8], mut replacements: Vec<Replacement>) -> Vec<u8> {
    replacements.sort_unstable_by_key(|(range, _)| (range.start, range.end));
    let mut output = Vec::with_capacity(bytes.len());
    let mut end = 0;
    for (range, replacement) in replacements {
        debug_assert!(end <= range.start, "replacement ranges must not overlap");
        if end < range.start {
            output.extend_from_slice(&bytes[end..range.start]);
        }
        output.extend(replacement);
        end = end.max(range.end);
    }
    output.extend_from_slice(&bytes[end..]);
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::lexer::lex_line;

    const BASE: &str = concat!(
        "# Global settings\n",
        "ServerAliveInterval 60\n",
        "\n",
        "# Web server\n",
        "Host web\n",
        "    HostName web.example.com   # primary\n",
        "    user = alice\n",
        "    Port=2222\n",
        "    ForwardAgent yes\n",
        "\n",
        "Host db cache\n",
        "\tHostName 10.0.0.5\n",
        "\tIdentityFile \"~/.ssh/my key\"\n",
        "\n",
        "# Defaults for everything\n",
        "Host *\n",
        "    User root\n",
    );

    fn parsed(input: impl AsRef<[u8]>) -> ParsedConfig {
        ParsedConfig::parse(input.as_ref().to_vec())
    }

    fn edit(block_index: usize, changes: OptionChanges) -> HostEdit {
        HostEdit {
            block_index,
            pattern_index: 0,
            new_name: None,
            changes,
        }
    }

    fn assert_edit(input: impl AsRef<[u8]>, edit: HostEdit, expected: impl AsRef<[u8]>) {
        assert_eq!(
            apply_host_edit(&parsed(input), &edit).unwrap(),
            expected.as_ref()
        );
    }

    fn assert_base_edit(block_index: usize, changes: OptionChanges, expected: impl AsRef<[u8]>) {
        assert_edit(BASE, edit(block_index, changes), expected);
    }

    fn new_host() -> NewHost {
        NewHost {
            name: "new".into(),
            host_name: Some("h".into()),
            user: None,
            port: None,
            identity_file: None,
            password_authentication: None,
        }
    }

    fn assert_add(input: impl AsRef<[u8]>, host: &NewHost, expected: impl AsRef<[u8]>) {
        assert_eq!(add_host(&parsed(input), host).unwrap(), expected.as_ref());
    }

    fn assert_delete(
        input: impl AsRef<[u8]>,
        block_index: usize,
        pattern_index: usize,
        expected: impl AsRef<[u8]>,
    ) {
        assert_eq!(
            delete_pattern(&parsed(input), block_index, pattern_index).unwrap(),
            expected.as_ref()
        );
    }

    const WEB_NAME: &str = "    HostName web.example.com   # primary\n";
    const WEB_NEW_NAME: &str = "    HostName web2.example.com   # primary\n";
    const WEB_USER: &str = "    user = alice\n";
    const WEB_NEW_USER: &str = "    user = bob\n";
    const WEB_PORT: &str = "    Port=2222\n";
    const WEB_FORWARD_AGENT: &str = "    ForwardAgent yes\n";
    const WEB_FORWARD_AGENT_AND_IDENTITY: &str =
        "    ForwardAgent yes\n    IdentityFile ~/.ssh/id_ed25519\n";
    const DB_IDENTITY: &str = "\tIdentityFile \"~/.ssh/my key\"\n";
    const DB_IDENTITY_USER_PORT: &str = "\tIdentityFile \"~/.ssh/my key\"\n\tUser x\n\tPort 2200\n";

    #[test]
    fn changes_hostname_preserving_the_trailing_comment() {
        assert_base_edit(
            0,
            OptionChanges {
                host_name: Change::Set("web2.example.com".into()),
                ..Default::default()
            },
            BASE.replacen(WEB_NAME, WEB_NEW_NAME, 1),
        );
    }

    #[test]
    fn changes_user_preserving_spelling_and_separator() {
        assert_base_edit(
            0,
            OptionChanges {
                user: Change::Set("bob".into()),
                ..Default::default()
            },
            BASE.replacen(WEB_USER, WEB_NEW_USER, 1),
        );
    }

    #[test]
    fn removes_only_the_port_line() {
        assert_base_edit(
            0,
            OptionChanges {
                port: Change::Remove,
                ..Default::default()
            },
            BASE.replacen(WEB_PORT, "", 1),
        );
    }

    #[test]
    fn adds_identity_file_after_the_last_directive() {
        assert_base_edit(
            0,
            OptionChanges {
                identity_file: Change::Set("~/.ssh/id_ed25519".into()),
                ..Default::default()
            },
            BASE.replacen(WEB_FORWARD_AGENT, WEB_FORWARD_AGENT_AND_IDENTITY, 1),
        );
    }

    #[test]
    fn quotes_a_replacement_identity_file_path_with_spaces() {
        assert_base_edit(
            1,
            OptionChanges {
                identity_file: Change::Set("/keys/new key".into()),
                ..Default::default()
            },
            BASE.replacen(DB_IDENTITY, "\tIdentityFile \"/keys/new key\"\n", 1),
        );
    }

    #[test]
    fn adds_multiple_options_in_canonical_order_with_block_indentation() {
        assert_base_edit(
            1,
            OptionChanges {
                user: Change::Set("x".into()),
                port: Change::Set("2200".into()),
                ..Default::default()
            },
            BASE.replacen(DB_IDENTITY, DB_IDENTITY_USER_PORT, 1),
        );
    }

    #[test]
    fn edits_removes_and_adds_options_in_one_request() {
        assert_base_edit(
            0,
            OptionChanges {
                host_name: Change::Set("web2.example.com".into()),
                user: Change::Set("bob".into()),
                port: Change::Remove,
                identity_file: Change::Set("~/.ssh/id_ed25519".into()),
                password_authentication: Change::Keep,
            },
            BASE.replacen(WEB_NAME, WEB_NEW_NAME, 1)
                .replacen(WEB_USER, WEB_NEW_USER, 1)
                .replacen(WEB_PORT, "", 1)
                .replacen(WEB_FORWARD_AGENT, WEB_FORWARD_AGENT_AND_IDENTITY, 1),
        );
    }

    #[test]
    fn keeps_all_bytes_when_all_changes_are_keep_and_name_is_unchanged() {
        for new_name in [None, Some("web".into())] {
            assert_edit(
                BASE,
                HostEdit {
                    new_name,
                    ..edit(0, OptionChanges::default())
                },
                BASE,
            );
        }
        assert_base_edit(
            0,
            OptionChanges {
                identity_file: Change::Remove,
                ..Default::default()
            },
            BASE,
        );
    }

    #[test]
    fn keeps_the_raw_token_when_set_matches_the_parsed_value() {
        assert_base_edit(
            1,
            OptionChanges {
                identity_file: Change::Set("~/.ssh/my key".into()),
                ..Default::default()
            },
            BASE,
        );
        let input = "Host 'a'\n\tIdentityFile 'path with space'\n";
        assert_edit(
            input,
            HostEdit {
                new_name: Some("a".into()),
                ..edit(
                    0,
                    OptionChanges {
                        identity_file: Change::Set("path with space".into()),
                        ..Default::default()
                    },
                )
            },
            input,
        );
    }

    #[test]
    fn renames_either_alias_in_a_shared_header() {
        for (pattern_index, new_name, expected_header) in [
            (0, "database", "Host database cache\n"),
            (1, "redis", "Host db redis\n"),
        ] {
            assert_edit(
                BASE,
                HostEdit {
                    pattern_index,
                    new_name: Some(new_name.into()),
                    ..edit(1, OptionChanges::default())
                },
                BASE.replacen("Host db cache\n", expected_header, 1),
            );
        }
        assert_edit(
            b"  hOsT = 'a'  b\t# note\n",
            HostEdit {
                new_name: Some("it's".into()),
                ..edit(0, OptionChanges::default())
            },
            b"  hOsT = \"it's\"  b\t# note\n",
        );
    }

    #[test]
    fn renames_an_alias_and_changes_an_option_together() {
        assert_edit(
            BASE,
            HostEdit {
                pattern_index: 1,
                new_name: Some("redis".into()),
                ..edit(
                    1,
                    OptionChanges {
                        host_name: Change::Set("10.0.0.6".into()),
                        ..Default::default()
                    },
                )
            },
            BASE.replacen("Host db cache\n", "Host db redis\n", 1)
                .replacen("\tHostName 10.0.0.5\n", "\tHostName 10.0.0.6\n", 1),
        );
    }

    #[test]
    fn inserts_after_an_unterminated_header_without_terminating_the_file() {
        assert_edit(
            b"Host a",
            edit(
                0,
                OptionChanges {
                    host_name: Change::Set("h".into()),
                    ..Default::default()
                },
            ),
            b"Host a\n    HostName h",
        );
        assert_edit(
            b"Host a",
            HostEdit {
                new_name: Some("b".into()),
                ..edit(
                    0,
                    OptionChanges {
                        host_name: Change::Set("h".into()),
                        user: Change::Set("x".into()),
                        ..Default::default()
                    },
                )
            },
            b"Host b\n    HostName h\n    User x",
        );
    }

    #[test]
    fn removes_an_unterminated_last_option_line() {
        assert_edit(
            b"Host a\n    Port 22",
            edit(
                0,
                OptionChanges {
                    port: Change::Remove,
                    ..Default::default()
                },
            ),
            b"Host a\n",
        );
    }

    #[test]
    fn inserts_after_the_header_when_removing_the_only_unterminated_option() {
        assert_edit(
            b"Host a\n    Port 22",
            edit(
                0,
                OptionChanges {
                    host_name: Change::Set("h".into()),
                    port: Change::Remove,
                    ..Default::default()
                },
            ),
            b"Host a\n    HostName h\n",
        );
    }

    #[test]
    fn inserts_after_user_when_removing_an_unterminated_port() {
        assert_edit(
            b"Host a\n    User u\n    Port 22",
            edit(
                0,
                OptionChanges {
                    host_name: Change::Set("h".into()),
                    port: Change::Remove,
                    ..Default::default()
                },
            ),
            b"Host a\n    User u\n    HostName h\n",
        );
    }

    #[test]
    fn inserts_identity_file_after_user_when_removing_the_last_port() {
        assert_edit(
            b"Host a\n    User u\n    Port 22\n",
            edit(
                0,
                OptionChanges {
                    port: Change::Remove,
                    identity_file: Change::Set("k".into()),
                    ..Default::default()
                },
            ),
            b"Host a\n    User u\n    IdentityFile k\n",
        );
    }

    #[test]
    fn inserts_hostname_after_port_when_removing_the_last_user() {
        assert_edit(
            b"Host a\n    Port 22\n    User u\n",
            edit(
                0,
                OptionChanges {
                    host_name: Change::Set("h".into()),
                    user: Change::Remove,
                    ..Default::default()
                },
            ),
            b"Host a\n    Port 22\n    HostName h\n",
        );
    }

    #[test]
    fn preserves_the_blank_line_and_next_block_when_replacing_the_only_option() {
        assert_edit(
            b"Host a\n    Port 22\n\nHost b\n    HostName y\n",
            edit(
                0,
                OptionChanges {
                    user: Change::Set("u".into()),
                    port: Change::Remove,
                    ..Default::default()
                },
            ),
            b"Host a\n    User u\n\nHost b\n    HostName y\n",
        );
    }

    #[test]
    fn leaves_only_the_header_when_removing_all_four_options() {
        assert_edit(
            b"Host a\n    HostName h\n    User u\n    Port 22\n    IdentityFile k\n",
            edit(
                0,
                OptionChanges {
                    host_name: Change::Remove,
                    user: Change::Remove,
                    port: Change::Remove,
                    identity_file: Change::Remove,
                    password_authentication: Change::Keep,
                },
            ),
            b"Host a\n",
        );
    }

    #[test]
    fn inserts_with_crlf_after_user_when_removing_an_unterminated_port() {
        assert_edit(
            b"Host a\r\n    User u\r\n    Port 22",
            edit(
                0,
                OptionChanges {
                    host_name: Change::Set("h".into()),
                    port: Change::Remove,
                    ..Default::default()
                },
            ),
            b"Host a\r\n    User u\r\n    HostName h\r\n",
        );
    }

    #[test]
    fn keeps_lowercase_hostname_spelling() {
        assert_edit(
            b"Host a\nhostname old\n",
            edit(
                0,
                OptionChanges {
                    host_name: Change::Set("new".into()),
                    ..Default::default()
                },
            ),
            b"Host a\nhostname new\n",
        );
    }

    #[test]
    fn treats_duplicate_identity_files_as_read_only() {
        let input = b"Host a\r\n  IdentityFile x\r\n\tidentityfile y # \xff \r\n  User old\r\n";
        let config = parsed(input);
        assert_eq!(
            block_fields(&config, 0),
            Ok(BlockFields {
                host_name: FieldState::Absent,
                user: FieldState::Value("old".into()),
                port: FieldState::Absent,
                identity_file: FieldState::Unrepresentable {
                    lines: vec![
                        "  IdentityFile x".into(),
                        "\tidentityfile y # \u{fffd} ".into()
                    ],
                },
                password_authentication: FieldState::Absent,
            })
        );
        for change in [Change::Set("z".into()), Change::Remove] {
            let edit = edit(
                0,
                OptionChanges {
                    identity_file: change,
                    ..Default::default()
                },
            );
            assert_eq!(
                apply_host_edit(&config, &edit),
                Err(EditError::Unrepresentable(OptionKey::IdentityFile))
            );
        }
        assert_edit(
            input,
            edit(
                0,
                OptionChanges {
                    user: Change::Set("new".into()),
                    ..Default::default()
                },
            ),
            b"Host a\r\n  IdentityFile x\r\n\tidentityfile y # \xff \r\n  User new\r\n",
        );
    }

    #[test]
    fn treats_options_with_zero_or_multiple_arguments_as_read_only() {
        for line in ["HostName a b", "HostName"] {
            let config = parsed(format!("Host a\n{line}\n"));
            assert_eq!(
                block_fields(&config, 0).unwrap().get(OptionKey::HostName),
                &FieldState::Unrepresentable {
                    lines: vec![line.into()]
                },
            );
        }
    }

    #[test]
    fn reads_password_authentication_with_standard_and_alternate_syntax() {
        for line in [
            "PasswordAuthentication no",
            "passwordauthentication=no",
            "PASSWORDAuthentication = \"no\"",
        ] {
            let config = parsed(format!("Host web\n    {line}\n"));
            assert_eq!(
                block_fields(&config, 0)
                    .unwrap()
                    .get(OptionKey::PasswordAuthentication),
                &FieldState::Value("no".into()),
            );
        }
    }

    #[test]
    fn writes_password_authentication_last_in_a_new_host() {
        let host = NewHost {
            user: Some("alice".into()),
            port: Some("22".into()),
            identity_file: Some("my.key".into()),
            password_authentication: Some("no".into()),
            ..new_host()
        };
        assert_add(
            b"",
            &host,
            b"Host new\n    HostName h\n    User alice\n    Port 22\n    IdentityFile my.key\n    PasswordAuthentication no\n",
        );
    }

    #[test]
    fn adds_password_authentication_and_preserves_other_lines() {
        assert_edit(
            b"Host web\r\n\tUser alice  # \xff\r\n\r\nHost db\r\n  Port 22",
            edit(
                0,
                OptionChanges {
                    password_authentication: Change::Set("no".into()),
                    ..Default::default()
                },
            ),
            b"Host web\r\n\tUser alice  # \xff\r\n\tPasswordAuthentication no\r\n\r\nHost db\r\n  Port 22",
        );
    }

    #[test]
    fn changes_or_removes_password_authentication() {
        let input = b"Host web\n  passwordauthentication=\"yes\"  # policy\n  User alice\n";
        for (change, expected) in [
            (
                Change::Set("no".into()),
                b"Host web\n  passwordauthentication=no  # policy\n  User alice\n".as_slice(),
            ),
            (Change::Remove, b"Host web\n  User alice\n".as_slice()),
        ] {
            assert_edit(
                input,
                edit(
                    0,
                    OptionChanges {
                        password_authentication: change,
                        ..Default::default()
                    },
                ),
                expected,
            );
        }
    }

    #[test]
    fn rejects_duplicate_and_multi_argument_password_authentication_edits() {
        let key = OptionKey::PasswordAuthentication;
        for lines in [
            vec![
                "  PasswordAuthentication no",
                "  passwordauthentication=yes",
            ],
            vec!["  PasswordAuthentication no extra"],
        ] {
            let config = parsed(format!("Host web\n{}\n", lines.join("\n")));
            assert_eq!(
                block_fields(&config, 0).unwrap().get(key),
                &FieldState::Unrepresentable {
                    lines: lines.into_iter().map(String::from).collect(),
                },
            );
            assert_eq!(
                apply_host_edit(
                    &config,
                    &edit(
                        0,
                        OptionChanges {
                            password_authentication: Change::Remove,
                            ..Default::default()
                        },
                    ),
                ),
                Err(EditError::Unrepresentable(key)),
            );
        }
    }

    #[test]
    fn changes_only_the_target_block() {
        assert_base_edit(
            1,
            OptionChanges {
                host_name: Change::Set("new".into()),
                ..Default::default()
            },
            BASE.replacen("\tHostName 10.0.0.5\n", "\tHostName new\n", 1),
        );
    }

    #[test]
    fn rejects_invalid_targets_and_values() {
        let config = parsed(b"Host a\n  User \"\"\nMatch all\n  User root\n");
        for (block_index, pattern_index, error) in [
            (2, 0, EditError::BlockNotFound(2)),
            (1, 0, EditError::NotHostBlock(1)),
            (
                0,
                1,
                EditError::PatternNotFound {
                    block_index: 0,
                    pattern_index: 1,
                },
            ),
        ] {
            let request = HostEdit {
                pattern_index,
                ..edit(block_index, OptionChanges::default())
            };
            assert_eq!(apply_host_edit(&config, &request), Err(error.clone()));
            assert_eq!(
                delete_pattern(&config, block_index, pattern_index),
                Err(error)
            );
        }
        assert_eq!(block_fields(&config, 2), Err(EditError::BlockNotFound(2)));
        assert_eq!(block_fields(&config, 1), Err(EditError::NotHostBlock(1)));

        for value in ["", "a\nb", "a\0b", "a\u{85}b", "a\"b"] {
            let request = HostEdit {
                new_name: Some("renamed".into()),
                ..edit(
                    0,
                    OptionChanges {
                        host_name: Change::Set("valid".into()),
                        user: Change::Set(value.into()),
                        ..Default::default()
                    },
                )
            };
            assert!(matches!(
                apply_host_edit(&config, &request),
                Err(EditError::InvalidValue(_))
            ));
            assert_eq!(
                config.bytes(),
                b"Host a\n  User \"\"\nMatch all\n  User root\n"
            );
        }
        for name in ["a b", "-x", "a*"] {
            let request = HostEdit {
                new_name: Some(name.into()),
                ..edit(0, OptionChanges::default())
            };
            assert_eq!(
                apply_host_edit(&config, &request),
                Err(EditError::InvalidName(validate_nickname(name).unwrap_err())),
            );
        }
    }

    #[test]
    fn preserves_non_utf8_bytes_when_editing() {
        assert_edit(
            b"# \xff\nHost a\n   User=old\t# \xff\n",
            edit(
                0,
                OptionChanges {
                    user: Change::Set("new".into()),
                    ..Default::default()
                },
            ),
            b"# \xff\nHost a\n   User=new\t# \xff\n",
        );
    }

    #[test]
    fn edits_and_inserts_in_crlf_files() {
        let input = BASE.replace('\n', "\r\n");
        for (block_index, changes, expected) in [
            (
                0,
                OptionChanges {
                    host_name: Change::Set("web2.example.com".into()),
                    ..Default::default()
                },
                BASE.replacen(WEB_NAME, WEB_NEW_NAME, 1),
            ),
            (
                0,
                OptionChanges {
                    port: Change::Remove,
                    ..Default::default()
                },
                BASE.replacen(WEB_PORT, "", 1),
            ),
            (
                0,
                OptionChanges {
                    identity_file: Change::Set("~/.ssh/id_ed25519".into()),
                    ..Default::default()
                },
                BASE.replacen(WEB_FORWARD_AGENT, WEB_FORWARD_AGENT_AND_IDENTITY, 1),
            ),
            (
                1,
                OptionChanges {
                    user: Change::Set("x".into()),
                    port: Change::Set("2200".into()),
                    ..Default::default()
                },
                BASE.replacen(DB_IDENTITY, DB_IDENTITY_USER_PORT, 1),
            ),
        ] {
            assert_edit(
                &input,
                edit(block_index, changes),
                expected.replace('\n', "\r\n"),
            );
        }
    }

    #[test]
    fn adds_a_host_before_the_wildcard_blocks_attached_comment() {
        let host = NewHost {
            host_name: Some("h.example.com".into()),
            port: Some("22".into()),
            ..new_host()
        };
        assert_add(
            BASE,
            &host,
            BASE.replacen(
                "# Defaults for everything\n",
                "Host new\n    HostName h.example.com\n    Port 22\n\n# Defaults for everything\n",
                1,
            ),
        );
    }

    #[test]
    fn writes_all_new_options_in_canonical_order_and_quotes_values() {
        let host = NewHost {
            name: "it's".into(),
            user: Some("x".into()),
            port: Some("22".into()),
            identity_file: Some("/keys/my key".into()),
            ..new_host()
        };
        assert_add(
            b"",
            &host,
            b"Host \"it's\"\n    HostName h\n    User x\n    Port 22\n    IdentityFile \"/keys/my key\"\n",
        );
    }

    #[test]
    fn appends_a_host_at_eof_after_one_blank_line() {
        assert_add(
            b"Host a\n    HostName x\n",
            &new_host(),
            b"Host a\n    HostName x\n\nHost new\n    HostName h\n",
        );
    }

    #[test]
    fn adds_a_host_to_an_empty_file() {
        assert_add(b"", &new_host(), b"Host new\n    HostName h\n");
        assert_add(
            b"",
            &NewHost {
                host_name: None,
                ..new_host()
            },
            b"Host new\n",
        );
    }

    #[test]
    fn terminates_the_last_line_before_appending_a_host() {
        assert_add(
            b"Host a\n    HostName x",
            &new_host(),
            b"Host a\n    HostName x\n\nHost new\n    HostName h\n",
        );
    }

    #[test]
    fn reuses_the_trailing_blank_line_before_an_appended_host() {
        assert_add(
            b"Host a\n\n",
            &new_host(),
            b"Host a\n\nHost new\n    HostName h\n",
        );
        assert_add(
            b"Host a\n \t",
            &new_host(),
            b"Host a\n \t\nHost new\n    HostName h\n",
        );
    }

    #[test]
    fn adds_before_the_first_match_block() {
        assert_add(
            b"Host a\n\nMatch host x\n  User root\nHost *\n",
            &new_host(),
            b"Host a\n\nHost new\n  HostName h\n\nMatch host x\n  User root\nHost *\n",
        );
    }

    #[test]
    fn recognizes_mixed_wildcard_and_negated_blocks_as_insertion_boundaries() {
        for (input, expected) in [
            (
                "Host web *.internal\n",
                "Host new\n    HostName h\n\nHost web *.internal\n",
            ),
            ("Host !x y\n", "Host new\n    HostName h\n\nHost !x y\n"),
        ] {
            assert_add(input, &new_host(), expected);
        }
    }

    #[test]
    fn puts_a_blank_line_on_both_sides_of_an_inserted_host() {
        assert_add(
            b"Host a\n    HostName x\nHost *\n",
            &new_host(),
            b"Host a\n    HostName x\n\nHost new\n    HostName h\n\nHost *\n",
        );
    }

    #[test]
    fn inserts_before_attached_comments_but_after_an_indented_comment() {
        assert_add(
            b"Host a\n    # belongs to a\n# defaults\n# shared settings\nHost *\n",
            &new_host(),
            b"Host a\n    # belongs to a\n\nHost new\n    HostName h\n\n# defaults\n# shared settings\nHost *\n",
        );
    }

    #[test]
    fn uses_the_first_indented_body_directive_for_a_new_host() {
        assert_add(
            b"  Port 22\nHost a\nHostName x\n\tUser y\n",
            &new_host(),
            b"  Port 22\nHost a\nHostName x\n\tUser y\n\nHost new\n\tHostName h\n",
        );
    }

    #[test]
    fn writes_new_hosts_with_crlf_in_crlf_files() {
        assert_add(
            b"Host a\r\n\tUser x\r\n# note\r\nHost *\r\n",
            &NewHost { port: Some("22".into()), ..new_host() },
            b"Host a\r\n\tUser x\r\n\r\nHost new\r\n\tHostName h\r\n\tPort 22\r\n\r\n# note\r\nHost *\r\n",
        );
    }

    #[test]
    fn rejects_invalid_new_host_names_and_values() {
        let config = parsed(BASE);
        let host = NewHost {
            name: "-x".into(),
            ..new_host()
        };
        assert_eq!(
            add_host(&config, &host),
            Err(EditError::InvalidName(
                validate_nickname(&host.name).unwrap_err()
            )),
        );
        for value in ["", "a\rb", "a\"b"] {
            let host = NewHost {
                identity_file: Some(value.into()),
                ..new_host()
            };
            assert!(matches!(
                add_host(&config, &host),
                Err(EditError::InvalidValue(_))
            ));
        }
        assert_eq!(config.bytes(), BASE.as_bytes());
    }

    #[test]
    fn removes_either_alias_in_a_shared_header() {
        assert_delete(
            BASE,
            1,
            0,
            BASE.replacen("Host db cache\n", "Host cache\n", 1),
        );
        assert_delete(BASE, 1, 1, BASE.replacen("Host db cache\n", "Host db\n", 1));
    }

    #[test]
    fn removes_first_middle_and_last_patterns_with_the_specified_spacing() {
        let input = b"Host a  b   c # note";
        for (pattern_index, expected) in [
            (0, b"Host b   c # note".as_slice()),
            (1, b"Host a  c # note"),
            (2, b"Host a  b # note"),
        ] {
            assert_delete(input, 0, pattern_index, expected);
        }
    }

    const WEB_BLOCK: &str = concat!(
        "Host web\n",
        "    HostName web.example.com   # primary\n",
        "    user = alice\n",
        "    Port=2222\n",
        "    ForwardAgent yes\n",
        "\n",
    );

    #[test]
    fn deletes_a_single_alias_block_and_keeps_the_comment_above_it() {
        assert_delete(BASE, 0, 0, BASE.replacen(WEB_BLOCK, "", 1));
    }

    #[test]
    fn deletes_blank_lines_directly_above_the_last_block() {
        assert_delete(
            b"Host a\n    HostName x\n\nHost b\n    HostName y\n",
            1,
            0,
            b"Host a\n    HostName x\n",
        );
    }

    #[test]
    fn preserves_next_block_comments_and_deletes_indented_body_comments() {
        assert_delete(
            b"# first\nHost a\n  # body\n# next\n# settings\nMatch all\n",
            0,
            0,
            b"# first\n# next\n# settings\nMatch all\n",
        );
    }

    #[test]
    fn retains_global_options_after_removing_the_only_block() {
        assert_delete(
            b"ServerAliveInterval 60\n\n\nHost a\n    User x",
            0,
            0,
            b"ServerAliveInterval 60\n",
        );
    }

    #[test]
    fn deletes_a_block_in_a_crlf_file() {
        assert_delete(
            BASE.replace('\n', "\r\n"),
            0,
            0,
            BASE.replacen(WEB_BLOCK, "", 1).replace('\n', "\r\n"),
        );
    }

    const QUOTING_CASES: [(&str, &str); 6] = [
        ("example.com", "example.com"),
        ("a b", "\"a b\""),
        ("a\tb", "\"a\tb\""),
        ("a#b", "\"a#b\""),
        ("a'b", "\"a'b\""),
        ("=a", "\"=a\""),
    ];
    const BACKSLASH_CASES: [(&str, &str); 4] = [
        (r"C:\Users\me\key", r"C:\Users\me\key"),
        (r"\\server\share", r"\\\server\share"),
        ("it's", "\"it's\""),
        (r"dir with space\", r#""dir with space\\""#),
    ];

    #[test]
    fn quotes_arguments_only_when_required() {
        for (value, expected) in QUOTING_CASES {
            assert_eq!(format_arg(value).unwrap().as_bytes(), expected.as_bytes());
        }
    }

    #[test]
    fn escapes_backslashes_according_to_their_following_character() {
        for (value, expected) in BACKSLASH_CASES {
            assert_eq!(format_arg(value).unwrap().as_bytes(), expected.as_bytes());
        }
    }

    #[test]
    fn formatted_arguments_round_trip_through_the_lexer() {
        for value in QUOTING_CASES
            .into_iter()
            .chain(BACKSLASH_CASES)
            .map(|(value, _)| value)
            .chain([r"a\'b", r"a\\\\b", "한글 경로/키", r"a\", r"a\\", r"a\ b"])
        {
            let formatted = format_arg(value).unwrap();
            let line = format!("K {formatted}");
            let LineKind::Directive(directive) = lex_line(line.as_bytes(), 0) else {
                panic!("expected a directive for {line:?}");
            };
            assert_eq!(
                directive
                    .args
                    .iter()
                    .map(|arg| arg.value.as_str())
                    .collect::<Vec<_>>(),
                [value],
                "{line:?}"
            );
        }
    }

    #[test]
    fn derives_the_six_form_changes() {
        let value = FieldState::Value("old".into());
        let read_only = FieldState::Unrepresentable {
            lines: vec!["User a b".into()],
        };

        assert_eq!(Change::from_form(&read_only, "new"), Change::Keep);
        assert_eq!(Change::from_form(&value, "old"), Change::Keep);
        assert_eq!(Change::from_form(&FieldState::Absent, ""), Change::Keep);
        assert_eq!(Change::from_form(&value, ""), Change::Remove);
        assert_eq!(Change::from_form(&value, "new"), Change::Set("new".into()));
        assert_eq!(
            Change::from_form(&FieldState::Absent, "new"),
            Change::Set("new".into())
        );
    }

    #[test]
    fn keeps_tokens_and_comments_separate_from_a_replacement_ending_in_backslash() {
        assert_edit(
            b"Host a b # note\n    IdentityFile old # key\n",
            HostEdit {
                new_name: Some(r"a\".into()),
                ..edit(
                    0,
                    OptionChanges {
                        identity_file: Change::Set(r"path\".into()),
                        ..Default::default()
                    },
                )
            },
            b"Host a\\\\ b # note\n    IdentityFile path\\\\ # key\n",
        );
    }

    #[test]
    fn adds_options_at_the_end_of_a_removed_anchor_line() {
        assert_edit(
            b"Host a\nPort 22\r\n# tail\n",
            edit(
                0,
                OptionChanges {
                    port: Change::Remove,
                    user: Change::Set("x".into()),
                    ..Default::default()
                },
            ),
            b"Host a\nUser x\n# tail\n",
        );
    }
}
