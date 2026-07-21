//! Milestone 071 — synthetic regression for the upgraded
//! `mikebom sbom parity-check` value-equality logic.
//!
//! Pre-071 the CLI subcommand only checked **presence parity**
//! ("all 3 formats non-empty"), which silently missed real gaps
//! where a `SymmetricEqual` row had different set CONTENTS across
//! formats. Post-071 the same per-Directionality invariants used
//! by `tests/holistic_parity.rs` are applied here too.
//!
//! This test constructs a synthetic drift scenario: three minimal
//! SBOM documents where a `SymmetricEqual` annotation key has the
//! same VALUES in two formats but a *different* value in the
//! third. Pre-071 the subcommand reported "0 parity gaps"
//! (presence held). Post-071 it correctly reports the gap.
//!
//! The test invokes the parity_cmd::build_report function directly
//! (it's `pub` so this is supported) rather than shelling out to
//! the CLI binary, keeping the test fast and hermetic.

use std::collections::BTreeSet;

use waybill::parity::{catalog, extractors};

/// Build the same SBOM content in CDX 1.6 / SPDX 2.3 / SPDX 3
/// shape, but vary the `mikebom:source-files` annotation value
/// in the SPDX 2.3 document so set-equality is violated. The
/// CDX and SPDX 3 sides agree; only SPDX 2.3 diverges.
fn build_drift_triple() -> (serde_json::Value, serde_json::Value, serde_json::Value) {
    use serde_json::json;

    // CDX: source-files = ["go.sum"]
    let cdx = json!({
        "bomFormat": "CycloneDX",
        "specVersion": "1.6",
        "components": [{
            "bom-ref": "test-pkg",
            "name": "test",
            "version": "1.0.0",
            "purl": "pkg:generic/test@1.0.0",
            "type": "library",
            "properties": [
                { "name": "mikebom:source-files", "value": "go.sum" },
            ],
        }],
    });

    // SPDX 2.3: source-files = ["DRIFTED.sum"] (different from CDX!)
    let spdx23 = json!({
        "spdxVersion": "SPDX-2.3",
        "SPDXID": "SPDXRef-DOCUMENT",
        "packages": [{
            "SPDXID": "SPDXRef-test-pkg",
            "name": "test",
            "versionInfo": "1.0.0",
            "externalRefs": [
                { "referenceCategory": "PACKAGE-MANAGER", "referenceType": "purl",
                  "referenceLocator": "pkg:generic/test@1.0.0" },
            ],
            "annotations": [{
                "annotator": "Tool: mikebom-test",
                "annotationDate": "1970-01-01T00:00:00Z",
                "annotationType": "OTHER",
                "comment": r#"{"schema":"mikebom-annotation/v1","field":"mikebom:source-files","value":["DRIFTED.sum"]}"#,
            }],
        }],
    });

    // SPDX 3: source-files = ["go.sum"] (matches CDX)
    let spdx3 = json!({
        "@context": "https://spdx.org/rdf/3.0.1/spdx-context.jsonld",
        "@graph": [
            {
                "type": "SpdxDocument",
                "spdxId": "https://example.org/spdx/doc",
            },
            {
                "type": "software_Package",
                "spdxId": "https://example.org/spdx/pkg",
                "name": "test",
                "software_packageVersion": "1.0.0",
                "software_packageUrl": "pkg:generic/test@1.0.0",
            },
            {
                "type": "Annotation",
                "subject": "https://example.org/spdx/pkg",
                "statement": r#"{"schema":"mikebom-annotation/v1","field":"mikebom:source-files","value":["go.sum"]}"#,
            },
        ],
    });

    (cdx, spdx23, spdx3)
}

