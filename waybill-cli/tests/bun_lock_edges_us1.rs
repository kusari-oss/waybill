//! Milestone 667 T026 — end-to-end integration test for the bun.lock
//! transitive-edge fix (issue #723).
//!
//! **What this covers**
//!
//! Each of the four m667 fixtures at `tests/fixtures/bun_lock/*/`
//! gets its own `#[test]` that shells out `waybill sbom scan
//! --path <fixture> --offline --format cyclonedx-json --output <file>`
//! and asserts on the emitted CDX JSON. Assertions verify the
//! spec.md US1 acceptance scenarios end-to-end (reader → resolve →
//! graph-completeness → emitter), NOT just the reader-level
//! `parse_bun_lock` invariants covered by T018-T025.
//!
//! Coverage per fixture (from tasks.md T026):
//! - **minimal_repro**: SC-001 (parent→child edge lands, child
//!   reachable, graph is `complete`).
//! - **multi_version**: SC-004 (per-version disambiguation via the
//!   graph-builder's `<name> <version>` secondary `name_to_purl` key).
//! - **scoped_name**: SC-005 (scope-atomic R2 walker resolves the
//!   scope-nested `@types/node` correctly through the URL-encoded PURL).
//! - **optional_deps**: US1 scenario 3 (opt-child is reachable AND
//!   carries m180's optional decoration in both `scope: "excluded"`
//!   (CDX `scope` enum's non-runtime value per m052 FR-010) and the
//!   `waybill:optional-derivation = "bun-optional-dependencies"` property.
//!
//! **Note on CDX `scope` value**: tasks.md T026 says `scope: "optional"`;
//! actual m052/m112 emission is `scope: "excluded"` (CDX 1.6's 3-value
//! enum only distinguishes runtime/required/excluded — the finer
//! optional-vs-dev-vs-build split lives in the
//! `waybill:lifecycle-scope` property, not the CDX enum). Asserting on
//! the actual value; the task-text discrepancy is a docstring bug, not
//! an emission bug.

use std::path::PathBuf;
use std::process::Command;

mod common;

/// Path to a bun_lock fixture crate-local at
/// `waybill-cli/tests/fixtures/bun_lock/<name>/`. `common::local_fixture_path`
/// resolves against workspace root (`<repo>/tests/fixtures/`), but the
/// m667 fixtures deliberately live crate-local (they're m667-specific
/// and don't fit the m090 stay-set criteria). We compute the path
/// against `CARGO_MANIFEST_DIR` (which cargo sets to `waybill-cli/`
/// for this integration test).
fn bun_lock_fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/bun_lock")
        .join(name)
}

/// Run `waybill sbom scan --path <fixture> --offline --format
/// cyclonedx-json --output <tempfile>` and return the parsed CDX
/// JSON. Follows the m665 `no_binary_scan_us3_annotation.rs`
/// harness shape verbatim, adapted for single-format CDX output.
fn scan_cdx(fixture: &std::path::Path) -> serde_json::Value {
    let bin = env!("CARGO_BIN_EXE_waybill");
    let tmp = tempfile::tempdir().expect("tempdir");
    let cdx_out = tmp.path().join("out.cdx.json");

    let output = Command::new(bin)
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(fixture)
        .arg("--no-deep-hash")
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(&cdx_out)
        .output()
        .expect("waybill should run");

    assert!(
        output.status.success(),
        "waybill scan failed for {}: stderr={}",
        fixture.display(),
        String::from_utf8_lossy(&output.stderr),
    );

    serde_json::from_str(
        &std::fs::read_to_string(&cdx_out).expect("read CDX output"),
    )
    .expect("valid CDX JSON")
}

/// Find the `dependencies[]` entry whose `ref` matches the given PURL
/// and return its `dependsOn` list. Returns `None` if no such entry
/// exists (which means "no outbound edges from this component" — a
/// legit shape for leaf components, but a fix regression for
/// registered parents).
fn depends_on(cdx: &serde_json::Value, from_ref: &str) -> Option<Vec<String>> {
    cdx["dependencies"].as_array()?.iter().find_map(|d| {
        if d["ref"].as_str()? == from_ref {
            let arr = d["dependsOn"].as_array()?;
            Some(
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect(),
            )
        } else {
            None
        }
    })
}

