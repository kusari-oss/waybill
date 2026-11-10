//! Milestone 1281 — joining declared licenses onto closure members.
//!
//! The evaluation in `eval::licenses` returns rows; this module decides which
//! closure member each row describes, and what license that member carries.
//!
//! # The join is the store path
//!
//! `RawClosure::derivations` is keyed by `.drv` basename, and a row carries
//! the same basename for the derivation nixpkgs builds. Those two agreeing is
//! the whole of the evidence that a license belongs to a member (spec
//! FR-002). A name match is not evidence: `autoconf269` declares
//! `pname = autoconf`, so joining on the name attaches 2.73's license to 2.69
//! with nothing in the document to say it is wrong (research R5).
//!
//! Keyed on `.drv` paths for the same reason milestone 1280 keyed its graph
//! that way — the identity pass rewrites component PURLs after emission, so a
//! PURL-keyed join would dangle where a `.drv`-keyed one survives.

use std::collections::{BTreeMap, BTreeSet};

use waybill_common::types::license::SpdxExpression;

use super::super::eval::licenses::{DeclaredLicenseRaw, LicenseRow};

/// Licenses for closure members, keyed by `.drv` basename.
pub(crate) type LicenseIndex = BTreeMap<String, Vec<SpdxExpression>>;

/// How many members were offered a license and how many received one.
///
/// Principle X's carrier. Without it a closure whose members genuinely
/// declare nothing is indistinguishable from a lookup that silently returned
/// nothing — and coverage can never reach 100%, because a `.drv` hash encodes
/// the whole transitive input closure, so the same package built in a
/// different context is a different store path (research R5). The gap is
/// permanent, so it has to be legible rather than inferred from silence.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct LicenseCoverage {
    /// Closure members eligible for a license.
    pub(crate) wanted: usize,
    /// Members the join actually licensed.
    pub(crate) received: usize,
    /// Members whose rows disagreed across roots, so FR-015 withheld a
    /// license rather than guessing.
    pub(crate) withheld_ambiguous: usize,
}

/// Turn one declared license into the two-state form emission understands.
///
/// nixpkgs' own `spdxId` is the authority where present (spec FR-014).
/// Re-deriving an identifier from the attribute name would be a second
/// mapping maintained beside the one nixpkgs already ships, and the two would
/// drift. Where no `spdxId` exists the attribute is preserved as stated and
/// resolved leniently, which mints a non-listed reference rather than
/// inventing a plausible-looking identifier (FR-003).
///
/// #1404 — a composite declaration is rendered to the one expression it
/// states and resolved whole. Where every operand carries an `spdxId` that is
/// a canonical SPDX expression; where one does not, the ladder preserves the
/// whole stated text rather than keeping only the operands that canonicalise,
/// which would assert a narrower license than the package declared.
fn to_expression(raw: &DeclaredLicenseRaw) -> Option<SpdxExpression> {
    let rendered = render(raw)?;
    let value = rendered.trim();
    if value.is_empty() {
        return None;
    }
    // `declared_license::resolve` is the package-db-wide ladder: canonical
    // where it parses, preserved-and-marked where it does not, absent where
    // it cannot be stored at all. Reused rather than re-derived so this
    // reader states licenses the way every other reader does.
    super::super::super::declared_license::resolve(value, std::path::Path::new("<nixpkgs meta>"))
        .into_licenses()
        .into_iter()
        .next()
}

/// Render one declaration as SPDX expression text.
///
/// Brackets exactly where nixpkgs' `lib.licenses.toSPDX` brackets — around a
/// compound or exception operand — so the text has the structure nixpkgs
/// built. `None` when any part is unreadable (no `spdxId` and no name, an
/// operator this does not know, or the wrong number of operands): dropping one
/// operand of an `AND` would state a narrower license than the package
/// declared, and of an `OR` a choice it did not offer.
fn render(raw: &DeclaredLicenseRaw) -> Option<String> {
    let Some(op) = raw.op.as_deref() else {
        let leaf = raw.spdx.as_deref().or(raw.name.as_deref())?.trim();
        return (!leaf.is_empty()).then(|| leaf.to_string());
    };
    let operand = |t: &DeclaredLicenseRaw| -> Option<String> {
        let text = render(t)?;
        Some(if is_bracketed_operand(t) { format!("({text})") } else { text })
    };
    match (op, raw.terms.as_slice()) {
        ("AND" | "OR", [only]) => render(only),
        ("AND" | "OR", terms @ [_, _, ..]) => {
            let parts = terms.iter().map(operand).collect::<Option<Vec<_>>>()?;
            Some(parts.join(&format!(" {op} ")))
        }
        ("WITH", [license, exception]) => {
            Some(format!("{} WITH {}", operand(license)?, operand(exception)?))
        }
        ("+", [license]) => Some(format!("{}+", operand(license)?)),
        _ => None,
    }
}

