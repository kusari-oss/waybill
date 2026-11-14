//! Where a component's emitted CPEs came from (spec 1491, milestone 1377).
//!
//! One function, [`cpe_source`], feeds all three emitters' native carriers:
//! CycloneDX `evidence.identity[]`, SPDX 2.3 `externalRefs[].comment` and
//! SPDX 3 `ExternalIdentifier.comment`. The formats therefore agree by
//! construction (spec 1491 FR-008).
//!
//! Precedence between sources (FR-005), highest first: `--root-cpe`, then
//! `--supplement-cdx` for the exact PURL, then a `--cpe-map` row, then
//! nixpkgs-declared, curated and heuristic. An operator source is made final
//! by [`mark_operator`]: every later CPE pass skips a marked component
//! (research R5).

use waybill_common::resolution::ResolvedComponent;
use waybill_common::types::cpe::Cpe;

use crate::scan_fs::package_db::nix::closure::identifiers as ids;

/// Internal: this component's CPEs were set by an operator surface, and no
/// later pass may replace them. The value is [`OperatorSurface::wire`].
/// Never emitted (`root_selector::is_internal_emission_key`).
pub(crate) const ANN_OPERATOR_CPE: &str = "waybill:operator-cpe";

/// The operator surface that supplied a CPE (contracts/cli.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperatorSurface {
    RootCpe,
    Supplement,
    CpeMap,
}

impl OperatorSurface {
    /// The marker's value.
    pub fn wire(self) -> &'static str {
        match self {
            Self::RootCpe => "root-cpe",
            Self::Supplement => "supplement",
            Self::CpeMap => "cpe-map",
        }
    }

    fn from_wire(s: &str) -> Option<Self> {
        match s {
            "root-cpe" => Some(Self::RootCpe),
            "supplement" => Some(Self::Supplement),
            "cpe-map" => Some(Self::CpeMap),
            _ => None,
        }
    }

    /// The CLI flag, for log messages.
    pub fn flag(self) -> &'static str {
        match self {
            Self::RootCpe => "--root-cpe",
            Self::Supplement => "--supplement-cdx",
            Self::CpeMap => "--cpe-map",
        }
    }

    /// C212's `basis`: how the operator's value was joined to the component
    /// (contracts/wire.md).
    pub(crate) fn basis(self) -> &'static str {
        match self {
            Self::RootCpe => "root-cpe",
            Self::Supplement => "purl",
            Self::CpeMap => "cpe-map",
        }
    }
}

/// SPDX `comment` strings for operator CPEs (contracts/wire.md).
pub const ROOT_CPE_COMMENT: &str = "CPE supplied by the operator with --root-cpe";
pub const SUPPLEMENT_CPE_COMMENT: &str = "CPE supplied by the operator with --supplement-cdx";
pub const CPE_MAP_COMMENT: &str = "CPE supplied by the operator with --cpe-map";

/// Where a component's CPEs came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpeSource {
    Operator(OperatorSurface),
    NixpkgsDeclared,
    Curated,
}

impl CpeSource {
    /// The comment both SPDX formats carry on the CPE.
    pub fn comment(self) -> &'static str {
        match self {
            Self::Operator(OperatorSurface::RootCpe) => ROOT_CPE_COMMENT,
            Self::Operator(OperatorSurface::Supplement) => SUPPLEMENT_CPE_COMMENT,
            Self::Operator(OperatorSurface::CpeMap) => CPE_MAP_COMMENT,
            Self::NixpkgsDeclared => ids::DECLARED_CPE_COMMENT,
            Self::Curated => ids::CURATED_CPE_COMMENT,
        }
    }

    /// The CycloneDX `evidence.identity[].methods[]` entry:
    /// `(technique, value, confidence)`. The confidences are ordinal, not
    /// calibrated: the operator's statement over nixpkgs' over waybill's
    /// table.
    pub fn cdx_method(self) -> (&'static str, &'static str, f64) {
        match self {
            Self::Operator(OperatorSurface::RootCpe) => ("other", "operator --root-cpe", 1.0),
            Self::Operator(OperatorSurface::Supplement) => {
                ("other", "operator --supplement-cdx", 1.0)
            }
            Self::Operator(OperatorSurface::CpeMap) => ("other", "operator --cpe-map", 1.0),
            Self::NixpkgsDeclared => ("manifest-analysis", "nixpkgs meta.identifiers.cpe", 0.9),
            Self::Curated => ("other", "waybill curated CPE table", 0.7),
        }
    }
}

