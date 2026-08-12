//! Integration tests for milestone 233 (Go per-main-module `dependsOn` scoping).
//!
//! Reuses the m230 `nuget_main_module_parity.rs` + m231
//! `golang_workspace_mode_preflight.rs` subprocess scaffolds verbatim.
//! Every test spawns `waybill sbom scan --offline` against the
//! 4-module fixture at `waybill-cli/tests/fixtures/golden_inputs/
//! golang/per_mainmod_scope_4modules/` and asserts the emitted CDX
//! shape.
//!
//! Pre-233 baseline verified 2026-08-11: root's `dependsOn` contained
//! `x/text@v0.25.0` (from deep/src/thing/) instead of its own declared
//! `v0.40.0`. Post-233: each main-module's edge set matches its own
//! `go.mod` + `go.sum` declaration; no cross-main-module edges.

use std::path::PathBuf;
use std::process::Command;

mod common;
use common::bin;
use common::normalize::apply_fake_home_env;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("golden_inputs")
        .join("golang")
        .join("per_mainmod_scope_4modules")
}

fn run_scan(project_discovery: &str) -> serde_json::Value {
    let workdir = tempfile::tempdir().expect("workdir tempdir");
    let fake_home = tempfile::tempdir().expect("fake-home tempdir");
    let out_path = workdir.path().join("sbom.cdx.json");

    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home.path());
    cmd.env("WAYBILL_FIXED_TIMESTAMP", "2026-01-01T00:00:00Z");
    let pd_flag = format!("--project-discovery={}", project_discovery);
    cmd.args([
        "--offline",
        "sbom",
        "scan",
        "--path",
        fixture().to_str().unwrap(),
        &pd_flag,
        "--format",
        "cyclonedx-json",
        "--output",
        out_path.to_str().unwrap(),
        "--no-deep-hash",
    ]);
    let output = cmd.output().expect("spawn waybill");
    assert!(
        output.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );
    let bytes = std::fs::read(&out_path).expect("read emitted SBOM");
    serde_json::from_slice(&bytes).expect("parse JSON")
}

