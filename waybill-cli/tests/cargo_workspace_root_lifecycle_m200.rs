//! Milestone 200 (issue #585) — cargo workspace-root [package] Runtime
//! classification. Regression fixture + integration tests per FR-006.
//!
//! Pre-m200 behavior: the workspace-root [package] fell through the
//! m052 classifier cascade to `LifecycleScope::Development` because the
//! prod-set BFS was seeded only from `[dependencies]` table contents
//! (never from `[package].name`). That misclassification cascaded into
//! CDX `scope: "excluded"` + m127 root-selector de-prioritization.
//!
//! Post-m200 (per m200 FR-001): `parse_cargo_toml` also seeds the
//! workspace-root `[package].name` into `CargoTomlSections.prod_deps`,
//! so the BFS closure includes the root as a Runtime seed → root is
//! Runtime → `scope: null` → root-selector picks the actual application.
//!
//! Two tests:
//! - `scan_cargo_workspace_root_is_runtime_m200` (FR-002 + SC-001):
//!   asserts the root component has `scope: null` and no
//!   `waybill:lifecycle-scope: "development"` annotation.
//! - `scan_cargo_workspace_root_wins_root_election_m200` (SC-001):
//!   asserts `metadata.component.name == "app"` when no operator
//!   override is passed.

use std::path::{Path, PathBuf};
use std::process::Command;

fn fixture_root() -> PathBuf {
    // The fixture is committed in-tree at
    // waybill-cli/tests/fixtures/cargo/root_package_lifecycle/.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/cargo/root_package_lifecycle")
}

/// Mirrors the `scan_path` helper at `scan_npm.rs:387`: shells out to
/// the built waybill binary via `env!("CARGO_BIN_EXE_waybill")` and
/// returns the parsed CDX JSON.
fn scan_path(path: &Path) -> serde_json::Value {
    let bin = env!("CARGO_BIN_EXE_waybill");
    let out_path = tempfile::NamedTempFile::new()
        .expect("tempfile")
        .path()
        .to_path_buf();
    let output = Command::new(bin)
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(path)
        .arg("--format")
        .arg("cyclonedx-json")
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

fn find_component<'a>(sbom: &'a serde_json::Value, name: &str) -> Option<&'a serde_json::Value> {
    sbom["components"]
        .as_array()?
        .iter()
        .find(|c| c["name"].as_str() == Some(name))
}

fn property_value<'a>(component: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    component["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"].as_str() == Some(key))?
        .get("value")?
        .as_str()
}

/// FR-002 + SC-001: workspace-root [package] emits with `scope: null`
/// and no `waybill:lifecycle-scope: "development"` annotation.
#[test]
fn scan_cargo_workspace_root_is_runtime_m200() {
    let sbom = scan_path(&fixture_root());
    // The `app` component might land in `components[]` OR as
    // `metadata.component` depending on whether it wins the m127 root
    // election. Check both locations.
    let root_component = find_component(&sbom, "app")
        .or_else(|| {
            let meta = &sbom["metadata"]["component"];
            if meta["name"].as_str() == Some("app") {
                Some(meta)
            } else {
                None
            }
        })
        .expect("app component must be emitted somewhere");
    // FR-002: scope MUST NOT be "excluded". (null OR absent both count
    // as "not excluded" — CDX 1.6 default is unscoped.)
    let scope = root_component.get("scope").and_then(|v| v.as_str());
    assert!(
        scope != Some("excluded"),
        "workspace-root [package] must NOT be scope=excluded post-m200; got scope={scope:?}"
    );
    // FR-002 corollary: no development lifecycle-scope annotation.
    let lifecycle = property_value(root_component, "waybill:lifecycle-scope");
    assert!(
        lifecycle != Some("development"),
        "workspace-root [package] must NOT carry waybill:lifecycle-scope=development post-m200; got {lifecycle:?}"
    );
}

/// SC-001 corollary: `app` (workspace root) MUST end up as
/// `metadata.component` in a simple 2-crate fixture. The m127 root-
/// selector prefers workspace-root components via the RepoRoot ladder
/// when exactly one `is_workspace_root=true` component exists — which
/// is the case here (app's Cargo.toml is at fixture root, helper's is
/// in `helper/`). Note: this SIMPLE fixture doesn't reproduce the
/// vaultwarden multi-ecosystem m127 tie-break bug (where npm+cargo+
/// cargo-workspace-member candidates each may be tagged
/// is_workspace_root=true depending on their manifest location). That
/// broader case is out of m200 scope; see follow-up issue.
#[test]
fn scan_cargo_workspace_root_wins_root_election_m200() {
    let sbom = scan_path(&fixture_root());
    let meta_name = sbom["metadata"]["component"]["name"].as_str();
    let meta_purl = sbom["metadata"]["component"]["purl"].as_str();
    assert_eq!(
        meta_name,
        Some("app"),
        "root election must pick 'app' (workspace root), not 'helper'; got name={meta_name:?}"
    );
    assert!(
        meta_purl.is_some_and(|p| p.starts_with("pkg:cargo/app@")),
        "metadata.component.purl must be a pkg:cargo/app@... variant; got {meta_purl:?}"
    );
}

