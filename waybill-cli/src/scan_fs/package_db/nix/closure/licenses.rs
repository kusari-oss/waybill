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
fn to_expression(raw: &DeclaredLicenseRaw) -> Option<SpdxExpression> {
    let value = raw.spdx.as_deref().or(raw.name.as_deref())?.trim();
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
        // collapses any difference.
        let shapes: BTreeSet<Vec<(Option<&str>, Option<&str>)>> = rows
            .iter()
            .map(|r| {
                r.licenses
                    .iter()
                    .map(|l| (l.spdx.as_deref(), l.name.as_deref()))
                    .collect()
            })
            .collect();
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
            drv: drv.to_string(),
            out: format!("/nix/store/out-{drv}"),
            attr: "waybill-fixture-pkg".to_string(),
            root: root.to_string(),
            licenses: lics
                .iter()
                .map(|(s, n)| DeclaredLicenseRaw {
                    spdx: s.map(str::to_string),
                    name: n.map(str::to_string),
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
}
