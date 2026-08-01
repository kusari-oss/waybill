//! Milestone 223 US1 — integration tests for the Pants pex-lockfile
//! reader. Each test invokes waybill as a subprocess against a
//! synthetic fixture and asserts the emitted SBOM contains the
//! expected components + annotations + dependency edges + log lines.
//!
//! Fixtures live at `waybill-cli/tests/fixtures/pants_pex/` (crate-local
//! per T008 / T009 in specs/223-pants-pex-reader/tasks.md). Every
//! fixture uses synthetic `waybill-fixture-*` package names per memory
//! `feedback_fixture_synthetic_package_names`.

#![cfg_attr(test, allow(clippy::unwrap_used))]

use std::path::{Path, PathBuf};
use std::process::Command;

mod common;
use common::bin;

/// Crate-local pants fixture path resolver.
fn pants_fixture(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/pants_pex")
        .join(rel)
}

/// Run `waybill sbom scan` against the given fixture and return the
/// subprocess output. Uses `--offline` to avoid any accidental network
/// calls; `--no-deep-hash` to keep the scan under 1s.
fn run_scan(
    fixture: &Path,
    output: &Path,
    extra_args: &[&str],
) -> std::process::Output {
    let mut cmd = Command::new(bin());
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(fixture)
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(output)
        .arg("--no-deep-hash")
        .env("RUST_LOG", "info");
    for a in extra_args {
        cmd.arg(a);
    }
    cmd.output().expect("waybill invocation")
}

/// Parse the emitted CDX + return the JSON value.
fn read_cdx(path: &Path) -> serde_json::Value {
    let raw = std::fs::read(path).expect("read cdx");
    serde_json::from_slice(&raw).expect("parse cdx")
}

/// Extract a component's property value by name. CDX properties are
/// `{ "name": "...", "value": "..." }` objects in a components[].properties[]
/// array.
fn get_property<'a>(component: &'a serde_json::Value, name: &str) -> Option<&'a str> {
    component
        .get("properties")?
        .as_array()?
        .iter()
        .find(|p| p.get("name").and_then(|v| v.as_str()) == Some(name))
        .and_then(|p| p.get("value"))
        .and_then(|v| v.as_str())
}

// ---------------------------------------------------------------------
// US1 (T011 + T013 + T017a via combined subprocess) — minimal Python
// lockfile emits 3 pypi components with hashes, deps, resolve annotation,
// FR-010 log line
// ---------------------------------------------------------------------

