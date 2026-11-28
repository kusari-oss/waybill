//! Spec 1666 (#1741): an unreachable `cabal.project.freeze` pin is explained.
//!
//! Positive control (FR-007, FR-009):
//! - `unused`: `cabal.project` lists `app`, whose library depends only on
//!   `base`; `cabal.project.freeze` pins `waybill-fixture-frozen ==4.0.0`. The
//!   pin is unreachable and carries `edgeless-lock-pin`, and I7's rule (every
//!   unreachable component carries a reason) holds.
//! - `used`: the package sits at the root (`packages: .`) beside the freeze
//!   file and depends on `waybill-fixture-frozen`, so the pin is reachable and
//!   carries no reason. A root freeze file does not yet govern a member in a
//!   subdirectory: the declaration and the pin stay two components there, on
//!   `main` as on this branch (filed separately).

#![allow(clippy::unwrap_used)]

mod common;

use std::collections::{HashMap, HashSet};
use std::process::Command;

use serde_json::Value;

const FROZEN: &str = "pkg:hackage/waybill-fixture-frozen@4.0.0";

fn scan(member: &str, app_deps: &str) -> (Value, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("waybill-fixture-root");
    let (listed, cabal) = if member.is_empty() {
        (".".to_string(), "waybill-fixture-app.cabal".to_string())
    } else {
        (member.to_string(), format!("{member}/waybill-fixture-app.cabal"))
    };
    let files = [
        ("cabal.project".to_string(), format!("packages: {listed}\n")),
        (
            cabal,
            format!("cabal-version: 2.4\nname: waybill-fixture-app\nversion: 0.1.0.0\nlibrary\n  build-depends: {app_deps}\n"),
        ),
        ("cabal.project.freeze".to_string(), "constraints: any.waybill-fixture-frozen ==4.0.0\n".to_string()),
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
    // C45 is a catalog row: parity carries the CycloneDX result to both SPDX
    // formats.
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

/// bom-refs reachable from the document's root component.
fn reachable(doc: &Value) -> HashSet<String> {
    let adj: HashMap<&str, Vec<&str>> = doc["dependencies"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| {
            let to = d["dependsOn"].as_array().map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
            (d["ref"].as_str().unwrap(), to)
        })
        .collect();
    let mut seen = HashSet::new();
    let mut stack = vec![doc["metadata"]["component"]["bom-ref"].as_str().unwrap()];
    while let Some(r) = stack.pop() {
        if seen.insert(r.to_string()) {
            stack.extend(adj.get(r).into_iter().flatten().copied());
        }
    }
    seen
}

fn frozen(doc: &Value) -> &Value {
    components(doc).into_iter().find(|c| c["purl"] == FROZEN).expect("the freeze pin is emitted")
}

#[test]
fn an_unused_freeze_pin_is_an_edgeless_lock_pin() {
    let (doc, out) = scan("app", "base");
    let pin = frozen(&doc);
    assert!(!reachable(&doc).contains(pin["bom-ref"].as_str().unwrap()), "the pin is unreachable");
    assert_eq!(prop(pin, "waybill:orphan-reason"), Some("edgeless-lock-pin"));
    for f in ["waybill.spdx.json", "waybill.spdx3.json"] {
        let text = std::fs::read_to_string(out.path().join(f)).unwrap();
        assert!(text.contains("edgeless-lock-pin"), "{f} carries the reason");
    }
}

/// FR-007: I7's rule over the freeze fixture.
#[test]
fn i7_every_unreachable_component_carries_a_reason() {
    let (doc, _out) = scan("app", "base");
    let seen = reachable(&doc);
    let unexplained: Vec<&str> = components(&doc)
        .into_iter()
        .filter(|c| !seen.contains(c["bom-ref"].as_str().unwrap_or_default()))
        .filter(|c| prop(c, "waybill:orphan-reason").is_none())
        .filter_map(|c| c["purl"].as_str())
        .collect();
    assert!(unexplained.is_empty(), "unexplained orphans: {unexplained:?}");
}

#[test]
fn a_used_freeze_pin_is_reachable_and_unexplained() {
    let (doc, _out) = scan("", "base, waybill-fixture-frozen");
    let pin = frozen(&doc);
    assert!(reachable(&doc).contains(pin["bom-ref"].as_str().unwrap()), "the pin is reached");
    assert_eq!(prop(pin, "waybill:orphan-reason"), None);
}
