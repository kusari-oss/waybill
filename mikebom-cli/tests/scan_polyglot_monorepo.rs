//! End-to-end integration test for polyglot monorepos — a single
//! `mikebom sbom scan --path` invocation over a repo containing both
//! a Python backend and an npm frontend must emit one SBOM carrying
//! components from BOTH ecosystems, with per-ecosystem compositions
//! records where authoritative.
//!
//! This exercises the bounded-depth project-root walks in pip.rs and
//! npm.rs working in parallel against the same scan root.

use std::path::PathBuf;
use std::process::Command;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .join("tests/fixtures/polyglot-monorepo")
}

fn scan(exclude_dev_test: bool) -> serde_json::Value {
    let bin = env!("CARGO_BIN_EXE_mikebom");
    let out_path = tempfile::NamedTempFile::new()
        .expect("tempfile")
        .path()
        .to_path_buf();
    let mut cmd = Command::new(bin);
    cmd.arg("--offline");
    if exclude_dev_test {
        cmd.arg("--exclude-scope").arg("dev,build,test");
    }
    cmd.arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(fixture())
        .arg("--output")
        .arg(&out_path)
        .arg("--no-deep-hash");
    let output = cmd.output().expect("mikebom should run");
    assert!(
        output.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let raw = std::fs::read_to_string(&out_path).expect("read sbom");
    serde_json::from_str(&raw).expect("valid JSON")
}

fn components_by_prefix<'a>(
    sbom: &'a serde_json::Value,
    prefix: &str,
) -> Vec<&'a serde_json::Value> {
    sbom["components"]
        .as_array()
        .expect("components array")
        .iter()
        .filter(|c| {
            c["purl"]
                .as_str()
                .is_some_and(|p| p.starts_with(prefix))
        })
        .collect()
}

#[test]
fn polyglot_monorepo_emits_both_python_and_npm_components() {
    // Milestone 052/part-3: --exclude-scope dev,build,test restores
    // the strict pre-052 prod-only view (vite filtered out).
    let sbom = scan(true);

    let pypi = components_by_prefix(&sbom, "pkg:pypi/");
    let npm = components_by_prefix(&sbom, "pkg:npm/");

    // Backend: 3 design-tier requirements.txt entries (fastapi,
    // uvicorn, httpx) PLUS the milestone-068 main-module emitted
    // from `backend/pyproject.toml` (`[project]` declares
    // `name = "backend"`). Pre-068 expected 3; post-068 expects 4.
    assert_eq!(
        pypi.len(),
        4,
        "backend: expected fastapi + uvicorn + httpx + backend (068 main-module), got {:?}",
        pypi.iter().map(|c| c["name"].as_str()).collect::<Vec<_>>()
    );
    let pypi_names: Vec<&str> = pypi.iter().filter_map(|c| c["name"].as_str()).collect();
    assert!(pypi_names.contains(&"fastapi"));
    assert!(pypi_names.contains(&"uvicorn"));
    assert!(pypi_names.contains(&"httpx"));
    assert!(pypi_names.contains(&"backend"), "milestone 068: pyproject.toml [project] emits a main-module");

    // Frontend: 2 source-tier lockfile entries (vite filtered via
    // --exclude-scope) + the milestone-066 main-module emitted from
    // `frontend/package.json` (`name = "frontend"`).
    assert_eq!(
        npm.len(),
        3,
        "frontend: expected react + axios + frontend (066 main-module), got {:?}",
        npm.iter().map(|c| c["name"].as_str()).collect::<Vec<_>>()
    );
    let npm_names: Vec<&str> = npm.iter().filter_map(|c| c["name"].as_str()).collect();
    assert!(npm_names.contains(&"react"));
    assert!(npm_names.contains(&"axios"));
    assert!(npm_names.contains(&"frontend"), "milestone 066: package.json `name` emits a main-module");
}

#[test]
fn polyglot_monorepo_default_surfaces_both_ecosystems_dev_deps() {
    // Milestone 052/part-3: default mode emits ALL lifecycle scopes.
    let sbom = scan(false);
    let npm = components_by_prefix(&sbom, "pkg:npm/");
    let names: Vec<&str> = npm.iter().filter_map(|c| c["name"].as_str()).collect();
    assert!(
        names.contains(&"vite"),
        "vite dev-dep must appear in default mode (post-052); got {names:?}"
    );
}

#[test]
fn polyglot_monorepo_marks_both_ecosystems_authoritative_when_pinned() {
    // Frontend has a lockfile → npm is source-tier → complete.
    // Backend has pinned requirements.txt (all `==`) → pypi is
    // also source-tier → complete. Pinned requirements.txt is
    // authoritative for the versions it carries (same semantics as
    // a lockfile for the purpose of `aggregate: complete`).
    let sbom = scan(false);
    let compositions = sbom["compositions"].as_array().expect("compositions array");

    let has_complete_for = |ecosystem_prefix: &str| -> bool {
        compositions.iter().any(|r| {
            r["aggregate"].as_str() == Some("complete")
                && r["assemblies"]
                    .as_array()
                    .map(|a| {
                        a.iter().any(|p| {
                            p.as_str().is_some_and(|s| s.starts_with(ecosystem_prefix))
                        })
                    })
                    .unwrap_or(false)
        })
    };

    assert!(
        has_complete_for("pkg:npm/"),
        "npm ecosystem must be aggregate=complete (lockfile-sourced)"
    );
    assert!(
        has_complete_for("pkg:pypi/"),
        "pypi ecosystem must be aggregate=complete when requirements.txt is fully pinned"
    );
}
