//! Lifecycle-phase aggregation shared between CDX and SPDX
//! serializers (milestone 047, extended in milestone 081).
//!
//! Maps the per-component `mikebom:sbom-tier` value (one of
//! `design`, `source`, `build`, `deployed`, `analyzed`) to the
//! corresponding CycloneDX 1.6 / SPDX-comment phase name, and
//! aggregates the observed phase set across a scan's components.
//!
//! Source-of-truth for CDX `metadata.lifecycles[]`, the SPDX
//! 2.3 `creationInfo.comment` / SPDX 3 `SpdxDocument.comment`
//! aggregation, AND (milestone 081) the SPDX 3 native
//! `software_Sbom.software_sbomType[]` field. All serializers
//! MUST call into this module so the phase set is identical
//! regardless of output format.
//!
//! Sort order is deterministic (lexicographic via `BTreeSet`)
//! so byte-identity goldens regen cleanly.
//!
//! Coverage: end-to-end byte-identity goldens
//! (`mikebom-cli/tests/cdx_regression.rs`,
//! `mikebom-cli/tests/spdx_regression.rs`,
//! `mikebom-cli/tests/spdx3_regression.rs`) exercise both
//! functions through the live serializer pipeline. Any change
//! to phase mapping or aggregation order surfaces as a goldens
//! regression.
//!
//! Milestone 081 additions:
//! - [`SbomType`] enum: the 6 CISA SBOM Types vocabulary used by
//!   the `--sbom-type` operator-assert flag and as the source
//!   set for SPDX 3 native-field emission.
//! - [`tier_to_spdx3_sbomtype_value`]: per-tier → SPDX 3
//!   `software_SbomType` short-name mapping (mirrors
//!   `tier_to_phase`).
//! - [`aggregate_spdx3_sbom_types`]: SPDX 3 emission helper
//!   mirroring [`aggregate_phases`], with the same
//!   override-assertion mechanics.

use std::collections::BTreeSet;

use mikebom_common::resolution::ResolvedComponent;

/// The 6 CISA SBOM Types (April 2023). Mapped 1:1 with SPDX 3's
/// `software_SbomType` enum and (via [`tier_to_phase`]) with
/// CDX 1.6's `metadata.lifecycles[].phase` enum.
///
/// Used by:
/// - The `--sbom-type` operator-assert flag (milestone 081 US3) to
///   override the auto-detected document-level SBOM-type signal in
///   all three formats while preserving per-component
///   `mikebom:sbom-tier` annotations.
/// - The SPDX 3 native-field emission path
///   (`software_Sbom.software_sbomType[]`) in
///   `mikebom-cli/src/generate/spdx/v3_document.rs`.
///
/// Per Constitution Principle V (standards-native first), this
/// vocabulary mirrors the CISA framework verbatim — no
/// `mikebom:`-prefix bridge needed because the SPDX 3 native field
/// already accepts these 6 values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SbomType {
    Design,
    Source,
    Build,
    Analyzed,
    Deployed,
    Runtime,
}

