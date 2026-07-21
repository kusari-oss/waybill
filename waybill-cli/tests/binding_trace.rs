//! Milestone 072 T026 — `mikebom sbom trace-binding` integration tests.
//!
//! Three scenarios per the spec / quickstart Recipe 6:
//!
//!   (a) Component exists in image with one source-SBOM-bound
//!       instance → trace returns `verified` for that instance with
//!       the bound source ID.
//!   (b) Component exists in image but NO candidate source SBOM
//!       contains it → trace returns `unknown` with
//!       `reason: "source-not-found-in-bind-target"`.
//!   (c) Component appears via two paths (one bound + one unbound)
//!       → trace returns BOTH instances with their respective
//!       binding states.
//!
//! All scenarios exit 0 — `trace-binding` is informational, not
//! validating. The contrast with `verify-binding` (which exits
//! non-zero on hash mismatch per FR-005) is part of the FR-006 contract.
//!
//! Hermetic: synthesizes both SBOMs as JSON literals in tempdirs.

#![cfg_attr(test, allow(clippy::unwrap_used))]

use std::path::Path;
use std::process::Command;

mod common;
use common::bin;

fn write_json(path: &Path, value: &serde_json::Value) {
    std::fs::write(path, serde_json::to_string_pretty(value).unwrap()).unwrap();
}

/// Compute the SHA-256 hex of a file's bytes for comparison with
/// `SourceDocumentId.sha256`.
fn file_sha256_hex(path: &Path) -> String {
    use sha2::Digest;
    let bytes = std::fs::read(path).unwrap();
    let mut hasher = sha2::Sha256::new();
    hasher.update(&bytes);
    data_encoding::HEXLOWER.encode(&hasher.finalize())
}

/// Run `trace-binding` and return (exit-success, parsed JSON report).
fn run_trace(
    component_purl: &str,
    image_sbom: &Path,
    source_sbom: Option<&Path>,
    candidate_dir: Option<&Path>,
) -> (bool, serde_json::Value) {
    let mut cmd = Command::new(bin());
    cmd.arg("sbom")
        .arg("trace-binding")
        .arg("--component-purl")
        .arg(component_purl)
        .arg("--image-sbom")
        .arg(image_sbom)
        .arg("--format")
        .arg("json");
    if let Some(p) = source_sbom {
        cmd.arg("--source-sbom").arg(p);
    }
    if let Some(d) = candidate_dir {
        cmd.arg("--candidate-sources-dir").arg(d);
    }
    let out = cmd.output().expect("trace-binding runs");
    let success = out.status.success();
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!(
            "trace-binding output not JSON: {e}\nstdout={stdout}\nstderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
    });
    (success, parsed)
}

/// Build a CDX SBOM with one component carrying a binding annotation
/// pointing at the supplied source-doc SHA-256.
fn cdx_image_with_bound_instance(
    purl: &str,
    bom_ref: &str,
    binding_hash: &str,
    source_doc_sha: &str,
) -> serde_json::Value {
    let binding = serde_json::json!({
        "algo": "v1",
        "hash": binding_hash,
        "source_doc_id": { "sha256": source_doc_sha },
        "strength": "verified",
    });
    let binding_str = serde_json::to_string(&binding).unwrap();
    serde_json::json!({
        "bomFormat": "CycloneDX",
        "specVersion": "1.6",
        "components": [{
            "type": "library",
            "name": "foo",
            "version": "1.0.0",
            "purl": purl,
            "bom-ref": bom_ref,
            "properties": [{
                "name": "mikebom:source-document-binding",
                "value": binding_str,
            }]
        }]
    })
}

