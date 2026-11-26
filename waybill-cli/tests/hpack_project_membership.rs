//! Spec 1662 (#1685): a `package.yaml` that the governing `cabal.project`
//! does not declare is not emitted, as #1032 already does for a `.cabal`.
//!
//! Positive control (FR-007): one declared and one undeclared hpack package,
//! with no `.cabal` files in the tree, so only the hpack reader can produce
//! either component.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeSet;
use std::process::Command;

use serde_json::Value;

const ROOT: &str = "waybill-fixture-root";

fn scan(files: &[(&str, &str)]) -> Value {
    let dir = tempfile::tempdir().unwrap();
    for (rel, text) in files {
        let p = dir.path().join(ROOT).join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    let out_dir = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let cdx = out_dir.path().join("waybill.cdx.json");
    let out = Command::new(env!("CARGO_BIN_EXE_waybill"))
        .env("HOME", home.path())
        .env("NO_COLOR", "1")
        .args(["--offline", "sbom", "scan", "--path"])
        .arg(dir.path().join(ROOT))
        .args(["--no-deep-hash", "--format", "cyclonedx-json", "--output"])
        .arg(format!("cyclonedx-json={}", cdx.display()))
        .output()
        .unwrap();
    assert!(out.status.success(), "scan failed: {}", String::from_utf8_lossy(&out.stderr));
    serde_json::from_slice(&std::fs::read(cdx).unwrap()).unwrap()
}

/// Every `pkg:hackage/<name>` name in the document, components nested or not.
fn hackage_names(v: &Value) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut stack = vec![v];
    while let Some(v) = stack.pop() {
        match v {
            Value::Object(m) => {
                if let Some(p) = m.get("purl").and_then(Value::as_str) {
                    if let Some(rest) = p.strip_prefix("pkg:hackage/") {
                        out.insert(rest.split(['@', '?']).next().unwrap().to_string());
                    }
                }
                stack.extend(m.values());
            }
            Value::Array(a) => stack.extend(a.iter()),
            _ => {}
        }
    }
    out
}

fn manifest(name: &str) -> String {
    format!("name: {name}\nversion: 0.1.0\nlibrary:\n  dependencies:\n    - base\n")
}

#[test]
fn undeclared_package_yaml_is_not_emitted() {
    let a = manifest("waybill-fixture-hpack-a");
    let b = manifest("waybill-fixture-hpack-b");
    let doc = scan(&[
        ("cabal.project", "packages: ./pkgs/a\n"),
        ("pkgs/a/package.yaml", &a),
        ("pkgs/b/package.yaml", &b),
    ]);
    let names = hackage_names(&doc);
    assert!(names.contains("waybill-fixture-hpack-a"), "declared package missing: {names:?}");
    assert!(!names.contains("waybill-fixture-hpack-b"), "undeclared package emitted: {names:?}");
}

/// The same tree without `cabal.project`: nothing governs it, so both stay
/// (FR-004). Guards against a filter that drops everything.
#[test]
fn ungoverned_package_yamls_are_both_emitted() {
    let a = manifest("waybill-fixture-hpack-a");
    let b = manifest("waybill-fixture-hpack-b");
    let names = hackage_names(&scan(&[("pkgs/a/package.yaml", &a), ("pkgs/b/package.yaml", &b)]));
    assert!(names.contains("waybill-fixture-hpack-a") && names.contains("waybill-fixture-hpack-b"), "{names:?}");
}

/// C200 (`waybill:hpack-read-outcome`) as one string, wherever it sits.
fn c200(v: &Value) -> String {
    let mut out = String::new();
    let mut stack = vec![v];
    while let Some(v) = stack.pop() {
        match v {
            Value::Object(m) => {
                if m.get("name").and_then(Value::as_str) == Some("waybill:hpack-read-outcome") {
                    out.push_str(&m.get("value").map(Value::to_string).unwrap_or_default());
                }
                stack.extend(m.values());
            }
            Value::Array(a) => stack.extend(a.iter()),
            _ => {}
        }
    }
    out
}

const A: &str = "waybill-fixture-hpack-a";
const B: &str = "waybill-fixture-hpack-b";

fn both(names: &BTreeSet<String>) -> bool {
    names.contains(A) && names.contains(B)
}

/// FR-004: a `cabal.project` without `packages:` governs nothing.
#[test]
fn a_project_without_packages_governs_nothing() {
    let (a, b) = (manifest(A), manifest(B));
    let names = hackage_names(&scan(&[
        ("cabal.project", "index-state: 2026-01-01T00:00:00Z\n"),
        ("pkgs/a/package.yaml", &a),
        ("pkgs/b/package.yaml", &b),
    ]));
    assert!(both(&names), "{names:?}");
}

/// FR-004: a stack file alone does not filter, as before.
#[test]
fn a_stack_only_project_is_unaffected() {
    let (a, b) = (manifest(A), manifest(B));
    let names = hackage_names(&scan(&[
        ("stack.yaml", "resolver: lts-22.0\npackages:\n  - pkgs/a\n"),
        ("pkgs/a/package.yaml", &a),
        ("pkgs/b/package.yaml", &b),
    ]));
    assert!(both(&names), "{names:?}");
}

