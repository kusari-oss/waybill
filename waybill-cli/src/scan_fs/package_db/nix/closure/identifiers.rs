//! Milestone 1377 (#1394) — the CPE nixpkgs declares for a closure member.
//!
//! nixpkgs states a CPE in `meta.identifiers.cpe` where its maintainers know
//! the vendor: openssh, curl, openssl and zlib among others at the measured
//! revision. Where it records a product but no vendor (`cpeParts` only:
//! python313, httpie, gh) there is no `cpe`, and nothing is synthesised here
//! from the parts (FR-006).
//!
//! The value arrives in the milestone-1281 meta evaluation, one row per
//! attribute, and is joined to closure members in two steps (research R1):
//!
//! 1. **by `.drv` basename**, the same evidence the license join uses;
//! 2. otherwise **by `pname` + `version`**. A CPE names a vendor, a product
//!    and a version, not a build, so a member built in a different context
//!    (stdenv's `bash 5.3p9`, a second `perl 5.42.0`) carries the CPE of the
//!    attribute with that name and version. Measured: this recovers exactly
//!    the declared CPE for the 2 of 17 declared-CPE members a drv join
//!    misses on openssh's closure, and adds no ambiguity (0 of 109
//!    name+version joins disagree).
//!
//! Where several rows disagree the member gets no declared CPE, and the
//! disagreement is counted, as milestone 1281 withholds an ambiguous license.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::super::eval::licenses::LicenseRow;

/// C212 — this component's CPE is nixpkgs-declared. The value is the JSON
/// string `{"basis":"drv"|"name-version","overrode":"<curated cpe>"}`, with
/// `overrode` omitted when the curated table agreed or had no row.
///
/// Its presence is also the signal the native provenance carriers read
/// ([`cpe_source`]). It carries only what no format has a field for, the
/// join basis and the overridden identifier (FR-008, Principle V).
pub(crate) const ANN_DECLARED_CPE: &str = "waybill:cpe-provenance";

/// Internal: this closure component's CPE came from the curated table.
/// Drives the native carriers only; never emitted
/// (`root_selector::is_internal_emission_key`).
pub(crate) const ANN_CURATED_CPE: &str = "waybill:nix-curated-cpe";

/// SPDX 2.3 `externalRefs[].comment` and SPDX 3 `ExternalIdentifier.comment`
/// on a nixpkgs-declared CPE (contracts/wire.md §3).
pub const DECLARED_CPE_COMMENT: &str = "CPE declared by nixpkgs meta.identifiers.cpe";
/// The same, on a CPE from waybill's curated table.
pub const CURATED_CPE_COMMENT: &str = "CPE from waybill's curated CPE table";

/// Where a closure component's CPE came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpeSource {
    NixpkgsDeclared,
    Curated,
}

impl CpeSource {
    /// The comment both SPDX formats carry on the CPE.
    pub fn comment(self) -> &'static str {
        match self {
            Self::NixpkgsDeclared => DECLARED_CPE_COMMENT,
            Self::Curated => CURATED_CPE_COMMENT,
        }
    }
}

/// The recorded source of `c`'s CPEs, or `None` when nothing recorded one:
/// every non-closure component, and a closure component whose CPE another
/// reader or the operator supplied (FR-013). One function for all three
/// emitters, so the formats agree by construction.
pub fn cpe_source(c: &waybill_common::resolution::ResolvedComponent) -> Option<CpeSource> {
    if c.cpes.is_empty() {
        return None;
    }
    if c.extra_annotations.contains_key(ANN_DECLARED_CPE) {
        Some(CpeSource::NixpkgsDeclared)
    } else if c.extra_annotations.contains_key(ANN_CURATED_CPE) {
        Some(CpeSource::Curated)
    } else {
        None
    }
}

/// A CPE 2.3 formatted string that parsed (Constitution Principle IV).
///
/// Closure-local. The shared `ResolvedComponent.cpes` is `Vec<String>` for
/// every ecosystem (#1403); this type is converted to `String` exactly once,
/// where it is placed there.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct DeclaredCpe(String);