/// Locate a `components[]` entry by its PURL.
fn find_component<'a>(
    cdx: &'a serde_json::Value,
    purl: &str,
) -> Option<&'a serde_json::Value> {
    cdx["components"]
        .as_array()?
        .iter()
        .find(|c| c["purl"].as_str() == Some(purl))
}

/// Extract a per-component `properties[].value` by name. Returns
/// `None` when the property is absent — the shape for reachable,
/// non-orphan, non-optional components.
fn component_property_value(
    component: &serde_json::Value,
    name: &str,
) -> Option<String> {
    component["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"].as_str() == Some(name))
        .and_then(|p| p["value"].as_str().map(|s| s.to_string()))
}

/// Extract the document-scope `metadata.properties[].value` by name
/// (same shape as m665's `cdx_property_value`).
fn document_property_value(
    cdx: &serde_json::Value,
    name: &str,
) -> Option<String> {
    cdx["metadata"]["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"].as_str() == Some(name))
        .and_then(|p| p["value"].as_str().map(|s| s.to_string()))
}

// ────────────────────────────────────────────────────────────────
// SC-001: minimal_repro — issue #723's exact repro.
//
// Post-fix: parent-pkg → child-pkg edge lands, child is reachable,
// the whole graph is `complete`.
// ────────────────────────────────────────────────────────────────
#[test]
fn m667_us1_sc001_minimal_repro_edge_lands() {
    let fixture: PathBuf = bun_lock_fixture("minimal_repro");
    let cdx = scan_cdx(&fixture);

    // Edge: parent-pkg@1.0.0 → child-pkg@1.0.0
    let parent_ref = "pkg:npm/parent-pkg@1.0.0";
    let child_ref = "pkg:npm/child-pkg@1.0.0";
    let edges = depends_on(&cdx, parent_ref).unwrap_or_else(|| {
        panic!(
            "SC-001: no dependencies[] entry for {parent_ref} — the fix's \
             single load-bearing edge is missing"
        )
    });
    assert!(
        edges.iter().any(|e| e == child_ref),
        "SC-001: parent-pkg MUST dependsOn child-pkg; got: {edges:?}",
    );

    // child-pkg carries no waybill:orphan-reason (it's reachable).
    let child = find_component(&cdx, child_ref)
        .expect("child-pkg component must be emitted");
    assert_eq!(
        component_property_value(child, "waybill:orphan-reason"),
        None,
        "SC-001: child-pkg is now reachable via parent-pkg; \
         waybill:orphan-reason MUST be absent",
    );

    // Document-scope graph-completeness = "complete".
    let completeness = document_property_value(&cdx, "waybill:graph-completeness");
    assert_eq!(
        completeness.as_deref(),
        Some("complete"),
        "SC-001: with the sole reachable transitive edge landing, \
         graph-completeness MUST be `complete`; got: {completeness:?}",
    );
}

// ────────────────────────────────────────────────────────────────
// SC-004: multi_version — per-version disambiguation.
//
// `big@1.0.0` and `small@2.0.0` each depend on a DIFFERENT
// `minimatch` version. R1's `<name> <version>` disambiguation
// combined with the graph-builder's secondary `name_to_purl` key
// makes each edge point at its own version copy.
// ────────────────────────────────────────────────────────────────
#[test]
fn m667_us1_sc004_multi_version_edges_target_correct_copy() {
    let fixture: PathBuf = bun_lock_fixture("multi_version");
    let cdx = scan_cdx(&fixture);

    // big@1.0.0 → minimatch@3.1.2 (NOT 5.1.6).
    let big_edges = depends_on(&cdx, "pkg:npm/big@1.0.0")
        .expect("big@1.0.0 must have a dependencies[] entry");
    assert!(
        big_edges.iter().any(|e| e == "pkg:npm/minimatch@3.1.2"),
        "SC-004: big@1.0.0 MUST dependsOn minimatch@3.1.2; got: {big_edges:?}",
    );
    assert!(
        !big_edges.iter().any(|e| e == "pkg:npm/minimatch@5.1.6"),
        "SC-004: big@1.0.0 MUST NOT dependsOn the 5.1.6 version; got: {big_edges:?}",
    );

    // small@2.0.0 → minimatch@5.1.6 (NOT 3.1.2).
    let small_edges = depends_on(&cdx, "pkg:npm/small@2.0.0")
        .expect("small@2.0.0 must have a dependencies[] entry");
    assert!(
        small_edges.iter().any(|e| e == "pkg:npm/minimatch@5.1.6"),
        "SC-004: small@2.0.0 MUST dependsOn minimatch@5.1.6; got: {small_edges:?}",
    );
    assert!(
        !small_edges.iter().any(|e| e == "pkg:npm/minimatch@3.1.2"),
        "SC-004: small@2.0.0 MUST NOT dependsOn the 3.1.2 version; got: {small_edges:?}",
    );

    // Both versions emitted as distinct components.
    assert!(
        find_component(&cdx, "pkg:npm/minimatch@3.1.2").is_some(),
        "minimatch@3.1.2 component MUST be emitted",
    );
    assert!(
        find_component(&cdx, "pkg:npm/minimatch@5.1.6").is_some(),
        "minimatch@5.1.6 component MUST be emitted",
    );
}