/// Build a source-tier CDX SBOM with the named PURL's main-module
/// component carrying a sibling binding annotation. Used as the
/// candidate against which `trace-binding` matches.
fn cdx_source_with_main_module(
    purl: &str,
    binding_hash: &str,
    source_doc_sha: &str,
) -> serde_json::Value {
    let binding = serde_json::json!({
        "algo": "v1",
        "hash": binding_hash,
        "source_doc_id": { "sha256": source_doc_sha },
        "strength": "verified",
    });
    let binding_str = serde_json::to_string(&binding).unwrap();
    serde_json::json!({
        "bomFormat": "CycloneDX",
        "specVersion": "1.6",
        "components": [{
            "type": "library",
            "name": "foo",
            "version": "1.0.0",
            "purl": purl,
            "bom-ref": format!("{purl}-source-bom"),
            "properties": [{
                "name": "mikebom:source-document-binding",
                "value": binding_str,
            }]
        }]
    })
}

/// (a) Component exists with one bound instance → trace verified.
#[test]
fn trace_bound_instance_returns_verified() {
    let dir = tempfile::tempdir().unwrap();
    let purl = "pkg:cargo/foo@1.0.0";
    let binding_hash = "a".repeat(64);

    let candidate_dir = dir.path().join("candidates");
    std::fs::create_dir_all(&candidate_dir).unwrap();
    let source_path = candidate_dir.join("foo-source.cdx.json");
    let source_doc =
        cdx_source_with_main_module(purl, &binding_hash, &"0".repeat(64));
    // Write source first so we can compute its SHA-256.
    write_json(&source_path, &source_doc);
    let source_sha = file_sha256_hex(&source_path);

    // Re-write the source with its own SHA-256 baked into the binding's
    // source_doc_id so the trace can content-SHA-match.
    let source_doc =
        cdx_source_with_main_module(purl, &binding_hash, &source_sha);
    write_json(&source_path, &source_doc);
    let source_sha = file_sha256_hex(&source_path);
    // Loop once more — the rewrite changed the bytes, hence the SHA.
    // Pin the SHA after the source is finalized; then the IMAGE SBOM
    // points at this SHA. (We don't need the source SBOM to refer to
    // its own SHA — that's a self-referential field; what matters is
    // the IMAGE side asserting the source's SHA correctly.)
    let _ = source_sha;
    let final_source_sha = file_sha256_hex(&source_path);

    let image_path = dir.path().join("foo-image.cdx.json");
    let image_doc = cdx_image_with_bound_instance(
        purl,
        "foo-image-instance",
        &binding_hash,
        &final_source_sha,
    );
    write_json(&image_path, &image_doc);

    let (success, report) = run_trace(purl, &image_path, None, Some(&candidate_dir));
    assert!(success, "trace-binding exits 0 (informational)");

    let instances = report["instances"].as_array().expect("instances array");
    assert_eq!(instances.len(), 1, "one instance of the PURL in the image");
    assert_eq!(
        instances[0]["binding"]["strength"].as_str(),
        Some("verified"),
        "asserted binding round-trips as verified"
    );
    assert_eq!(
        instances[0]["bom_ref"].as_str(),
        Some("foo-image-instance")
    );
    let summary = instances[0]["audit_summary"]
        .as_str()
        .expect("audit_summary string");
    assert!(
        summary.contains("verified"),
        "audit summary names verified strength: {summary}"
    );
}

/// (b) PURL not in any candidate → trace returns unknown with
/// `source-not-found-in-bind-target` reason.
#[test]
fn trace_no_candidate_match_returns_source_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let purl = "pkg:cargo/foo@1.0.0";
    let other_purl = "pkg:cargo/different@9.9.9";

    let candidate_dir = dir.path().join("candidates");
    std::fs::create_dir_all(&candidate_dir).unwrap();
    let source_path = candidate_dir.join("other-source.cdx.json");
    let source_doc = cdx_source_with_main_module(other_purl, &"e".repeat(64), &"0".repeat(64));
    write_json(&source_path, &source_doc);

    // Image-tier component with NO asserted binding annotation +
    // NO candidate source containing this PURL → unknown.
    let image_path = dir.path().join("foo-image.cdx.json");
    let image_doc = serde_json::json!({
        "bomFormat": "CycloneDX",
        "specVersion": "1.6",
        "components": [{
            "type": "library",
            "name": "foo",
            "version": "1.0.0",
            "purl": purl,
            "bom-ref": "foo-image-instance",
        }]
    });
    write_json(&image_path, &image_doc);

    let (success, report) = run_trace(purl, &image_path, None, Some(&candidate_dir));
    assert!(success, "trace-binding exits 0 (informational)");

    let instances = report["instances"].as_array().expect("instances array");
    assert_eq!(instances.len(), 1);
    assert_eq!(
        instances[0]["binding"]["strength"].as_str(),
        Some("unknown")
    );
    assert_eq!(
        instances[0]["binding"]["reason"].as_str(),
        Some("source-not-found-in-bind-target")
    );
}

