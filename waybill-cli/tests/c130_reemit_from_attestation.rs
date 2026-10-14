//! #1197 — `waybill sbom generate` re-emits C130 / C131 from the
//! attestation's `compiler_pipeline`.
//!
//! The field was written into `BuildTracePredicate.compiler_pipeline` by
//! `waybill trace` and never read back out: every construction of
//! `ScanArtifacts.compiler_pipeline` in the tree was `None`, and this
//! command — the one a comment at `scan_cmd.rs:5207` named as the path that
//! carried it — mentioned the field zero times. So C130
//! `waybill:source-read-set` and C131 `waybill:read-set-source` were
//! unreachable from every entry point.
//!
//! These tests are the reason to believe the wiring fires, and the two
//! controls are the reason to believe the assertion is not vacuous: a
//! `source-read-set` property can be absent because the feature is off, or
//! because nothing matched, and those must not look alike. Control A is the
//! pre-#1197 shape (no field at all), Control B has the field present with a
//! write set that intersects no component.
//!
//! Package coordinates are synthetic (`waybill-fixture-*`) per the project's
//! fixture rule — a real one trips the advisory scan in review.
//!
//! No eBPF and no Linux: `sbom generate` takes an attestation *file*, so the
//! emission half of the #1197 experiment is host-independent. Capturing a
//! real trace is the only part that needs a kernel.

use std::path::PathBuf;
use std::process::Command;

use serde_json::{json, Value};
use tempfile::tempdir;

/// The one component path the synthetic attestation observes, and so the
/// only path a write set can intersect.
const COMPONENT_PATH: &str =
    "/home/user/.cargo/registry/cache/crates.io-6f17d22bba15001f/waybill-fixture-alpha-1.0.0.crate";

const COMPONENT_PURL: &str = "pkg:cargo/waybill-fixture-alpha@1.0.0";

const HASH_LIB: &str = "1111111111111111111111111111111111111111111111111111111111111111";
const HASH_DE: &str = "2222222222222222222222222222222222222222222222222222222222222222";

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("waybill-cli parent")
        .to_path_buf()
}

/// A minimal build-trace attestation observing exactly one `.crate` file.
///
/// Derived from `tests/fixtures/sample-attestation.json` so the predicate's
/// required shape stays in one place: if a field becomes mandatory, that
/// fixture is what gets updated, and this test follows. Network connections
/// are cleared and the single file operation is rewritten to a synthetic
/// coordinate so the resolved component set is fully under test control.
fn synthetic_attestation() -> Value {
    let raw = std::fs::read_to_string(
        workspace_root().join("tests/fixtures/sample-attestation.json"),
    )
    .expect("read sample attestation fixture");
    let mut v: Value = serde_json::from_str(&raw).expect("parse sample attestation");

    v["predicate"]["network_trace"]["connections"] = json!([]);
    let op = v["predicate"]["file_access"]["operations"][0].clone();
    let mut op = op;
    op["path"] = json!(COMPONENT_PATH);
    v["predicate"]["file_access"]["operations"] = json!([op]);
    v["predicate"].as_object_mut()
        .expect("predicate is an object")
        .remove("compiler_pipeline");
    v
}

/// A pipeline with one invocation reading two sources and writing `write_to`.
fn pipeline_writing(write_to: &str) -> Value {
    json!({
        "invocations": [{
            "invocation_id": 1,
            "compiler": "rustc",
            "pid": 12345,
            "ppid": 12344,
            "cgroup_id": 0,
            "start_timestamp": "2026-04-15T10:00:01.000Z",
            "end_timestamp": "2026-04-15T10:00:02.000Z",
            "argv": ["rustc", "src/lib.rs"],
            "exit_code": 0,
            "read_set": [
                {"path": "/work/src/lib.rs", "kind": "file", "existed_at_close": true,
                 "sha256_at_close": {"algorithm": "sha256", "value": HASH_LIB}},
                {"path": "/work/src/de.rs", "kind": "file", "existed_at_close": true,
                 "sha256_at_close": {"algorithm": "sha256", "value": HASH_DE}}
            ],
            "write_set": [
                {"path": write_to, "existed_at_close": true}
            ],
            "events_dropped": 0
        }],
        "dag_edges": [],
        "completeness": {"state": "complete"},
        "secrets_read_filtered": 0,
        "include_system_reads_flag": false,
        "filter_categories_applied": []
    })
}

