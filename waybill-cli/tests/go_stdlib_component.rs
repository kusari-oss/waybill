//! Issue #364 integration test — Go SBOMs include a `stdlib`
//! component carrying the Go toolchain version from the project's
//! go.mod. Closes the vulnerability-scanning gap (e.g. CVE-2024-34156
//! big.Int overflow) on the same shape syft v1.42.3 produces:
//!
//!   PURL: pkg:golang/stdlib@v<go-version>
//!   CPE:  cpe:2.3:a:golang:go:<go-version>:*:*:*:*:*:*:*

use std::path::{Path, PathBuf};
use std::process::Command;

fn fixture(sub: &str) -> PathBuf {
    PathBuf::from(env!("WAYBILL_FIXTURES_DIR")).join(sub)
}

fn run_scan(path: &Path) -> serde_json::Value {
    let bin = env!("CARGO_BIN_EXE_waybill");
    let tmp = tempfile::tempdir().expect("tempdir");
    let out_path = tmp.path().join("sbom.cdx.json");
    let status = Command::new(bin)
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(path)
        .arg("--output")
        .arg(&out_path)
        .arg("--file-inventory=off")
        .arg("--no-deep-hash")
        .status()
        .expect("waybill should run");
    assert!(status.success(), "scan failed");
    let raw = std::fs::read(&out_path).expect("read sbom");
    serde_json::from_slice(&raw).expect("valid JSON")
}

fn find_stdlib(sbom: &serde_json::Value) -> Option<&serde_json::Value> {
    sbom["components"]
        .as_array()?
        .iter()
        .find(|c| c["name"].as_str() == Some("stdlib"))
}

#[test]
#[cfg_attr(test, allow(clippy::unwrap_used))]
fn go_source_scan_emits_stdlib_component_with_purl_and_cpe() {
    let path = fixture("go/simple-module");
    let sbom = run_scan(&path);
    let stdlib = find_stdlib(&sbom).expect("stdlib component must be emitted on a Go scan");

    let purl = stdlib["purl"].as_str().unwrap_or("");
    assert!(
        purl.starts_with("pkg:golang/stdlib@v"),
        "stdlib PURL must follow `pkg:golang/stdlib@v<version>`; got {purl:?}"
    );

    let cpe = stdlib["cpe"].as_str().unwrap_or("");
    assert!(
        cpe.starts_with("cpe:2.3:a:golang:go:"),
        "stdlib CPE must use NVD's `golang:go` vendor/product slug; got {cpe:?}"
    );
    // The CPE version segment must match the PURL version with the
    // `v`-prefix stripped (NVD's bare-version convention).
    let purl_version = purl.trim_start_matches("pkg:golang/stdlib@v");
    let expected_cpe_prefix = format!("cpe:2.3:a:golang:go:{purl_version}:");
    assert!(
        cpe.starts_with(&expected_cpe_prefix),
        "stdlib CPE version segment {cpe:?} must match the PURL bare version {purl_version:?}"
    );

    assert_eq!(
        stdlib["type"].as_str(),
        Some("library"),
        "stdlib CDX type must be `library`"
    );

    // Build-inclusion misclassification regression: stdlib must NOT
    // carry `waybill:build-inclusion = "not-needed"` (false positive
    // from `go mod why stdlib` returning "package not in import
    // graph"). See `apply_go_mod_why_verdicts` stdlib skip.
    let props = stdlib["properties"].as_array().cloned().unwrap_or_default();
    for p in &props {
        if p["name"].as_str() == Some("waybill:build-inclusion") {
            assert_ne!(
                p["value"].as_str(),
                Some("not-needed"),
                "stdlib MUST NOT be classified `not-needed` (#364 regression)"
            );
        }
    }
}

