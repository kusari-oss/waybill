//! Milestone 226: Pants Go target-declaration regex extractor.
//!
//! Reuses m225 pants_shell/build_dsl.rs's hybrid approach: an
//! anchoring regex finds each recognized target-type call at
//! line-start, then a char-by-char scan (respecting string
//! literals) locates the matching closing `)`. Focused per-kwarg
//! regexes then extract `name=` / `import_path=` / `main=` from
//! the call body.
//!
//! Constitution Principle I: no embedded Python interpreter,
//! no PyO3. Every accepted target follows a narrow, predictable
//! call shape.

use std::sync::OnceLock;

use regex::Regex;

use super::super::pants_common::find_matching_close_paren;
use super::{GoTargetDeclaration, GoTargetKind, GoTargetParseError};

/// Anchoring regex — matches `<target_type>(` at line-start.
fn anchor_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?m)^[ \t]*(go_mod|go_third_party_package|go_binary|go_package)\s*\(",
        )
        .expect("valid anchor regex")
    })
}

/// Extract `name="..."`. Requires the closing quote to be followed
/// by `,`, `)`, or EOL — so concat / f-string source is detected.
fn name_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r#"(?m)(?:^|[,\s(])name\s*=\s*(?:"([^"]*)"|'([^']*)')\s*(?:[,)]|$)"#,
        )
        .expect("valid name regex")
    })
}

/// Extract `import_path="..."`.
fn import_path_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r#"(?m)(?:^|[,\s(])import_path\s*=\s*(?:"([^"]*)"|'([^']*)')\s*(?:[,)]|$)"#,
        )
        .expect("valid import_path regex")
    })
}

/// Detects `import_path=` KEY presence (even when value is not a
/// string literal). If key is seen but value isn't parsed as a
/// literal by `import_path_regex`, we report `NonStringLiteralValue`.
fn import_path_key_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?m)(?:^|[,\s(])import_path\s*=\s*")
            .expect("valid import_path-key regex")
    })
}

/// Extract `main="..."`.
fn main_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r#"(?m)(?:^|[,\s(])main\s*=\s*(?:"([^"]*)"|'([^']*)')\s*(?:[,)]|$)"#,
        )
        .expect("valid main regex")
    })
}

fn main_key_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?m)(?:^|[,\s(])main\s*=\s*")
            .expect("valid main-key regex")
    })
}

