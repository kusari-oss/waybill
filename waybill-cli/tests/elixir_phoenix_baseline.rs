//! Milestone 140 US1 — Phoenix baseline integration tests.
//!
//! Covers SC-001 (Phoenix baseline + lock count), SC-007 (dev-scope
//! filterability), SC-008 (main-module emission + dep edges), SC-009
//! (dual SHA-256 + pre-Hex-2.0 single hash per Q3).

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_mikebom")
}

fn run_scan_with_flags(project_root: &Path, extra: &[&str]) -> Value {
    let mut top_level_extra: Vec<&str> = Vec::new();
    let mut subcommand_extra: Vec<&str> = Vec::new();
    let mut iter = extra.iter().peekable();
    while let Some(a) = iter.next() {
        if *a == "--exclude-scope" {
            top_level_extra.push(a);
            if let Some(v) = iter.next() {
                top_level_extra.push(v);
            }
        } else {
            subcommand_extra.push(a);
        }
    }
    let out_dir = tempfile::tempdir().unwrap();
    let out_path = out_dir.path().join("out.cdx.json");
    let mut cmd = Command::new(binary_path());
    cmd.arg("--offline");
    for a in &top_level_extra {
        cmd.arg(a);
    }
    cmd.arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(project_root)
        .arg("--no-deep-hash")
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", out_path.display()));
    for a in &subcommand_extra {
        cmd.arg(a);
    }
    let result = cmd.output().unwrap();
    assert!(
        result.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&result.stderr),
    );
    let bytes = std::fs::read(&out_path).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn run_scan(project_root: &Path) -> Value {
    run_scan_with_flags(project_root, &[])
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
                                    == Some("mikebom:source-type")
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

fn write_phoenix_fixture(root: &Path) {
    let inner = "a".repeat(64);
    let outer = "b".repeat(64);
    std::fs::write(
        root.join("mix.exs"),
        r#"defmodule MyApp.MixProject do
  use Mix.Project

  def project do
    [
      app: :my_app,
      version: "0.5.2",
      elixir: "~> 1.16",
      deps: deps()
    ]
  end

  defp deps do
    [
      {:phoenix, "~> 1.7"},
      {:plug, "~> 1.15"},
      {:ecto, "~> 3.11"}
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
  "phoenix": {{:hex, :phoenix, "1.7.10", "{inner}", [:mix], [{{:plug, "~> 1.15", [hex: :plug, repo: "hexpm", optional: false]}}], "hexpm", "{outer}"}},
  "plug": {{:hex, :plug, "1.15.2", "{inner}", [:mix], [], "hexpm", "{outer}"}},
  "ecto": {{:hex, :ecto, "3.11.1", "{inner}", [:mix], [], "hexpm", "{outer}"}},
  "telemetry": {{:hex, :telemetry, "1.2.1", "{inner}", [:rebar3], [], "hexpm", "{outer}"}},
  "mime": {{:hex, :mime, "2.0.5", "{inner}", [:mix], [], "hexpm", "{outer}"}}
}}
"#
        ),
    )
    .unwrap();
}

#[test]
fn phoenix_baseline_emits_lock_count_plus_main_module() {
    // SC-001: 5 lockfile entries + 1 main-module = 6 hex-derived components.
    let tmp = tempfile::tempdir().unwrap();
    write_phoenix_fixture(tmp.path());
    let doc = run_scan(tmp.path());
    let purls = hex_purls(&doc);
    assert_eq!(
        purls.len(),
        6,
        "expected 6 hex components (5 lockfile + 1 main-module); got {purls:#?}"
    );
    for expected in &[
        "pkg:hex/phoenix@1.7.10",
        "pkg:hex/plug@1.15.2",
        "pkg:hex/ecto@3.11.1",
        "pkg:hex/telemetry@1.2.1",
        "pkg:hex/mime@2.0.5",
        "pkg:hex/my_app@0.5.2",
    ] {
        assert!(
            purls.contains(&expected.to_string()),
            "expected PURL {expected} not found; got {purls:#?}"
        );
    }
}

#[test]
fn main_module_from_mix_exs_app_keyword() {
    // SC-008: mix.exs `app: :my_app, version: "0.5.2"` produces
    // main-module PURL `pkg:hex/my_app@0.5.2`.
    let tmp = tempfile::tempdir().unwrap();
    write_phoenix_fixture(tmp.path());
    let doc = run_scan(tmp.path());
    let main = component_with_purl(&doc, "pkg:hex/my_app@0.5.2")
        .expect("main-module must exist");
    assert_eq!(
        property_value(main, "mikebom:component-role"),
        Some("main-module"),
    );
}

#[test]
fn main_module_depends_lists_direct_deps() {
    // SC-008 cont: dependencies[] for main-module bom-ref targets each
    // direct dep's bom-ref.
    let tmp = tempfile::tempdir().unwrap();
    write_phoenix_fixture(tmp.path());
    let doc = run_scan(tmp.path());
    let main = component_with_purl(&doc, "pkg:hex/my_app@0.5.2").unwrap();
    let main_ref = main.get("bom-ref").and_then(|v| v.as_str()).unwrap();
    let deps = doc.get("dependencies").and_then(|v| v.as_array()).unwrap();
    let main_deps = deps
        .iter()
        .find(|d| d.get("ref").and_then(|v| v.as_str()) == Some(main_ref))
        .and_then(|d| d.get("dependsOn").and_then(|v| v.as_array()))
        .expect("main-module dependencies entry must exist");
    let main_dep_refs: Vec<&str> =
        main_deps.iter().filter_map(|v| v.as_str()).collect();
    for direct_purl in &[
        "pkg:hex/phoenix@1.7.10",
        "pkg:hex/plug@1.15.2",
        "pkg:hex/ecto@3.11.1",
    ] {
        let direct = component_with_purl(&doc, direct_purl)
            .unwrap_or_else(|| panic!("direct dep {direct_purl} missing"));
        let direct_ref = direct.get("bom-ref").and_then(|v| v.as_str()).unwrap();
        assert!(
            main_dep_refs.contains(&direct_ref),
            "main-module dependsOn must include {direct_purl}; got {main_dep_refs:?}",
        );
    }
}