impl DeclaredCpe {
    /// Accept a CPE 2.3 formatted string: 13 colon-separated fields (a `\:`
    /// is an escaped colon inside a field), `cpe:2.3:` first, `part` one of
    /// `a`, `o`, `h`, and a vendor and product that are neither empty nor
    /// `*`. A vendorless CPE would match every product of that name, which
    /// is the heuristic a declared value is supposed to improve on.
    pub(crate) fn parse(value: &str) -> Option<Self> {
        let fields = split_fields(value);
        if fields.len() != 13 || fields[0] != "cpe" || fields[1] != "2.3" {
            return None;
        }
        if !matches!(fields[2].as_str(), "a" | "o" | "h") {
            return None;
        }
        let named = |f: &str| !f.is_empty() && f != "*";
        if !named(&fields[3]) || !named(&fields[4]) {
            return None;
        }
        Some(Self(value.to_string()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    /// The one conversion into the shared field's `String` (#1403).
    pub(crate) fn into_string(self) -> String {
        self.0
    }
}

/// Split on unescaped colons, keeping escapes in the field text.
fn split_fields(value: &str) -> Vec<String> {
    let mut fields = vec![String::new()];
    let mut escaped = false;
    for ch in value.chars() {
        let last = fields.len() - 1;
        if escaped {
            fields[last].push(ch);
            escaped = false;
        } else if ch == '\\' {
            fields[last].push(ch);
            escaped = true;
        } else if ch == ':' {
            fields.push(String::new());
        } else {
            fields[last].push(ch);
        }
    }
    fields
}

/// How a member was reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum JoinBasis {
    Drv,
    NameVersion,
}

impl JoinBasis {
    pub(crate) fn wire(self) -> &'static str {
        match self {
            Self::Drv => "drv",
            Self::NameVersion => "name-version",
        }
    }
}

/// A member's declared CPE and how it was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Declared {
    pub(crate) cpe: DeclaredCpe,
    pub(crate) basis: JoinBasis,
}

/// Declared CPEs keyed by `.drv` basename, as the license index is.
pub(crate) type IdentifierIndex = BTreeMap<String, Declared>;

/// C184 `identifiers`: how far the meta evaluation reached.
///
/// `wanted − received` is the unreached count FR-009 asks for. Invariants:
/// `by_drv + by_name_version == received <= wanted` and
/// `declared + rejected <= received`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct IdentifierCoverage {
    pub(crate) wanted: usize,
    pub(crate) received: usize,
    pub(crate) by_drv: usize,
    pub(crate) by_name_version: usize,
    pub(crate) declared: usize,
    /// Declared values that did not parse, plus joins whose rows disagreed.
    pub(crate) rejected: usize,
}

/// What the closure carries: the index and its coverage.
#[derive(Debug, Clone, Default)]
pub(crate) struct DeclaredIdentifiers {
    pub(crate) index: IdentifierIndex,
    pub(crate) coverage: IdentifierCoverage,
}

/// One member of the coverage population.
pub(crate) struct Member<'a> {
    /// `.drv` basename.
    pub(crate) key: &'a str,
    /// `RawDerivation::pname`, the name the evaluation was asked for.
    pub(crate) pname: &'a str,
    pub(crate) version: Option<&'a str>,
}

/// Join `members` to `rows`: by `.drv` first, then by `pname` + `version`.
pub(crate) fn join<'a>(
    rows: &[LicenseRow],
    members: impl IntoIterator<Item = Member<'a>>,
) -> DeclaredIdentifiers {
    let mut by_drv: HashMap<&str, Vec<&LicenseRow>> = HashMap::new();
    let mut by_attr: HashMap<&str, Vec<&LicenseRow>> = HashMap::new();
    for row in rows {
        by_drv.entry(row.drv.as_str()).or_default().push(row);
        by_attr.entry(row.attr.as_str()).or_default().push(row);
    }

    let mut out = DeclaredIdentifiers::default();
    for m in members {
        out.coverage.wanted += 1;
        let (basis, hits): (JoinBasis, Vec<&LicenseRow>) = match by_drv.get(m.key) {
            Some(rows) => (JoinBasis::Drv, rows.clone()),
            None => {
                let Some(version) = m.version else { continue };
                let hits: Vec<&LicenseRow> = by_attr
                    .get(m.pname)
                    .map(|rows| {
                        rows.iter()
                            .copied()
                            .filter(|r| r.version.as_deref() == Some(version))
                            .collect()
                    })
                    .unwrap_or_default();
                if hits.is_empty() {
                    continue; // unreached
                }
                (JoinBasis::NameVersion, hits)
            }
        };
        out.coverage.received += 1;
        match basis {
            JoinBasis::Drv => out.coverage.by_drv += 1,
            JoinBasis::NameVersion => out.coverage.by_name_version += 1,
        }

        // Every row the member reached must state the same CPE, absence
        // included: one root declaring a CPE and another declaring none is
        // a disagreement, not a vote.
        let stated: BTreeSet<Option<&str>> = hits.iter().map(|r| r.cpe.as_deref()).collect();
        let mut stated = stated.into_iter();
        let (Some(only), None) = (stated.next(), stated.next()) else {
            out.coverage.rejected += 1;
            tracing::debug!(
                drv = m.key,
                "nix-closure-identifiers: rows disagree about this member's CPE; emitting none"
            );
            continue;
        };
        let Some(value) = only else { continue };
        match DeclaredCpe::parse(value) {
            Some(cpe) => {
                out.coverage.declared += 1;
                out.index.insert(m.key.to_string(), Declared { cpe, basis });
            }
            None => {
                out.coverage.rejected += 1;
                tracing::warn!(
                    drv = m.key,
                    value,
                    "nix-closure-identifiers: nixpkgs declares a CPE that is not a valid \
                     CPE 2.3 name; emitting none"
                );
            }
        }
    }
    out
}

