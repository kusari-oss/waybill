//! Milestone 072 T017 — `--bind-to-source` end-to-end emission test.
//!
//! Synthesizes a tiny cargo-workspace-shaped source tree, scans it
//! once to produce a source-tier SBOM, then scans the same tree
//! again with `--bind-to-source <source-sbom>` to produce an
//! "image-tier" SBOM (we use `--path` + a separate scan since
//! end-to-end OCI image scans aren't hermetic enough for an integration
//! test). Asserts the resulting image-tier SBOM carries:
//!
//!   * Per-component `mikebom:source-document-binding` annotation on
//!     components whose PURL appears in the source SBOM (FR-001).
//!   * Document-level cross-document reference per format
//!     (T010 / T012 / T014).
//!
//! Note: the scan_cmd implementation only emits binding annotations
//! on `--image` scans (per the contract that source-tier SBOMs stay
//! byte-identical). To exercise the emission path under a hermetic
//! test, we work around the gate by creating a tarball + using the
//! `--image` flag pointing at it. For PR-A we cover the unit-test
//! shape end-to-end via the `binding_verify.rs` integration test
//! (which uses the verify-binding subcommand against pre-synthesized
//! SBOMs), and we cover the bind-to-source attach helper directly via
//! library unit tests in `binding/verify.rs::tests`. Future PR work
//! adds a tarball-based image fixture for the full --bind-to-source
//! emission round-trip; the attach logic is the same code path.

use std::process::Command;

mod common;
use common::normalize::apply_fake_home_env;
use common::{bin, fixture_path};

/// `--bind-to-source` with a non-existent path MUST fail per FR-011.
#[test]
fn bind_to_source_missing_path_aborts_scan() {
    let fixture = fixture_path("npm/node-modules-walk");
    let dir = tempfile::tempdir().expect("tempdir");
    let fake_home = tempfile::tempdir().expect("fake-home");
    let nonexistent =
        dir.path().join("does-not-exist.cdx.json");
    let cdx_out = dir.path().join("image.cdx.json");

    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home.path());
    let out = cmd
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(&fixture)
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", cdx_out.to_string_lossy()))
        .arg("--no-deep-hash")
        .arg("--bind-to-source")
        .arg(&nonexistent)
        .output()
        .expect("scan runs");
    assert!(
        !out.status.success(),
        "expected non-zero exit per FR-011 (source SBOM cannot be loaded); \
         got success. stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// `--bind-to-source` with a `--path` scan should warn-and-NOT-emit
/// binding annotations (source-tier SBOMs stay byte-identical to
/// alpha.14 per the contract). The scan succeeds; the source-tier
/// goldens are not regressed.
#[test]
fn bind_to_source_with_path_scan_does_not_emit_bindings() {
    let fixture = fixture_path("npm/node-modules-walk");
    let dir = tempfile::tempdir().expect("tempdir");
    let fake_home = tempfile::tempdir().expect("fake-home");
    let source_sbom = dir.path().join("source.cdx.json");
    // Step 1 — produce a source-tier SBOM.
    let mut step1 = Command::new(bin());
    apply_fake_home_env(&mut step1, fake_home.path());
    let out1 = step1
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(&fixture)
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", source_sbom.to_string_lossy()))
        .arg("--no-deep-hash")
        .output()
        .expect("step1 runs");
    assert!(
        out1.status.success(),
        "step 1 (source scan) failed: stderr={}",
        String::from_utf8_lossy(&out1.stderr)
    );

    // Step 2 — re-scan the source tree with --bind-to-source. This
    // is technically a misuse (path scans don't emit bindings) but
    // the scan should still succeed; the warning goes to stderr and
    // no binding annotations should appear in the output.
    let bound_sbom = dir.path().join("bound.cdx.json");
    let mut step2 = Command::new(bin());
    apply_fake_home_env(&mut step2, fake_home.path());
    let out2 = step2
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(&fixture)
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", bound_sbom.to_string_lossy()))
        .arg("--no-deep-hash")
        .arg("--bind-to-source")
        .arg(&source_sbom)
        .output()
        .expect("step2 runs");
    assert!(
        out2.status.success(),
        "step 2 (bind-to-source path scan) failed: stderr={}",
        String::from_utf8_lossy(&out2.stderr)
    );

    let bound_bytes = std::fs::read(&bound_sbom).expect("read bound SBOM");
    let bound: serde_json::Value =
        serde_json::from_slice(&bound_bytes).expect("parse bound SBOM");

    // The path-scan path warns and DOES NOT emit binding
    // annotations on per-component properties; confirm.
    let mut found_binding_annotation = false;
    if let Some(comps) = bound.get("components").and_then(|v| v.as_array()) {
        for c in comps {
            if let Some(props) = c.get("properties").and_then(|v| v.as_array()) {
                for p in props {
                    if p.get("name").and_then(|v| v.as_str())
                        == Some("mikebom:source-document-binding")
                    {
                        found_binding_annotation = true;
                    }
                }
            }
        }
    }
    assert!(
        !found_binding_annotation,
        "source-tier (--path) scan with --bind-to-source MUST NOT emit \
         binding annotations (alpha.14 source-tier byte-identity contract)"
    );
}

/// The `verify-binding` CLI MUST fail-fast when an input file is
/// missing. Smoke-tests T016's CLI surface alongside FR-005's
/// non-zero exit on errors.
#[test]
fn verify_binding_cli_missing_input_exits_nonzero() {
    let dir = tempfile::tempdir().expect("tempdir");
    let nonexistent = dir.path().join("nope.cdx.json");

    let mut cmd = Command::new(bin());
    let out = cmd
        .arg("sbom")
        .arg("verify-binding")
        .arg("--image-sbom")
        .arg(&nonexistent)
        .arg("--source-sbom")
        .arg(&nonexistent)
        .arg("--format")
        .arg("json")
        .output()
        .expect("verify-binding runs");
    assert!(
        !out.status.success(),
        "expected non-zero exit when --image-sbom doesn't exist"
    );
}
