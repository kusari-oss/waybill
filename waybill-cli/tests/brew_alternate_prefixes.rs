//! Milestone 136 US2 — alternate Homebrew prefixes (Intel macOS
//! `/usr/local`, Linuxbrew `/home/linuxbrew/.linuxbrew`) get the same
//! component PURLs as Apple Silicon. The reader detects all three
//! prefixes independently; the install location does NOT leak into
//! the PURL identity.
//!
//! Covers spec acceptance scenarios US2.1–US2.4 + SC-002 + FR-001 +
//! FR-009 (cross-reader coexistence with dpkg).

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

fn run_scan(rootfs: &Path) -> Value {
    let out_dir = tempfile::tempdir().unwrap();
    let out_path = out_dir.path().join("out.cdx.json");
    let result = Command::new(binary_path())
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(rootfs)
        .arg("--no-deep-hash")
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", out_path.display()))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&result.stderr),
    );
    let bytes = std::fs::read(&out_path).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn write_formula(
    rootfs: &Path,
    prefix: &str,
    formula: &str,
    version: &str,
    receipt_body: &str,
) {
    let dir = rootfs
        .join(prefix)
        .join("Cellar")
        .join(formula)
        .join(version);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("INSTALL_RECEIPT.json"), receipt_body).unwrap();
}

fn brew_purls(doc: &Value) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut visit = |c: &Value| {
        if let Some(p) = c.get("purl").and_then(|v| v.as_str()) {
            if p.starts_with("pkg:brew/") {
                out.push(p.to_string());
            }
        }
    };
    if let Some(arr) = doc.get("components").and_then(|v| v.as_array()) {
        for c in arr {
            visit(c);
        }
    }
    if let Some(c) = doc.get("metadata").and_then(|m| m.get("component")) {
        visit(c);
    }
    out
}

fn purls_with_prefix(doc: &Value, prefix: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Some(arr) = doc.get("components").and_then(|v| v.as_array()) {
        for c in arr {
            if let Some(p) = c.get("purl").and_then(|v| v.as_str()) {
                if p.starts_with(prefix) {
                    out.push(p.to_string());
                }
            }
        }
    }
    out
}

#[test]
fn intel_macos_emits_identical_purl_to_apple_silicon() {
    let tmp = tempfile::tempdir().unwrap();
    write_formula(
        tmp.path(),
        "usr/local",
        "curl",
        "8.5.0",
        r#"{"source": {"tap": "homebrew/core"}}"#,
    );
    let doc = run_scan(tmp.path());
    let purls = brew_purls(&doc);
    assert_eq!(purls, vec!["pkg:brew/curl@8.5.0".to_string()]);
}

#[test]
fn linuxbrew_emits_identical_purl_to_apple_silicon() {
    let tmp = tempfile::tempdir().unwrap();
    write_formula(
        tmp.path(),
        "home/linuxbrew/.linuxbrew",
        "curl",
        "8.5.0",
        r#"{"source": {"tap": "homebrew/core"}}"#,
    );
    let doc = run_scan(tmp.path());
    let purls = brew_purls(&doc);
    assert_eq!(purls, vec!["pkg:brew/curl@8.5.0".to_string()]);
}

#[test]
fn usr_local_without_cellar_emits_zero_brew_components() {
    // U2 (analysis remediation) — non-ELF README at /usr/local/share/
    // avoids binary-walker pollution; PURL-prefix-specific assertion
    // avoids counting unrelated walker output.
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("usr/local/share")).unwrap();
    std::fs::write(tmp.path().join("usr/local/share/README.txt"), "hello world").unwrap();
    let doc = run_scan(tmp.path());
    let purls = brew_purls(&doc);
    assert!(
        purls.is_empty(),
        "/usr/local/ without Cellar/ must produce zero brew components; got {purls:?}",
    );
}

#[test]
fn multi_prefix_dedup_collapses_purl_identical_entries() {
    // T016 — same formula present at BOTH /opt/homebrew and /usr/local
    // (pathological Apple Silicon machine with a leftover Rosetta-era
    // /usr/local install). PURL-identical components dedupe via the
    // standard seen_purls collision check.
    let tmp = tempfile::tempdir().unwrap();
    write_formula(
        tmp.path(),
        "opt/homebrew",
        "curl",
        "8.5.0",
        r#"{"source": {"tap": "homebrew/core"}}"#,
    );
    write_formula(
        tmp.path(),
        "usr/local",
        "curl",
        "8.5.0",
        r#"{"source": {"tap": "homebrew/core"}}"#,
    );
    let doc = run_scan(tmp.path());
    let purls = brew_purls(&doc);
    assert_eq!(
        purls.len(),
        1,
        "multi-prefix duplicates must dedupe to one component; got {purls:?}",
    );
    assert_eq!(purls[0], "pkg:brew/curl@8.5.0");
}

#[test]
fn cross_reader_linuxbrew_and_dpkg_coexist() {
    // T016b (closes analysis-finding C1 / FR-009 / US2 acceptance
    // scenario 2): a Linuxbrew install on a Debian rootfs produces
    // BOTH brew + deb components — neither reader suppresses the
    // other.
    let tmp = tempfile::tempdir().unwrap();

    // Linuxbrew side.
    write_formula(
        tmp.path(),
        "home/linuxbrew/.linuxbrew",
        "curl",
        "8.5.0",
        r#"{"source": {"tap": "homebrew/core"}}"#,
    );

    // Synthetic minimal dpkg DB declaring an unrelated package.
    // The dpkg parser is RFC-822-style stanzas; a single stanza
    // marked `install ok installed` is sufficient.
    std::fs::create_dir_all(tmp.path().join("var/lib/dpkg")).unwrap();
    std::fs::write(
        tmp.path().join("var/lib/dpkg/status"),
        "Package: bash\nStatus: install ok installed\nVersion: 5.2.15-2+b8\nArchitecture: amd64\nMaintainer: Test\nDescription: GNU Bash\n",
    )
    .unwrap();
    // /etc/os-release so dpkg picks the debian namespace.
    std::fs::create_dir_all(tmp.path().join("etc")).unwrap();
    std::fs::write(
        tmp.path().join("etc/os-release"),
        "ID=debian\nVERSION_ID=12\n",
    )
    .unwrap();

    let doc = run_scan(tmp.path());

    // (a) brew component emits.
    let brew = brew_purls(&doc);
    assert!(
        brew.contains(&"pkg:brew/curl@8.5.0".to_string()),
        "brew curl must emit; got {brew:?}",
    );

    // (b) dpkg component emits.
    let deb = purls_with_prefix(&doc, "pkg:deb/");
    assert!(
        deb.iter().any(|p| p.contains("/bash@5.2.15")),
        "dpkg bash must emit alongside brew; got {deb:?}",
    );

    // (c) Both surface in the same SBOM — neither suppresses the other.
}
