//! Milestone 671 — source-code file-shape classification.
//!
//! Adds a closed-enum `SourceShape` covering the 21 source-code file
//! extensions the m671 spec's FR-002 allowlist blesses. Populated by
//! T003 (variants + methods) and T004 (parse-restriction helper).
//!
//! See `specs/671-file-tier-cpython/data-model.md` for the entity
//! definitions + `contracts/source_shape_restriction.md` for the
//! CLI parse-error contract.

#![allow(dead_code)] // Wired into `content_shape::classify` by T007.

/// Milestone 671 FR-002 — the 21 source-code file extensions eligible
/// for file-tier emission under the opt-in `--file-inventory=source-tree`
/// mode.
///
/// **Variants ordered by declaration** so the derived `Ord`/`PartialOrd`
/// impls produce a stable, review-friendly iteration order in the
/// C156 annotation's `restriction` field. The set is grouped by
/// language family (Python, C/C++, systems, JVM, JS/TS, dynamic,
/// Apple) for readability, then re-sorted by `as_str()` output at
/// annotation-emission time (BTreeSet iteration is by `Ord`).
///
/// `Ord`/`Hash` derived so `SourceShapeSet` (BTreeSet) has
/// deterministic iteration for byte-identical annotation values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum SourceShape {
    // Python
    Py,
    Pyi,
    // C / C++
    C,
    Cc,
    Cpp,
    Cxx,
    H,
    Hh,
    Hpp,
    // Systems
    Rs,
    Go,
    // JVM
    Java,
    Kt,
    // JavaScript / TypeScript
    Js,
    Ts,
    // Dynamic
    Rb,
    Php,
    // .NET
    Cs,
    // Apple platforms
    Swift,
    M,
    Mm,
}

/// Sorted, deduplicated collection of source-shape values. Populated
/// via `SourceShape::from_extension` + BTreeSet insert. Empty set is
/// invalid at the CLI parse layer (see [`parse_restriction`] under
/// T004) — an empty set has no observable use in the file-tier walker.
pub(crate) type SourceShapeSet = std::collections::BTreeSet<SourceShape>;

impl SourceShape {
    /// Lookup a `SourceShape` from a file extension string.
    /// Case-insensitive; tolerant of a leading `.` (e.g., `".py"`
    /// and `"py"` both map to `Py`). Returns `None` for any extension
    /// outside the FR-002 21-extension allowlist.
    pub(crate) fn from_extension(ext: &str) -> Option<Self> {
        let ext = ext.trim();
        // Strip exactly ONE leading `.` — `..py` is malformed input.
        let ext = ext.strip_prefix('.').unwrap_or(ext);
        // ASCII lowercase — all FR-002 extensions are lowercase ASCII;
        // this avoids allocation on the common already-lowercase path.
        let lower = ext.to_ascii_lowercase();
        Some(match lower.as_str() {
            "py" => Self::Py,
            "pyi" => Self::Pyi,
            "c" => Self::C,
            "cc" => Self::Cc,
            "cpp" => Self::Cpp,
            "cxx" => Self::Cxx,
            "h" => Self::H,
            "hh" => Self::Hh,
            "hpp" => Self::Hpp,
            "rs" => Self::Rs,
            "go" => Self::Go,
            "java" => Self::Java,
            "kt" => Self::Kt,
            "js" => Self::Js,
            "ts" => Self::Ts,
            "rb" => Self::Rb,
            "php" => Self::Php,
            "cs" => Self::Cs,
            "swift" => Self::Swift,
            "m" => Self::M,
            "mm" => Self::Mm,
            _ => return None,
        })
    }

    /// Canonical string form: lowercase extension name without the
    /// leading `.`. Round-trips through `from_extension`.
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Py => "py",
            Self::Pyi => "pyi",
            Self::C => "c",
            Self::Cc => "cc",
            Self::Cpp => "cpp",
            Self::Cxx => "cxx",
            Self::H => "h",
            Self::Hh => "hh",
            Self::Hpp => "hpp",
            Self::Rs => "rs",
            Self::Go => "go",
            Self::Java => "java",
            Self::Kt => "kt",
            Self::Js => "js",
            Self::Ts => "ts",
            Self::Rb => "rb",
            Self::Php => "php",
            Self::Cs => "cs",
            Self::Swift => "swift",
            Self::M => "m",
            Self::Mm => "mm",
        }
    }

    /// FR-002 allowlist enumerated for use in error diagnostics.
    /// Sorted lex by `as_str()` for stable diagnostic output.
    pub(crate) const ALL: [Self; 21] = [
        Self::C,
        Self::Cc,
        Self::Cpp,
        Self::Cs,
        Self::Cxx,
        Self::Go,
        Self::H,
        Self::Hh,
        Self::Hpp,
        Self::Java,
        Self::Js,
        Self::Kt,
        Self::M,
        Self::Mm,
        Self::Php,
        Self::Py,
        Self::Pyi,
        Self::Rb,
        Self::Rs,
        Self::Swift,
        Self::Ts,
    ];
}