/// The recorded source of `c`'s CPEs, or `None` when nothing recorded one:
/// a heuristic candidate, and a curated CPE on a non-closure component
/// (FR-010).
pub fn cpe_source(c: &ResolvedComponent) -> Option<CpeSource> {
    if c.cpes.is_empty() {
        return None;
    }
    if let Some(surface) = operator_surface(c) {
        Some(CpeSource::Operator(surface))
    } else if c.extra_annotations.contains_key(ids::ANN_DECLARED_CPE) {
        Some(CpeSource::NixpkgsDeclared)
    } else if c.extra_annotations.contains_key(ids::ANN_CURATED_CPE) {
        Some(CpeSource::Curated)
    } else {
        None
    }
}

/// The operator surface that set `c`'s CPEs, if one did.
pub(crate) fn operator_surface(c: &ResolvedComponent) -> Option<OperatorSurface> {
    c.extra_annotations
        .get(ANN_OPERATOR_CPE)
        .and_then(|v| v.as_str())
        .and_then(OperatorSurface::from_wire)
}

/// Give `c` the operator's CPEs and make them final.
///
/// Sets the marker and C212. `overrode` is the primary CPE `c` carried
/// before, recorded only when it differs from the new primary (FR-009). The
/// curated-table marker is dropped: the curated CPE is no longer emitted,
/// and nothing may describe a CPE that was not.
pub(crate) fn set_operator_cpes(c: &mut ResolvedComponent, cpes: Vec<Cpe>, surface: OperatorSurface) {
    let overrode = c
        .cpes
        .first()
        .filter(|old| Some(*old) != cpes.first())
        .map(|old| old.as_str().to_string());
    c.cpes = cpes;
    mark_operator(c, surface, overrode.as_deref());
}

/// Record that an operator surface set `c`'s CPEs: the marker, and C212
/// when there is a displaced primary to record.
///
/// With nothing displaced C212 is omitted (Principle V): its `basis` would
/// only restate the surface the native comment and CycloneDX method already
/// name. Any C212 already present is removed then, because it described a
/// CPE from another source.
pub(crate) fn mark_operator(c: &mut ResolvedComponent, surface: OperatorSurface, overrode: Option<&str>) {
    c.extra_annotations.remove(ids::ANN_CURATED_CPE);
    c.extra_annotations.insert(
        ANN_OPERATOR_CPE.to_string(),
        serde_json::Value::String(surface.wire().to_string()),
    );
    match overrode {
        Some(_) => {
            c.extra_annotations.insert(
                ids::ANN_DECLARED_CPE.to_string(),
                ids::provenance_with(surface.basis(), overrode),
            );
        }
        None => {
            c.extra_annotations.remove(ids::ANN_DECLARED_CPE);
        }
    }
}

/// Record `displaced` as the CPE an operator source overrode, when the
/// component carries no C212 yet and it differs from the emitted primary.
///
/// For a supplement entry that only later turns out to stand for a closure
/// member: the supplement merge saw nothing to displace, and the closure
/// merge knows what the member would have carried.
pub(crate) fn note_displaced(c: &mut ResolvedComponent, displaced: Option<&Cpe>) {
    let (Some(surface), Some(displaced)) = (operator_surface(c), displaced) else {
        return;
    };
    if c.cpes.first() == Some(displaced) {
        return;
    }
    if c.extra_annotations.contains_key(ids::ANN_DECLARED_CPE) {
        return;
    }
    c.extra_annotations.insert(
        ids::ANN_DECLARED_CPE.to_string(),
        ids::provenance_with(surface.basis(), Some(displaced.as_str())),
    );
}

/// The version field of a CPE 2.3 formatted string, unescaped. `None` for
/// `*` / `-` (any / not applicable) or a string with too few fields.
pub(crate) fn cpe_version(cpe: &Cpe) -> Option<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut chars = cpe.as_str().chars();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            ':' => fields.push(std::mem::take(&mut current)),
            other => current.push(other),
        }
    }
    fields.push(current);
    let version = fields.get(5)?;
    (version != "*" && version != "-").then(|| version.clone())
}

