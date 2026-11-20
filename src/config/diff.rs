//! Line diffs for reviewing configuration changes before saving.

use similar::{Algorithm, ChangeTag, capture_diff_slices, group_diff_ops};

/// Role of a line in the preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffTag {
    Context,
    Removed,
    Added,
}

/// One line of a change or its surrounding context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub tag: DiffTag,
    /// 1-based line number in the old file (None for Added).
    pub old_line: Option<usize>,
    /// 1-based line number in the new file (None for Removed).
    pub new_line: Option<usize>,
    /// Line content without its terminator (lossy UTF-8).
    pub text: String,
}

/// A group of nearby changes and their context lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffHunk {
    pub lines: Vec<DiffLine>,
}

/// Shows a line diff with `context` unchanged lines around each change.
/// Lines are compared as bytes including their terminators.
pub fn diff_lines(old: &[u8], new: &[u8], context: usize) -> Vec<DiffHunk> {
    if old == new {
        return Vec::new();
    }

    let old: Vec<_> = old.split_inclusive(|&byte| byte == b'\n').collect();
    let new: Vec<_> = new.split_inclusive(|&byte| byte == b'\n').collect();
    let ops = capture_diff_slices(Algorithm::Myers, &old, &new);
    // Grouping doubles the context to decide when to split adjacent hunks.
    let context = context.min(old.len().max(new.len()));
    group_diff_ops(ops, context)
        .into_iter()
        .map(|group| DiffHunk {
            lines: group
                .iter()
                .flat_map(|op| op.iter_changes(&old, &new))
                .map(|change| {
                    let line = change.value();
                    let content = line
                        .strip_suffix(b"\r\n")
                        .or_else(|| line.strip_suffix(b"\n"))
                        .unwrap_or(line);
                    DiffLine {
                        tag: match change.tag() {
                            ChangeTag::Equal => DiffTag::Context,
                            ChangeTag::Delete => DiffTag::Removed,
                            ChangeTag::Insert => DiffTag::Added,
                        },
                        old_line: change.old_index().map(|index| index + 1),
                        new_line: change.new_index().map(|index| index + 1),
                        text: String::from_utf8_lossy(content).into_owned(),
                    }
                })
                .collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{DiffTag::*, *};

    fn line(
        tag: DiffTag,
        old_line: Option<usize>,
        new_line: Option<usize>,
        text: &str,
    ) -> DiffLine {
        DiffLine {
            tag,
            old_line,
            new_line,
            text: text.into(),
        }
    }

    #[test]
    fn shows_an_inserted_line_and_shifted_line_numbers() {
        let hunks = diff_lines(b"a\nc\n", b"a\nb\nc\n", 1);
        assert_eq!(hunks.len(), 1);
        assert_eq!(
            hunks[0].lines,
            [
                line(Context, Some(1), Some(1), "a"),
                line(Added, None, Some(2), "b"),
                line(Context, Some(2), Some(3), "c"),
            ]
        );
    }

    #[test]
    fn shows_a_deleted_line_and_shifted_line_numbers() {
        let hunks = diff_lines(b"a\nb\nc\n", b"a\nc\n", 1);
        assert_eq!(hunks.len(), 1);
        assert_eq!(
            hunks[0].lines,
            [
                line(Context, Some(1), Some(1), "a"),
                line(Removed, Some(2), None, "b"),
                line(Context, Some(3), Some(2), "c"),
            ]
        );
    }

    #[test]
    fn lists_all_removed_lines_before_added_lines_in_a_replacement() {
        let hunks = diff_lines(b"keep\na\nb\n", b"keep\nc\nd\n", 0);
        assert_eq!(hunks.len(), 1);
        assert_eq!(
            hunks[0].lines,
            [
                line(Removed, Some(2), None, "a"),
                line(Removed, Some(3), None, "b"),
                line(Added, None, Some(2), "c"),
                line(Added, None, Some(3), "d"),
            ]
        );
    }

    #[test]
    fn splits_or_merges_hunks_based_on_context() {
        let old = b"before\na\n1\n2\n3\nb\nafter\n";
        let new = b"before\nc\n1\n2\n3\nd\nafter\n";
        let split = diff_lines(old, new, 1);
        assert_eq!(split.len(), 2);
        assert_eq!(
            split[0].lines.last(),
            Some(&line(Context, Some(3), Some(3), "1"))
        );
        assert_eq!(
            split[1].lines.first(),
            Some(&line(Context, Some(5), Some(5), "3"))
        );

        let merged = diff_lines(old, new, 2);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].lines.len(), 9);
        assert_eq!(merged[0].lines[4], line(Context, Some(4), Some(4), "2"));
        assert_eq!(diff_lines(old, new, usize::MAX), merged);
    }

    #[test]
    fn has_no_hunks_for_identical_bytes() {
        assert!(diff_lines(b"a\r\n\xff", b"a\r\n\xff", 2).is_empty());
        assert!(diff_lines(b"", b"", 2).is_empty());
    }

    #[test]
    fn shows_empty_file_creation_as_additions() {
        let hunks = diff_lines(b"", b"first\nlast", 2);
        assert_eq!(hunks.len(), 1);
        assert_eq!(
            hunks[0].lines,
            [
                line(Added, None, Some(1), "first"),
                line(Added, None, Some(2), "last")
            ]
        );
    }

    #[test]
    fn shows_complete_deletion_as_removals() {
        let hunks = diff_lines(b"first\nlast\n", b"", 2);
        assert_eq!(hunks.len(), 1);
        assert_eq!(
            hunks[0].lines,
            [
                line(Removed, Some(1), None, "first"),
                line(Removed, Some(2), None, "last")
            ]
        );
    }

    #[test]
    fn hides_crlf_terminators_and_displays_non_utf8_lossily() {
        let hunks = diff_lines(b"a\r\n\xff\r\n", b"a\r\n\xfe\r\n", 1);
        assert_eq!(hunks.len(), 1);
        assert_eq!(
            hunks[0].lines,
            [
                line(Context, Some(1), Some(1), "a"),
                line(Removed, Some(2), None, "\u{fffd}"),
                line(Added, None, Some(2), "\u{fffd}"),
            ]
        );
    }

    #[test]
    fn shows_a_change_from_lf_to_crlf() {
        let hunks = diff_lines(b"a\n", b"a\r\n", 2);
        assert_eq!(hunks.len(), 1);
        assert_eq!(
            hunks[0].lines,
            [
                line(Removed, Some(1), None, "a"),
                line(Added, None, Some(1), "a")
            ]
        );
    }

    #[test]
    fn shows_a_change_to_the_final_terminator() {
        let hunks = diff_lines(b"a\nlast\n", b"a\nlast", 2);
        assert_eq!(hunks.len(), 1);
        assert_eq!(
            hunks[0].lines,
            [
                line(Context, Some(1), Some(1), "a"),
                line(Removed, Some(2), None, "last"),
                line(Added, None, Some(2), "last"),
            ]
        );
    }
}