/// Milestone 671 FR-009 — CLI parse-error surface for the
/// `--file-inventory-source-shapes` companion flag. Fail-loud posture
/// (Principle IX Accuracy) — no silent acceptance of unrecognized
/// extensions or empty inputs.
///
/// Display strings match `contracts/source_shape_restriction.md`'s
/// error-message contract verbatim so downstream consumers can grep
/// stderr for the expected diagnostic.
#[derive(Debug, thiserror::Error)]
pub(crate) enum SourceShapeParseError {
    /// Operator named an extension NOT in the FR-002 21-extension
    /// allowlist (e.g., `md`, `toml`). Diagnostic enumerates every
    /// accepted extension so the operator can fix in one edit.
    #[error(
        "unknown source-shape extension {actual:?}; accepted extensions are: \
         c, cc, cpp, cs, cxx, go, h, hh, hpp, java, js, kt, m, mm, php, py, \
         pyi, rb, rs, swift, ts (case-insensitive; leading dot optional)"
    )]
    UnknownExtension { actual: String },

    /// Operator passed `--file-inventory-source-shapes=` (empty value)
    /// or a comma-only string. An empty set has no observable use.
    #[error("empty --file-inventory-source-shapes value; pass a non-empty comma-separated list")]
    Empty,
}

/// Parse a comma-separated `--file-inventory-source-shapes` value into
/// a sorted, deduplicated `SourceShapeSet` per FR-009 semantics.
///
/// **Steps** (matches `contracts/source_shape_restriction.md`):
/// 1. Split raw value on `,`. Trim whitespace on each token.
/// 2. Drop empty tokens (from adjacent commas like `py,,c`). If EVERY
///    token was empty (input was `""` or `,,,`), return `Empty`.
/// 3. For each token: normalize via [`SourceShape::from_extension`]
///    which case-folds and strips one optional leading `.`.
/// 4. Unknown extension → `UnknownExtension { actual: <original-token> }`.
/// 5. Known extension → insert into `BTreeSet` (dedup by set semantic).
///
/// Silent duplicate handling matches how `clap` treats repeated
/// `--exclude` args — duplicates are absorbed, not diagnosed.
pub(crate) fn parse_restriction(raw: &str) -> Result<SourceShapeSet, SourceShapeParseError> {
    let mut out: SourceShapeSet = SourceShapeSet::new();
    let mut saw_any_token = false;
    for token in raw.split(',') {
        let trimmed = token.trim();
        if trimmed.is_empty() {
            continue;
        }
        saw_any_token = true;
        match SourceShape::from_extension(trimmed) {
            Some(shape) => {
                out.insert(shape);
            }
            None => {
                return Err(SourceShapeParseError::UnknownExtension {
                    actual: trimmed.to_string(),
                });
            }
        }
    }
    if !saw_any_token {
        return Err(SourceShapeParseError::Empty);
    }
    Ok(out)
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    /// FR-002: every variant round-trips through
    /// `from_extension(v.as_str()) == Some(v)`. Locks the enum
    /// invariant that `as_str` and `from_extension` are inverses.
    #[test]
    fn every_variant_round_trips_through_as_str() {
        for shape in SourceShape::ALL {
            let s = shape.as_str();
            let round_tripped = SourceShape::from_extension(s);
            assert_eq!(
                round_tripped,
                Some(shape),
                "as_str for {shape:?} yielded {s:?}; round-trip through from_extension yielded {round_tripped:?}",
            );
        }
    }

    /// FR-002 spec-clarification: case-insensitive matching lets
    /// cpython's `Doc/*.PY` (rare but real) surface as a `Py` shape.
    #[test]
    fn from_extension_is_case_insensitive() {
        for input in ["py", "PY", "Py", "pY"] {
            assert_eq!(
                SourceShape::from_extension(input),
                Some(SourceShape::Py),
                "case variant {input:?} should map to Py",
            );
        }
        for input in ["cpp", "CPP", "Cpp"] {
            assert_eq!(
                SourceShape::from_extension(input),
                Some(SourceShape::Cpp),
                "case variant {input:?} should map to Cpp",
            );
        }
    }

    /// Operators may include the leading `.` by habit (e.g.,
    /// `--file-inventory-source-shapes=.py`); tolerate defensively.
    #[test]
    fn from_extension_tolerates_leading_dot() {
        assert_eq!(SourceShape::from_extension(".py"), Some(SourceShape::Py));
        assert_eq!(SourceShape::from_extension(".rs"), Some(SourceShape::Rs));
        assert_eq!(SourceShape::from_extension(".Mm"), Some(SourceShape::Mm));
        // Double-dot input is malformed and should NOT match — the
        // trim only strips ONE leading dot.
        assert_eq!(SourceShape::from_extension("..py"), None);
    }

    /// Unknown extensions return None. Regression guard against
    /// accidentally matching partial substrings.
    #[test]
    fn from_extension_rejects_unknown() {
        for input in ["md", "toml", "yaml", "json", "txt", "gitignore", ""] {
            assert_eq!(
                SourceShape::from_extension(input),
                None,
                "unknown extension {input:?} must not match any variant",
            );
        }
    }

    /// The `ALL` array MUST be sorted lex by `as_str()` — the const
    /// is consumed by T004's error-message construction. If the
    /// declaration order drifts, diagnostic messages become noisy.
    #[test]
    fn all_array_is_sorted_lex_by_as_str() {
        for pair in SourceShape::ALL.windows(2) {
            assert!(
                pair[0].as_str() < pair[1].as_str(),
                "ALL is out of order: {} >= {}",
                pair[0].as_str(),
                pair[1].as_str(),
            );
        }
    }

    // -----------------------------------------------------------------
    // T004: parse_restriction — FR-009 CLI-parse contract
    // -----------------------------------------------------------------

    #[test]
    fn parse_restriction_typical_multi_extension_input() {
        let result = parse_restriction("py,c,h").expect("valid input parses");
        assert_eq!(
            result,
            [SourceShape::C, SourceShape::H, SourceShape::Py]
                .into_iter()
                .collect::<SourceShapeSet>(),
        );
        // BTreeSet iteration is by Ord — variant declaration order
        // in T003 groups by language family, but Ord is derived on
        // that same order, so this checks the deterministic-emission
        // invariant used by C156's `restriction` field.
        let iter_order: Vec<&'static str> = result.iter().map(|s| s.as_str()).collect();
        assert_eq!(iter_order, vec!["py", "c", "h"],
            "BTreeSet iteration follows variant declaration order (Py < C < H per T003 grouping)");
    }

    #[test]
    fn parse_restriction_empty_input_errors() {
        assert!(matches!(
            parse_restriction(""),
            Err(SourceShapeParseError::Empty)
        ));
        // Comma-only input has no non-empty tokens → also Empty.
        assert!(matches!(
            parse_restriction(",,,"),
            Err(SourceShapeParseError::Empty)
        ));
    }

    #[test]
    fn parse_restriction_unknown_extension_errors() {
        match parse_restriction("md") {
            Err(SourceShapeParseError::UnknownExtension { actual }) => {
                assert_eq!(actual, "md");
            }
            other => panic!("expected UnknownExtension for 'md', got {other:?}"),
        }
        // First-unknown-wins: valid extension before an invalid one
        // still short-circuits at the invalid token.
        match parse_restriction("py,yaml,c") {
            Err(SourceShapeParseError::UnknownExtension { actual }) => {
                assert_eq!(actual, "yaml");
            }
            other => panic!("expected UnknownExtension for 'yaml', got {other:?}"),
        }
    }

    #[test]
    fn parse_restriction_dedups_silently() {
        let result = parse_restriction("py,py,PY,.py").expect("all resolve to Py");
        let single: SourceShapeSet = [SourceShape::Py].into_iter().collect();
        assert_eq!(result, single);
    }

    #[test]
    fn parse_restriction_error_diagnostic_lists_accepted_extensions() {
        // FR-009: unknown-extension error message MUST enumerate the
        // FR-002 allowlist so operators can fix in one edit.
        let err = parse_restriction("md").expect_err("md is not accepted");
        let msg = err.to_string();
        // Sanity-check that a sample of accepted extensions appears
        // in the diagnostic. If ALL drifts, this test also flags it.
        for expected_ext in ["py", "c", "h", "rs", "swift"] {
            assert!(
                msg.contains(expected_ext),
                "diagnostic missing accepted extension {expected_ext:?}: {msg}",
            );
        }
    }

    #[test]
    fn parse_restriction_tolerates_whitespace_and_leading_dot() {
        // Whitespace-tolerant per contract's parse-step 1.
        let result =
            parse_restriction(" py , c , .h ").expect("whitespace + leading dot both tolerated");
        let expected: SourceShapeSet = [SourceShape::C, SourceShape::H, SourceShape::Py]
            .into_iter()
            .collect();
        assert_eq!(result, expected);
    }
}
