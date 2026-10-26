//! Milestone 179 US3 — end-to-end integration tests for the Cargo
//! optional-dep classifier. Scans the m179 fixture
//! (`tests/fixtures/optional_dep/cargo`) via the compiled `waybill`
//! binary and asserts:
//!
//! - **T026** (Full mode): SPDX 2.3 emits `once_cell
//!   OPTIONAL_DEPENDENCY_OF <root>` (reversed direction per m052);
//!   CDX emits `once_cell` with `scope: "excluded"`; both formats
//!   carry the `waybill:optional-derivation = "cargo-optional-true"`
//!   annotation; SPDX 3 carries the annotation (no native
//!   `lifecycleScope: "optional"` per FR-017).
//!
//! - **T027** (Basic mode): under
//!   `--spdx2-relationship-compat=basic`, SPDX 2.3 has zero
//!   `OPTIONAL_DEPENDENCY_OF` edges — every dep collapses to
//!   natural-direction `DEPENDS_ON` per m228; the annotation IS still
//!   present (annotation is orthogonal to relationship-compat).

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/optional_dep/cargo")
}

/// Skip cleanly if `cargo` is not on PATH. Mirrors the m205 suite's
/// guard at `cargo_optional_feature_resolve.rs::require_cargo`.
fn require_cargo() -> bool {
    Command::new("cargo").arg("--version").output().is_ok()
}

/// Copy the committed fixture into a tempdir and run `cargo fetch`
/// there, so waybill's own `cargo metadata --offline` subprocess can
/// resolve the feature set.
///
/// `cargo fetch` rather than `generate-lockfile` is deliberate and the
/// reason is m205's, quoted from `build_synthetic_workspace`: metadata
/// reads dep manifests from *unpacked* crates under `registry/src/`,
/// and `generate-lockfile` populates the index without unpacking. That
/// mismatch is what made m205's first US1 test pass on a warm local
/// cache and fail on a cold CI runner.
///
/// The copy exists so the fetch cannot write into the committed
/// fixture directory.
fn fixture_copy_with_activation_evidence() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let src = fixture_path();
    for name in ["Cargo.toml", "Cargo.lock"] {
        std::fs::copy(src.join(name), dir.path().join(name)).unwrap();
    }
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("src/main.rs"), "fn main() {}\n").unwrap();
    let fetched = Command::new("cargo")
        .arg("fetch")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(
        fetched.status.success(),
        "cargo fetch must succeed so that waybill's `cargo metadata --offline` \
         has activation evidence to read; without it the scan falls back to \
         FeatureActivation::Unknown and every assertion below changes meaning \
         (#1272). stderr={}",
        String::from_utf8_lossy(&fetched.stderr),
    );
    dir
}

fn run_scan(project_root: &Path, extra_args: &[&str]) -> (Value, Value, Value) {
    run_scan_with_offline(project_root, extra_args, true)
}

/// `run_scan`, with control over whether waybill is given `--offline`.
///
/// This matters since #1272. `--offline` skips the m205 cargo-metadata
/// subprocess entirely (#771), leaving no feature-activation evidence,
/// and without evidence waybill declines to assert that a
/// declared-optional dependency is non-runtime. A test that wants to
/// observe the `Optional` classification on the wire must therefore
/// run *without* `--offline` against a workspace whose activation set
/// cargo can resolve.
fn run_scan_with_offline(
    project_root: &Path,
    extra_args: &[&str],
    offline: bool,
) -> (Value, Value, Value) {
    let out_dir = tempfile::tempdir().unwrap();
    let cdx_path = out_dir.path().join("out.cdx.json");
    let spdx23_path = out_dir.path().join("out.spdx.json");
    let spdx3_path = out_dir.path().join("out.spdx3.json");
    let mut cmd = Command::new(binary_path());
    if offline {
        cmd.arg("--offline");
    }
    cmd.arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(project_root)
        .arg("--no-deep-hash")
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--format")
        .arg("spdx-2.3-json")
        .arg("--format")
        .arg("spdx-3-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", cdx_path.display()))
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", spdx23_path.display()))
        .arg("--output")
        .arg(format!("spdx-3-json={}", spdx3_path.display()));
    for arg in extra_args {
        cmd.arg(arg);
    }
    let result = cmd.output().unwrap();
    assert!(
        result.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&result.stderr),
    );
    let cdx: Value = serde_json::from_slice(&std::fs::read(&cdx_path).unwrap()).unwrap();
    let spdx23: Value = serde_json::from_slice(&std::fs::read(&spdx23_path).unwrap()).unwrap();
    let spdx3: Value = serde_json::from_slice(&std::fs::read(&spdx3_path).unwrap()).unwrap();
    (cdx, spdx23, spdx3)
}

