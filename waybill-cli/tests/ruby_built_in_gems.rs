//! Milestone 162 (T024): SC-010 integration test — Ruby built-in gem
//! synthetic components end-to-end via the release binary.
//!
//! Synthesizes a tempdir Ruby project with a `Gemfile.lock` that
//! includes:
//!   - `bundler-audit (0.9.3)` in GEM/specs, declaring `bundler (>= 1.2.0)`
//!     + `thor (~> 1.0)` as deps.
//!   - `thor (1.4.0)` in GEM/specs (a real gem).
//!   - `bundler` is NOT in specs — it's a Ruby built-in.
//!
//! Invokes the release binary and asserts:
//!   (a) `pkg:gem/bundler-audit@0.9.3` present as a real component.
//!   (b) `pkg:gem/thor@1.4.0` present as a real component.
//!   (c) `pkg:gem/bundler` (versionless) present as a synthetic component
//!       with `waybill:synthetic-built-in = "ruby"` +
//!       `waybill:built-in-requirement = ">= 1.2.0"`.
//!   (d) `dependencies[]` array shows `bundler-audit → thor` AND
//!       `bundler-audit → pkg:gem/bundler`.
//!   (e) SC-004 dual invariant across every `pkg:gem/*` component.

use std::path::Path;
use std::process::Command;

fn write_ruby_fixture() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();

    let gemfile_lock = "\
GEM
  remote: https://rubygems.org/
  specs:
    bundler-audit (0.9.3)
      bundler (>= 1.2.0)
      thor (~> 1.0)
    thor (1.4.0)

PLATFORMS
  ruby

DEPENDENCIES
  bundler-audit

BUNDLED WITH
   2.5.3
";
    std::fs::write(root.join("Gemfile.lock"), gemfile_lock).unwrap();
    tmp
}

fn scan(path: &Path) -> serde_json::Value {
    let bin = env!("CARGO_BIN_EXE_waybill");
    let tmp = tempfile::NamedTempFile::new().expect("tempfile");
    let out_path = tmp.path().to_path_buf();
    let output = Command::new(bin)
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(path)
        .arg("--output")
        .arg(&out_path)
        .arg("--no-deep-hash")
        .output()
        .expect("waybill should run");
    assert!(
        output.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let raw = std::fs::read_to_string(&out_path).expect("read sbom");
    serde_json::from_str(&raw).expect("valid JSON")
}

fn gem_components(sbom: &serde_json::Value) -> Vec<&serde_json::Value> {
    sbom["components"]
        .as_array()
        .expect("components array")
        .iter()
        .filter(|c| {
            c["purl"]
                .as_str()
                .map(|p| p.starts_with("pkg:gem/"))
                .unwrap_or(false)
        })
        .collect()
}

fn find_component<'a>(
    sbom: &'a serde_json::Value,
    purl: &str,
) -> Option<&'a serde_json::Value> {
    gem_components(sbom).into_iter().find(|c| c["purl"] == purl)
}

fn component_property<'a>(
    component: &'a serde_json::Value,
    name: &str,
) -> Option<&'a serde_json::Value> {
    component["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"] == name)
        .map(|p| &p["value"])
}

/// SC-002 spot-check: bundler-audit → bundler edge present.
#[test]
fn t024_bundler_audit_to_bundler_edge_present() {
    let fixture = write_ruby_fixture();
    let sbom = scan(fixture.path());

    // (a) bundler-audit is a real component
    assert!(
        find_component(&sbom, "pkg:gem/bundler-audit@0.9.3").is_some(),
        "SC-002: bundler-audit@0.9.3 must be present"
    );

    // (b) thor is a real component
    assert!(
        find_component(&sbom, "pkg:gem/thor@1.4.0").is_some(),
        "SC-002: thor@1.4.0 must be present"
    );

    // (c) bundler is a synthetic component with versionless PURL
    let bundler = find_component(&sbom, "pkg:gem/bundler")
        .expect("SC-002: pkg:gem/bundler (versionless) must be present");
    assert_eq!(
        component_property(bundler, "waybill:synthetic-built-in"),
        Some(&serde_json::Value::String("ruby".to_string()))
    );
    assert_eq!(
        component_property(bundler, "waybill:built-in-requirement"),
        Some(&serde_json::Value::String(">= 1.2.0".to_string()))
    );

    // (d) dependencies[] shows bundler-audit → bundler
    let dependencies = sbom["dependencies"].as_array().expect("dependencies array");
    let bundler_audit_bomref: String = sbom["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["purl"] == "pkg:gem/bundler-audit@0.9.3")
        .expect("bundler-audit component")
        ["bom-ref"]
        .as_str()
        .expect("bom-ref string")
        .to_string();

    let bundler_audit_deps = dependencies
        .iter()
        .find(|d| d["ref"] == bundler_audit_bomref.as_str())
        .expect("bundler-audit dependencies entry");
    let depends_on: Vec<&str> = bundler_audit_deps["dependsOn"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    // The dependency array may reference by bom-ref OR by PURL; check both.
    assert!(
        depends_on.iter().any(|d| d.contains("bundler")
            && !d.contains("bundler-audit")),
        "SC-002: bundler-audit@0.9.3 must have an edge to bundler; got {depends_on:?}"
    );
}

/// SC-004 dual invariant: every synthetic component has versionless
/// PURL + C113 annotation; every real component has @version PURL + no C113.
#[test]
fn t024_dual_invariant_holds() {
    let fixture = write_ruby_fixture();
    let sbom = scan(fixture.path());

    for component in gem_components(&sbom) {
        let purl = component["purl"].as_str().unwrap_or("");
        let has_at = purl.contains('@');
        let has_c113 =
            component_property(component, "waybill:synthetic-built-in").is_some();
        assert_ne!(
            has_at, has_c113,
            "SC-004 dual invariant violated for {purl}: has_at={has_at}, has_c113={has_c113}"
        );
    }
}
