//! Milestone 670 PR-1 — integration tests for pyproject.toml-declared
//! dependency emission (the m018 policy reversal).
//!
//! These tests validate SC-001 acceptance criteria against synthetic
//! fixtures that reproduce the shapes surfaced by the 2026-08-31 sweep:
//! - `pyproject-multi-declared-deps`  → markitdown/OctoPrint case
//!   (PEP 621 `[project.dependencies]` with many entries, no lockfile)
//! - `pyproject-poetry-legacy`        → Poetry-legacy projects that use
//!   `[tool.poetry]` sections instead of PEP 621
//! - `pyproject-pep735-groups`        → PEP 735 `[dependency-groups]`
//!
//! Fixtures are synthetic (per `feedback_fixture_synthetic_package_names`
//! memory): package names use the `waybill-fixture-*` prefix so Kusari
//! Inspector's advisory scan on downstream doesn't fire on real
//! coordinates. SC-001's ≥30 target for real markitdown is verified
//! separately by T019's ad-hoc sweep script.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;
use std::time::Instant;

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

/// Locked m236 reason string reserved for m670 PR-1 at
/// `specs/236-unresolved-reason/contracts/per-reader-strings.md`.
const MANIFEST_UNRESOLVED_REASON: &str =
    "declared in pyproject.toml; no uv.lock / poetry.lock / Pipfile.lock fallback";

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
        .arg("--output")
        .arg(&out_path)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&result.stderr)
    );
    let bytes = std::fs::read(&out_path).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn pypi_components(doc: &Value) -> Vec<&Value> {
    doc["components"]
        .as_array()
        .expect("components array")
        .iter()
        .filter(|c| {
            c["purl"]
                .as_str()
                .is_some_and(|p| p.starts_with("pkg:pypi/"))
        })
        .collect()
}

fn prop_value<'a>(c: &'a Value, key: &str) -> Option<&'a str> {
    c["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"].as_str() == Some(key))?
        .get("value")?
        .as_str()
}

// -----------------------------------------------------------------------
// US1 core — PEP 621 [project.dependencies] emit as design-tier
// -----------------------------------------------------------------------

#[test]
fn m670_pep621_multi_declared_deps_all_emit_as_design_tier() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("pyproject.toml"),
        r#"
[project]
name = "waybill-fixture-pypi-multi"
version = "1.0.0"
dependencies = [
    "waybill-fixture-pypi-alpha>=1.0",
    "waybill-fixture-pypi-beta>=2.0,<3",
    "waybill-fixture-pypi-gamma",
    "waybill-fixture-pypi-delta==4.2.0",
    "waybill-fixture-pypi-epsilon",
]
"#,
    )
    .unwrap();

    let doc = run_scan(tmp.path());
    let pypi = pypi_components(&doc);
    // 5 declared deps; main-module goes to metadata.component so pypi
    // components in components[] = 5 (SC-001 shape).
    assert_eq!(pypi.len(), 5, "5 declared deps should emit as components");

    for c in &pypi {
        assert_eq!(
            c["version"].as_str(),
            Some("unresolved"),
            "m670 design-tier components emit version=unresolved when no lockfile"
        );
        assert_eq!(
            prop_value(c, "waybill:unresolved-reason"),
            Some(MANIFEST_UNRESOLVED_REASON),
            "every m670 manifest-declared dep must carry the locked m236 reason"
        );
        // SC-005 evidence: source_file_paths must contain ≥ 1 entry.
        let source_files = prop_value(c, "waybill:source-files")
            .expect("m670: every manifest-declared dep has source-files evidence");
        assert!(
            !source_files.is_empty() && source_files != "[]",
            "SC-005: source_file_paths must be non-empty for component {}",
            c["name"]
        );
    }
}

// -----------------------------------------------------------------------
// US1 constraint preservation
// -----------------------------------------------------------------------

#[test]
fn m670_version_constraint_annotation_populated_when_present() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("pyproject.toml"),
        r#"
[project]
name = "waybill-fixture-pypi-constraints"
version = "1.0.0"
dependencies = [
    "waybill-fixture-pypi-pinned>=1.0,<2",
    "waybill-fixture-pypi-unpinned",
]
"#,
    )
    .unwrap();

    let doc = run_scan(tmp.path());
    let pypi = pypi_components(&doc);
    let pinned = pypi
        .iter()
        .find(|c| c["name"].as_str() == Some("waybill-fixture-pypi-pinned"))
        .expect("pinned dep should be emitted");
    assert_eq!(
        prop_value(pinned, "waybill:version-constraint"),
        Some(">=1.0,<2"),
        "constrained-but-unpinned deps carry the raw constraint string"
    );
    let unpinned = pypi
        .iter()
        .find(|c| c["name"].as_str() == Some("waybill-fixture-pypi-unpinned"))
        .expect("unpinned dep should be emitted");
    assert_eq!(
        prop_value(unpinned, "waybill:version-constraint"),
        None,
        "no version-constraint annotation on truly unpinned deps"
    );
}

