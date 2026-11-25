#[allow(unused_imports)]
use std::path::PathBuf;
#[allow(unused_imports)]
use crate::corpus_harness_195::harness::{AssertionFailure, EmittedSboms, FailureFormat, update_goldens_gate};
#[allow(unused_imports)]
use crate::corpus_harness_195::js_filter;
use crate::corpus_harness_195::layer2_golden::*;
use serde_json::json;

/// The sidecar's hash is masked; a package checksum is not.
#[test]
fn external_document_ref_checksums_are_masked_and_package_checksums_are_not() {
    let doc = json!({
        "externalDocumentRefs": [{
            "externalDocumentId": "DocumentRef-OpenVEX",
            "checksum": {"algorithm": "SHA256", "checksumValue": "dd6f3de3"},
            "spdxDocument": "waybill.openvex.json"
        }],
        "packages": [{
            "SPDXID": "SPDXRef-Package-x",
            "checksums": [{"algorithm": "SHA256", "checksumValue": "abc123"}]
        }]
    });
    let m = mask_nondeterministic(&doc);
    assert_eq!(m["externalDocumentRefs"][0]["checksum"]["checksumValue"], "<masked>");
    assert_eq!(m["externalDocumentRefs"][0]["spdxDocument"], "waybill.openvex.json");
    assert_eq!(m["packages"][0]["checksums"][0]["checksumValue"], "abc123");
}

/// #1067: outside regen mode a missing golden fails, and nothing is
/// written. It used to be written from whatever machine ran the test,
/// and the test passed.
#[test]
fn a_missing_golden_fails_and_is_not_written_outside_regen_mode() {
    if update_goldens_gate() {
        return; // regen mode is the one place a missing golden is written
    }
    let target = "no-such-target-1067";
    let tmp = tempfile::tempdir().unwrap();
    let sboms = EmittedSboms {
        cdx: json!({}),
        spdx_2_3: json!({}),
        spdx_3: json!({}),
        paths: crate::corpus_harness_195::harness::EmittedPaths {
            cdx: tmp.path().join("cdx.json"),
            spdx_2_3: tmp.path().join("spdx23.json"),
            spdx_3: tmp.path().join("spdx3.json"),
        },
    };
    let result = compare_golden(target, FailureFormat::Cdx, &sboms);
    // Clean up before asserting, so a regression cannot leave a stray
    // target directory in the committed fixture tree.
    let dir = fixtures_root().join(target);
    let written = dir.exists();
    std::fs::remove_dir_all(&dir).ok();
    assert!(!written, "no golden may be written outside regen mode");
    let err = result.expect_err("a missing golden must not compare as a pass");
    assert_eq!(err.check_name, "layer2-golden-missing");
}

/// The defect: masking keyed on `spdxId`, so every other IRI-bearing
/// field kept the real document hash and the stored golden referenced
/// identifiers absent from its own document.
#[test]
fn every_iri_bearing_field_is_masked_not_just_spdx_id() {
    let doc = json!({
        "@graph": [
            { "type": "software_Package", "spdxId": "https://w.dev/spdx3/doc-ABC/pkg-1" },
            { "type": "Relationship",
              "spdxId": "https://w.dev/spdx3/doc-ABC/rel-1",
              "from": "https://w.dev/spdx3/doc-ABC/pkg-1",
              "to": ["https://w.dev/spdx3/doc-ABC/pkg-2"] },
            { "type": "Annotation",
              "spdxId": "https://w.dev/spdx3/doc-ABC/anno-1",
              "subject": "https://w.dev/spdx3/doc-ABC/pkg-1" },
            { "type": "SpdxDocument",
              "spdxId": "https://w.dev/spdx3/doc-ABC",
              "rootElement": ["https://w.dev/spdx3/doc-ABC/pkg-1"] }
        ]
    });
    let masked = mask_nondeterministic(&doc);
    let text = serde_json::to_string(&masked).unwrap();
    assert!(
        !text.contains("doc-ABC"),
        "no field may retain the real document id; got {text}",
    );
    assert!(text.contains("doc-<masked>"), "the shape must be preserved");
}