fn find_component_by_name<'a>(cdx: &'a Value, name: &str) -> Option<&'a Value> {
    cdx.get("components")
        .and_then(|v| v.as_array())?
        .iter()
        .find(|c| c.get("name").and_then(|v| v.as_str()) == Some(name))
}

fn find_property<'a>(component: &'a Value, name: &str) -> Option<&'a Value> {
    component
        .get("properties")
        .and_then(|v| v.as_array())?
        .iter()
        .find(|p| p.get("name").and_then(|v| v.as_str()) == Some(name))
        .and_then(|p| p.get("value"))
}

fn find_package_by_name<'a>(spdx: &'a Value, name: &str) -> Option<&'a Value> {
    spdx.get("packages")
        .and_then(|v| v.as_array())?
        .iter()
        .find(|p| p.get("name").and_then(|v| v.as_str()) == Some(name))
}

fn find_annotation_value(pkg: &Value, key: &str) -> Option<String> {
    let annotations = pkg.get("annotations").and_then(|v| v.as_array())?;
    for anno in annotations {
        // The SPDX 2.3 wire-format key is `comment`, not
        // `annotationComment` (though the spec allows both — waybill
        // emits `comment`).
        let comment = anno
            .get("comment")
            .or_else(|| anno.get("annotationComment"))
            .and_then(|v| v.as_str())?;
        // MikebomAnnotationCommentV1 envelope: JSON-encoded string.
        let parsed: Value = serde_json::from_str(comment).ok()?;
        let field = parsed.get("field").or(parsed.get("name")).and_then(|v| v.as_str())?;
        if field == key {
            let value = parsed.get("value")?;
            return Some(match value {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            });
        }
    }
    None
}

