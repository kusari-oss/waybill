//! Milestone 225: Pants BUILD-file target-declaration regex extractor.
//!
//! Per research.md §R2: hybrid approach that finds each recognized
//! target-function call via an anchoring regex, then walks the source
//! character-by-character to find the matching closing `)` (respecting
//! string literals so a `)` inside a quoted value doesn't terminate
//! early). The extracted "call body" is then scanned with focused
//! per-kwarg regexes for `name=` / `source=` / `sources=[...]`.
//!
//! This is more robust than one monolithic regex — it handles
//! multi-line kwargs, arbitrary kwarg ordering, and trailing commas
//! without the pathological backtracking a giant single-pattern would
//! risk.
//!
//! Constitution Principle I: no embedded Python interpreter, no PyO3.
//! Every accepted target follows a narrow, predictable call shape
//! that this module parses in ~150 LOC.

use std::sync::OnceLock;

use regex::Regex;

use super::super::pants_common::find_matching_close_paren;
use super::{ShellTargetKind, TargetDeclaration, TargetParseError, TargetSource};

/// Anchoring regex — matches `<target_type>(` at the start of a line
/// (after any leading whitespace). Captures the target-function name.
fn anchor_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?m)^[ \t]*(shell_source|shell_sources|shunit2_test|shunit2_tests)\s*\(")
            .expect("valid anchor regex")
    })
}

/// Extract the `name="..."` kwarg from a call body. Returns `None`
/// when absent (which is legal for `shell_sources` / `shunit2_tests`).
/// Handles single- and double-quoted string literals.
fn name_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?m)(?:^|[,\s(])name\s*=\s*(?:"([^"]*)"|'([^']*)')"#)
            .expect("valid name regex")
    })
}

/// Extract the `source="..."` kwarg from a call body. Returns `None`
/// when absent. Requires the closing quote to be followed by `,`, `)`,
/// whitespace-then-`,`/`)`, or end-of-string — so a concat like
/// `"scripts/" + "deploy.sh"` does NOT match (the trailing ` +` is
/// invalid) and downstream logic reports `NonStringLiteralSource`.
fn source_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?m)(?:^|[,\s(])source\s*=\s*(?:"([^"]*)"|'([^']*)')\s*(?:[,)]|$)"#)
            .expect("valid source regex")
    })
}

/// Detect a bad-shape `source=` (variable ref / concat / f-string).
/// If `source=` appears but NOT as a string literal, we want to
/// report `NonStringLiteralSource`.
fn source_key_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?m)(?:^|[,\s(])source\s*=\s*").expect("valid source-key regex")
    })
}

/// Extract the `sources=[...]` list body (contents between the outer
/// `[` and `]`). Returns `None` when the kwarg is absent.
fn sources_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?ms)(?:^|[,\s(])sources\s*=\s*\[([^\]]*)\]")
            .expect("valid sources regex")
    })
}

/// Extract every string-literal element from a `sources=[...]` list body.
fn list_element_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#""([^"]*)"|'([^']*)'"#).expect("valid list-element regex")
    })
}

