//! JSONC (JSON with Comments) stripper — milestone 106 phase 2A;
//! extended in milestone 189 (#555) to also strip trailing commas.
//!
//! Used by the Bun lockfile reader (`bun.lock`) which is JSONC-formatted
//! per Bun 1.2+. Every real-world `bun.lock` carries at least the
//! top-of-file `// bun: lockfileVersion: 1` marker comment that
//! `serde_json::from_str` rejects AND (per Fable5's 2026-07-13 audit
//! finding) trailing commas inside `packages` map objects that trigger
//! silent parser rejection + entire-resolved-tree truncation. Pre-m189
//! bug: 282 components emitted instead of 1584 for the same repo.
//!
//! Behavior:
//! - Strips `// ... \n` line comments and `/* ... */` block comments.
//! - Strips trailing commas: `,` immediately preceding (post-whitespace)
//!   `]` or `}`, both at top level and nested. Only outside string
//!   literals.
//! - Preserves newlines from stripped comments as `\n` so
//!   `serde_json`'s line/column error positions stay accurate.
//! - String literal contents are PRESERVED verbatim — comment markers
//!   and commas inside `"..."` strings are NOT stripped.
//!
//! Pattern mirrors `gem::strip_ruby_comment` and
//! `golang::legacy::strip_line_comment` (single-line strippers already
//! in mikebom). This helper extends those with block-comment +
//! string-boundary awareness + trailing-comma stripping.

/// Strip C-style line comments (`//`), block comments (`/* */`), AND
/// trailing commas from a JSONC source string, returning a String
/// suitable for passing to `serde_json::from_str`.
///
/// String-literal contents are preserved verbatim — comments and
/// commas inside `"..."` strings are NOT stripped.
///
/// Newlines from `//` and `/* */` are preserved as `\n` so line/column
/// info in serde_json error messages still points at correct positions.
///
/// Trailing-comma pass added in milestone 189 (#555) — every real-world
/// `bun.lock` produced by Bun 1.2+ has trailing commas inside its
/// `packages` map, causing pre-m189 silent parser rejection + resolved-
/// tree truncation.
#[allow(dead_code)] // wired by US2 (T024); see milestone 106 plan
pub(super) fn strip_comments(input: &str) -> String {
    let after_comments = strip_comments_only(input);
    strip_trailing_commas(&after_comments)
}

/// Pre-m189 behavior — comment stripping only. Kept as a private helper
/// so the two passes remain independently testable.
fn strip_comments_only(input: &str) -> String {
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

/// Strip trailing commas from JSONC input — `,` immediately followed
/// (past whitespace) by `]` or `}` is removed. String-literal contents
/// are preserved verbatim.
///
/// Milestone 189 (#555) — fix for Fable5's audit finding that mikebom
/// silently truncated bun.lock parses to manifest-tier only (282
/// components vs 1584 for the full transitive graph).
fn strip_trailing_commas(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(bytes.len());
    let mut state = TrailingState::Normal;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        match state {
            TrailingState::Normal => {
                if b == b'"' {
                    out.push('"');
                    state = TrailingState::InString;
                    i += 1;
                } else if b == b',' {
                    // Peek forward past whitespace to see if the next
                    // non-whitespace byte is `]` or `}`. If so, drop
                    // the comma.
                    let mut j = i + 1;
                    while j < bytes.len() && is_json_whitespace(bytes[j]) {
                        j += 1;
                    }
                    if j < bytes.len() && (bytes[j] == b']' || bytes[j] == b'}') {
                        // Drop the comma — preserve the whitespace
                        // between it and the closer so error line/column
                        // info stays accurate.
                        i += 1;
                    } else {
                        out.push(',');
                        i += 1;
                    }
                } else {
                    out.push(b as char);
                    i += 1;
                }
            }
            TrailingState::InString => {
                if b == b'\\' && i + 1 < bytes.len() {
                    out.push(b as char);
                    out.push(bytes[i + 1] as char);
                    i += 2;
                } else if b == b'"' {
                    out.push('"');
                    state = TrailingState::Normal;
                    i += 1;
                } else {
                    out.push(b as char);
                    i += 1;
                }
            }
        }
    }
    out
}

#[derive(Copy, Clone, Debug)]
enum TrailingState {
    Normal,
    InString,
}

/// JSON whitespace per RFC 8259 §2 — space, tab, LF, CR.
fn is_json_whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r')
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

    // ── Milestone 189 (#555) — trailing-comma stripping ──

    #[test]
    fn strips_trailing_comma_before_close_brace() {
        let input = r#"{"a": 1, "b": 2,}"#;
        let out = strip_comments(input);
        // Trailing comma before `}` removed.
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["a"], serde_json::json!(1));
        assert_eq!(parsed["b"], serde_json::json!(2));
    }

    #[test]
    fn strips_trailing_comma_before_close_bracket() {
        let input = r#"[1, 2, 3,]"#;
        let out = strip_comments(input);
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed, serde_json::json!([1, 2, 3]));
    }

    #[test]
    fn strips_trailing_commas_with_whitespace_between() {
        let input = "{\n  \"a\": 1,\n  \"b\": 2  ,\n\n}";
        let out = strip_comments(input);
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["a"], serde_json::json!(1));
    }

    #[test]
    fn does_not_strip_comma_inside_string() {
        let input = r#"{"key": "a,b,c,","other": 1}"#;
        let out = strip_comments(input);
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["key"], serde_json::json!("a,b,c,"));
    }

    #[test]
    fn does_not_strip_non_trailing_commas() {
        // Comma between elements MUST be preserved.
        let input = r#"[1, 2, 3]"#;
        assert_eq!(strip_comments(input), "[1, 2, 3]");
    }

    #[test]
    fn bun_lock_realistic_snippet_parses() {
        // Real-world bun.lock shape — comment + trailing commas at
        // multiple nesting levels.
        let input = r#"// bun: lockfileVersion: 1
{
  "lockfileVersion": 1,
  "workspaces": {
    "": {
      "name": "myapp",
      "dependencies": {
        "lodash": "4.17.21",
      },
    },
  },
  "packages": {
    "lodash": ["lodash@4.17.21", "sha512-abc"],
  },
}
"#;
        let out = strip_comments(input);
        let parsed: serde_json::Value = serde_json::from_str(&out)
            .expect("bun.lock realistic snippet should parse after strip");
        assert_eq!(parsed["lockfileVersion"], serde_json::json!(1));
        assert!(parsed["packages"]["lodash"].is_array());
    }

    #[test]
    fn nested_trailing_commas_all_stripped() {
        let input = r#"{"outer":{"inner":[1,2,],},}"#;
        let out = strip_comments(input);
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["outer"]["inner"], serde_json::json!([1, 2]));
    }
}
