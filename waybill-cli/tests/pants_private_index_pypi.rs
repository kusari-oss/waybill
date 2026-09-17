//! Issue #901 — a Pex-locked requirement served by a private index is an
//! ordinary PyPI distribution and must be typed `pkg:pypi`.
//!
//! **The defect.** `ArtifactSourceType::from_url` typed an artifact as PyPI
//! only when its URL began `https://files.pythonhosted.org/`. Everything else
//! on http(s) fell into a "direct URL" bucket and was demoted to
//! `pkg:generic`. A repository resolving through a private index — any
//! corporate proxy or mirror — therefore got zero ecosystem-typed components
//! out of its Pex lockfiles, and nothing downstream could match them against
//! an advisory database. On one real monorepo that was 1054 of 1826
//! components.
//!
//! **The policy.** If the name and version match, treat it as the upstream
//! project. This is not a new rule: `pip/uv_lock.rs` already types by source
//! *kind* — `Registry { url }` emits `pkg:pypi` and never inspects the host —
//! so a uv project behind the same private index was already typed correctly.
//! The Pex reader was the outlier, and waybill contradicted itself between two
//! of its own Python readers.
//!
//! **What the policy costs**, tracked at #909: an internally-built package
//! whose name collides with a real PyPI project now claims that project's
//! identity. The mitigation kept here is that the artifact URL stays on the
//! component, so the question remains answerable from the emitted document.

use std::path::PathBuf;
use std::process::Command;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pants_pex/non_pypi_entries")
}

fn scan() -> serde_json::Value {
    let out = tempfile::tempdir().expect("tempdir");
    let path = out.path().join("actual.cdx.json");
    let status = Command::new(env!("CARGO_BIN_EXE_waybill"))
        .args([
            "sbom",
            "scan",
            "--path",
            fixture().to_str().expect("fixture"),
            "--offline",
            "--format",
            "cyclonedx-json",
            "--output",
            path.to_str().expect("out"),
        ])
        .status()
        .expect("run waybill");
    assert!(status.success(), "scan failed: {status}");
    serde_json::from_slice(&std::fs::read(&path).expect("read")).expect("parse")
}

/// Find a component by the fixture package name embedded in its PURL, without
/// assuming which PURL *type* it carries — that is the thing under test.
fn component_named<'a>(doc: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    doc["components"]
        .as_array()
        .expect("components")
        .iter()
        .find(|c| c["name"].as_str() == Some(name))
        .unwrap_or_else(|| panic!("no component named {name}"))
}

fn prop(c: &serde_json::Value, key: &str) -> Option<String> {
    c["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"].as_str() == Some(key))
        .and_then(|p| p["value"].as_str())
        .map(str::to_string)
}

/// The fix. A wheel served from a host that is not `files.pythonhosted.org`
/// is still a PyPI distribution of a named project at a pinned version.
#[test]
fn private_index_wheel_is_typed_pypi() {
    let doc = scan();
    let c = component_named(&doc, "waybill-fixture-url");
    assert_eq!(
        c["purl"].as_str(),
        Some("pkg:pypi/waybill-fixture-url@1.0.0"),
        "a wheel from a private index must carry PyPI identity, or nothing \
         downstream can match it against an advisory. Component:\n{c:#?}"
    );
}

/// #909's mitigation. Typing it `pkg:pypi` asserts an identity; keeping the
/// artifact URL is what lets someone later check whether that assertion holds.
/// If this regresses, the collision question stops being answerable from the
/// document at all.
#[test]
fn private_index_component_retains_its_artifact_url() {
    let doc = scan();
    let c = component_named(&doc, "waybill-fixture-url");
    let url = prop(c, "waybill:source-url").unwrap_or_else(|| {
        panic!("private-index component lost waybill:source-url:\n{c:#?}")
    });
    assert!(
        url.starts_with("https://mirror.example.test/"),
        "source-url should name the index that actually served it, got {url}"
    );
}

/// A canonically-hosted wheel is unchanged: pypi, and no source-url, because
/// there is nothing non-obvious to record about it.
#[test]
fn canonical_pypi_wheel_is_unchanged() {
    let doc = scan();
    let c = component_named(&doc, "waybill-fixture-normal");
    assert_eq!(
        c["purl"].as_str(),
        Some("pkg:pypi/waybill-fixture-normal@1.0.0")
    );
    assert_eq!(
        prop(c, "waybill:source-url"),
        None,
        "a canonical PyPI wheel needs no source-url; its absence is what \
         distinguishes it from a private-index one"
    );
}

/// The policy widens which URLs mean PyPI. It must not swallow the two kinds
/// that genuinely are not PyPI distributions — a VCS checkout and a local
/// file — or the fix would trade one wrong identity for another.
#[test]
fn git_and_local_sources_stay_generic() {
    let doc = scan();
    for (name, expected_type, url_prefix) in [
        ("waybill-fixture-git", "git", "git+https://example.test/"),
        ("waybill-fixture-local", "local", "file:///"),
    ] {
        let c = component_named(&doc, name);
        let purl = c["purl"].as_str().unwrap_or_default();
        assert!(
            purl.starts_with("pkg:generic/"),
            "{name} is not a PyPI distribution and must stay generic, got {purl}"
        );
        assert_eq!(prop(c, "waybill:source-type").as_deref(), Some(expected_type));
        assert!(prop(c, "waybill:source-url")
            .unwrap_or_default()
            .starts_with(url_prefix));
    }
}
