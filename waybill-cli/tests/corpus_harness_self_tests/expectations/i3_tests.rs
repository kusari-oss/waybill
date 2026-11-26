#[allow(unused_imports)]
use crate::corpus_harness_195::harness::{AssertionFailure, EmittedSboms, FailureFormat};
#[allow(unused_imports)]
use crate::corpus_harness_195::harness::EmittedPaths;
use crate::corpus_harness_195::expectations::*;

fn golden(target: &str, file: &str) -> serde_json::Value {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/public_corpus")
        .join(target)
        .join(file);
    serde_json::from_str(&std::fs::read_to_string(&p).unwrap_or_else(|e| {
        panic!("read {}: {e}", p.display())
    }))
    .unwrap()
}

fn counts(target: &str) -> (usize, usize, usize) {
    (
        cdx_root_out_edges(&golden(target, "cdx.json")),
        spdx23_root_out_edges(&golden(target, "spdx-2.3.json")),
        spdx3_root_out_edges(&golden(target, "spdx-3.json")),
    )
}

/// A target where all three formats agree. Guards the counters
/// themselves: each must find the root and its edges in a real
/// document, in three different encodings.
#[test]
fn counters_agree_on_rust_ripgrep() {
    assert_eq!(counts("rust-ripgrep"), (10, 10, 10));
}

/// Two more agreeing targets in different ecosystems, so a counter
/// that only works for one encoding can't pass by luck.
#[test]
fn counters_agree_on_maven_and_haskell() {
    assert_eq!(counts("maven-guice"), (16, 16, 16));
    assert_eq!(counts("haskell-aeson"), (6, 6, 6));
}

/// #1022 / #1108: the Pants targets that used to sit on the SPDX 3
/// divergence allowlist now agree. Each count is the root's owning
/// component(s) plus whatever only the root fallback reaches: django's
/// 12 unversioned requirements and a file-tier `get-pants.sh`.
///
/// Spec 1537: neither example has a project of its own, so the root
/// `requirements.txt` belongs to the subject, and the root also depends
/// directly on what that file pins and the resolve locks: django `mypy`
/// (14 -> 15), python `ansicolors` and `pytest` (2 -> 4).
#[test]
fn pants_targets_agree_across_formats() {
    assert_eq!(counts("pants-example-django"), (15, 15, 15));
    assert_eq!(counts("pants-example-python"), (4, 4, 4));
    assert_eq!(counts("pants-example-jvm"), (2, 2, 2));
    assert_eq!(counts("pants-clojure-polyglot"), (4, 4, 4));
}

#[test]
fn i3_fires_when_cdx_and_spdx23_disagree() {
    let sboms = EmittedSboms {
        cdx: serde_json::json!({
            "metadata": {"component": {"bom-ref": "root@1"}},
            "components": [{"bom-ref": "pkg:npm/a@1"}],
            "dependencies": [{"ref": "root@1", "dependsOn": []}]
        }),
        spdx_2_3: serde_json::json!({
            "SPDXID": "SPDXRef-DOCUMENT",
            "documentDescribes": ["SPDXRef-Root"],
            "packages": [{"SPDXID": "SPDXRef-Root"}, {"SPDXID": "SPDXRef-A"}],
            "relationships": [
                {"spdxElementId": "SPDXRef-Root", "relatedSpdxElement": "SPDXRef-A",
                 "relationshipType": "DEPENDS_ON"}
            ]
        }),
        spdx_3: serde_json::json!({"@graph": []}),
        paths: EmittedPaths {
            cdx: std::path::PathBuf::new(),
            spdx_2_3: std::path::PathBuf::new(),
            spdx_3: std::path::PathBuf::new(),
        },
    };
    let err = layer0_document_integrity("synthetic", &sboms)
        .expect_err("I3 did not fire on a 0-vs-1 root-edge disagreement");
    assert_eq!(err.check_name, "l3-root-out-edge-count-agrees-across-formats");
    assert!(err.observed.contains("CycloneDX=0"), "{}", err.observed);
    assert!(err.observed.contains("SPDX 2.3=1"), "{}", err.observed);
}
