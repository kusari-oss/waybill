//! Milestone 166 (implements m166 SC-009) — SPDX 3 duplicate-Annotation-
//! spdxId dedup integration test. Verifies that waybill's emitted SPDX 3
//! document from a real end-to-end scan satisfies the universal
//! uniqueness invariant: no two `@graph[]` elements share the same
//! `spdxId`.
//!
//! Approach: invoke the release binary against a synthesized tempdir
//! fixture with a `package.json` + minimal npm layout so waybill emits
//! a graph-completeness annotation (milestone 158) + component
//! annotations (milestone 011 US2). Parse the emitted SPDX 3 document
//! and assert (a) uniqueness, (b) that the emitted document parses as
//! valid JSON with a `@graph` array, (c) every retained Annotation has
//! well-formed fields.

use std::collections::HashMap;
use std::process::Command;

fn build_fixture(tmp: &std::path::Path) {
    // Minimal npm project — triggers m011 component annotations + m158
    // graph-completeness annotation at document scope.
    std::fs::write(
        tmp.join("package.json"),
        r#"{
  "name": "test-fixture-166",
  "version": "1.0.0",
  "dependencies": {
    "some-declared-only": "^1.0.0"
  }
}
"#,
    )
    .unwrap();
}

fn scan_fixture(tmp: &std::path::Path) -> serde_json::Value {
    let bin = env!("CARGO_BIN_EXE_waybill");
    let out_path = tempfile::NamedTempFile::new()
        .expect("tempfile")
        .path()
        .to_path_buf();
    let output = Command::new(bin)
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(tmp)
        .arg("--format")
        .arg("spdx-3-json")
        .arg("--output")
        .arg(&out_path)
        .arg("--no-deep-hash")
        .output()
        .expect("waybill should run");
    assert!(
        output.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let raw = std::fs::read_to_string(&out_path).expect("read sbom");
    serde_json::from_str(&raw).expect("valid JSON")
}

#[test]
fn t012_spdx3_no_duplicate_spdx_ids_in_graph() {
    let tmp_holder = tempfile::tempdir().expect("tempdir");
    let tmp = tmp_holder.path();
    build_fixture(tmp);

    let sbom = scan_fixture(tmp);
    let graph = sbom
        .get("@graph")
        .and_then(|g| g.as_array())
        .expect("@graph array");

    // ---------------------------------------------------------------
    // Assertion 1 (SC-004 universal invariant): no duplicate spdxIds.
    // ---------------------------------------------------------------
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for elem in graph {
        if let Some(spdx_id) = elem.get("spdxId").and_then(|v| v.as_str()) {
            *counts.entry(spdx_id).or_insert(0) += 1;
        }
    }
    let dupes: Vec<(&&str, &usize)> =
        counts.iter().filter(|(_, &c)| c > 1).collect();
    assert!(
        dupes.is_empty(),
        "SC-004 violated: {} spdxId(s) appear more than once: {:?}",
        dupes.len(),
        dupes.iter().take(3).collect::<Vec<_>>()
    );

    // ---------------------------------------------------------------
    // Assertion 2: emitted document is well-formed (has @graph + at
    // least one Annotation element).
    // ---------------------------------------------------------------
    let annotation_count = graph
        .iter()
        .filter(|e| e.get("type").and_then(|t| t.as_str()) == Some("Annotation"))
        .count();
    assert!(
        annotation_count > 0,
        "expected at least 1 Annotation element in @graph[] (m011 US2 + m158)"
    );

    // ---------------------------------------------------------------
    // Assertion 3 (FR-003): every retained Annotation has well-formed
    // fields — spdxId, subject, statement, annotationType.
    // ---------------------------------------------------------------
    for elem in graph.iter().filter(|e| {
        e.get("type").and_then(|t| t.as_str()) == Some("Annotation")
    }) {
        assert!(
            elem.get("spdxId").is_some(),
            "Annotation missing spdxId: {elem}"
        );
        assert!(
            elem.get("subject").is_some(),
            "Annotation missing subject: {elem}"
        );
        assert!(
            elem.get("statement").is_some(),
            "Annotation missing statement: {elem}"
        );
        assert!(
            elem.get("annotationType").is_some(),
            "Annotation missing annotationType: {elem}"
        );
    }
}
