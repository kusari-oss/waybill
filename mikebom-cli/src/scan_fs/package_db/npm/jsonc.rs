//! JSONC (JSON with Comments) stripper — milestone 106 phase 2A.
//!
//! Used by the Bun lockfile reader (`bun.lock`) which is JSONC-formatted
//! per Bun 1.2+. Every real-world `bun.lock` carries at least the
//! top-of-file `// bun: lockfileVersion: 1` marker comment that
//! `serde_json::from_str` rejects.
//!
//! Behavior:
//! - Strips `// ... \n` line comments and `/* ... */` block comments.
//! - Preserves newlines from stripped comments as `\n` so
//!   `serde_json`'s line/column error positions stay accurate.
//! - String literal contents are PRESERVED verbatim — comment markers
//!   inside `"..."` strings are NOT stripped.
//!
//! Pattern mirrors `gem::strip_ruby_comment` and
//! `golang::legacy::strip_line_comment` (single-line strippers already
//! in mikebom). This helper extends those with block-comment + string-
//! boundary awareness.

/// Strip C-style line comments (`//`) and block comments (`/* */`) from
/// a JSONC source string, returning a String suitable for passing to
/// `serde_json::from_str`.
///
/// String-literal contents are preserved verbatim — comments inside
/// `"..."` strings are NOT stripped.
///
/// Newlines from `//` and `/* */` are preserved as `\n` so line/column
/// info in serde_json error messages still points at correct positions.
#[allow(dead_code)] // wired by US2 (T024); see milestone 106 plan
pub(super) fn strip_comments(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut state = State::Normal;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        match state {
            State::Normal => {
                if b == b'"' {
                    out.push('"');
                    state = State::InString;
                    i += 1;
                } else if b == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                    state = State::LineComment;
                    i += 2;
                } else if b == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
                    state = State::BlockComment;
                    i += 2;
                } else {
                    out.push(b as char);
                    i += 1;
                }
            }
            State::InString => {
                if b == b'\\' && i + 1 < bytes.len() {
                    // Preserve escape sequence as two bytes.
                    out.push(b as char);
                    out.push(bytes[i + 1] as char);
                    i += 2;
                } else if b == b'"' {
                    out.push('"');
                    state = State::Normal;
                    i += 1;
                } else {
                    out.push(b as char);
                    i += 1;
                }
            }
            State::LineComment => {
                if b == b'\n' {
                    out.push('\n');
                    state = State::Normal;
                }
                // Otherwise drop the char.
                i += 1;
            }
            State::BlockComment => {
                if b == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                    state = State::Normal;
                    i += 2;
                } else if b == b'\n' {
                    out.push('\n');
                    i += 1;
                } else {
                    i += 1;
                }
            }
        }
    }
    out
}

#[derive(Copy, Clone, Debug)]
enum State {
    Normal,
    InString,
    LineComment,
    BlockComment,
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn strip_line_comment_basic() {
        assert_eq!(strip_comments("// foo\nbar"), "\nbar");
    }

    #[test]
    fn strip_block_comment_basic() {
        assert_eq!(strip_comments("a /* x */ b"), "a  b");
    }

    #[test]
    fn preserves_strings() {
        assert_eq!(
            strip_comments(r#""// not a comment""#),
            r#""// not a comment""#
        );
    }

    #[test]
    fn preserves_strings_with_block_marker() {
        assert_eq!(
            strip_comments(r#""/* still string */""#),
            r#""/* still string */""#
        );
    }

    #[test]
    fn escaped_quote_in_string() {
        // Input: "he said \"hi\""//comment
        // After strip: "he said \"hi\""
        let input = r#""he said \"hi\""//comment"#;
        let expected = r#""he said \"hi\"""#;
        assert_eq!(strip_comments(input), expected);
    }

    #[test]
    fn multiline_block_preserves_newlines() {
        // Newlines inside block comments are kept so serde_json error
        // line/column info stays usable.
        assert_eq!(strip_comments("a /* line1\nline2 */ b"), "a \n b");
    }

    #[test]
    fn unterminated_block_comment() {
        // Eats to EOF, no panic.
        assert_eq!(strip_comments("/* unterminated"), "");
    }

    #[test]
    fn top_of_file_bun_marker() {
        // The exact pattern every real-world bun.lock starts with.
        assert_eq!(
            strip_comments("// bun: lockfileVersion: 1\n{}"),
            "\n{}"
        );
    }

    #[test]
    fn adjacent_comment_types() {
        // Line comment followed by block comment followed by text.
        assert_eq!(strip_comments("// line\n/* block */text"), "\ntext");
    }

    #[test]
    fn empty_input() {
        assert_eq!(strip_comments(""), "");
    }
}
