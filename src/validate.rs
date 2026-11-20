//! Validation of SSH connection form fields.

use std::fmt;

/// A failed validation rule for an SSH connection form field.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ValidationError {
    #[error("{field} is required")]
    Empty { field: Field },
    #[error("{field} must not start with '-'")]
    LeadingDash { field: Field },
    #[error("{field} must not contain {ch:?}")]
    ForbiddenChar { field: Field, ch: char },
    #[error("{field} must not contain whitespace")]
    Whitespace { field: Field },
    #[error("{field} must not contain control characters")]
    ControlChar { field: Field },
    #[error("port must be a number between 1 and 65535")]
    InvalidPort,
    #[error("nickname {0:?} is already in use")]
    DuplicateNickname(String),
}

/// Form field a validation error refers to. Displays as a lowercase label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Nickname,
    User,
    Address,
    Port,
    KeyFile,
}

impl fmt::Display for Field {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Nickname => "nickname",
            Self::User => "user",
            Self::Address => "address",
            Self::Port => "port",
            Self::KeyFile => "key file",
        })
    }
}

/// Validates the required nickname for use as an SSH host alias.
pub fn validate_nickname(nickname: &str) -> Result<(), ValidationError> {
    let field = Field::Nickname;
    if nickname.is_empty() {
        return Err(ValidationError::Empty { field });
    }
    validate_token(nickname, field)?;
    if nickname.starts_with('=') {
        return Err(ValidationError::ForbiddenChar { field, ch: '=' });
    }
    reject_forbidden_chars(nickname, field, "*?!#\"")
}

/// Checks for an exact, case-sensitive nickname match among existing names.
pub fn check_nickname_unique<'a, I>(nickname: &str, existing: I) -> Result<(), ValidationError>
where
    I: IntoIterator<Item = &'a str>,
{
    if existing.into_iter().any(|name| name == nickname) {
        return Err(ValidationError::DuplicateNickname(nickname.to_owned()));
    }
    Ok(())
}

/// Validates the required SSH address field.
pub fn validate_address(address: &str) -> Result<(), ValidationError> {
    let field = Field::Address;
    if address.is_empty() {
        return Err(ValidationError::Empty { field });
    }
    validate_token(address, field)?;
    reject_forbidden_chars(address, field, "@#\"")
}

/// Validates the optional SSH username field.
pub fn validate_user(user: &str) -> Result<(), ValidationError> {
    let field = Field::User;
    validate_token(user, field)?;
    reject_forbidden_chars(user, field, "#\"")
}

/// Parses an optional port from ASCII digits in the range 1 to 65535.
pub fn parse_port(port: &str) -> Result<Option<u16>, ValidationError> {
    if port.is_empty() {
        return Ok(None);
    }
    if !port.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ValidationError::InvalidPort);
    }
    match port.parse() {
        Ok(value) if value != 0 => Ok(Some(value)),
        _ => Err(ValidationError::InvalidPort),
    }
}

/// Validates the optional key file path, allowing spaces.
pub fn validate_key_file(path: &str) -> Result<(), ValidationError> {
    let field = Field::KeyFile;
    reject_control_chars(path, field)?;
    reject_forbidden_chars(path, field, "\"")?;
    reject_leading_dash(path, field)
}

fn validate_token(input: &str, field: Field) -> Result<(), ValidationError> {
    reject_control_chars(input, field)?;
    if input.chars().any(char::is_whitespace) {
        return Err(ValidationError::Whitespace { field });
    }
    reject_leading_dash(input, field)
}

fn reject_control_chars(input: &str, field: Field) -> Result<(), ValidationError> {
    if input.chars().any(char::is_control) {
        return Err(ValidationError::ControlChar { field });
    }
    Ok(())
}

fn reject_leading_dash(input: &str, field: Field) -> Result<(), ValidationError> {
    if input.starts_with('-') {
        return Err(ValidationError::LeadingDash { field });
    }
    Ok(())
}

