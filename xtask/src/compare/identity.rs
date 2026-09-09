// milestone 780 — private comparative benchmark harness.
// Spec:     specs/780-comparative-bench-harness/spec.md
// Plan:     specs/780-comparative-bench-harness/plan.md
// Contract: specs/780-comparative-bench-harness/contracts/xtask-compare-cli.md
//
// Package-identity reduction (FR-006, FR-006a, FR-007).
//
// Hand-rolled rather than reusing `waybill_common::types::purl::Purl` on
// purpose (research.md R4). An instrument that shares code with its subject
// would apply the same normalisation bug to every tool AND to its own
// self-check, and the self-check would pass while the comparison was wrong.

use std::collections::BTreeSet;

/// A package, normalised so cosmetic differences between tools do not
/// register as different packages.
///
/// **Version is retained.** The duplication that motivated this harness was
/// the same name *and* version repeated once per manifest that required it,
/// so keeping the version removes it without merging genuinely distinct
/// findings. Version-stripping would understate a tool that correctly
/// resolves several versions of a package in a monorepo.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PackageIdentity {
    pub ptype: String,
    pub namespace: String,
    pub name: String,
    pub version: String,
}

impl std::fmt::Display for PackageIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.namespace.is_empty() {
            write!(f, "pkg:{}/{}@{}", self.ptype, self.name, self.version)
        } else {
            write!(
                f,
                "pkg:{}/{}/{}@{}",
                self.ptype, self.namespace, self.name, self.version
            )
        }
    }
}

/// Parse and normalise a package URL.
///
/// Returns `None` for anything that is not a usable identity, rather than
/// producing a partial one: a half-parsed identity would silently become a
/// distinct package and inflate a tool's count.
pub fn parse_purl(raw: &str) -> Option<PackageIdentity> {
    let raw = raw.trim();
    let rest = raw.strip_prefix("pkg:")?;
    // Qualifiers and subpath carry no identity for our purposes.
    let rest = rest.split('?').next()?;
    let rest = rest.split('#').next()?;

    let (before_version, version) = match rest.rsplit_once('@') {
        Some((b, v)) if !b.is_empty() && !v.is_empty() => (b, v),
        // No version. Not an error, but not comparable either — a tool that
        // omits versions cannot be scored against one that supplies them.
        _ => return None,
    };

    let mut segments: Vec<&str> = before_version.split('/').filter(|s| !s.is_empty()).collect();
    let name = segments.pop()?;
    if name.is_empty() {
        return None;
    }
    let ptype = if segments.is_empty() {
        return None;
    } else {
        segments.remove(0)
    };
    let namespace = segments.join("/");

    Some(PackageIdentity {
        ptype: ptype.to_ascii_lowercase(),
        namespace,
        name: name.to_string(),
        version: version.to_string(),
    })
}

/// What a reduction produced. All three numbers are reported; none is
/// derivable from the others.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reduction {
    pub identities: BTreeSet<PackageIdentity>,
    /// FR-006a — the entry count this reduced FROM. A large gap between
    /// this and `identities.len()` says the tool emits duplicates, which is
    /// a fact about the tool worth surfacing rather than normalising away.
    pub raw_components: usize,
    /// FR-007 — components with no usable package identity. Never folded
    /// into the package count in either direction.
    pub identityless: usize,
}

impl Reduction {
    pub fn distinct(&self) -> usize {
        self.identities.len()
    }
}

