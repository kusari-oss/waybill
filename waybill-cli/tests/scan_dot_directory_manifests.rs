//! Spec 1613 (#1610): dependency manifests in CI and docs dot-directories.
//!
//! Contract `specs/1613-dot-directory-manifests/contracts/wire.md`, cases
//! D-1..D-12c. Every case is checked in the three formats through the parity
//! extractors for C42 (lifecycle scope), C62 (its derivation) and C155 (a
//! requirements file's scope).
#![cfg(unix)]
#![allow(clippy::unwrap_used)]

mod common;
#[path = "corpus_harness_195/mod.rs"]
mod corpus_harness_195;

use std::collections::BTreeMap;
use std::path::Path;

use common::installed_lib::{components, prop, row_all_formats, scan_with, write, Docs};
use corpus_harness_195::layer2_golden::{mask_nondeterministic, masked_difference};
use serde_json::Value;

const SCANS: usize = 20;
const C155: &str = "waybill:python-req-file-scope";
const C62: &str = "waybill:lifecycle-scope-derivation";

// ---------------------------------------------------------------------
// Helpers (T002)
// ---------------------------------------------------------------------

fn scan(root: &Path) -> Docs {
    let d = scan_with("--path", root, &[]);
    for row in ["C42", "C62", "C155"] {
        row_all_formats(&d, row);
    }
    d
}

/// The one component named `name`, nested ones included.
fn named<'a>(d: &'a Docs, name: &str) -> Option<&'a Value> {
    let mut found = components(&d.cdx).into_iter().filter(|c| c["name"] == name);
    let first = found.next();
    assert!(found.next().is_none(), "{name} appears more than once");
    first
}

fn present<'a>(d: &'a Docs, name: &str) -> &'a Value {
    named(d, name).unwrap_or_else(|| {
        let all: Vec<&str> = components(&d.cdx).into_iter().filter_map(|c| c["name"].as_str()).collect();
        panic!("{name} missing; components: {all:?}")
    })
}

fn absent(d: &Docs, name: &str) {
    assert!(named(d, name).is_none(), "{name} must not appear");
}

/// CycloneDX `scope`, which is absent for a required component.
fn scope(c: &Value) -> &str {
    c["scope"].as_str().unwrap_or("required")
}

fn requirements(root: &Path, rel: &str, names: &[&str]) {
    let body: String = names.iter().map(|n| format!("{n}==1.0\n")).collect();
    write(root, rel, &body);
}

fn tree() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

/// [`SCANS`] scans of `root` give one masked document per format.
fn assert_repeatable(root: &Path) {
    let docs: Vec<Docs> = (0..SCANS).map(|_| scan_with("--path", root, &[])).collect();
    for (f, name) in ["CycloneDX", "SPDX 2.3", "SPDX 3"].iter().enumerate() {
        let mut variants: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (i, d) in docs.iter().enumerate() {
            let v = [&d.cdx, &d.spdx23, &d.spdx3][f];
            variants.entry(serde_json::to_string_pretty(&mask_nondeterministic(v)).unwrap()).or_default().push(i);
        }
        if variants.len() > 1 {
            let parsed: Vec<Value> = variants.keys().map(|s| serde_json::from_str(s).unwrap()).collect();
            panic!(
                "{name}: {} variants across {SCANS} scans; {}",
                variants.len(),
                masked_difference(&parsed[0], &parsed[1]).unwrap_or_default()
            );
        }
    }
}

// ---------------------------------------------------------------------
// User story 1: CI and docs packages appear, labelled (T005)
// ---------------------------------------------------------------------

/// D-1..D-5: a requirements file under a listed directory, at any depth and
/// in any case, is read and labelled by the directory's class.
#[test]
fn d01_to_d05_requirements_under_listed_directories_are_read_and_labelled() {
    let t = tree();
    let r = t.path();
    requirements(r, "requirements.txt", &["waybill-fixture-root"]);
    requirements(r, ".ci/requirements.txt", &["waybill-fixture-d01"]);
    requirements(r, "sub/.ci/metrics/requirements.txt", &["waybill-fixture-d02"]);
    requirements(r, ".github/requirements-tool-ci.txt", &["waybill-fixture-d03"]);
    requirements(r, "docs/.sphinx/requirements.txt", &["waybill-fixture-d04"]);
    requirements(r, "other/.CI/requirements.txt", &["waybill-fixture-d05"]);
    let d = scan(r);
    for (name, class) in [
        ("waybill-fixture-d01", "ci"),
        ("waybill-fixture-d02", "ci"),
        ("waybill-fixture-d03", "ci"),
        ("waybill-fixture-d04", "docs"),
        ("waybill-fixture-d05", "ci"),
    ] {
        let c = present(&d, name);
        assert_eq!(scope(c), "excluded", "{name}: a CI or docs package is non-runtime");
        assert_eq!(prop(c, C155), Some(class), "{name}: C155");
    }
    assert_eq!(scope(present(&d, "waybill-fixture-root")), "required");
    assert_eq!(prop(present(&d, "waybill-fixture-root"), C155), None);
}

/// D-8: the selector reads `*requirements*.txt` under a listed directory
/// and `<scope>-requirements*.txt` anywhere.
#[test]
fn d08_widened_selector_reads_all_requirements_and_dev_requirements() {
    let t = tree();
    let r = t.path();
    requirements(r, ".ci/all_requirements.txt", &["waybill-fixture-all"]);
    requirements(r, "dev-requirements.txt", &["waybill-fixture-dev"]);
    let d = scan(r);
    assert_eq!(prop(present(&d, "waybill-fixture-all"), C155), Some("ci"));
    assert_eq!(prop(present(&d, "waybill-fixture-dev"), C155), Some("dev"));
    assert_eq!(scope(present(&d, "waybill-fixture-dev")), "excluded");
}

/// D-10: a JavaScript action's `package.json` under `.github/actions/` is
/// read, its dependency is Development with C62 `ci-directory`, and a
/// committed `node_modules/` stays skipped.
#[test]
fn d10_javascript_action_dependency_is_development_ci_directory() {
    let t = tree();
    let r = t.path();
    write(
        r,
        ".github/actions/waybill-fixture-act/package.json",
        r#"{"name":"waybill-fixture-act","version":"1.0.0","dependencies":{"waybill-fixture-npm-a":"1.0.0"}}"#,
    );
    write(
        r,
        ".github/actions/waybill-fixture-act/node_modules/waybill-fixture-npm-junk/package.json",
        r#"{"name":"waybill-fixture-npm-junk","version":"9.9.9"}"#,
    );
    let d = scan(r);
    let c = present(&d, "waybill-fixture-npm-a");
    assert_eq!(scope(c), "excluded", "Development is non-runtime");
    assert_eq!(prop(c, C62), Some("ci-directory"));
    absent(&d, "waybill-fixture-npm-junk");
}

/// D-11 (FR-006, research R8): a package declared under `.ci/` and at the
/// root appears once, with the runtime scope and no CI label.
#[test]
fn d11_a_package_also_declared_at_the_root_keeps_the_runtime_scope_and_no_ci_label() {
    let t = tree();
    let r = t.path();
    requirements(r, ".ci/requirements.txt", &["waybill-fixture-both", "waybill-fixture-cionly"]);
    requirements(r, "requirements.txt", &["waybill-fixture-both"]);
    let d = scan(r);
    let both = present(&d, "waybill-fixture-both");
    assert_eq!(scope(both), "required");
    assert_eq!(prop(both, C155), None, "a runtime component carries no CI label");
    assert_eq!(prop(present(&d, "waybill-fixture-cionly"), C155), Some("ci"));
}
