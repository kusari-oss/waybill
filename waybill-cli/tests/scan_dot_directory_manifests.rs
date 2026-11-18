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

/// D-10: a JavaScript action under `.github/actions/` ships its committed
/// `node_modules/`, which the npm reader reads as the action's installed
/// tree (as it does for any project root). Its dependency is Development
/// with C62 `ci-directory`. No project root is found *inside*
/// `node_modules/`: a nested `package.json` there is a package, not a project.
#[test]
fn d10_javascript_action_dependency_is_development_ci_directory() {
    let t = tree();
    let r = t.path();
    requirements(r, "requirements.txt", &["waybill-fixture-root"]);
    write(
        r,
        ".github/actions/waybill-fixture-act/package.json",
        r#"{"name":"waybill-fixture-act","version":"1.0.0","dependencies":{"waybill-fixture-npm-a":"1.0.0"}}"#,
    );
    write(
        r,
        ".github/actions/waybill-fixture-act/node_modules/waybill-fixture-npm-a/package.json",
        r#"{"name":"waybill-fixture-npm-a","version":"1.0.0"}"#,
    );
    let d = scan(r);
    let c = present(&d, "waybill-fixture-npm-a");
    assert_eq!(scope(c), "excluded", "Development is non-runtime");
    assert_eq!(prop(c, C62), Some("ci-directory"));
    assert_eq!(scope(present(&d, "waybill-fixture-root")), "required");
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

/// Three shapes from the quality corpus's pytorch pin, each wrong on the first
/// implementation:
/// - a package also declared in a NESTED project's unscoped file is runtime
///   (the pip reader's cross-project merge was first-wins);
/// - a package in two non-runtime files stays non-runtime (two `Optional`
///   declarations merged to no scope, read as runtime);
/// - a project under `.ci/` declares its dependencies with an explicit
///   runtime scope, and they are CI tooling all the same.
#[test]
fn d11b_nested_unscoped_files_two_scoped_files_and_ci_projects() {
    let t = tree();
    let r = t.path();
    requirements(r, ".ci/docker/requirements-ci.txt", &["waybill-fixture-nested", "waybill-fixture-twice"]);
    requirements(r, "scripts/notes/requirements.txt", &["waybill-fixture-nested"]);
    requirements(r, "docs/requirements.txt", &["waybill-fixture-twice"]);
    write(
        r,
        ".ci/tool/pyproject.toml",
        "[project]\nname = \"waybill-fixture-ci-tool\"\nversion = \"0.1.0\"\ndependencies = [\"waybill-fixture-tool-dep\"]\n",
    );
    let d = scan(r);
    let nested = present(&d, "waybill-fixture-nested");
    assert_eq!(scope(nested), "required", "the nested project's unscoped file is a runtime declaration");
    assert_eq!(prop(nested, C155), None);
    assert_eq!(scope(present(&d, "waybill-fixture-twice")), "excluded", "two non-runtime declarations stay non-runtime");
    let dep = present(&d, "waybill-fixture-tool-dep");
    assert_eq!(scope(dep), "excluded");
    assert_eq!(prop(dep, C62), Some("ci-directory"));
}

// ---------------------------------------------------------------------
// User story 2: caches and test suites stay skipped (T013)
// ---------------------------------------------------------------------

/// D-6, D-7, D-9: nothing from a non-listed dot-directory, from a cache
/// nested inside a listed one, or from an unlabellable requirements name.
#[test]
fn d06_d07_d09_caches_test_suites_and_unlabellable_names_stay_out() {
    let t = tree();
    let r = t.path();
    requirements(r, "requirements.txt", &["waybill-fixture-root"]);
    for (dir, name) in [
        (".venv", "waybill-fixture-venv"),
        (".tox", "waybill-fixture-tox-env"),
        (".cache", "waybill-fixture-cache"),
        (".upstream-tests", "waybill-fixture-upstream"),
        (".ci/.venv", "waybill-fixture-nested-venv"),
    ] {
        requirements(r, &format!("{dir}/requirements.txt"), &[name]);
    }
    requirements(r, "tox-requirements.txt", &["waybill-fixture-toxreq"]);
    let d = scan(r);
    present(&d, "waybill-fixture-root");
    for name in [
        "waybill-fixture-venv",
        "waybill-fixture-tox-env",
        "waybill-fixture-cache",
        "waybill-fixture-upstream",
        "waybill-fixture-nested-venv",
        "waybill-fixture-toxreq",
    ] {
        absent(&d, name);
    }
}

/// D-12a: adding manifests under non-listed dot-directories changes nothing
/// but the file tier's existing count of the directories it skipped
/// (`waybill:file-inventory-skipped-build-dirs`), which records exactly that
/// they were skipped.
#[test]
fn d12a_non_listed_dot_directories_change_nothing() {
    // The same directory name under two parents: the subject, and SPDX 3's
    // root IRI derived from it, are then the same in both documents.
    let (plain_parent, noisy_parent) = (tree(), tree());
    let plain = plain_parent.path().join("waybill-fixture-tree");
    let noisy = noisy_parent.path().join("waybill-fixture-tree");
    requirements(&plain, "requirements.txt", &["waybill-fixture-root"]);
    requirements(&noisy, "requirements.txt", &["waybill-fixture-root"]);
    for dir in [".cache", ".venv", ".tox"] {
        requirements(&noisy, &format!("{dir}/requirements.txt"), &["waybill-fixture-hidden"]);
    }
    let (a, b) = (scan(&plain), scan(&noisy));
    for (name, x, y) in [("CycloneDX", &a.cdx, &b.cdx), ("SPDX 2.3", &a.spdx23, &b.spdx23), ("SPDX 3", &a.spdx3, &b.spdx3)] {
        let (x, y) = (mask_tree_name(x), mask_tree_name(y));
        assert_eq!(masked_difference(&x, &y), None, "{name}: a non-listed dot-directory changed the document");
    }
}

/// The two trees live in different temp directories; their names reach the
/// subject. Replace every temp-directory name with one token so only the
/// content is compared.
fn mask_tree_name(v: &Value) -> Value {
    let s = serde_json::to_string(&mask_nondeterministic(v)).unwrap();
    let re = regex::Regex::new(r"\.tmp[A-Za-z0-9]{6}").unwrap();
    without_mentions(&serde_json::from_str(&re.replace_all(&s, ".tmpXXXXXX")).unwrap(), SKIP_COUNT)
}

/// The file tier's count of the directories it skipped: present in the
/// noisy tree only, in each format's own encoding.
const SKIP_COUNT: &str = "waybill:file-inventory-skipped-build-dirs";

/// `v` without any array element that mentions `needle`, at any depth.
fn without_mentions(v: &Value, needle: &str) -> Value {
    match v {
        Value::Array(a) => Value::Array(
            a.iter().filter(|e| !e.to_string().contains(needle)).map(|e| without_mentions(e, needle)).collect(),
        ),
        Value::Object(o) => Value::Object(o.iter().map(|(k, e)| (k.clone(), without_mentions(e, needle))).collect()),
        other => other.clone(),
    }
}

/// FR-011: repeated scans of the US1 tree are identical.
#[test]
fn repeated_scans_of_a_tree_with_listed_directories_are_identical() {
    let t = tree();
    let r = t.path();
    requirements(r, "requirements.txt", &["waybill-fixture-root", "waybill-fixture-both"]);
    requirements(r, ".ci/requirements.txt", &["waybill-fixture-both", "waybill-fixture-d01"]);
    requirements(r, ".github/requirements-tool-ci.txt", &["waybill-fixture-d03"]);
    write(
        r,
        ".github/actions/waybill-fixture-act/node_modules/waybill-fixture-npm-a/package.json",
        r#"{"name":"waybill-fixture-npm-a","version":"1.0.0"}"#,
    );
    write(
        r,
        ".github/actions/waybill-fixture-act/package.json",
        r#"{"name":"waybill-fixture-act","version":"1.0.0","dependencies":{"waybill-fixture-npm-a":"1.0.0"}}"#,
    );
    assert_repeatable(r);
}

// ---------------------------------------------------------------------
// User story 3: the file tier does not report CI files (T015)
// ---------------------------------------------------------------------

/// D-12c: with the file inventory on, nothing under a listed directory
/// becomes a file-tier component. `src/app.py` is the positive control: it
/// shows the file tier ran and reports source files in this mode.
#[test]
fn d12c_the_file_tier_reports_nothing_under_listed_directories() {
    let t = tree();
    let r = t.path();
    write(r, ".github/workflows/ci.yml", "on: push\njobs: {}\n");
    write(r, ".github/scripts/release.py", "print('waybill-fixture release')\n");
    write(r, ".ci/script.sh", "#!/bin/sh\necho waybill-fixture\n");
    write(r, "src/app.py", "print('waybill-fixture app')\n");
    for mode in ["full", "source-tree"] {
        let d = scan_with("--path", r, &["--file-inventory", mode]);
        let paths: Vec<String> = components(&d.cdx)
            .into_iter()
            .filter(|c| prop(c, "waybill:component-tier") == Some("file"))
            .filter_map(|c| prop(c, "waybill:file-paths").map(str::to_string))
            .collect();
        if mode == "source-tree" {
            assert!(paths.iter().any(|p| p.contains("src/app.py")), "{mode}: control file-tier entry missing: {paths:?}");
        }
        for p in &paths {
            assert!(!p.contains(".github/") && !p.contains(".ci/"), "{mode}: file-tier entry under a listed directory: {p}");
        }
    }
}