#[test]
fn t026_cargo_optional_full_mode_end_to_end() {
    if !require_cargo() {
        eprintln!("skipping: cargo needed to resolve the fixture's feature set");
        return;
    }
    // Runs WITHOUT `--offline` against a fetched copy, because the
    // `Optional` classification this test observes now requires
    // feature-activation evidence (#1272). Under `--offline` waybill
    // declines to assert non-runtime and `once_cell` classifies
    // Runtime — that path is asserted by
    // `issue_1272_offline_declines_to_assert_non_runtime` below.
    let ws = fixture_copy_with_activation_evidence();
    let (cdx, spdx23, spdx3) = run_scan_with_offline(ws.path(), &[], false);

    // --- CDX 1.6 assertions ---
    let once_cell_cdx =
        find_component_by_name(&cdx, "once_cell").expect("once_cell present in CDX");
    // FR-006 / FR-016 — native CDX signal is `scope: "excluded"`,
    // auto-emitted via LifecycleScope::Optional's is_non_runtime().
    assert_eq!(
        once_cell_cdx.get("scope").and_then(|v| v.as_str()),
        Some("excluded"),
        "CDX MUST emit `scope: \"excluded\"` on once_cell (Optional -> is_non_runtime)"
    );
    // FR-019 — annotation carries derivation source.
    assert_eq!(
        find_property(once_cell_cdx, "waybill:optional-derivation")
            .and_then(|v| v.as_str()),
        Some("cargo-optional-true"),
        "CDX MUST carry waybill:optional-derivation on once_cell"
    );

    // --- SPDX 2.3 assertions (Full mode is default) ---
    // Locate the relationship: `once_cell OPTIONAL_DEPENDENCY_OF <root>`.
    // The reversed-direction convention (m052 + m179 T007) means
    // once_cell is the `spdxElementId` (source) and the root is the
    // `relatedSpdxElement` (target).
    let rels = spdx23
        .get("relationships")
        .and_then(|v| v.as_array())
        .expect("SPDX 2.3 has relationships");
    let opt_dep_of = rels
        .iter()
        .find(|r| {
            r.get("relationshipType").and_then(|v| v.as_str())
                == Some("OPTIONAL_DEPENDENCY_OF")
        })
        .expect("SPDX 2.3 MUST emit at least one OPTIONAL_DEPENDENCY_OF edge in Full mode");
    // Source SPDXID should map to once_cell.
    let once_cell_spdx =
        find_package_by_name(&spdx23, "once_cell").expect("once_cell package in SPDX 2.3");
    let once_cell_id = once_cell_spdx.get("SPDXID").and_then(|v| v.as_str()).unwrap();
    assert_eq!(
        opt_dep_of.get("spdxElementId").and_then(|v| v.as_str()),
        Some(once_cell_id),
        "reversed-direction: once_cell SPDXID MUST be the source (spdxElementId) of OPTIONAL_DEPENDENCY_OF"
    );
    // Annotation on the Package.
    assert_eq!(
        find_annotation_value(once_cell_spdx, "waybill:optional-derivation"),
        Some("cargo-optional-true".to_string()),
        "SPDX 2.3 MUST carry waybill:optional-derivation on once_cell Package"
    );

    // --- SPDX 3.0.1 assertions ---
    // No native lifecycleScope="optional" per FR-017; annotation
    // carries the classification instead.
    let elements = spdx3
        .get("@graph")
        .and_then(|v| v.as_array())
        .expect("SPDX 3 @graph present");
    let once_cell_elem = elements
        .iter()
        .find(|e| {
            e.get("name").and_then(|v| v.as_str()) == Some("once_cell")
                && e.get("type").and_then(|v| v.as_str()) == Some("software_Package")
        })
        .expect("once_cell software_Package in SPDX 3");
    let once_cell_iri = once_cell_elem.get("spdxId").and_then(|v| v.as_str()).unwrap();
    // Find any Annotation subject-linked to once_cell.
    let annotations: Vec<&Value> = elements
        .iter()
        .filter(|e| e.get("type").and_then(|v| v.as_str()) == Some("Annotation"))
        .filter(|e| e.get("subject").and_then(|v| v.as_str()) == Some(once_cell_iri))
        .collect();
    let has_optional_deriv = annotations.iter().any(|a| {
        a.get("statement")
            .and_then(|v| v.as_str())
            .map(|s| s.contains("waybill:optional-derivation") && s.contains("cargo-optional-true"))
            .unwrap_or(false)
    });
    assert!(
        has_optional_deriv,
        "SPDX 3 MUST carry a waybill:optional-derivation Annotation on once_cell software_Package"
    );
    // Confirm no LifecycleScopedRelationship with scope="optional" was
    // emitted (FR-017 — SPDX 3.0.1 has no such enum value).
    for e in elements {
        if e.get("type").and_then(|v| v.as_str()) == Some("LifecycleScopedRelationship") {
            let scope = e.get("scope").and_then(|v| v.as_str()).unwrap_or("");
            assert_ne!(
                scope, "optional",
                "SPDX 3 MUST NOT emit LifecycleScopedRelationship with scope=optional (FR-017)"
            );
        }
    }
}

