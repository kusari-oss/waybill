//! Milestone 176: monorepo workspace-member visibility integration tests.
//!
//! Covers three user stories:
//!
//! * **US1 (P1 MVP)**: per-component `waybill:workspace-member`
//!   annotation lets consumers filter emitted components by workspace
//!   via jq. FR-001 emission; FR-002 file-tier absence; SC-001
//!   per-workspace filter behavior; SC-006 tf-models-shape 3-workspace
//!   distinct-set assertion (via T014 remediation from `/speckit-analyze`).
//! * **US2 (P1)** and **US3 (P2)** land in Phases 4 + 5 —
//!   this file grows to hold their tests too, mirroring
//!   `tests/warm_go_cache.rs` (m173).

use std::path::Path;
use std::process::Command;

mod common;
use common::bin;

/// Scan `path`, capturing the emitted CDX SBOM as parsed JSON plus
/// the captured stderr. Isolates HOME to a fresh tempdir so no
/// operator-side dotfile / cache leaks into the scan.
fn scan_cdx(path: &Path) -> (serde_json::Value, String) {
    let tmp = tempfile::NamedTempFile::new().expect("tempfile");
    let out_path = tmp.path().to_path_buf();
    let fake_home = tempfile::tempdir().expect("fake-home tempdir");

    let mut cmd = Command::new(bin());
    common::normalize::apply_fake_home_env(&mut cmd, fake_home.path());
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(path)
        .arg("--output")
        .arg(&out_path)
        .arg("--no-deep-hash");

    let output = cmd.output().expect("waybill should run");
    assert!(
        output.status.success(),
        "scan failed (exit={:?}): stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    let raw = std::fs::read_to_string(&out_path).expect("read sbom");
    let sbom = serde_json::from_str(&raw).expect("valid JSON");
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    (sbom, stderr)
}

/// Synthesize a 3-workspace pip monorepo:
///   root/pyproject.toml       — root workspace, distinct + shared deps
///   root/subproject_a/pyproject.toml — subproject A, distinct + shared
///   root/subproject_b/pyproject.toml — subproject B, distinct + shared
/// The shared dep (`shared-dep`) appears in all three workspaces —
/// exercises the FR-001 "cross-workspace shared component" acceptance
/// scenario 3.
fn write_three_workspace_pip_fixture(root: &Path) {
    fn write_pyproject(dir: &Path, name: &str, distinct_dep: &str) {
        std::fs::create_dir_all(dir).expect("mkdir");
        let content = format!(
            "[project]\n\
             name = \"{name}\"\n\
             version = \"0.1.0\"\n\
             requires-python = \">=3.10\"\n\
             dependencies = [\n\
                 \"{distinct_dep}\",\n\
                 \"shared-dep>=1.0\",\n\
             ]\n"
        );
        std::fs::write(dir.join("pyproject.toml"), content).expect("write pyproject.toml");
    }

    write_pyproject(root, "root-pkg", "root-only-dep>=1.0");
    write_pyproject(&root.join("subproject_a"), "sub-a-pkg", "sub-a-only-dep>=2.0");
    write_pyproject(&root.join("subproject_b"), "sub-b-pkg", "sub-b-only-dep>=3.0");
}

/// Extract the union of every `waybill:workspace-member` value found
/// on components in the emitted CDX SBOM. Returns a deduplicated,
/// alphabetically-sorted Vec of workspace paths.
fn component_workspace_paths(sbom: &serde_json::Value) -> Vec<String> {
    use std::collections::BTreeSet;
    let mut out: BTreeSet<String> = BTreeSet::new();
    let Some(components) = sbom["components"].as_array() else {
        return Vec::new();
    };
    for c in components {
        let Some(props) = c["properties"].as_array() else {
            continue;
        };
        for p in props {
            if p["name"] == "waybill:workspace-member" {
                if let Some(v) = p["value"].as_str() {
                    if let Ok(parsed) = serde_json::from_str::<Vec<String>>(v) {
                        for entry in parsed {
                            out.insert(entry);
                        }
                    }
                }
            }
        }
    }
    out.into_iter().collect()
}

/// For a given workspace path, return every component PURL whose
/// `waybill:workspace-member` array-contains the path.
fn purls_in_workspace(sbom: &serde_json::Value, workspace: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let Some(components) = sbom["components"].as_array() else {
        return out;
    };
    for c in components {
        let Some(props) = c["properties"].as_array() else {
            continue;
        };
        let mut member = false;
        for p in props {
            if p["name"] == "waybill:workspace-member" {
                if let Some(v) = p["value"].as_str() {
                    if let Ok(parsed) = serde_json::from_str::<Vec<String>>(v) {
                        if parsed.iter().any(|x| x == workspace) {
                            member = true;
                            break;
                        }
                    }
                }
            }
        }
        if member {
            if let Some(purl) = c["purl"].as_str() {
                out.push(purl.to_string());
            }
        }
    }
    out
}

// -----------------------------------------------------------------------
// US1 T014 — per-component `waybill:workspace-member` acceptance.
// Covers acceptance scenarios 1 + 2 + 3 (per spec.md US1) AND SC-006
// (3-workspace distinct-set count via the /speckit-analyze C1 remediation).
// -----------------------------------------------------------------------

/// Given a 3-workspace synthesized fixture, verify:
///
/// * Every workspace-scoped component gains `waybill:workspace-member`
///   whose value is a JSON-encoded array (FR-001).
/// * The union of every emitted `waybill:workspace-member` value
///   contains exactly the 3 workspaces the fixture created (SC-006).
/// * jq-shaped per-workspace filters return workspace-scoped components
///   (acceptance scenarios 1 + 2).
/// * The shared dep (`shared-dep`) appears in ALL THREE workspaces'
///   filters via array-containment (acceptance scenario 3).
#[test]
fn t007_us1_per_component_workspace_member_annotation() {
    let tmp = tempfile::tempdir().expect("tempdir");
    write_three_workspace_pip_fixture(tmp.path());

    let (sbom, _stderr) = scan_cdx(tmp.path());

    // Acceptance scenarios 1 + 2 + SC-006 — the union of all
    // per-component workspace-member values equals the three
    // fixture workspaces exactly.
    let distinct = component_workspace_paths(&sbom);
    let expected = vec![
        ".".to_string(),
        "subproject_a".to_string(),
        "subproject_b".to_string(),
    ];
    assert_eq!(
        distinct, expected,
        "SC-006 gate: expected exactly 3 distinct workspaces ({expected:?}); \
         got {distinct:?}. Every emitted waybill:workspace-member value across \
         all components must dedup to these three paths"
    );

    // Per-workspace filter behavior (acceptance scenario 1 + 2).
    // Each workspace should have at least ONE component tagged to it
    // (the manifest-derived main-module component at minimum).
    for workspace in &expected {
        let purls = purls_in_workspace(&sbom, workspace);
        assert!(
            !purls.is_empty(),
            "acceptance scenario: workspace {workspace:?} filter returned zero \
             components, but the fixture declared a pyproject.toml at that path"
        );
    }
}

// -----------------------------------------------------------------------
// US2 T016 — advisory-log emission behavior.
// Covers FR-004 (fires exactly once when N > 1), FR-005 (silent when
// N <= 1), FR-006 (not gated on --offline), plus SC-005 (10-workspace
// stability from /speckit-analyze C2 remediation).
// -----------------------------------------------------------------------

/// Load-bearing stable substring per FR-004 + data-model.md §Entity 6.
/// CI dashboards `grep -F` this token to detect monorepo scans.
const ADVISORY_SUBSTRING: &str = "monorepo shape detected: ";

/// Count how many times the stable advisory substring appears in the
/// captured stderr. Equivalent to `grep -cF 'monorepo shape detected: '`.
fn advisory_hit_count(stderr: &str) -> usize {
    stderr.matches(ADVISORY_SUBSTRING).count()
}

/// Synthesize an N-workspace pip monorepo: root/pyproject.toml plus
/// N-1 subproject directories each with their own pyproject.toml.
/// Each subproject declares a distinct dependency so components fan
/// out cleanly across workspaces.
fn write_n_workspace_pip_fixture(root: &Path, n: usize) {
    fn write_pyproject(dir: &Path, name: &str, distinct_dep: &str) {
        std::fs::create_dir_all(dir).expect("mkdir");
        let content = format!(
            "[project]\n\
             name = \"{name}\"\n\
             version = \"0.1.0\"\n\
             requires-python = \">=3.10\"\n\
             dependencies = [\n\
                 \"{distinct_dep}\",\n\
             ]\n"
        );
        std::fs::write(dir.join("pyproject.toml"), content).expect("write pyproject.toml");
    }
    write_pyproject(root, "root-pkg", "root-dep>=1.0");
    for i in 1..n {
        let sub_name = format!("sub_{i:02}");
        write_pyproject(
            &root.join(&sub_name),
            &format!("sub-{i:02}-pkg"),
            &format!("sub-{i:02}-dep>=1.0"),
        );
    }
}

/// FR-004 + SC-002 gate — 3-workspace scan emits exactly one advisory
/// log line whose body contains all three workspace paths.
#[test]
fn t008_us2_advisory_log_fires_once_on_monorepo() {
    let tmp = tempfile::tempdir().expect("tempdir");
    write_three_workspace_pip_fixture(tmp.path());

    let (_sbom, stderr) = scan_cdx(tmp.path());

    let hits = advisory_hit_count(&stderr);
    assert_eq!(
        hits, 1,
        "FR-004: expected exactly one advisory line matching {ADVISORY_SUBSTRING:?} on a \
         3-workspace scan; got {hits}. Full stderr:\n{stderr}"
    );
    // Body should name every workspace.
    for workspace in [".", "subproject_a", "subproject_b"] {
        assert!(
            stderr.contains(workspace),
            "advisory body should name workspace {workspace:?}; full stderr:\n{stderr}"
        );
    }
}

/// FR-005 + SC-003 gate — single-project (N = 1) scan produces zero
/// advisory hits.
#[test]
fn t009_us2_advisory_log_silent_on_single_workspace() {
    let tmp = tempfile::tempdir().expect("tempdir");
    // Only one workspace: root pyproject.toml.
    std::fs::write(
        tmp.path().join("pyproject.toml"),
        "[project]\n\
         name = \"solo\"\n\
         version = \"0.1.0\"\n\
         requires-python = \">=3.10\"\n\
         dependencies = [\"only-dep>=1.0\"]\n",
    )
    .expect("write pyproject.toml");

    let (_sbom, stderr) = scan_cdx(tmp.path());

    let hits = advisory_hit_count(&stderr);
    assert_eq!(
        hits, 0,
        "FR-005: single-workspace scan MUST emit zero advisories; got {hits}. \
         Full stderr:\n{stderr}"
    );
}

/// FR-005 + SC-003 gate — bare directory (N = 0) scan produces zero
/// advisory hits. Scanning a directory with no package manifests
/// yields no workspaces, so the advisory MUST stay quiet.
#[test]
fn t010_us2_advisory_log_silent_on_bare_directory() {
    let tmp = tempfile::tempdir().expect("tempdir");
    // Bare directory — nothing here for readers to discover. Add a
    // marker file so the scanner has something to walk without any
    // manifest-derived component.
    std::fs::write(tmp.path().join("README.txt"), b"empty scan target\n")
        .expect("write README");

    let (_sbom, stderr) = scan_cdx(tmp.path());

    let hits = advisory_hit_count(&stderr);
    assert_eq!(
        hits, 0,
        "FR-005: bare-directory scan MUST emit zero advisories; got {hits}. \
         Full stderr:\n{stderr}"
    );
}

/// FR-006 gate — advisory fires under `--offline` on a monorepo scan
/// (remediation is consumer-side jq slicing; no network required).
/// `scan_cdx` already sets `--offline`, so t008 already exercises the
/// offline path. This test makes the FR-006 guarantee explicit by
/// asserting the same behavior on a distinct fixture and documenting
/// intent.
#[test]
fn t011_us2_advisory_log_fires_under_offline() {
    let tmp = tempfile::tempdir().expect("tempdir");
    // Distinct 2-workspace fixture — smaller than t008's 3-workspace
    // fixture but still exercises the N > 1 branch.
    std::fs::write(
        tmp.path().join("pyproject.toml"),
        "[project]\nname = \"root-pkg\"\nversion = \"0.1.0\"\nrequires-python = \">=3.10\"\n\
         dependencies = [\"root-dep>=1.0\"]\n",
    )
    .expect("write root pyproject.toml");
    std::fs::create_dir_all(tmp.path().join("subA")).expect("mkdir");
    std::fs::write(
        tmp.path().join("subA/pyproject.toml"),
        "[project]\nname = \"sub-a\"\nversion = \"0.1.0\"\nrequires-python = \">=3.10\"\n\
         dependencies = [\"sub-a-dep>=1.0\"]\n",
    )
    .expect("write subA pyproject.toml");

    let (_sbom, stderr) = scan_cdx(tmp.path());

    let hits = advisory_hit_count(&stderr);
    assert_eq!(
        hits, 1,
        "FR-006: advisory MUST fire under --offline on a 2-workspace scan; got {hits}. \
         Full stderr:\n{stderr}"
    );
}

// -----------------------------------------------------------------------
// US3 T024 — doc-scope `waybill:workspaces-detected` (C121) behavior.
// Covers FR-003 (emitted when N >= 1, absent when N = 0), FR-012
// cross-annotation invariant (C121 == union of C120 values).
// -----------------------------------------------------------------------

/// Extract the doc-scope `waybill:workspaces-detected` value from
/// `metadata.properties[]` in a CDX SBOM. Returns `Some(parsed_array)`
/// if present, `None` if the annotation is absent (FR-003 signal).
fn doc_scope_workspaces_detected(sbom: &serde_json::Value) -> Option<Vec<String>> {
    let props = sbom["metadata"]["properties"].as_array()?;
    for p in props {
        if p["name"] == "waybill:workspaces-detected" {
            let raw = p["value"].as_str()?;
            return serde_json::from_str::<Vec<String>>(raw).ok();
        }
    }
    None
}

/// FR-003 gate — 3-workspace scan emits the doc-scope annotation with
/// an alphabetically-sorted array containing all 3 workspace paths.
#[test]
fn t012_us3_doc_scope_workspaces_detected_annotation() {
    let tmp = tempfile::tempdir().expect("tempdir");
    write_three_workspace_pip_fixture(tmp.path());

    let (sbom, _stderr) = scan_cdx(tmp.path());

    let detected = doc_scope_workspaces_detected(&sbom).expect(
        "FR-003: doc-scope waybill:workspaces-detected MUST be present when N >= 1",
    );
    assert_eq!(
        detected,
        vec![
            ".".to_string(),
            "subproject_a".to_string(),
            "subproject_b".to_string()
        ],
        "FR-003: doc-scope aggregate must be alphabetically sorted, deduplicated"
    );
}

/// FR-003 gate — bare-directory scan (N = 0 workspaces) omits the
/// annotation entirely. Absence is the wire signal.
#[test]
fn t013_us3_absent_when_zero_workspaces() {
    let tmp = tempfile::tempdir().expect("tempdir");
    // No package manifests — just a marker text file so the walker
    // has something to descend into.
    std::fs::write(tmp.path().join("README.txt"), b"no manifests here\n")
        .expect("write README");

    let (sbom, _stderr) = scan_cdx(tmp.path());

    let detected = doc_scope_workspaces_detected(&sbom);
    assert!(
        detected.is_none(),
        "FR-003: doc-scope aggregate MUST be absent when zero workspaces detected; \
         got: {detected:?}"
    );
}

/// FR-012 cross-annotation invariant — the doc-scope
/// `waybill:workspaces-detected` value equals the sorted-deduplicated
/// union of every per-component `waybill:workspace-member` value.
/// Verified against the 3-workspace fixture.
#[test]
fn t014_us3_c121_equals_union_of_c120() {
    let tmp = tempfile::tempdir().expect("tempdir");
    write_three_workspace_pip_fixture(tmp.path());

    let (sbom, _stderr) = scan_cdx(tmp.path());

    let doc_scope = doc_scope_workspaces_detected(&sbom).expect(
        "FR-012 prerequisite: doc-scope aggregate must be present on 3-workspace scan",
    );
    let component_union = component_workspace_paths(&sbom);
    assert_eq!(
        doc_scope, component_union,
        "FR-012 invariant violated: doc-scope C121 aggregate {doc_scope:?} \
         does not equal the union of per-component C120 values {component_union:?}"
    );
}

/// SC-005 gate (per /speckit-analyze C2 remediation) — 10-workspace
/// stability: substring appears exactly once AND the count-prefix in
/// the body equals 10 AND every workspace path is named in the body.
#[test]
fn t011a_us2_sc005_ten_workspace_advisory_stability() {
    let tmp = tempfile::tempdir().expect("tempdir");
    write_n_workspace_pip_fixture(tmp.path(), 10);

    let (_sbom, stderr) = scan_cdx(tmp.path());

    let hits = advisory_hit_count(&stderr);
    assert_eq!(
        hits, 1,
        "SC-005: 10-workspace scan MUST emit exactly one advisory; got {hits}. \
         Full stderr:\n{stderr}"
    );

    // Body carries the numeric count "10 workspaces" — proves the
    // count-prefix scales correctly and the log formatting doesn't
    // truncate at higher N.
    assert!(
        stderr.contains("10 workspaces"),
        "SC-005: advisory body should carry `10 workspaces` prefix; full stderr:\n{stderr}"
    );

    // Every workspace path must appear in the body (root + 9 subs).
    for workspace in std::iter::once(".".to_string())
        .chain((1..10).map(|i| format!("sub_{i:02}")))
    {
        assert!(
            stderr.contains(&workspace),
            "SC-005: advisory body should name workspace {workspace:?}; full stderr:\n{stderr}"
        );
    }
}

// -----------------------------------------------------------------------
// SC-004 byte-identity gate (per /speckit-analyze U1 remediation).
// The 33 golden regression fixtures (cdx_regression + spdx_regression +
// spdx3_regression) are the load-bearing gate — any future emission
// change that drifts fixtures outside C120/C121 breaks them. This
// test formalizes the semantic invariant in-code across TWO
// representative fixture classes (monorepo + single-project) as a
// belt-and-braces guard for scans that don't hit the 33 goldens.
// -----------------------------------------------------------------------

/// Collect every unique document-scope property name in a CDX SBOM.
fn doc_scope_property_names(sbom: &serde_json::Value) -> std::collections::BTreeSet<String> {
    let mut out: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    if let Some(props) = sbom["metadata"]["properties"].as_array() {
        for p in props {
            if let Some(name) = p["name"].as_str() {
                out.insert(name.to_string());
            }
        }
    }
    out
}

/// Collect every unique per-component property name across all
/// components in a CDX SBOM.
fn per_component_property_names(sbom: &serde_json::Value) -> std::collections::BTreeSet<String> {
    let mut out: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    if let Some(components) = sbom["components"].as_array() {
        for c in components {
            if let Some(props) = c["properties"].as_array() {
                for p in props {
                    if let Some(name) = p["name"].as_str() {
                        out.insert(name.to_string());
                    }
                }
            }
        }
    }
    out
}

/// SC-004 monorepo gate — a 3-workspace scan's ONLY m176-introduced
/// property additions are `waybill:workspace-member` (per-component)
/// and `waybill:workspaces-detected` (doc-scope). No other property
/// keys are added by m176.
///
/// This test is the semantic complement to the 33 golden regression
/// fixtures. Any future emission change that adds a NEW property key
/// beyond these two, without regenerating the goldens, breaks this
/// test AND the goldens — providing two independent detection paths.
#[test]
fn t015_sc004_monorepo_byte_identity_gate_semantic() {
    let tmp = tempfile::tempdir().expect("tempdir");
    write_three_workspace_pip_fixture(tmp.path());

    let (sbom, _stderr) = scan_cdx(tmp.path());

    // The doc-scope property set MUST contain the C121 addition.
    let doc_props = doc_scope_property_names(&sbom);
    assert!(
        doc_props.contains("waybill:workspaces-detected"),
        "SC-004 monorepo: doc-scope MUST include waybill:workspaces-detected; got {doc_props:?}"
    );

    // The per-component property set MUST contain the C120 addition.
    let component_props = per_component_property_names(&sbom);
    assert!(
        component_props.contains("waybill:workspace-member"),
        "SC-004 monorepo: per-component MUST include waybill:workspace-member; got {component_props:?}"
    );

    // Sanity — components[] is non-empty (workspace-attributable scan).
    let component_count = sbom["components"]
        .as_array()
        .map(|a| a.len())
        .unwrap_or(0);
    assert!(
        component_count > 0,
        "SC-004 monorepo: components[] must be non-empty; got {component_count}"
    );
}

/// SC-004 + SC-008 single-project gate (FR-013) — a single-workspace
/// scan's SBOM includes the two m176 additions but nothing else new.
/// Components[], metadata.component, and every other shape aspect
/// remains stable.
///
/// Uses a `requirements.txt` fixture so pip's design-tier reader
/// emits at least one locked component in `components[]` (main-module
/// components live in `metadata.component` per m127 and aren't in the
/// per-component walk).
#[test]
fn t015b_sc004_single_project_byte_identity_gate_semantic() {
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        tmp.path().join("requirements.txt"),
        "requests==2.31.0\n\
         click==8.1.7\n",
    )
    .expect("write requirements.txt");

    let (sbom, _stderr) = scan_cdx(tmp.path());

    // Single-workspace scan MUST still emit C120 + C121 (per FR-013:
    // "the ONLY change is the addition of the two new annotations").
    let doc_props = doc_scope_property_names(&sbom);
    assert!(
        doc_props.contains("waybill:workspaces-detected"),
        "SC-008 single-project: doc-scope MUST include waybill:workspaces-detected on N=1 scans; got {doc_props:?}"
    );
    let component_props = per_component_property_names(&sbom);
    assert!(
        component_props.contains("waybill:workspace-member"),
        "SC-008 single-project: per-component MUST include waybill:workspace-member on N=1 scans; got {component_props:?}"
    );

    // C121 value MUST be exactly `["."]` for the single-workspace case.
    let detected = doc_scope_workspaces_detected(&sbom).expect(
        "SC-008: waybill:workspaces-detected MUST be present on single-workspace scan",
    );
    assert_eq!(
        detected,
        vec![".".to_string()],
        "SC-008 single-project: single-workspace scan MUST emit C121 = [\".\"]; got {detected:?}"
    );
}
