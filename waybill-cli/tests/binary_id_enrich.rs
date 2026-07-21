//! Milestone 096 integration tests — identify-unknown-binaries
//! enrichment (embedded version strings + packer transparency +
//! symbol fingerprinting).
//!
//! These tests run end-to-end against the compiled `mikebom` binary
//! and verify the milestone-096 invariants on real binaries available
//! on the test host. They SKIP cleanly when no suitable binary exists
//! (e.g., minimal CI container missing `/bin/ls`) rather than
//! false-failing.
//!
//! Coverage map:
//! - **US1** (FR-001 embedded version strings): unit-tested
//!   exhaustively in `scan_fs::binary::version_strings::tests`; this
//!   file's integration coverage is the negative-control assertion
//!   that mikebom itself (which uses rustls, not OpenSSL) does NOT
//!   emit a `pkg:generic/openssl@*` component.
//! - **US2** (FR-003 packer transparency, Q2 always-emit): asserts
//!   every file-level binary component carries `mikebom:binary-packed`
//!   with value `none` on an unpacked binary.
//! - **US3** (FR-004 symbol fingerprinting): negative-control —
//!   mikebom's own dynamic symbol table doesn't match the 3 v1
//!   fingerprints (openssl/zlib/libcurl) so no spurious
//!   `pkg:generic/openssl|zlib|libcurl` should appear. The
//!   composite-evidence merge (Q1) is unit-tested via
//!   `symbol_fingerprint::tests::two_libraries_both_match` and
//!   verified at the binary-scanner level by the SC-007 ≤1-spurious
//!   bound enforced at PR-review time.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_mikebom")
}

fn scan(dir: &Path) -> Value {
    let out_file = dir.join("out.cdx.json");
    let output = Command::new(binary_path())
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(dir)
        .arg("--output")
        .arg(&out_file)
        .arg("--no-deep-hash")
        .output()
        .expect("failed to invoke mikebom");
    assert!(
        output.status.success(),
        "mikebom sbom scan failed: stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );
    let json_bytes = std::fs::read(&out_file).expect("SBOM not written");
    serde_json::from_slice(&json_bytes).expect("invalid JSON")
}

fn property_value(component: &Value, name: &str) -> Option<String> {
    component["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"].as_str() == Some(name))
        .and_then(|p| p["value"].as_str().map(|s| s.to_string()))
}

fn find_file_level(sbom: &Value) -> Option<&Value> {
    sbom["components"]
        .as_array()?
        .iter()
        .find(|c| property_value(c, "mikebom:binary-class").is_some())
}

fn find_system_binary() -> Option<PathBuf> {
    for candidate in ["/bin/ls", "/usr/bin/ls"] {
        let p = PathBuf::from(candidate);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

/// Contract 2 (FR-003 + Clarification Q2 always-emit): every
/// file-level binary component carries `mikebom:binary-packed` even
/// when the binary is not packed. Value is `"none"` on an unpacked
/// binary (the universal case for `/bin/ls`).
#[test]
fn unpacked_binary_emits_binary_packed_none() {
    let Some(src) = find_system_binary() else {
        eprintln!("skipping: no /bin/ls on host");
        return;
    };

    let dir = TempDir::new().unwrap();
    let dest = dir.path().join("sample");
    std::fs::copy(&src, &dest).unwrap();

    let sbom = scan(dir.path());
    let file_level = find_file_level(&sbom)
        .expect("file-level binary component missing — discover step failed");
    let packed = property_value(file_level, "mikebom:binary-packed");
    assert_eq!(
        packed.as_deref(),
        Some("none"),
        "Q2 always-emit invariant: unpacked binary must carry \
         mikebom:binary-packed = 'none' (got {packed:?}). The file-level \
         entry's properties were {:?}",
        file_level["properties"]
    );
}

/// Contract 1 negative control (FR-001 / SC-007 spurious-match bound):
/// mikebom itself does NOT statically link OpenSSL, zlib, libcurl,
/// SQLite, or libxml2. Its binary scan MUST NOT emit any
/// `pkg:generic/<v1-lib>@<version>` component. If this assertion fires
/// the v1 pattern set has a false-positive in mikebom's own
/// `.rodata` / `__cstring` / `.rdata`. Tightens the anchor.
#[test]
fn mikebom_itself_does_not_emit_spurious_version_strings() {
    // Scan a directory containing only the mikebom binary itself —
    // its own bytes shouldn't trip any v1 anchor.
    let dir = TempDir::new().unwrap();
    let dest = dir.path().join("mikebom-under-test");
    std::fs::copy(binary_path(), &dest).unwrap();

    let sbom = scan(dir.path());
    let components = sbom["components"].as_array().unwrap();
    let spurious: Vec<&Value> = components
        .iter()
        .filter(|c| {
            let purl = c["purl"].as_str().unwrap_or("");
            purl.starts_with("pkg:generic/openssl@")
                || purl.starts_with("pkg:generic/zlib@")
                || purl.starts_with("pkg:generic/curl@")
                || purl.starts_with("pkg:generic/libcurl@")
                || purl.starts_with("pkg:generic/sqlite@")
                || purl.starts_with("pkg:generic/libxml2@")
        })
        .collect();
    assert!(
        spurious.is_empty(),
        "SC-007 false-positive guard: mikebom binary should not trip \
         any v1 version-string pattern. Found {} spurious matches: {:?}",
        spurious.len(),
        spurious.iter().map(|c| c["purl"].as_str()).collect::<Vec<_>>(),
    );
}

/// Contract 3 negative control (FR-004 / SC-007): mikebom's dynamic
/// symbol table doesn't export the OpenSSL / zlib / libcurl public
/// API. The 3 v1 fingerprints should produce zero
/// `pkg:generic/<lib>` (no-version) emissions on mikebom's own bytes.
/// If this fires either (a) mikebom started linking one of those
/// libraries (signaling a real dependency change), or (b) the
/// fingerprint threshold is too loose.
#[test]
fn mikebom_itself_does_not_emit_spurious_symbol_fingerprints() {
    let dir = TempDir::new().unwrap();
    let dest = dir.path().join("mikebom-under-test");
    std::fs::copy(binary_path(), &dest).unwrap();

    let sbom = scan(dir.path());
    let components = sbom["components"].as_array().unwrap();
    let spurious: Vec<&Value> = components
        .iter()
        .filter(|c| {
            let purl = c["purl"].as_str().unwrap_or("");
            // Match the no-version PURL shape that
            // symbol_match_to_entry emits. Trailing `@` or `?`
            // would indicate a version-string composite — handled by
            // the version-string spurious-match test.
            matches!(
                purl,
                // Milestone-096 v1 fingerprints.
                "pkg:generic/openssl"
                    | "pkg:generic/zlib"
                    | "pkg:generic/libcurl"
                    // Milestone-099 v2 fingerprint expansion. Mikebom
                    // uses rustls + Rust's own regex crate (NOT linking
                    // any of these libraries) so the assertion still
                    // holds — a regression here would mean mikebom
                    // now actually depends on the matched library.
                    | "pkg:generic/sqlite"
                    | "pkg:generic/pcre"
                    | "pkg:generic/pcre2"
                    | "pkg:generic/gnutls"
            )
        })
        .collect();
    assert!(
        spurious.is_empty(),
        "SC-007 false-positive guard: mikebom should not trip any v1 \
         symbol fingerprint. Found {} spurious matches: {:?}",
        spurious.len(),
        spurious.iter().map(|c| c["purl"].as_str()).collect::<Vec<_>>(),
    );
}
