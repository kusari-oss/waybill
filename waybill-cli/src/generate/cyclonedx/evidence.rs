use serde_json::json;

use waybill_common::resolution::{FileOccurrence, ResolutionEvidence, ResolutionTechnique};

/// Confidence in an identifier derived by the milestone-1366 closure
/// ecosystem identity pass.
///
/// Measured context for the number: on the pinned closure, confirmation
/// accepts 75 of 75 candidates and declines none, so it establishes existence
/// rather than identity. 0.8 says "concluded, on evidence that bounds this
/// short of proof"; 1.0 would say something the measurement does not support.
const DERIVED_IDENTITY_CONFIDENCE: f64 = 0.8;

/// A derived PURL and the evidence that supported deriving it (C207).
#[derive(Debug, Clone, Copy)]
pub struct DerivedIdentity<'a> {
    /// The PURL the pass concluded, e.g. `pkg:pypi/urllib3@2.6.3`.
    pub concluded_value: &'a str,
    /// The wire form of the candidate evidence.
    pub evidence: &'a str,
}

/// Confidence value used for the additional `methods[]` entries
/// emitted on behalf of losing readers (milestone 105 hybrid
/// emission). The winning reader's method uses `evidence.confidence`
/// (typically 0.95 for manifest-mode); losers are slightly less
/// confident since the dedup precedence ranked them below.
const LOSING_READER_CONFIDENCE: f64 = 0.85;

