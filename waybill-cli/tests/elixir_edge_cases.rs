//! Milestone 140 polish — edge cases.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

fn run_scan(project_root: &Path) -> (Value, String) {
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
    let doc: Value = serde_json::from_slice(&bytes).unwrap();
    (doc, String::from_utf8_lossy(&result.stderr).into_owned())
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

fn hex_purls(doc: &Value) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut visit = |c: &Value| {
        if let Some(p) = c.get("purl").and_then(|v| v.as_str()) {
            if p.starts_with("pkg:hex/")
                || (p.starts_with("pkg:generic/")
                    && c.get("properties")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter().any(|p| {
                                p.get("name").and_then(|v| v.as_str())
                                    == Some("waybill:source-type")
                                    && p.get("value")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.starts_with("hex-"))
                                        .unwrap_or(false)
                            })
                        })
                        .unwrap_or(false))
            {
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

#[test]
fn malformed_mix_lock_falls_back_to_design_tier() {
    // SC-005: malformed mix.lock + sibling mix.exs → design-tier
    // components emit + warning fires.
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("mix.exs"),
        r#"defmodule X.MixProject do
  def project, do: [app: :x, version: "0.1.0", deps: deps()]
  defp deps, do: [{:phoenix, "~> 1.7"}]
end
"#,
    )
    .unwrap();
    std::fs::write(tmp.path().join("mix.lock"), "this is not valid elixir syntax }}}}").unwrap();
    let (doc, _stderr) = run_scan(tmp.path());
    // Design-tier phoenix emits.
    assert!(
        component_with_purl(&doc, "pkg:hex/phoenix@~>_1.7").is_some(),
        "design-tier phoenix must emit when mix.lock is malformed; got purls: {:?}",
        hex_purls(&doc)
    );
}

#[test]
fn multi_line_tuple_in_mix_lock_parses_correctly() {
    // Multi-line :hex tuple — `:deps` element wraps across lines.
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
  "phoenix": {{:hex, :phoenix, "1.7.10", "{inner}", [:mix], [
    {{:plug, "~> 1.15", [hex: :plug, repo: "hexpm", optional: false]}},
    {{:telemetry, "~> 1.2", [hex: :telemetry, repo: "hexpm", optional: false]}}
  ], "hexpm"}}
}}
"#
        ),
    )
    .unwrap();
    let (doc, _) = run_scan(tmp.path());
    assert!(
        component_with_purl(&doc, "pkg:hex/phoenix@1.7.10").is_some(),
        "multi-line :hex tuple must parse correctly via brace-counting",
    );
}

#[test]
fn private_org_lowercased_in_purl_namespace() {
    // Phase 0 + IX accuracy: private-org slug lowercased per purl-spec
    // hex canonical form.
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
  "my_lib": {{:hex, :my_lib, "2.0.0", "{inner}", [:mix], [], "hexpm:ACME"}}
}}
"#
        ),
    )
    .unwrap();
    let (doc, _) = run_scan(tmp.path());
    assert!(
        component_with_purl(
            &doc,
            "pkg:hex/acme/my_lib@2.0.0?repository_url=https://repo.hex.pm"
        )
        .is_some(),
        "private-org slug ACME must lowercase to acme",
    );
}

#[test]
fn unknown_source_atom_skipped_via_unparseable_entry() {
    // Lockfile with unknown atom discriminator → entry skipped; other
    // valid entries still emit.
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
  "valid": {{:hex, :valid, "1.0.0", "{inner}", [:mix], [], "hexpm"}},
  "weird": {{:hg, "https://example.com/r", "abc123", []}}
}}
"#
        ),
    )
    .unwrap();
    let (doc, _) = run_scan(tmp.path());
    assert!(
        component_with_purl(&doc, "pkg:hex/valid@1.0.0").is_some(),
        "valid entry must still emit when sibling unknown-source entry is present",
    );
    let purls = hex_purls(&doc);
    assert!(
        !purls.iter().any(|p| p.contains("weird")),
        "unknown-source entry must NOT emit; got {purls:?}",
    );
}

#[test]
fn apps_path_with_custom_value_still_detected_as_umbrella() {
    // R4: `apps_path:` KEY-presence detection (value-ignored).
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("my_umbrella");
    std::fs::create_dir_all(root.join("modules").join("core")).unwrap();
    std::fs::write(
        root.join("mix.exs"),
        r#"defmodule MyUmbrella.MixProject do
  def project, do: [app: :my_umbrella, version: "0.1.0", apps_path: "modules"]
end
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("modules").join("core").join("mix.exs"),
        r#"defmodule Core.MixProject do
  def project, do: [app: :core, version: "0.1.0"]
end
"#,
    )
    .unwrap();
    let (doc, _) = run_scan(root.as_path());
    let umbrella = component_with_purl(&doc, "pkg:hex/my_umbrella@0.1.0")
        .expect("custom apps_path value must still detect umbrella");
    let props = umbrella.get("properties").and_then(|v| v.as_array()).unwrap();
    let has_umbrella_root_annotation = props.iter().any(|p| {
        p.get("name").and_then(|v| v.as_str()) == Some("waybill:umbrella-root")
            && p.get("value").and_then(|v| v.as_str()) == Some("true")
    });
    assert!(
        has_umbrella_root_annotation,
        "R4: apps_path KEY presence (not value) must trigger umbrella detection; got props: {props:#?}",
    );
}

#[test]
fn outer_sha256_empty_string_treated_as_absent() {
    // Q3 edge: lockfile entry with empty-string outer SHA-256 ("")
    // emits ONE hash (inner only), NOT two with one empty content.
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
  "phoenix": {{:hex, :phoenix, "1.7.10", "{inner}", [:mix], [], "hexpm", ""}}
}}
"#
        ),
    )
    .unwrap();
    let (doc, _) = run_scan(tmp.path());
    let phx = component_with_purl(&doc, "pkg:hex/phoenix@1.7.10").unwrap();
    let hashes = phx.get("hashes").and_then(|v| v.as_array()).unwrap();
    let count = hashes
        .iter()
        .filter(|h| h.get("alg").and_then(|v| v.as_str()) == Some("SHA-256"))
        .count();
    assert_eq!(
        count, 1,
        "empty-string outer SHA-256 must be skipped per Q3 — only inner emits",
    );
}
