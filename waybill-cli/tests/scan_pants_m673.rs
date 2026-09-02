//! Milestone 673 — integration tests for the Pants pex-lockfile
//! reader discovery-layout extensions: `<scan_root>/*.lock` (US1),
//! `<scan_root>/lockfiles/*.lock` (US2), and the content-detection
//! defensive guard (US3).
//!
//! Fixtures are composed at test time via `tempfile::tempdir()` (per
//! m670 T007 / m671 T012 / m672 T008 precedent — no new files
//! committed under `waybill-cli/tests/fixtures/`). Every synthetic
//! package name uses the `waybill-fixture-*` prefix per memory
//! `feedback_fixture_synthetic_package_names`.
//!
//! Test coverage (populated by T006–T007 + T010–T011 + T012–T013):
//! - US1: repo-root PEX lockfile discovery + multi-lockfile-at-root
//!   with distinct stem-derived resolve names.
//! - US2: `<root>/lockfiles/` layout discovery + non-`.lock` files
//!   in the directory get ignored.
//! - US3: non-PEX `.lock` files (Cargo, Poetry) at wide-scope paths
//!   silent-skip with zero WARN + sibling PEX still emits.
//!
//! Cross-linked: `specs/673-pants-lockfile-layouts/{spec,plan,data-model}.md`.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

/// Strip ANSI color escapes so `stderr.contains("foo=bar")` works
/// against `tracing`'s ANSI-colored key-value formatter. Copied
/// verbatim from `scan_pants_m672.rs`.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
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
/// as needed.
fn write_pants_repo(root: &Path, layout: &[(&str, &[u8])]) {
    for (rel_path, contents) in layout {
        let abs = root.join(rel_path);
        if let Some(parent) = abs.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&abs, contents).unwrap();
    }
}

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

/// Extract every emitted component's `waybill:pants-resolve`
/// property value as `Vec<(purl, resolve_name)>` sorted by purl for
/// stability.
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