/// Reduce a CycloneDX document's `components[]` to distinct identities.
pub fn reduce_cyclonedx(doc: &serde_json::Value) -> Reduction {
    let mut identities = BTreeSet::new();
    let mut raw_components = 0usize;
    let mut identityless = 0usize;

    if let Some(components) = doc.get("components").and_then(|c| c.as_array()) {
        for component in components {
            raw_components += 1;
            match component.get("purl").and_then(|p| p.as_str()) {
                Some(purl) => match parse_purl(purl) {
                    Some(id) => {
                        identities.insert(id);
                    }
                    None => identityless += 1,
                },
                None => identityless += 1,
            }
        }
    }

    Reduction {
        identities,
        raw_components,
        identityless,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn id(s: &str) -> PackageIdentity {
        parse_purl(s).unwrap_or_else(|| panic!("expected {s} to parse"))
    }

    #[test]
    fn type_case_is_normalised() {
        assert_eq!(id("pkg:GoLang/foo@v1.0"), id("pkg:golang/foo@v1.0"));
    }

    #[test]
    fn qualifiers_and_subpath_are_dropped() {
        assert_eq!(
            id("pkg:golang/foo@v1.0?arch=amd64#sub/dir"),
            id("pkg:golang/foo@v1.0")
        );
    }

    #[test]
    fn namespace_is_preserved() {
        let p = id("pkg:golang/github.com/spf13/cobra@v1.8.0");
        assert_eq!(p.ptype, "golang");
        assert_eq!(p.namespace, "github.com/spf13");
        assert_eq!(p.name, "cobra");
        assert_eq!(p.version, "v1.8.0");
    }

    #[test]
    fn malformed_purls_are_rejected_not_partially_parsed() {
        // A partial identity would become a distinct package and inflate
        // the count.
        assert!(parse_purl("not-a-purl").is_none());
        assert!(parse_purl("pkg:").is_none());
        assert!(parse_purl("pkg:golang").is_none(), "no version");
        assert!(parse_purl("pkg:golang/foo").is_none(), "no version");
        assert!(parse_purl("pkg:golang/foo@").is_none(), "empty version");
    }

    /// THE regression test for the error that motivated this harness.
    ///
    /// One tool reported 2,355 Go components where 427 distinct modules
    /// existed — the same module once per manifest requiring it — and a
    /// raw-count comparison concluded waybill found 22% of the packages
    /// when it in fact found the most.
    #[test]
    fn duplicate_entries_reduce_to_one_identity() {
        let doc = json!({"components": (0..5)
            .map(|_| json!({"purl": "pkg:golang/foo@v1.0"}))
            .collect::<Vec<_>>()});
        let r = reduce_cyclonedx(&doc);
        assert_eq!(r.distinct(), 1, "five identical entries are one package");
        assert_eq!(r.raw_components, 5, "raw count must still be reported");
    }

    /// The other half, and the one that discriminates the chosen rule from
    /// the version-stripping rule used in the ad-hoc comparison. If someone
    /// later "simplifies" identity by dropping the version, this fails.
    #[test]
    fn two_versions_of_one_package_are_two_identities() {
        let doc = json!({"components": [
            {"purl": "pkg:golang/foo@v1.0"},
            {"purl": "pkg:golang/foo@v2.0"},
        ]});
        let r = reduce_cyclonedx(&doc);
        assert_eq!(
            r.distinct(),
            2,
            "version-stripping would merge these and understate a tool that \
             correctly resolves both"
        );
    }

    #[test]
    fn purl_less_components_are_counted_separately() {
        let doc = json!({"components": [
            {"purl": "pkg:golang/foo@v1.0"},
            {"name": "some-file.yml"},
            {"purl": "not-a-purl"},
        ]});
        let r = reduce_cyclonedx(&doc);
        assert_eq!(r.distinct(), 1);
        assert_eq!(r.identityless, 2, "one missing purl, one unparseable");
        assert_eq!(r.raw_components, 3);
        // The three numbers are independent; none may be inferred.
        assert_ne!(r.distinct() + r.identityless, r.raw_components - 1);
    }

    #[test]
    fn document_without_components_reduces_to_nothing() {
        let r = reduce_cyclonedx(&json!({}));
        assert_eq!(r.distinct(), 0);
        assert_eq!(r.raw_components, 0);
        assert_eq!(r.identityless, 0);
    }

    #[test]
    fn mixed_document_matches_hand_computed_counts() {
        let doc = json!({"components": [
            {"purl": "pkg:golang/github.com/a/b@v1.0"},
            {"purl": "pkg:golang/github.com/a/b@v1.0"},
            {"purl": "pkg:GOLANG/github.com/a/b@v1.0?x=y"},
            {"purl": "pkg:golang/github.com/a/b@v2.0"},
            {"purl": "pkg:npm/left-pad@1.3.0"},
            {"name": "LICENSE"},
        ]});
        let r = reduce_cyclonedx(&doc);
        assert_eq!(r.raw_components, 6);
        assert_eq!(r.identityless, 1);
        assert_eq!(r.distinct(), 3, "a@v1.0, a@v2.0, left-pad@1.3.0");
    }
}