impl SbomType {
    /// Returns the SPDX 3 `software_SbomType` wire-format value the
    /// emission writes into `software_Sbom.software_sbomType[]`.
    ///
    /// Wire format: bare lowercase short-name per the SPDX 3 JSON
    /// schema's `prop_software_Sbom_software_sbomType` enum
    /// (`mikebom-cli/tests/fixtures/schemas/spdx-3.0.1.json`). The
    /// JSON-LD `@context` resolves these short names to full IRIs
    /// (`spdx:Software/SbomType/<name>`) at consumption time, but
    /// the wire shape mirrors the existing
    /// `externalIdentifierType` / `relationshipType` convention
    /// (bare strings, not IRIs) used throughout mikebom's SPDX 3
    /// emission.
    ///
    /// Per VR-081-001 / VR-081-004.
    pub fn as_spdx3_iri(&self) -> &'static str {
        match self {
            Self::Design => "design",
            Self::Source => "source",
            Self::Build => "build",
            Self::Analyzed => "analyzed",
            Self::Deployed => "deployed",
            Self::Runtime => "runtime",
        }
    }

    /// Returns the lowercase short-name for `--sbom-type` flag
    /// parsing and round-trip with the `mikebom:sbom-tier`
    /// per-component vocabulary.
    #[allow(dead_code)]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Design => "design",
            Self::Source => "source",
            Self::Build => "build",
            Self::Analyzed => "analyzed",
            Self::Deployed => "deployed",
            Self::Runtime => "runtime",
        }
    }

    /// Parse a `--sbom-type` flag value. Case-sensitive; valid set
    /// is `{design, source, build, analyzed, deployed, runtime}`.
    /// Per VR-081-001.
    pub fn parse_str(s: &str) -> Result<Self, ParseSbomTypeError> {
        match s {
            "design" => Ok(Self::Design),
            "source" => Ok(Self::Source),
            "build" => Ok(Self::Build),
            "analyzed" => Ok(Self::Analyzed),
            "deployed" => Ok(Self::Deployed),
            "runtime" => Ok(Self::Runtime),
            other => Err(ParseSbomTypeError {
                value: other.to_string(),
            }),
        }
    }

    /// Map this `SbomType` to its CDX 1.6
    /// `metadata.lifecycles[].phase` value via the equivalence
    /// table (research §2):
    ///
    /// | CISA      | mikebom    | CDX phase    |
    /// |-----------|------------|--------------|
    /// | Design    | `design`   | `design`     |
    /// | Source    | `source`   | `pre-build`  |
    /// | Build     | `build`    | `build`      |
    /// | Analyzed  | `analyzed` | `post-build` |
    /// | Deployed  | `deployed` | `operations` |
    /// | Runtime   | `runtime`  | `operations` |
    ///
    /// Note: CDX 1.6 has no `runtime` phase value; CISA Runtime
    /// maps to the closest CDX equivalent (`operations`, per OBOM
    /// narrative). SPDX 3 expresses Runtime cleanly via its own
    /// dedicated value.
    pub fn as_cdx_phase(&self) -> &'static str {
        match self {
            Self::Design => "design",
            Self::Source => "pre-build",
            Self::Build => "build",
            Self::Analyzed => "post-build",
            Self::Deployed => "operations",
            Self::Runtime => "operations",
        }
    }
}

/// Parse error for [`SbomType::parse_str`]. The error message
/// reports the rejected value verbatim and lists the valid
/// vocabulary so operators understand what to use.
#[derive(Debug, thiserror::Error)]
#[error(
    "--sbom-type '{value}' is not a valid CISA SBOM type; valid values are design/source/build/analyzed/deployed/runtime"
)]
pub struct ParseSbomTypeError {
    pub value: String,
}

/// Map a `mikebom:sbom-tier` string to its corresponding
/// CycloneDX 1.5+ `lifecycles[].phase` value. Returns `None` for
/// unrecognised tier strings so unknown tiers don't pollute the
/// aggregated phase set.
pub fn tier_to_phase(tier: &str) -> Option<&'static str> {
    match tier {
        "build" => Some("build"),
        "deployed" => Some("operations"),
        "analyzed" => Some("post-build"),
        "source" => Some("pre-build"),
        "design" => Some("design"),
        _ => None,
    }
}

/// Map a `mikebom:sbom-tier` string to its corresponding SPDX 3
/// `software_SbomType` short-name (wire form). Returns `None` for
/// unrecognised tiers (matches the [`tier_to_phase`] resilience
/// pattern for unknown-tier inputs).
///
/// 1:1 mapping per research §2 equivalence table — all 6 mikebom
/// tier values round-trip cleanly with the SPDX 3 vocabulary,
/// unlike CDX which lacks a dedicated `runtime` phase.
///
/// Per VR-081-002.
pub fn tier_to_spdx3_sbomtype_value(tier: &str) -> Option<&'static str> {
    match tier {
        "design" => Some("design"),
        "source" => Some("source"),
        "build" => Some("build"),
        "analyzed" => Some("analyzed"),
        "deployed" => Some("deployed"),
        "runtime" => Some("runtime"),
        _ => None,
    }
}

/// Compatibility alias matching the spec docs' naming (research
/// §1, data-model.md, contracts/sbom-type-signaling.md). The
/// returned value is the JSON-LD short-form (e.g. `"build"`) the
/// SPDX 3 schema's `prop_software_Sbom_software_sbomType` enum
/// validates; the JSON-LD `@context` resolves it to the full IRI
/// (`spdx:Software/SbomType/build`) at consumption time. See
/// [`tier_to_spdx3_sbomtype_value`] for implementation.
#[allow(dead_code)]
pub fn tier_to_spdx3_sbomtype_iri(tier: &str) -> Option<&'static str> {
    tier_to_spdx3_sbomtype_value(tier)
}