/// Map a `ResolutionEvidence` (plus optional per-file occurrences) to a
/// CycloneDX 1.6 `evidence` object.
///
/// Technique mapping (right-hand side uses CycloneDX 1.6 technique
/// identifiers):
/// - UrlPattern        -> "instrumentation"
/// - HashMatch         -> "hash-comparison"
/// - PackageDatabase   -> "manifest-analysis"  (reads the package db manifest)
/// - FilePathPattern   -> "filename"
/// - HostnameHeuristic -> "other"
///
/// Per-file occurrences (deep-hashed dpkg components) are emitted under
/// `evidence.occurrences[]`. Each occurrence records its location, the
/// SHA-256 we computed at scan time, and — when dpkg's `.md5sums`
/// recorded one — the MD5 it shipped with, packed into
/// `additionalContext` for cross-reference.
///
/// CDX 1.6 notes:
/// - `evidence.identity` is emitted as an ARRAY of identity objects
///   (bom-1.6.schema.json:2091-2107). The single-object form from 1.5
///   is deprecated.
/// - `evidence.identity[].tools` used to carry source connection IDs
///   and deps.dev markers, but CDX 1.6 requires those entries to be
///   bom-refs of items declared elsewhere in the BOM. Neither waybill
///   payload fits that — both are now emitted as component properties
///   via [`evidence_to_properties`] instead.
///
/// ## Milestone 105 — hybrid emission for cross-reader dedup (FR-015)
///
/// `winning_source_mechanism` and `losing_source_mechanisms` carry
/// the dedup-pipeline output for C/C++ readers. The hybrid emission
/// per research R1 puts the `waybill:also-detected-via` signal in
/// `evidence.identity[0].methods[*].waybill-source-mechanism`
/// natively in CDX (SPDX 2.3/3 use a parallel annotation per C56).
///
/// - `winning_source_mechanism = Some(value)`: the FIRST method entry
///   carries `waybill-source-mechanism = value`.
/// - `losing_source_mechanisms` non-empty: additional method entries
///   follow the winner — one per losing reader, each with
///   `technique = "manifest-analysis"`, `confidence = 0.85`,
///   `waybill-source-mechanism = <loser>`.
/// - Both `None` / empty: byte-identical to pre-milestone-105 output.
///
/// **Important (T023)**: the C56 parity catalog row's CDX side reads
/// the loser set EXCLUSIVELY from this `evidence.identity[].methods[]`
/// native field. Callers MUST NOT also emit a `waybill:also-detected-via`
/// component property — that would duplicate the signal on the CDX
/// path and break SymmetricEqual byte-identity against the SPDX
/// annotation-based emission. The SPDX 2.3 / SPDX 3.0.1 emission
/// paths emit the `waybill:also-detected-via` annotation as their
/// sole home (no native equivalent on the SPDX side).
pub fn build_evidence(
    evidence: &ResolutionEvidence,
    occurrences: &[FileOccurrence],
    winning_source_mechanism: Option<&str>,
    losing_source_mechanisms: &[&str],
    derived_identity: Option<DerivedIdentity<'_>>,
) -> serde_json::Value {
    let technique = match evidence.technique {
        ResolutionTechnique::UrlPattern => "instrumentation",
        ResolutionTechnique::HashMatch => "hash-comparison",
        ResolutionTechnique::PackageDatabase => "manifest-analysis",
        ResolutionTechnique::LocalCacheHit => "filename",
        ResolutionTechnique::FilePathPattern => "filename",
        ResolutionTechnique::HostnameHeuristic => "other",
    };

    // Build the winning method entry. Adds `waybill-source-mechanism`
    // sub-field iff the caller supplied a winner (C/C++ reader path).
    let winning_method = if let Some(value) = winning_source_mechanism {
        json!({
            "technique": technique,
            "confidence": evidence.confidence,
            "waybill-source-mechanism": value,
        })
    } else {
        json!({
            "technique": technique,
            "confidence": evidence.confidence,
        })
    };

    // Append loser method entries (one per losing reader).
    let mut methods: Vec<serde_json::Value> = Vec::with_capacity(1 + losing_source_mechanisms.len());
    methods.push(winning_method);
    for loser in losing_source_mechanisms {
        methods.push(json!({
            "technique": "manifest-analysis",
            "confidence": LOSING_READER_CONFIDENCE,
            "waybill-source-mechanism": loser,
        }));
    }

    // Milestone 1366 (#1365) — C207, natively.
    //
    // `component.evidence.identity[]` is CycloneDX 1.6's own construct for
    // "an identifier was CONCLUDED, by this technique, with this confidence",
    // which is precisely what the nix closure's ecosystem identity pass does.
    // Constitution Principle V requires the native field where one exists, so
    // this is the signal's only home on the CDX path and the generic
    // `extra_annotations` property is suppressed in `builder.rs`.
    //
    // Note this reaches the OPPOSITE conclusion to C201's audit, which
    // rejected the same construct — correctly, because C201 records why an
    // identity was *rejected* and `identity[]` has no slot for that. Here a
    // value was concluded.
    let identity_obj = match derived_identity {
        None => json!({
            "field": "purl",
            "confidence": evidence.confidence,
            "methods": methods,
        }),
        Some(d) => {
            methods.push(json!({
                "technique": "manifest-analysis",
                "confidence": DERIVED_IDENTITY_CONFIDENCE,
                "waybill-closure-ecosystem-evidence": d.evidence,
            }));
            json!({
                "field": "purl",
                // Deliberate and documented, not tuned. It states what the
                // check establishes and no more: a published package exists
                // at this name and version. That is necessary and NOT
                // sufficient to prove this derivation is that package, and
                // 1.0 would assert the sufficiency the spec explicitly
                // declines to claim.
                "confidence": DERIVED_IDENTITY_CONFIDENCE,
                "concludedValue": d.concluded_value,
                "methods": methods,
            })
        }
    };

    let mut out = json!({
        "identity": [identity_obj]
    });

    if !occurrences.is_empty() {
        let occ_entries: Vec<serde_json::Value> = occurrences
            .iter()
            .map(|o| {
                let mut ctx = serde_json::Map::new();
                ctx.insert("sha256".to_string(), json!(o.sha256));
                if let Some(ref md5) = o.md5_legacy {
                    ctx.insert("md5".to_string(), json!(md5));
                }
                // Milestone 040 US2: apk-provided per-file SHA-1
                // cross-ref (`Z:` line in the package's stanza).
                // Surfaced for deb-/apk-/rpm-uniform consumers; deb
                // and rpm occurrences leave this field None.
                if let Some(ref sha1) = o.apk_sha1 {
                    ctx.insert("sha1".to_string(), json!(sha1));
                }
                // Milestone 041: rpm-provided per-file digest
                // from the package's FILEDIGESTS tag,
                // algorithm-prefixed (e.g. "sha256:abc...",
                // "md5:def..."). Carried alongside waybill's own
                // `sha256` so consumers can correlate observed
                // bytes against the upstream-claimed digest.
                if let Some(ref rpm_fd) = o.rpm_file_digest {
                    ctx.insert("rpm_filedigest".to_string(), json!(rpm_fd));
                }
                json!({
                    "location": crate::scan_fs::sbom_path::normalize_sbom_path_str(&o.location),
                    "additionalContext": serde_json::to_string(&ctx)
                        .unwrap_or_default(),
                })
            })
            .collect();
        out["occurrences"] = json!(occ_entries);
    }

    out
}