fn reject_forbidden_chars(
    input: &str,
    field: Field,
    forbidden: &str,
) -> Result<(), ValidationError> {
    if let Some(ch) = input.chars().find(|&ch| forbidden.contains(ch)) {
        return Err(ValidationError::ForbiddenChar { field, ch });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Field, ValidationError::*, *};

    #[test]
    fn nickname_accepts_valid_aliases() {
        for input in ["web-1", "db.prod_2", "서버1", "a=b", "a-b"] {
            assert_eq!(validate_nickname(input), Ok(()), "{input:?}");
        }
    }

    #[test]
    fn nickname_rejects_invalid_aliases() {
        let field = Field::Nickname;
        for (input, error) in [
            ("", Empty { field }),
            ("-x", LeadingDash { field }),
            ("a b", Whitespace { field }),
            (" x", Whitespace { field }),
            ("x ", Whitespace { field }),
            ("a\tb", ControlChar { field }),
            ("a\nb", ControlChar { field }),
            ("a*", ForbiddenChar { field, ch: '*' }),
            ("a?b", ForbiddenChar { field, ch: '?' }),
            ("!a", ForbiddenChar { field, ch: '!' }),
            ("a#b", ForbiddenChar { field, ch: '#' }),
            ("=a", ForbiddenChar { field, ch: '=' }),
            ("a\"b", ForbiddenChar { field, ch: '"' }),
        ] {
            assert_eq!(validate_nickname(input), Err(error), "{input:?}");
        }
    }

    #[test]
    fn nickname_reports_errors_in_rule_order() {
        let field = Field::Nickname;
        for (input, error) in [
            ("-a \t#", ControlChar { field }),
            ("-a #", Whitespace { field }),
            ("-a#", LeadingDash { field }),
            ("=a*", ForbiddenChar { field, ch: '=' }),
            ("a#*", ForbiddenChar { field, ch: '#' }),
        ] {
            assert_eq!(validate_nickname(input), Err(error), "{input:?}");
        }
    }

    #[test]
    fn nickname_uniqueness_is_exact_and_does_not_validate_format() {
        let existing = ["web", "db"];
        assert_eq!(
            check_nickname_unique("web", existing),
            Err(DuplicateNickname("web".into()))
        );
        for input in ["Web", "we", "-x"] {
            assert_eq!(check_nickname_unique(input, existing), Ok(()), "{input:?}");
        }
    }

    #[test]
    fn address_accepts_domains_and_ip_addresses() {
        for input in [
            "example.com",
            "192.168.0.1",
            "::1",
            "fe80::1%en0",
            "xn--9t4b11yi5a.kr",
        ] {
            assert_eq!(validate_address(input), Ok(()), "{input:?}");
        }
    }

    #[test]
    fn address_rejects_invalid_values_in_rule_order() {
        let field = Field::Address;
        for (input, error) in [
            ("", Empty { field }),
            ("-oProxyCommand=x", LeadingDash { field }),
            ("a b", Whitespace { field }),
            ("user@host", ForbiddenChar { field, ch: '@' }),
            ("host#suffix", ForbiddenChar { field, ch: '#' }),
            ("host\"", ForbiddenChar { field, ch: '"' }),
            ("host\nProxyCommand x", ControlChar { field }),
            ("-host @user", Whitespace { field }),
            ("-host@user", LeadingDash { field }),
            ("host#@user", ForbiddenChar { field, ch: '#' }),
        ] {
            assert_eq!(validate_address(input), Err(error), "{input:?}");
        }
    }

    #[test]
    fn user_accepts_an_empty_field_and_usernames() {
        for input in ["", "root", "first.last", "user@domain"] {
            assert_eq!(validate_user(input), Ok(()), "{input:?}");
        }
    }

    #[test]
    fn user_rejects_invalid_values_in_rule_order() {
        let field = Field::User;
        for (input, error) in [
            ("-l", LeadingDash { field }),
            ("a b", Whitespace { field }),
            ("a\nb", ControlChar { field }),
            ("a#b", ForbiddenChar { field, ch: '#' }),
            ("a\"b", ForbiddenChar { field, ch: '"' }),
            ("-a \n#", ControlChar { field }),
            ("-a #", Whitespace { field }),
            ("-a#", LeadingDash { field }),
            ("a\"#", ForbiddenChar { field, ch: '"' }),
        ] {
            assert_eq!(validate_user(input), Err(error), "{input:?}");
        }
    }

    #[test]
    fn port_accepts_empty_and_in_range_ascii_numbers() {
        for (input, expected) in [
            ("", None),
            ("1", Some(1)),
            ("22", Some(22)),
            ("65535", Some(65535)),
            ("022", Some(22)),
        ] {
            assert_eq!(parse_port(input), Ok(expected), "{input:?}");
        }
    }

    #[test]
    fn port_rejects_invalid_numbers_and_overflow() {
        for input in [
            "0",
            "65536",
            "-1",
            "+22",
            "22 ",
            "2a",
            "９",
            "99999999999999999999",
        ] {
            assert_eq!(parse_port(input), Err(InvalidPort), "{input:?}");
        }
    }

    #[test]
    fn key_file_accepts_empty_and_paths_with_spaces() {
        for input in ["", "~/.ssh/id_ed25519", "/path/with space/key", " key "] {
            assert_eq!(validate_key_file(input), Ok(()), "{input:?}");
        }
    }

    #[test]
    fn key_file_rejects_invalid_values_in_rule_order() {
        let field = Field::KeyFile;
        for (input, error) in [
            ("a\"b", ForbiddenChar { field, ch: '"' }),
            ("a\nb", ControlChar { field }),
            ("-key", LeadingDash { field }),
            ("-\"\n", ControlChar { field }),
            ("-\"", ForbiddenChar { field, ch: '"' }),
        ] {
            assert_eq!(validate_key_file(input), Err(error), "{input:?}");
        }
    }

    #[test]
    fn field_display_uses_lowercase_labels() {
        for (field, label) in [
            (Field::Nickname, "nickname"),
            (Field::User, "user"),
            (Field::Address, "address"),
            (Field::Port, "port"),
            (Field::KeyFile, "key file"),
        ] {
            assert_eq!(field.to_string(), label);
        }
    }
}
