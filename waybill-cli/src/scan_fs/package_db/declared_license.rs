//! Resolve a project's own declared license from the manifest a reader has
//! already parsed.
//!
//! Issue #954. Every main-module reader holds, at the point it constructs its
//! component, the parsed manifest table that declares the license — and threw
//! it away. This is the shared resolution those readers call, so the eleven
//! ecosystems cannot drift apart. They already had begun to: milestone 957
//! implemented Haskell alone, choosing to discard a value that would not
//! canonicalise, and that choice is superseded here (FR-008a).
//!
//! # The ladder (FR-004a)
//!
//! Two steps, in this order, and never the other way round:
//!
//! 1. [`SpdxExpression::try_canonical`] — runs the real expression parser and
//!    stores the canonical spelling. A value that canonicalises MUST be emitted
//!    as a recognised license identifier (FR-005).
//! 2. [`SpdxExpression::new`] — lenient, stores the raw text verbatim. Reached
//!    only when step 1 fails, so that the declaration survives (FR-004).
//!
//! Step 2 is not a fallback into silence. The emission layer already detects a
//! value that will not canonicalise and mints a `LicenseRef-<hash>` plus the
//! document-level extracted-text record for it — see
//! `generate::spdx::packages::reduce_license_vec`, whose contract is *"any term
//! fails canon → `(LicenseRef(id), Some(extracted_info))`"*. The OS-package
//! readers have fed that path for several milestones. So preserving costs no
//! emission work; dropping was never the cheaper option, only the earlier one.
//!
//! What step 2 must never do is present an unrecognised string as though it
//! were a listed identifier. That invariant is milestone 957's, and it is kept.
//!
//! # Combining several declarations (FR-010)
//!
//! A reader that finds more than one declared license combines them **here**,
//! naming the operator its own ecosystem documents, and emits one expression.
//! It must not push several values and let something downstream join them:
//! `reduce_license_vec` joins with an unconditional ` AND `, and for an
//! ecosystem whose list documents a *choice* that asserts a consumer must
//! satisfy every license when the project said any one would do. That inverts
//! the legal meaning, so the decision belongs to the only layer that knows the
//! ecosystem — the reader (FR-010b).
//!
//! Phase 0 research found that most ecosystems document nothing here. Of the
//! list-valued ones, only Composer defines the relationship (its array is
//! disjunctive); RubyGems states outright that its array "does not state how the
//! licenses combine", and the Maven POM reference is silent. So
//! [`LicenseJoin::Conjunction`] is the common path, chosen because it
//! over-states the obligation rather than under-stating it: a consumer
//! complying with more licenses than required cannot breach one, whereas a
//! consumer told any single license suffices can.

use std::path::Path;

use waybill_common::types::license::SpdxExpression;

/// How a reader combines several declared licenses into one expression.
///
/// The variant is a property of the *ecosystem*, not of the values, and is
/// recorded per ecosystem in `specs/1010-manifest-declared-license/contracts/`.
// Unused until the list-valued readers land: maven, gem, composer, elixir,
// erlang and scala all combine several declarations, while cargo, npm, pip,
// nuget and cocoapods take a single expression and call `resolve` directly.
// Kept here rather than deferred because the operator table is what the
// per-ecosystem research established (FR-010a) and its behaviour is pinned by
// tests; splitting the helper from its contract would invite the two to drift.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LicenseJoin {
    /// The ecosystem documents its list as a choice among licenses. Composer is
    /// the only verified instance: "when there is a choice between licenses
    /// (\"disjunctive license\"), multiple can be specified as an array".
    Disjunction,
    /// The ecosystem documents cumulative terms, or documents nothing at all.
    /// The conservative default — see the module docs for why.
    Conjunction,
}

impl LicenseJoin {
    fn separator(self) -> &'static str {
        match self {
            Self::Disjunction => " OR ",
            Self::Conjunction => " AND ",
        }
    }
}

/// The outcome of resolving one manifest's license declaration.
///
/// This is deliberately not `Option<SpdxExpression>`. `Option` cannot
/// distinguish a canonicalised value from preserved raw text, yet the two are
/// emitted differently and only one may be presented as a recognised
/// identifier. Encoding the difference in the type makes FR-004c enforceable by
/// the compiler rather than by reviewer attention (Constitution Principle IV).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DeclaredLicense {
    /// Parsed and canonicalised. Safe to emit as a license identifier.
    Canonical(SpdxExpression),
    /// Did not canonicalise; the raw declaration is retained so emission can
    /// mint a non-listed license reference for it.
    Preserved(SpdxExpression),
    /// No declaration, or an inheritance that could not be resolved. Not an
    /// error, and never warned about — most projects in some ecosystems declare
    /// nothing (FR-006).
    Absent,
}

