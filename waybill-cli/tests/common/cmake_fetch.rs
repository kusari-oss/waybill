//! Shared helpers for the CMake fetch suites: spec 1538's
//! `cmake_nested_fetches.rs` and spec 1539's `cmake_fetch_wrappers.rs`.
//! A scan in all three formats, component lookups, and the invariants
//! every scan is checked against (spec 1538 I-2, I-3; spec 1539 I-5, I-6).

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

use serde_json::Value;

use super::git_fixture::GitFixture;

pub struct Docs {
    pub cdx: Value,
    pub spdx23: Value,
    pub spdx3: Value,
}

pub fn scan(fx: &GitFixture, path: &Path, extra: &[&str]) -> Docs {
    let out_dir = tempfile::tempdir().unwrap();
    let (c, s2, s3) = (
        out_dir.path().join("o.cdx.json"),
        out_dir.path().join("o.spdx.json"),
        out_dir.path().join("o.spdx3.json"),
    );
    let out = Command::new(super::bin())
        .env("HOME", fx.home())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", fx.home().join(".gitconfig"))
        .env("WAYBILL_FIXED_TIMESTAMP", "2026-01-01T00:00:00Z")
        .args(["--offline", "sbom", "scan", "--path"])
        .arg(path)
        .args(["--no-deep-hash", "--root-name", "waybill-fixture-root", "--root-version", "1"])
        .args(["--format", "cyclonedx-json,spdx-2.3-json,spdx-3-json"])
        .arg("--output")
        .arg(format!("cyclonedx-json={}", c.display()))
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", s2.display()))
        .arg("--output")
        .arg(format!("spdx-3-json={}", s3.display()))
        .args(extra)
        .output()
        .unwrap();
    assert!(out.status.success(), "scan failed: {}", String::from_utf8_lossy(&out.stderr));
    let read = |p: &Path| serde_json::from_slice::<Value>(&std::fs::read(p).unwrap()).unwrap();
    let docs = Docs { cdx: read(&c), spdx23: read(&s2), spdx3: read(&s3) };
    assert_invariants(&docs);
    docs
}

pub fn cdx_components(cdx: &Value) -> Vec<&Value> {
    let mut out = Vec::new();
    let mut stack: Vec<&Value> = cdx["components"].as_array().into_iter().flatten().collect();
    while let Some(c) = stack.pop() {
        out.push(c);
        stack.extend(c["components"].as_array().into_iter().flatten());
    }
    out
}

pub fn prop<'a>(c: &'a Value, name: &str) -> Option<&'a str> {
    c["properties"].as_array()?.iter().find(|p| p["name"] == name).and_then(|p| p["value"].as_str())
}

pub fn is_fetch(c: &Value) -> bool {
    prop(c, "waybill:source-mechanism")
        .is_some_and(|m| m.starts_with("cmake-fetchcontent") || m == "cmake-externalproject")
}

pub fn fetches(docs: &Docs) -> Vec<&Value> {
    cdx_components(&docs.cdx).into_iter().filter(|c| is_fetch(c)).collect()
}

pub fn fetch_purls(docs: &Docs) -> BTreeSet<String> {
    fetches(docs).iter().filter_map(|c| c["purl"].as_str()).map(str::to_string).collect()
}

/// The one fetch component whose PURL starts with `prefix`.
pub fn the<'a>(docs: &'a Docs, prefix: &str) -> &'a Value {
    let hits: Vec<&Value> = fetches(docs)
        .into_iter()
        .filter(|c| c["purl"].as_str().is_some_and(|p| p.starts_with(prefix)))
        .collect();
    assert_eq!(hits.len(), 1, "exactly one fetch component {prefix}: {:?}", fetch_purls(docs));
    hits[0]
}

pub fn coverage(docs: &Docs) -> Option<Value> {
    prop(&docs.cdx["metadata"], "waybill:cmake-fetch-coverage").map(|v| serde_json::from_str(v).unwrap())
}

pub fn n(v: &Value, path: &[&str]) -> u64 {
    let mut x = v;
    for k in path {
        x = &x[*k];
    }
    x.as_u64().unwrap_or_else(|| panic!("missing {path:?} in {v}"))
}

pub fn conditions(c: &Value) -> Option<Vec<String>> {
    prop(c, "waybill:cmake-fetch-conditions").map(|v| serde_json::from_str(v).unwrap())
}

