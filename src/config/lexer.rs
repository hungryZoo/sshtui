//! Line-by-line tokenization of OpenSSH configuration.

use super::model::{Arg, Directive, LineKind};

/// Classifies one line without its terminator, using absolute spans from `offset`.
pub fn lex_line(content: &[u8], offset: usize) -> LineKind {
    let end = content
        .iter()
        .rposition(|byte| !matches!(byte, b' ' | b'\t' | b'\r' | 0x0c))
        .map_or(0, |index| index + 1);
    let trimmed = &content[..end];
    let line = trim_start(trimmed, |byte| matches!(byte, b' ' | b'\t' | b'\r'));
    match line.first() {
        None => return LineKind::Blank,
        Some(b'#') => return LineKind::Comment,
        Some(_) => {}
    }

    let keyword_len = line
        .iter()
        .position(|&byte| is_separator(byte) || matches!(byte, b'=' | b'"'))
        .unwrap_or(line.len());
    if keyword_len == 0 {
        return LineKind::Malformed;
    }
    let keyword_start = offset + (trimmed.len() - line.len());
    let (keyword, rest) = line.split_at(keyword_len);
    let rest = trim_start(rest, is_separator);
    let mut rest = rest.strip_prefix(b"=").unwrap_or(rest);
    let mut args = Vec::new();
    let mut comment_span = None;

    loop {
        rest = trim_start(rest, is_separator);
        let start = offset + (trimmed.len() - rest.len());
        match rest.first() {
            None => break,
            Some(b'#') => {
                comment_span = Some(start..offset + content.len());
                break;
            }
            Some(_) => {}
        }

        let Some((value, len)) = lex_arg(rest) else {
            return LineKind::Malformed;
        };
        args.push(Arg {
            value,
            span: start..start + len,
        });
        rest = &rest[len..];
    }

    LineKind::Directive(Directive {
        keyword: String::from_utf8_lossy(keyword).into_owned(),
        keyword_span: keyword_start..keyword_start + keyword_len,
        args,
        comment_span,
    })
}

fn is_separator(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t')
}

fn trim_start(bytes: &[u8], predicate: impl Fn(u8) -> bool) -> &[u8] {
    let start = bytes
        .iter()
        .position(|&byte| !predicate(byte))
        .unwrap_or(bytes.len());
    &bytes[start..]
}

