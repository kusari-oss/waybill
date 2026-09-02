//! Milestone 674 — integration tests for the uv.lock reader.
//!
//! Two ingest paths tested:
//! - US1: standalone `<scan_root>/uv.lock` discovery.
//! - US2: Pants FR-002 fallback (uv-format lockfiles discovered by
//!   m673 Pants pipeline that failed the m223 PEX-JSON parse).
//! - US3: m191 reconciler interaction with m670 pyproject.toml
//!   declared-deps fallback.
//!
//! Fixtures are committed under
//! `waybill-cli/tests/fixtures/uv_lock/` per m223 committed-fixture
//! pattern. Every synthetic package name uses the `waybill-fixture-*`
//! prefix per memory `feedback_fixture_synthetic_package_names`.
//!
//! Cross-linked: `specs/674-uv-lock-reader/{spec,plan,data-model}.md`.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

/// Strip ANSI color escapes so `stderr.contains("foo=bar")` works
/// against tracing's ANSI-colored key-value formatter.
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

/// Path to a committed fixture directory under
/// `waybill-cli/tests/fixtures/uv_lock/<name>/`.
fn fixture_root(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/uv_lock")
        .join(name)
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
/// sorted lex Vec for stable assertions.
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

/// Filter to only `pkg:pypi/*` PURLs.
fn pypi_purls(purls: &[String]) -> Vec<&String> {
    purls.iter().filter(|p| p.starts_with("pkg:pypi/")).collect()
}

/// Extract components' property `waybill:python-lockfile-format`
/// value (if any) keyed by PURL.
fn component_lockfile_format(doc: &Value) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = doc
        .get("components")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| {
                    let purl = c.get("purl")?.as_str()?.to_string();
                    let props = c.get("properties")?.as_array()?;
                    let fmt = props
                        .iter()
                        .find(|p| {
                            p.get("name").and_then(|n| n.as_str())
                                == Some("waybill:python-lockfile-format")
                        })
                        .and_then(|p| p.get("value")?.as_str().map(String::from))?;
                    Some((purl, fmt))
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

// -------------------------------------------------------------------
// User Story 1 — standalone uv-managed Python project (Priority: P1)
// -------------------------------------------------------------------

#[test]
fn standalone_uv_project_emits_pypi_components() {
    // T011 (US1, SC-001): standalone `<root>/uv.lock` at repo root
    // with pyproject.toml declaring 3 top-level deps + uv.lock
    // resolving 3 top-level + 1 transitive. Assert 4 pypi components
    // emit with SHA-256 hashes and `waybill:python-lockfile-format`
    // = "uv".
    let (doc, stderr) = run_scan(&fixture_root("minimal_uv"), &[]);

    let purls = component_purls(&doc);
    let pypi = pypi_purls(&purls);
    assert_eq!(
        pypi.len(),
        4,
        "expected 4 pypi components (3 top-level + 1 transitive); got {pypi:#?}",
    );
    for name in [
        "waybill-fixture-alpha@1.0.0",
        "waybill-fixture-alpha-dep@0.5.0",
        "waybill-fixture-beta@2.0.0",
        "waybill-fixture-gamma@3.0.0",
    ] {
        let expected = format!("pkg:pypi/{name}");
        assert!(
            pypi.iter().any(|p| p.as_str() == expected),
            "missing {expected} in purls: {pypi:#?}",
        );
    }

    // Every emitted pypi component MUST carry waybill:python-lockfile-format=uv.
    let formats = component_lockfile_format(&doc);
    let uv_tagged: Vec<&(String, String)> = formats
        .iter()
        .filter(|(_, fmt)| fmt == "uv")
        .collect();
    assert_eq!(
        uv_tagged.len(),
        4,
        "expected 4 components tagged with python-lockfile-format=uv; got {uv_tagged:#?}",
    );

    // Every component must have at least one SHA-256 hash.
    let components = doc["components"].as_array().unwrap();
    for comp in components.iter().filter(|c| {
        c.get("purl")
            .and_then(|p| p.as_str())
            .is_some_and(|s| s.starts_with("pkg:pypi/"))
    }) {
        let hashes = comp
            .get("hashes")
            .and_then(|h| h.as_array())
            .unwrap_or_else(|| panic!("component missing hashes: {comp:#?}"));
        assert!(
            hashes.iter().any(|h| {
                h.get("alg").and_then(|a| a.as_str()) == Some("SHA-256")
            }),
            "component missing SHA-256 hash: {comp:#?}",
        );
    }

    // Note: m106's `pip/uv_lock.rs` reader doesn't emit a standalone
    // "uv reader complete" log line — it's dispatched from the pip
    // reader family. Absence of a WARN + presence of 4 pypi
    // components + C157 annotation is the success signal.
    assert!(
        !stderr.contains("uv.lock parse failed"),
        "unexpected uv-lock parse WARN; stderr={stderr}",
    );
}

#[test]
fn multi_source_variants_emit_correctly() {
    // T013 (US1): every UvSource variant handled per m674 pivot to
    // enhance m106's reader. Registry (custom URL) + Editable +
    // Virtual emit as pkg:pypi (m106 backward-compat — workspace
    // members + solo-project's own package are typically editable/
    // virtual and get main-module role via m127). Git + Path + Url
    // emit as pkg:generic with source-type/source-url annotations
    // per m674 FR-005 through FR-007.
    let (doc, _stderr) = run_scan(&fixture_root("multi_source"), &[]);

    let purls = component_purls(&doc);

    // Registry + Editable + Virtual = 3 pypi components.
    let pypi = pypi_purls(&purls);
    assert_eq!(
        pypi.len(),
        3,
        "expected 3 pypi components (registry + editable + virtual); got {pypi:#?}",
    );
    assert!(
        pypi.iter().any(|p| p.as_str() == "pkg:pypi/waybill-fixture-reg@1.0.0"),
        "registry variant should emit pkg:pypi; got {pypi:#?}",
    );

    // Git + Path + Url = 3 pkg:generic components.
    let generic: Vec<&String> = purls
        .iter()
        .filter(|p| p.starts_with("pkg:generic/waybill-fixture-"))
        .collect();
    assert_eq!(
        generic.len(),
        3,
        "expected 3 pkg:generic components (git+path+url); got {generic:#?}",
    );

    // Verify the source annotations landed correctly.
    let components = doc["components"].as_array().unwrap();
    let get_prop = |comp: &Value, name: &str| -> Option<String> {
        let props = comp.get("properties")?.as_array()?;
        props
            .iter()
            .find(|p| p.get("name").and_then(|n| n.as_str()) == Some(name))
            .and_then(|p| p.get("value")?.as_str().map(String::from))
    };
    let find_by_name = |name: &str| -> &Value {
        components
            .iter()
            .find(|c| c.get("name").and_then(|n| n.as_str()) == Some(name))
            .unwrap_or_else(|| panic!("missing component named {name}"))
    };

    // Registry custom URL → waybill:pypi-source-url annotation.
    let reg = find_by_name("waybill-fixture-reg");
    assert_eq!(
        get_prop(reg, "waybill:pypi-source-url").as_deref(),
        Some("https://internal.pypi.example/simple"),
    );

    // Git → source-type=git + source-url=<git>@<rev>.
    let git = find_by_name("waybill-fixture-git");
    assert_eq!(get_prop(git, "waybill:source-type").as_deref(), Some("git"));
    assert_eq!(
        get_prop(git, "waybill:source-url").as_deref(),
        Some("https://github.com/kusari-sandbox/waybill-fixture-git.git@abc123def456"),
    );

    // Path → file://<path>. Note: m106's PackageDbEntry.source_type
    // field emits as `"local"` (m106 convention for path-source deps),
    // not `"path"` — the annotation naming matches pip's existing
    // shape for local-file installs.
    let path = find_by_name("waybill-fixture-path");
    assert_eq!(get_prop(path, "waybill:source-type").as_deref(), Some("local"));
    assert_eq!(
        get_prop(path, "waybill:source-url").as_deref(),
        Some("file://../local-package"),
    );

    // Url → source-url verbatim.
    let url = find_by_name("waybill-fixture-url");
    assert_eq!(get_prop(url, "waybill:source-type").as_deref(), Some("url"));
    assert_eq!(
        get_prop(url, "waybill:source-url").as_deref(),
        Some("https://example.test/wheel.whl"),
    );
}

#[test]
fn editable_and_virtual_kept_as_pypi_no_source_annotations() {
    // T014 (US1, m674 pivot): m674 enhanced m106's reader. m106
    // emits editable + virtual entries as `pkg:pypi/*` for
    // workspace-mode + solo-project backward-compat (see m106 test
    // `optional_dependencies_sub_table_classifies` which uses
    // `source = { virtual = "." }` on the pyproject's own package).
    // The m674 enhancement leaves this behavior intact — editable +
    // virtual keep emitting as pypi, but they do NOT get the
    // `waybill:source-type` + `waybill:source-url` annotations that
    // git/path/url variants get.
    let (doc, _stderr) = run_scan(&fixture_root("multi_source"), &[]);

    let components = doc["components"].as_array().unwrap();
    let get_prop = |comp: &Value, name: &str| -> Option<String> {
        let props = comp.get("properties")?.as_array()?;
        props
            .iter()
            .find(|p| p.get("name").and_then(|n| n.as_str()) == Some(name))
            .and_then(|p| p.get("value")?.as_str().map(String::from))
    };
    let find_by_name = |name: &str| -> Option<&Value> {
        components
            .iter()
            .find(|c| c.get("name").and_then(|n| n.as_str()) == Some(name))
    };

    // Editable emits as pkg:pypi (m106 backward-compat).
    let editable = find_by_name("waybill-fixture-editable")
        .expect("editable variant emitted (m106 keeps as pypi)");
    let editable_purl = editable.get("purl").and_then(|p| p.as_str()).unwrap_or("");
    assert!(
        editable_purl.starts_with("pkg:pypi/"),
        "editable must emit as pkg:pypi (m106 backward-compat); got {editable_purl}",
    );
    // But NO source-type/source-url annotations (only git/path/url get those).
    assert!(
        get_prop(editable, "waybill:source-type").is_none(),
        "editable must NOT carry waybill:source-type",
    );
    assert!(
        get_prop(editable, "waybill:source-url").is_none(),
        "editable must NOT carry waybill:source-url",
    );

    // Virtual: same shape as editable.
    let virt = find_by_name("waybill-fixture-virtual")
        .expect("virtual variant emitted (m106 keeps as pypi)");
    let virt_purl = virt.get("purl").and_then(|p| p.as_str()).unwrap_or("");
    assert!(
        virt_purl.starts_with("pkg:pypi/"),
        "virtual must emit as pkg:pypi (m106 backward-compat); got {virt_purl}",
    );
    assert!(get_prop(virt, "waybill:source-type").is_none());
    assert!(get_prop(virt, "waybill:source-url").is_none());
}

// -------------------------------------------------------------------
// User Story 2 — Pants FR-002 fallback (Priority: P1)
// -------------------------------------------------------------------

#[test]
fn pants_fr002_fallback_recovers_uv_shape_lockfiles() {
    // T017 (US2, SC-002): Pants monorepo with `[python.resolves]`
    // declaring 2 lockfiles at `3rdparty/python/*.lock` — the files
    // are uv-shape TOML, not PEX-shape JSON. The m673 Pants pipeline
    // discovers them + fails PEX parse + falls back to
    // `pip::uv_lock::parse_uv_lock_bytes`, emitting components with
    // `waybill:pants-resolve` preserved.
    let (doc, stderr) = run_scan(&fixture_root("pants_uv_backend"), &[]);

    let purls = component_purls(&doc);
    let pypi = pypi_purls(&purls);
    // 2 packages in python-default.lock + 1 in tools.lock = 3.
    assert_eq!(
        pypi.len(),
        3,
        "expected 3 pypi components (2 from python-default + 1 from tools); got {pypi:#?}",
    );

    // FR-002: every emitted component MUST carry the Pants
    // resolve-name annotation matching the pants.toml map key.
    let components = doc["components"].as_array().unwrap();
    let get_prop = |comp: &Value, name: &str| -> Option<String> {
        let props = comp.get("properties")?.as_array()?;
        props
            .iter()
            .find(|p| p.get("name").and_then(|n| n.as_str()) == Some(name))
            .and_then(|p| p.get("value")?.as_str().map(String::from))
    };
    let resolve_tags: std::collections::HashSet<String> = components
        .iter()
        .filter(|c| {
            c.get("purl")
                .and_then(|p| p.as_str())
                .is_some_and(|s| s.starts_with("pkg:pypi/waybill-fixture-"))
        })
        .filter_map(|c| get_prop(c, "waybill:pants-resolve"))
        .collect();
    assert!(
        resolve_tags.contains("python-default"),
        "expected `python-default` resolve tag; got {resolve_tags:?}",
    );
    assert!(
        resolve_tags.contains("tools"),
        "expected `tools` resolve tag; got {resolve_tags:?}",
    );

    // FR-002 INFO log fires per-lockfile.
    assert!(
        stderr.contains("recognized as uv.lock format after Pex parse rejection"),
        "expected uv-lock fallback INFO log; stderr={stderr}",
    );

    // Every m674-emitted component carries C157.
    let format_tags: Vec<(String, String)> = component_lockfile_format(&doc)
        .into_iter()
        .filter(|(purl, _)| purl.contains("waybill-fixture-"))
        .collect();
    assert_eq!(
        format_tags.len(),
        3,
        "expected 3 components with python-lockfile-format=uv; got {format_tags:#?}",
    );
    for (_, fmt) in &format_tags {
        assert_eq!(fmt, "uv");
    }
}
