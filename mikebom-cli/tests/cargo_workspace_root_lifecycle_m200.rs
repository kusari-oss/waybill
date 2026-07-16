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
//!   `mikebom:lifecycle-scope: "development"` annotation.
//! - `scan_cargo_workspace_root_wins_root_election_m200` (SC-001):
//!   asserts `metadata.component.name == "app"` when no operator
//!   override is passed.

use std::path::{Path, PathBuf};
use std::process::Command;

fn fixture_root() -> PathBuf {
    // The fixture is committed in-tree at
    // mikebom-cli/tests/fixtures/cargo/root_package_lifecycle/.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/cargo/root_package_lifecycle")
}

/// Mirrors the `scan_path` helper at `scan_npm.rs:387`: shells out to
/// the built mikebom binary via `env!("CARGO_BIN_EXE_mikebom")` and
/// returns the parsed CDX JSON.
fn scan_path(path: &Path) -> serde_json::Value {
    let bin = env!("CARGO_BIN_EXE_mikebom");
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
        .expect("mikebom should run");
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
/// and no `mikebom:lifecycle-scope: "development"` annotation.
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
    let lifecycle = property_value(root_component, "mikebom:lifecycle-scope");
    assert!(
        lifecycle != Some("development"),
        "workspace-root [package] must NOT carry mikebom:lifecycle-scope=development post-m200; got {lifecycle:?}"
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
