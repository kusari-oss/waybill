//! Milestone 669 T012 — contract test per json-schema.md T3.
//!
//! Writes a file with `schema_version: 2`, tries to read it via the
//! v1 reader, asserts fail-close refusal with the documented V1
//! violation error message. Guards contract json-schema.md C-1:
//! forward-compat consumers refuse unknown schema versions rather
//! than misinterpret them.

use xtask::bench::schema::BenchRun;

/// A synthetic v2 file whose non-`schema_version` shape happens to be
/// deserialize-compatible with the current v1 struct. In reality a v2
/// schema might have new fields or renamed fields, but constructing a
/// "same-shape-different-version" payload isolates the version-gate
/// check from any accidental structural rejection.
const SYNTHETIC_V2_JSON: &str = r#"{
    "schema_version": 2,
    "metadata": {
        "waybill_commit_sha": "0000000000000000000000000000000000000000",
        "fixture_sha": "1111111111111111111111111111111111111111",
        "runner_uname": "Linux ci-runner 6.5.0-generic x86_64",
        "noise_class": "reference",
        "started_at": "2026-08-29T00:00:00Z",
        "finished_at": "2026-08-29T00:15:00Z",
        "total_duration_sec": 900
    },
    "results": []
}"#;

#[test]
fn v2_schema_version_deserializes_but_validate_rejects() {
    // Serde-level parse succeeds — `schema_version` is a plain u32.
    // The v1-vs-v2 rejection is the responsibility of validate().
    let run: BenchRun = serde_json::from_str(SYNTHETIC_V2_JSON)
        .expect("v2 file should deserialize cleanly (rejection happens in validate)");
    assert_eq!(run.schema_version, 2);

    // validate() MUST refuse it with a V1 violation.
    let err = run.validate().expect_err(
        "v1 reader MUST refuse to process schema_version=2 files (contract C-1)",
    );
    let msg = err.to_string();
    assert!(
        msg.contains("V1 violation"),
        "Error must name the violation as V1. Got: {msg}",
    );
    // The diagnostic must show BOTH the observed and expected version
    // so operators can immediately see the upgrade direction.
    assert!(
        msg.contains("schema_version=2"),
        "Error must cite the observed version (2). Got: {msg}",
    );
    assert!(
        msg.contains("expected version=1"),
        "Error must cite the expected version (1). Got: {msg}",
    );
}

#[test]
fn v0_schema_version_also_rejected() {
    // No v0 shipped, but "backwards" also fail-closes per C-1's
    // spirit: consumers refuse ANY schema version they don't
    // recognize, not just future ones.
    let mut synthetic = SYNTHETIC_V2_JSON.to_string();
    synthetic = synthetic.replace(r#""schema_version": 2"#, r#""schema_version": 0"#);
    let run: BenchRun = serde_json::from_str(&synthetic).expect("parse");
    let err = run.validate().expect_err("v0 must also be rejected");
    assert!(err.to_string().contains("V1 violation"));
}

#[test]
fn v1_still_passes() {
    // Sanity: verify the gate isn't over-broad by confirming a valid
    // v1 payload continues to validate cleanly.
    let mut synthetic = SYNTHETIC_V2_JSON.to_string();
    synthetic = synthetic.replace(r#""schema_version": 2"#, r#""schema_version": 1"#);
    let run: BenchRun = serde_json::from_str(&synthetic).expect("parse");
    assert!(run.validate().is_ok(), "v1 payload must still pass");
}
