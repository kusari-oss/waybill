//! #1060 — a binary-tier component discovered from several files carries
//! each of them as its own scan-root-relative source path.
//!
//! Linkage components (one per soname) were built with every linking
//! binary `"; "`-joined into one source path. Normalisation stripped the
//! scan root from the start of that string only, so every path after the
//! first reached the SBOM as an absolute path on the scanning host.

use std::path::Path;
use std::process::Command;

mod common;
use common::bin;

fn scan_cdx(path: &Path) -> serde_json::Value {
    let out = tempfile::NamedTempFile::new().expect("tempfile");
    let fake_home = tempfile::tempdir().expect("fake-home tempdir");
    let mut cmd = Command::new(bin());
    common::normalize::apply_fake_home_env(&mut cmd, fake_home.path());
    let output = cmd
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(path)
        .arg("--output")
        .arg(out.path())
        .arg("--no-deep-hash")
        .output()
        .expect("waybill should run");
    assert!(
        output.status.success(),
        "scan failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_str(&std::fs::read_to_string(out.path()).expect("read sbom"))
        .expect("valid JSON")
}

fn source_files(c: &serde_json::Value) -> Vec<String> {
    c["properties"]
        .as_array()
        .and_then(|ps| ps.iter().find(|p| p["name"] == "waybill:source-files"))
        .and_then(|p| p["value"].as_str())
        .and_then(|v| serde_json::from_str(v).ok())
        .unwrap_or_default()
}

/// Two copies of one executable in different directories give each
/// library it links two parents. `/bin/ls` exists on every Unix CI host
/// and links at least the C library, whatever its binary format.
#[cfg(unix)]
#[test]
fn every_source_path_is_relative_and_separate() {
    let dir = tempfile::tempdir().expect("tempdir");
    for sub in ["a/bin", "b/bin"] {
        let d = dir.path().join(sub);
        std::fs::create_dir_all(&d).expect("mkdir");
        std::fs::copy("/bin/ls", d.join("tool")).expect("copy executable");
    }

    let sbom = scan_cdx(dir.path());
    let components = sbom["components"].as_array().expect("components");

    let linkage: Vec<&serde_json::Value> = components
        .iter()
        .filter(|c| {
            c["properties"].as_array().is_some_and(|ps| {
                ps.iter().any(|p| {
                    p["name"] == "waybill:evidence-kind" && p["value"] == "dynamic-linkage"
                })
            })
        })
        .collect();
    // The control: with no linkage component the loop below asserts
    // nothing.
    assert!(
        !linkage.is_empty(),
        "fixture must yield a dynamic-linkage component; got {}",
        serde_json::to_string_pretty(components).unwrap_or_default(),
    );
    for c in &linkage {
        let paths = source_files(c);
        assert_eq!(paths, vec!["a/bin/tool".to_string(), "b/bin/tool".to_string()], "{c}");
        let locations: Vec<&str> = c["evidence"]["occurrences"]
            .as_array()
            .map(|os| os.iter().filter_map(|o| o["location"].as_str()).collect())
            .unwrap_or_default();
        assert_eq!(locations, vec!["a/bin/tool", "b/bin/tool"], "{c}");
    }

    // Nothing anywhere in the document may name the scanning host's
    // temp directory.
    let host = dir.path().canonicalize().expect("canonicalize");
    let raw = sbom.to_string();
    for prefix in [dir.path(), host.as_path()] {
        assert!(
            !raw.contains(prefix.to_string_lossy().as_ref()),
            "SBOM leaks the scan root {}",
            prefix.display(),
        );
    }
}
