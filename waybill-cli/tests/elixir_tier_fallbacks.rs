//! Milestone 140 US3 — design-tier + Q1 conditional-flattened +
//! Q2 umbrella aggregation + C2 source-kind dispatch.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
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

fn hex_components(doc: &Value) -> Vec<&Value> {
    doc.get("components")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|c| {
                    let purl = c.get("purl").and_then(|v| v.as_str()).unwrap_or("");
                    let role = property_value(c, "waybill:component-role");
                    if role == Some("main-module") {
                        return false;
                    }
                    purl.starts_with("pkg:hex/")
                        || (purl.starts_with("pkg:generic/")
                            && property_value(c, "waybill:source-type")
                                .map(|s| s.starts_with("hex-"))
                                .unwrap_or(false))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn property_value<'a>(component: &'a Value, name: &str) -> Option<&'a str> {
    component
        .get("properties")
        .and_then(|v| v.as_array())?
        .iter()
        .find(|p| p.get("name").and_then(|v| v.as_str()) == Some(name))
        .and_then(|p| p.get("value").and_then(|v| v.as_str()))
}

fn find_by_name<'a>(components: &'a [&'a Value], name: &str) -> Option<&'a Value> {
    components
        .iter()
        .copied()
        .find(|c| c.get("name").and_then(|v| v.as_str()) == Some(name))
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

#[test]
fn design_tier_mix_exs_only_emits_constraints() {
    // SC-003: mix.exs only → 2 components with sbom-tier=design +
    // requirement-range preserved.
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("mix.exs"),
        r#"defmodule X.MixProject do
  def project, do: [app: :my_lib, version: "0.1.0", deps: deps()]
  defp deps do
    [
      {:phoenix, "~> 1.7"},
      {:plug, "~> 1.15"}
    ]
  end
end
"#,
    )
    .unwrap();
    let doc = run_scan(tmp.path());
    let comps = hex_components(&doc);
    assert_eq!(comps.len(), 2);
    for &c in &comps {
        assert_eq!(property_value(c, "waybill:sbom-tier"), Some("design"));
    }
    let phx = find_by_name(&comps, "phoenix").unwrap();
    // Milestone 199: always-array shape — JSON-array-in-string value.
    assert_eq!(
        property_value(phx, "waybill:requirement-ranges"),
        Some(r#"["~> 1.7"]"#),
    );
}

#[test]
fn design_tier_no_transitive_deps() {
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
    let doc = run_scan(tmp.path());
    let comps = hex_components(&doc);
    let names: Vec<&str> = comps
        .iter()
        .filter_map(|c| c.get("name").and_then(|v| v.as_str()))
        .collect();
    assert!(names.contains(&"phoenix"));
    assert!(
        !names.contains(&"plug"),
        "design-tier must NOT emit transitive plug; got {names:?}",
    );
}

#[test]
fn conditional_block_dep_carries_extraction_mode_annotation() {
    // Q1: `if Mix.env() == :test do {:meck, "~> 0.9"} end` emits with
    // waybill:elixir-extraction-mode = "conditional-flattened".
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("mix.exs"),
        r#"defmodule X.MixProject do
  def project, do: [app: :x, version: "0.1.0", deps: deps()]
  defp deps do
    [
      {:phoenix, "~> 1.7"},
      if Mix.env() == :test do
        {:meck, "~> 0.9"}
      end
    ]
  end
end
"#,
    )
    .unwrap();
    let doc = run_scan(tmp.path());
    let comps = hex_components(&doc);
    let meck = find_by_name(&comps, "meck")
        .expect("meck must emit (conditional-flattened per Q1)");
    assert_eq!(
        property_value(meck, "waybill:elixir-extraction-mode"),
        Some("conditional-flattened"),
    );
}

#[test]
fn unconditional_dep_does_not_carry_extraction_mode_annotation() {
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
    let doc = run_scan(tmp.path());
    let comps = hex_components(&doc);
    let phx = find_by_name(&comps, "phoenix").unwrap();
    assert!(
        property_value(phx, "waybill:elixir-extraction-mode").is_none(),
        "top-level dep must NOT carry conditional-flattened annotation",
    );
}

#[test]
fn umbrella_root_carries_umbrella_root_annotation() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("my_umbrella");
    let apps_core = root.join("apps").join("core");
    std::fs::create_dir_all(&apps_core).unwrap();
    std::fs::write(
        root.join("mix.exs"),
        r#"defmodule MyUmbrella.MixProject do
  def project, do: [app: :my_umbrella, version: "0.1.0", apps_path: "apps", deps: deps()]
  defp deps, do: [{:dialyxir, "~> 1.4", only: [:dev], runtime: false}]
end
"#,
    )
    .unwrap();
    std::fs::write(
        apps_core.join("mix.exs"),
        r#"defmodule Core.MixProject do
  def project, do: [app: :core, version: "0.1.0", deps: deps()]
  defp deps, do: []
end
"#,
    )
    .unwrap();

    let doc = run_scan(root.as_path());
    let umbrella = component_with_purl(&doc, "pkg:hex/my_umbrella@0.1.0")
        .expect("umbrella root main-module must emit");
    assert_eq!(
        property_value(umbrella, "waybill:umbrella-root"),
        Some("true"),
    );

    // Sub-app main-module does NOT carry umbrella-root annotation.
    let core = component_with_purl(&doc, "pkg:hex/core@0.1.0")
        .expect("sub-app core main-module must emit");
    assert!(
        property_value(core, "waybill:umbrella-root").is_none(),
        "sub-app must NOT carry umbrella-root annotation",
    );
}

