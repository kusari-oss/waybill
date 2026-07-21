//! Milestone 141 US3 — design-tier + Q3 keyword family + umbrella.
//!
//! Covers SC-003 (design-tier from rebar.config only), SC-007
//! (profile-scoped dev-deps + --exclude-scope dev), SC-009 (umbrella
//! per-sub-app main-modules), SC-010 (Q3 keyword family discrimination).

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

fn all_components(doc: &Value) -> Vec<&Value> {
    let mut out: Vec<&Value> = Vec::new();
    if let Some(arr) = doc.get("components").and_then(|v| v.as_array()) {
        out.extend(arr.iter());
    }
    if let Some(c) = doc.get("metadata").and_then(|m| m.get("component")) {
        out.push(c);
    }
    out
}

fn property_value<'a>(component: &'a Value, name: &str) -> Option<&'a str> {
    component
        .get("properties")
        .and_then(|v| v.as_array())?
        .iter()
        .find(|p| p.get("name").and_then(|v| v.as_str()) == Some(name))
        .and_then(|p| p.get("value").and_then(|v| v.as_str()))
}

fn component_with_name<'a>(doc: &'a Value, name: &str) -> Option<&'a Value> {
    all_components(doc)
        .into_iter()
        .find(|c| c.get("name").and_then(|v| v.as_str()) == Some(name))
}

#[test]
fn sc003_design_tier_from_rebar_config_only() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("rebar.config"),
        r#"{deps, [
    {cowboy, "~> 2.10"},
    {jiffy, {pkg, jiffy, "~> 1.1"}}
]}."#,
    )
    .unwrap();
    // NO rebar.lock — triggers design-tier fallback per FR-005.
    let doc = run_scan(dir.path());
    let cowboy = component_with_name(&doc, "cowboy").expect("cowboy design-tier component");
    assert_eq!(
        property_value(cowboy, "waybill:sbom-tier"),
        Some("design"),
    );
    // requirement_range is emitted as the CDX-native field via the
    // builder. Check via property channel — waybill emits both.
    // (Builder writes requirement_range into a dedicated property too.)
    let jiffy = component_with_name(&doc, "jiffy").expect("jiffy design-tier component");
    assert_eq!(
        property_value(jiffy, "waybill:sbom-tier"),
        Some("design"),
    );
}

#[test]
fn sc007_profile_scoped_dev_dep_lifecycle_scope() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("rebar.config"),
        r#"{deps, []}.

{profiles, [
    {test, [{deps, [{meck, "~> 0.9"}]}]}
]}.
"#,
    )
    .unwrap();
    // NO lockfile — design-tier mode emits meck with lifecycle-scope=dev.
    let doc = run_scan(dir.path());
    let meck = component_with_name(&doc, "meck").expect("meck dev-profile component");
    // CDX native field: scope. Check via property channel (waybill
    // emits both — milestone 052 lifecycle-scope-as-property bridge).
    let scope = meck.get("scope").and_then(|v| v.as_str());
    // Per milestone 052 lifecycle-scope bridge: dev-scope maps to CDX
    // `scope = "excluded"` so consumers' "non-runtime" filters catch it.
    assert_eq!(
        scope,
        Some("excluded"),
        "test-profile deps must map to CDX scope='excluded' per the milestone-052 dev-scope native-field bridge",
    );
    // Confirm via component-level discrimination — meck should NOT
    // appear when --exclude-scope dev is passed.
    let doc_excluded =
        run_scan_with_flags(dir.path(), &["--exclude-scope", "dev"]);
    let meck_excluded = component_with_name(&doc_excluded, "meck");
    assert!(
        meck_excluded.is_none(),
        "meck must be suppressed under --exclude-scope dev per FR-008 + SC-007",
    );
}