#[test]
fn inner_and_outer_sha256_emitted() {
    // SC-009 + Q3: hex entry with both inner + outer SHA-256 produces
    // TWO CDX hashes[] entries with alg = SHA-256 and distinct contents.
    let tmp = tempfile::tempdir().unwrap();
    write_phoenix_fixture(tmp.path());
    let doc = run_scan(tmp.path());
    let phoenix = component_with_purl(&doc, "pkg:hex/phoenix@1.7.10").unwrap();
    let hashes = phoenix
        .get("hashes")
        .and_then(|v| v.as_array())
        .expect("phoenix must carry hashes[] (FR-011)");
    let sha256_entries: Vec<&Value> = hashes
        .iter()
        .filter(|h| h.get("alg").and_then(|v| v.as_str()) == Some("SHA-256"))
        .collect();
    assert_eq!(
        sha256_entries.len(),
        2,
        "expected 2 SHA-256 hashes (inner + outer); got {hashes:#?}"
    );
    let contents: Vec<&str> = sha256_entries
        .iter()
        .filter_map(|h| h.get("content").and_then(|v| v.as_str()))
        .collect();
    assert_eq!(contents.len(), 2);
    assert_ne!(
        contents[0], contents[1],
        "inner + outer SHA-256 contents must differ"
    );
}

#[test]
fn pre_hex_2_entry_emits_only_inner_sha256() {
    // Q3 edge: lockfile entry with only inner SHA-256 (no outer)
    // emits ONE hash entry per Principle IX accuracy.
    let tmp = tempfile::tempdir().unwrap();
    let inner = "a".repeat(64);
    std::fs::write(
        tmp.path().join("mix.exs"),
        r#"defmodule X.MixProject do
  def project, do: [app: :x, version: "0.1.0", deps: deps()]
  defp deps, do: [{:phoenix, "~> 1.7"}]
end
"#,
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("mix.lock"),
        format!(
            r#"%{{
  "phoenix": {{:hex, :phoenix, "1.7.10", "{inner}", [:mix], [], "hexpm"}}
}}
"#
        ),
    )
    .unwrap();
    let doc = run_scan(tmp.path());
    let phoenix = component_with_purl(&doc, "pkg:hex/phoenix@1.7.10").unwrap();
    let hashes = phoenix
        .get("hashes")
        .and_then(|v| v.as_array())
        .expect("phoenix must carry hashes[]");
    let sha256_count = hashes
        .iter()
        .filter(|h| h.get("alg").and_then(|v| v.as_str()) == Some("SHA-256"))
        .count();
    assert_eq!(
        sha256_count, 1,
        "pre-Hex-2.0 entry must emit ONE hash (inner only); got {hashes:#?}"
    );
}

#[test]
fn dev_scope_filterability() {
    // SC-007: mix.lock entry whose mix.exs deps/0 declares
    // `only: [:dev, :test]` carries mikebom:lifecycle-scope=development;
    // --exclude-scope dev suppresses (top-level flag BEFORE sbom subcommand).
    let tmp = tempfile::tempdir().unwrap();
    let inner = "a".repeat(64);
    std::fs::write(
        tmp.path().join("mix.exs"),
        r#"defmodule X.MixProject do
  def project, do: [app: :x, version: "0.1.0", deps: deps()]
  defp deps do
    [
      {:phoenix, "~> 1.7"},
      {:credo, "~> 1.7", only: [:dev, :test], runtime: false}
    ]
  end
end
"#,
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("mix.lock"),
        format!(
            r#"%{{
  "phoenix": {{:hex, :phoenix, "1.7.10", "{inner}", [:mix], [], "hexpm"}},
  "credo": {{:hex, :credo, "1.7.0", "{inner}", [:mix], [], "hexpm"}}
}}
"#
        ),
    )
    .unwrap();

    let doc_with_dev = run_scan(tmp.path());
    let credo = component_with_purl(&doc_with_dev, "pkg:hex/credo@1.7.0")
        .expect("credo must emit by default");
    let lifecycle = property_value(credo, "mikebom:lifecycle-scope");
    let cdx_scope = credo.get("scope").and_then(|v| v.as_str());
    assert!(
        lifecycle == Some("development")
            || matches!(cdx_scope, Some("excluded") | Some("optional")),
        "credo must carry development indicator; got property={lifecycle:?} scope={cdx_scope:?}",
    );

    let doc_without_dev = run_scan_with_flags(tmp.path(), &["--exclude-scope", "dev"]);
    assert!(
        component_with_purl(&doc_without_dev, "pkg:hex/credo@1.7.0").is_none(),
        "--exclude-scope dev must suppress credo",
    );
    assert!(
        component_with_purl(&doc_without_dev, "pkg:hex/phoenix@1.7.10").is_some(),
        "--exclude-scope dev must NOT suppress phoenix",
    );
}