// ────────────────────────────────────────────────────────────────
// SC-005: scoped_name — scope-atomic resolver + URL-encoded PURL.
//
// Parent `@fast-csv/format` (URL-encoded to `%40fast-csv/format`
// per purl-spec) → `@types/node@22.5.0` (URL-encoded to
// `%40types/node`) via the scope-nested `@fast-csv/format/@types/node`
// packages-map key.
// ────────────────────────────────────────────────────────────────
#[test]
fn m667_us1_sc005_scoped_name_edge_targets_scope_nested_copy() {
    let fixture: PathBuf = bun_lock_fixture("scoped_name");
    let cdx = scan_cdx(&fixture);

    let parent_ref = "pkg:npm/%40fast-csv/format@4.3.6";
    let child_ref = "pkg:npm/%40types/node@22.5.0";

    let edges = depends_on(&cdx, parent_ref).unwrap_or_else(|| {
        panic!("SC-005: no dependencies[] entry for {parent_ref}")
    });
    assert!(
        edges.iter().any(|e| e == child_ref),
        "SC-005: @fast-csv/format MUST dependsOn the scope-nested \
         @types/node@22.5.0; got: {edges:?}",
    );

    assert!(
        find_component(&cdx, child_ref).is_some(),
        "@types/node@22.5.0 component MUST be emitted",
    );
}

// ────────────────────────────────────────────────────────────────
// US1 scenario 3 (optional_deps): optional decoration reaches CDX.
//
// The edge exists, the target component is emitted, and the
// target carries BOTH:
//   (a) native CDX `scope: "excluded"` (m052/m112 emission for any
//       non-Runtime `LifecycleScope`; `Optional` maps here), and
//   (b) the `waybill:optional-derivation` property with value
//       `"bun-optional-dependencies"`.
// ────────────────────────────────────────────────────────────────
#[test]
fn m667_us1_optional_dep_decoration_reaches_cdx() {
    let fixture: PathBuf = bun_lock_fixture("optional_deps");
    let cdx = scan_cdx(&fixture);

    let parent_ref = "pkg:npm/parent@1.0.0";
    let child_ref = "pkg:npm/opt-child@1.0.0";

    // Edge lands (target-side scope carries the optionality per
    // m180 convention; edge itself is a plain dependsOn entry).
    let edges = depends_on(&cdx, parent_ref).unwrap_or_else(|| {
        panic!("US1-scenario-3: no dependencies[] entry for {parent_ref}")
    });
    assert!(
        edges.iter().any(|e| e == child_ref),
        "US1-scenario-3: parent MUST dependsOn opt-child; got: {edges:?}",
    );

    // Native CDX `scope` field — m052/m112 emits "excluded" for any
    // non-Runtime LifecycleScope. (Task text says "optional"; actual
    // value is "excluded" per the 3-value CDX enum. See docstring.)
    let child = find_component(&cdx, child_ref)
        .expect("opt-child component must be emitted");
    assert_eq!(
        child["scope"].as_str(),
        Some("excluded"),
        "opt-child MUST carry CDX `scope: \"excluded\"` (m052/m112 \
         convention for non-Runtime LifecycleScope); got: {:?}",
        child["scope"],
    );

    // m667's own tag: waybill:optional-derivation = "bun-optional-dependencies".
    assert_eq!(
        component_property_value(child, "waybill:optional-derivation").as_deref(),
        Some("bun-optional-dependencies"),
        "opt-child MUST carry waybill:optional-derivation = \
         `bun-optional-dependencies` (m667's derivation tag)",
    );
}