impl DeclaredLicense {
    /// The value in the shape `PackageDbEntry.licenses` expects.
    ///
    /// At most one element, by design: a reader combines several declarations
    /// itself (FR-010b), so this never returns more.
    pub(crate) fn into_licenses(self) -> Vec<SpdxExpression> {
        match self {
            Self::Canonical(expr) | Self::Preserved(expr) => vec![expr],
            Self::Absent => Vec::new(),
        }
    }

    /// Whether a declaration was found at all, regardless of how it resolved.
    /// Used by callers that log coverage rather than by emission.
    #[allow(dead_code)] // for readers that log declaration coverage
    pub(crate) fn is_present(&self) -> bool {
        !matches!(self, Self::Absent)
    }
}

/// Resolve a single raw declaration through the ladder.
///
/// `manifest` is used only for the FR-004b diagnostic, so that an operator
/// raising log verbosity learns which file carried the value that would not
/// canonicalise.
pub(crate) fn resolve(raw: &str, manifest: &Path) -> DeclaredLicense {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return DeclaredLicense::Absent;
    }

    match SpdxExpression::try_canonical(trimmed) {
        Ok(expr) => DeclaredLicense::Canonical(expr),
        Err(canon_err) => match SpdxExpression::new(trimmed) {
            Ok(expr) => {
                tracing::debug!(
                    manifest = %manifest.display(),
                    raw = trimmed,
                    error = %canon_err,
                    "declared license is not a canonical SPDX expression; preserved \
                     as a non-listed license reference rather than dropped (#954)"
                );
                DeclaredLicense::Preserved(expr)
            }
            // Unreachable for a non-empty string without control characters,
            // which `SpdxExpression::new` is the only rejecter of. Treated as
            // absent rather than panicking: a control character in a manifest
            // must not fail a scan (FR-007).
            Err(lenient_err) => {
                tracing::debug!(
                    manifest = %manifest.display(),
                    error = %lenient_err,
                    "declared license could not be stored even leniently; omitted"
                );
                DeclaredLicense::Absent
            }
        },
    }
}

/// Resolve several raw declarations, combining them with `join` into one
/// expression before resolution.
///
/// Empty and whitespace-only entries are dropped first, so a manifest listing
/// `["MIT", ""]` yields `MIT` rather than a trailing operator. If every entry is
/// empty the result is [`DeclaredLicense::Absent`].
///
/// Note that the join happens *before* canonicalisation, so a list where one
/// term is unrecognised preserves the whole combined string rather than
/// silently keeping only the terms that parsed. Emitting a subset would assert
/// a narrower license than the project declared.
#[allow(dead_code)] // consumed by the list-valued readers; see `LicenseJoin`
pub(crate) fn resolve_many<S: AsRef<str>>(
    raws: &[S],
    join: LicenseJoin,
    manifest: &Path,
) -> DeclaredLicense {
    let terms: Vec<&str> = raws
        .iter()
        .map(|r| r.as_ref().trim())
        .filter(|r| !r.is_empty())
        .collect();

    match terms.len() {
        0 => DeclaredLicense::Absent,
        1 => resolve(terms[0], manifest),
        _ => resolve(&terms.join(join.separator()), manifest),
    }
}

