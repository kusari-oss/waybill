//! Spec 1615 (#1614): pip requirements files under docs and test
//! directories, and named for a scope by a long or singular word.
//!
//! Contract `specs/1615-nested-req-scope/contracts/wire.md`, cases W-1..W-14,
//! W-ctl and W-rep. Every case is checked in the three formats through the
//! parity extractors for C42 (lifecycle scope) and C155 (a requirements
//! file's scope).
#![cfg(unix)]
#![allow(clippy::unwrap_used)]

mod common;
#[path = "corpus_harness_195/mod.rs"]
mod corpus_harness_195;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use common::installed_lib::{components, prop, row_all_formats, scan_with, write, Docs};
use corpus_harness_195::layer2_golden::{mask_nondeterministic, masked_difference};
use serde_json::Value;

const SCANS: usize = 20;
const C155: &str = "waybill:python-req-file-scope";

// ---------------------------------------------------------------------
// Helpers (T002)
// ---------------------------------------------------------------------

fn scan(root: &Path) -> Docs {
    let d = scan_with("--path", root, &[]);
    for row in ["C42", "C155"] {
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

/// `name` is non-runtime with C155 `label` (`Some`), or required with no
/// C155 (`None`). C155 has already been compared across the formats by
/// [`scan`].
fn assert_label(d: &Docs, name: &str, label: Option<&str>) {
    let c = present(d, name);
    match label {
        Some(l) => {
            assert_eq!(scope(c), "excluded", "{name}: labelled {l}, so non-runtime");
            assert_eq!(prop(c, C155), Some(l), "{name}: C155");
        }
        None => {
            assert_eq!(scope(c), "required", "{name}: runtime");
            assert_eq!(prop(c, C155), None, "{name}: no C155");
        }
    }
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
// W-ctl: byte-identity control (T003, FR-009)
// ---------------------------------------------------------------------

const GOLDEN_DIR: &str = "tests/fixtures/nested_req_scope/ctl-golden";
const REGEN: &str = "WAYBILL_UPDATE_NESTED_REQ_SCOPE_GOLDENS";

fn copy_dir(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for e in std::fs::read_dir(src).unwrap() {
        let e = e.unwrap();
        let to = dst.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy_dir(&e.path(), &to);
        } else {
            std::fs::copy(e.path(), &to).unwrap();
        }
    }
}

/// `v` with the harness's masks and the temporary parent's name masked.
fn masked(v: &Value) -> Value {
    let s = serde_json::to_string(&mask_nondeterministic(v)).unwrap();
    let re = regex::Regex::new(r"/[^\x22]*?/waybill-fixture-tree").unwrap();
    serde_json::from_str(&re.replace_all(&s, "/TMP/waybill-fixture-tree")).unwrap()
}

/// A tree whose every requirements file is labelled, or not, the way `main`
/// labels it: output is `main`'s. The golden was generated with `main`'s
/// binary before any source change (T003).
#[test]
fn w_ctl_is_byte_identical_to_main() {
    let parent = tempfile::tempdir().unwrap();
    let root: PathBuf = parent.path().join("waybill-fixture-tree");
    copy_dir(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/nested_req_scope/ctl"), &root);
    let d = scan(&root);
    assert_label(&d, "waybill-fixture-ctl-runtime", None);
    assert_label(&d, "waybill-fixture-ctl-dev", Some("dev"));
    assert_label(&d, "waybill-fixture-ctl-docs", Some("docs"));
    assert_label(&d, "waybill-fixture-ctl-ci", Some("ci"));
    let golden_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(GOLDEN_DIR);
    for (name, doc) in [("cdx", &d.cdx), ("spdx23", &d.spdx23), ("spdx3", &d.spdx3)] {
        let got = masked(doc);
        let path = golden_dir.join(format!("{name}.json"));
        if std::env::var(REGEN).is_ok() {
            std::fs::create_dir_all(&golden_dir).unwrap();
            std::fs::write(&path, serde_json::to_string_pretty(&got).unwrap() + "\n").unwrap();
            continue;
        }
        let want: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        if got != want {
            panic!("{name}: differs from main's output; {}", masked_difference(&want, &got).unwrap_or_default());
        }
    }
}

// ---------------------------------------------------------------------
// User story 1: a docs site's requirements are not runtime (T005)
// ---------------------------------------------------------------------

/// W-1, W-2: a `docs` directory above the parent labels the file docs, in
/// any case and at any depth.
#[test]
fn w01_w02_docs_ancestor_labels_docs() {
    let t = tree();
    let r = t.path();
    requirements(r, "proj/requirements.txt", &["waybill-fixture-runtime"]);
    requirements(r, "proj/docs/sphinx/requirements.txt", &["waybill-fixture-w01"]);
    requirements(r, "proj/docs/userguide/requirements.txt", &["waybill-fixture-w02a"]);
    requirements(r, "proj/Doc/x/y/requirements.txt", &["waybill-fixture-w02b"]);
    requirements(r, "proj/documentation/site/requirements.txt", &["waybill-fixture-w02c"]);
    let d = scan(r);
    for n in ["waybill-fixture-w01", "waybill-fixture-w02a", "waybill-fixture-w02b", "waybill-fixture-w02c"] {
        assert_label(&d, n, Some("docs"));
    }
    assert_label(&d, "waybill-fixture-runtime", None);
}

/// W-7: the nearest docs or test directory decides.
#[test]
fn w07_nearest_directory_wins() {
    let t = tree();
    let r = t.path();
    requirements(r, "proj/tests/docs/sphinx/requirements.txt", &["waybill-fixture-w07a"]);
    requirements(r, "proj/docs/tests/x/requirements.txt", &["waybill-fixture-w07b"]);
    let d = scan(r);
    assert_label(&d, "waybill-fixture-w07a", Some("docs"));
    assert_label(&d, "waybill-fixture-w07b", Some("test"));
}

/// W-8: docs/test directories and listed dot-directories are one search.
/// The first case amends spec 1613's FR-004 (spec FR-003).
#[test]
fn w08_one_search_across_dot_directories() {
    let t = tree();
    let r = t.path();
    requirements(r, "proj/.github/docs/x/requirements.txt", &["waybill-fixture-w08a"]);
    requirements(r, "proj/docs/.github/x/requirements.txt", &["waybill-fixture-w08b"]);
    let d = scan(r);
    assert_label(&d, "waybill-fixture-w08a", Some("docs"));
    assert_label(&d, "waybill-fixture-w08b", Some("ci"));
}

/// W-10: a runtime declaration anywhere keeps the package required, with
/// no C155 (FR-007).
#[test]
fn w10_runtime_declaration_wins() {
    let t = tree();
    let r = t.path();
    requirements(r, "proj/requirements.txt", &["waybill-fixture-w10"]);
    requirements(r, "proj/docs/sphinx/requirements.txt", &["waybill-fixture-w10", "waybill-fixture-w10-docs"]);
    let d = scan(r);
    assert_label(&d, "waybill-fixture-w10", None);
    assert_label(&d, "waybill-fixture-w10-docs", Some("docs"));
}

/// W-12: the scan root's own name labels nothing (FR-004).
#[test]
fn w12_scan_root_name_labels_nothing() {
    let t = tree();
    let r = t.path().join("tests");
    requirements(&r, "proj/x/requirements.txt", &["waybill-fixture-w12"]);
    let d = scan(&r);
    assert_label(&d, "waybill-fixture-w12", None);
}

/// W-13: a whole project under `tests/` is test (FR-004, clarified).
#[test]
fn w13_project_vendored_under_tests_is_test() {
    let t = tree();
    let r = t.path();
    requirements(r, "requirements.txt", &["waybill-fixture-root"]);
    requirements(r, "tests/vendored/proj/requirements.txt", &["waybill-fixture-w13"]);
    write(r, "tests/vendored/proj/setup.py", "from setuptools import setup\nsetup(name='waybill-fixture-vendored')\n");
    let d = scan(r);
    assert_label(&d, "waybill-fixture-w13", Some("test"));
    assert_label(&d, "waybill-fixture-root", None);
}

/// W-14: other directory names, and partial names, label nothing (FR-005).
#[test]
fn w14_other_and_partial_directory_names_stay_runtime() {
    let t = tree();
    let r = t.path();
    let cases = [
        ("proj/examples/a/requirements.txt", "waybill-fixture-w14a"),
        ("proj/benchmarks/b/requirements.txt", "waybill-fixture-w14b"),
        ("proj/tools/c/requirements.txt", "waybill-fixture-w14c"),
        ("proj/test_data/d/requirements.txt", "waybill-fixture-w14d"),
        ("proj/documents/e/requirements.txt", "waybill-fixture-w14e"),
        ("proj/testing_utils/f/requirements.txt", "waybill-fixture-w14f"),
        ("proj/doctor/g/requirements.txt", "waybill-fixture-w14g"),
    ];
    for (rel, n) in cases {
        requirements(r, rel, &[n]);
    }
    let d = scan(r);
    for (_, n) in cases {
        assert_label(&d, n, None);
    }
}

// ---------------------------------------------------------------------
// User story 2: a file named for documentation is labelled docs (T009)
// ---------------------------------------------------------------------

/// W-4: `doc` is the docs scope word; `doc-requirements.txt` is read.
#[test]
fn w04_doc_name_is_docs() {
    let t = tree();
    let r = t.path();
    requirements(r, "proj/requirements-doc.txt", &["waybill-fixture-w04a"]);
    requirements(r, "proj/doc-requirements.txt", &["waybill-fixture-w04b"]);
    let d = scan(r);
    assert_label(&d, "waybill-fixture-w04a", Some("docs"));
    assert_label(&d, "waybill-fixture-w04b", Some("docs"));
}

/// W-5: the long and plural forms of the scope words (FR-006).
#[test]
fn w05_long_and_plural_scope_words() {
    let t = tree();
    let r = t.path();
    requirements(r, "proj/requirements-development.txt", &["waybill-fixture-w05a"]);
    requirements(r, "proj/requirements-documentation.txt", &["waybill-fixture-w05b"]);
    requirements(r, "proj/testing-requirements.txt", &["waybill-fixture-w05c"]);
    requirements(r, "proj/requirements-tests.txt", &["waybill-fixture-w05d"]);
    let d = scan(r);
    assert_label(&d, "waybill-fixture-w05a", Some("dev"));
    assert_label(&d, "waybill-fixture-w05b", Some("docs"));
    assert_label(&d, "waybill-fixture-w05c", Some("test"));
    assert_label(&d, "waybill-fixture-w05d", Some("test"));
}

/// W-6: a word that only begins with a scope word stays runtime.
#[test]
fn w06_word_boundary_holds() {
    let t = tree();
    let r = t.path();
    requirements(r, "proj/requirements-docker.txt", &["waybill-fixture-w06a"]);
    requirements(r, "proj/requirements-testinfra.txt", &["waybill-fixture-w06b"]);
    requirements(r, "proj/requirements-devel.txt", &["waybill-fixture-w06c"]);
    let d = scan(r);
    for n in ["waybill-fixture-w06a", "waybill-fixture-w06b", "waybill-fixture-w06c"] {
        assert_label(&d, n, None);
    }
}

// ---------------------------------------------------------------------
// User story 3: requirements inside a test tree are test-only (T012)
// ---------------------------------------------------------------------

/// W-3: a `test` or `tests` directory above the parent labels the file test.
#[test]
fn w03_test_ancestor_labels_test() {
    let t = tree();
    let r = t.path();
    requirements(r, "proj/tests/corpus/dbt/requirements-x.txt", &["waybill-fixture-w03a"]);
    requirements(r, "proj/test/integration/requirements.txt", &["waybill-fixture-w03b"]);
    let d = scan(r);
    assert_label(&d, "waybill-fixture-w03a", Some("test"));
    assert_label(&d, "waybill-fixture-w03b", Some("test"));
}

// ---------------------------------------------------------------------
// Polish: W-9, W-11, W-rep (T013)
// ---------------------------------------------------------------------

/// W-9: the name beats an ancestor.
#[test]
fn w09_name_beats_ancestor() {
    let t = tree();
    let r = t.path();
    requirements(r, "proj/tests/fixtures/requirements-docs.txt", &["waybill-fixture-w09a"]);
    requirements(r, "proj/docs/x/requirements-dev.txt", &["waybill-fixture-w09b"]);
    let d = scan(r);
    assert_label(&d, "waybill-fixture-w09a", Some("docs"));
    assert_label(&d, "waybill-fixture-w09b", Some("dev"));
}

/// Every case's tree at once.
fn all_cases(r: &Path) {
    requirements(r, "proj/requirements.txt", &["waybill-fixture-runtime"]);
    requirements(r, "proj/docs/sphinx/requirements.txt", &["waybill-fixture-w01"]);
    requirements(r, "proj/tests/corpus/dbt/requirements-x.txt", &["waybill-fixture-w03"]);
    requirements(r, "proj/requirements-doc.txt", &["waybill-fixture-w04a"]);
    requirements(r, "proj/doc-requirements.txt", &["waybill-fixture-w04b"]);
    requirements(r, "proj/requirements-development.txt", &["waybill-fixture-w05a"]);
    requirements(r, "proj/testing-requirements.txt", &["waybill-fixture-w05c"]);
    requirements(r, "proj/requirements-docker.txt", &["waybill-fixture-w06"]);
    requirements(r, "proj/tests/docs/sphinx/requirements.txt", &["waybill-fixture-w07"]);
    requirements(r, "proj/.github/docs/x/requirements.txt", &["waybill-fixture-w08"]);
    requirements(r, "proj/tests/fixtures/requirements-docs.txt", &["waybill-fixture-w09"]);
    requirements(r, "proj/examples/a/requirements.txt", &["waybill-fixture-w14"]);
}

/// W-11: no component is unreachable without an orphan reason (R5).
#[test]
fn w11_no_unexplained_orphans() {
    let t = tree();
    all_cases(t.path());
    let d = scan(t.path());
    let reached: std::collections::BTreeSet<&str> = d.cdx["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|e| e["dependsOn"].as_array().into_iter().flatten().filter_map(Value::as_str))
        .collect();
    let root = d.cdx["metadata"]["component"]["bom-ref"].as_str().unwrap_or_default();
    let unexplained: Vec<&str> = components(&d.cdx)
        .into_iter()
        .filter(|c| {
            let r = c["bom-ref"].as_str().unwrap_or_default();
            r != root && !reached.contains(r) && prop(c, "waybill:orphan-reason").is_none()
        })
        .filter_map(|c| c["name"].as_str())
        .collect();
    assert!(unexplained.is_empty(), "unreachable without an orphan reason: {unexplained:?}");
}

/// W-rep: [`SCANS`] scans of every case agree.
#[test]
fn w_rep_repeatable() {
    let t = tree();
    all_cases(t.path());
    assert_repeatable(t.path());
}
