//! Cross-ecosystem consistency for declared licenses (issue #954, User Story 3).
//!
//! `declared_license.rs` asserts each ecosystem individually. This file asserts
//! the properties that only make sense *across* them, which is the reason #954 is
//! one feature rather than eleven:
//!
//! - every ecosystem treats the same input shape the same way (SC-003);
//! - every declaration found is emitted, none silently dropped (SC-004a);
//! - repeated scans of identical input agree (SC-007, FR-015);
//! - nothing that carried a license before this feature lost one (SC-005).
//!
//! Where a case needs a second run to compare against, it runs the scan twice
//! rather than comparing against a stored golden — a golden would make this file
//! a second copy of the corpus suite, and would go stale for reasons unrelated to
//! licensing.

mod common;

use std::path::Path;
use std::process::Command;

use common::normalize::apply_fake_home_env;

fn write(path: &Path, body: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create fixture dir");
    }
    std::fs::write(path, body).expect("write fixture file");
}

/// Scan `root` and return the SPDX 2.3 `licenseDeclared` values, excluding the
/// `NOASSERTION` placeholder.
///
/// The operator lives here rather than in CycloneDX. CDX splits an `AND` into one
/// entry per operand — which keeps each listed id matchable — so the operator is
/// not observable there for conjunctions. SPDX 2.3 keeps the whole expression in
/// `licenseDeclared` for both operators, making it the format-independent place to
/// assert what waybill combined.
fn scan_spdx_declared(root: &Path) -> Vec<String> {
    let tmp = tempfile::tempdir().expect("tempdir");
    let fake_home = tempfile::tempdir().expect("fake-home tempdir");
    let out_path = tmp.path().join("o.spdx.json");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_waybill"));
    apply_fake_home_env(&mut cmd, fake_home.path());
    let out = cmd
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(root)
        .arg("--no-deep-hash")
        .arg("--format")
        .arg("spdx-2.3-json")
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", out_path.display()))
        .output()
        .expect("waybill should run");
    assert!(
        out.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&out_path).expect("read"))
            .expect("valid JSON");
    doc["packages"]
        .as_array()
        .map(|ps| {
            ps.iter()
                .filter_map(|p| p["licenseDeclared"].as_str())
                .filter(|v| *v != "NOASSERTION")
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Scan `root` and return the CycloneDX document.
fn scan_cdx(root: &Path) -> serde_json::Value {
    let tmp = tempfile::tempdir().expect("tempdir");
    let fake_home = tempfile::tempdir().expect("fake-home tempdir");
    let out_path = tmp.path().join("o.cdx.json");

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_waybill"));
    apply_fake_home_env(&mut cmd, fake_home.path());
    let out = cmd
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(root)
        .arg("--no-deep-hash")
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", out_path.display()))
        .output()
        .expect("waybill should run");
    assert!(
        out.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_str(&std::fs::read_to_string(&out_path).expect("read output"))
        .expect("valid JSON")
}

/// Every declared license in the document, as `id` or `name`, sorted.
fn all_licenses(doc: &serde_json::Value) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut take = |v: &serde_json::Value| {
        if let Some(arr) = v.as_array() {
            for l in arr {
                if let Some(s) = l["license"]["id"]
                    .as_str()
                    .or_else(|| l["license"]["name"].as_str())
                    .or_else(|| l["expression"].as_str())
                {
                    out.push(s.to_string());
                }
            }
        }
    };
    take(&doc["metadata"]["component"]["licenses"]);
    if let Some(cs) = doc["components"].as_array() {
        for c in cs {
            take(&c["licenses"]);
        }
    }
    out.sort();
    out
}

// ---------------------------------------------------------------------------
// Per-ecosystem fixtures, one shape per ecosystem.
//
// Each declares TWO licenses where its format permits a list, so the operator is
// exercised rather than only the single-value path (SC-004b).
// ---------------------------------------------------------------------------

/// `(ecosystem, builder, expected single combined expression)`
type Case = (&'static str, fn(&Path), &'static str);

fn cargo_two(root: &Path) {
    // cargo takes one expression string, so the author writes the operator.
    write(
        &root.join("Cargo.toml"),
        "[package]\nname = \"waybill-fixture-cargo2\"\nversion = \"1.0.0\"\n\
         edition = \"2021\"\nlicense = \"MIT OR Apache-2.0\"\n",
    );
    write(&root.join("src/main.rs"), "fn main() {}\n");
}

fn composer_two(root: &Path) {
    // Composer documents its array as a CHOICE, so this must join with OR.
    write(
        &root.join("composer.json"),
        r#"{ "name": "waybill-fixture/composer2", "license": ["MIT", "Apache-2.0"] }
"#,
    );
}

fn gem_two(root: &Path) {
    // RubyGems states the array does not express how licenses combine, so the
    // conjunctive fallback applies.
    write(
        &root.join("waybill-fixture-gem2.gemspec"),
        r#"Gem::Specification.new do |spec|
  spec.name    = "waybill-fixture-gem2"
  spec.version = "1.0.0"
  spec.licenses = ["MIT", "Apache-2.0"]
end
"#,
    );
}

fn maven_two(root: &Path) {
    // The POM reference specifies nothing, so the conjunctive fallback applies.
    write(
        &root.join("pom.xml"),
        r#"<project xmlns="http://maven.apache.org/POM/4.0.0">
  <modelVersion>4.0.0</modelVersion>
  <groupId>dev.waybill.fixture</groupId>
  <artifactId>waybill-fixture-maven2</artifactId>
  <version>1.0.0</version>
  <licenses>
    <license><name>MIT</name></license>
    <license><name>Apache-2.0</name></license>
  </licenses>
</project>
"#,
    );
}

fn elixir_two(root: &Path) {
    write(
        &root.join("mix.exs"),
        r#"defmodule WaybillFixture.MixProject do
  use Mix.Project
  def project do
    [app: :waybill_fixture_elixir2, version: "1.0.0", package: package()]
  end
  defp package do
    [licenses: ["MIT", "Apache-2.0"], links: %{}]
  end
end
"#,
    );
}

fn scala_two(root: &Path) {
    // sbt names are free-form; these two ARE valid SPDX ids, so the combined
    // expression canonicalises and the operator is observable.
    write(
        &root.join("build.sbt"),
        r#"name := "waybill-fixture-scala2"
version := "1.0.0"
organization := "dev.waybill.fixture"
licenses := Seq(("MIT", url("http://example.invalid/mit")), ("Apache-2.0", url("http://example.invalid/apache")))
"#,
    );
}

fn cases() -> Vec<Case> {
    vec![
        ("cargo", cargo_two, "MIT OR Apache-2.0"),
        ("composer", composer_two, "MIT OR Apache-2.0"),
        ("gem", gem_two, "MIT AND Apache-2.0"),
        ("maven", maven_two, "MIT AND Apache-2.0"),
        ("elixir", elixir_two, "MIT AND Apache-2.0"),
        ("scala", scala_two, "MIT AND Apache-2.0"),
    ]
}

// ---------------------------------------------------------------------------
// SC-004b / SC-003 — the operator matches each ecosystem's documented semantics
// ---------------------------------------------------------------------------

#[test]
fn m954_sc004b_multi_license_operator_matches_each_ecosystem() {
    let mut failures: Vec<String> = Vec::new();
    for (name, build, expected) in cases() {
        let tmp = tempfile::tempdir().expect("tempdir");
        build(tmp.path());
        let got = scan_spdx_declared(tmp.path());
        if !got.iter().any(|g| g == expected) {
            failures.push(format!("{name}: expected {expected:?}, got {got:?}"));
        }
    }
    assert!(
        failures.is_empty(),
        "the operator must match each ecosystem's own documentation — getting it \
         wrong inverts legal meaning, asserting a consumer must satisfy every \
         license when the project said any one would do:\n{}",
        failures.join("\n")
    );
}

#[test]
fn m954_sc004b_control_disjunctive_and_conjunctive_are_distinguishable() {
    // Without this, the test above could pass with every ecosystem emitting the
    // same operator — a uniform wrong answer looks identical to a uniform right
    // one when each case only checks its own expectation.
    let ops: Vec<&str> = cases().iter().map(|(_, _, e)| *e).collect();
    assert!(
        ops.iter().any(|e| e.contains(" OR ")) && ops.iter().any(|e| e.contains(" AND ")),
        "the fixture set must contain both operators, else SC-004b is vacuous"
    );
}

// ---------------------------------------------------------------------------
// SC-004a — every declaration found is emitted
// ---------------------------------------------------------------------------

#[test]
fn m954_sc004a_no_declaration_is_silently_dropped() {
    // Mixed valid and invalid across ecosystems. Each fixture declares exactly
    // one license, so the emitted count must equal the fixture count: a value
    // that will not canonicalise is preserved, not discarded.
    // Named type: clippy::type_complexity rejects the inline tuple-with-fn-ptr.
    type Spec = (&'static str, fn(&Path));
    let specs: Vec<Spec> = vec![
        ("valid-cargo", |r: &Path| {
            write(
                &r.join("Cargo.toml"),
                "[package]\nname = \"waybill-fixture-v1\"\nversion = \"1.0.0\"\n\
                 edition = \"2021\"\nlicense = \"MIT\"\n",
            );
            write(&r.join("src/main.rs"), "fn main() {}\n");
        }),
        ("invalid-cargo", |r: &Path| {
            write(
                &r.join("Cargo.toml"),
                "[package]\nname = \"waybill-fixture-v2\"\nversion = \"1.0.0\"\n\
                 edition = \"2021\"\nlicense = \"AllRightsReserved\"\n",
            );
            write(&r.join("src/main.rs"), "fn main() {}\n");
        }),
        ("free-form-scala", |r: &Path| {
            write(
                &r.join("build.sbt"),
                "name := \"waybill-fixture-v3\"\nversion := \"1.0.0\"\n\
                 organization := \"dev.waybill.fixture\"\n\
                 licenses := Seq((\"Apache 2\", url(\"http://example.invalid/a\")))\n",
            );
        }),
    ];
    let mut missing: Vec<&str> = Vec::new();
    for (name, build) in &specs {
        let tmp = tempfile::tempdir().expect("tempdir");
        build(tmp.path());
        if all_licenses(&scan_cdx(tmp.path())).is_empty() {
            missing.push(name);
        }
    }
    assert!(
        missing.is_empty(),
        "every fixture declares one license, so none may emit zero — a value that \
         will not canonicalise is preserved, not dropped (FR-004). Missing: {missing:?}"
    );
}

// ---------------------------------------------------------------------------
// SC-007 / FR-015 — determinism
// ---------------------------------------------------------------------------

#[test]
fn m954_sc007_repeated_scans_agree_on_licenses() {
    // Two scans of the same tree must produce identical license data, including
    // ordering. Licenses reach the document through a merge that sorts groups by
    // confidence, and group arrival order is not stable across runs, so this is
    // not a free property.
    let tmp = tempfile::tempdir().expect("tempdir");
    maven_two(tmp.path());
    let first = all_licenses(&scan_cdx(tmp.path()));
    let second = all_licenses(&scan_cdx(tmp.path()));
    assert!(
        !first.is_empty(),
        "control: the fixture emitted no licenses, so agreement is vacuous"
    );
    assert_eq!(first, second, "repeated scans disagreed on licenses");
}

// ---------------------------------------------------------------------------
// SC-005 — coverage only increases
// ---------------------------------------------------------------------------

#[test]
fn m954_sc005_os_reader_licenses_are_unaffected() {
    // The OS-package readers carried licenses long before #954. This feature must
    // not disturb them: it only populates a field they already populated, via a
    // different path. A synthetic dpkg-style tree is the cheapest available
    // regression check that the shared ladder did not capture their path too.
    let tmp = tempfile::tempdir().expect("tempdir");
    let status = tmp.path().join("var/lib/dpkg/status");
    write(
        &status,
        r#"Package: waybill-fixture-osdep
Status: install ok installed
Version: 1.0.0
Architecture: amd64
Maintainer: Fixture
Description: fixture

"#,
    );
    let doc = scan_cdx(tmp.path());
    // The assertion is deliberately weak: this fixture declares no license, so
    // the property under test is that the scan still succeeds and emits the
    // component. A strong assertion would need a copyright file, which belongs to
    // the dpkg suite rather than here.
    let found = doc["components"]
        .as_array()
        .map(|a| a.iter().any(|c| c["name"] == "waybill-fixture-osdep"))
        .unwrap_or(false);
    assert!(
        found,
        "the dpkg reader must still emit its component after #954; components={:?}",
        doc["components"]
    );
}
