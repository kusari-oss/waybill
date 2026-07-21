//! Milestone 140 US2 — source-discriminator distinction tests +
//! Phase 0 correction regressions.
//!
//! Covers SC-002: hex (default + private-org) + git + path source
//! types emit with correct purl-spec-conformant PURLs.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_mikebom")
}

fn run_scan(project_root: &Path) -> Value {
    let out_dir = tempfile::tempdir().unwrap();
    let out_path = out_dir.path().join("out.cdx.json");
    let result = Command::new(binary_path())
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(project_root)
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

fn component_with_purl<'a>(doc: &'a Value, purl: &str) -> Option<&'a Value> {
    if let Some(c) = doc.get("metadata").and_then(|m| m.get("component")) {
        if c.get("purl").and_then(|v| v.as_str()) == Some(purl) {
            return Some(c);
        }
    }
    doc.get("components")
        .and_then(|v| v.as_array())
        .and_then(|arr| {
            arr.iter().find(|c| {
                c.get("purl").and_then(|v| v.as_str()) == Some(purl)
            })
        })
}

fn property_value<'a>(component: &'a Value, name: &str) -> Option<&'a str> {
    component
        .get("properties")
        .and_then(|v| v.as_array())?
        .iter()
        .find(|p| p.get("name").and_then(|v| v.as_str()) == Some(name))
        .and_then(|p| p.get("value").and_then(|v| v.as_str()))
}

fn write_mixed_fixture(root: &Path) {
    let inner = "a".repeat(64);
    let resolved = "eb39649a76b87e8451baf75d10ce82ca3a3d5601";
    std::fs::write(
        root.join("mix.exs"),
        r#"defmodule X.MixProject do
  def project, do: [app: :x, version: "0.1.0", deps: deps()]
  defp deps do
    [
      {:phoenix, "~> 1.7"},
      {:internal_lib, "~> 2.0", organization: "acme"},
      {:my_fork, git: "https://github.com/foo/my-fork.git", ref: "main"},
      {:shared_lib, path: "apps/shared_lib"}
    ]
  end
end
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("mix.lock"),
        format!(
            r#"%{{
  "phoenix": {{:hex, :phoenix, "1.7.10", "{inner}", [:mix], [], "hexpm"}},
  "internal_lib": {{:hex, :internal_lib, "2.0.0", "{inner}", [:mix], [], "hexpm:acme"}},
  "my_fork": {{:git, "https://github.com/foo/my-fork.git", "{resolved}", [ref: "main"]}},
  "shared_lib": {{:path, "apps/shared_lib", []}}
}}
"#
        ),
    )
    .unwrap();
}

#[test]
fn default_hexpm_emits_bare_purl() {
    let tmp = tempfile::tempdir().unwrap();
    write_mixed_fixture(tmp.path());
    let doc = run_scan(tmp.path());
    let c = component_with_purl(&doc, "pkg:hex/phoenix@1.7.10")
        .expect("default hexpm must emit bare PURL");
    assert_eq!(property_value(c, "waybill:source-type"), Some("hex-hex"));
}

#[test]
fn private_hexpm_org_emits_namespace_and_repository_url() {
    // Phase 0 correction regression: lockfile entry with "hexpm:acme"
    // repo string emits pkg:hex/acme/internal_lib@2.0.0?repository_url=...
    // NOT a waybill:hex-repo annotation.
    let tmp = tempfile::tempdir().unwrap();
    write_mixed_fixture(tmp.path());
    let doc = run_scan(tmp.path());
    let expected =
        "pkg:hex/acme/internal_lib@2.0.0?repository_url=https://repo.hex.pm";
    let c = component_with_purl(&doc, expected)
        .unwrap_or_else(|| panic!("private-org PURL {expected} not found"));
    assert_eq!(property_value(c, "waybill:source-type"), Some("hex-hex"));
    // Should NOT have the deprecated waybill:hex-repo annotation
    // (verifies the initial spec-guess form was removed).
    assert!(
        property_value(c, "waybill:hex-repo").is_none(),
        "Phase 0 correction: waybill:hex-repo annotation must NOT appear; private-org info goes in PURL namespace + repository_url qualifier",
    );
}