#[test]
fn umbrella_root_dependencies_targets_sub_app_main_modules() {
    // Q2 + SC-010 + I1: umbrella root's dependsOn (post-orchestrator
    // name→bom-ref resolution) contains each sub-app's main-module
    // bom-ref. PackageDbEntry.depends carries NAMES per I1; orchestrator
    // resolves to bom-refs at dep-edge wiring.
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("my_umbrella");
    std::fs::create_dir_all(root.join("apps").join("core")).unwrap();
    std::fs::create_dir_all(root.join("apps").join("web")).unwrap();
    std::fs::create_dir_all(root.join("apps").join("worker")).unwrap();
    std::fs::write(
        root.join("mix.exs"),
        r#"defmodule MyUmbrella.MixProject do
  def project, do: [app: :my_umbrella, version: "0.1.0", apps_path: "apps", deps: deps()]
  defp deps, do: []
end
"#,
    )
    .unwrap();
    for sub in &["core", "web", "worker"] {
        let sub_dir = root.join("apps").join(sub);
        std::fs::write(
            sub_dir.join("mix.exs"),
            format!(
                r#"defmodule {cap}.MixProject do
  def project, do: [app: :{sub}, version: "0.1.0", deps: deps()]
  defp deps, do: []
end
"#,
                cap = sub.chars().next().unwrap().to_uppercase().collect::<String>() + &sub[1..],
                sub = sub,
            ),
        )
        .unwrap();
    }

    let doc = run_scan(root.as_path());
    // 4 main-modules: root + 3 sub-apps.
    for expected in &[
        "pkg:hex/my_umbrella@0.1.0",
        "pkg:hex/core@0.1.0",
        "pkg:hex/web@0.1.0",
        "pkg:hex/worker@0.1.0",
    ] {
        assert!(
            component_with_purl(&doc, expected).is_some(),
            "expected main-module {expected} not found",
        );
    }

    // Umbrella root's dependsOn includes each sub-app's bom-ref (after
    // name→bom-ref resolution per I1).
    let umbrella = component_with_purl(&doc, "pkg:hex/my_umbrella@0.1.0").unwrap();
    let umbrella_ref = umbrella.get("bom-ref").and_then(|v| v.as_str()).unwrap();
    let deps = doc.get("dependencies").and_then(|v| v.as_array()).unwrap();
    let umbrella_deps = deps
        .iter()
        .find(|d| d.get("ref").and_then(|v| v.as_str()) == Some(umbrella_ref))
        .and_then(|d| d.get("dependsOn").and_then(|v| v.as_array()))
        .expect("umbrella root must have a dependencies entry");
    let dep_refs: Vec<&str> = umbrella_deps.iter().filter_map(|v| v.as_str()).collect();

    for sub_purl in &["pkg:hex/core@0.1.0", "pkg:hex/web@0.1.0", "pkg:hex/worker@0.1.0"] {
        let sub = component_with_purl(&doc, sub_purl).unwrap();
        let sub_ref = sub.get("bom-ref").and_then(|v| v.as_str()).unwrap();
        assert!(
            dep_refs.contains(&sub_ref),
            "umbrella root dependsOn must include {sub_purl}; got {dep_refs:?}",
        );
    }
}

#[test]
fn design_tier_git_declared_dep_emits_pkg_generic() {
    // C2 remediation: `{:my_fork, git: "https://...", branch: "main"}`
    // in design-tier mode emits pkg:generic/, not pkg:hex/.
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("mix.exs"),
        r#"defmodule X.MixProject do
  def project, do: [app: :x, version: "0.1.0", deps: deps()]
  defp deps do
    [{:my_fork, git: "https://github.com/foo/my-fork.git", branch: "main"}]
  end
end
"#,
    )
    .unwrap();
    let doc = run_scan(tmp.path());
    let expected = "pkg:generic/my_fork@unspecified?vcs_url=git+https://github.com/foo/my-fork.git";
    let c = component_with_purl(&doc, expected).unwrap_or_else(|| {
        panic!(
            "design-tier git dep must emit pkg:generic/ form per C2; got purls: {:?}",
            doc.get("components")
                .and_then(|v| v.as_array())
                .map(|a| a
                    .iter()
                    .filter_map(|c| c.get("purl").and_then(|v| v.as_str()).map(String::from))
                    .collect::<Vec<_>>())
        )
    });
    assert_eq!(property_value(c, "waybill:source-type"), Some("hex-git"));
    assert_eq!(
        property_value(c, "waybill:vcs-declared-ref"),
        Some("branch: main"),
    );
}

#[test]
fn design_tier_path_declared_dep_emits_pkg_generic() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("mix.exs"),
        r#"defmodule X.MixProject do
  def project, do: [app: :x, version: "0.1.0", deps: deps()]
  defp deps, do: [{:shared, path: "../shared"}]
end
"#,
    )
    .unwrap();
    let doc = run_scan(tmp.path());
    let c = component_with_purl(&doc, "pkg:generic/shared@unspecified")
        .expect("design-tier path dep must emit pkg:generic/ form per C2");
    assert_eq!(property_value(c, "waybill:source-type"), Some("hex-path"));
    assert_eq!(property_value(c, "waybill:path"), Some("../shared"));
}

#[test]
fn design_tier_github_shortcut_expanded_to_git_url() {
    // C2 + R3: `github: "owner/repo"` shortcut expands to
    // `git: "https://github.com/owner/repo.git"`.
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("mix.exs"),
        r#"defmodule X.MixProject do
  def project, do: [app: :x, version: "0.1.0", deps: deps()]
  defp deps, do: [{:foo, github: "owner/repo"}]
end
"#,
    )
    .unwrap();
    let doc = run_scan(tmp.path());
    let expected = "pkg:generic/foo@unspecified?vcs_url=git+https://github.com/owner/repo.git";
    assert!(
        component_with_purl(&doc, expected).is_some(),
        ":github shortcut must expand to git URL per R3",
    );
}
