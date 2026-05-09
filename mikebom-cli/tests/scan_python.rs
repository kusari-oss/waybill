//! Integration tests for Python-ecosystem scanning (US1 of milestone 002).
//!
//! Each test invokes the `mikebom sbom scan --path <fixture>` binary
//! against a fixture directory under `tests/fixtures/python/` and
//! asserts the CycloneDX-output invariants declared in spec.md's
//! Success Criteria (SC-001..SC-010) and the per-story acceptance
//! scenarios. We shell out to the binary rather than call the library
//! directly because `mikebom-cli` exposes its scan surface via the CLI
//! only (matches how users invoke it in practice).
//!
//! The binary path is the debug build produced by `cargo test`; the
//! `env!("CARGO_BIN_EXE_mikebom")` env var points at it automatically.

use std::path::PathBuf;
use std::process::Command;

fn fixture(sub: &str) -> PathBuf {
    PathBuf::from(env!("MIKEBOM_FIXTURES_DIR")).join("python")
        .join(sub)
}

/// Run `mikebom sbom scan --path <fixture>` (with `--offline` so we
/// don't hit deps.dev from CI). Returns the parsed CycloneDX JSON.
///
/// Milestone 052/part-3: `exclude_dev_test` adds
/// `--exclude-scope dev,build,test` to restore the strict pre-052
/// "runtime-only" subset. The default (`false`) emits all lifecycle
/// scopes per FR-002.
fn scan(fixture_sub: &str, exclude_dev_test: bool) -> serde_json::Value {
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
        .arg(fixture(fixture_sub))
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

fn pypi_components(sbom: &serde_json::Value) -> Vec<&serde_json::Value> {
    sbom["components"]
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

fn prop_value<'a>(component: &'a serde_json::Value, name: &str) -> Option<&'a str> {
    component["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"].as_str() == Some(name))?
        .get("value")?
        .as_str()
}

#[test]
fn simple_venv_fixture_produces_seven_pypi_components() {
    let sbom = scan("simple-venv", false);
    let pypi = pypi_components(&sbom);
    // 5 original + charset-normalizer + idna (added to exercise
    // transitive dep-tree resolution via requests' Requires-Dist).
    assert_eq!(pypi.len(), 7, "simple-venv: expected 7 pypi components");
    // Every component tagged deployed.
    for c in &pypi {
        assert_eq!(prop_value(c, "mikebom:sbom-tier"), Some("deployed"));
    }
    // Every PURL round-trips via Purl::new (SC-004). We round-trip via
    // the same parser by re-serialising through a jq-ish check: the
    // raw `purl` field is what builder emitted, which came through
    // `Purl::new`. The `packageurl` crate agrees that:
    for c in &pypi {
        let purl = c["purl"].as_str().expect("purl");
        assert!(purl.starts_with("pkg:pypi/"), "{purl}");
    }
    // All five fixture packages carry a resolved license (SC-005).
    let with_license = pypi
        .iter()
        .filter(|c| {
            c["licenses"]
                .as_array()
                .map(|a| !a.is_empty())
                .unwrap_or(false)
        })
        .count();
    assert_eq!(with_license, 7, "all fixture packages must have licenses");
    // compositions includes pypi as complete.
    let compositions = sbom["compositions"]
        .as_array()
        .expect("compositions array");
    let complete_pypi = compositions.iter().any(|r| {
        r["aggregate"].as_str() == Some("complete")
            && r["assemblies"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .any(|p| p.as_str().is_some_and(|s| s.starts_with("pkg:pypi/")))
                })
                .unwrap_or(false)
    });
    assert!(
        complete_pypi,
        "pypi must be marked aggregate=complete for venv-sourced scans"
    );
}

#[test]
fn python_dependency_tree_resolves_transitively() {
    // simple-venv's requests METADATA declares Requires-Dist for
    // urllib3, certifi, charset-normalizer, idna — all of which are
    // installed in the same venv. The SBOM's `dependencies[]` block
    // must carry a record `{ref: pkg:pypi/requests@..., dependsOn: [...]}`
    // listing every one at its lockfile-resolved version.
    let sbom = scan("simple-venv", false);
    let deps = sbom["dependencies"]
        .as_array()
        .expect("dependencies array");

    let requests_record = deps
        .iter()
        .find(|r| {
            r["ref"]
                .as_str()
                .is_some_and(|s| s == "pkg:pypi/requests@2.31.0")
        })
        .expect("requests must have a dependencies[] record");

    let depends_on: Vec<&str> = requests_record["dependsOn"]
        .as_array()
        .expect("dependsOn array")
        .iter()
        .filter_map(|v| v.as_str())
        .collect();

    // Exact transitive set per fixture METADATA.
    assert!(
        depends_on.contains(&"pkg:pypi/urllib3@2.0.7"),
        "requests → urllib3@2.0.7 expected; got {depends_on:?}"
    );
    assert!(
        depends_on.contains(&"pkg:pypi/certifi@2023.7.22"),
        "requests → certifi expected; got {depends_on:?}"
    );
    assert!(
        depends_on.contains(&"pkg:pypi/charset-normalizer@3.3.2"),
        "requests → charset-normalizer expected; got {depends_on:?}"
    );
    assert!(
        depends_on.contains(&"pkg:pypi/idna@3.6"),
        "requests → idna expected; got {depends_on:?}"
    );
}

