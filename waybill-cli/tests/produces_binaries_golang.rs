//! Integration tests for the milestone 116 PR-C golang slice (FR-010).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixture(sub: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("produces_binaries")
        .join("golang")
        .join(sub)
}

fn run_scan(path: &Path, out_path: &Path) -> Output {
    let bin = env!("CARGO_BIN_EXE_mikebom");
    Command::new(bin)
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(path)
        .arg("--output")
        .arg(out_path)
        .arg("--no-deep-hash")
        .output()
        .expect("mikebom should run")
}

fn read_sbom(path: &Path) -> serde_json::Value {
    let raw = std::fs::read_to_string(path).expect("read sbom");
    serde_json::from_str(&raw).expect("valid JSON")
}

fn produces_binaries_for_purl(sbom: &serde_json::Value, purl: &str) -> Option<Vec<String>> {
    let mut candidates: Vec<&serde_json::Value> = Vec::new();
    if let Some(c) = sbom.get("metadata").and_then(|m| m.get("component")) {
        candidates.push(c);
    }
    if let Some(arr) = sbom.get("components").and_then(|v| v.as_array()) {
        for c in arr {
            candidates.push(c);
        }
    }
    for c in candidates {
        if c.get("purl").and_then(|v| v.as_str()) == Some(purl) {
            let Some(props) = c.get("properties").and_then(|v| v.as_array()) else {
                return Some(Vec::new());
            };
            for p in props {
                if p.get("name").and_then(|v| v.as_str()) == Some("mikebom:produces-binaries") {
                    let v = p.get("value").and_then(|v| v.as_str())?;
                    let arr: Vec<String> = serde_json::from_str(v).ok()?;
                    return Some(arr);
                }
            }
            return Some(Vec::new());
        }
    }
    None
}

fn produces_binaries_anywhere(
    sbom: &serde_json::Value,
    purl_prefix: &str,
) -> Option<Vec<String>> {
    let mut candidates: Vec<&serde_json::Value> = Vec::new();
    if let Some(c) = sbom.get("metadata").and_then(|m| m.get("component")) {
        candidates.push(c);
    }
    if let Some(arr) = sbom.get("components").and_then(|v| v.as_array()) {
        for c in arr {
            candidates.push(c);
        }
    }
    for c in candidates {
        let p = c.get("purl").and_then(|v| v.as_str()).unwrap_or("");
        if p.starts_with(purl_prefix) {
            let Some(props) = c.get("properties").and_then(|v| v.as_array()) else {
                continue;
            };
            for prop in props {
                if prop.get("name").and_then(|v| v.as_str())
                    == Some("mikebom:produces-binaries")
                {
                    let v = prop.get("value").and_then(|v| v.as_str())?;
                    let arr: Vec<String> = serde_json::from_str(v).ok()?;
                    return Some(arr);
                }
            }
            return Some(Vec::new());
        }
    }
    None
}

#[test]
fn cmd_layout_emits_basename_per_package_main_dir() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let out = tmp.path().join("sbom.cdx.json");
    let output = run_scan(&fixture("cmd-layout"), &out);
    assert!(
        output.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );
    let sbom = read_sbom(&out);
    let produced = produces_binaries_anywhere(&sbom, "pkg:golang/github.com/foo/fixture-baz")
        .expect("Go main-module component should be present and carry produces-binaries");
    // Lex-sorted per the shared normalizer.
    assert_eq!(
        produced,
        vec!["baz".to_string(), "baz-helper".to_string()],
        "cmd/baz/main.go and cmd/baz-helper/main.go both detected"
    );
}

#[test]
fn root_main_emits_project_root_basename() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let out = tmp.path().join("sbom.cdx.json");
    let output = run_scan(&fixture("root-main"), &out);
    assert!(
        output.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );
    let sbom = read_sbom(&out);
    let produced = produces_binaries_anywhere(&sbom, "pkg:golang/github.com/foo/root-main")
        .expect("root-main Go main-module component should be present");
    assert_eq!(produced, vec!["root-main".to_string()]);
}

#[test]
fn library_only_go_module_omits_property() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let out = tmp.path().join("sbom.cdx.json");
    let output = run_scan(&fixture("library-only"), &out);
    assert!(
        output.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );
    let sbom = read_sbom(&out);
    let produced =
        produces_binaries_anywhere(&sbom, "pkg:golang/github.com/foo/fixture-libonly");
    match produced {
        None => {} // component not emitted
        Some(v) => assert!(
            v.is_empty(),
            "library-only Go module must NOT carry produces-binaries; got {v:?}"
        ),
    }
    // Silence the unused fn warning if it's referenced indirectly only.
    let _ = produces_binaries_for_purl;
}