/// Whether nixpkgs' `toSPDX` would bracket this operand: a compound of more
/// than one license, or an exception. A compound of one renders as its only
/// license, so it needs none.
fn is_bracketed_operand(raw: &DeclaredLicenseRaw) -> bool {
    match raw.op.as_deref() {
        Some("AND" | "OR") => raw.terms.len() > 1,
        Some("WITH") => true,
        _ => false,
    }
}

/// Build the index, applying FR-015 where a store path is reached by several
/// roots.
///
/// Returns the index and the ambiguity count. Rows carried identical licenses
/// for every joined store path in both probed shapes (research R4), so
/// disagreement is asserted against rather than assumed away — if it ever
/// happens, the member receives no license instead of an arbitrary one.
pub(crate) fn index(rows: &[LicenseRow]) -> (LicenseIndex, usize) {
    let mut by_drv: BTreeMap<&str, Vec<&LicenseRow>> = BTreeMap::new();
    for row in rows {
        by_drv.entry(row.drv.as_str()).or_default().push(row);
    }

    let mut index = LicenseIndex::new();
    let mut ambiguous = 0usize;
    for (drv, rows) in by_drv {
        // Compare the declarations themselves, not the resolved expressions:
        // two roots stating the same thing must agree before canonicalisation
        // collapses any difference. The whole tree is compared (#1404): every
        // compound's top level reads `(None, None)`, so comparing only that
        // would call two different compounds equal.
        let shapes: BTreeSet<&Vec<DeclaredLicenseRaw>> =
            rows.iter().map(|r| &r.licenses).collect();
        if shapes.len() > 1 {
            ambiguous += 1;
            tracing::warn!(
                drv,
                roots = rows.len(),
                "nix-closure-licenses: roots disagree about this store path's license; \
                 emitting none rather than choosing (FR-015)"
            );
            continue;
        }
        let Some(first) = rows.first() else { continue };
        let mut seen = BTreeSet::new();
        let mut licenses = Vec::new();
        for raw in &first.licenses {
            if let Some(expr) = to_expression(raw) {
                // FR-005 — all declared licenses, in nixpkgs' stated order,
                // deduplicated.
                if seen.insert(expr.as_str().to_string()) {
                    licenses.push(expr);
                }
            }
        }
        if !licenses.is_empty() {
            index.insert(drv.to_string(), licenses);
        }
    }
    (index, ambiguous)
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn row(drv: &str, root: &str, lics: &[(Option<&str>, Option<&str>)]) -> LicenseRow {
        LicenseRow {
            version: None,
            cpe: None,
            drv: drv.to_string(),
            out: format!("/nix/store/out-{drv}"),
            attr: "waybill-fixture-pkg".to_string(),
            root: root.to_string(),
            licenses: lics
                .iter()
                .map(|(s, n)| DeclaredLicenseRaw {
                    spdx: s.map(str::to_string),
                    name: n.map(str::to_string),
                    op: None,
                    terms: Vec::new(),
                })
                .collect(),
        }
    }

    #[test]
    fn an_spdx_id_is_used_directly() {
        let (idx, amb) = index(&[row("aaa-x-1.0.drv", "pkgs", &[(Some("MIT"), Some("mit"))])]);
        assert_eq!(amb, 0);
        assert_eq!(idx["aaa-x-1.0.drv"][0].as_str(), "MIT");
    }

    /// FR-003 — `publicDomain` has no SPDX identifier, and must not acquire
    /// an invented one.
    #[test]
    fn an_attribute_without_an_spdx_id_is_preserved_not_faked() {
        let (idx, _) = index(&[row(
            "bbb-y-1.0.drv",
            "pkgs",
            &[(None, Some("Public Domain"))],
        )]);
        let got = &idx["bbb-y-1.0.drv"][0];
        assert_eq!(got.as_str(), "Public Domain");
        assert!(
            got.as_spdx_id().is_none(),
            "a preserved declaration must not present as a canonical SPDX id"
        );
    }

    /// FR-005 — every declared license, not the first.
    #[test]
    fn several_declared_licenses_are_all_carried_in_order() {
        let (idx, _) = index(&[row(
            "ccc-z-1.0.drv",
            "pkgs",
            &[
                (Some("Apache-2.0"), Some("asl20")),
                (Some("MIT"), Some("mit")),
            ],
        )]);
        let got: Vec<&str> = idx["ccc-z-1.0.drv"].iter().map(|e| e.as_str()).collect();
        assert_eq!(got, vec!["Apache-2.0", "MIT"]);
    }

    #[test]
    fn repeated_declarations_are_deduplicated() {
        let (idx, _) = index(&[row(
            "ddd-w-1.0.drv",
            "pkgs",
            &[(Some("MIT"), None), (Some("MIT"), Some("mit"))],
        )]);
        assert_eq!(idx["ddd-w-1.0.drv"].len(), 1);
    }

    /// FR-015 — agreeing roots are fine; the member keeps its license.
    #[test]
    fn roots_that_agree_license_the_member_once() {
        let (idx, amb) = index(&[
            row("eee-v-1.0.drv", "pkgs", &[(Some("MIT"), Some("mit"))]),
            row(
                "eee-v-1.0.drv",
                "haskellPackages",
                &[(Some("MIT"), Some("mit"))],
            ),
        ]);
        assert_eq!(amb, 0);
        assert_eq!(idx["eee-v-1.0.drv"].len(), 1);
    }

    /// FR-015 — disagreeing roots withhold rather than guess. Guessing here
    /// is the same failure FR-002 refuses for name matching: a wrong license
    /// with nothing in the document to flag it.
    #[test]
    fn roots_that_disagree_withhold_the_license() {
        let (idx, amb) = index(&[
            row("fff-u-1.0.drv", "pkgs", &[(Some("MIT"), Some("mit"))]),
            row(
                "fff-u-1.0.drv",
                "haskellPackages",
                &[(Some("GPL-3.0-only"), Some("gpl3Only"))],
            ),
        ]);
        assert_eq!(amb, 1);
        assert!(!idx.contains_key("fff-u-1.0.drv"));
    }

    #[test]
    fn a_member_with_no_declaration_gets_no_entry() {
        let (idx, _) = index(&[row("ggg-t-1.0.drv", "pkgs", &[])]);
        assert!(idx.is_empty());
    }

    // ------------------------------------------------ #1404 compound licenses

    fn leaf(spdx: Option<&str>, name: Option<&str>) -> DeclaredLicenseRaw {
        DeclaredLicenseRaw {
            spdx: spdx.map(str::to_string),
            name: name.map(str::to_string),
            op: None,
            terms: Vec::new(),
        }
    }

    fn node(op: &str, terms: Vec<DeclaredLicenseRaw>) -> DeclaredLicenseRaw {
        DeclaredLicenseRaw {
            spdx: None,
            name: None,
            op: Some(op.to_string()),
            terms,
        }
    }

    fn row_of(drv: &str, root: &str, licenses: Vec<DeclaredLicenseRaw>) -> LicenseRow {
        LicenseRow {
            licenses,
            ..row(drv, root, &[])
        }
    }

    fn one_expression(licenses: Vec<DeclaredLicenseRaw>) -> Option<String> {
        let (idx, _) = index(&[row_of("hhh-c-1.0.drv", "pkgs", licenses)]);
        idx.get("hhh-c-1.0.drv").map(|v| {
            assert_eq!(v.len(), 1, "one declaration must stay one expression: {v:?}");
            v[0].as_str().to_string()
        })
    }

    /// `llvm`, `lld` and `clang-wrapper` 21.1.8 — the shape #1404 measured,
    /// which was emitted with no license at all.
    fn llvm() -> DeclaredLicenseRaw {
        node(
            "AND",
            vec![
                leaf(Some("NCSA"), Some("ncsa")),
                node(
                    "WITH",
                    vec![
                        leaf(Some("Apache-2.0"), Some("asl20")),
                        leaf(Some("LLVM-exception"), Some("llvm-exception")),
                    ],
                ),
            ],
        )
    }

    #[test]
    fn a_compound_license_becomes_the_spdx_expression_it_states() {
        let got = one_expression(vec![llvm()]).expect("llvm must be licensed");
        assert_eq!(got, "NCSA AND (Apache-2.0 WITH LLVM-exception)");
        assert!(
            SpdxExpression::try_canonical(&got).is_ok(),
            "every operand had an spdxId, so the expression must canonicalise"
        );
    }

    #[test]
    fn a_disjunction_keeps_its_operator() {
        let got = one_expression(vec![node(
            "OR",
            vec![leaf(Some("MIT"), Some("mit")), leaf(Some("Apache-2.0"), Some("asl20"))],
        )]);
        assert_eq!(got.as_deref(), Some("MIT OR Apache-2.0"));
    }

    /// Nesting is bracketed exactly where nixpkgs' own `toSPDX` brackets it,
    /// so `(A AND B) OR C` does not come out as `A AND B OR C`.
    #[test]
    fn nested_compounds_are_bracketed() {
        let got = one_expression(vec![node(
            "OR",
            vec![
                node("AND", vec![leaf(Some("MIT"), None), leaf(Some("ISC"), None)]),
                leaf(Some("Apache-2.0"), None),
            ],
        )]);
        assert_eq!(got.as_deref(), Some("(MIT AND ISC) OR Apache-2.0"));
    }

    #[test]
    fn a_plus_license_is_the_or_later_form() {
        let got = one_expression(vec![node("+", vec![leaf(Some("EUPL-1.1"), Some("eupl11"))])]);
        assert_eq!(got.as_deref(), Some("EUPL-1.1+"));
    }

    #[test]
    fn a_compound_of_one_is_that_license_without_brackets() {
        let got = one_expression(vec![node("AND", vec![leaf(Some("MIT"), Some("mit"))])]);
        assert_eq!(got.as_deref(), Some("MIT"));
    }

    /// FR-003 inside a compound: the member with no `spdxId` keeps its stated
    /// name. nixpkgs' `toSPDX` would write `LicenseRef-nixos-publicDomain`; that
    /// identifier is nixpkgs' invention and is not stated in `meta.license`, so
    /// it must not appear here either. The whole expression is preserved, as
    /// `declared_license::resolve_many` preserves a list with one unknown term.
    #[test]
    fn a_compound_member_without_an_spdx_id_is_preserved_not_faked() {
        let (idx, _) = index(&[row_of(
            "iii-p-1.0.drv",
            "pkgs",
            vec![node(
                "AND",
                vec![leaf(Some("MIT"), Some("mit")), leaf(None, Some("Public Domain"))],
            )],
        )]);
        let got = &idx["iii-p-1.0.drv"][0];
        assert_eq!(got.as_str(), "MIT AND Public Domain");
        assert!(got.as_spdx_id().is_none());
        assert!(SpdxExpression::try_canonical(got.as_str()).is_err());
        assert!(!got.as_str().contains("LicenseRef"), "no invented identifier: {got}");
    }

    /// Dropping the unrenderable operand would state a narrower license than
    /// the package declared (for AND) or a choice it did not offer (for OR).
    #[test]
    fn a_compound_with_an_unreadable_member_yields_no_license_rather_than_a_subset() {
        let got = one_expression(vec![node(
            "AND",
            vec![leaf(Some("MIT"), Some("mit")), leaf(None, None)],
        )]);
        assert_eq!(got, None);
    }

    #[test]
    fn an_unknown_operator_or_malformed_arity_yields_no_license() {
        assert_eq!(one_expression(vec![node("XOR", vec![leaf(Some("MIT"), None)])]), None);
        assert_eq!(one_expression(vec![node("AND", vec![])]), None);
        assert_eq!(one_expression(vec![node("WITH", vec![leaf(Some("MIT"), None)])]), None);
        assert_eq!(
            one_expression(vec![node(
                "+",
                vec![leaf(Some("MIT"), None), leaf(Some("ISC"), None)]
            )]),
            None
        );
    }

    /// FR-015 with compounds. Comparing only the top-level `(spdx, name)`
    /// pair sees two compounds as `(None, None)` each and calls them equal,
    /// so roots that disagree inside the tree must still withhold.
    #[test]
    fn roots_that_disagree_inside_a_compound_withhold_the_license() {
        let other = node(
            "AND",
            vec![leaf(Some("NCSA"), Some("ncsa")), leaf(Some("MIT"), Some("mit"))],
        );
        let (idx, amb) = index(&[
            row_of("jjj-d-1.0.drv", "pkgs", vec![llvm()]),
            row_of("jjj-d-1.0.drv", "haskellPackages", vec![other]),
        ]);
        assert_eq!(amb, 1);
        assert!(!idx.contains_key("jjj-d-1.0.drv"));
    }

    #[test]
    fn roots_that_agree_on_a_compound_license_the_member() {
        let (idx, amb) = index(&[
            row_of("kkk-a-1.0.drv", "pkgs", vec![llvm()]),
            row_of("kkk-a-1.0.drv", "haskellPackages", vec![llvm()]),
        ]);
        assert_eq!(amb, 0);
        assert_eq!(idx["kkk-a-1.0.drv"].len(), 1);
    }
}
