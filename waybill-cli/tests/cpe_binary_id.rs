//! Milestone 097 integration test — CPE candidate emission for
//! binary-extracted `pkg:generic/<lib>@<version>` components.
//!
//! Negative-control coverage for SC-007: waybill's own binary uses
//! rustls (not OpenSSL), so scanning the waybill binary itself must
//! NOT produce an OpenSSL CPE. If this test ever fires, either:
//! (a) the milestone-096 version-string scanner now matches an
//!     OpenSSL literal embedded in waybill (a spurious match — fix
//!     the scanner's anchor regex),
//! (b) the milestone-097 CPE table has a row whose vendor/product
//!     would produce an `openssl:openssl:` CPE for an unrelated PURL
//!     (impossible by construction — the table maps slug → vendor),
//! (c) waybill genuinely started linking OpenSSL (which would be a
//!     real dependency change worth reviewing).
//!
//! Positive end-to-end coverage is via the unit tests in
//! `cpe.rs::tests` (8 new tests covering canonical OpenSSL,
//! dual-candidate curl, OpenJDK suffix-strip, composite-evidence
//! one-CPE, table well-formedness, all-row validity, slug coverage,
//! and versionless-suppression). No toolchain-dependent fixture is
//! required; build-OpenSSL fixtures are deliberately out of scope
//! to keep CI hermetic.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;

/// SC-007 negative control: waybill binary should not trip the
/// milestone-097 CPE table OR the milestone-096 version-string
/// scanner. Scanning waybill-itself emits NO `openssl:openssl` CPE
/// in the resulting CDX SBOM.
#[test]
fn mikebom_self_scan_emits_no_spurious_openssl_cpe() {
    let dir = TempDir::new().unwrap();
    let dest = dir.path().join("waybill-under-test");
    std::fs::copy(env!("CARGO_BIN_EXE_waybill"), &dest).unwrap();

    let out_file = dir.path().join("out.cdx.json");
    let output = Command::new(env!("CARGO_BIN_EXE_waybill"))
        .args(["sbom", "scan", "--path"])
        .arg(dir.path())
        .arg("--output")
        .arg(&out_file)
        .arg("--no-deep-hash")
        .output()
        .expect("failed to invoke waybill");
    assert!(
        output.status.success(),
        "waybill sbom scan failed: stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );

    let bytes = std::fs::read(&out_file).expect("SBOM not written");
    let sbom: Value = serde_json::from_slice(&bytes).expect("invalid SBOM JSON");
    let json_str = serde_json::to_string(&sbom).expect("SBOM JSON re-serialize");

    assert!(
        !json_str.contains("cpe:2.3:a:openssl:openssl:"),
        "milestone-097 SC-007 spurious-emission guard: waybill binary \
         should not emit an openssl:openssl CPE (waybill uses rustls). \
         If this fires, check (a) milestone-096 version-string scanner \
         for a false-positive anchor match, OR (b) whether waybill now \
         genuinely links OpenSSL (a real dependency change)."
    );
}