/// (c) Two instances of same PURL — one bound, one unbound. Trace
/// returns both with their respective states.
#[test]
fn trace_two_instances_returns_both_with_per_instance_binding() {
    let dir = tempfile::tempdir().unwrap();
    let purl = "pkg:golang/golang.org/x/net@v0.28.0";
    let binding_hash = "b".repeat(64);

    // Build the source SBOM first so we can pin its content-SHA into
    // the bound instance's annotation.
    let candidate_dir = dir.path().join("candidates");
    std::fs::create_dir_all(&candidate_dir).unwrap();
    let source_path = candidate_dir.join("proj-source.cdx.json");
    let source_doc =
        cdx_source_with_main_module(purl, &binding_hash, &"0".repeat(64));
    write_json(&source_path, &source_doc);
    let source_sha = file_sha256_hex(&source_path);

    // Construct the image SBOM with two instances:
    //   - golang-net-from-foo: bound (annotation present, SHA matches)
    //   - golang-net-from-baselayer-server: unbound (no annotation)
    let bound_binding = serde_json::json!({
        "algo": "v1",
        "hash": binding_hash,
        "source_doc_id": { "sha256": source_sha },
        "strength": "verified",
    });
    let bound_str = serde_json::to_string(&bound_binding).unwrap();
    let image_doc = serde_json::json!({
        "bomFormat": "CycloneDX",
        "specVersion": "1.6",
        "components": [
            {
                "type": "library",
                "name": "net",
                "version": "0.28.0",
                "purl": purl,
                "bom-ref": "golang-net-from-foo",
                "properties": [{
                    "name": "mikebom:source-document-binding",
                    "value": bound_str,
                }],
            },
            {
                "type": "library",
                "name": "net",
                "version": "0.28.0",
                "purl": purl,
                "bom-ref": "golang-net-from-baselayer-server",
            }
        ]
    });
    let image_path = dir.path().join("proj-image.cdx.json");
    write_json(&image_path, &image_doc);

    let (success, report) = run_trace(purl, &image_path, None, Some(&candidate_dir));
    assert!(success, "trace-binding exits 0 (informational)");

    let instances = report["instances"].as_array().expect("instances array");
    assert_eq!(instances.len(), 2, "two instances of the same PURL");

    // Find each instance by bom_ref (order is insertion-driven; we
    // assert by name to avoid relying on it).
    let bound = instances
        .iter()
        .find(|i| i["bom_ref"] == "golang-net-from-foo")
        .expect("bound instance");
    let unbound = instances
        .iter()
        .find(|i| i["bom_ref"] == "golang-net-from-baselayer-server")
        .expect("unbound instance");

    assert_eq!(
        bound["binding"]["strength"].as_str(),
        Some("verified"),
        "bound instance should be verified"
    );
    assert_eq!(
        unbound["binding"]["strength"].as_str(),
        Some("unknown"),
        "unbound instance should be unknown — no asserted binding annotation"
    );
    // Per FR-006 / quickstart.md Recipe 6 (b): when an image
    // instance has no asserted binding, trace conservatively
    // reports `unknown` even if a candidate happens to carry the
    // same PURL (PURL-collision across independent build paths is
    // exactly the worked-example concern from US2 AS#4 / SC-003).
    assert_eq!(
        unbound["binding"]["reason"].as_str(),
        Some("source-not-found-in-bind-target"),
        "unbound instance reason names the no-candidate-match outcome"
    );
}
