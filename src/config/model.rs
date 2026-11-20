//! File and block types that preserve the original bytes.

use std::ops::Range;

use super::lexer::lex_line;

/// Line terminator of a physical line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Eol {
    None,
    Lf,
    CrLf,
}

impl Eol {
    /// Returns the terminator's bytes.
    pub fn as_bytes(self) -> &'static [u8] {
        match self {
            Self::None => b"",
            Self::Lf => b"\n",
            Self::CrLf => b"\r\n",
        }
    }
}

/// One argument of a directive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arg {
    /// Unquoted, unescaped value (lossy UTF-8).
    pub value: String,
    /// Byte range in the file covering the raw token, including quotes and escapes.
    pub span: Range<usize>,
}

/// A directive keyword, its arguments, and an optional trailing comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Directive {
    /// Keyword as written (lossy UTF-8).
    pub keyword: String,
    /// Byte range in the file covering the keyword.
    pub keyword_span: Range<usize>,
    /// Arguments in their original order.
    pub args: Vec<Arg>,
    /// Trailing comment from its `#` to the end of the line content, including whitespace.
    pub comment_span: Option<Range<usize>>,
}

impl Directive {
    /// Compares the keyword using ASCII case insensitivity.
    pub fn is(&self, keyword: &str) -> bool {
        self.keyword.eq_ignore_ascii_case(keyword)
    }
}

/// Syntactic classification of one physical line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineKind {
    Blank,
    Comment,
    Directive(Directive),
    Malformed,
}

/// A physical line and its location in the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// Byte range of the line content, excluding the terminator.
    pub span: Range<usize>,
    /// Terminator immediately after the content, if present.
    pub eol: Eol,
    /// Syntactic classification of the content.
    pub kind: LineKind,
}

impl Line {
    /// Returns the byte range including the terminator.
    pub fn full_span(&self) -> Range<usize> {
        self.span.start..self.span.end + self.eol.as_bytes().len()
    }

    /// Returns the parsed directive for a directive line.
    pub fn directive(&self) -> Option<&Directive> {
        match &self.kind {
            LineKind::Directive(directive) => Some(directive),
            _ => None,
        }
    }
}

/// Directive that starts a block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Host,
    Match,
}

/// Range of lines belonging to a `Host` or `Match` directive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// Type of the header directive.
    pub kind: BlockKind,
    /// Index of the header line in `ParsedConfig::lines()`.
    pub header: usize,
    /// Exclusive end line index.
    pub end: usize,
}

/// A parsed config file that keeps the original bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedConfig {
    bytes: Vec<u8>,
    lines: Vec<Line>,
    blocks: Vec<Block>,
}

impl ParsedConfig {
    /// Parses any byte sequence, marking unparseable lines as `LineKind::Malformed`.
    pub fn parse(bytes: Vec<u8>) -> Self {
        let mut lines = Vec::new();
        let mut offset = 0;
        for raw_line in bytes.split_inclusive(|&byte| byte == b'\n') {
            let (content, eol) = if let Some(content) = raw_line.strip_suffix(b"\r\n") {
                (content, Eol::CrLf)
            } else if let Some(content) = raw_line.strip_suffix(b"\n") {
                (content, Eol::Lf)
            } else {
                (raw_line, Eol::None)
            };

            lines.push(Line {
                span: offset..offset + content.len(),
                eol,
                kind: lex_line(content, offset),
            });
            offset += raw_line.len();
        }

        let mut blocks: Vec<Block> = Vec::new();
        for (header, line) in lines.iter().enumerate() {
            let Some(directive) = line.directive() else {
                continue;
            };
            let kind = if directive.is("Host") {
                BlockKind::Host
            } else if directive.is("Match") {
                BlockKind::Match
            } else {
                continue;
            };

            if let Some(previous) = blocks.last_mut() {
                previous.end = header;
            }
            blocks.push(Block {
                kind,
                header,
                end: lines.len(),
            });
        }

        Self {
            bytes,
            lines,
            blocks,
        }
    }

    /// Returns the complete original file, including line terminators.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns all physical lines in file order.
    pub fn lines(&self) -> &[Line] {
        &self.lines
    }

    /// Returns all `Host` and `Match` blocks in file order.
    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    /// Returns the content bytes of line `index` without the terminator.
    pub fn line_bytes(&self, index: usize) -> &[u8] {
        &self.bytes[self.lines[index].span.clone()]
    }