/// Extract every recognized Pants Go target declaration from a
/// BUILD-file blob. Per-target fail-open: each result is either a
/// successfully-parsed declaration or a typed parse error. The
/// caller (orchestrator) logs errors as WARN and continues with
/// successful entries.
pub(crate) fn extract_targets(
    bytes: &[u8],
) -> Vec<Result<GoTargetDeclaration, GoTargetParseError>> {
    let text = match std::str::from_utf8(bytes) {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    for m in anchor_regex().captures_iter(text) {
        let full_match = m.get(0).expect("group 0 present");
        let target_type = m.get(1).expect("group 1 present").as_str();
        let kind = match target_type {
            "go_mod" => GoTargetKind::GoMod,
            "go_third_party_package" => GoTargetKind::GoThirdPartyPackage,
            "go_binary" => GoTargetKind::GoBinary,
            "go_package" => GoTargetKind::GoPackage,
            _ => continue,
        };
        let open_paren_offset = full_match.end() - 1;
        let start_line = 1u32
            + text[..full_match.start()]
                .bytes()
                .filter(|&b| b == b'\n')
                .count() as u32;

        let close = match find_matching_close_paren(text.as_bytes(), open_paren_offset) {
            Some(c) => c,
            None => {
                out.push(Err(GoTargetParseError::UnbalancedParens { line: start_line }));
                continue;
            }
        };
        let body = &text[open_paren_offset + 1..close];

        let name = name_regex().captures(body).and_then(|c| {
            c.get(1)
                .or_else(|| c.get(2))
                .map(|m| m.as_str().to_string())
        });

        // import_path (only meaningful for go_third_party_package;
        // extracted opportunistically for all kinds and simply
        // discarded downstream by the ownership_index).
        let import_path_lit = import_path_regex().captures(body).and_then(|c| {
            c.get(1)
                .or_else(|| c.get(2))
                .map(|m| m.as_str().to_string())
        });
        let import_path_key_seen = import_path_key_regex().is_match(body);

        // main (only meaningful for go_binary).
        let main_lit = main_regex().captures(body).and_then(|c| {
            c.get(1)
                .or_else(|| c.get(2))
                .map(|m| m.as_str().to_string())
        });
        let main_key_seen = main_key_regex().is_match(body);

        // Kind-based validation:
        match kind {
            GoTargetKind::GoMod => {
                // name is optional (defaults to "mod"); no other kwargs required.
            }
            GoTargetKind::GoThirdPartyPackage => {
                if name.is_none() {
                    out.push(Err(GoTargetParseError::MissingRequiredKwarg {
                        line: start_line,
                    }));
                    continue;
                }
                if import_path_lit.is_none() {
                    if import_path_key_seen {
                        out.push(Err(GoTargetParseError::NonStringLiteralValue {
                            line: start_line,
                            snippet: body.chars().take(80).collect(),
                        }));
                    } else {
                        out.push(Err(GoTargetParseError::MissingRequiredKwarg {
                            line: start_line,
                        }));
                    }
                    continue;
                }
            }
            GoTargetKind::GoBinary => {
                if name.is_none() {
                    out.push(Err(GoTargetParseError::MissingRequiredKwarg {
                        line: start_line,
                    }));
                    continue;
                }
                if main_lit.is_none() {
                    if main_key_seen {
                        out.push(Err(GoTargetParseError::NonStringLiteralValue {
                            line: start_line,
                            snippet: body.chars().take(80).collect(),
                        }));
                    } else {
                        out.push(Err(GoTargetParseError::MissingRequiredKwarg {
                            line: start_line,
                        }));
                    }
                    continue;
                }
            }
            GoTargetKind::GoPackage => {
                // name is optional (defaults to dir basename); no other kwargs required.
            }
        }

        out.push(Ok(GoTargetDeclaration {
            kind,
            name,
            import_path: import_path_lit,
            main: main_lit,
            start_line,
        }));
    }
    out
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn parse_one(input: &str) -> Result<GoTargetDeclaration, GoTargetParseError> {
        let mut out = extract_targets(input.as_bytes());
        assert_eq!(out.len(), 1, "expected exactly 1 result, got {}", out.len());
        out.remove(0)
    }

    // 1. Valid go_mod with explicit name.
    #[test]
    fn valid_go_mod_explicit_name() {
        let decl = parse_one(r#"go_mod(name="mod")"#).unwrap();
        assert_eq!(decl.kind, GoTargetKind::GoMod);
        assert_eq!(decl.name.as_deref(), Some("mod"));
    }

    // 2. Valid go_mod default-name (no name kwarg).
    #[test]
    fn valid_go_mod_default_name() {
        let decl = parse_one(r#"go_mod()"#).unwrap();
        assert_eq!(decl.kind, GoTargetKind::GoMod);
        assert!(decl.name.is_none());
    }

    // 3. Valid go_third_party_package with both kwargs.
    #[test]
    fn valid_go_third_party_package_both_kwargs() {
        let decl = parse_one(
            r#"go_third_party_package(name="cobra", import_path="github.com/spf13/cobra")"#,
        )
        .unwrap();
        assert_eq!(decl.kind, GoTargetKind::GoThirdPartyPackage);
        assert_eq!(decl.name.as_deref(), Some("cobra"));
        assert_eq!(decl.import_path.as_deref(), Some("github.com/spf13/cobra"));
    }

    // 4. Valid go_binary with main=".".
    #[test]
    fn valid_go_binary_main_dot() {
        let decl = parse_one(r#"go_binary(name="frontend", main=".")"#).unwrap();
        assert_eq!(decl.kind, GoTargetKind::GoBinary);
        assert_eq!(decl.name.as_deref(), Some("frontend"));
        assert_eq!(decl.main.as_deref(), Some("."));
    }

    // 5. Valid go_binary with main="./cmd/foo".
    #[test]
    fn valid_go_binary_main_subdir() {
        let decl =
            parse_one(r#"go_binary(name="cli", main="./cmd/foo")"#).unwrap();
        assert_eq!(decl.main.as_deref(), Some("./cmd/foo"));
    }

    // 6. Valid go_package default-name.
    #[test]
    fn valid_go_package_default_name() {
        let decl = parse_one(r#"go_package()"#).unwrap();
        assert_eq!(decl.kind, GoTargetKind::GoPackage);
        assert!(decl.name.is_none());
    }

    // 7. go_third_party_package missing import_path returns MissingRequiredKwarg.
    #[test]
    fn go_third_party_package_missing_import_path_err() {
        let err = parse_one(r#"go_third_party_package(name="x")"#).unwrap_err();
        assert!(matches!(err, GoTargetParseError::MissingRequiredKwarg { .. }));
    }

    // 8. Variable-reference import_path returns NonStringLiteralValue.
    #[test]
    fn variable_reference_import_path_err() {
        let err = parse_one(
            r#"go_third_party_package(name="x", import_path=IMPORT_VAR)"#,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            GoTargetParseError::NonStringLiteralValue { .. }
        ));
    }

    // 9. Unbalanced parens returns UnbalancedParens.
    #[test]
    fn unbalanced_parens_err() {
        let text = r#"go_third_party_package(name="x", import_path="y"
# forgot to close the paren
another_top_level_thing = 42"#;
        let out = extract_targets(text.as_bytes());
        assert_eq!(out.len(), 1);
        assert!(matches!(
            out[0],
            Err(GoTargetParseError::UnbalancedParens { .. })
        ));
    }

    // 10. Three valid targets in one blob all parse.
    #[test]
    fn three_valid_targets_all_parse() {
        let text = r#"
go_mod(name="mod")

go_third_party_package(name="cobra", import_path="github.com/spf13/cobra")

go_binary(name="frontend", main="./cmd/frontend")
"#;
        let out = extract_targets(text.as_bytes());
        assert_eq!(out.len(), 3);
        assert!(out.iter().all(|r| r.is_ok()));
    }

    // 11. Comment line inside target body is ignored.
    #[test]
    fn comment_inside_target_ignored() {
        let text = r#"go_third_party_package(
    # this is a comment
    name="cobra",
    import_path="github.com/spf13/cobra",
)"#;
        let decl = parse_one(text).unwrap();
        assert_eq!(decl.name.as_deref(), Some("cobra"));
    }

    // 12. Zero recognized targets → empty vec.
    #[test]
    fn zero_recognized_targets_returns_empty() {
        let text = r#"
shell_source(name="foo", source="foo.sh")
python_source(name="bar", source="bar.py")
"#;
        assert!(extract_targets(text.as_bytes()).is_empty());
    }
}
