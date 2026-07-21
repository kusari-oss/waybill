//! Integration tests for milestone 068 — pip source-tree main-module
//! emission for PEP 621 `pyproject.toml` roots.

use std::path::{Path, PathBuf};
use std::process::Command;

fn cli_local_fixture(sub: &str) -> PathBuf {
    // Milestone 090: waybill-cli/tests/fixtures/<sub> dirs moved to
    // waybill-test-fixtures repo; resolve via WAYBILL_FIXTURES_DIR.
    PathBuf::from(env!("WAYBILL_FIXTURES_DIR")).join(sub)
}

fn scan_path(path: &Path, format: &str) -> serde_json::Value {
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
        .arg(format)
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

/// US1 AS#1 + SC-001: PEP 621 single-project scan emits a main-module
/// in CDX `metadata.component`.
#[test]
fn scan_pip_pep621_emits_main_module_in_metadata_component() {
    let path = cli_local_fixture("pip-pyproject-pep621");
    let cdx = scan_path(&path, "cyclonedx-json");
    let meta = &cdx["metadata"]["component"];
    assert_eq!(meta["type"].as_str(), Some("application"));
    assert_eq!(meta["purl"].as_str(), Some("pkg:pypi/my-pkg@1.0.0"));
    assert_eq!(meta["name"].as_str(), Some("my_pkg"));
    let role = meta["properties"]
        .as_array()
        .expect("metadata.component.properties")
        .iter()
        .find(|p| p["name"].as_str() == Some("waybill:component-role"));
    assert_eq!(
        role.and_then(|p| p["value"].as_str()),
        Some("main-module")
    );
}

/// US1 AS#2 + SC-002: PEP 503 name normalization in PURL.
/// `my_pkg` (manifest) → `pkg:pypi/my-pkg@<v>` (PURL with hyphen).
#[test]
fn scan_pip_pep503_name_normalization_in_purl() {
    let path = cli_local_fixture("pip-pyproject-pep621");
    let cdx = scan_path(&path, "cyclonedx-json");
    // PEP 503: lowercase + underscore→hyphen. `name` field stays
    // verbatim (`my_pkg`); the PURL normalizes (`my-pkg`).
    assert_eq!(
        cdx["metadata"]["component"]["purl"].as_str(),
        Some("pkg:pypi/my-pkg@1.0.0"),
        "PEP 503 name normalization: manifest `my_pkg` → PURL `my-pkg`"
    );
}

/// US1 AS#3: `dynamic = ["version"]` → `0.0.0-unknown` placeholder.
#[test]
fn scan_pip_dynamic_version_uses_placeholder() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("pyproject.toml"),
        r#"
[project]
name = "dyn-app"
dynamic = ["version"]
"#,
    )
    .unwrap();
    let cdx = scan_path(dir.path(), "cyclonedx-json");
    assert_eq!(
        cdx["metadata"]["component"]["purl"].as_str(),
        Some("pkg:pypi/dyn-app@0.0.0-unknown"),
        "dynamic version → 0.0.0-unknown placeholder per FR-001"
    );
}

/// US1 AS#4 + FR-002: `[tool.poetry]`-only manifest skips main-module
/// emission. `documentDescribes` falls through to a synthetic root
/// (no `pkg:pypi/poetry-only-app` package emitted).
#[test]
fn scan_pip_poetry_only_skips_main_module() {
    let path = cli_local_fixture("pip-pyproject-poetry-only");
    let spdx = scan_path(&path, "spdx-2.3-json");
    let app_pkgs: Vec<&serde_json::Value> = spdx["packages"]
        .as_array()
        .map(|a| a.as_slice())
        .unwrap_or(&[])
        .iter()
        .filter(|p| {
            p["primaryPackagePurpose"].as_str() == Some("APPLICATION")
        })
        .collect();
    assert_eq!(
        app_pkgs.len(),
        0,
        "Poetry-only manifest must NOT emit a main-module per FR-002. \
         Got APPLICATION-purpose packages: {app_pkgs:#?}"
    );
    // Verify no pkg:pypi/poetry-only-app PURL anywhere.
    let any_poetry_main = spdx["packages"]
        .as_array()
        .map(|a| {
            a.iter().any(|p| {
                p["externalRefs"]
                    .as_array()
                    .map(|refs| {
                        refs.iter().any(|r| {
                            r["referenceLocator"]
                                .as_str()
                                .is_some_and(|s| s.starts_with("pkg:pypi/poetry-only-app"))
                        })
                    })
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false);
    assert!(
        !any_poetry_main,
        "no pkg:pypi/poetry-only-app should appear when [tool.poetry] is the only schema"
    );
}

/// FR-011: editable-install merge — when a venv `.dist-info` shares
/// the same PURL as a Phase-A main-module, the venv evidence
/// (`sbom_tier: deployed`) wins while Phase A's C40 tag is
/// preserved on the merged entry.
#[test]
fn scan_pip_editable_install_merges_venv_evidence() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    // PEP 621 manifest declaring name + version.
    std::fs::write(
        root.join("pyproject.toml"),
        r#"
[project]
name = "editable_pkg"
version = "1.0.0"
"#,
    )
    .unwrap();
    // Synthetic venv with a matching .dist-info to simulate
    // `pip install -e .`. The Tier-1 venv reader scans
    // `<rootfs>/<venv>/lib/python*/site-packages/<name>-<version>.dist-info/METADATA`.
    let dist_info = root
        .join("venv/lib/python3.11/site-packages/editable_pkg-1.0.0.dist-info");
    std::fs::create_dir_all(&dist_info).unwrap();
    std::fs::write(
        dist_info.join("METADATA"),
        "Metadata-Version: 2.1\nName: editable_pkg\nVersion: 1.0.0\n",
    )
    .unwrap();
    let cdx = scan_path(root, "cyclonedx-json");
    // The merged main-module should be in metadata.component (since
    // there's only one main-module, it gets promoted per the
    // C40-tag-driven hooks). Verify both signals: C40 tag (Phase A)
    // and `waybill:sbom-tier: deployed` (venv evidence wins per
    // FR-011).
    let meta = &cdx["metadata"]["component"];
    assert_eq!(meta["purl"].as_str(), Some("pkg:pypi/editable-pkg@1.0.0"));
    let props = meta["properties"].as_array().expect("properties array");
    let role = props
        .iter()
        .find(|p| p["name"].as_str() == Some("waybill:component-role"));
    assert_eq!(
        role.and_then(|p| p["value"].as_str()),
        Some("main-module"),
        "FR-011: Phase A's C40 tag must survive the merge"
    );
    let tier = props
        .iter()
        .find(|p| p["name"].as_str() == Some("waybill:sbom-tier"));
    assert_eq!(
        tier.and_then(|p| p["value"].as_str()),
        Some("deployed"),
        "FR-011: venv evidence wins for sbom_tier (deployed beats source)"
    );
}