/// #918 — the SPDX 2.3 DocumentRoot id must be masked. It was not, for
/// one commit, because the mask lived inside a guard that only matched
/// SPDX 3 document IRIs. Without this test that is invisible until a
/// release rotates the id and the lane goes red again.
#[test]
fn spdx23_document_root_id_is_masked() {
    let mut v = serde_json::json!({
        "SPDXID": "SPDXRef-DocumentRoot-2Z6INX5CK3NXX4LN",
        "documentDescribes": ["SPDXRef-DocumentRoot-2Z6INX5CK3NXX4LN"],
    });
    walk_mask(&mut v);
    assert_eq!(v["SPDXID"], "SPDXRef-DocumentRoot-<masked>");
    assert_eq!(v["documentDescribes"][0], "SPDXRef-DocumentRoot-<masked>");
}

/// A package SPDXID must NOT be masked — only the DocumentRoot one is
/// content-addressed over the tool version. Masking package ids would
/// leave a gate that cannot see a component identity change.
#[test]
fn package_spdx_ids_are_left_alone() {
    let mut v = serde_json::json!({ "SPDXID": "SPDXRef-Package-ABCDEFGH12345678" });
    walk_mask(&mut v);
    assert_eq!(v["SPDXID"], "SPDXRef-Package-ABCDEFGH12345678");
}

/// The property the goldens were violating: after masking, every
/// relationship endpoint must still resolve to an element of the
/// same document. Without this, no structural check on a stored
/// golden means anything.
#[test]
fn masked_document_stays_internally_coherent() {
    let doc = json!({
        "@graph": [
            { "type": "software_Package", "spdxId": "https://w.dev/spdx3/doc-XYZ/pkg-1" },
            { "type": "software_Package", "spdxId": "https://w.dev/spdx3/doc-XYZ/pkg-2" },
            { "type": "Relationship",
              "spdxId": "https://w.dev/spdx3/doc-XYZ/rel-1",
              "from": "https://w.dev/spdx3/doc-XYZ/pkg-1",
              "to": ["https://w.dev/spdx3/doc-XYZ/pkg-2"] }
        ]
    });
    let masked = mask_nondeterministic(&doc);
    let graph = masked["@graph"].as_array().unwrap();
    let ids: std::collections::BTreeSet<&str> = graph
        .iter()
        .filter_map(|e| e["spdxId"].as_str())
        .collect();
    for e in graph.iter().filter(|e| e["type"] == "Relationship") {
        let from = e["from"].as_str().unwrap();
        assert!(ids.contains(from), "dangling `from` after masking: {from}");
        for t in e["to"].as_array().unwrap() {
            let t = t.as_str().unwrap();
            assert!(ids.contains(t), "dangling `to` after masking: {t}");
        }
    }
}

/// The helper masked only the first occurrence, which silently left
/// later ones intact inside JSON-in-string values.
#[test]
fn every_occurrence_in_one_string_is_masked() {
    let s = "a https://w.dev/spdx3/doc-AAA/pkg-1 b https://w.dev/spdx3/doc-AAA/pkg-2 c";
    let out = mask_doc_prefix(s);
    assert!(!out.contains("doc-AAA"), "got {out}");
    assert_eq!(out.matches("doc-<masked>").count(), 2);
}

/// A bare document IRI with no trailing segment must still mask.
#[test]
fn trailing_document_iri_is_masked() {
    assert_eq!(
        mask_doc_prefix("https://w.dev/spdx3/doc-ZZZ"),
        "https://w.dev/spdx3/doc-<masked>",
    );
}

/// A real filesystem path containing `doc-` must survive intact.
/// The first version of this fix matched a bare `/doc-` and rewrote
/// `/usr/share/doc-base/...` inside the image-postgres16 golden,
/// corrupting real data. The regeneration diff caught it: CDX and
/// SPDX 2.3 moved when a masking-only change should have touched
/// SPDX 3 alone.
#[test]
fn real_paths_containing_doc_are_not_masked() {
    for p in [
        "/usr/share/doc-base/findutils.findutils",
        "/usr/share/doc-base/base-passwd.users-and-groups",
        "/etc/doc-something",
    ] {
        assert_eq!(mask_doc_prefix(p), p, "must not rewrite the real path {p}");
    }
}

/// Strings with no document IRI must pass through untouched — the
/// leaf rule runs on every string in the document.
#[test]
fn unrelated_strings_are_untouched() {
    for s in ["pkg:cargo/serde@1.0.0", "MIT", "", "no slashes here"] {
        assert_eq!(mask_doc_prefix(s), s, "must not alter {s:?}");
    }
}
