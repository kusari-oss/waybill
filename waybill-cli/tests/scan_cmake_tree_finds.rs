//! Spec 1614 (#1591, #1605): package searches (`find_package`,
//! `pkg_check_modules`, `pkg_search_module`) read from every CMake file, with
//! the conditions that reach them.
//!
//! Contract `specs/1614-cmake-tree-finds/contracts/wire.md`, cases W-1..W-17,
//! W-ctl and W-rep. Every case is checked in the three formats through the
//! parity extractors.
#![cfg(unix)]
#![allow(clippy::unwrap_used)]

mod common;
#[path = "corpus_harness_195/mod.rs"]
mod corpus_harness_195;

use std::path::{Path, PathBuf};

use common::installed_lib::{row_all_formats, scan_with, Docs};
use corpus_harness_195::layer2_golden::{mask_nondeterministic, masked_difference};
use serde_json::Value;

/// C237, new in this feature: present whenever package searches are read,
/// so the byte-identity control compares documents without it (FR-009).
const C237: &str = "waybill:cmake-find-coverage";

// ---------------------------------------------------------------------
// Helpers (T002)
// ---------------------------------------------------------------------

/// All three formats of a scan of `root`, with every catalog row this
/// feature touches asserted to agree across them.
fn scan(root: &Path) -> Docs {
    let d = scan_with("--path", root, &[]);
    for row in ["C42", "C62", "C122", "C232"] {
        row_all_formats(&d, row);
    }
    d
}

/// The fixture tree copied under a fresh temporary parent, as a directory
/// with a fixed name: SPDX 3 IRIs and the document name depend on it.
fn copy_fixture(rel: &str) -> (tempfile::TempDir, PathBuf) {
    let parent = tempfile::tempdir().unwrap();
    let dst = parent.path().join("waybill-fixture-tree");
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(rel);
    copy_dir(&src, &dst);
    (parent, dst)
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

/// `v` masked for comparison across scans and hosts: the harness's masks,
/// the temporary parent's name, and C237 removed.
fn comparable(v: &Value) -> Value {
    let s = serde_json::to_string(&mask_nondeterministic(v)).unwrap();
    let re = regex::Regex::new(r"/[^\x22]*?/waybill-fixture-tree").unwrap();
    without_mentions(&serde_json::from_str(&re.replace_all(&s, "/TMP/waybill-fixture-tree")).unwrap(), C237)
}

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

// ---------------------------------------------------------------------
// W-ctl: byte-identity control (T003, FR-009)
// ---------------------------------------------------------------------

const GOLDEN_DIR: &str = "tests/fixtures/cmake_tree_finds/ctl-golden";
const REGEN: &str = "WAYBILL_UPDATE_CMAKE_TREE_FINDS_GOLDENS";

/// Every package search in the narrow file set, unconditional, and a fetch
/// with no chain conditions: output is `main`'s, apart from C237. The golden
/// was generated with `main`'s binary before any source change (T003).
#[test]
fn w_ctl_is_byte_identical_to_main() {
    let (_parent, root) = copy_fixture("cmake_tree_finds/ctl");
    let d = scan(&root);
    let golden_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(GOLDEN_DIR);
    for (name, doc) in [("cdx", &d.cdx), ("spdx23", &d.spdx23), ("spdx3", &d.spdx3)] {
        let got = comparable(doc);
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