/// Extract all recognized shell target declarations from a BUILD-file
/// blob. Each element is either a successfully-parsed declaration or
/// a parse error naming the offending line. The caller (orchestrator)
/// logs errors as WARN and continues with successful entries.
pub(crate) fn extract_targets(
    bytes: &[u8],
) -> Vec<Result<TargetDeclaration, TargetParseError>> {
    let text = match std::str::from_utf8(bytes) {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    for m in anchor_regex().captures_iter(text) {
        let full_match = m.get(0).expect("group 0 present");
        let target_type = m.get(1).expect("group 1 present").as_str();
        let kind = match target_type {
            "shell_source" => ShellTargetKind::ShellSource,
            "shell_sources" => ShellTargetKind::ShellSources,
            "shunit2_test" => ShellTargetKind::Shunit2Test,
            "shunit2_tests" => ShellTargetKind::Shunit2Tests,
            _ => continue,
        };
        // The anchor ends AT the `(` — back off by one to point AT it.
        let open_paren_offset = full_match.end() - 1;
        let start_line = 1u32
            + text[..full_match.start()]
                .bytes()
                .filter(|&b| b == b'\n')
                .count() as u32;

        let close = match find_matching_close_paren(text.as_bytes(), open_paren_offset) {
            Some(c) => c,
            None => {
                out.push(Err(TargetParseError::UnbalancedParens { line: start_line }));
                continue;
            }
        };
        // Body is everything strictly between the outer parens.
        let body = &text[open_paren_offset + 1..close];

        // Extract kwargs.
        let name = name_regex().captures(body).and_then(|c| {
            c.get(1)
                .or_else(|| c.get(2))
                .map(|m| m.as_str().to_string())
        });
        let source_lit = source_regex().captures(body).and_then(|c| {
            c.get(1)
                .or_else(|| c.get(2))
                .map(|m| m.as_str().to_string())
        });
        let source_key_seen = source_key_regex().is_match(body);
        let sources_body = sources_regex()
            .captures(body)
            .and_then(|c| c.get(1).map(|m| m.as_str()));

        // Kind-based routing:
        // - Single-source kinds (shell_source, shunit2_test) REQUIRE `source=`.
        // - Multi-sources kinds (shell_sources, shunit2_tests) use `sources=`
        //   OR default to empty (Pants default applies at resolver time).
        let source = match kind {
            ShellTargetKind::ShellSource | ShellTargetKind::Shunit2Test => {
                if let Some(s) = source_lit {
                    TargetSource::Single(s)
                } else if source_key_seen {
                    // `source=` present but NOT a string literal:
                    // variable ref, concat, f-string, etc.
                    out.push(Err(TargetParseError::NonStringLiteralSource {
                        line: start_line,
                        snippet: body.chars().take(80).collect(),
                    }));
                    continue;
                } else {
                    // Missing both name AND source? Report kwarg-missing.
                    if name.is_none() {
                        out.push(Err(TargetParseError::MissingRequiredKwarg {
                            line: start_line,
                        }));
                        continue;
                    }
                    // name present but source absent — still an error for
                    // single-source targets (Pants requires source= on
                    // shell_source and shunit2_test).
                    out.push(Err(TargetParseError::MissingRequiredKwarg {
                        line: start_line,
                    }));
                    continue;
                }
            }
            ShellTargetKind::ShellSources | ShellTargetKind::Shunit2Tests => {
                if let Some(body_slice) = sources_body {
                    let globs: Vec<String> = list_element_regex()
                        .captures_iter(body_slice)
                        .filter_map(|c| {
                            c.get(1)
                                .or_else(|| c.get(2))
                                .map(|m| m.as_str().to_string())
                        })
                        .collect();
                    TargetSource::Globs(globs)
                } else {
                    // Operator omitted `sources=` — resolver applies Pants default.
                    TargetSource::Globs(Vec::new())
                }
            }
        };

        out.push(Ok(TargetDeclaration {
            kind,
            name,
            source,
            start_line,
        }));
    }
    out
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn parse_one(input: &str) -> Result<TargetDeclaration, TargetParseError> {
        let mut out = extract_targets(input.as_bytes());
        assert_eq!(out.len(), 1, "expected exactly 1 result, got {}", out.len());
        out.remove(0)
    }

    // 1. Valid shell_source — name then source order.
    #[test]
    fn valid_shell_source_name_source_order() {
        let decl = parse_one(r#"shell_source(name="deploy", source="deploy.sh")"#).unwrap();
        assert_eq!(decl.kind, ShellTargetKind::ShellSource);
        assert_eq!(decl.name.as_deref(), Some("deploy"));
        assert!(matches!(decl.source, TargetSource::Single(ref s) if s == "deploy.sh"));
    }

    // 2. Valid shell_source — source then name order.
    #[test]
    fn valid_shell_source_source_name_order() {
        let decl = parse_one(r#"shell_source(source="deploy.sh", name="deploy")"#).unwrap();
        assert_eq!(decl.name.as_deref(), Some("deploy"));
        assert!(matches!(decl.source, TargetSource::Single(ref s) if s == "deploy.sh"));
    }

    // 3. Valid shell_sources with 3-element sources list.
    #[test]
    fn valid_shell_sources_three_element_list() {
        let decl = parse_one(
            r#"shell_sources(name="utils", sources=["a.sh", "b.sh", "c.sh"])"#,
        )
        .unwrap();
        assert_eq!(decl.kind, ShellTargetKind::ShellSources);
        assert_eq!(decl.name.as_deref(), Some("utils"));
        if let TargetSource::Globs(g) = &decl.source {
            assert_eq!(g, &vec!["a.sh".to_string(), "b.sh".into(), "c.sh".into()]);
        } else {
            panic!("expected Globs");
        }
    }

    // 4. Valid shunit2_test.
    #[test]
    fn valid_shunit2_test() {
        let decl = parse_one(r#"shunit2_test(name="dep-test", source="dep_test.sh")"#).unwrap();
        assert_eq!(decl.kind, ShellTargetKind::Shunit2Test);
    }

    // 5. Valid shunit2_tests without explicit name (default = dir name at resolver).
    #[test]
    fn valid_shunit2_tests_default_name() {
        let decl = parse_one(r#"shunit2_tests(sources=["*_test.sh"])"#).unwrap();
        assert_eq!(decl.kind, ShellTargetKind::Shunit2Tests);
        assert!(decl.name.is_none());
        assert!(matches!(decl.source, TargetSource::Globs(ref g) if g == &vec!["*_test.sh"]));
    }

    // 6. Variable reference source returns NonStringLiteralSource.
    #[test]
    fn variable_reference_source_returns_err() {
        let err = parse_one(r#"shell_source(name="x", source=SCRIPT_NAME)"#).unwrap_err();
        assert!(matches!(err, TargetParseError::NonStringLiteralSource { .. }));
    }

    // 7. Concat source returns NonStringLiteralSource.
    #[test]
    fn concat_source_returns_err() {
        let err =
            parse_one(r#"shell_source(name="x", source="scripts/" + "deploy.sh")"#).unwrap_err();
        assert!(matches!(err, TargetParseError::NonStringLiteralSource { .. }));
    }

    // 8. Missing name AND source returns MissingRequiredKwarg.
    #[test]
    fn missing_name_and_source_returns_err() {
        let err = parse_one(r#"shell_source(tags=["a"])"#).unwrap_err();
        assert!(matches!(err, TargetParseError::MissingRequiredKwarg { .. }));
    }

    // 9. Multi-line target spanning 3 lines.
    #[test]
    fn multi_line_target_parses() {
        let text = "shell_source(\n    name=\"deploy\",\n    source=\"deploy.sh\",\n)";
        let decl = parse_one(text).unwrap();
        assert_eq!(decl.name.as_deref(), Some("deploy"));
    }

    // 10. Comment line inside target body is ignored by kwarg extraction.
    #[test]
    fn comment_inside_target_ignored() {
        let text = r#"shell_source(
    # this is a comment
    name="deploy",
    source="deploy.sh",
)"#;
        let decl = parse_one(text).unwrap();
        assert_eq!(decl.name.as_deref(), Some("deploy"));
    }

    // 11. Three valid targets in a single blob — all 3 parse.
    #[test]
    fn three_valid_targets_all_parse() {
        let text = r#"
shell_source(name="a", source="a.sh")

shell_source(name="b", source="b.sh")

shell_sources(name="c", sources=["c*.sh"])
"#;
        let out = extract_targets(text.as_bytes());
        assert_eq!(out.len(), 3);
        assert!(out.iter().all(|r| r.is_ok()));
    }

    // 12. Unbalanced parens returns UnbalancedParens.
    #[test]
    fn unbalanced_parens_returns_err() {
        let text = r#"shell_source(name="x", source="y.sh"
# forgot to close the paren
another_top_level_thing = 42"#;
        let out = extract_targets(text.as_bytes());
        assert_eq!(out.len(), 1);
        assert!(matches!(
            out[0],
            Err(TargetParseError::UnbalancedParens { .. })
        ));
    }

    // 13. Zero recognized targets → empty vec.
    #[test]
    fn zero_recognized_targets_returns_empty() {
        let text = r#"
python_source(name="foo", source="foo.py")
resource(name="bar", source="bar.txt")
"#;
        assert!(extract_targets(text.as_bytes()).is_empty());
    }
}