    /// Returns the index of the block that contains line `index`, if any.
    pub fn block_of_line(&self, index: usize) -> Option<usize> {
        let block_index = self
            .blocks
            .partition_point(|block| block.header <= index)
            .checked_sub(1)?;
        (index < self.blocks[block_index].end).then_some(block_index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crlf_is_excluded_from_content_and_arguments() {
        let input = b"Host one\r\n  HostName a\rb\r\r\n";
        let parsed = ParsedConfig::parse(input.to_vec());
        assert_eq!(parsed.lines().len(), 2);
        assert_eq!(parsed.lines()[0].eol, Eol::CrLf);
        assert_eq!(parsed.lines()[1].eol, Eol::CrLf);
        assert_eq!(parsed.line_bytes(0), b"Host one");
        assert_eq!(parsed.line_bytes(1), b"  HostName a\rb\r");
        assert_eq!(parsed.lines()[0].span, 0..8);
        assert_eq!(parsed.lines()[1].span, 10..25);
        let first_arg = &parsed.lines()[0].directive().unwrap().args[0];
        assert_eq!(first_arg.value, "one");
        let second_arg = &parsed.lines()[1].directive().unwrap().args[0];
        assert_eq!(second_arg.value, "a\rb");
        assert_eq!(&input[second_arg.span.clone()], b"a\rb");
    }

    #[test]
    fn handles_empty_mixed_and_unterminated_files() {
        let empty = ParsedConfig::parse(Vec::new());
        assert!(empty.lines().is_empty());
        assert!(empty.blocks().is_empty());
        assert_eq!(empty.block_of_line(0), None);

        let parsed = ParsedConfig::parse(b"# comment\n\r\nHost last".to_vec());
        let endings: Vec<_> = parsed.lines().iter().map(|line| line.eol).collect();
        assert_eq!(endings, [Eol::Lf, Eol::CrLf, Eol::None]);
        assert_eq!(parsed.line_bytes(1), b"");
        assert_eq!(parsed.line_bytes(2), b"Host last");

        let terminated = ParsedConfig::parse(b"\n\n".to_vec());
        assert_eq!(terminated.lines().len(), 2);
        assert_eq!(terminated.lines()[0].full_span(), 0..1);
        assert_eq!(terminated.lines()[1].full_span(), 1..2);
    }

    #[test]
    fn full_spans_partition_the_original_bytes() {
        for bytes in [
            b"".as_slice(),
            b"\n",
            b"\r\n",
            b"Host one",
            b"# global\r\nHost a\nHost b c\r\nMatch host x\nHost *\n",
            b"Port == 22  # note \r\nHost a\"b c\"d\nHost 'incomplete\r",
            b"Host \xffa\r\n\xfe \x00\n\r",
        ] {
            let parsed = ParsedConfig::parse(bytes.to_vec());
            assert_eq!(parsed.bytes(), bytes);
            let mut end = 0;
            let mut reconstructed = Vec::new();
            for line in parsed.lines() {
                let span = line.full_span();
                assert_eq!(span.start, end);
                end = span.end;
                reconstructed.extend_from_slice(&parsed.bytes()[span]);
            }
            assert_eq!(end, bytes.len());
            assert_eq!(reconstructed, bytes);
        }

        let parsed = ParsedConfig::parse(b"Host \xffa\n".to_vec());
        let arg = &parsed.lines()[0].directive().unwrap().args[0];
        assert_eq!(arg.value, "\u{fffd}a");
        assert_eq!(&parsed.bytes()[arg.span.clone()], b"\xffa");
    }

    #[test]
    fn block_boundaries_follow_host_and_match_headers() {
        let input = b"Port 22\nHost a\n User someone\nHost b c\nMatch host x\n # comment\nHost *";
        let parsed = ParsedConfig::parse(input.to_vec());
        assert_eq!(
            parsed.blocks(),
            [
                Block {
                    kind: BlockKind::Host,
                    header: 1,
                    end: 3,
                },
                Block {
                    kind: BlockKind::Host,
                    header: 3,
                    end: 4,
                },
                Block {
                    kind: BlockKind::Match,
                    header: 4,
                    end: 6,
                },
                Block {
                    kind: BlockKind::Host,
                    header: 6,
                    end: 7,
                }
            ]
        );
        let membership: Vec<_> = (0..=7).map(|index| parsed.block_of_line(index)).collect();
        assert_eq!(
            membership,
            [
                None,
                Some(0),
                Some(0),
                Some(1),
                Some(2),
                Some(2),
                Some(3),
                None
            ]
        );
    }
}
