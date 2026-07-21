//! Milestone 116 PR-A T018 — backwards-compatibility verification.
//!
//! Per spec FR-014 / SC-005, source SBOMs that lack the new
//! `mikebom:produces-binaries` property MUST bind identically to the
//! pre-feature milestone-072 baseline:
//!
//! 1. A source SBOM with no `mikebom:produces-binaries` properties
//!    anywhere produces no auto-alias hits — every `binding_for_purl()`
//!    short-circuits to the existing exact-PURL match path.
//! 2. A milestone-111-era binding envelope (operator-supplied alias
//!    fields present, no `alias_source` field) deserializes cleanly
//!    via `#[serde(default)]`. Consumers reading `alias_source` find
//!    `None`.

use waybill::binding::{AliasSource, BindingStrength, SourceDocumentBinding, SourceSbomContext};
use serde_json::json;

fn write_sbom(dir: &std::path::Path, sbom: serde_json::Value) -> std::path::PathBuf {
    let p = dir.join("source.cdx.json");
    std::fs::write(&p, sbom.to_string()).expect("write source sbom");
    p
}

#[test]
fn source_sbom_without_produces_binaries_binds_identically_to_baseline() {
    // SC-005: pre-feature source SBOM (no produces-binaries) should
    // bind identically to milestone-072 baseline. Every binding_for_purl
    // call either hits the exact-PURL match path OR returns the
    // unchanged Unknown(source-not-found-in-bind-target).
    let tmp = tempfile::tempdir().expect("tempdir");
    let src = write_sbom(
        tmp.path(),
        json!({
            "bomFormat": "CycloneDX",
            "specVersion": "1.6",
            "components": [
                {
                    "type": "library",
                    "purl": "pkg:cargo/baz@1.0.0"
                    // intentionally NO produces-binaries property
                },
                {
                    "type": "library",
                    "purl": "pkg:cargo/transitive-dep@0.1.0"
                }
            ]
        }),
    );
    let ctx = SourceSbomContext::load(&src).expect("load source sbom");

    // The index MUST be empty when no source component carries
    // produces-binaries — this is the structural guarantee of the
    // backwards-compat path (auto-alias lookup short-circuits on
    // empty-index).
    assert!(
        ctx.binary_name_to_purl.is_empty(),
        "expected empty binary_name_to_purl index, got {:?}",
        ctx.binary_name_to_purl
    );

    // Exact-match path: PURL in source SBOM → Unknown with
    // source-tier-binding-evidence-missing reason (no binding envelope
    // attached to source-tier components in this fixture).
    let b = ctx.binding_for_purl("pkg:cargo/baz@1.0.0");
    assert_eq!(b.strength, BindingStrength::Unknown);
    assert_eq!(b.reason.as_deref(), Some("source-tier-binding-evidence-missing"));
    assert_eq!(b.alias_source, None);

    // No-exact-match + no-auto-alias path: PURL NOT in source SBOM →
    // unchanged Unknown(source-not-found-in-bind-target). This is the
    // milestone-072 baseline behavior.
    let b = ctx.binding_for_purl("pkg:generic/baz");
    assert_eq!(b.strength, BindingStrength::Unknown);
    assert_eq!(b.reason.as_deref(), Some("source-not-found-in-bind-target"));
    assert_eq!(b.alias_source, None);
}

#[test]
fn pre_feature_milestone_111_envelope_deserializes_cleanly() {
    // A milestone-111-era SBOM carries alias_from / alias_to but no
    // alias_source field (the field doesn't exist yet in 111). The
    // post-feature deserializer MUST handle this via #[serde(default)]
    // without error AND present alias_source = None to consumers.
    let pre_feature_json = json!({
        "source_doc_id": {
            "sha256": "deadbeef".repeat(8)
        },
        "strength": "weak",
        "algo": "v1",
        "alias_from": "pkg:generic/baz",
        "alias_to": "pkg:cargo/baz@1.0.0"
        // intentionally NO alias_source field
    });
    let binding: SourceDocumentBinding = serde_json::from_value(pre_feature_json)
        .expect("pre-feature envelope must deserialize cleanly");
    assert!(
        binding.alias_from.is_some(),
        "milestone-111 alias_from preserved"
    );
    assert!(binding.alias_to.is_some(), "milestone-111 alias_to preserved");
    assert_eq!(
        binding.alias_source, None,
        "absent alias_source presents as None per #[serde(default)]"
    );
    assert_eq!(binding.strength, BindingStrength::Weak);
}

#[test]
fn pre_feature_envelope_serializes_round_trip_byte_identical() {
    // A `SourceDocumentBinding` constructed without alias_source MUST
    // serialize to JSON WITHOUT the field (per #[serde(skip_serializing
    // _if = "Option::is_none")]). This guarantees byte-identical
    // emission for pre-feature scans even after upgrading the binary.
    let baseline = SourceDocumentBinding::unknown(
        waybill::binding::SourceDocumentId {
            sha256: "deadbeef".repeat(8),
            iri: None,
        },
        "source-not-found-in-bind-target",
    );
    let json = serde_json::to_value(&baseline).expect("serialize");
    assert!(
        json.get("alias_source").is_none(),
        "absent alias_source must NOT serialize: {json}"
    );
    assert!(
        json.get("alias_from").is_none(),
        "absent alias_from must NOT serialize: {json}"
    );
}

#[test]
fn post_feature_envelope_carries_alias_source_when_set() {
    // The complement check: when alias_source IS set, it serializes
    // with the kebab-case rendering.
    let binding = SourceDocumentBinding {
        source_doc_id: waybill::binding::SourceDocumentId {
            sha256: "feedface".repeat(8),
            iri: None,
        },
        hash: None,
        strength: BindingStrength::Weak,
        reason: None,
        algo: waybill::binding::BINDING_HASH_ALGO_V1.to_string(),
        alias_from: waybill_common::types::purl::Purl::new("pkg:generic/baz").ok(),
        alias_to: waybill_common::types::purl::Purl::new("pkg:cargo/baz@1.0.0").ok(),
        alias_source: Some(AliasSource::AutomaticFromProducesBinaries),
    };
    let json = serde_json::to_value(&binding).expect("serialize");
    assert_eq!(
        json.get("alias_source").and_then(|v| v.as_str()),
        Some("automatic-from-produces-binaries"),
        "alias_source serializes kebab-case: {json}"
    );
}