/// FR-005: entries that match nothing leave the tree alone.
#[test]
fn entries_matching_nothing_leave_the_tree_alone() {
    let (a, b) = (manifest(A), manifest(B));
    let names = hackage_names(&scan(&[
        ("cabal.project", "packages: ./nowhere\n"),
        ("pkgs/a/package.yaml", &a),
        ("pkgs/b/package.yaml", &b),
    ]));
    assert!(both(&names), "{names:?}");
}

/// With a `stack.yaml` beside the `cabal.project`, a package either one
/// declares belongs (spec 1662, the hs-opentelemetry `otlp-demo` shape), and
/// one neither declares is still dropped.
#[test]
fn a_stack_file_beside_a_cabal_project_adds_its_packages() {
    let (a, b) = (manifest(A), manifest(B));
    let c = manifest("waybill-fixture-hpack-c");
    let names = hackage_names(&scan(&[
        ("stack.yaml", "resolver: lts-22.0\npackages:\n  - pkgs/b\n"),
        ("cabal.project", "packages: ./pkgs/a\n"),
        ("pkgs/a/package.yaml", &a),
        ("pkgs/b/package.yaml", &b),
        ("pkgs/c/package.yaml", &c),
    ]));
    assert!(both(&names), "{names:?}");
    assert!(!names.contains("waybill-fixture-hpack-c"), "declared by neither: {names:?}");
}

/// The same union for a `.cabal`-only tree: #1032's own reader follows it,
/// so the two readers keep agreeing (FR-002).
#[test]
fn the_cabal_reader_takes_the_same_union() {
    let cabal = |n: &str| {
        format!("cabal-version: 2.4\nname: {n}\nversion: 0.1.0\n\nlibrary\n  build-depends: base\n  default-language: Haskell2010\n")
    };
    let names = hackage_names(&scan(&[
        ("stack.yaml", "resolver: lts-22.0\npackages:\n  - pkgs/b\n"),
        ("cabal.project", "packages: ./pkgs/a\n"),
        ("pkgs/a/waybill-fixture-hpack-a.cabal", &cabal(A)),
        ("pkgs/b/waybill-fixture-hpack-b.cabal", &cabal(B)),
        ("pkgs/c/waybill-fixture-hpack-c.cabal", &cabal("waybill-fixture-hpack-c")),
    ]));
    assert!(both(&names), "{names:?}");
    assert!(!names.contains("waybill-fixture-hpack-c"), "declared by neither: {names:?}");
}

/// A `stack.yaml` with no `packages:` means Stack's default, the root only:
/// it adds nothing nested.
#[test]
fn a_stack_file_without_packages_adds_only_the_root() {
    let (a, b) = (manifest(A), manifest(B));
    let names = hackage_names(&scan(&[
        ("stack.yaml", "resolver: lts-22.0\n"),
        ("cabal.project", "packages: ./pkgs/a\n"),
        ("pkgs/a/package.yaml", &a),
        ("pkgs/b/package.yaml", &b),
    ]));
    assert!(names.contains(A) && !names.contains(B), "{names:?}");
}

fn generated(name: &str) -> String {
    format!(
        "cabal-version: 1.12\n\n-- This file has been generated from package.yaml by hpack version 0.38.3.\n--\n-- see: https://github.com/sol/hpack\n\nname:           {name}\nversion:        0.1.0\nbuild-type:     Simple\n\nlibrary\n  build-depends:\n      base\n  default-language: Haskell2010\n"
    )
}

/// FR-003: a `package.yaml` beside its generated `.cabal` gives one component
/// when declared and none when not — the pair always shares a verdict.
#[test]
fn a_generated_pair_is_one_component_or_none() {
    let (a, b) = (manifest(A), manifest(B));
    let (ga, gb) = (generated(A), generated(B));
    let doc = scan(&[
        ("cabal.project", "packages: ./pkgs/a\n"),
        ("pkgs/a/package.yaml", &a),
        ("pkgs/a/waybill-fixture-hpack-a.cabal", &ga),
        ("pkgs/b/package.yaml", &b),
        ("pkgs/b/waybill-fixture-hpack-b.cabal", &gb),
    ]);
    // Every object carrying the purl, wherever it sits: the package may be the
    // document's root (`metadata.component`) or nested, not only top-level.
    let needle = format!("pkg:hackage/{A}@");
    let mut found = 0usize;
    let mut stack = vec![&doc];
    while let Some(v) = stack.pop() {
        match v {
            Value::Object(m) => {
                if m.get("purl").and_then(Value::as_str).is_some_and(|p| p.starts_with(&needle)) {
                    found += 1;
                }
                stack.extend(m.values());
            }
            Value::Array(a) => stack.extend(a.iter()),
            _ => {}
        }
    }
    assert!(!hackage_names(&doc).contains(B), "undeclared pair emitted");
    assert_eq!(found, 1, "declared pair must be exactly one component");
}

/// A dropped manifest leaves no trace: not in C200, though its `!include`
/// is unresolved and would otherwise be reported.
#[test]
fn a_dropped_manifest_is_absent_from_c200() {
    let a = manifest(A);
    let b = format!("{}<<: !include missing.yaml\n", manifest(B));
    let doc = scan(&[
        ("cabal.project", "packages: ./pkgs/a\n"),
        ("pkgs/a/package.yaml", &a),
        ("pkgs/b/package.yaml", &b),
    ]);
    let c = c200(&doc);
    assert!(c.contains("pkgs/a/package.yaml"), "kept manifest missing from C200: {c}");
    assert!(!c.contains("pkgs/b"), "dropped manifest reported in C200: {c}");
}