#[test]
fn git_source_emits_pkg_generic_with_vcs_url() {
    // Phase 0 correction regression: :git lockfile entry emits
    // pkg:generic/<name>@<sha>?vcs_url=git+<url>, NOT pkg:hex/...?vcs_url=.
    let tmp = tempfile::tempdir().unwrap();
    write_mixed_fixture(tmp.path());
    let doc = run_scan(tmp.path());
    let expected = "pkg:generic/my_fork@eb39649a76b87e8451baf75d10ce82ca3a3d5601?vcs_url=git+https://github.com/foo/my-fork.git";
    let c = component_with_purl(&doc, expected)
        .unwrap_or_else(|| panic!("git-source PURL {expected} not found"));
    assert_eq!(property_value(c, "waybill:source-type"), Some("hex-git"));
}

#[test]
fn git_source_carries_vcs_declared_ref() {
    let tmp = tempfile::tempdir().unwrap();
    write_mixed_fixture(tmp.path());
    let doc = run_scan(tmp.path());
    let c = component_with_purl(
        &doc,
        "pkg:generic/my_fork@eb39649a76b87e8451baf75d10ce82ca3a3d5601?vcs_url=git+https://github.com/foo/my-fork.git",
    )
    .unwrap();
    assert_eq!(
        property_value(c, "waybill:vcs-declared-ref"),
        Some("ref: main"),
    );
}

#[test]
fn path_source_emits_pkg_generic_placeholder() {
    let tmp = tempfile::tempdir().unwrap();
    write_mixed_fixture(tmp.path());
    let doc = run_scan(tmp.path());
    let c = component_with_purl(&doc, "pkg:generic/shared_lib@unspecified")
        .expect("path source must use pkg:generic/ placeholder");
    assert_eq!(property_value(c, "waybill:source-type"), Some("hex-path"));
    assert_eq!(property_value(c, "waybill:path"), Some("apps/shared_lib"));
}

#[test]
fn path_in_umbrella_carries_in_umbrella_annotation() {
    let tmp = tempfile::tempdir().unwrap();
    let inner = "a".repeat(64);
    std::fs::write(
        tmp.path().join("mix.exs"),
        r#"defmodule X.MixProject do
  def project, do: [app: :x, version: "0.1.0", deps: deps()]
  defp deps, do: [{:core, in_umbrella: true}]
end
"#,
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("mix.lock"),
        format!(
            r#"%{{
  "core": {{:path, "apps/core", [in_umbrella: true]}},
  "phoenix": {{:hex, :phoenix, "1.7.10", "{inner}", [:mix], [], "hexpm"}}
}}
"#
        ),
    )
    .unwrap();
    let doc = run_scan(tmp.path());
    let c = component_with_purl(&doc, "pkg:generic/core@unspecified").unwrap();
    assert_eq!(property_value(c, "waybill:in-umbrella"), Some("true"));
}

#[test]
fn hex_name_lowercased_in_purl() {
    // purl-spec canonical form: hex names lowercased. Hex.pm enforces
    // at publish time so this is typically no-op, but the rule applies.
    let tmp = tempfile::tempdir().unwrap();
    let inner = "a".repeat(64);
    std::fs::write(
        tmp.path().join("mix.exs"),
        r#"defmodule X.MixProject do
  def project, do: [app: :x, version: "0.1.0"]
end
"#,
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("mix.lock"),
        format!(
            r#"%{{
  "MixedCase": {{:hex, :MixedCase, "1.0.0", "{inner}", [:mix], [], "hexpm"}}
}}
"#
        ),
    )
    .unwrap();
    let doc = run_scan(tmp.path());
    assert!(
        component_with_purl(&doc, "pkg:hex/mixedcase@1.0.0").is_some(),
        "Mixed-case name MixedCase must lowercase to mixedcase per purl-spec",
    );
}