fn lex_arg(bytes: &[u8]) -> Option<(String, usize)> {
    let mut value = Vec::new();
    let mut quote = None;
    let mut indexed_bytes = bytes.iter().copied().enumerate().peekable();

    while let Some((index, byte)) = indexed_bytes.next() {
        match byte {
            b'\\' => {
                if let Some(&(_, next)) = indexed_bytes.peek()
                    && (matches!(next, b'\'' | b'"' | b'\\') || (quote.is_none() && next == b' '))
                {
                    value.push(next);
                    indexed_bytes.next();
                } else {
                    value.push(byte);
                }
            }
            b' ' | b'\t' if quote.is_none() => {
                return Some((String::from_utf8_lossy(&value).into_owned(), index));
            }
            b'\'' | b'"' if quote.is_none() => quote = Some(byte),
            delimiter if quote == Some(delimiter) => quote = None,
            _ => value.push(byte),
        }
    }

    quote
        .is_none()
        .then(|| (String::from_utf8_lossy(&value).into_owned(), bytes.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn directive(content: &[u8], offset: usize) -> Directive {
        match lex_line(content, offset) {
            LineKind::Directive(directive) => directive,
            other => panic!("expected a directive, got {other:?}"),
        }
    }

    fn values(directive: &Directive) -> Vec<&str> {
        directive
            .args
            .iter()
            .map(|arg| arg.value.as_str())
            .collect()
    }

    #[test]
    fn classifies_blank_and_comment_lines() {
        for input in [b"".as_slice(), b" \t\r", b" \t\x0c\r"] {
            assert_eq!(lex_line(input, 0), LineKind::Blank);
        }
        for input in [b"# note".as_slice(), b" \t\r# note"] {
            assert_eq!(lex_line(input, 0), LineKind::Comment);
        }
    }

    #[test]
    fn keyword_and_argument_spans_are_absolute() {
        let bytes = b"ignored\n  HostName example.com";
        let parsed = directive(&bytes[8..], 8);
        assert_eq!(parsed.keyword, "HostName");
        assert_eq!(parsed.keyword_span, 10..18);
        assert_eq!(&bytes[parsed.keyword_span], b"HostName");
        assert_eq!(parsed.args.len(), 1);
        assert_eq!(parsed.args[0].value, "example.com");
        assert_eq!(parsed.args[0].span, 19..30);
        assert_eq!(&bytes[parsed.args[0].span.clone()], b"example.com");
    }

    #[test]
    fn accepts_spacing_around_one_equals_separator() {
        for input in [
            "Port=22",
            "Port = 22",
            "Port =22",
            "Port= 22",
            "Port\t=\t22",
        ] {
            let parsed = directive(input.as_bytes(), 0);
            assert_eq!(parsed.keyword, "Port");
            assert_eq!(values(&parsed), ["22"]);
            assert_eq!(&input[parsed.args[0].span.clone()], "22");
        }
        let parsed = directive(b"Port == 22", 0);
        assert_eq!(parsed.keyword, "Port");
        assert_eq!(values(&parsed), ["=", "22"]);
        assert_eq!(parsed.args[0].span, 6..7);
    }

    #[test]
    fn removes_quotes_and_keeps_them_in_argument_spans() {
        let input = br#"IdentityFile "~/my keys/id" 'other id' a"b c"d "" '"x"'"#;
        let parsed = directive(input, 0);
        assert_eq!(
            values(&parsed),
            ["~/my keys/id", "other id", "ab cd", "", r#""x""#]
        );
        assert_eq!(&input[parsed.args[0].span.clone()], br#""~/my keys/id""#);
        assert_eq!(&input[parsed.args[1].span.clone()], b"'other id'");
        assert_eq!(&input[parsed.args[2].span.clone()], br#"a"b c"d"#);
    }

    #[test]
    fn unescapes_quotes_backslashes_and_unquoted_spaces() {
        let input =
            br#"Host "a\"b" C:\Users\me\key \\\\server\\share my\ key 'x\'y' "my\ key" a\#b"#;
        let parsed = directive(input, 0);
        assert_eq!(
            values(&parsed),
            [
                r#"a"b"#,
                r"C:\Users\me\key",
                r"\\server\share",
                "my key",
                "x'y",
                r"my\ key",
                r"a\#b",
            ]
        );
        assert_eq!(&input[parsed.args[3].span.clone()], br"my\ key");
        assert_eq!(values(&directive(b"Host a\\\tb \\", 0)), [r"a\", "b", r"\"]);
    }

    #[test]
    fn comments_start_at_a_token_boundary_and_keep_trailing_whitespace() {
        let input = b"Port 22 # note";
        let parsed = directive(input, 0);
        assert_eq!(values(&parsed), ["22"]);
        assert_eq!(&input[parsed.comment_span.unwrap()], b"# note");

        let input = b"Port 22 # note \t\r\x0c";
        let parsed = directive(input, 0);
        assert_eq!(&input[parsed.comment_span.unwrap()], b"# note \t\r\x0c");

        let parsed = directive(b"Host a#b '#c' d\\ #e", 0);
        assert_eq!(values(&parsed), ["a#b", "#c", "d #e"]);
        assert_eq!(parsed.comment_span, None);
    }

    #[test]
    fn unclosed_quotes_make_the_whole_directive_malformed() {
        for input in [r#"Host a "b"#, "Host pre'fix", r#"Host "abc\" "#] {
            assert_eq!(lex_line(input.as_bytes(), 0), LineKind::Malformed);
        }
    }

    #[test]
    fn accepts_directives_without_arguments() {
        let parsed = directive(b"Host", 0);
        assert_eq!(parsed.keyword, "Host");
        assert!(parsed.args.is_empty());
        assert_eq!(parsed.comment_span, None);
    }

    #[test]
    fn compares_keywords_case_insensitively() {
        let parsed = directive(b"HostName example.com", 0);
        assert!(parsed.is("hostname"));
        assert!(parsed.is("HOSTNAME"));
        assert!(!parsed.is("host"));
    }
}
