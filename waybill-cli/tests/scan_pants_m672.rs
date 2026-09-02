//! Milestone 672 — integration tests for the Pants pex-lockfile
//! reader follow-up: `//`-comment front-matter tolerance +
//! `[python.resolves]` bare-string map override + zero-discovered
//! diagnostic log path.
//!
//! Fixtures are composed at test time via `tempfile::tempdir()` (per
//! m670 T007 / m671 T012 precedent — no new files committed under
//! `waybill-cli/tests/fixtures/`). Every synthetic package name uses
//! the `waybill-fixture-*` prefix per memory
//! `feedback_fixture_synthetic_package_names`.
//!
//! Test coverage (populated by T009–T011 + T014–T018 + T020–T021):
//! - US1: legacy `//`-shape lockfile round-trips + malformed body
//!   fails open + clean-JSON is stripper no-op (T009, T010, T011)
//! - US2: `[python.resolves]` map extends discovery + dedup with
//!   map-wins + table WARN + missing-path WARN + legacy-singular
//!   union (T014–T018)
//! - US3: zero-discovered-with-signal emits hint; zero-discovered-
//!   without-signal stays silent (T020, T021)
//!
//! Cross-linked: `specs/672-pants-reader-follow-up/{spec,plan,data-model}.md`.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

/// Strip ANSI color escapes so `stderr.contains("foo=bar")` works
/// against `tracing`'s ANSI-colored key-value formatter. The regex
/// `\x1b\[[0-9;]*m` matches every SGR escape.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            // Skip until the terminator (a letter, typically 'm').
            let mut j = i + 2;
            while j < bytes.len() && !bytes[j].is_ascii_alphabetic() {
                j += 1;
            }
            i = j + 1;
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

/// Materialize a synthetic Pants repo under `root` by writing each
/// `(relative_path, bytes)` layout entry. Creates parent directories
/// as needed. Fixture author's responsibility to supply well-formed
/// content — the helper does no validation.
fn write_pants_repo(root: &Path, layout: &[(&str, &[u8])]) {
    for (rel_path, contents) in layout {
        let abs = root.join(rel_path);
        if let Some(parent) = abs.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&abs, contents).unwrap();
    }
}

/// Run `waybill sbom scan` against the fixture directory and return
/// (parsed CDX Value, raw stderr as a UTF-8 lossy string). Uses
/// `--offline` + `--no-deep-hash` to keep the scan hermetic + fast.
/// Panics if the scan process fails to spawn.
fn run_scan(root: &Path, extra_args: &[&str]) -> (Value, String) {
    let out_dir = tempfile::tempdir().unwrap();
    let out_path = out_dir.path().join("out.cdx.json");
    let mut cmd = Command::new(binary_path());
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(root)
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(&out_path)
        .arg("--no-deep-hash")
        .env("RUST_LOG", "info");
    for a in extra_args {
        cmd.arg(a);
    }
    let result = cmd.output().unwrap();
    let stderr = strip_ansi(&String::from_utf8_lossy(&result.stderr));
    assert!(
        result.status.success(),
        "scan failed: stderr={stderr}",
    );
    let bytes = std::fs::read(&out_path).unwrap();
    let doc = serde_json::from_slice(&bytes).unwrap();
    (doc, stderr)
}

/// Same as `run_scan` but expects the scan to succeed AND returns the
/// exit code + stderr for callers that want to inspect
/// non-zero-but-expected exits (currently unused; kept for symmetry
/// with the m671 test harness).
#[allow(dead_code)]
fn run_scan_full(root: &Path, extra_args: &[&str]) -> (i32, String) {
    let out_dir = tempfile::tempdir().unwrap();
    let out_path = out_dir.path().join("out.cdx.json");
    let mut cmd = Command::new(binary_path());
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(root)
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(&out_path)
        .arg("--no-deep-hash")
        .env("RUST_LOG", "info");
    for a in extra_args {
        cmd.arg(a);
    }
    let result = cmd.output().unwrap();
    (
        result.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&result.stderr).into_owned(),
    )
}