/// Milestone 1377 (FR-008) — the `evidence.identity[]` entry recording where
/// a closure component's CPE came from, per `contracts/wire.md` §3.
///
/// CycloneDX 1.6's own construct for "this identifier was concluded, by this
/// technique, with this confidence" (Principle V). The two confidences are
/// ordinal, declared over curated, and deliberate rather than calibrated, as
/// with milestone 1366's 0.8.
pub fn cpe_identity(
    cpe: &str,
    source: crate::scan_fs::package_db::nix::closure::identifiers::CpeSource,
) -> serde_json::Value {
    use crate::scan_fs::package_db::nix::closure::identifiers::CpeSource;
    let (technique, value, confidence) = match source {
        CpeSource::NixpkgsDeclared => ("manifest-analysis", "nixpkgs meta.identifiers.cpe", 0.9),
        CpeSource::Curated => ("other", "waybill curated CPE table", 0.7),
    };
    json!({
        "field": "cpe",
        "concludedValue": cpe,
        "confidence": confidence,
        "methods": [ { "technique": technique, "confidence": confidence, "value": value } ],
    })
}

/// Append [`cpe_identity`] to an `evidence` object's `identity[]`, creating
/// either if absent. A no-op for a component with no recorded CPE source.
pub fn append_cpe_identity(
    evidence: &mut serde_json::Value,
    component: &waybill_common::resolution::ResolvedComponent,
) {
    let Some(source) = crate::scan_fs::package_db::nix::closure::identifiers::cpe_source(component)
    else {
        return;
    };
    let Some(cpe) = component.cpes.first() else { return };
    if !evidence.is_object() {
        *evidence = json!({});
    }
    let entry = cpe_identity(cpe, source);
    match evidence.get_mut("identity").and_then(|v| v.as_array_mut()) {
        Some(arr) => arr.push(entry),
        None => evidence["identity"] = json!([entry]),
    }
}