// ============================================================
// Milestone 201 (issue #587) — root-selector disambiguation via
// `waybill:is-cargo-workspace-toplevel` positive-identifier signal.
// The extended fixture (sub/package.json + sub/index.js) introduces
// a 3rd main-module candidate (npm) alongside the existing cargo-
// root `app` + cargo-member `helper`. Post-m201 the RepoRoot ladder
// picks `app` as metadata.component via the positive-identifier
// short-circuit, sidestepping the shared-Cargo.lock-path collision
// that fools the filesystem-based is_workspace_root check.
// ============================================================

/// FR-002 + SC-001 + SC-002: with 3 main-module candidates (cargo-root,
/// cargo-member, npm-nested), the RepoRoot ladder correctly picks the
/// cargo workspace-toplevel `app` as `metadata.component`, AND reports
/// the root-selection heuristic as `"repo-root"` (was `"ecosystem-
/// priority"` pre-m201). The heuristic assertion is the LOAD-BEARING
/// part: `app` happens to win the alphabetical tie-break even pre-m201
/// in this fixture (a-p-p < h-e-l-p-e-r < s-u-b), so checking meta_name
/// alone doesn't prove the fix. Only heuristic name proves the RepoRoot
/// ladder branch fired (post-m201) rather than falling through to
/// ecosystem-priority (pre-m201).
#[test]
fn scan_cargo_workspace_root_wins_multi_ecosystem_m201() {
    let sbom = scan_path(&fixture_root());
    let meta_name = sbom["metadata"]["component"]["name"].as_str();
    let meta_purl = sbom["metadata"]["component"]["purl"].as_str();
    assert_eq!(
        meta_name,
        Some("app"),
        "multi-main-module root election must pick 'app' (cargo workspace toplevel), \
         not 'helper' (cargo member) or 'sub' (nested npm); got name={meta_name:?}"
    );
    assert!(
        meta_purl.is_some_and(|p| p.starts_with("pkg:cargo/app@")),
        "metadata.component.purl must be a pkg:cargo/app@... variant; got {meta_purl:?}"
    );
    // LOAD-BEARING: verify the RepoRoot ladder actually fired (post-m201
    // positive-identifier signal), NOT ecosystem-priority (pre-m201
    // fallback that happened to also pick 'app' alphabetically in this
    // fixture layout). The heuristic annotation lives at
    // metadata.properties[] as a JSON-envelope-in-string carrying
    // {"heuristic": "...", "confidence": N} per the m127 wire contract.
    let heuristic_annot = sbom["metadata"]["properties"]
        .as_array()
        .and_then(|arr| {
            arr.iter()
                .find(|p| p["name"].as_str() == Some("waybill:root-selection-heuristic"))
        })
        .and_then(|p| p["value"].as_str())
        .expect("root-selection-heuristic annotation must be present on metadata");
    let envelope: serde_json::Value =
        serde_json::from_str(heuristic_annot).expect("valid JSON envelope");
    let heuristic_name = envelope["value"]["heuristic"].as_str();
    // Heuristic name is `"repo-root-main-module"` per
    // `RootSelectionHeuristic::RepoRoot::name()` at root_selector.rs:77
    // (confidence 0.95). Pre-m201 was `"ecosystem-priority"` conf 0.7.
    assert_eq!(
        heuristic_name,
        Some("repo-root-main-module"),
        "post-m201 the multi-main-module root election MUST use the RepoRoot ladder \
         (heuristic=\"repo-root-main-module\") — was \"ecosystem-priority\" pre-m201. \
         Full annotation envelope: {heuristic_annot}"
    );
}

/// FR-007: the new internal-only annotation `waybill:is-cargo-workspace-toplevel`
/// MUST NOT appear anywhere in emitted SBOM output (filtered by extended
/// `is_internal_emission_key` at root_selector.rs).
#[test]
fn scan_cargo_new_internal_annotation_is_filtered_from_output_m201() {
    let sbom = scan_path(&fixture_root());
    let raw = serde_json::to_string(&sbom).unwrap();
    assert!(
        !raw.contains("waybill:is-cargo-workspace-toplevel"),
        "internal-only annotation MUST NOT leak into emitted SBOM (FR-007); \
         found in serialized output"
    );
}