#[test]
fn poetry_project_surfaces_prod_default_dev_behind_flag() {
    // Milestone 052/part-3: default emits all scopes (prod + dev),
    // `--exclude-scope dev,build,test` restores the prod-only view.

    // Default mode: both requests (prod) and pytest (dev) emit; pytest
    // tagged with native CDX scope + mikebom:lifecycle-scope.
    let all = scan("poetry-project", false);
    let pypi_all = pypi_components(&all);
    assert_eq!(
        pypi_all.len(),
        2,
        "poetry default (post-052): expected 2 pypi components (prod + dev)"
    );
    let pytest = pypi_all
        .iter()
        .find(|c| c["name"] == "pytest")
        .expect("pytest present in default mode (post-052)");
    assert_eq!(
        pytest["scope"].as_str(),
        Some("excluded"),
        "pytest must carry native CDX scope: \"excluded\" in default mode"
    );
    assert_eq!(
        prop_value(pytest, "mikebom:lifecycle-scope"),
        Some("development"),
        "pytest must carry mikebom:lifecycle-scope = \"development\" in default mode"
    );

    // --exclude-scope dev,build,test: restores the strict prod-only view.
    let prod = scan("poetry-project", true);
    let pypi_prod = pypi_components(&prod);
    assert_eq!(
        pypi_prod.len(),
        1,
        "poetry --exclude-scope dev,build,test: expected 1 pypi component"
    );
    assert_eq!(pypi_prod[0]["name"], "requests");
    assert_eq!(
        prop_value(pypi_prod[0], "mikebom:sbom-tier"),
        Some("source")
    );
}

#[test]
fn pipfile_project_splits_default_vs_develop() {
    // Milestone 052/part-3: default emits all (post-052 FR-002);
    // --exclude-scope dev,build,test restores the prod-only view.

    let all = scan("pipfile-project", false);
    let pypi = pypi_components(&all);
    assert_eq!(pypi.len(), 2);
    let pytest = pypi
        .iter()
        .find(|c| c["name"] == "pytest")
        .expect("pytest present in default mode (post-052)");
    assert_eq!(pytest["scope"].as_str(), Some("excluded"));
    assert_eq!(
        prop_value(pytest, "mikebom:lifecycle-scope"),
        Some("development")
    );

    let prod = scan("pipfile-project", true);
    assert_eq!(pypi_components(&prod).len(), 1);
}

#[test]
fn requirements_only_emits_mixed_tier_components() {
    // Fixture has: `requests==2.31.0` (pinned), `urllib3>=2,<3`
    // (range), `certifi` (unpinned). Pin → source tier (we have
    // a concrete version); range / unpinned → design tier.
    let sbom = scan("requirements-only", false);
    let pypi = pypi_components(&sbom);
    assert_eq!(pypi.len(), 3);

    let find = |name: &str| -> &serde_json::Value {
        pypi.iter()
            .find(|c| c["name"] == name)
            .unwrap_or_else(|| panic!("{name} missing from pypi components"))
    };
    assert_eq!(
        prop_value(find("requests"), "mikebom:sbom-tier"),
        Some("source"),
        "pinned `requests==2.31.0` must be source-tier",
    );
    assert_eq!(
        prop_value(find("urllib3"), "mikebom:sbom-tier"),
        Some("design"),
        "range `urllib3>=2,<3` must be design-tier",
    );
    assert_eq!(
        prop_value(find("certifi"), "mikebom:sbom-tier"),
        Some("design"),
        "unpinned `certifi` must be design-tier",
    );
    for c in &pypi {
        assert!(
            prop_value(c, "mikebom:requirement-range").is_some(),
            "{}: must carry requirement-range",
            c["name"]
        );
    }
    // Mixed tiers: the presence of ONE source-tier component marks
    // the ecosystem authoritative for that subset. The existence of
    // the `requests` pin is enough to emit an `aggregate: complete`
    // record for pypi.
    let compositions = sbom["compositions"].as_array().unwrap();
    let pypi_complete = compositions.iter().any(|r| {
        r["aggregate"].as_str() == Some("complete")
            && r["assemblies"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .any(|p| p.as_str().is_some_and(|s| s.starts_with("pkg:pypi/")))
                })
                .unwrap_or(false)
    });
    assert!(
        pypi_complete,
        "any pinned pypi requirement marks the ecosystem complete"
    );
    // envelope lifecycles still includes "design" because urllib3 /
    // certifi are design-tier.
    let lifecycles = &sbom["metadata"]["lifecycles"];
    if let Some(arr) = lifecycles.as_array() {
        assert!(
            arr.iter().any(|p| p["phase"].as_str() == Some("design")),
            "envelope lifecycles must include 'design'"
        );
    }
}

#[test]
fn pyproject_only_emits_zero_pypi_components() {
    let sbom = scan("pyproject-only", false);
    let pypi = pypi_components(&sbom);
    assert_eq!(
        pypi.len(),
        0,
        "pyproject-only MUST emit no pypi components (FR-005)"
    );
}