/// Serialize `source_connection_ids` and `deps_dev_match` as CDX
/// component properties. These used to live under
/// `evidence.identity.tools` — per CDX 1.6 those entries must be
/// bom-refs to items declared elsewhere in the BOM, but connection IDs
/// (TLS session tokens from the build trace) and deps.dev markers are
/// neither. Properties are the idiomatic home for scanner-specific
/// provenance data.
pub fn evidence_to_properties(
    evidence: &ResolutionEvidence,
) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    if !evidence.source_connection_ids.is_empty() {
        out.push(json!({
            "name": "waybill:source-connection-ids",
            "value": evidence.source_connection_ids.join(","),
        }));
    }
    if let Some(ref m) = evidence.deps_dev_match {
        out.push(json!({
            "name": "waybill:deps-dev-match",
            "value": format!("{}:{}@{}", m.system, m.name, m.version),
        }));
    }
    out
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use waybill_common::resolution::DepsDevMatch;

    fn make_evidence(technique: ResolutionTechnique, confidence: f64) -> ResolutionEvidence {
        ResolutionEvidence {
            technique,
            confidence,
            source_connection_ids: vec![],
            source_file_paths: vec![],
            deps_dev_match: None,
        }
    }

    /// Milestone 1377 T023 — the declared and curated entries, per
    /// `contracts/wire.md` §3.
    #[test]
    fn m1377_cpe_identity_for_a_declared_and_a_curated_cpe() {
        use crate::scan_fs::package_db::nix::closure::identifiers::CpeSource;
        let bash = "cpe:2.3:a:gnu:bash:5.3:9:*:*:*:*:*:*";
        assert_eq!(
            cpe_identity(bash, CpeSource::NixpkgsDeclared),
            json!({
                "field": "cpe", "concludedValue": bash, "confidence": 0.9,
                "methods": [{ "technique": "manifest-analysis", "confidence": 0.9,
                              "value": "nixpkgs meta.identifiers.cpe" }]
            })
        );
        let zlib = "cpe:2.3:a:zlib:zlib:1.3.2:*:*:*:*:*:*:*";
        assert_eq!(
            cpe_identity(zlib, CpeSource::Curated),
            json!({
                "field": "cpe", "concludedValue": zlib, "confidence": 0.7,
                "methods": [{ "technique": "other", "confidence": 0.7,
                              "value": "waybill curated CPE table" }]
            })
        );
    }

    /// Milestone 1377 T023 — appended after the PURL entry; nothing added to
    /// a component with no recorded source (every non-closure component).
    #[test]
    fn m1377_append_cpe_identity_only_when_a_source_is_recorded() {
        use crate::scan_fs::package_db::nix::closure::identifiers as ids;
        let base = || build_evidence(&make_evidence(ResolutionTechnique::UrlPattern, 0.95), &[], None, &[], None);
        let mut c = crate::generate::graph_completeness::test_support::mk_component("pkg:generic/zlib@1.3.2");
        c.cpes = vec!["cpe:2.3:a:zlib:zlib:1.3.2:*:*:*:*:*:*:*".to_string()];

        let mut untouched = base();
        append_cpe_identity(&mut untouched, &c);
        assert_eq!(untouched, base(), "no recorded source, no entry");

        c.extra_annotations.insert(ids::ANN_CURATED_CPE.to_string(), json!(true));
        let mut ev = base();
        append_cpe_identity(&mut ev, &c);
        let identity = ev["identity"].as_array().unwrap();
        assert_eq!(identity.len(), 2);
        assert_eq!(identity[0]["field"], "purl");
        assert_eq!(identity[1]["field"], "cpe");
        assert_eq!(identity[1]["methods"][0]["technique"], "other");
    }

    #[test]
    fn url_pattern_maps_to_instrumentation() {
        let ev = make_evidence(ResolutionTechnique::UrlPattern, 0.95);
        let result = build_evidence(&ev, &[], None, &[], None);
        assert_eq!(
            result["identity"][0]["methods"][0]["technique"],
            "instrumentation"
        );
    }

    #[test]
    fn hash_match_maps_to_hash_comparison() {
        let ev = make_evidence(ResolutionTechnique::HashMatch, 0.99);
        let result = build_evidence(&ev, &[], None, &[], None);
        assert_eq!(
            result["identity"][0]["methods"][0]["technique"],
            "hash-comparison"
        );
    }

    #[test]
    fn file_path_maps_to_filename() {
        let ev = make_evidence(ResolutionTechnique::FilePathPattern, 0.7);
        let result = build_evidence(&ev, &[], None, &[], None);
        assert_eq!(
            result["identity"][0]["methods"][0]["technique"],
            "filename"
        );
    }

    #[test]
    fn hostname_maps_to_other() {
        let ev = make_evidence(ResolutionTechnique::HostnameHeuristic, 0.5);
        let result = build_evidence(&ev, &[], None, &[], None);
        assert_eq!(result["identity"][0]["methods"][0]["technique"], "other");
    }

    #[test]
    fn package_database_maps_to_manifest_analysis() {
        let ev = make_evidence(ResolutionTechnique::PackageDatabase, 0.85);
        let result = build_evidence(&ev, &[], None, &[], None);
        assert_eq!(
            result["identity"][0]["methods"][0]["technique"],
            "manifest-analysis"
        );
    }

    #[test]
    fn confidence_is_preserved() {
        let ev = make_evidence(ResolutionTechnique::UrlPattern, 0.87);
        let result = build_evidence(&ev, &[], None, &[], None);
        assert_eq!(result["identity"][0]["confidence"], 0.87);
        assert_eq!(result["identity"][0]["methods"][0]["confidence"], 0.87);
    }

    #[test]
    fn identity_is_emitted_as_array_not_object() {
        // CDX 1.6 requires evidence.identity to be an array.
        let ev = make_evidence(ResolutionTechnique::UrlPattern, 0.9);
        let result = build_evidence(&ev, &[], None, &[], None);
        assert!(
            result["identity"].is_array(),
            "evidence.identity must be an array per CDX 1.6"
        );
        assert_eq!(result["identity"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn tools_field_is_never_emitted() {
        // Regression guard for the sbomqs parse failure: the CDX Go
        // library rejects our old `{"ref": "..."}` object shape.
        // Ensure the field simply isn't present, regardless of what's
        // on the evidence.
        let ev_with_everything = ResolutionEvidence {
            technique: ResolutionTechnique::UrlPattern,
            confidence: 0.9,
            source_connection_ids: vec!["conn-1".to_string(), "conn-2".to_string()],
            source_file_paths: vec![],
            deps_dev_match: Some(DepsDevMatch {
                system: "npm".to_string(),
                name: "express".to_string(),
                version: "4.19.2".to_string(),
            }),
        };
        let result = build_evidence(&ev_with_everything, &[], None, &[], None);
        assert!(
            result["identity"][0].get("tools").is_none(),
            "evidence.identity[].tools must not be emitted: got {:?}",
            result["identity"][0].get("tools")
        );
    }

    #[test]
    fn evidence_to_properties_emits_connection_ids() {
        let ev = ResolutionEvidence {
            technique: ResolutionTechnique::UrlPattern,
            confidence: 0.9,
            source_connection_ids: vec!["conn-1".to_string(), "conn-2".to_string()],
            source_file_paths: vec![],
            deps_dev_match: None,
        };
        let props = evidence_to_properties(&ev);
        assert_eq!(props.len(), 1);
        assert_eq!(props[0]["name"], "waybill:source-connection-ids");
        assert_eq!(props[0]["value"], "conn-1,conn-2");
    }

    #[test]
    fn evidence_to_properties_emits_deps_dev_match() {
        let ev = ResolutionEvidence {
            technique: ResolutionTechnique::HashMatch,
            confidence: 0.9,
            source_connection_ids: vec![],
            source_file_paths: vec![],
            deps_dev_match: Some(DepsDevMatch {
                system: "npm".to_string(),
                name: "express".to_string(),
                version: "4.19.2".to_string(),
            }),
        };
        let props = evidence_to_properties(&ev);
        assert_eq!(props.len(), 1);
        assert_eq!(props[0]["name"], "waybill:deps-dev-match");
        assert_eq!(props[0]["value"], "npm:express@4.19.2");
    }

    #[test]
    fn evidence_to_properties_returns_empty_when_no_provenance() {
        let ev = make_evidence(ResolutionTechnique::FilePathPattern, 0.7);
        let props = evidence_to_properties(&ev);
        assert!(props.is_empty());
    }

    #[test]
    fn evidence_to_properties_emits_both_when_both_present() {
        let ev = ResolutionEvidence {
            technique: ResolutionTechnique::UrlPattern,
            confidence: 0.95,
            source_connection_ids: vec!["conn-7".to_string()],
            source_file_paths: vec![],
            deps_dev_match: Some(DepsDevMatch {
                system: "maven".to_string(),
                name: "com.google.guava:guava".to_string(),
                version: "32.1.3-jre".to_string(),
            }),
        };
        let props = evidence_to_properties(&ev);
        assert_eq!(props.len(), 2);
        assert_eq!(props[0]["name"], "waybill:source-connection-ids");
        assert_eq!(props[1]["name"], "waybill:deps-dev-match");
    }

    #[test]
    fn occurrences_are_emitted_when_present() {
        let ev = make_evidence(ResolutionTechnique::PackageDatabase, 0.85);
        let occs = vec![
            FileOccurrence {
                location: "/usr/bin/jq".to_string(),
                sha256: "a".repeat(64),
                md5_legacy: Some("b".repeat(32)),
                apk_sha1: None,
                rpm_file_digest: None,
            },
            FileOccurrence {
                location: "/usr/share/doc/jq/copyright".to_string(),
                sha256: "c".repeat(64),
                md5_legacy: None,
                apk_sha1: None,
                rpm_file_digest: None,
            },
        ];
        let result = build_evidence(&ev, &occs, None, &[], None);
        let out_occs = result["occurrences"]
            .as_array()
            .expect("occurrences array");
        assert_eq!(out_occs.len(), 2);
        assert_eq!(out_occs[0]["location"], "/usr/bin/jq");
        let ctx0: serde_json::Value =
            serde_json::from_str(out_occs[0]["additionalContext"].as_str().unwrap())
                .expect("ctx parses");
        assert_eq!(ctx0["sha256"], "a".repeat(64));
        assert_eq!(ctx0["md5"], "b".repeat(32));

        let ctx1: serde_json::Value =
            serde_json::from_str(out_occs[1]["additionalContext"].as_str().unwrap())
                .expect("ctx parses");
        assert!(ctx1.get("md5").is_none());
    }

    #[test]
    fn occurrences_omitted_when_empty() {
        let ev = make_evidence(ResolutionTechnique::PackageDatabase, 0.85);
        let result = build_evidence(&ev, &[], None, &[], None);
        assert!(result.get("occurrences").is_none());
    }

    // ----------------------------------------------------------------
    // Milestone 105 phase 2E — hybrid emission for cross-reader dedup
    // (FR-015). The winning + losing source-mechanisms ride
    // `evidence.identity[0].methods[*].waybill-source-mechanism`
    // natively on the CDX side; the C56 parity extractor reads from
    // there exclusively.
    // ----------------------------------------------------------------

    #[test]
    fn winning_source_mechanism_attaches_to_first_method() {
        let ev = make_evidence(ResolutionTechnique::PackageDatabase, 0.95);
        let result = build_evidence(&ev, &[], Some("conan-recipe"), &[], None);
        let method = &result["identity"][0]["methods"][0];
        assert_eq!(method["technique"], "manifest-analysis");
        assert_eq!(method["confidence"], 0.95);
        assert_eq!(method["waybill-source-mechanism"], "conan-recipe");
    }

    #[test]
    fn no_winning_source_mechanism_omits_subfield() {
        // Byte-identity guard for the pre-milestone-105 path: when no
        // winner is supplied, the first method entry MUST NOT carry
        // a `waybill-source-mechanism` sub-field.
        let ev = make_evidence(ResolutionTechnique::PackageDatabase, 0.95);
        let result = build_evidence(&ev, &[], None, &[], None);
        let method = &result["identity"][0]["methods"][0];
        assert!(
            method.get("waybill-source-mechanism").is_none(),
            "no winner supplied → no waybill-source-mechanism on first method; got {method:?}"
        );
    }

    #[test]
    fn losing_source_mechanisms_append_method_entries() {
        // gRPC-like scenario: ConanRecipe wins; GitSubmodule and
        // CmakeVendored are detected-via losers.
        let ev = make_evidence(ResolutionTechnique::PackageDatabase, 0.95);
        let losers: [&str; 2] = ["git-submodule", "cmake-vendored"];
        let result = build_evidence(&ev, &[], Some("conan-recipe"), &losers, None);
        let methods = result["identity"][0]["methods"].as_array().unwrap();
        assert_eq!(methods.len(), 3, "1 winner + 2 losers = 3 method entries");
        // Winner first
        assert_eq!(methods[0]["waybill-source-mechanism"], "conan-recipe");
        assert_eq!(methods[0]["confidence"], 0.95);
        // Losers follow, each at confidence 0.85
        assert_eq!(methods[1]["waybill-source-mechanism"], "git-submodule");
        assert_eq!(methods[1]["technique"], "manifest-analysis");
        assert_eq!(methods[1]["confidence"], 0.85);
        assert_eq!(methods[2]["waybill-source-mechanism"], "cmake-vendored");
        assert_eq!(methods[2]["confidence"], 0.85);
    }

    #[test]
    fn loser_only_input_is_unusual_but_handled() {
        // Edge: empty winner + non-empty losers. Unusual (the dedup
        // pipeline doesn't produce this shape), but the function MUST
        // not panic. The first method is the C/C++-reader-neutral
        // form (no waybill-source-mechanism); losers follow normally.
        let ev = make_evidence(ResolutionTechnique::PackageDatabase, 0.95);
        let result = build_evidence(&ev, &[], None, &["git-submodule"], None);
        let methods = result["identity"][0]["methods"].as_array().unwrap();
        assert_eq!(methods.len(), 2);
        assert!(methods[0].get("waybill-source-mechanism").is_none());
        assert_eq!(methods[1]["waybill-source-mechanism"], "git-submodule");
    }

    // Note: cross-module integration between this emission shape and
    // the C56 parity extractor (`parity::extractors::cdx::c56_cdx`)
    // is covered by the holistic parity test suite under
    // `waybill-cli/tests/holistic_parity.rs`. The extractor is
    // `pub(super)` and not directly callable from this module — the
    // round-trip check happens at the integration test layer.
}