/// I-2 (C231 V1/V2) and I-3 (C122 and C232 go together), on every scan.
pub fn assert_invariants(docs: &Docs) {
    if let Some(c) = coverage(docs) {
        let read = n(&c, &["read"]);
        let accounted = n(&c, &["emitted"])
            + n(&c, &["merged", "first-declaration-wins"])
            + n(&c, &["merged", "existing-component"])
            + n(&c, &["unresolved", "identity"]);
        assert_eq!(read, accounted, "I-2 V1: every declaration read is accounted for: {c}");
        assert!(n(&c, &["conditional"]) + n(&c, &["test"]) <= n(&c, &["components"]), "I-2 V2: {c}");
        for doc in [&docs.spdx23, &docs.spdx3] {
            assert!(doc.to_string().contains("waybill:cmake-fetch-coverage"), "C231 in every format");
        }
        // Spec 1539 I-6.
        if let Some(w) = c.get("wrappers") {
            assert!(n(w, &["ambiguous-calls"]) <= n(&c, &["unresolved", "identity"]), "I-6: {c}");
            assert!(n(w, &["calls"]) <= read, "I-6: {c}");
        }
    }
    // Spec 1539 I-5: C234 is a non-empty sorted array of non-empty chains,
    // in every format the component's rows are.
    let mut chains = 0;
    for f in fetches(docs) {
        if let Some(v) = wrapper_chains(f) {
            assert!(!v.is_empty() && v.iter().all(|c| !c.is_empty()), "I-5: {f}");
            let mut sorted = v.clone();
            sorted.sort();
            sorted.dedup();
            assert_eq!(v, sorted, "I-5: sorted and unique: {f}");
            chains += 1;
        }
    }
    for doc in [&docs.spdx23, &docs.spdx3] {
        let text = doc.to_string();
        assert_eq!(chains > 0, text.contains("waybill:cmake-fetch-wrapper"), "I-5: C234 in every format");
    }
    for f in fetches(docs) {
        let derivation = prop(f, "waybill:optional-derivation").filter(|d| d.starts_with("cmake-"));
        let optional = f["scope"] == "excluded" && derivation.is_some();
        // A shape that comes from an enclosing block has condition text;
        // FIND_PACKAGE_ARGS, ALWAYS and a provider do not, and C122 names them.
        let from_block = ["cmake-not-target", "cmake-find-package-fallback", "cmake-conditional", "cmake-function-body"];
        if derivation.is_some_and(|d| from_block.contains(&d)) {
            assert!(conditions(f).is_some(), "I-3: a block-derived conditional fetch states its conditions: {f}");
        }
        if f["scope"] == "excluded" && prop(f, "waybill:lifecycle-scope") == Some("optional") {
            assert!(derivation.is_some(), "I-3: a conditional fetch names its shape (C122): {f}");
        }
        if conditions(f).is_some() {
            let test = prop(f, "waybill:lifecycle-scope") == Some("test");
            // Spec 1641 (FR-005): a catch-all conditional whose conditions
            // hold by default is in scope and keeps C232, with no C122.
            let kept = f["scope"] != "excluded" && prop(f, "waybill:optional-derivation").is_none();
            assert!(optional || test || kept, "I-3: conditions only on a conditional, test or kept fetch: {f}");
        }
    }
}

/// C234: the wrapper chains a component was fetched through.
pub fn wrapper_chains(c: &Value) -> Option<Vec<Vec<String>>> {
    prop(c, "waybill:cmake-fetch-wrapper").map(|v| serde_json::from_str(v).unwrap())
}

pub fn required(c: &Value) -> bool {
    c["scope"].is_null() && prop(c, "waybill:optional-derivation").is_none() && conditions(c).is_none()
}

pub fn assert_optional(c: &Value, derivation: &str) {
    assert_eq!(c["scope"], "excluded", "{c}");
    assert_eq!(prop(c, "waybill:optional-derivation"), Some(derivation), "{c}");
}

pub fn url_fetch(name: &str, version: &str, extra: &str) -> String {
    format!(
        "FetchContent_Declare({name} URL https://example.invalid/{name}-{version}.tar.gz URL_HASH SHA256={} {extra})\n",
        "5".repeat(64)
    )
}

pub const ROOT: &str = "cmake_minimum_required(VERSION 3.25)\nproject(waybill_fixture_superbuild)\ninclude(FetchContent)\n";

pub fn tree(files: &[(&str, &str)]) -> GitFixture {
    let fx = GitFixture::new();
    for (rel, text) in files {
        fx.write(&format!("t/{rel}"), text);
    }
    fx
}

pub fn git_fetch(name: &str, tag: &str) -> String {
    format!("FetchContent_Declare({name} GIT_REPOSITORY /waybill-fixture/rr.git GIT_TAG {tag})\n")
}

pub fn guarded(body: &str) -> String {
    format!("if(NOT TARGET rr)\n{body}endif()\n")
}