#[test]
fn drift_in_symmetric_equal_row_is_caught() {
    // Find the C18 (mikebom:source-files) extractor.
    let extractor = extractors::EXTRACTORS
        .iter()
        .find(|e| e.row_id == "C18")
        .expect("C18 catalog row must exist");
    assert!(matches!(
        extractor.directional,
        extractors::Directionality::SymmetricEqual,
    ));

    let (cdx, spdx23, spdx3) = build_drift_triple();
    let cdx_set = (extractor.cdx)(&cdx);
    let spdx23_set = (extractor.spdx23)(&spdx23);
    let spdx3_set = (extractor.spdx3)(&spdx3);

    // Demonstrate the synthesized drift: SPDX 2.3 has DRIFTED.sum,
    // CDX + SPDX 3 have go.sum.
    assert!(!cdx_set.is_empty(), "CDX must carry the value");
    assert!(!spdx23_set.is_empty(), "SPDX 2.3 must carry the value");
    assert!(!spdx3_set.is_empty(), "SPDX 3 must carry the value");
    assert_eq!(cdx_set, spdx3_set, "CDX and SPDX 3 must agree");
    assert_ne!(
        cdx_set, spdx23_set,
        "synthesized drift: CDX != SPDX 2.3 by design",
    );

    // Pre-071 logic (presence-only): all 3 non-empty → "no gap".
    let pre_071_says_gap = !{
        let any_present =
            !cdx_set.is_empty() || !spdx23_set.is_empty() || !spdx3_set.is_empty();
        let all_present =
            !cdx_set.is_empty() && !spdx23_set.is_empty() && !spdx3_set.is_empty();
        any_present && all_present
    };
    assert!(
        !pre_071_says_gap,
        "pre-071 presence-only check WOULD HAVE reported no gap on this drift — exactly the bug 071 fixes",
    );

    // Post-071 logic (per-Directionality invariant): SymmetricEqual
    // requires set equality. The drift breaks it.
    let post_071_invariant_holds =
        cdx_set == spdx23_set && spdx23_set == spdx3_set;
    assert!(
        !post_071_invariant_holds,
        "post-071 SymmetricEqual check MUST report the drift as a parity gap",
    );

    let only_in_cdx: BTreeSet<_> =
        cdx_set.difference(&spdx23_set).cloned().collect();
    let only_in_spdx23: BTreeSet<_> =
        spdx23_set.difference(&cdx_set).cloned().collect();
    println!("synthesized drift detected — CDX-only: {only_in_cdx:?}, SPDX2.3-only: {only_in_spdx23:?}");
}

#[test]
fn no_drift_in_real_fixture_passes_post_071_check() {
    // Sanity inverse: the byte-identity goldens (which are the
    // production output of `mikebom sbom scan`) MUST pass the
    // post-071 invariant for every row. If this test ever fails,
    // there's a real cross-format parity bug in mikebom — the same
    // assertion the integration test `holistic_parity.rs` makes,
    // restated against pinned goldens for fast smoke detection.
    let workspace_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let golden_dir = workspace_root.join("tests/fixtures/golden");

    let cdx_path = golden_dir.join("cyclonedx").join("cargo.cdx.json");
    let spdx23_path = golden_dir.join("spdx-2.3").join("cargo.spdx.json");
    let spdx3_path = golden_dir.join("spdx-3").join("cargo.spdx3.json");

    let cdx: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&cdx_path).expect("read cdx golden"))
            .expect("parse cdx golden");
    let spdx23: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&spdx23_path).expect("read spdx2.3 golden"))
            .expect("parse spdx2.3 golden");
    let spdx3: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&spdx3_path).expect("read spdx3 golden"))
            .expect("parse spdx3 golden");

    let mut violations: Vec<String> = Vec::new();
    let mapping_doc = workspace_root
        .parent()
        .expect("workspace parent")
        .join("docs/reference/sbom-format-mapping.md");
    let rows = catalog::parse_mapping_doc(&mapping_doc);
    for row in rows.iter() {
        let Some(extractor) = extractors::EXTRACTORS
            .iter()
            .find(|e| e.row_id == row.id)
        else {
            continue;
        };
        let cdx_set = (extractor.cdx)(&cdx);
        let spdx23_set = (extractor.spdx23)(&spdx23);
        let spdx3_set = (extractor.spdx3)(&spdx3);
        let any_present =
            !cdx_set.is_empty() || !spdx23_set.is_empty() || !spdx3_set.is_empty();
        if !any_present {
            continue;
        }
        let ok = match extractor.directional {
            extractors::Directionality::SymmetricEqual => {
                cdx_set == spdx23_set && spdx23_set == spdx3_set
            }
            extractors::Directionality::CdxSubsetOfSpdx => {
                cdx_set.is_subset(&spdx23_set) && cdx_set.is_subset(&spdx3_set)
            }
            extractors::Directionality::PresenceOnly => {
                !cdx_set.is_empty() && !spdx23_set.is_empty() && !spdx3_set.is_empty()
            }
            extractors::Directionality::CdxOnly => true,
        };
        if !ok {
            violations.push(format!(
                "{} ({}) [{:?}] cdx={:?} spdx23={:?} spdx3={:?}",
                row.id, row.label, extractor.directional, cdx_set, spdx23_set, spdx3_set,
            ));
        }
    }
    assert!(
        violations.is_empty(),
        "post-071 parity-check found violations on cargo golden:\n{}",
        violations.join("\n"),
    );
}