/// Extract every dependency edge whose source is a Go main-module of
/// name `mainmod_name`. Returns the sorted list of `dependsOn` target
/// PURLs. Handles both metadata.component subject and components[] entries.
fn depends_of_mainmod(json: &serde_json::Value, mainmod_name: &str) -> Vec<String> {
    let expected_ref = format!("pkg:golang/{}@v0.0.0-unknown", mainmod_name);
    let mut out: Vec<String> = json["dependencies"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter(|d| d["ref"].as_str() == Some(expected_ref.as_str()))
                .flat_map(|d| d["dependsOn"].as_array().cloned().unwrap_or_default())
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

// -----------------------------------------------------------
// SC-001 + FR-003 — per-mainmod dep matches own go.mod; 4 distinct
// x/text components emitted
// -----------------------------------------------------------

#[test]
fn per_mainmod_dep_matches_own_gomod_all_mode() {
    let json = run_scan("all");
    // Each main-module's x/text edge matches its own go.mod version.
    let root_deps = depends_of_mainmod(&json, "example.com/root");
    let hack_deps = depends_of_mainmod(&json, "example.com/hack");
    let tools_deps = depends_of_mainmod(&json, "example.com/tools");
    let deepthing_deps = depends_of_mainmod(&json, "example.com/deepthing");

    assert!(
        root_deps
            .iter()
            .any(|d| d == "pkg:golang/example.com/mikebomfixture/text@v0.40.0"),
        "root should point at v0.40.0; got {:?}",
        root_deps
    );
    assert!(
        !root_deps
            .iter()
            .any(|d| d.starts_with("pkg:golang/example.com/mikebomfixture/text@") && !d.ends_with("@v0.40.0")),
        "root MUST NOT point at any other x/text version; got {:?}",
        root_deps
    );

    assert!(
        hack_deps
            .iter()
            .any(|d| d == "pkg:golang/example.com/mikebomfixture/text@v0.37.0"),
        "hack should point at v0.37.0; got {:?}",
        hack_deps
    );

    assert!(
        tools_deps
            .iter()
            .any(|d| d == "pkg:golang/example.com/mikebomfixture/text@v0.29.0"),
        "tools should point at v0.29.0; got {:?}",
        tools_deps
    );

    assert!(
        deepthing_deps
            .iter()
            .any(|d| d == "pkg:golang/example.com/mikebomfixture/text@v0.25.0"),
        "deepthing should point at v0.25.0; got {:?}",
        deepthing_deps
    );

    // FR-003 explicit assertion — 4 distinct x/text components emitted.
    let text_component_count = json["components"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter(|c| {
                    c["purl"]
                        .as_str()
                        .map(|p| p.contains("mikebomfixture/text"))
                        .unwrap_or(false)
                })
                .count()
        })
        .unwrap_or(0);
    assert_eq!(
        text_component_count, 4,
        "expected 4 distinct x/text components (one per declared version); got {}",
        text_component_count
    );
}

// -----------------------------------------------------------
// SC-002 — root-only drops nested-module versions
// -----------------------------------------------------------

#[test]
fn per_mainmod_root_only_drops_nested_versions() {
    let json = run_scan("root-only");
    let versions: std::collections::BTreeSet<String> = json["components"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|c| c["purl"].as_str())
                .filter(|p| p.contains("mikebomfixture/text"))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    assert!(
        versions.contains("pkg:golang/example.com/mikebomfixture/text@v0.40.0"),
        "root's own v0.40.0 should survive; got {:?}",
        versions
    );
    // Pre-233 also emitted v0.25 (leaked from deep/src/thing). Post-233:
    // only root's v0.40 survives.
    for wrong in ["v0.25.0", "v0.29.0", "v0.37.0"] {
        let wrong_purl = format!("pkg:golang/example.com/mikebomfixture/text@{}", wrong);
        assert!(
            !versions.contains(&wrong_purl),
            "nested-module x/text {} leaked into root-only SBOM; got {:?}",
            wrong,
            versions
        );
    }
}

// -----------------------------------------------------------
// SC-005 — no main-module dependsOn any other main-module
// -----------------------------------------------------------

#[test]
fn no_main_module_depends_on_other_main_module() {
    let json = run_scan("all");
    let main_mods = ["example.com/root", "example.com/hack", "example.com/tools", "example.com/deepthing"];
    for src in &main_mods {
        let deps = depends_of_mainmod(&json, src);
        for dst in &main_mods {
            if src == dst {
                continue;
            }
            let dst_purl = format!("pkg:golang/{}@v0.0.0-unknown", dst);
            assert!(
                !deps.contains(&dst_purl),
                "{} MUST NOT dependsOn {} (no `replace` directive in fixture); got {:?}",
                src, dst, deps
            );
        }
    }
}

// -----------------------------------------------------------
// FR-005 (C1 remediation) — mode-invariance cross-check
// -----------------------------------------------------------

#[test]
fn mode_invariance_root_only_vs_all() {
    let all_json = run_scan("all");
    let root_json = run_scan("root-only");
    let all_root_deps = depends_of_mainmod(&all_json, "example.com/root");
    let root_root_deps = depends_of_mainmod(&root_json, "example.com/root");
    // Compare as sets — root's OWN edges should be identical regardless
    // of project-discovery mode. project-discovery filters WHICH main-
    // modules appear; it doesn't rewrite an emitted main-module's edges.
    use std::collections::BTreeSet;
    let all_set: BTreeSet<_> = all_root_deps.iter().collect();
    let root_set: BTreeSet<_> = root_root_deps.iter().collect();
    assert_eq!(
        all_set, root_set,
        "root main-module's edge set must be identical across --project-discovery modes; \
         all-mode: {:?}; root-only: {:?}",
        all_root_deps, root_root_deps
    );
}

// -----------------------------------------------------------
// FR-007 (C2 remediation) — graph-completeness reflects no leak orphans
// -----------------------------------------------------------

#[test]
fn graph_completeness_no_leak_orphans() {
    // FR-007 / C2 remediation — verify the leak-attributable orphan
    // signature is gone: pre-233, root's OWN declared x/text@v0.40.0
    // ended up orphaned (because a sibling's v0.25.0 got wrongly
    // attached to root instead). Post-233, root's v0.40.0 gets its
    // correct incoming edge from root and is NOT orphaned.
    //
    // Note: the classifier may still report `orphaned-components-detected`
    // because the 4 independent main-modules (hack/tools/deepthing) are
    // legitimately unreachable from root post-fix — that's the correct
    // shape, not a bug. FR-002 explicitly says main-modules shouldn't
    // point at each other without `replace`. So this test focuses on
    // the SPECIFIC leak signature: root's declared version must have
    // an incoming edge from root.
    let json = run_scan("all");
    let root_deps = depends_of_mainmod(&json, "example.com/root");
    assert!(
        root_deps
            .iter()
            .any(|d| d == "pkg:golang/example.com/mikebomfixture/text@v0.40.0"),
        "root's declared v0.40.0 must have an incoming edge from root (pre-233 it was orphaned because sibling's v0.25.0 got wrongly attached instead); got {:?}",
        root_deps
    );
}