/// #1068 — each module links to the stdlib of the `go` version it
/// declares. With two versions in one tree, every module used to link to
/// whichever stdlib was indexed last: the edge named bare `stdlib`, and
/// the resolver's `(ecosystem, name)` index holds one entry per name.
#[test]
fn each_module_links_to_the_stdlib_of_its_own_go_version() {
    let dir = tempfile::tempdir().expect("tempdir");
    for (m, go) in [("a", "1.21"), ("b", "1.23"), ("c", "1.21")] {
        let root = dir.path().join(m);
        std::fs::create_dir_all(&root).expect("mkdir");
        std::fs::write(root.join("go.mod"), format!("module example.com/{m}\n\ngo {go}\n"))
            .expect("write go.mod");
        std::fs::write(root.join("go.sum"), "").expect("write go.sum");
    }

    let sbom = run_scan(dir.path());

    let edges: Vec<(String, String)> = sbom["dependencies"]
        .as_array()
        .expect("dependencies")
        .iter()
        .flat_map(|d| {
            let from = d["ref"].as_str().unwrap_or_default().to_string();
            d["dependsOn"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|t| t.as_str())
                .filter(|t| t.starts_with("pkg:golang/stdlib@"))
                .map(move |t| (from.clone(), t.to_string()))
                .collect::<Vec<_>>()
        })
        .collect();
    let mut stdlib_edges: Vec<(&str, &str)> =
        edges.iter().map(|(f, t)| (f.as_str(), t.as_str())).collect();
    stdlib_edges.sort();
    assert_eq!(
        stdlib_edges,
        vec![
            ("pkg:golang/example.com/a@v0.0.0-unknown", "pkg:golang/stdlib@v1.21"),
            ("pkg:golang/example.com/b@v0.0.0-unknown", "pkg:golang/stdlib@v1.23"),
            ("pkg:golang/example.com/c@v0.0.0-unknown", "pkg:golang/stdlib@v1.21"),
        ],
    );
}

/// #1068 — a module links only to its own stdlib, not its parent's.
/// The main-module match was a string prefix on the root path, so the
/// root `.` reached the nested `sub/` module, and `a` reached its sibling
/// `ab`. A bare `stdlib` dependency masked that; a versioned one would
/// give the nested module both versions.
#[test]
fn a_nested_or_prefix_sharing_module_gets_only_its_own_stdlib() {
    let dir = tempfile::tempdir().expect("tempdir");
    for (rel, m, go) in [(".", "root", "1.21"), ("sub", "sub", "1.23"), ("a", "a", "1.21"), ("ab", "ab", "1.22")] {
        let root = dir.path().join(rel);
        std::fs::create_dir_all(&root).expect("mkdir");
        std::fs::write(root.join("go.mod"), format!("module example.com/{m}\n\ngo {go}\n"))
            .expect("write go.mod");
        std::fs::write(root.join("go.sum"), "").expect("write go.sum");
    }

    let sbom = run_scan(dir.path());

    let mut stdlib_edges: Vec<(String, String)> = sbom["dependencies"]
        .as_array()
        .expect("dependencies")
        .iter()
        .flat_map(|d| {
            let from = d["ref"].as_str().unwrap_or_default().to_string();
            d["dependsOn"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|t| t.as_str())
                .filter(|t| t.starts_with("pkg:golang/stdlib@"))
                .map(move |t| (from.clone(), t.to_string()))
                .collect::<Vec<_>>()
        })
        .collect();
    stdlib_edges.sort();
    // The root module is the document subject; match it by suffix.
    let simplified: Vec<(String, &str)> = stdlib_edges
        .iter()
        .map(|(f, t)| {
            let name = f.rsplit('/').next().unwrap_or(f).split('@').next().unwrap_or(f);
            (name.to_string(), t.as_str())
        })
        .collect();
    let mut expected = vec![
        ("a".to_string(), "pkg:golang/stdlib@v1.21"),
        ("ab".to_string(), "pkg:golang/stdlib@v1.22"),
        ("root".to_string(), "pkg:golang/stdlib@v1.21"),
        ("sub".to_string(), "pkg:golang/stdlib@v1.23"),
    ];
    expected.sort();
    let mut got = simplified.clone();
    got.sort();
    assert_eq!(got, expected, "raw edges: {stdlib_edges:?}");
}