// -----------------------------------------------------------------------
// US1 optional-scope classification
// -----------------------------------------------------------------------

#[test]
fn m670_optional_dependencies_emit_with_scope_annotation() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("pyproject.toml"),
        r#"
[project]
name = "waybill-fixture-pypi-optional"
version = "1.0.0"
dependencies = ["waybill-fixture-pypi-runtime>=1"]

[project.optional-dependencies]
docs = ["waybill-fixture-pypi-docs-alpha"]
test = ["waybill-fixture-pypi-test-alpha"]
"#,
    )
    .unwrap();

    let doc = run_scan(tmp.path());
    let pypi = pypi_components(&doc);
    assert_eq!(pypi.len(), 3);

    let docs_dep = pypi
        .iter()
        .find(|c| c["name"].as_str() == Some("waybill-fixture-pypi-docs-alpha"))
        .expect("docs optional dep emitted");
    assert_eq!(
        prop_value(docs_dep, "waybill:optional-derivation"),
        Some("pip-pyproject-optional-dependencies:docs"),
        "optional-dependencies group name preserved in derivation"
    );
    let test_dep = pypi
        .iter()
        .find(|c| c["name"].as_str() == Some("waybill-fixture-pypi-test-alpha"))
        .expect("test optional dep emitted");
    assert_eq!(
        prop_value(test_dep, "waybill:optional-derivation"),
        Some("pip-pyproject-optional-dependencies:test")
    );
    let runtime_dep = pypi
        .iter()
        .find(|c| c["name"].as_str() == Some("waybill-fixture-pypi-runtime"))
        .expect("runtime dep emitted");
    assert_eq!(
        prop_value(runtime_dep, "waybill:optional-derivation"),
        None,
        "Runtime deps have no optional-derivation annotation"
    );
}

// -----------------------------------------------------------------------
// US1 Poetry-legacy support (T003 pyproject_declared_deps + T005 main-module)
// -----------------------------------------------------------------------

#[test]
fn m670_poetry_legacy_emits_main_module_and_declared_deps() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("pyproject.toml"),
        r#"
[tool.poetry]
name = "waybill-fixture-poetry-legacy"
version = "2.0.0"

[tool.poetry.dependencies]
python = "^3.11"
waybill-fixture-pypi-legacy-alpha = "^1.0"
waybill-fixture-pypi-legacy-beta = { version = "^2.0" }

[tool.poetry.dev-dependencies]
waybill-fixture-pypi-legacy-devdep = "^3"

[tool.poetry.group.docs.dependencies]
waybill-fixture-pypi-legacy-docdep = "^4"
"#,
    )
    .unwrap();

    let doc = run_scan(tmp.path());
    // Main-module goes to metadata.component — assert it exists (T005).
    assert_eq!(
        doc["metadata"]["component"]["name"].as_str(),
        Some("waybill-fixture-poetry-legacy"),
        "T005: Poetry-legacy pyproject.tomls emit a main-module (post-m670)"
    );
    assert_eq!(
        doc["metadata"]["component"]["version"].as_str(),
        Some("2.0.0"),
        "T005: main-module version read from [tool.poetry].version"
    );
    // Declared deps in components[] — 4 (python is skipped per T003).
    let pypi = pypi_components(&doc);
    assert_eq!(pypi.len(), 4, "python skipped; 4 Poetry-legacy deps emit");
    for name in [
        "waybill-fixture-pypi-legacy-alpha",
        "waybill-fixture-pypi-legacy-beta",
        "waybill-fixture-pypi-legacy-devdep",
        "waybill-fixture-pypi-legacy-docdep",
    ] {
        assert!(
            pypi.iter().any(|c| c["name"].as_str() == Some(name)),
            "expected {name} emitted from Poetry-legacy sections"
        );
    }
    assert!(
        !pypi.iter().any(|c| c["name"].as_str() == Some("python")),
        "T003: `python` under [tool.poetry.dependencies] must be skipped"
    );
}

// -----------------------------------------------------------------------
// US1 PEP 735 dependency-groups
// -----------------------------------------------------------------------

#[test]
fn m670_pep735_dependency_groups_emit_with_scope() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("pyproject.toml"),
        r#"
[project]
name = "waybill-fixture-pep735"
version = "1.0.0"