/// Aggregate the unique set of CDX phase names observed across
/// the given components' `sbom_tier` values. Returns the phase
/// list sorted lexicographically (deterministic for byte-identity
/// goldens).
///
/// Milestone 081: when `override_assertion` is `Some(SbomType)`,
/// returns a single-element Vec with the operator-asserted CDX
/// phase via the equivalence table (per research §4 override
/// semantics). Per-component tier values in the input are IGNORED
/// in this case — the operator's document-level claim wins.
///
/// When `override_assertion` is `None`, retains the
/// milestone-047 aggregation behavior verbatim (back-compat for
/// existing call sites that don't yet thread the override).
pub fn aggregate_phases<'a>(
    components: impl IntoIterator<Item = &'a ResolvedComponent>,
    override_assertion: Option<SbomType>,
) -> Vec<&'static str> {
    if let Some(t) = override_assertion {
        return vec![t.as_cdx_phase()];
    }
    let mut phases: BTreeSet<&'static str> = BTreeSet::new();
    for c in components {
        if let Some(ref tier) = c.sbom_tier {
            if let Some(phase) = tier_to_phase(tier) {
                phases.insert(phase);
            }
        }
    }
    phases.into_iter().collect()
}

/// Aggregate the unique set of SPDX 3 `software_SbomType` short-
/// name values observed across the given components' `sbom_tier`
/// values. Returns the value list sorted lexicographically
/// (deterministic for byte-identity goldens). Mirrors the
/// [`aggregate_phases`] pattern.
///
/// When `override_assertion` is `Some(SbomType)`, returns a
/// single-element Vec with the operator-asserted SPDX 3 value
/// (per research §4 override semantics). Per-component tier
/// values in the input are IGNORED in this case — the operator's
/// document-level claim wins.
///
/// Per VR-081-003.
pub fn aggregate_spdx3_sbom_types<'a>(
    components: impl IntoIterator<Item = &'a ResolvedComponent>,
    override_assertion: Option<SbomType>,
) -> Vec<&'static str> {
    if let Some(t) = override_assertion {
        return vec![t.as_spdx3_iri()];
    }
    let mut values: BTreeSet<&'static str> = BTreeSet::new();
    for c in components {
        if let Some(ref tier) = c.sbom_tier {
            if let Some(v) = tier_to_spdx3_sbomtype_value(tier) {
                values.insert(v);
            }
        }
    }
    values.into_iter().collect()
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use mikebom_common::resolution::{
        ResolutionEvidence, ResolutionTechnique, ResolvedComponent,
    };
    use mikebom_common::types::purl::Purl;

    fn component_with_tier(
        purl: &str,
        tier: Option<&str>,
    ) -> ResolvedComponent {
        ResolvedComponent {
            build_inclusion: None,
            purl: Purl::new(purl).unwrap(),
            name: purl.to_string(),
            version: "0.0.0".to_string(),
            evidence: ResolutionEvidence {
                technique: ResolutionTechnique::UrlPattern,
                confidence: 0.9,
                source_connection_ids: vec![],
                source_file_paths: vec![],
                deps_dev_match: None,
            },
            licenses: vec![],
            concluded_licenses: vec![],
            hashes: vec![],
            supplier: None,
            cpes: vec![],
            advisories: vec![],
            occurrences: vec![],
            lifecycle_scope: None,
            requirement_range: None,
            source_type: None,
            sbom_tier: tier.map(|s| s.to_string()),
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

    #[test]
    fn tier_to_phase_maps_known_tiers() {
        assert_eq!(tier_to_phase("design"), Some("design"));
        assert_eq!(tier_to_phase("source"), Some("pre-build"));
        assert_eq!(tier_to_phase("build"), Some("build"));
        assert_eq!(tier_to_phase("deployed"), Some("operations"));
        assert_eq!(tier_to_phase("analyzed"), Some("post-build"));
    }

    #[test]
    fn tier_to_phase_returns_none_for_unknown() {
        assert_eq!(tier_to_phase("unknown-tier"), None);
        assert_eq!(tier_to_phase(""), None);
    }

    // ----- Milestone 081 unit tests (T003) ---------------------------

    #[test]
    fn sbom_type_parse_str_accepts_six_vocab_values() {
        assert_eq!(SbomType::parse_str("design").unwrap(), SbomType::Design);
        assert_eq!(SbomType::parse_str("source").unwrap(), SbomType::Source);
        assert_eq!(SbomType::parse_str("build").unwrap(), SbomType::Build);
        assert_eq!(
            SbomType::parse_str("analyzed").unwrap(),
            SbomType::Analyzed
        );
        assert_eq!(
            SbomType::parse_str("deployed").unwrap(),
            SbomType::Deployed
        );
        assert_eq!(
            SbomType::parse_str("runtime").unwrap(),
            SbomType::Runtime
        );
    }

    #[test]
    fn sbom_type_parse_str_rejects_invalid_value() {
        // case mismatch — case-sensitive per spec/contract
        assert!(SbomType::parse_str("Build").is_err());
        // garbage
        assert!(SbomType::parse_str("foobar").is_err());
        // empty
        assert!(SbomType::parse_str("").is_err());

        // verify the error message format includes the rejected
        // value AND the valid vocabulary list
        let err = SbomType::parse_str("foobar").unwrap_err();
        let s = err.to_string();
        assert!(
            s.contains("foobar"),
            "error must echo the rejected value, got: {s}"
        );
        assert!(
            s.contains("design/source/build/analyzed/deployed/runtime"),
            "error must list the valid vocab, got: {s}"
        );
    }

    #[test]
    fn tier_to_spdx3_sbomtype_iri_returns_correct_iri_for_each_tier() {
        // The "iri" name is historical (per data-model.md) — wire
        // values are JSON-LD short-names per the schema enum.
        assert_eq!(tier_to_spdx3_sbomtype_iri("design"), Some("design"));
        assert_eq!(tier_to_spdx3_sbomtype_iri("source"), Some("source"));
        assert_eq!(tier_to_spdx3_sbomtype_iri("build"), Some("build"));
        assert_eq!(
            tier_to_spdx3_sbomtype_iri("analyzed"),
            Some("analyzed")
        );
        assert_eq!(
            tier_to_spdx3_sbomtype_iri("deployed"),
            Some("deployed")
        );
        assert_eq!(tier_to_spdx3_sbomtype_iri("runtime"), Some("runtime"));
    }

    #[test]
    fn tier_to_spdx3_sbomtype_iri_returns_none_for_unknown_tier() {
        assert_eq!(tier_to_spdx3_sbomtype_iri("unknown-tier"), None);
        assert_eq!(tier_to_spdx3_sbomtype_iri(""), None);
        // case mismatch is treated as unknown
        assert_eq!(tier_to_spdx3_sbomtype_iri("Build"), None);
    }

    #[test]
    fn aggregate_spdx3_sbom_types_with_override_returns_single_element_vec() {
        let comps = [
            component_with_tier("pkg:cargo/foo@1.0", Some("source")),
            component_with_tier("pkg:cargo/bar@1.0", Some("build")),
            component_with_tier("pkg:cargo/baz@1.0", Some("analyzed")),
        ];
        let out =
            aggregate_spdx3_sbom_types(comps.iter(), Some(SbomType::Build));
        // Override wins — per-component tiers are ignored.
        assert_eq!(out, vec!["build"]);

        // Same shape for runtime (the `--sbom-type runtime`
        // operator-self-assertion path that auto-detection doesn't
        // cover yet — research §3).
        let out2 =
            aggregate_spdx3_sbom_types(comps.iter(), Some(SbomType::Runtime));
        assert_eq!(out2, vec!["runtime"]);
    }

    #[test]
    fn aggregate_spdx3_sbom_types_without_override_aggregates_lex_sorted() {
        let comps = [
            component_with_tier("pkg:cargo/foo@1.0", Some("source")),
            component_with_tier("pkg:cargo/bar@1.0", Some("build")),
        ];
        let out = aggregate_spdx3_sbom_types(comps.iter(), None);
        // Lex-sorted: "build" < "source"
        assert_eq!(out, vec!["build", "source"]);
    }

    #[test]
    fn aggregate_spdx3_sbom_types_empty_when_no_components_carry_tiers() {
        let comps = [component_with_tier("pkg:cargo/foo@1.0", None)];
        let out = aggregate_spdx3_sbom_types(comps.iter(), None);
        assert!(out.is_empty());
    }

    #[test]
    fn aggregate_phases_override_returns_single_element_vec() {
        let comps = [
            component_with_tier("pkg:cargo/foo@1.0", Some("source")),
            component_with_tier("pkg:cargo/bar@1.0", Some("build")),
        ];
        // Without override — original milestone-047 behavior.
        let auto = aggregate_phases(comps.iter(), None);
        assert_eq!(auto, vec!["build", "pre-build"]);

        // With override — single element via the equivalence table.
        let asserted = aggregate_phases(comps.iter(), Some(SbomType::Build));
        assert_eq!(asserted, vec!["build"]);

        // Source override → CDX `pre-build` per equivalence table.
        let src_assert = aggregate_phases(comps.iter(), Some(SbomType::Source));
        assert_eq!(src_assert, vec!["pre-build"]);
    }
}