/// Helper: a clean-JSON PEX lockfile body naming N synthetic packages
/// under a single locked-resolve block. Each package carries one
/// `files.pythonhosted.org` artifact URL so the m223 reader emits
/// `pkg:pypi/` PURLs (not the `pkg:generic/` fallback used when the
/// artifact list is empty or the URL is non-PyPI).
fn synth_clean_lockfile(packages: &[(&str, &str)]) -> Vec<u8> {
    let mut reqs = Vec::new();
    for (name, version) in packages {
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
/// metadata block (matching m672's `synth_legacy_lockfile` shape).
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

#[allow(dead_code)]
fn root_of(dir: &tempfile::TempDir) -> PathBuf {
    dir.path().to_path_buf()
}

// -------------------------------------------------------------------
// User Story 1 — repo-root `<resolve>.lock` discovery (Priority: P1)
// -------------------------------------------------------------------

#[test]
fn repo_root_lockfile_discovered() {
    // T006 (US1): a Pants 2.31+ default layout — `<root>/pants.toml`
    // (no `[python.resolves]` map) + `<root>/python-default.lock`
    // (PEX shape with `//`-frontmatter naming 3 synthetic packages).
    // Assert (a) 3 pypi components emit, (b) each tagged with
    // waybill:pants-resolve=python-default, (c) the m672 legacy-shape
    // counter fires because the fixture uses `//`-frontmatter.
    let dir = tempfile::tempdir().unwrap();
    let legacy_lock = synth_legacy_lockfile(&[
        ("waybill-fixture-m673-alpha", "1.0.0"),
        ("waybill-fixture-m673-beta", "2.0.0"),
        ("waybill-fixture-m673-gamma", "3.0.0"),
    ]);
    write_pants_repo(
        dir.path(),
        &[
            ("pants.toml", b"[GLOBAL]\npants_version = \"2.31.0\"\n"),
            ("python-default.lock", &legacy_lock),
        ],
    );

    let (doc, stderr) = run_scan(dir.path(), &[]);

    let pypi_purls: Vec<String> = component_purls(&doc)
        .into_iter()
        .filter(|p| p.starts_with("pkg:pypi/"))
        .collect();
    assert_eq!(
        pypi_purls.len(),
        3,
        "expected 3 pypi components from the repo-root lockfile; got {pypi_purls:#?}",
    );
    for name in ["m673-alpha", "m673-beta", "m673-gamma"] {
        assert!(
            pypi_purls.iter().any(|p| p.contains(name)),
            "missing waybill-fixture-{name} in purls: {pypi_purls:#?}",
        );
    }

    // Every emitted component tagged with resolve_name=python-default.
    let resolve_names = component_resolve_names(&doc);
    assert_eq!(resolve_names.len(), 3);
    for (_, rn) in &resolve_names {
        assert_eq!(
            rn, "python-default",
            "resolve name must come from file stem; got {rn:?}",
        );
    }

    // Reader-complete log with the m672 legacy counter firing.
    assert!(
        stderr.contains("lockfiles_discovered=1"),
        "expected discovered=1; stderr={stderr}",
    );
    assert!(
        stderr.contains("lockfiles_parsed_ok=1"),
        "expected parsed_ok=1; stderr={stderr}",
    );
    assert!(
        stderr.contains("legacy_shape_lockfiles=1"),
        "expected legacy_shape_lockfiles=1; stderr={stderr}",
    );
}

#[test]
fn multiple_repo_root_lockfiles_discovered_with_stem_names() {
    // T007 (US1): three repo-root `.lock` files, each PEX-shaped
    // (clean JSON, no `//`-frontmatter). Assert each emits its
    // synthetic package and the resolve names come from filename
    // stems. Also verifies clean-JSON shape acceptance works.
    let dir = tempfile::tempdir().unwrap();
    let default_lock = synth_clean_lockfile(&[("waybill-fixture-m673-def", "1.0.0")]);
    let mypy_lock = synth_clean_lockfile(&[("waybill-fixture-m673-mypy-dep", "2.0.0")]);
    let pytest_lock = synth_clean_lockfile(&[("waybill-fixture-m673-pytest-dep", "3.0.0")]);
    write_pants_repo(
        dir.path(),
        &[
            ("pants.toml", b"[GLOBAL]\npants_version = \"2.31.0\"\n"),
            ("python-default.lock", &default_lock),
            ("mypy.lock", &mypy_lock),
            ("pytest.lock", &pytest_lock),
        ],
    );

    let (doc, stderr) = run_scan(dir.path(), &[]);

    let pypi_purls: Vec<String> = component_purls(&doc)
        .into_iter()
        .filter(|p| p.starts_with("pkg:pypi/"))
        .collect();
    assert_eq!(
        pypi_purls.len(),
        3,
        "expected 3 pypi components; got {pypi_purls:#?}",
    );

    // Assert each resolve emits with the correct stem-derived name.
    let resolve_names = component_resolve_names(&doc);
    assert_eq!(resolve_names.len(), 3);
    let names: std::collections::BTreeSet<String> =
        resolve_names.into_iter().map(|(_, r)| r).collect();
    for expected in ["python-default", "mypy", "pytest"] {
        assert!(
            names.contains(expected),
            "missing resolve name `{expected}` in {names:?}",
        );
    }

    // Reader-complete log: 3 lockfiles discovered + parsed.
    assert!(
        stderr.contains("lockfiles_discovered=3"),
        "expected discovered=3; stderr={stderr}",
    );
    assert!(
        stderr.contains("lockfiles_parsed_ok=3"),
        "expected parsed_ok=3; stderr={stderr}",
    );
    // Clean-JSON files → zero legacy-shape counter.
    assert!(
        stderr.contains("legacy_shape_lockfiles=0"),
        "expected legacy_shape_lockfiles=0 on clean-JSON scans; stderr={stderr}",
    );
}

// -------------------------------------------------------------------
// User Story 2 — `<scan_root>/lockfiles/` directory (Priority: P1)
// -------------------------------------------------------------------

#[test]
fn lockfiles_directory_layout_discovered() {
    // T010 (US2): the `example-django` layout — `<root>/pants.toml`
    // + `<root>/lockfiles/python-default.lock` + `<root>/lockfiles/mypy.lock`,
    // both valid PEX (clean-JSON shape). Assert (a) 2 pypi components
    // emitted, (b) resolve names match filename stems, (c) reader-
    // complete log shows lockfiles_discovered=2.
    let dir = tempfile::tempdir().unwrap();
    let default_lock =
        synth_clean_lockfile(&[("waybill-fixture-m673-django-dep", "1.0.0")]);
    let mypy_lock =
        synth_clean_lockfile(&[("waybill-fixture-m673-django-mypy", "2.0.0")]);
    write_pants_repo(
        dir.path(),
        &[
            ("pants.toml", b"[GLOBAL]\npants_version = \"2.31.0\"\n"),
            ("lockfiles/python-default.lock", &default_lock),
            ("lockfiles/mypy.lock", &mypy_lock),
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
        "expected 2 pypi components from lockfiles/ dir; got {pypi_purls:#?}",
    );

    let resolve_names = component_resolve_names(&doc);
    assert_eq!(resolve_names.len(), 2);
    let names: std::collections::BTreeSet<String> =
        resolve_names.into_iter().map(|(_, r)| r).collect();
    for expected in ["python-default", "mypy"] {
        assert!(
            names.contains(expected),
            "missing resolve name `{expected}` in {names:?}",
        );
    }

    assert!(
        stderr.contains("lockfiles_discovered=2"),
        "expected discovered=2; stderr={stderr}",
    );
}

#[test]
fn lockfiles_dir_ignores_non_lock_files() {
    // T011 (US2): `<root>/lockfiles/README.md` (non-`.lock`) +
    // `<root>/lockfiles/python-default.lock` (valid PEX). Assert
    // (a) exactly 1 pypi component (from the PEX file only),
    // (b) NO WARN about README.md from the Pants reader,
    // (c) reader-complete log shows lockfiles_discovered=1.
    let dir = tempfile::tempdir().unwrap();
    let default_lock =
        synth_clean_lockfile(&[("waybill-fixture-m673-only-pex", "1.0.0")]);
    write_pants_repo(
        dir.path(),
        &[
            ("pants.toml", b"[GLOBAL]\npants_version = \"2.31.0\"\n"),
            (
                "lockfiles/README.md",
                b"# Lockfile directory README\n\nSome informational content.\n",
            ),
            ("lockfiles/python-default.lock", &default_lock),
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
        "expected 1 pypi component (PEX only); got {pypi_purls:#?}",
    );
    // README.md not in the discovery-set.
    assert!(
        stderr.contains("lockfiles_discovered=1"),
        "expected discovered=1; stderr={stderr}",
    );
    // No pants-pex WARN about README.md.
    assert!(
        !stderr.contains("pants-pex reader: ") || {
            // Detailed check: no WARN lines about README.md.
            !stderr
                .lines()
                .any(|line| line.contains("WARN") && line.contains("README.md"))
        },
        "expected no WARN about README.md; stderr={stderr}",
    );
}

// -------------------------------------------------------------------
// User Story 3 — content-detection defensive guard (Priority: P2)
// -------------------------------------------------------------------

#[test]
fn content_detection_silent_skips_cargo_and_poetry() {
    // T012 (US3, FR-004): repo-root Cargo.lock + lockfiles/poetry.lock
    // — neither is a PEX lockfile. Assert (a) the Pants reader emits
    // ZERO components tagged with waybill:pants-resolve (verifies
    // silent-skip), (b) stderr contains NO "failed to parse Pex
    // lockfile as JSON" WARN (would fire if content-detect ran full-
    // schema parse — this is the anti-regression signal).
    let dir = tempfile::tempdir().unwrap();
    let cargo_lock = b"version = 3\n\
[[package]]\n\
name = \"waybill-fixture-m673-cargo-a\"\n\
version = \"1.0.0\"\n\
source = \"registry+https://github.com/rust-lang/crates.io-index\"\n\
\n\
[[package]]\n\
name = \"waybill-fixture-m673-cargo-b\"\n\
version = \"2.0.0\"\n\
source = \"registry+https://github.com/rust-lang/crates.io-index\"\n";
    let poetry_lock = b"[metadata]\n\
lock-version = \"2.0\"\n\
python-versions = \"^3.10\"\n\
\n\
[[package]]\n\
name = \"waybill-fixture-m673-poetry-a\"\n\
version = \"1.0.0\"\n\
description = \"synthetic\"\n";
    // Minimal Cargo.toml so the cargo reader recognizes the workspace.
    let cargo_toml = b"[package]\n\
name = \"waybill-fixture-m673-root\"\n\
version = \"1.0.0\"\n\
edition = \"2021\"\n";
    write_pants_repo(
        dir.path(),
        &[
            ("Cargo.toml", cargo_toml),
            ("Cargo.lock", cargo_lock),
            ("lockfiles/poetry.lock", poetry_lock),
        ],
    );

    let (doc, stderr) = run_scan(dir.path(), &[]);

    // (a) No components carry waybill:pants-resolve — the Pants
    // reader didn't claim any of these files.
    let pants_tagged = component_resolve_names(&doc);
    assert_eq!(
        pants_tagged.len(),
        0,
        "no components should carry waybill:pants-resolve; got {pants_tagged:#?}",
    );

    // (b) No JSON-parse WARN from Pants reader about the non-PEX
    // files. This is the FR-004 silent-skip anti-regression signal.
    assert!(
        !stderr.contains("failed to parse Pex lockfile as JSON"),
        "unexpected JSON-parse WARN — FR-004 silent-skip failed; stderr={stderr}",
    );
    assert!(
        !stderr.contains("unsupported Pex lockfile version"),
        "unexpected Pex-version WARN — FR-004 silent-skip failed; stderr={stderr}",
    );
}

#[test]
fn repo_root_non_pex_lockfile_silent_skipped() {
    // T013 (US3, FR-004): repo-root Cargo.lock + repo-root valid
    // PEX python-default.lock. The Cargo.lock silent-skips; the PEX
    // emits normally. Verifies the wide-scope discovery loop's
    // content-detect gate distinguishes the two shapes.
    let dir = tempfile::tempdir().unwrap();
    let cargo_lock = b"version = 3\n\
[[package]]\n\
name = \"waybill-fixture-m673-x\"\n\
version = \"1.0.0\"\n";
    let cargo_toml = b"[package]\n\
name = \"waybill-fixture-m673-mixed-root\"\n\
version = \"1.0.0\"\n\
edition = \"2021\"\n";
    let pex_lock =
        synth_clean_lockfile(&[("waybill-fixture-m673-mixed-pex", "3.0.0")]);
    write_pants_repo(
        dir.path(),
        &[
            ("Cargo.toml", cargo_toml),
            ("Cargo.lock", cargo_lock),
            ("python-default.lock", &pex_lock),
        ],
    );

    let (doc, stderr) = run_scan(dir.path(), &[]);

    // (a) Exactly 1 pypi component from the PEX lockfile.
    let pypi_purls: Vec<String> = component_purls(&doc)
        .into_iter()
        .filter(|p| p.starts_with("pkg:pypi/"))
        .collect();
    assert_eq!(
        pypi_purls.len(),
        1,
        "expected 1 pypi component from PEX; got {pypi_purls:#?}",
    );
    assert!(
        pypi_purls[0].contains("waybill-fixture-m673-mixed-pex"),
        "unexpected PURL: {}",
        pypi_purls[0],
    );

    // (b) lockfiles_discovered=1 — Cargo.lock did not contribute.
    assert!(
        stderr.contains("lockfiles_discovered=1"),
        "expected discovered=1 (Cargo.lock silent-skipped); stderr={stderr}",
    );

    // (c) No pants-pex WARN about Cargo.lock.
    assert!(
        !stderr.contains("failed to parse Pex lockfile as JSON"),
        "expected no JSON-parse WARN; stderr={stderr}",
    );
}