#[test]
fn sc009_umbrella_three_subapps_emit_main_modules() {
    let dir = tempfile::tempdir().unwrap();
    // Shared root rebar.config + per-subapp *.app.src.
    std::fs::write(
        dir.path().join("rebar.config"),
        r#"{erl_opts, [debug_info]}.
{deps, []}.
"#,
    )
    .unwrap();
    let apps_dir = dir.path().join("apps");
    for app_name in &["my_app", "my_lib", "my_worker"] {
        let app_dir = apps_dir.join(app_name);
        let src_dir = app_dir.join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::write(
            src_dir.join(format!("{app_name}.app.src")),
            format!(
                r#"{{application, {app_name}, [
    {{vsn, "1.0.0"}},
    {{applications, [kernel, stdlib]}},
    {{description, "{app_name} umbrella member"}}
]}}."#
            ),
        )
        .unwrap();
    }
    let doc = run_scan(dir.path());
    // Each sub-app emits its own main-module PURL.
    for app_name in &["my_app", "my_lib", "my_worker"] {
        let purl = format!("pkg:hex/{app_name}@1.0.0");
        let found = all_components(&doc)
            .into_iter()
            .any(|c| c.get("purl").and_then(|v| v.as_str()) == Some(purl.as_str()));
        assert!(found, "expected main-module {purl} per FR-009/SC-009");
    }
    // Same-PURL OTP runtime deps (kernel, stdlib) collapse via dedup.
    let kernel_count = all_components(&doc)
        .into_iter()
        .filter(|c| c.get("purl").and_then(|v| v.as_str()) == Some("pkg:generic/kernel@unspecified"))
        .count();
    assert_eq!(
        kernel_count, 1,
        "kernel should dedupe across umbrella sub-apps to a single component entry",
    );
}

#[test]
fn sc010_q3_keyword_family_discrimination() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("rebar.config"),
        r#"{deps, []}.
"#,
    )
    .unwrap();
    let src_dir = dir.path().join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::write(
        src_dir.join("my_app.app.src"),
        r#"{application, my_app, [
    {vsn, "1.0.0"},
    {applications, [kernel, stdlib, cowboy]},
    {included_applications, [config_app]},
    {optional_applications, [telemetry]},
    {description, "Q3 keyword family fixture"}
]}."#,
    )
    .unwrap();
    let doc = run_scan(dir.path());
    // Q3: each edge-target carries waybill:erlang-app-dep-kind.
    let cowboy = component_with_name(&doc, "cowboy").expect("cowboy required-required");
    assert_eq!(
        property_value(cowboy, "waybill:erlang-app-dep-kind"),
        Some("required"),
    );
    let config_app =
        component_with_name(&doc, "config_app").expect("config_app included");
    assert_eq!(
        property_value(config_app, "waybill:erlang-app-dep-kind"),
        Some("included"),
    );
    let telemetry =
        component_with_name(&doc, "telemetry").expect("telemetry optional");
    assert_eq!(
        property_value(telemetry, "waybill:erlang-app-dep-kind"),
        Some("optional"),
    );
    // Operator filtering on optional retrieves exactly telemetry:
    let optionals: Vec<&str> = all_components(&doc)
        .into_iter()
        .filter(|c| property_value(c, "waybill:erlang-app-dep-kind") == Some("optional"))
        .filter_map(|c| c.get("name").and_then(|v| v.as_str()))
        .collect();
    assert_eq!(optionals, vec!["telemetry"]);
}

#[test]
fn sc010_q3_precedence_required_wins() {
    // Verifies the required > included > optional precedence when the
    // same atom appears in multiple keyword families across umbrella
    // sub-apps. Sub-app A has `cowboy` in optional; sub-app B has
    // `cowboy` in required. The component-level annotation must be
    // "required" per Q3 precedence.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("rebar.config"),
        r#"{deps, []}.
"#,
    )
    .unwrap();
    let apps_dir = dir.path().join("apps");
    let app_a_src = apps_dir.join("app_a/src");
    std::fs::create_dir_all(&app_a_src).unwrap();
    std::fs::write(
        app_a_src.join("app_a.app.src"),
        r#"{application, app_a, [
    {vsn, "1.0.0"},
    {applications, [kernel]},
    {optional_applications, [cowboy]},
    {description, "app_a"}
]}."#,
    )
    .unwrap();
    let app_b_src = apps_dir.join("app_b/src");
    std::fs::create_dir_all(&app_b_src).unwrap();
    std::fs::write(
        app_b_src.join("app_b.app.src"),
        r#"{application, app_b, [
    {vsn, "1.0.0"},
    {applications, [kernel, cowboy]},
    {description, "app_b"}
]}."#,
    )
    .unwrap();
    let doc = run_scan(dir.path());
    let cowboy = component_with_name(&doc, "cowboy")
        .expect("cowboy must emit (as OTP-runtime placeholder since no lockfile)");
    assert_eq!(
        property_value(cowboy, "waybill:erlang-app-dep-kind"),
        Some("required"),
        "Q3 precedence: required > optional when an atom appears in both",
    );
}