#[test]
fn us1_minimal_python_lockfile_emits_3_pypi_components() {
    let fixture = pants_fixture("minimal_python");
    let tmp = tempfile::tempdir().expect("tempdir");
    let cdx_out = tmp.path().join("us1.cdx.json");
    let spdx_out = tmp.path().join("us1.spdx.json");

    // Emits BOTH formats in one scan invocation per C3 remediation
    // (SC-001 explicit CDX+SPDX claim). Multi-format requires
    // `--output <fmt>=<path>` per waybill's CLI convention.
    let out = Command::new(bin())
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(&fixture)
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--format")
        .arg("spdx-2.3-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", cdx_out.display()))
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", spdx_out.display()))
        .arg("--no-deep-hash")
        .env("RUST_LOG", "info")
        .output()
        .expect("waybill invocation");

    assert!(
        out.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    // ----- (a) CDX assertions -----
    let cdx = read_cdx(&cdx_out);
    let pants_components: Vec<&serde_json::Value> = cdx
        .get("components")
        .and_then(|c| c.as_array())
        .expect("components array")
        .iter()
        .filter(|c| {
            c.get("purl")
                .and_then(|p| p.as_str())
                .is_some_and(|p| p.starts_with("pkg:pypi/waybill-fixture-"))
        })
        .collect();

    assert_eq!(
        pants_components.len(),
        3,
        "expected exactly 3 pants-derived components, got {}: {:?}",
        pants_components.len(),
        pants_components
            .iter()
            .map(|c| c.get("purl").and_then(|p| p.as_str()).unwrap_or(""))
            .collect::<Vec<_>>()
    );

    for c in &pants_components {
        // Each component has one sha256 hash.
        let hashes = c
            .get("hashes")
            .and_then(|h| h.as_array())
            .expect("hashes array");
        assert_eq!(hashes.len(), 1, "expected 1 hash per component");
        assert_eq!(
            hashes[0].get("alg").and_then(|a| a.as_str()),
            Some("SHA-256"),
            "expected SHA-256 hash"
        );
        // Each has waybill:pants-resolve=default in properties.
        assert_eq!(
            get_property(c, "waybill:pants-resolve"),
            Some("default"),
            "expected waybill:pants-resolve=default on component: {c}"
        );
    }

    // ----- (b) Dependency edge from -b → -a (per fixture's requires_dists) -----
    let deps = cdx
        .get("dependencies")
        .and_then(|d| d.as_array())
        .expect("dependencies array");
    let b_ref_target = "pkg:pypi/waybill-fixture-b@1.0.0";
    let a_ref_target = "pkg:pypi/waybill-fixture-a@1.0.0";
    let b_entry = deps
        .iter()
        .find(|d| d.get("ref").and_then(|r| r.as_str()) == Some(b_ref_target))
        .expect("dependencies[] entry for waybill-fixture-b");
    let b_depends_on: Vec<&str> = b_entry
        .get("dependsOn")
        .and_then(|d| d.as_array())
        .expect("dependsOn array")
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(
        b_depends_on.contains(&a_ref_target),
        "expected {b_ref_target} → {a_ref_target} edge, got dependsOn: {b_depends_on:?}"
    );

    // ----- (c) SPDX 2.3 assertions (C3 remediation) -----
    let spdx: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&spdx_out).expect("read spdx")).expect("parse spdx");
    let pants_packages: Vec<&serde_json::Value> = spdx
        .get("packages")
        .and_then(|p| p.as_array())
        .expect("packages array")
        .iter()
        .filter(|pkg| {
            pkg.get("externalRefs")
                .and_then(|r| r.as_array())
                .is_some_and(|refs| {
                    refs.iter().any(|r| {
                        r.get("referenceLocator")
                            .and_then(|v| v.as_str())
                            .is_some_and(|s| s.starts_with("pkg:pypi/waybill-fixture-"))
                    })
                })
        })
        .collect();
    assert_eq!(
        pants_packages.len(),
        3,
        "expected 3 pants-derived SPDX packages, got {}",
        pants_packages.len()
    );
    for pkg in &pants_packages {
        // Each has 1 SHA256 checksum.
        let checksums = pkg
            .get("checksums")
            .and_then(|c| c.as_array())
            .expect("checksums array");
        let sha256_count = checksums
            .iter()
            .filter(|c| c.get("algorithm").and_then(|a| a.as_str()) == Some("SHA256"))
            .count();
        assert_eq!(sha256_count, 1, "expected 1 SHA256 checksum on SPDX package");
    }
}

// ---------------------------------------------------------------------
// US1 (T012) — multi-resolve tags scope by allowlist
// ---------------------------------------------------------------------

