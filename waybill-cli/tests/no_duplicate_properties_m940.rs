//! Issue #940 — a property emitted from a typed field must not be emitted
//! again from the annotation bag.
//!
//! Seven readers populate both slots for the same fact (cocoapods, composer,
//! dart, elixir, erlang, haskell, scala) and the emitters rendered each, so
//! `waybill:source-type` appeared twice on every component they produced — 61
//! across the public corpus, 21 on one reference repository.
//!
//! SPDX 3 never showed it, because m166 added a dedup by `spdxId` forced by a
//! SHACL cardinality constraint. CycloneDX and SPDX 2.3 have no such
//! constraint, so nothing surfaced the same root cause there.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

fn binary_path() -> &'static str { env!("CARGO_BIN_EXE_waybill") }

/// A cabal project: the haskell reader is one of the seven that writes both
/// slots, so this reproduces the defect without needing a network fetch.
fn tree() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(
        d.path().join("app.cabal"),
        "name: waybill-fixture-app\nversion: 1.0\nlicense: MIT\n\n\
         library\n  build-depends: waybill-fixture-alpha, waybill-fixture-beta\n",
    ).unwrap();
    d
}

fn scan(root: &Path, fmt: &str, ext: &str) -> serde_json::Value {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let seq = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let out = std::env::temp_dir().join(format!("m940-{}-{seq}.{ext}", std::process::id()));
    let st = Command::new(binary_path())
        .args(["sbom", "scan", "--path", root.to_str().unwrap(), "--offline",
               "--format", fmt, "--output", out.to_str().unwrap()])
        .status().unwrap();
    assert!(st.success(), "scan failed: {st:?}");
    serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap()
}

/// CycloneDX: no component carries the same property name twice.
#[test]
fn cyclonedx_emits_each_property_name_at_most_once_per_component_m940() {
    let d = tree();
    let v = scan(d.path(), "cyclonedx-json", "json");
    let mut offenders = Vec::new();
    for c in v["components"].as_array().unwrap() {
        let mut seen: HashMap<&str, usize> = HashMap::new();
        for p in c["properties"].as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
            *seen.entry(p["name"].as_str().unwrap_or("")).or_insert(0) += 1;
        }
        for (name, n) in seen {
            if n > 1 {
                offenders.push(format!("{}: {name} x{n}", c["name"].as_str().unwrap_or("?")));
            }
        }
    }
    assert!(offenders.is_empty(), "duplicate properties: {offenders:?}");
}

/// SPDX 2.3: same, over the annotation envelopes.
#[test]
fn spdx23_emits_each_annotation_field_at_most_once_per_package_m940() {
    let d = tree();
    let v = scan(d.path(), "spdx-2.3-json", "spdx.json");
    let mut offenders = Vec::new();
    for p in v["packages"].as_array().unwrap() {
        let mut seen: HashMap<String, usize> = HashMap::new();
        for a in p["annotations"].as_array().map(|x| x.as_slice()).unwrap_or(&[]) {
            if let Some(field) = a["comment"].as_str()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
                .and_then(|j| j.get("field").and_then(|f| f.as_str()).map(str::to_string))
            {
                *seen.entry(field).or_insert(0) += 1;
            }
        }
        for (field, n) in seen {
            if n > 1 {
                offenders.push(format!("{}: {field} x{n}", p["name"].as_str().unwrap_or("?")));
            }
        }
    }
    assert!(offenders.is_empty(), "duplicate annotations: {offenders:?}");
}

/// The dedup must not DROP the value — only the second copy.
///
/// This is the assertion that distinguishes a fix from a regression: skipping
/// the bag entry unconditionally would also pass the tests above, while losing
/// any key the bag is the only source for.
#[test]
fn deduplication_keeps_the_value_it_does_not_remove_it_m940() {
    let d = tree();
    let v = scan(d.path(), "cyclonedx-json", "json");
    let with_source_type = v["components"].as_array().unwrap().iter()
        .filter(|c| c["properties"].as_array().map(|a| {
            a.iter().any(|p| p["name"].as_str() == Some("waybill:source-type"))
        }).unwrap_or(false))
        .count();
    assert!(
        with_source_type > 0,
        "every haskell component should still carry `waybill:source-type` — once. \
         Zero would mean the guard dropped the property rather than the duplicate"
    );
}