/// Run `sbom generate` on `attestation` and return the emitted CDX document.
fn generate(attestation: &Value, label: &str) -> Value {
    let dir = tempdir().expect("tempdir");
    let att_path = dir.path().join(format!("{label}.attestation.json"));
    let out_path = dir.path().join(format!("{label}.cdx.json"));
    std::fs::write(
        &att_path,
        serde_json::to_vec_pretty(attestation).expect("serialize attestation"),
    )
    .expect("write attestation");

    let out = Command::new(env!("CARGO_BIN_EXE_waybill"))
        .args(["sbom", "generate"])
        .arg(&att_path)
        // Keep the test hermetic: no deps.dev round-trip.
        .arg("--skip-purl-validation")
        .arg("--output")
        .arg(&out_path)
        .output()
        .expect("run waybill sbom generate");
    assert!(
        out.status.success(),
        "`sbom generate` failed for {label}:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let emitted = std::fs::read_to_string(&out_path).expect("read emitted CDX");
    serde_json::from_str(&emitted).expect("parse emitted CDX")
}

/// Every `(purl, value)` pair for one property name across all components.
fn property_values(bom: &Value, name: &str) -> Vec<(String, String)> {
    bom["components"]
        .as_array()
        .map(|cs| {
            cs.iter()
                .flat_map(|c| {
                    let purl = c["purl"].as_str().unwrap_or("<no purl>").to_string();
                    c["properties"]
                        .as_array()
                        .map(|ps| {
                            ps.iter()
                                .filter(|p| p["name"].as_str() == Some(name))
                                .map(|p| {
                                    (
                                        purl.clone(),
                                        p["value"].as_str().unwrap_or_default().to_string(),
                                    )
                                })
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default()
                })
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn c130_carries_the_read_set_for_the_component_the_invocation_wrote() {
    let mut att = synthetic_attestation();
    att["predicate"]["compiler_pipeline"] = pipeline_writing(COMPONENT_PATH);
    let bom = generate(&att, "matching");

    let c130 = property_values(&bom, "waybill:source-read-set");
    assert_eq!(
        c130.len(),
        1,
        "exactly one component's path intersects the write set; got {c130:?}"
    );
    assert_eq!(c130[0].0, COMPONENT_PURL);

    let payload: Value =
        serde_json::from_str(&c130[0].1).expect("C130 value is a JSON document");
    assert_eq!(payload["invocation_ids"], json!([1]));

    // Sorted by path per R8, so `de.rs` precedes `lib.rs`. Asserting the
    // order is the cheapest check that the determinism contract holds.
    let paths: Vec<&str> = payload["read_set"]
        .as_array()
        .expect("read_set is an array")
        .iter()
        .map(|e| e["path"].as_str().expect("read_set entry has a path"))
        .collect();
    assert_eq!(paths, vec!["/work/src/de.rs", "/work/src/lib.rs"]);

    // The hashes are what make the claim verifiable rather than decorative.
    let hashes: Vec<&str> = payload["read_set"]
        .as_array()
        .expect("read_set is an array")
        .iter()
        .map(|e| e["sha256_at_close"].as_str().unwrap_or("<missing>"))
        .collect();
    assert_eq!(hashes, vec![HASH_DE, HASH_LIB]);

    let c131 = property_values(&bom, "waybill:read-set-source");
    assert!(
        c131.contains(&(COMPONENT_PURL.to_string(), "traced".to_string())),
        "the written component is `traced`; got {c131:?}"
    );
}

/// Control A — the pre-#1197 shape. Without the field, neither annotation may
/// appear, or every attestation already in existence would re-emit
/// differently.
#[test]
fn neither_annotation_appears_when_the_attestation_has_no_compiler_pipeline() {
    let att = synthetic_attestation();
    assert!(
        att["predicate"].get("compiler_pipeline").is_none(),
        "control A must start with the field genuinely absent"
    );
    let bom = generate(&att, "control-a");

    assert!(
        property_values(&bom, "waybill:source-read-set").is_empty(),
        "C130 must not be emitted without a compiler pipeline"
    );
    assert!(
        property_values(&bom, "waybill:read-set-source").is_empty(),
        "C131 must not be emitted without a compiler pipeline"
    );
}

/// Control B — the field is present and populated, but its write set
/// intersects no component. C130 must be absent and C131 must say `unknown`,
/// which is the state a consumer needs in order to tell "not traced" from
/// "feature off". Without this control, the first test would also pass if the
/// emitter annotated every component unconditionally.
#[test]
fn c130_is_absent_and_c131_is_unknown_when_the_write_set_matches_nothing() {
    let mut att = synthetic_attestation();
    att["predicate"]["compiler_pipeline"] =
        pipeline_writing("/work/target/debug/libunrelated.rlib");
    let bom = generate(&att, "control-b");

    assert!(
        property_values(&bom, "waybill:source-read-set").is_empty(),
        "C130 must be absent when no component path is in a write set"
    );
    let c131 = property_values(&bom, "waybill:read-set-source");
    assert!(!c131.is_empty(), "C131 is emitted for every component once a pipeline exists");
    for (purl, value) in &c131 {
        assert_eq!(value, "unknown", "{purl} should be `unknown`, not `{value}`");
    }
}