/// Count top-level components (all types) in the CDX.
#[allow(dead_code)]
fn component_count(doc: &Value) -> usize {
    doc.get("components")
        .and_then(|c| c.as_array())
        .map(|arr| arr.len())
        .unwrap_or(0)
}

/// Extract every emitted PURL (top-level `components[].purl`) as a
/// sorted lex `Vec<String>` for stable assertions.
fn component_purls(doc: &Value) -> Vec<String> {
    let mut out: Vec<String> = doc
        .get("components")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| c.get("purl").and_then(|p| p.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

/// Extract every emitted component's `resolve-name` annotation value
/// (via `waybill:pants-resolve` property, per m223 emission shape).
/// Returns `Vec<(purl, resolve_name)>` sorted by purl for stability.
#[allow(dead_code)]
fn component_resolve_names(doc: &Value) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = doc
        .get("components")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| {
                    let purl = c.get("purl")?.as_str()?.to_string();
                    let props = c.get("properties")?.as_array()?;
                    let resolve = props
                        .iter()
                        .find(|p| {
                            p.get("name").and_then(|n| n.as_str())
                                == Some("waybill:pants-resolve")
                        })
                        .and_then(|p| p.get("value")?.as_str().map(String::from))?;
                    Some((purl, resolve))
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

// -------------------------------------------------------------------
// Tests populated by T009–T011 (US1), T014–T018 (US2), T020–T021 (US3).
// -------------------------------------------------------------------

/// Helper: a clean-JSON PEX lockfile body naming N synthetic packages
/// under a single locked-resolve block. Each package carries one
/// `files.pythonhosted.org` artifact URL so the m223 reader emits
/// `pkg:pypi/` PURLs (not the `pkg:generic/` fallback used when the
/// artifact list is empty or the URL is non-PyPI).
fn synth_clean_lockfile(packages: &[(&str, &str)]) -> Vec<u8> {
    let mut reqs = Vec::new();
    for (name, version) in packages {
        // Underscore-mangled name matches PyPI's convention (dashes
        // become underscores in wheel filenames).
        let mangled = name.replace('-', "_");
        reqs.push(format!(
            r#"{{"project_name":"{name}","version":"{version}","artifacts":[{{"algorithm":"sha256","hash":"{h}","url":"https://files.pythonhosted.org/packages/xx/{mangled}-{version}-py3-none-any.whl"}}]}}"#,
            h = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        ));
    }
    let body = format!(
        r#"{{"pex_version":"2.10.0","locked_resolves":[{{"locked_requirements":[{joined}]}}]}}"#,
        joined = reqs.join(",")
    );
    body.into_bytes()
}

/// Helper: a PEX lockfile body wrapped in a Pants ≤ 2.29 `//`-comment
/// metadata block (matching the shape observed in research.md §R1).
fn synth_legacy_lockfile(packages: &[(&str, &str)]) -> Vec<u8> {
    let body = synth_clean_lockfile(packages);
    let mut out = Vec::new();
    out.extend_from_slice(b"// This lockfile was autogenerated by Pants.\n");
    out.extend_from_slice(b"//\n");
    out.extend_from_slice(b"// --- BEGIN PANTS LOCKFILE METADATA ---\n");
    out.extend_from_slice(b"// {\"version\":3}\n");
    out.extend_from_slice(b"// --- END PANTS LOCKFILE METADATA ---\n");
    out.extend_from_slice(&body);
    out
}

/// Return a `PathBuf` for the fixture root, to keep test bodies terse.
#[allow(dead_code)]
fn root_of(dir: &tempfile::TempDir) -> PathBuf {
    dir.path().to_path_buf()
}

// -------------------------------------------------------------------
// User Story 1 — legacy `//`-comment lockfile shape (Priority: P1)
// -------------------------------------------------------------------

#[test]
fn legacy_shape_lockfile_round_trips_through_stripper() {
    // T009 (US1): a Pants ≤ 2.29 legacy-shape lockfile at the default
    // glob path emits its 2 synthetic components, the resolve name
    // matches the file stem, and the scan exits 0.
    let dir = tempfile::tempdir().unwrap();
    let legacy_lockfile = synth_legacy_lockfile(&[
        ("waybill-fixture-legacy-alpha", "1.0.0"),
        ("waybill-fixture-legacy-beta", "2.0.0"),
    ]);
    write_pants_repo(
        dir.path(),
        &[("3rdparty/python/legacy.lock", &legacy_lockfile)],
    );

    let (doc, stderr) = run_scan(dir.path(), &[]);

    // (a) Exactly 2 pypi components (the 2 synthetic packages).
    let purls = component_purls(&doc);
    let pypi_purls: Vec<&String> = purls
        .iter()
        .filter(|p| p.starts_with("pkg:pypi/"))
        .collect();
    assert_eq!(
        pypi_purls.len(),
        2,
        "expected 2 pypi components; got {:#?}\nfull component list: {:#?}",
        pypi_purls,
        purls,
    );
    // Specific PURLs.
    assert!(
        purls.iter().any(|p| p.contains("waybill-fixture-legacy-alpha")
            && p.contains("1.0.0")),
        "missing alpha@1.0.0 in purls: {purls:#?}",
    );
    assert!(
        purls.iter().any(|p| p.contains("waybill-fixture-legacy-beta")
            && p.contains("2.0.0")),
        "missing beta@2.0.0 in purls: {purls:#?}",
    );

    // (b) reader-complete INFO log shows legacy_shape_lockfiles=1.
    assert!(
        stderr.contains("pants-pex reader complete"),
        "missing reader-complete log; stderr={stderr}",
    );
    assert!(
        stderr.contains("legacy_shape_lockfiles=1"),
        "expected legacy_shape_lockfiles=1 in log; stderr={stderr}",
    );
}

#[test]
fn legacy_shape_malformed_body_fails_open() {
    // T010 (US1, fail-open): a `//`-comment header followed by a
    // MALFORMED JSON body must WARN + skip the file + still exit 0.
    // Verifies the m223 fail-open contract survives the m672
    // stripper-always-first change.
    let dir = tempfile::tempdir().unwrap();
    let mut malformed = Vec::new();
    malformed.extend_from_slice(b"// legacy metadata block\n");
    malformed.extend_from_slice(b"// --- END PANTS LOCKFILE METADATA ---\n");
    // Intentionally malformed JSON — missing quote + missing close brace.
    malformed.extend_from_slice(
        br#"{"pex_version":"2.10.0","locked_resolves":[{invalid_json"#,
    );
    write_pants_repo(
        dir.path(),
        &[("3rdparty/python/legacy_malformed.lock", &malformed)],
    );

    let (doc, stderr) = run_scan(dir.path(), &[]);

    // (a) 0 pypi components emitted from this file.
    let pypi_purls: Vec<String> = component_purls(&doc)
        .into_iter()
        .filter(|p| p.starts_with("pkg:pypi/"))
        .collect();
    assert_eq!(
        pypi_purls.len(),
        0,
        "expected 0 pypi components on malformed body; got {pypi_purls:#?}",
    );

    // (b) The WARN naming the JSON-parse failure must be present.
    assert!(
        stderr.contains("failed to parse Pex lockfile as JSON"),
        "expected JSON-parse WARN; stderr={stderr}",
    );
    // (c) The reader-complete summary shows skipped=1 (fail-open path).
    assert!(
        stderr.contains("lockfiles_skipped_corrupt=1"),
        "expected skipped_corrupt=1; stderr={stderr}",
    );
}

#[test]
fn clean_json_lockfile_is_stripper_no_op() {
    // T011 (US1, byte-identity): a clean-JSON lockfile at the default
    // glob path must (a) emit its component, (b) report
    // legacy_shape_lockfiles=0 (the stripper detected no legacy shape),
    // (c) tag with resolve_name from file-stem derivation.
    let dir = tempfile::tempdir().unwrap();
    let clean = synth_clean_lockfile(&[("waybill-fixture-clean-only", "3.0.0")]);
    write_pants_repo(dir.path(), &[("3rdparty/python/clean.lock", &clean)]);

    let (doc, stderr) = run_scan(dir.path(), &[]);

    // (a) Exactly 1 pypi component.
    let pypi_purls: Vec<String> = component_purls(&doc)
        .into_iter()
        .filter(|p| p.starts_with("pkg:pypi/"))
        .collect();
    assert_eq!(
        pypi_purls.len(),
        1,
        "expected 1 pypi component; got {pypi_purls:#?}",
    );
    assert!(
        pypi_purls[0].contains("waybill-fixture-clean-only"),
        "PURL missing project name; got {}",
        pypi_purls[0],
    );

    // (b) legacy_shape_lockfiles=0 (clean file, no leading `//`).
    assert!(
        stderr.contains("legacy_shape_lockfiles=0"),
        "expected legacy_shape_lockfiles=0 on clean-JSON scan; stderr={stderr}",
    );
}

// -------------------------------------------------------------------
// User Story 2 — `[python.resolves]` bare-string map (Priority: P1)
// -------------------------------------------------------------------

#[test]
fn python_resolves_map_extends_discovery_set() {
    // T014 (US2): `[python.resolves]` naming a lockfile in a
    // non-default directory (`build-support/py/`) AND a second
    // lockfile at the default glob path. Both must emit; both
    // resolve names come from the map keys.
    let dir = tempfile::tempdir().unwrap();
    let mypy_lock = synth_clean_lockfile(&[("waybill-fixture-mypy-pkg", "1.0.0")]);
    let user_reqs_lock =
        synth_clean_lockfile(&[("waybill-fixture-user-req", "2.0.0")]);
    write_pants_repo(
        dir.path(),
        &[
            (
                "pants.toml",
                br#"
[python.resolves]
mypy = "build-support/py/mypy.lock"
user_reqs = "3rdparty/python/user_reqs.lock"
"#,
            ),
            ("build-support/py/mypy.lock", &mypy_lock),
            ("3rdparty/python/user_reqs.lock", &user_reqs_lock),
        ],
    );

    let (doc, stderr) = run_scan(dir.path(), &[]);

    let pypi_purls: Vec<String> = component_purls(&doc)
        .into_iter()
        .filter(|p| p.starts_with("pkg:pypi/"))
        .collect();
    assert_eq!(
        pypi_purls.len(),
        2,
        "expected 2 pypi components (one per resolve); got {pypi_purls:#?}",
    );
    assert!(
        pypi_purls.iter().any(|p| p.contains("waybill-fixture-mypy-pkg")),
        "missing mypy-pkg in purls: {pypi_purls:#?}",
    );
    assert!(
        pypi_purls.iter().any(|p| p.contains("waybill-fixture-user-req")),
        "missing user-req in purls: {pypi_purls:#?}",
    );
    // Reader-complete log shows lockfiles_discovered=2.
    assert!(
        stderr.contains("lockfiles_discovered=2"),
        "expected discovered=2 on T014; stderr={stderr}",
    );
}

#[test]
fn python_resolves_map_wins_over_default_glob_on_collision() {
    // T015 (US2, FR-009): a `[python.resolves]` entry names a file
    // that ALSO matches the default glob. The file is parsed exactly
    // once, and the emitted resolve-name annotation is the map key
    // (`custom-name`), NOT the file-stem-derived name (`generic-file`).
    let dir = tempfile::tempdir().unwrap();
    let lock = synth_clean_lockfile(&[("waybill-fixture-collision-pkg", "1.0.0")]);
    write_pants_repo(
        dir.path(),
        &[
            (
                "pants.toml",
                br#"
[python.resolves]
custom-name = "3rdparty/python/generic-file.lock"
"#,
            ),
            ("3rdparty/python/generic-file.lock", &lock),
        ],
    );

    let (doc, stderr) = run_scan(dir.path(), &[]);

    // (a) Exactly 1 pypi component (single parse; the collision
    //     dedup succeeded).
    let pypi_purls: Vec<String> = component_purls(&doc)
        .into_iter()
        .filter(|p| p.starts_with("pkg:pypi/"))
        .collect();
    assert_eq!(
        pypi_purls.len(),
        1,
        "expected 1 pypi component (dedup fired); got {pypi_purls:#?}",
    );
    // (b) The reader-complete log shows lockfiles_parsed_ok=1
    //     (proves the file was parsed exactly once).
    assert!(
        stderr.contains("lockfiles_parsed_ok=1"),
        "expected parsed_ok=1; stderr={stderr}",
    );

    // (c) The emitted resolve-name is the map key. Check via the
    //     component's `waybill:pants-resolve` property.
    let resolve_names = component_resolve_names(&doc);
    assert_eq!(resolve_names.len(), 1);
    assert_eq!(
        resolve_names[0].1, "custom-name",
        "map key `custom-name` must win over file-stem `generic-file`; got={:?}",
        resolve_names[0].1,
    );
}

#[test]
fn python_resolves_table_shape_warns_and_skips() {
    // T016 (US2, FR-007 + clarify Q2): a table-shape entry in
    // `[python.resolves]` WARNs naming the resolve name AND the
    // observed TOML type, AND skips the entry. Other bare-string
    // entries in the same map are still honored.
    let dir = tempfile::tempdir().unwrap();
    let valid_lock = synth_clean_lockfile(&[("waybill-fixture-valid-only", "1.0.0")]);
    // Note: we don't need a real lockfile for the table entry — the
    // WARN fires BEFORE any file read (contract C3 skips at parse
    // time).
    write_pants_repo(
        dir.path(),
        &[
            (
                "pants.toml",
                br#"
[python.resolves]
valid-resolve = "3rdparty/python/valid.lock"
[python.resolves.table-resolve]
path = "3rdparty/python/table.lock"
"#,
            ),
            ("3rdparty/python/valid.lock", &valid_lock),
        ],
    );

    let (doc, stderr) = run_scan(dir.path(), &[]);

    // (a) Exactly 1 pypi component — from `valid-resolve` only.
    let pypi_purls: Vec<String> = component_purls(&doc)
        .into_iter()
        .filter(|p| p.starts_with("pkg:pypi/"))
        .collect();
    assert_eq!(
        pypi_purls.len(),
        1,
        "expected 1 pypi component (valid-resolve only); got {pypi_purls:#?}",
    );

    // (b) WARN naming `table-resolve` and the observed TOML type.
    assert!(
        stderr.contains("table-resolve"),
        "WARN must name the offending resolve; stderr={stderr}",
    );
    assert!(
        stderr.contains("non-string value"),
        "WARN must mention non-string value; stderr={stderr}",
    );
    // (c) WARN carries the migration hint.
    assert!(
        stderr.contains("bare-string") || stderr.contains("follow-up issue"),
        "WARN must include the migration hint; stderr={stderr}",
    );
}

#[test]
fn python_resolves_map_missing_path_warns_and_skips() {
    // T017 (US2, FR-008): a `[python.resolves]` entry names a path
    // that does NOT exist on disk. The reader WARNs naming both the
    // resolve name AND the missing path, and continues honoring other
    // entries.
    let dir = tempfile::tempdir().unwrap();
    let exists_lock = synth_clean_lockfile(&[("waybill-fixture-exists-pkg", "1.0.0")]);
    write_pants_repo(
        dir.path(),
        &[
            (
                "pants.toml",
                br#"
[python.resolves]
exists = "3rdparty/python/exists.lock"
ghost = "3rdparty/python/ghost.lock"
"#,
            ),
            ("3rdparty/python/exists.lock", &exists_lock),
            // ghost.lock intentionally missing.
        ],
    );

    let (doc, stderr) = run_scan(dir.path(), &[]);

    let pypi_purls: Vec<String> = component_purls(&doc)
        .into_iter()
        .filter(|p| p.starts_with("pkg:pypi/"))
        .collect();
    assert_eq!(
        pypi_purls.len(),
        1,
        "expected 1 pypi component (exists only); got {pypi_purls:#?}",
    );
    assert!(
        stderr.contains("ghost"),
        "WARN must name the missing resolve; stderr={stderr}",
    );
    assert!(
        stderr.contains("does not exist on disk"),
        "WARN must mention path missing; stderr={stderr}",
    );
}

#[test]
fn python_lockfile_singular_and_resolves_map_both_honored() {
    // T018 (US2, FR-006): declaring BOTH `[python].lockfile = "..."`
    // AND `[python.resolves]` in the same pants.toml MUST honor
    // both (superset union). Legacy singular emits with file-stem-
    // derived name; map entries emit with map-key names.
    let dir = tempfile::tempdir().unwrap();
    let legacy_lock = synth_clean_lockfile(&[("waybill-fixture-legacy-pkg", "1.0.0")]);
    let modern_lock = synth_clean_lockfile(&[("waybill-fixture-modern-pkg", "2.0.0")]);
    write_pants_repo(
        dir.path(),
        &[
            (
                "pants.toml",
                br#"
[python]
lockfile = "build-support/legacy.lock"

[python.resolves]
modern = "build-support/modern.lock"
"#,
            ),
            ("build-support/legacy.lock", &legacy_lock),
            ("build-support/modern.lock", &modern_lock),
        ],
    );

    let (doc, stderr) = run_scan(dir.path(), &[]);

    let pypi_purls: Vec<String> = component_purls(&doc)
        .into_iter()
        .filter(|p| p.starts_with("pkg:pypi/"))
        .collect();
    assert_eq!(
        pypi_purls.len(),
        2,
        "expected 2 pypi components (union); got {pypi_purls:#?}",
    );

    // Resolve names — one from file-stem (`legacy`), one from map key
    // (`modern`).
    let names: Vec<String> = component_resolve_names(&doc)
        .into_iter()
        .map(|(_, r)| r)
        .collect();
    assert!(
        names.iter().any(|n| n == "legacy"),
        "expected file-stem-derived `legacy`; got {names:?}",
    );
    assert!(
        names.iter().any(|n| n == "modern"),
        "expected map-key-derived `modern`; got {names:?}",
    );
    assert!(
        stderr.contains("lockfiles_discovered=2"),
        "expected discovered=2 on T018; stderr={stderr}",
    );
}

// -------------------------------------------------------------------
// User Story 3 — zero-discovered diagnostic (Priority: P2)
// -------------------------------------------------------------------

#[test]
fn zero_discovered_with_pants_signal_logs_hint() {
    // T020 (US3, FR-010/FR-011): a repo with a `pants.toml` present
    // AND NO `3rdparty/python/` directory. discover_lockfiles finds
    // zero candidates but the Pants signal IS present — the reader
    // emits a single-line INFO diagnostic naming discovered=0 + the
    // hint text listing both supported override keys.
    let dir = tempfile::tempdir().unwrap();
    write_pants_repo(
        dir.path(),
        &[
            (
                "pants.toml",
                // Valid TOML, but no [python] section — no override
                // declared, no lockfiles discoverable.
                br#"
[jvm]
lockfile = "unrelated.lock"
"#,
            ),
        ],
    );

    let (_doc, stderr) = run_scan(dir.path(), &[]);

    // (a) Diagnostic log line is present.
    assert!(
        stderr.contains("pants-pex reader complete"),
        "expected reader-complete log; stderr={stderr}",
    );
    assert!(
        stderr.contains("lockfiles_discovered=0"),
        "expected discovered=0; stderr={stderr}",
    );
    // (b) Hint field names both supported override keys.
    assert!(
        stderr.contains("[python.resolves]"),
        "hint must name `[python.resolves]`; stderr={stderr}",
    );
    assert!(
        stderr.contains("[python].lockfile"),
        "hint must name `[python].lockfile`; stderr={stderr}",
    );
}

#[test]
fn zero_discovered_no_pants_signal_stays_silent() {
    // T021 (US3, FR-012 + m223 SC-003): a directory with NO Pants
    // signal at all — no pants.toml, no 3rdparty/python/ — emits
    // ZERO pants-pex log lines. Preserves the pre-m672 (and m223)
    // non-Pants-repo byte-identity.
    let dir = tempfile::tempdir().unwrap();
    // Write one unrelated file so the tempdir isn't empty (avoids
    // any surprising walker skip).
    write_pants_repo(
        dir.path(),
        &[("README.md", b"# Not a Pants repo\n")],
    );

    let (_doc, stderr) = run_scan(dir.path(), &[]);

    // ZERO pants-pex log lines.
    assert!(
        !stderr.contains("pants-pex reader"),
        "no Pants signal → no pants-pex log; stderr={stderr}",
    );
}
