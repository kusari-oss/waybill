//! Spec 1665 (#1736): every Haskell lock- or freeze-pinned component names the
//! file or files that pinned it.
//!
//! Positive control (FR-009):
//! - `a/stack.yaml.lock` pins `reached@1.0.0`, which `a/package.yaml` names,
//!   and `unreached@1.0.0`, which nothing names.
//! - `b/stack.yaml.lock` pins `reached@1.0.0` again, and `split@2.0.0`.
//! - `c/stack.yaml.lock` pins `split@3.0.0`.
//! - `d/cabal.project.freeze` pins `frozen@4.0.0`, and `reached@1.0.0` a third
//!   time, so one PURL merges across the stack-lock and freeze files.
//! - `e/cabal.project.freeze` pins `reached@1.0.0` a fourth time. Freeze files
//!   are read before lock files, so `e` arrives before `a`: the merged list is
//!   sorted, not in arrival order.

#![allow(clippy::unwrap_used)]

mod common;

use std::collections::{BTreeMap, HashSet};
use std::process::Command;

use serde_json::Value;

const ROOT: &str = "waybill-fixture-root";

fn lock(pins: &[&str]) -> String {
    let mut s = String::from("snapshots: []\npackages:\n");
    for p in pins {
        s.push_str(&format!("- completed:\n    hackage: {p}@sha256:00,1\n  original:\n    hackage: {p}\n"));
    }
    s
}

fn scan() -> (Value, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join(ROOT);
    let files: [(&str, String); 6] = [
        (
            "a/package.yaml",
            "name: waybill-fixture-app\nversion: 0.1.0\nlibrary:\n  dependencies:\n    - base\n    - waybill-fixture-reached\n".into(),
        ),
        ("a/stack.yaml.lock", lock(&["waybill-fixture-reached-1.0.0", "waybill-fixture-unreached-1.0.0"])),
        ("b/stack.yaml.lock", lock(&["waybill-fixture-reached-1.0.0", "waybill-fixture-split-2.0.0"])),
        ("c/stack.yaml.lock", lock(&["waybill-fixture-split-3.0.0"])),
        (
            "d/cabal.project.freeze",
            "constraints: any.waybill-fixture-frozen ==4.0.0,\n             any.waybill-fixture-reached ==1.0.0\n".into(),
        ),
        ("e/cabal.project.freeze", "constraints: any.waybill-fixture-reached ==1.0.0\n".into()),
    ];
    for (rel, text) in &files {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    let out = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let f = |n: &str| out.path().join(n);
    let o = Command::new(env!("CARGO_BIN_EXE_waybill"))
        .env("HOME", home.path())
        .env("WAYBILL_FIXED_TIMESTAMP", "2026-01-01T00:00:00Z")
        .args(["--offline", "sbom", "scan", "--path"])
        .arg(&root)
        .args(["--no-deep-hash", "--format", "cyclonedx-json,spdx-2.3-json,spdx-3-json"])
        .arg("--output")
        .arg(format!("cyclonedx-json={}", f("waybill.cdx.json").display()))
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", f("waybill.spdx.json").display()))
        .arg("--output")
        .arg(format!("spdx-3-json={}", f("waybill.spdx3.json").display()))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    // FR-008: C18 and every other catalog row agree across the three formats.
    common::parity::assert_parity(out.path());
    (serde_json::from_slice(&std::fs::read(f("waybill.cdx.json")).unwrap()).unwrap(), out)
}

fn components(v: &Value) -> Vec<&Value> {
    let mut out = Vec::new();
    let mut stack = vec![v];
    while let Some(v) = stack.pop() {
        match v {
            Value::Object(m) => {
                if m.contains_key("purl") {
                    out.push(v);
                }
                stack.extend(m.values());
            }
            Value::Array(a) => stack.extend(a.iter()),
            _ => {}
        }
    }
    out
}

fn prop<'a>(c: &'a Value, name: &str) -> Option<&'a str> {
    c["properties"].as_array()?.iter().find(|p| p["name"] == name)?["value"].as_str()
}

/// purl -> (source-files, bom-ref) for every lock- or freeze-pinned component.
fn pinned(doc: &Value) -> BTreeMap<String, (Vec<String>, String)> {
    components(doc)
        .into_iter()
        .filter(|c| matches!(prop(c, "waybill:source-type"), Some("hackage-stack-lock" | "hackage-freeze")))
        .map(|c| {
            let files: Vec<String> = serde_json::from_str(prop(c, "waybill:source-files").unwrap_or("[]")).unwrap();
            (c["purl"].as_str().unwrap().to_string(), (files, c["bom-ref"].as_str().unwrap().to_string()))
        })
        .collect()
}

#[test]
fn every_pin_names_exactly_the_files_that_pinned_it() {
    let (doc, _out) = scan();
    let got = pinned(&doc);
    let files: BTreeMap<&str, Vec<&str>> = got.iter().map(|(p, (f, _))| (p.as_str(), f.iter().map(String::as_str).collect())).collect();
    let want: BTreeMap<&str, Vec<&str>> = BTreeMap::from([
        ("pkg:hackage/waybill-fixture-reached@1.0.0", vec!["a/stack.yaml.lock", "b/stack.yaml.lock", "d/cabal.project.freeze", "e/cabal.project.freeze"]),
        ("pkg:hackage/waybill-fixture-unreached@1.0.0", vec!["a/stack.yaml.lock"]),
        ("pkg:hackage/waybill-fixture-split@2.0.0", vec!["b/stack.yaml.lock"]),
        ("pkg:hackage/waybill-fixture-split@3.0.0", vec!["c/stack.yaml.lock"]),
        ("pkg:hackage/waybill-fixture-frozen@4.0.0", vec!["d/cabal.project.freeze"]),
    ]);
    assert_eq!(files, want);
}

#[test]
fn an_unreached_pin_is_still_an_edgeless_lock_pin() {
    let (doc, _out) = scan();
    let got = pinned(&doc);
    let reached: HashSet<&str> = doc["dependencies"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|d| d["dependsOn"].as_array().unwrap().iter().filter_map(Value::as_str))
        .collect();
    let (_, r) = &got["pkg:hackage/waybill-fixture-reached@1.0.0"];
    assert!(reached.contains(r.as_str()), "reached@1.0.0 is reached");
    let (_, u) = &got["pkg:hackage/waybill-fixture-unreached@1.0.0"];
    assert!(!reached.contains(u.as_str()));
    let c = components(&doc).into_iter().find(|c| c["bom-ref"] == u.as_str()).unwrap();
    assert_eq!(prop(c, "waybill:orphan-reason"), Some("edgeless-lock-pin"));
}