/// Spec edge case: an operator CPE whose version differs from the
/// component's is emitted as given (the operator owns the statement), with
/// a WARN, because it may be a typo and a matcher will believe it.
pub(crate) fn warn_on_version_mismatch(c: &ResolvedComponent, cpes: &[Cpe], surface: OperatorSurface) {
    if c.version.is_empty() {
        return;
    }
    for cpe in cpes {
        if let Some(v) = cpe_version(cpe) {
            if v != c.version {
                tracing::warn!(
                    component = %c.purl,
                    cpe = %cpe,
                    "{}: the CPE's version {v:?} differs from the component's {:?}; emitted as given",
                    surface.flag(),
                    c.version,
                );
            }
        }
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use waybill_common::resolution::{ResolutionEvidence, ResolutionTechnique};
    use waybill_common::types::purl::Purl;

    fn make_component(purl_str: &str) -> ResolvedComponent {
        let purl = Purl::new(purl_str).expect("valid purl");
        ResolvedComponent {
            build_inclusion: None,
            artifact_presence: None,
            scope_evidence: None,
            name: purl.name().to_string(),
            version: purl.version().unwrap_or("0.0.0").to_string(),
            purl,
            evidence: ResolutionEvidence {
                technique: ResolutionTechnique::PackageDatabase,
                confidence: 0.85,
                source_connection_ids: vec![],
                source_file_paths: vec![],
                deps_dev_match: None,
            },
            licenses: vec![],
            concluded_licenses: Vec::new(),
            hashes: vec![],
            supplier: None,
            cpes: vec![],
            advisories: vec![],
            occurrences: vec![],
            lifecycle_scope: None,
            requirement_ranges: Vec::new(),
            source_type: None,
            sbom_tier: None,
            buildinfo_status: None,
            evidence_kind: None,
            binary_class: None,
            binary_stripped: None,
            linkage_kind: None,
            detected_go: None,
            confidence: None,
            binary_packed: None,
            npm_role: None,
            raw_version: None,
            parent_purl: None,
            co_owned_by: None,
            shade_relocation: None,
            external_references: Vec::new(),
            extra_annotations: Default::default(),
            binary_role: None,
        }
    }

    fn component(cpes: &[&str]) -> ResolvedComponent {
        let mut c = make_component("pkg:deb/debian/waybill-fixture-lib@1.0");
        c.cpes = cpes.iter().map(|s| Cpe::parse(s).unwrap()).collect();
        c
    }

    const GUESS: &str = "cpe:2.3:a:debian:waybill-fixture-lib:1.0:*:*:*:*:*:*:*";
    const OPERATOR: &str = "cpe:2.3:a:waybill-fixture-vendor:waybill-fixture-lib:1.0:*:*:*:*:*:*:*";

    /// The operator outranks every other recorded source, including a
    /// nixpkgs-declared C212 already on the component.
    #[test]
    fn the_operator_source_comes_first() {
        let mut c = component(&[GUESS]);
        c.extra_annotations.insert(ids::ANN_CURATED_CPE.to_string(), serde_json::json!(true));
        assert_eq!(cpe_source(&c), Some(CpeSource::Curated));

        set_operator_cpes(&mut c, vec![Cpe::parse(OPERATOR).unwrap()], OperatorSurface::CpeMap);
        assert_eq!(cpe_source(&c), Some(CpeSource::Operator(OperatorSurface::CpeMap)));
        assert!(!c.extra_annotations.contains_key(ids::ANN_CURATED_CPE));
        assert_eq!(
            c.extra_annotations[ids::ANN_DECLARED_CPE],
            serde_json::json!(format!(r#"{{"basis":"cpe-map","overrode":"{GUESS}"}}"#))
        );
    }

    /// No C212 when the operator's primary equals the old one, or when there
    /// was none: nothing was displaced, and the native fields already name
    /// the surface. The source is still the operator's.
    #[test]
    fn c212_is_emitted_only_for_a_displaced_primary() {
        let mut same = component(&[OPERATOR]);
        set_operator_cpes(&mut same, vec![Cpe::parse(OPERATOR).unwrap()], OperatorSurface::Supplement);
        assert!(!same.extra_annotations.contains_key(ids::ANN_DECLARED_CPE));
        assert_eq!(cpe_source(&same), Some(CpeSource::Operator(OperatorSurface::Supplement)));

        let mut empty = component(&[]);
        set_operator_cpes(&mut empty, vec![Cpe::parse(OPERATOR).unwrap()], OperatorSurface::RootCpe);
        assert!(!empty.extra_annotations.contains_key(ids::ANN_DECLARED_CPE));
        assert_eq!(cpe_source(&empty), Some(CpeSource::Operator(OperatorSurface::RootCpe)));
    }

    /// A nixpkgs C212 on a component whose CPE the operator restates is
    /// removed: it described nixpkgs' CPE, and the emitted one is the
    /// operator's.
    ///
    /// CONTROL: with a different operator CPE the record is replaced, not
    /// removed, so the equality is what decides.
    #[test]
    fn an_operator_restating_a_declared_cpe_drops_its_c212() {
        let declared = || {
            let mut c = component(&[OPERATOR]);
            c.extra_annotations.insert(
                ids::ANN_DECLARED_CPE.to_string(),
                ids::provenance(ids::JoinBasis::Drv, Some(GUESS)),
            );
            c
        };
        let mut c = declared();
        set_operator_cpes(&mut c, vec![Cpe::parse(OPERATOR).unwrap()], OperatorSurface::CpeMap);
        assert!(!c.extra_annotations.contains_key(ids::ANN_DECLARED_CPE));

        let mut control = declared();
        set_operator_cpes(&mut control, vec![Cpe::parse(GUESS).unwrap()], OperatorSurface::CpeMap);
        assert_eq!(
            control.extra_annotations[ids::ANN_DECLARED_CPE],
            serde_json::json!(format!(r#"{{"basis":"cpe-map","overrode":"{OPERATOR}"}}"#))
        );
    }

    /// A supplement that later turns out to stand for a closure member
    /// records what that member would have emitted, once.
    #[test]
    fn note_displaced_fills_an_absent_record_only() {
        let mut c = component(&[OPERATOR]);
        mark_operator(&mut c, OperatorSurface::Supplement, None);
        note_displaced(&mut c, Some(&Cpe::parse(OPERATOR).unwrap()));
        assert!(!c.extra_annotations.contains_key(ids::ANN_DECLARED_CPE));

        note_displaced(&mut c, Some(&Cpe::parse(GUESS).unwrap()));
        let first = serde_json::json!(format!(r#"{{"basis":"purl","overrode":"{GUESS}"}}"#));
        assert_eq!(c.extra_annotations[ids::ANN_DECLARED_CPE], first);

        let other = "cpe:2.3:a:waybill-fixture-other:waybill-fixture-lib:1.0:*:*:*:*:*:*:*";
        note_displaced(&mut c, Some(&Cpe::parse(other).unwrap()));
        assert_eq!(c.extra_annotations[ids::ANN_DECLARED_CPE], first);
    }

    /// The version field is read past escaped colons, and `*`/`-` are none.
    #[test]
    fn cpe_version_reads_the_sixth_field() {
        let v = |s: &str| cpe_version(&Cpe::parse(s).unwrap());
        assert_eq!(v("cpe:2.3:a:gnu:bash:5.3:9:*:*:*:*:*:*").as_deref(), Some("5.3"));
        assert_eq!(v(r"cpe:2.3:a:v:p:1\:2:*:*:*:*:*:*:*").as_deref(), Some("1:2"));
        assert_eq!(v("cpe:2.3:a:v:p:*:*:*:*:*:*:*:*"), None);
        assert_eq!(v("cpe:2.3:a:v:p:-:*:*:*:*:*:*:*"), None);
    }

    /// Each surface has its own comment and CycloneDX method value, and no
    /// two sources share either.
    #[test]
    fn every_source_has_distinct_wire_strings() {
        let all = [
            CpeSource::Operator(OperatorSurface::RootCpe),
            CpeSource::Operator(OperatorSurface::Supplement),
            CpeSource::Operator(OperatorSurface::CpeMap),
            CpeSource::NixpkgsDeclared,
            CpeSource::Curated,
        ];
        let comments: std::collections::HashSet<_> = all.iter().map(|s| s.comment()).collect();
        let values: std::collections::HashSet<_> = all.iter().map(|s| s.cdx_method().1).collect();
        assert_eq!(comments.len(), all.len());
        assert_eq!(values.len(), all.len());
        for s in [OperatorSurface::RootCpe, OperatorSurface::Supplement, OperatorSurface::CpeMap] {
            assert_eq!(OperatorSurface::from_wire(s.wire()), Some(s));
        }
    }
}
