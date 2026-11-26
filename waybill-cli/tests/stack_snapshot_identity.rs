//! Spec 1663 (#1678): every Stack snapshot is named, says which lock file
//! named it, and is explained.
//!
//! Positive control (FR-007). Four Stack packages, each with its own lock:
//! (a) `lts-18.8` pinned by stackage URL, (b) a nightly pinned by URL, which
//! exercises the padding, (c) a non-stackage URL, and (d) `lts-18.8` pinned by
//! name, the string `original` that used to reject the whole file (FR-000),
//! with the same sha256 as (a).

#![allow(clippy::unwrap_used)]

use std::process::Command;

use serde_json::Value;

const ROOT: &str = "waybill-fixture-root";
const SS: &str = "https://raw.githubusercontent.com/commercialhaskell/stackage-snapshots/master";
const LTS_SHA: &str = "93a107557e8691ed5ca17beaee41e68222b142c48868fc8c04a4181fb233477d";

fn lock(original: &str, url: &str, sha: &str, pkg: &str) -> String {
    format!(
        "snapshots:\n- completed:\n    sha256: {sha}\n    size: 1\n    url: {url}\n  original:{original}\n\
         packages:\n- completed:\n    hackage: {pkg}@sha256:00,1\n  original:\n    hackage: {pkg}\n"
    )
}

fn manifest(name: &str) -> String {
    format!("name: {name}\nversion: 0.1.0\nlibrary:\n  dependencies:\n    - base\n")
}

fn scan() -> Value {
    let lts = format!("{SS}/lts/18/8.yaml");
    let nightly = format!("{SS}/nightly/2024/5/7.yaml");
    let other = "https://example.invalid/waybill-fixture-snap.yaml";
    let files: Vec<(String, String)> = vec![
        ("a/package.yaml".into(), manifest("waybill-fixture-snap-a")),
        ("a/stack.yaml".into(), format!("resolver:\n  url: {lts}\n")),
        ("a/stack.yaml.lock".into(), lock(&format!("\n    url: {lts}"), &lts, LTS_SHA, "waybill-fixture-dep-a-1.0.0")),
        ("b/package.yaml".into(), manifest("waybill-fixture-snap-b")),
        ("b/stack.yaml".into(), format!("resolver:\n  url: {nightly}\n")),
        ("b/stack.yaml.lock".into(), lock(&format!("\n    url: {nightly}"), &nightly, "bb", "waybill-fixture-dep-b-1.0.0")),
        ("c/package.yaml".into(), manifest("waybill-fixture-snap-c")),
        ("c/stack.yaml".into(), format!("resolver:\n  url: {other}\n")),
        ("c/stack.yaml.lock".into(), lock(&format!("\n    url: {other}"), other, "cc", "waybill-fixture-dep-c-1.0.0")),
        ("d/package.yaml".into(), manifest("waybill-fixture-snap-d")),
        ("d/stack.yaml".into(), "resolver: lts-18.8\n".into()),
        ("d/stack.yaml.lock".into(), lock(" lts-18.8", &lts, LTS_SHA, "waybill-fixture-dep-d-2.3.4")),
    ];
    let dir = tempfile::tempdir().unwrap();
    for (rel, text) in &files {
        let p = dir.path().join(ROOT).join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    let out = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let cdx = out.path().join("w.cdx.json");
    let o = Command::new(env!("CARGO_BIN_EXE_waybill"))
        .env("HOME", home.path())
        .env("NO_COLOR", "1")
        .args(["--offline", "sbom", "scan", "--path"])
        .arg(dir.path().join(ROOT))
        .args(["--no-deep-hash", "--format", "cyclonedx-json", "--output"])
        .arg(format!("cyclonedx-json={}", cdx.display()))
        .output()
        .unwrap();
    assert!(o.status.success(), "scan failed: {}", String::from_utf8_lossy(&o.stderr));
    let stderr = String::from_utf8_lossy(&o.stderr);
    assert!(!stderr.contains("failed to parse stack.yaml.lock"), "a lock file was rejected: {stderr}");
    serde_json::from_slice(&std::fs::read(cdx).unwrap()).unwrap()
}

/// Every component object, nested or not.
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

#[test]
fn every_snapshot_is_named_sourced_and_explained() {
    let doc = scan();
    let all = components(&doc);
    let snaps: Vec<&&Value> = all.iter().filter(|c| prop(c, "waybill:source-type") == Some("hackage-snapshot")).collect();
    let purls: Vec<&str> = snaps.iter().filter_map(|c| c["purl"].as_str()).collect();

    assert!(!all.iter().any(|c| c["purl"].as_str().is_some_and(|p| p.contains("/unknown@"))), "an `unknown` component: {purls:?}");

    // (a) and (d): one component, both lock files.
    let lts: Vec<&&&Value> = snaps.iter().filter(|c| c["purl"] == format!("pkg:generic/stackage-lts-18.8@{LTS_SHA}")).collect();
    assert_eq!(lts.len(), 1, "lts-18.8 by URL and by name must be one component: {purls:?}");
    let files = prop(lts[0], "waybill:source-files").unwrap_or_default();
    assert!(files.contains("a/stack.yaml.lock") && files.contains("d/stack.yaml.lock"), "both locks: {files}");

    // (b): the nightly URL path, padded.
    assert!(purls.iter().any(|p| p.starts_with("pkg:generic/stackage-nightly-2024-05-07@bb")), "{purls:?}");
    // (c): a non-stackage URL keeps its URL.
    assert!(purls.iter().any(|p| p.starts_with("pkg:generic/stack-snapshot@cc?download_url=") && p.contains("example.invalid")), "{purls:?}");
    assert_eq!(snaps.len(), 3, "{purls:?}");

    // FR-005: reachable from the root, or explained. Here the synthetic root
    // depends on each snapshot; in hs-opentelemetry they are unreachable and
    // carry `build-toolchain` (measurements/snapshots.md, and the
    // `orphan_reason` unit test).
    let reached: std::collections::HashSet<&str> = doc["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|d| d["dependsOn"].as_array().into_iter().flatten().filter_map(Value::as_str))
        .collect();
    for c in &snaps {
        let r = c["bom-ref"].as_str().unwrap_or_default();
        assert!(
            reached.contains(r) || prop(c, "waybill:orphan-reason") == Some("build-toolchain"),
            "{} is neither reachable nor explained",
            c["purl"]
        );
    }

    // FR-000: (d)'s lock, the string form, is read: its pinned package is emitted.
    assert!(all.iter().any(|c| c["purl"] == "pkg:hackage/waybill-fixture-dep-d@2.3.4"), "lock package from (d) missing");
}
