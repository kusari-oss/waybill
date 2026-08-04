//! Shared helpers for Pants-family readers (m225 pants_shell + m226
//! pants_go, plus any future Pants BUILD-walker consumer).
//!
//! Extracted from m225/m226 after those two milestones shipped
//! byte-identical implementations of the two functions in this
//! module. Per m225/m226 §Follow-ups, the promotion trigger was
//! "when 2+ Pants readers exist" — that threshold is now met.
//!
//! What's shared (this module):
//! - [`discover_build_files`] — walks the scan root via `safe_walk`,
//!   returns every path whose file name is exactly `"BUILD"`.
//!   Respects symlink-cycle guard, `--exclude-path`, depth limits.
//! - [`find_matching_close_paren`] — byte-level scanner that finds
//!   the matching `)` for an opening paren, respecting single- and
//!   double-quoted string literals (with `\` escape). Used by each
//!   reader's regex-scoped target-declaration extractor to locate
//!   call-body boundaries without a full Python parser
//!   (Constitution Principle I — no embedded interpreter).
//!
//! What stays per-reader:
//! - The regex-anchored target-type match (`pants_shell` recognizes
//!   `shell_source`/`shell_sources`/`shunit2_test`/`shunit2_tests`;
//!   `pants_go` recognizes `go_mod`/`go_third_party_package`/
//!   `go_binary`/`go_package`).
//! - Per-kwarg regex extraction — the two readers extract different
//!   kwarg sets (`source=`/`sources=[...]` vs `import_path=`/`main=`).
//! - The typed `TargetKind` / `TargetDeclaration` / `TargetParseError`
//!   enums — each reader's target-type list is closed and distinct,
//!   so a generic version would either lose type safety or gain
//!   awkward `&'static str` dispatch.

use std::path::{Path, PathBuf};

use super::exclude_path::ExclusionSet;
use crate::scan_fs::walk::{safe_walk, WalkConfig};

/// Discover every `BUILD` file under `scan_root` via `safe_walk`
/// (respects symlink-cycle guard, `--exclude-path`, depth limits).
/// Callers filter the results by target-type further downstream.
pub(crate) fn discover_build_files(
    scan_root: &Path,
    exclude_set: &ExclusionSet,
) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let cfg = WalkConfig {
        max_depth: 32,
        should_skip: &|_candidate, _rootfs| false,
        exclude_set,
    };
    safe_walk(scan_root, &cfg, |path| {
        if path.is_file()
            && path.file_name().and_then(|s| s.to_str()) == Some("BUILD")
        {
            out.push(path.to_path_buf());
        }
    });
    out
}

/// Walk `bytes` from `start` (which MUST point at an opening `(`)
/// and return the byte offset of the matching closing `)`. Respects
/// string literals: single- and double-quoted (with `\` escape).
/// Returns `None` on unbalanced parens / EOF-in-string.
///
/// Used by each Pants reader's regex-scoped extractor to locate the
/// call-body of a target-function invocation without a full Python
/// parser (Constitution Principle I).
pub(crate) fn find_matching_close_paren(bytes: &[u8], start: usize) -> Option<usize> {
    debug_assert_eq!(bytes.get(start), Some(&b'('));
    let mut depth: i32 = 0;
    let mut i = start;
    let mut in_str: Option<u8> = None;
    let mut escape = false;
    while i < bytes.len() {
        let c = bytes[i];
        if let Some(q) = in_str {
            if escape {
                escape = false;
            } else if c == b'\\' {
                escape = true;
            } else if c == q {
                in_str = None;
            }
            i += 1;
            continue;
        }
        match c {
            b'"' | b'\'' => {
                in_str = Some(c);
            }
            b'(' | b'[' | b'{' => {
                depth += 1;
            }
            b')' | b']' | b'}' => {
                depth -= 1;
                if depth == 0 && c == b')' {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn find_matching_close_paren_simple() {
        let s = b"foo(bar)";
        assert_eq!(find_matching_close_paren(s, 3), Some(7));
    }

    #[test]
    fn find_matching_close_paren_nested() {
        let s = b"foo(bar(baz)qux)";
        assert_eq!(find_matching_close_paren(s, 3), Some(15));
    }

    #[test]
    fn find_matching_close_paren_string_literal_hides_paren() {
        let s = br#"foo(bar=")")"#;
        // Outer `(` at 3; the `)` at 9 is inside a string literal
        // and must be skipped; the outer `)` at 11 is the match.
        assert_eq!(find_matching_close_paren(s, 3), Some(11));
    }

    #[test]
    fn find_matching_close_paren_single_quoted() {
        let s = br#"foo(bar=')' )"#;
        assert_eq!(find_matching_close_paren(s, 3), Some(12));
    }

    #[test]
    fn find_matching_close_paren_escape_in_string() {
        let s = br#"foo(bar="a\"b" )"#;
        // Escaped quote inside string must NOT close the string.
        assert_eq!(find_matching_close_paren(s, 3), Some(15));
    }

    #[test]
    fn find_matching_close_paren_unbalanced_returns_none() {
        let s = b"foo(bar";
        assert_eq!(find_matching_close_paren(s, 3), None);
    }

    #[test]
    fn find_matching_close_paren_eof_in_string_returns_none() {
        let s = br#"foo(bar="unterminated"#;
        assert_eq!(find_matching_close_paren(s, 3), None);
    }

    #[test]
    fn discover_build_files_returns_empty_for_nonexistent_dir() {
        let exclude_set = ExclusionSet::new_empty();
        let files = discover_build_files(
            Path::new("/nonexistent-path-xyzzy"),
            &exclude_set,
        );
        assert!(files.is_empty());
    }
}
