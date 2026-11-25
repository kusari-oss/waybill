//! Spec 1616 (#1618): spec 1269's scope propagation protects the runtime
//! dependencies of every runtime project, not only one main module per
//! ecosystem.
//!
//! Contract `specs/1616-propagation-seeds/contracts/wire.md`, cases P-1..P-10
//! and P-rep. Every case is checked in the three formats through the parity
//! extractors for C42 (lifecycle scope) and C62 (its derivation).
#![cfg(unix)]
#![allow(clippy::unwrap_used)]

mod common;
#[path = "corpus_harness_195/mod.rs"]
mod corpus_harness_195;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use common::installed_lib::{components, prop, row_all_formats, scan_with, Docs};
use corpus_harness_195::layer2_golden::{mask_nondeterministic, masked_difference};
use serde_json::Value;

const SCANS: usize = 20;
const C62: &str = "waybill:lifecycle-scope-derivation";

// ---------------------------------------------------------------------
// Helpers (T003)
// ---------------------------------------------------------------------

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/propagation_seeds").join(name)
}

/// A copy of fixture `name` in a temporary directory, so a scan's paths and
/// root name do not depend on where the repository is checked out.
fn copied(name: &str) -> (tempfile::TempDir, PathBuf) {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("waybill-fixture-tree");
    copy_dir(&fixture(name), &root);
    (parent, root)
}

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

fn scan(root: &Path, extra: &[&str]) -> Docs {
    let d = scan_with("--path", root, extra);
    for row in ["C42", "C62"] {
        row_all_formats(&d, row);
    }
    d
}

fn component<'a>(d: &'a Docs, name: &str) -> &'a Value {
    let found: Vec<&Value> = components(&d.cdx).into_iter().filter(|c| c["name"] == name).collect();
    assert_eq!(found.len(), 1, "{name}: expected exactly one component");
    found[0]
}

fn assert_required(d: &Docs, name: &str) {
    let c = component(d, name);
    assert_eq!(c["scope"].as_str().unwrap_or("required"), "required", "{name}: runtime");
    assert_eq!(prop(c, C62), None, "{name}: no derivation");
}

fn assert_transitive(d: &Docs, name: &str) {
    let c = component(d, name);
    assert_eq!(c["scope"].as_str(), Some("excluded"), "{name}: non-runtime");
    assert_eq!(prop(c, C62), Some("transitive"), "{name}: propagated");
}

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
// P-7: byte-identity control (T004, FR-008)
// ---------------------------------------------------------------------

const GOLDEN_DIR: &str = "tests/fixtures/propagation_seeds/ctl-golden";
const REGEN: &str = "WAYBILL_UPDATE_PROPAGATION_SEEDS_GOLDENS";

fn masked(v: &Value) -> Value {
    let s = serde_json::to_string(&mask_nondeterministic(v)).unwrap();
    let re = regex::Regex::new(r"/[^\x22]*?/waybill-fixture-tree").unwrap();
    serde_json::from_str(&re.replace_all(&s, "/TMP/waybill-fixture-tree")).unwrap()
}

/// One main module: it is already the seed, so the output is `main`'s. The
/// golden was generated with `main`'s binary before any source change.
/// Spec 1654 (#1695, R7) regenerated it once: the promoted root's CycloneDX
/// `metadata.component` gained `waybill:python-lockfile-format`, which both
/// SPDX goldens already carried on the root package.
#[test]
fn p07_one_main_module_is_byte_identical_to_main() {
    let (_p, root) = copied("ctl");
    let d = scan(&root, &[]);
    assert_transitive(&d, "waybill-fixture-devtool-core");
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
// User story 1: every project's runtime dependencies stay runtime (T006)
// ---------------------------------------------------------------------

/// P-1, P-2, P-3: `b` is not the seeded project, yet its runtime closure is
/// protected; `a`'s own runtime dependency stays protected by the base seed;
/// a package only the dev tool reaches still takes the dev scope.
#[test]
fn p01_p03_an_unseeded_projects_runtime_closure_is_protected() {
    let (_p, root) = copied("two");
    let d = scan(&root, &[]);
    assert_required(&d, "waybill-fixture-shipped");
    assert_required(&d, "waybill-fixture-shipped-dep");
    assert_required(&d, "waybill-fixture-a-runtime");
    assert_transitive(&d, "waybill-fixture-devonly");
}

/// P-10: the same under an operator root override (research R1).
#[test]
fn p10_root_override_keeps_main_modules_as_seeds() {
    let (_p, root) = copied("two");
    let d = scan(&root, &["--root-name", "waybill-fixture-root", "--root-version", "1.0.0"]);
    assert_required(&d, "waybill-fixture-shipped");
    assert_required(&d, "waybill-fixture-shipped-dep");
    assert_required(&d, "waybill-fixture-a-runtime");
    assert_transitive(&d, "waybill-fixture-devonly");
}

// ---------------------------------------------------------------------
// User story 3: the record shows what the wider protection did (T015)
// ---------------------------------------------------------------------

/// P-8: the propagation's INFO line reports what only the added seeds
/// protected: `shipped` and `shipped-dep`, both protected by `b` alone.
#[test]
fn p08_the_summary_reports_what_added_seeds_protected() {
    let (_p, root) = copied("two");
    let home = tempfile::tempdir().unwrap();
    let out_dir = tempfile::tempdir().unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_waybill"))
        .env("HOME", home.path())
        .env("RUST_LOG", "info")
        .args(["--offline", "sbom", "scan", "--path"])
        .arg(&root)
        .args(["--format", "cyclonedx-json", "--output"])
        .arg(format!("cyclonedx-json={}", out_dir.path().join("o.json").display()))
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let ansi = regex::Regex::new(r"\x1b\[[0-9;]*m").unwrap();
    let stderr = ansi.replace_all(&String::from_utf8_lossy(&out.stderr), "").to_string();
    let line = stderr
        .lines()
        .find(|l| l.contains("propagated non-runtime lifecycle scope"))
        .unwrap_or_else(|| panic!("no propagation line in:\n{stderr}"));
    assert!(line.contains("immune_by_added_seeds=2"), "{line}");
}

// ---------------------------------------------------------------------
// User story 2: a non-runtime project protects nothing (T012)
// ---------------------------------------------------------------------

/// P-4: a main module that is itself non-runtime (an editable dev-group
/// member, as TheRock's `rocisa`) is not a seed.
#[test]
fn p04_a_non_runtime_main_module_is_not_a_seed() {
    let (_p, root) = copied("nonruntime");
    let d = scan(&root, &[]);
    assert_eq!(component(&d, "waybill-fixture-c")["scope"].as_str(), Some("excluded"));
    assert_transitive(&d, "waybill-fixture-cdep");
}

/// P-5: a main module whose manifests all lie under a listed dot-directory
/// is not a seed: spec 1613 makes it non-runtime after propagation.
#[test]
fn p05_a_project_under_a_dot_directory_is_not_a_seed() {
    let (_p, root) = copied("dotdir");
    let d = scan(&root, &[]);
    assert_eq!(prop(component(&d, "waybill-fixture-action"), C62), Some("ci-directory"));
    assert_transitive(&d, "waybill-fixture-actdep");
}

// ---------------------------------------------------------------------
// Polish: P-rep (T016)
// ---------------------------------------------------------------------

#[test]
fn p_rep_repeatable() {
    for name in ["two", "nonruntime", "dotdir"] {
        let (_p, root) = copied(name);
        assert_repeatable(&root);
    }
}
