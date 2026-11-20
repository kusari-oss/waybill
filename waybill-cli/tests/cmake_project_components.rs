//! Spec 1618 (#1616): a component for each CMake project the scanned tree
//! builds.
//!
//! Contract `specs/1618-cmake-project-components/contracts/wire.md`, cases
//! P-1..P-19, P-ctl and P-rep. Every case is checked in the three formats
//! through the parity extractors.
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
const ROWS: &[&str] = &["C42", "C55", "C62", "C197", "C236", "C237"];

// ---------------------------------------------------------------------
// Helpers (T003)
// ---------------------------------------------------------------------

fn scan_args(root: &Path, extra: &[&str]) -> Docs {
    let d = scan_with("--path", root, extra);
    for row in ROWS {
        row_all_formats(&d, row);
    }
    d
}

fn scan(root: &Path) -> Docs {
    scan_args(root, &[])
}

#[allow(dead_code)]
fn by_purl<'a>(d: &'a Docs, purl: &str) -> &'a Value {
    let found: Vec<&Value> = components(&d.cdx).into_iter().filter(|c| c["purl"] == purl).collect();
    assert_eq!(found.len(), 1, "{purl}: expected exactly one; all: {:?}", purls(d));
    found[0]
}

fn purls(d: &Docs) -> Vec<&str> {
    components(&d.cdx).into_iter().filter_map(|c| c["purl"].as_str()).collect()
}

/// The CycloneDX `dependsOn` of `bom_ref`, sorted.
#[allow(dead_code)]
fn edges_from(d: &Docs, bom_ref: &str) -> Vec<String> {
    let mut out: Vec<String> = d.cdx["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|e| e["ref"] == bom_ref)
        .flat_map(|e| e["dependsOn"].as_array().cloned().unwrap_or_default())
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();
    out.sort();
    out
}

#[allow(dead_code)]
fn root_of(d: &Docs) -> &Value {
    &d.cdx["metadata"]["component"]
}

#[allow(dead_code)]
fn c55(c: &Value) -> Option<&str> {
    prop(c, "waybill:source-mechanism")
}

/// How a test writes a project's `VERSION`.
#[allow(dead_code)]
enum Version<'a> {
    Literal(&'a str),
    None,
    Raw(&'a str),
}

/// `<dir>/CMakeLists.txt` declaring `project(<name> …)`, plus `extra` lines.
#[allow(dead_code)]
fn write_project(root: &Path, dir: &str, name: &str, version: Version<'_>, extra: &str) {
    let v = match version {
        Version::Literal(v) => format!(" VERSION {v}"),
        Version::None => String::new(),
        Version::Raw(r) => format!(" {r}"),
    };
    let rel = if dir.is_empty() { "CMakeLists.txt".to_string() } else { format!("{dir}/CMakeLists.txt") };
    write(root, &rel, &format!("project({name}{v})\n{extra}"));
}

fn tree() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

#[allow(dead_code)]
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
// P-ctl: byte-identity control (T004, FR-012)
// ---------------------------------------------------------------------

const GOLDEN_DIR: &str = "tests/fixtures/cmake_project_components/ctl-golden";
const REGEN: &str = "WAYBILL_UPDATE_CMAKE_PROJECT_GOLDENS";

fn masked(v: &Value) -> Value {
    let s = serde_json::to_string(&mask_nondeterministic(v)).unwrap();
    let re = regex::Regex::new(r"/[^\x22]*?/waybill-fixture-tree").unwrap();
    serde_json::from_str(&re.replace_all(&s, "/TMP/waybill-fixture-tree")).unwrap()
}

/// CMake fetches and searches, and no `project()`: `main`'s output,
/// including C237 with no `paired.project` key. The golden was generated
/// with `main`'s binary before any source change (T004).
#[test]
fn p_ctl_is_byte_identical_to_main() {
    let parent = tree();
    let root: PathBuf = parent.path().join("waybill-fixture-tree");
    write(
        &root,
        "CMakeLists.txt",
        "cmake_minimum_required(VERSION 3.20)\n\
         include(FetchContent)\n\
         FetchContent_Declare(waybill-fixture-fetch GIT_REPOSITORY https://github.com/example/waybill-fixture-fetch GIT_TAG v1.0.0)\n\
         find_package(waybill-fixture-found REQUIRED)\n\
         if(WAYBILL_FIXTURE_OPT)\n  find_package(waybill-fixture-optional)\nendif()\n",
    );
    write(&root, "sub/waybill-fixture-subConfig.cmake.in", "# config template\n");
    write(&root, "sub/CMakeLists.txt", "find_package(waybill-fixture-sub)\n");
    let d = scan(&root);
    assert!(purls(&d).iter().any(|p| p.contains("waybill-fixture-fetch")), "{:?}", purls(&d));
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