#[test]
fn us1_multi_resolve_tags_scope_per_allowlist() {
    let fixture = pants_fixture("multi_resolve");
    let tmp = tempfile::tempdir().expect("tempdir");
    let output = tmp.path().join("us1_multi.cdx.json");
    let out = run_scan(&fixture, &output, &[]);
    assert!(
        out.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let cdx = read_cdx(&output);
    let all: Vec<&serde_json::Value> = cdx
        .get("components")
        .and_then(|c| c.as_array())
        .expect("components array")
        .iter()
        .filter(|c| {
            c.get("purl")
                .and_then(|p| p.as_str())
                .is_some_and(|p| p.starts_with("pkg:pypi/waybill-fixture-"))
        })
        .collect();

    assert_eq!(
        all.len(),
        6,
        "expected 6 pants-derived components across 3 resolves, got {}",
        all.len()
    );

    // Group by resolve name via the waybill:pants-resolve annotation.
    for c in &all {
        let purl = c.get("purl").and_then(|p| p.as_str()).unwrap_or("");
        let resolve = get_property(c, "waybill:pants-resolve").unwrap_or("<missing>");
        let scope = c.get("scope").and_then(|s| s.as_str());

        match resolve {
            "default" => {
                assert!(
                    purl.starts_with("pkg:pypi/waybill-fixture-runtime-"),
                    "default resolve should carry runtime-* packages, got {purl}"
                );
                // CDX `scope=required` (or absent, which defaults to required)
                // for Runtime lifecycle-scope per waybill-common convention.
                assert!(
                    scope.is_none() || scope == Some("required"),
                    "runtime-scoped component should have scope=required or absent, got {scope:?}"
                );
            }
            "mypy" => {
                assert!(
                    purl.starts_with("pkg:pypi/waybill-fixture-typing-"),
                    "mypy resolve should carry typing-* packages, got {purl}"
                );
                // Development lifecycle-scope emits waybill:lifecycle-scope=dev.
                assert_eq!(
                    get_property(c, "waybill:lifecycle-scope"),
                    Some("development"),
                    "mypy-resolve component missing lifecycle-scope=development"
                );
            }
            "pytest" => {
                assert!(
                    purl.starts_with("pkg:pypi/waybill-fixture-testing-"),
                    "pytest resolve should carry testing-* packages, got {purl}"
                );
                assert_eq!(
                    get_property(c, "waybill:lifecycle-scope"),
                    Some("development"),
                    "pytest-resolve component missing lifecycle-scope=development"
                );
            }
            other => panic!("unexpected resolve name {other} on {purl}"),
        }
    }
}

// ---------------------------------------------------------------------
// US2 (T019) — Pex lockfile dedups against requirements.txt
// ---------------------------------------------------------------------

/// The Pants pex-lockfile entry and the pip reader's requirements.txt
/// entry both resolve to the same PURL (`pkg:pypi/waybill-fixture-shared@1.0.0`).
/// The m191 reconciler merges them into ONE component. The Pex-side
/// wins on hash-carrying (requirements.txt has no hashes), and the
/// merged component carries a `waybill:source-files` annotation
/// listing BOTH source paths for audit — matching FR-005 + m105+
/// existing `also-detected-via` semantics via the source-files channel.
#[test]
fn us2_lockfile_dedups_against_requirements_txt() {
    let fixture = pants_fixture("with_requirements_txt");
    let tmp = tempfile::tempdir().expect("tempdir");
    let output = tmp.path().join("us2.cdx.json");
    let out = run_scan(&fixture, &output, &[]);
    assert!(
        out.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let cdx = read_cdx(&output);
    // Find every component matching the shared PURL. There must be
    // exactly ONE — dedup should have collapsed the pip + pants entries.
    let shared: Vec<&serde_json::Value> = cdx
        .get("components")
        .and_then(|c| c.as_array())
        .expect("components array")
        .iter()
        .filter(|c| {
            c.get("purl")
                .and_then(|p| p.as_str())
                == Some("pkg:pypi/waybill-fixture-shared@1.0.0")
        })
        .collect();

    assert_eq!(
        shared.len(),
        1,
        "expected exactly ONE component after dedup (pants + pip merged into one); \
         got {} matching pkg:pypi/waybill-fixture-shared@1.0.0. \
         Raw components: {:#?}",
        shared.len(),
        cdx.get("components").cloned().unwrap_or(serde_json::json!([]))
    );

    let component = shared[0];

    // Hash from the lockfile MUST be preserved on the merged component
    // (requirements.txt carries no hashes; Pex-tier wins).
    let hashes = component
        .get("hashes")
        .and_then(|h| h.as_array())
        .expect("hashes array");
    assert!(
        hashes.iter().any(|h| {
            h.get("alg").and_then(|a| a.as_str()) == Some("SHA-256")
                && h.get("content")
                    .and_then(|c| c.as_str())
                    == Some(
                        "7777777777777777777777777777777777777777777777777777777777777777",
                    )
        }),
        "expected SHA-256 hash from lockfile to survive dedup; hashes: {:#?}",
        hashes
    );

    // waybill:pants-resolve annotation from the pants reader MUST
    // survive dedup — proves the pants entry is the merge-winner (or
    // at least a contributor). Reconciler preserves per-reader
    // annotations on the merged component.
    assert_eq!(
        get_property(component, "waybill:pants-resolve"),
        Some("default"),
        "waybill:pants-resolve missing after dedup — pants entry didn't contribute"
    );

    // The audit trail: waybill:source-files SHOULD list both the
    // lockfile and requirements.txt paths so operators can see which
    // sources contributed to the merged component. FR-005 satisfied.
    let source_files_str = get_property(component, "waybill:source-files")
        .expect("waybill:source-files annotation missing");
    assert!(
        source_files_str.contains("3rdparty/python/default.lock"),
        "waybill:source-files should include the Pex lockfile path; got: {source_files_str}"
    );
    assert!(
        source_files_str.contains("requirements.txt"),
        "waybill:source-files should include the requirements.txt path (dedup audit trail per FR-005); got: {source_files_str}"
    );
}

// ---------------------------------------------------------------------
// US3 (T022) — pants.toml custom lockfile path discovery
// ---------------------------------------------------------------------

/// FR-004: when `pants.toml` declares `[python].lockfile = "..."`,
/// waybill discovers that path even though the default
/// `3rdparty/python/*.lock` glob is unmet.
#[test]
fn us3_pants_toml_custom_path_discovery() {
    let fixture = pants_fixture("pants_toml_custom_path");
    let tmp = tempfile::tempdir().expect("tempdir");
    let output = tmp.path().join("us3_custom.cdx.json");
    let out = run_scan(&fixture, &output, &[]);
    assert!(
        out.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let cdx = read_cdx(&output);
    let matches: Vec<&serde_json::Value> = cdx
        .get("components")
        .and_then(|c| c.as_array())
        .expect("components array")
        .iter()
        .filter(|c| {
            c.get("purl")
                .and_then(|p| p.as_str())
                .is_some_and(|p| p.starts_with("pkg:pypi/waybill-fixture-custompath-"))
        })
        .collect();

    assert_eq!(
        matches.len(),
        2,
        "expected 2 components from pants.toml-declared custom path, got {}",
        matches.len()
    );

    // Sanity: FR-010 log reports 1 lockfile discovered (the custom-path
    // one, not counting any absent default-glob candidates).
    let ansi_re = regex::Regex::new(r"\x1b\[[0-9;]*[a-zA-Z]").expect("valid regex");
    let stderr = ansi_re
        .replace_all(&String::from_utf8_lossy(&out.stderr), "")
        .to_string();
    assert!(
        stderr.contains("lockfiles_discovered=1"),
        "expected lockfiles_discovered=1 (custom path only); got:\n{stderr}"
    );
    assert!(
        stderr.contains("components_emitted=2"),
        "expected components_emitted=2; got:\n{stderr}"
    );
}

// ---------------------------------------------------------------------
// US3 (T023) — Missing pants.toml → fall back to default glob
// ---------------------------------------------------------------------

/// FR-004 fallback: a repo with no `pants.toml` but a valid
/// `3rdparty/python/default.lock` discovers via the default glob.
/// Regression guard for the fallback contract — if config discovery
/// somehow becomes MANDATORY, this test fails.
#[test]
fn us3_missing_pants_toml_falls_back_to_default_glob() {
    // Reuse the US1 minimal_python fixture — it has no pants.toml,
    // so this is a pure fallback exercise.
    let fixture = pants_fixture("minimal_python");
    let tmp = tempfile::tempdir().expect("tempdir");
    let output = tmp.path().join("us3_fallback.cdx.json");
    let out = run_scan(&fixture, &output, &[]);
    assert!(
        out.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let cdx = read_cdx(&output);
    let count = cdx
        .get("components")
        .and_then(|c| c.as_array())
        .expect("components array")
        .iter()
        .filter(|c| {
            c.get("purl")
                .and_then(|p| p.as_str())
                .is_some_and(|p| p.starts_with("pkg:pypi/waybill-fixture-"))
        })
        .count();
    assert_eq!(
        count, 3,
        "US1 fixture (no pants.toml) should still discover 3 default-glob components, got {count}"
    );
}

// ---------------------------------------------------------------------
// US3 (T024) — Malformed pants.toml → WARN + fall back to default glob
// ---------------------------------------------------------------------

/// FR-004 + FR-006 fail-open: a `pants.toml` that is not valid TOML
/// must emit a WARN + fall back to the default glob without aborting
/// the scan. Verifies the pants.toml parser errors don't propagate as
/// scan failures.
#[test]
fn us3_malformed_pants_toml_falls_back_gracefully() {
    let fixture = pants_fixture("malformed_pants_toml");
    let tmp = tempfile::tempdir().expect("tempdir");
    let output = tmp.path().join("us3_malf.cdx.json");
    let out = run_scan(&fixture, &output, &[]);
    assert!(
        out.status.success(),
        "scan MUST NOT abort on malformed pants.toml (fail-open per FR-006). stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // The fallback default.lock has 1 entry.
    let cdx = read_cdx(&output);
    let matches: Vec<&serde_json::Value> = cdx
        .get("components")
        .and_then(|c| c.as_array())
        .expect("components array")
        .iter()
        .filter(|c| {
            c.get("purl")
                .and_then(|p| p.as_str())
                .is_some_and(|p| p.starts_with("pkg:pypi/waybill-fixture-fallback"))
        })
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "expected 1 component from default-glob fallback after malformed pants.toml; got {}",
        matches.len()
    );

    // WARN must name pants.toml.
    let ansi_re = regex::Regex::new(r"\x1b\[[0-9;]*[a-zA-Z]").expect("valid regex");
    let stderr = ansi_re
        .replace_all(&String::from_utf8_lossy(&out.stderr), "")
        .to_string();
    assert!(
        stderr.contains("pants.toml could not be parsed as TOML"),
        "expected WARN naming pants.toml + parse-fail reason; got:\n{stderr}"
    );
    assert!(
        stderr.contains("falling back to default glob"),
        "expected WARN mentioning fallback; got:\n{stderr}"
    );
}

// ---------------------------------------------------------------------
// Phase 6 (T027) — Non-PyPI lockfile entries emit pkg:generic PURLs
// with waybill:source-url + waybill:source-type annotations (FR-009 / Q2 A)
// ---------------------------------------------------------------------

#[test]
fn non_pypi_entries_emit_pkg_generic_with_source_annotations() {
    let fixture = pants_fixture("non_pypi_entries");
    let tmp = tempfile::tempdir().expect("tempdir");
    let output = tmp.path().join("p6_np.cdx.json");
    let out = run_scan(&fixture, &output, &[]);
    assert!(
        out.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let cdx = read_cdx(&output);
    let pants_comps: Vec<&serde_json::Value> = cdx
        .get("components")
        .and_then(|c| c.as_array())
        .expect("components array")
        .iter()
        .filter(|c| {
            c.get("purl")
                .and_then(|p| p.as_str())
                .is_some_and(|p| p.contains("waybill-fixture-"))
        })
        .collect();

    assert_eq!(pants_comps.len(), 4, "expected 4 pants-derived components");

    // Count by PURL type + source-type.
    let mut pypi_count = 0;
    let mut generic_count = 0;
    let mut generic_by_source_type: std::collections::BTreeMap<&str, &str> =
        std::collections::BTreeMap::new();
    for c in &pants_comps {
        let purl = c.get("purl").and_then(|p| p.as_str()).unwrap_or("");
        if purl.starts_with("pkg:pypi/") {
            pypi_count += 1;
            // PyPI entries MUST NOT have source-url / source-type
            // (per FR-009 — those annotations are only for non-PyPI).
            assert_eq!(
                get_property(c, "waybill:source-url"),
                None,
                "PyPI entry should not have waybill:source-url; got {c:#?}"
            );
            assert_eq!(
                get_property(c, "waybill:source-type"),
                None,
                "PyPI entry should not have waybill:source-type"
            );
        } else if purl.starts_with("pkg:generic/") {
            generic_count += 1;
            let src_type = get_property(c, "waybill:source-type")
                .expect("pkg:generic entry MUST have waybill:source-type");
            let src_url = get_property(c, "waybill:source-url")
                .expect("pkg:generic entry MUST have waybill:source-url");
            generic_by_source_type.insert(src_type, src_url);
        } else {
            panic!("unexpected PURL type: {purl}");
        }
    }

    assert_eq!(pypi_count, 1, "expected exactly 1 pkg:pypi/ entry");
    assert_eq!(generic_count, 3, "expected exactly 3 pkg:generic/ entries");

    // Confirm all 3 non-PyPI source types are represented + URLs match.
    let git_url = generic_by_source_type
        .get("git")
        .expect("missing git-source entry");
    assert!(
        git_url.starts_with("git+https://example.test/"),
        "git source URL wrong: {git_url}"
    );
    let url_url = generic_by_source_type
        .get("url")
        .expect("missing url-source entry");
    assert!(
        url_url.starts_with("https://mirror.example.test/"),
        "url source URL wrong: {url_url}"
    );
    let local_url = generic_by_source_type
        .get("local")
        .expect("missing local-source entry");
    assert!(
        local_url.starts_with("file:///"),
        "local source URL wrong: {local_url}"
    );
}

// ---------------------------------------------------------------------
// Phase 6 (T029) — Corrupt lockfile → WARN + fail-open (SC-005 / FR-006)
// ---------------------------------------------------------------------

#[test]
fn corrupt_lockfile_produces_warn_and_continues() {
    let fixture = pants_fixture("corrupt_lockfile");
    let tmp = tempfile::tempdir().expect("tempdir");
    let output = tmp.path().join("p6_corrupt.cdx.json");
    let out = run_scan(&fixture, &output, &[]);
    // Fail-open — scan MUST NOT abort even though the only lockfile is
    // unparseable.
    assert!(
        out.status.success(),
        "scan aborted on corrupt lockfile — FR-006 fail-open violated. stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Zero pants-derived components.
    let cdx = read_cdx(&output);
    let pants_count = cdx
        .get("components")
        .and_then(|c| c.as_array())
        .expect("components array")
        .iter()
        .filter(|c| {
            c.get("purl")
                .and_then(|p| p.as_str())
                .is_some_and(|p| p.contains("waybill-fixture-"))
        })
        .count();
    assert_eq!(
        pants_count, 0,
        "expected zero components from corrupt lockfile; got {pants_count}"
    );

    // WARN must name the corrupt lockfile.
    let ansi_re = regex::Regex::new(r"\x1b\[[0-9;]*[a-zA-Z]").expect("valid regex");
    let stderr = ansi_re
        .replace_all(&String::from_utf8_lossy(&out.stderr), "")
        .to_string();
    assert!(
        stderr.contains("failed to parse Pex lockfile as JSON"),
        "expected WARN with JSON-parse-fail reason; got:\n{stderr}"
    );
    assert!(
        stderr.contains("corrupt_lockfile/3rdparty/python/default.lock"),
        "expected WARN to name the corrupt file path; got:\n{stderr}"
    );
    assert!(
        stderr.contains("lockfiles_skipped_corrupt=1"),
        "expected FR-010 counter to reflect the skip; got:\n{stderr}"
    );
}

// ---------------------------------------------------------------------
// Phase 6 (T030) — No pants.toml + no lockfiles → reader is a no-op
// (SC-003 / FR-007: byte-identity for non-Pants repos)
// ---------------------------------------------------------------------

/// A repo with no `pants.toml` and no `3rdparty/python/*.lock` files
/// must produce zero pants-derived components AND the reader must
/// return early WITHOUT emitting the FR-010 log line. This guarantees
/// the feature adds zero visible cost on non-Pants repos per SC-003.
#[test]
fn no_pants_no_lockfiles_produces_no_reader_activity() {
    let fixture = pants_fixture("not_pants");
    let tmp = tempfile::tempdir().expect("tempdir");
    let output = tmp.path().join("p6_np_pants.cdx.json");
    let out = run_scan(&fixture, &output, &[]);
    assert!(
        out.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let cdx = read_cdx(&output);
    let pants_count = cdx
        .get("components")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|c| {
                    c.get("purl")
                        .and_then(|p| p.as_str())
                        .is_some_and(|p| p.contains("waybill-fixture-"))
                })
                .count()
        })
        .unwrap_or(0);
    assert_eq!(
        pants_count, 0,
        "expected zero pants-derived components on a non-Pants repo; got {pants_count}"
    );

    // The FR-010 log line MUST be absent — reader returned early
    // without touching lockfiles OR emitting a discovery report.
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("pants-pex reader complete"),
        "reader emitted its summary log on a non-Pants repo — SC-003 zero-cost \
         guarantee broken. stderr:\n{stderr}"
    );
}

// ---------------------------------------------------------------------
// US1 (T017a) — FR-010 INFO log emits all 4 structured fields
// ---------------------------------------------------------------------

#[test]
fn us1_fr010_info_log_emits_all_four_structured_fields() {
    let fixture = pants_fixture("minimal_python");
    let tmp = tempfile::tempdir().expect("tempdir");
    let output = tmp.path().join("us1_log.cdx.json");
    let out = run_scan(&fixture, &output, &[]);
    assert!(
        out.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    // tracing's default pretty formatter interleaves ANSI escape
    // codes between field names and values (e.g. `lockfiles_discovered\x1b[0m\x1b[2m=\x1b[0m1`).
    // Strip ANSI codes before substring matching so the assertions
    // aren't format-fragile.
    let stderr_raw = String::from_utf8_lossy(&out.stderr);
    let ansi_re = regex::Regex::new(r"\x1b\[[0-9;]*[a-zA-Z]").expect("valid regex");
    let stderr = ansi_re.replace_all(&stderr_raw, "").to_string();

    // All four FR-010 structured fields must appear in stderr.
    for field in &[
        "lockfiles_discovered=",
        "lockfiles_parsed_ok=",
        "lockfiles_skipped_corrupt=",
        "components_emitted=",
    ] {
        assert!(
            stderr.contains(field),
            "FR-010: stderr missing structured field {field}. stderr (ANSI-stripped):\n{stderr}"
        );
    }
    // Sanity: lockfiles_discovered=1 (one lockfile in the fixture).
    assert!(
        stderr.contains("lockfiles_discovered=1"),
        "expected lockfiles_discovered=1, got:\n{stderr}"
    );
    assert!(
        stderr.contains("components_emitted=3"),
        "expected components_emitted=3, got:\n{stderr}"
    );
}