/// The C212 value carried in [`ANN_DECLARED_CPE`]: a JSON string, as every
/// per-component closure row is, so CycloneDX and both SPDX envelopes carry
/// the same bytes.
pub(crate) fn provenance(basis: JoinBasis, overrode: Option<&str>) -> serde_json::Value {
    let mut v = serde_json::json!({ "basis": basis.wire() });
    if let Some(o) = overrode {
        v["overrode"] = serde_json::Value::String(o.to_string());
    }
    serde_json::Value::String(v.to_string())
}

/// The join basis recorded in a C212 value.
pub(crate) fn provenance_basis(value: &serde_json::Value) -> Option<JoinBasis> {
    let parsed: serde_json::Value = serde_json::from_str(value.as_str()?).ok()?;
    match parsed.get("basis")?.as_str()? {
        "drv" => Some(JoinBasis::Drv),
        "name-version" => Some(JoinBasis::NameVersion),
        _ => None,
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    const OPENSSH: &str = "cpe:2.3:a:openbsd:openssh:10.3p1:*:*:*:*:*:*:*";
    const BASH: &str = "cpe:2.3:a:gnu:bash:5.3:9:*:*:*:*:*:*";

    fn row(drv: &str, attr: &str, version: Option<&str>, cpe: Option<&str>) -> LicenseRow {
        LicenseRow {
            drv: drv.to_string(),
            out: format!("/nix/store/out-{drv}"),
            attr: attr.to_string(),
            root: "pkgs".to_string(),
            licenses: vec![],
            version: version.map(str::to_string),
            cpe: cpe.map(str::to_string),
        }
    }

    fn member<'a>(key: &'a str, pname: &'a str, version: Option<&'a str>) -> Member<'a> {
        Member { key, pname, version }
    }

    #[test]
    fn a_well_formed_cpe_parses() {
        assert_eq!(DeclaredCpe::parse(OPENSSH).unwrap().as_str(), OPENSSH);
        assert!(DeclaredCpe::parse(BASH).is_some());
        assert!(DeclaredCpe::parse("cpe:2.3:o:linux:linux_kernel:6.1:*:*:*:*:*:*:*").is_some());
        // An escaped colon is part of a field, not a separator.
        assert!(DeclaredCpe::parse(r"cpe:2.3:a:vendor:prod\:uct:1:*:*:*:*:*:*:*").is_some());
    }

    #[test]
    fn malformed_cpes_are_rejected() {
        for bad in [
            "cpe:2.3:a:openbsd:openssh:10.3p1:*:*:*:*:*:*",       // 12 fields
            "cpe:2.3:a:openbsd:openssh:10.3p1:*:*:*:*:*:*:*:*",   // 14 fields
            "cpe:2.3:x:openbsd:openssh:10.3p1:*:*:*:*:*:*:*",     // bad part
            "cpe:2.2:a:openbsd:openssh:10.3p1:*:*:*:*:*:*:*",     // wrong version
            "cpe:2.3:a::openssh:10.3p1:*:*:*:*:*:*:*",            // empty vendor
            "cpe:2.3:a:*:openssh:10.3p1:*:*:*:*:*:*:*",           // any vendor
            "cpe:2.3:a:openbsd::10.3p1:*:*:*:*:*:*:*",            // empty product
            "cpe:2.3:a:openbsd:*:10.3p1:*:*:*:*:*:*:*",           // any product
            "",
        ] {
            assert!(DeclaredCpe::parse(bad).is_none(), "{bad:?} parsed");
        }
    }

    #[test]
    fn the_drv_join_is_preferred_to_name_version() {
        let rows = [
            row("d-ssh.drv", "openssh", Some("10.3p1"), Some(OPENSSH)),
            row("d-other.drv", "openssh", Some("10.3p1"), Some("cpe:2.3:a:other:openssh:10.3p1:*:*:*:*:*:*:*")),
        ];
        let got = join(&rows, [member("d-ssh.drv", "openssh", Some("10.3p1"))]);
        let d = &got.index["d-ssh.drv"];
        assert_eq!(d.basis, JoinBasis::Drv);
        assert_eq!(d.cpe.as_str(), OPENSSH);
        assert_eq!(got.coverage.by_drv, 1);
        assert_eq!(got.coverage.by_name_version, 0);
    }

    /// The bash shape: stdenv's bash is a different derivation from
    /// `pkgs.bash`, with the same name and version.
    #[test]
    fn a_member_missed_by_drv_is_reached_by_name_and_version() {
        let rows = [row("d-pkgs-bash.drv", "bash", Some("5.3p9"), Some(BASH))];
        let got = join(&rows, [member("d-stdenv-bash.drv", "bash", Some("5.3p9"))]);
        let d = &got.index["d-stdenv-bash.drv"];
        assert_eq!(d.basis, JoinBasis::NameVersion);
        assert_eq!(d.cpe.as_str(), BASH);
    }

    #[test]
    fn a_different_version_is_not_reached() {
        let rows = [row("d-pkgs-bash.drv", "bash", Some("5.3p9"), Some(BASH))];
        let got = join(&rows, [member("d-old.drv", "bash", Some("5.2p37"))]);
        assert!(got.index.is_empty());
        assert_eq!(got.coverage.received, 0);
        assert_eq!(got.coverage.wanted, 1);
    }

    #[test]
    fn name_version_rows_that_disagree_give_no_cpe_and_are_counted() {
        let rows = [
            row("d-a.drv", "perl", Some("5.42.0"), Some("cpe:2.3:a:perl:perl:5.42.0:*:*:*:*:*:*:*")),
            row("d-b.drv", "perl", Some("5.42.0"), Some("cpe:2.3:a:other:perl:5.42.0:*:*:*:*:*:*:*")),
        ];
        let got = join(&rows, [member("d-c.drv", "perl", Some("5.42.0"))]);
        assert!(got.index.is_empty());
        assert_eq!(got.coverage.received, 1);
        assert_eq!(got.coverage.rejected, 1);
        assert_eq!(got.coverage.declared, 0);
    }

    #[test]
    fn an_invalid_declared_cpe_is_not_carried_and_is_counted() {
        let rows = [row("d-x.drv", "x", Some("1"), Some("cpe:2.3:a::x:1:*:*:*:*:*:*:*"))];
        let got = join(&rows, [member("d-x.drv", "x", Some("1"))]);
        assert!(got.index.is_empty());
        assert_eq!(got.coverage.rejected, 1);
    }

    /// A row with only `cpeParts` reaches the member with `cpe: null`:
    /// received, not declared, nothing synthesised.
    #[test]
    fn a_reached_member_without_a_cpe_is_received_but_not_declared() {
        let rows = [row("d-gh.drv", "gh", Some("2.93.0"), None)];
        let got = join(&rows, [member("d-gh.drv", "gh", Some("2.93.0"))]);
        assert!(got.index.is_empty());
        assert_eq!(got.coverage.received, 1);
        assert_eq!(got.coverage.declared, 0);
        assert_eq!(got.coverage.rejected, 0);
    }

    #[test]
    fn coverage_invariants_hold() {
        let rows = [
            row("d-ssh.drv", "openssh", Some("10.3p1"), Some(OPENSSH)),
            row("d-pkgs-bash.drv", "bash", Some("5.3p9"), Some(BASH)),
            row("d-gh.drv", "gh", Some("2.93.0"), None),
            row("d-bad.drv", "bad", Some("1"), Some("not-a-cpe")),
        ];
        let members = [
            member("d-ssh.drv", "openssh", Some("10.3p1")),
            member("d-stdenv-bash.drv", "bash", Some("5.3p9")),
            member("d-gh.drv", "gh", Some("2.93.0")),
            member("d-bad.drv", "bad", Some("1")),
            member("d-unreached.drv", "nothing", Some("1")),
            member("d-noversion.drv", "bash", None),
        ];
        let c = join(&rows, members).coverage;
        assert_eq!(c.wanted, 6);
        assert_eq!(c.received, 4);
        assert_eq!(c.by_drv + c.by_name_version, c.received);
        assert!(c.received <= c.wanted);
        assert_eq!(c.declared, 2);
        assert_eq!(c.rejected, 1);
        assert!(c.declared + c.rejected <= c.received);
    }

    #[test]
    fn provenance_omits_overrode_when_nothing_was_overridden() {
        assert_eq!(provenance(JoinBasis::Drv, None), serde_json::json!(r#"{"basis":"drv"}"#));
        assert_eq!(
            provenance(JoinBasis::NameVersion, Some("cpe:2.3:a:gnu:bash:5.3p9:*:*:*:*:*:*:*")),
            serde_json::json!(
                r#"{"basis":"name-version","overrode":"cpe:2.3:a:gnu:bash:5.3p9:*:*:*:*:*:*:*"}"#
            )
        );
        for b in [JoinBasis::Drv, JoinBasis::NameVersion] {
            assert_eq!(provenance_basis(&provenance(b, Some("x"))), Some(b));
        }
    }
}