/// Pull every quoted string literal out of a DSL fragment, in order.
///
/// Three readers declare licenses inside a host language rather than in a data
/// format — a Ruby gemspec (`spec.licenses = ["MIT", "Apache-2.0"]`), an Elixir
/// keyword list (`licenses: ["MIT"]`), and an sbt setting
/// (`licenses := Seq(("Apache 2", url(...))))`). All three need the same thing:
/// the quoted terms, ignoring the surrounding syntax.
///
/// Both quote styles are accepted because Ruby uses either. Nothing is
/// interpreted — no escape handling, no nesting — because the goal is to recover
/// license *terms*, and a term containing an escaped quote is not a license
/// identifier. In the sbt case this also picks up the URL half of each tuple;
/// callers that expect tuples keep only the first of each pair.
pub(crate) fn extract_quoted_terms(fragment: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = fragment.char_indices().peekable();
    while let Some((_, c)) = chars.next() {
        if c != '"' && c != '\'' {
            continue;
        }
        let quote = c;
        let mut term = String::new();
        for (_, c2) in chars.by_ref() {
            if c2 == quote {
                break;
            }
            term.push(c2);
        }
        let trimmed = term.trim();
        if !trimmed.is_empty() {
            out.push(trimmed.to_string());
        }
    }
    out
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn manifest() -> PathBuf {
        PathBuf::from("/waybill-fixture/Cargo.toml")
    }

    // ---- T013: the ladder's three outcomes ----

    #[test]
    fn t013_canonical_value_resolves_canonical() {
        let got = resolve("MIT", &manifest());
        assert_eq!(
            got,
            DeclaredLicense::Canonical(SpdxExpression::try_canonical("MIT").unwrap())
        );
    }

    #[test]
    fn t013_compound_expression_resolves_canonical() {
        // The common cargo shape. Must stay one expression, not be split.
        let got = resolve("MIT OR Apache-2.0", &manifest());
        match got {
            DeclaredLicense::Canonical(expr) => {
                assert_eq!(expr.as_str(), "MIT OR Apache-2.0");
            }
            other => panic!("expected Canonical, got {other:?}"),
        }
    }

    #[test]
    fn t013_non_canonical_spelling_is_canonicalised() {
        // FR-005: a valid but untidily spelled value is emitted canonically.
        let got = resolve("  Apache-2.0  ", &manifest());
        match got {
            DeclaredLicense::Canonical(expr) => assert_eq!(expr.as_str(), "Apache-2.0"),
            other => panic!("expected Canonical, got {other:?}"),
        }
    }

    #[test]
    fn t013_uncanonicalisable_value_is_preserved_not_dropped() {
        // FR-004 — the behaviour that supersedes #957. `AllRightsReserved` is
        // a real legacy .cabal spelling and is not a valid SPDX expression.
        let got = resolve("AllRightsReserved", &manifest());
        match got {
            DeclaredLicense::Preserved(expr) => {
                assert_eq!(expr.as_str(), "AllRightsReserved");
            }
            other => panic!("expected Preserved, got {other:?} — dropping loses the declaration"),
        }
    }

    #[test]
    fn t013_sbt_style_free_form_name_is_preserved() {
        // sbt's own documented example is `"Apache 2"`, not `Apache-2.0`, so
        // this is the common path for scala rather than an exotic case.
        match resolve("Apache 2", &manifest()) {
            DeclaredLicense::Preserved(expr) => assert_eq!(expr.as_str(), "Apache 2"),
            other => panic!("expected Preserved, got {other:?}"),
        }
    }

    #[test]
    fn t013_preserved_and_canonical_are_distinguishable() {
        // The reason this is an enum and not `Option`: emission must be able to
        // tell these apart, and a test must be able to fail if it cannot.
        let canonical = resolve("MIT", &manifest());
        let preserved = resolve("Made Up License", &manifest());
        assert!(matches!(canonical, DeclaredLicense::Canonical(_)));
        assert!(matches!(preserved, DeclaredLicense::Preserved(_)));
        assert_ne!(canonical, preserved);
    }

    // ---- T014: the join operator ----

    #[test]
    fn t014_disjunctive_ecosystem_joins_with_or() {
        // Composer: its array is documented as a choice between licenses.
        let got = resolve_many(
            &["LGPL-2.1-only", "GPL-3.0-or-later"],
            LicenseJoin::Disjunction,
            &manifest(),
        );
        match got {
            DeclaredLicense::Canonical(expr) => {
                assert_eq!(expr.as_str(), "LGPL-2.1-only OR GPL-3.0-or-later");
            }
            other => panic!("expected Canonical, got {other:?}"),
        }
    }

    #[test]
    fn t014_silent_ecosystem_joins_with_and() {
        // maven / gem / elixir / erlang / scala: the conservative fallback.
        let got = resolve_many(&["MIT", "Apache-2.0"], LicenseJoin::Conjunction, &manifest());
        match got {
            DeclaredLicense::Canonical(expr) => {
                assert_eq!(expr.as_str(), "MIT AND Apache-2.0");
            }
            other => panic!("expected Canonical, got {other:?}"),
        }
    }

    #[test]
    fn t014_operator_choice_changes_the_emitted_meaning() {
        // Guards the inversion this design exists to prevent: the same two
        // licenses must not produce the same expression under both operators.
        let or = resolve_many(&["MIT", "Apache-2.0"], LicenseJoin::Disjunction, &manifest());
        let and = resolve_many(&["MIT", "Apache-2.0"], LicenseJoin::Conjunction, &manifest());
        assert_ne!(or, and);
    }

    #[test]
    fn t014_single_entry_gets_no_operator() {
        let got = resolve_many(&["MIT"], LicenseJoin::Conjunction, &manifest());
        match got {
            DeclaredLicense::Canonical(expr) => assert_eq!(expr.as_str(), "MIT"),
            other => panic!("expected Canonical, got {other:?}"),
        }
    }

    #[test]
    fn t014_one_unrecognised_term_preserves_the_whole_join() {
        // Emitting only the terms that parsed would assert a narrower license
        // than the project declared.
        let got = resolve_many(
            &["MIT", "Made Up License"],
            LicenseJoin::Conjunction,
            &manifest(),
        );
        match got {
            DeclaredLicense::Preserved(expr) => {
                assert_eq!(expr.as_str(), "MIT AND Made Up License");
            }
            other => panic!("expected Preserved, got {other:?}"),
        }
    }

    // ---- extract_quoted_terms ----

    #[test]
    fn quoted_terms_reads_a_ruby_array() {
        assert_eq!(
            extract_quoted_terms(r#"= ["MIT", "Apache-2.0"]"#),
            vec!["MIT".to_string(), "Apache-2.0".to_string()]
        );
    }

    #[test]
    fn quoted_terms_accepts_single_quotes() {
        assert_eq!(extract_quoted_terms("= 'MIT'"), vec!["MIT".to_string()]);
    }

    #[test]
    fn quoted_terms_reads_an_sbt_tuple_list() {
        // The URL half comes back too; sbt callers keep the first of each pair.
        assert_eq!(
            extract_quoted_terms(r#":= Seq(("Apache 2", url("http://example.invalid")))"#),
            vec!["Apache 2".to_string(), "http://example.invalid".to_string()]
        );
    }

    #[test]
    fn quoted_terms_ignores_unquoted_syntax_and_blanks() {
        assert_eq!(extract_quoted_terms("licenses: [] # none"), Vec::<String>::new());
        assert_eq!(extract_quoted_terms(r#"["", "  ", "MIT"]"#), vec!["MIT".to_string()]);
    }

    // ---- T015: absence is ordinary ----

    #[test]
    fn t015_empty_declaration_is_absent() {
        assert_eq!(resolve("", &manifest()), DeclaredLicense::Absent);
    }

    #[test]
    fn t015_whitespace_only_declaration_is_absent() {
        assert_eq!(resolve("   \t  ", &manifest()), DeclaredLicense::Absent);
    }

    #[test]
    fn t015_empty_list_is_absent() {
        let empty: [&str; 0] = [];
        assert_eq!(
            resolve_many(&empty, LicenseJoin::Conjunction, &manifest()),
            DeclaredLicense::Absent
        );
    }

    #[test]
    fn t015_blank_entries_are_dropped_before_joining() {
        // A manifest listing ["MIT", ""] must not yield "MIT AND ".
        let got = resolve_many(&["MIT", "", "  "], LicenseJoin::Conjunction, &manifest());
        match got {
            DeclaredLicense::Canonical(expr) => assert_eq!(expr.as_str(), "MIT"),
            other => panic!("expected Canonical, got {other:?}"),
        }
    }

    #[test]
    fn t015_absent_yields_no_licenses() {
        assert!(DeclaredLicense::Absent.into_licenses().is_empty());
        assert!(!DeclaredLicense::Absent.is_present());
    }

    #[test]
    fn t015_resolved_yields_exactly_one_license() {
        // FR-010b's cardinality: readers combine, so this is never more than 1.
        assert_eq!(resolve("MIT", &manifest()).into_licenses().len(), 1);
        assert_eq!(
            resolve_many(&["MIT", "Apache-2.0"], LicenseJoin::Conjunction, &manifest())
                .into_licenses()
                .len(),
            1
        );
    }
}