#[test]
fn t027_cargo_optional_basic_mode_collapses() {
    if !require_cargo() {
        eprintln!("skipping: cargo needed to resolve the fixture's feature set");
        return;
    }
    // FR-003 / SC-006 — under --spdx2-relationship-compat=basic, ALL
    // typed dep-scope edges collapse to natural-direction DEPENDS_ON.
    // Not `--offline`, for the reason given on t026.
    let ws = fixture_copy_with_activation_evidence();
    let (cdx, spdx23, _) = run_scan_with_offline(
        ws.path(),
        &["--spdx2-relationship-compat=basic"],
        false,
    );

    // CDX side is unchanged in basic mode — still emits
    // `scope: "excluded"` on once_cell (the flag only affects SPDX 2.3).
    let once_cell_cdx =
        find_component_by_name(&cdx, "once_cell").expect("once_cell present in CDX");
    assert_eq!(
        once_cell_cdx.get("scope").and_then(|v| v.as_str()),
        Some("excluded"),
        "CDX scope emission is INDEPENDENT of --spdx2-relationship-compat"
    );

    // SPDX 2.3 has zero OPTIONAL_DEPENDENCY_OF edges under basic mode.
    let rels = spdx23
        .get("relationships")
        .and_then(|v| v.as_array())
        .expect("SPDX 2.3 has relationships");
    let optional_count = rels
        .iter()
        .filter(|r| {
            r.get("relationshipType").and_then(|v| v.as_str())
                == Some("OPTIONAL_DEPENDENCY_OF")
        })
        .count();
    assert_eq!(
        optional_count, 0,
        "SPDX 2.3 basic mode MUST NOT emit any OPTIONAL_DEPENDENCY_OF edges (m228 escape hatch)"
    );

    // The annotation IS still present — annotation is orthogonal.
    let once_cell_spdx =
        find_package_by_name(&spdx23, "once_cell").expect("once_cell package in SPDX 2.3");
    assert_eq!(
        find_annotation_value(once_cell_spdx, "waybill:optional-derivation"),
        Some("cargo-optional-true".to_string()),
        "waybill:optional-derivation annotation MUST be present in basic mode too"
    );
}

/// #1272 — under `--offline` there is no feature-activation evidence,
/// so waybill must not assert that a declared-optional dependency is
/// non-runtime.
///
/// The fixture's `once_cell` genuinely does not ship: it sits behind a
/// non-default `caching` feature, and `cargo metadata` confirms it is
/// absent from the resolved activation set. So this test asserts the
/// *less precise* answer on purpose. The two error directions are not
/// symmetric — over-including a dependency that does not ship costs a
/// downstream false positive, while under-including one that does ship
/// costs a false negative. Before #1272 the degraded path took the
/// second direction, and on the `rust-ripgrep` corpus target that
/// emitted `scope: "excluded"` for `log`, `serde` and `serde_json`,
/// two of them declared non-optionally in the workspace root's own
/// `[dependencies]`.
///
/// This is the hermetic half of the pair: no cargo subprocess runs, so
/// unlike `t026` it needs neither a cargo binary nor a warm registry.
#[test]
fn issue_1272_offline_declines_to_assert_non_runtime() {
    let (cdx, spdx23, _) = run_scan(&fixture_path(), &[]);

    let once_cell_cdx =
        find_component_by_name(&cdx, "once_cell").expect("once_cell present in CDX");
    // CDX omits `scope` for runtime components rather than writing
    // "runtime", so absence is the assertion. Anything else means
    // waybill claimed to know something it was never told.
    assert_eq!(
        once_cell_cdx.get("scope").and_then(|v| v.as_str()),
        None,
        "under --offline waybill has no activation evidence and MUST NOT emit \
         scope=excluded for a declared-optional dep. cdx component: {once_cell_cdx:#}"
    );
    assert_eq!(
        find_property(once_cell_cdx, "waybill:optional-derivation"),
        None,
        "the derivation annotation is set by the Optional branch, which must not \
         have fired. cdx component: {once_cell_cdx:#}"
    );

    // And the SPDX 2.3 side carries no typed optional edge, since the
    // edge type is derived from the same scope.
    let rels = spdx23
        .get("relationships")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let optional_edges: Vec<&Value> = rels
        .iter()
        .filter(|r| {
            r.get("relationshipType").and_then(|v| v.as_str()) == Some("OPTIONAL_DEPENDENCY_OF")
        })
        .collect();
    assert!(
        optional_edges.is_empty(),
        "no OPTIONAL_DEPENDENCY_OF edge may be emitted without activation \
         evidence, found: {optional_edges:#?}"
    );

    // The control: `serde`, a plain non-optional dependency, is still
    // present and still unscoped. Without this, the assertions above
    // would also pass against a scan that emitted no cargo components
    // at all.
    let serde_cdx = find_component_by_name(&cdx, "serde").expect("serde present in CDX");
    assert_eq!(
        serde_cdx.get("scope").and_then(|v| v.as_str()),
        None,
        "serde is a plain runtime dep and stays unscoped. cdx: {serde_cdx:#}"
    );
}