[dependency-groups]
lint = ["waybill-fixture-pypi-linter"]
"#,
    )
    .unwrap();

    let doc = run_scan(tmp.path());
    let pypi = pypi_components(&doc);
    assert_eq!(pypi.len(), 1);
    let linter = &pypi[0];
    assert_eq!(linter["name"].as_str(), Some("waybill-fixture-pypi-linter"));
    assert_eq!(
        prop_value(linter, "waybill:optional-derivation"),
        Some("pep-735-dependency-groups:lint")
    );
}

// -----------------------------------------------------------------------
// SC-007 performance envelope (relaxed from spec's 549ms markitdown target)
// -----------------------------------------------------------------------
//
// The spec's SC-007 targets a wall-clock ≤ 549ms for kusari-sandbox/
// test-markitdown (28MB, 8 pyproject.toml files across 4 sub-projects).
// This test uses a small synthetic fixture so the wall-clock envelope
// is much tighter. If m670's new pyproject_declared_deps regresses to
// O(N²) behavior on many deps, this ceiling catches it early.

#[test]
fn m670_scan_envelope_under_two_seconds_for_synthetic_fixture() {
    let tmp = tempfile::tempdir().unwrap();
    // 20-dep fixture — larger than the other tests to catch scaling
    // regressions but well under the 549ms SC-007 spec-cited ceiling.
    let deps: Vec<String> = (0..20)
        .map(|i| format!(r#"    "waybill-fixture-pypi-perf-{i:02}>=1.0","#))
        .collect();
    let manifest = format!(
        r#"
[project]
name = "waybill-fixture-pypi-perf"
version = "1.0.0"
dependencies = [
{}
]
"#,
        deps.join("\n")
    );
    std::fs::write(tmp.path().join("pyproject.toml"), manifest).unwrap();

    let start = Instant::now();
    let doc = run_scan(tmp.path());
    let elapsed = start.elapsed();

    let pypi = pypi_components(&doc);
    assert_eq!(pypi.len(), 20);
    // 2s ceiling is generous for a 20-dep synthetic fixture. If this
    // trips consistently, investigate pyproject_declared_deps scaling.
    assert!(
        elapsed.as_secs_f64() < 2.0,
        "m670 pyproject scan of 20 declared deps took {elapsed:?} — investigate regression"
    );
}

// -----------------------------------------------------------------------
// Milestone 670 T014: cpython-shaped fixture coverage
// -----------------------------------------------------------------------
//
// Reproduces the multi-file requirements-file patterns real cpython
// ships (Doc/requirements.txt case-insensitive parent-dir, Tools/
// requirements-dev.txt filename signal, PEP 508 direct-URL in
// Doc/requirements.txt). Verifies T012 + T013 emit the correct
// annotations end-to-end on cpython-shaped inputs. Package names are
// synthetic per `feedback_fixture_synthetic_package_names`.

/// Create a cpython-shaped fixture tree at `root`:
/// - `Doc/requirements.txt` (capital-D parent-dir, docs scope)
/// - `Tools/requirements-dev.txt` (filename-signal dev scope)
/// - `Tools/requirements-hypothesis.txt` (no scope signal → Main)
fn write_cpython_shaped_fixture(root: &Path) {
    std::fs::create_dir_all(root.join("Doc")).unwrap();
    std::fs::create_dir_all(root.join("Tools")).unwrap();
    std::fs::write(
        root.join("Doc/requirements.txt"),
        // Mirror real cpython's Doc/requirements.txt patterns: pinned
        // deps, PEP 508 direct-URL entry (T012), and constraints ref.
        r#"# Doc build requirements
waybill-fixture-pypi-sphinx<9.0.0
waybill-fixture-pypi-blurb
waybill-fixture-pypi-linklint

# T012: PEP 508 direct-URL — archive with no rev
waybill-fixture-pypi-pygments @ https://example.test/archive/2cad2642058441b59782a6a18f03c98c42d081f1.tar.gz
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("Tools/requirements-dev.txt"),
        r#"# T013: filename-signal dev-scope
waybill-fixture-pypi-mypy==2.1.0
waybill-fixture-pypi-types-psutil==7.2.2.20260508
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("Tools/requirements-hypothesis.txt"),
        r#"# Tools/ parent-dir doesn't match; filename `hypothesis` isn't in
# the dev/test/docs/ci vocabulary → falls to Main scope
waybill-fixture-pypi-hypothesis==6.155.3
"#,
    )
    .unwrap();
}

#[test]
fn m670_cpython_shape_doc_requirements_gets_docs_scope() {
    // Case-insensitive parent-dir match: `Doc/` → `docs` scope (per
    // T013 classifier; cpython's actual directory is capital-D).
    let tmp = tempfile::tempdir().unwrap();
    write_cpython_shaped_fixture(tmp.path());

    let doc = run_scan(tmp.path());
    let pypi = pypi_components(&doc);

    // Every Doc/-derived component gets scope=docs.
    for name in [
        "waybill-fixture-pypi-sphinx",
        "waybill-fixture-pypi-blurb",
        "waybill-fixture-pypi-linklint",
        "waybill-fixture-pypi-pygments",
    ] {
        let c = pypi
            .iter()
            .find(|c| c["name"].as_str() == Some(name))
            .unwrap_or_else(|| panic!("{name} not emitted"));
        assert_eq!(
            prop_value(c, "waybill:python-req-file-scope"),
            Some("docs"),
            "{name} should carry scope=docs (Doc/ parent-dir signal)",
        );
    }
}

#[test]
fn m670_cpython_shape_tools_dev_requirements_gets_dev_scope() {
    // Filename-signal: `Tools/requirements-dev.txt` → dev scope. This
    // is a filename-derived signal (Tools/ parent-dir doesn't match
    // T013's whitelist; the classifier falls through to filename).
    let tmp = tempfile::tempdir().unwrap();
    write_cpython_shaped_fixture(tmp.path());

    let doc = run_scan(tmp.path());
    let pypi = pypi_components(&doc);

    for name in [
        "waybill-fixture-pypi-mypy",
        "waybill-fixture-pypi-types-psutil",
    ] {
        let c = pypi
            .iter()
            .find(|c| c["name"].as_str() == Some(name))
            .unwrap_or_else(|| panic!("{name} not emitted"));
        assert_eq!(
            prop_value(c, "waybill:python-req-file-scope"),
            Some("dev"),
            "{name} should carry scope=dev (requirements-dev.txt filename)",
        );
    }
}

#[test]
fn m670_cpython_shape_tools_hypothesis_falls_to_main_scope() {
    // Filename `requirements-hypothesis.txt` doesn't match any T013
    // scope keyword (dev/test/docs/ci with word-boundary), and
    // Tools/ parent-dir doesn't either. Falls to Main → no annotation,
    // no lifecycle_scope. Regression guard for the substring-vs-word-
    // boundary distinction (see m670_scope_classify_filename_signals
    // in the unit tests for `requirements-special.txt` → not `ci`).
    let tmp = tempfile::tempdir().unwrap();
    write_cpython_shaped_fixture(tmp.path());

    let doc = run_scan(tmp.path());
    let pypi = pypi_components(&doc);
    let hyp = pypi
        .iter()
        .find(|c| c["name"].as_str() == Some("waybill-fixture-pypi-hypothesis"))
        .expect("hypothesis emitted");
    assert_eq!(
        prop_value(hyp, "waybill:python-req-file-scope"),
        None,
        "hypothesis (Main scope) MUST NOT carry the scope annotation",
    );
    // scope field also absent (Runtime by default; not `excluded` — CDX
    // encodes Optional as `scope: excluded` per m179).
    assert!(
        hyp.get("scope").is_none() || hyp["scope"].as_str() != Some("excluded"),
        "Main-scope entries must not emit CDX scope=excluded",
    );
}

#[test]
fn m670_cpython_shape_direct_url_annotation_populated_on_pep508_entry() {
    // Verifies T012 end-to-end: `pkg @ https://.../tar.gz` in
    // Doc/requirements.txt emits `waybill:direct-url-source` annotation
    // with kind=url, resolved_rev=null, url preserved.
    let tmp = tempfile::tempdir().unwrap();
    write_cpython_shaped_fixture(tmp.path());

    let doc = run_scan(tmp.path());
    let pypi = pypi_components(&doc);
    let pygments = pypi
        .iter()
        .find(|c| c["name"].as_str() == Some("waybill-fixture-pypi-pygments"))
        .expect("pygments emitted");

    // Version stays empty (archive URLs have no rev semantics).
    assert!(
        pygments["version"].as_str().is_none()
            || pygments["version"].as_str() == Some("")
            || pygments["version"].as_str() == Some("unresolved"),
        "pygments version should be empty/unresolved for archive URL, got: {:?}",
        pygments["version"]
    );

    let anno = prop_value(pygments, "waybill:direct-url-source")
        .expect("waybill:direct-url-source annotation present");
    let parsed: Value = serde_json::from_str(anno).expect("annotation value is JSON");
    assert_eq!(parsed["kind"].as_str(), Some("url"));
    assert!(parsed["resolved_rev"].is_null());
    assert!(
        parsed["url"].as_str().unwrap().contains("archive/2cad2642"),
        "url preserved verbatim: {}",
        parsed["url"]
    );
}

