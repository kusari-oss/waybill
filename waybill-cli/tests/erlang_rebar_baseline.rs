//! Milestone 141 US1 — rebar3 OTP app baseline integration tests.
//!
//! Covers SC-001 (3 direct + 2 transitive = 5 hex components emit
//! from rebar.lock).

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

fn run_scan(project_root: &Path) -> Value {
    let out_dir = tempfile::tempdir().unwrap();
    let out_path = out_dir.path().join("out.cdx.json");
    let result = Command::new(binary_path())
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(project_root)
        .arg("--no-deep-hash")
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", out_path.display()))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&result.stderr),
    );
    let bytes = std::fs::read(&out_path).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn erlang_components(doc: &Value) -> Vec<&Value> {
    let mut out: Vec<&Value> = Vec::new();
    let predicate = |c: &Value| -> bool {
        c.get("properties")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter().any(|p| {
                    p.get("name").and_then(|v| v.as_str()) == Some("waybill:source-type")
                        && p.get("value")
                            .and_then(|v| v.as_str())
                            .map(|s| s.starts_with("erlang-"))
                            .unwrap_or(false)
                })
            })
            .unwrap_or(false)
    };
    if let Some(arr) = doc.get("components").and_then(|v| v.as_array()) {
        for c in arr {
            if predicate(c) {
                out.push(c);
            }
        }
    }
    if let Some(c) = doc.get("metadata").and_then(|m| m.get("component")) {
        if predicate(c) {
            out.push(c);
        }
    }
    out
}

fn hex_purls(doc: &Value) -> Vec<String> {
    erlang_components(doc)
        .into_iter()
        .filter_map(|c| c.get("purl").and_then(|v| v.as_str()).map(String::from))
        .filter(|p| p.starts_with("pkg:hex/"))
        .collect()
}

fn component_with_name<'a>(doc: &'a Value, name: &str) -> Option<&'a Value> {
    if let Some(c) = doc.get("metadata").and_then(|m| m.get("component")) {
        if c.get("name").and_then(|v| v.as_str()) == Some(name) {
            return Some(c);
        }
    }
    doc.get("components")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.iter().find(|c| c.get("name").and_then(|v| v.as_str()) == Some(name)))
}

fn write_rebar3_fixture(root: &Path) {
    // 64-hex-char SHA-256 strings (deterministic constants for test
    // reproducibility per F5 finding remediation).
    let sha_cowboy = "a".repeat(64);
    let sha_jiffy = "b".repeat(64);
    let sha_lager = "c".repeat(64);
    let sha_ranch = "d".repeat(64);
    let sha_cowlib = "e".repeat(64);
    std::fs::write(
        root.join("rebar.config"),
        r#"{erl_opts, [debug_info]}.
{deps, [
    {cowboy, "2.10.0"},
    {jiffy, "1.1.1"},
    {lager, "3.9.2"}
]}.
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("rebar.lock"),
        format!(
            r#"{{"1.2.0",
[{{<<"cowboy">>,{{pkg,<<"cowboy">>,<<"2.10.0">>,<<"{sha_cowboy}">>}},0}},
 {{<<"cowlib">>,{{pkg,<<"cowlib">>,<<"2.12.1">>,<<"{sha_cowlib}">>}},1}},
 {{<<"jiffy">>,{{pkg,<<"jiffy">>,<<"1.1.1">>,<<"{sha_jiffy}">>}},0}},
 {{<<"lager">>,{{pkg,<<"lager">>,<<"3.9.2">>,<<"{sha_lager}">>}},0}},
 {{<<"ranch">>,{{pkg,<<"ranch">>,<<"1.8.0">>,<<"{sha_ranch}">>}},1}}]}}.
"#
        ),
    )
    .unwrap();
}

#[test]
fn sc001_baseline_five_hex_components() {
    let dir = tempfile::tempdir().unwrap();
    write_rebar3_fixture(dir.path());
    let doc = run_scan(dir.path());
    let mut purls = hex_purls(&doc);
    purls.sort();
    // Exactly 5 pinned hex deps from rebar.lock (no main-module emitted
    // because no *.app.src present in this US1 baseline fixture).
    assert_eq!(
        purls,
        vec![
            "pkg:hex/cowboy@2.10.0".to_string(),
            "pkg:hex/cowlib@2.12.1".to_string(),
            "pkg:hex/jiffy@1.1.1".to_string(),
            "pkg:hex/lager@3.9.2".to_string(),
            "pkg:hex/ranch@1.8.0".to_string(),
        ],
        "expected exactly 5 pkg:hex/ components from the rebar.lock fixture; got {purls:?}",
    );
}

#[test]
fn sc001_inner_sha256_hash_emitted() {
    let dir = tempfile::tempdir().unwrap();
    write_rebar3_fixture(dir.path());
    let doc = run_scan(dir.path());
    let cowboy = component_with_name(&doc, "cowboy").expect("cowboy component");
    let hashes = cowboy
        .get("hashes")
        .and_then(|v| v.as_array())
        .expect("cowboy must carry hashes array");
    assert_eq!(hashes.len(), 1, "exactly one SHA-256 entry per FR-011");
    let alg = hashes[0]
        .get("alg")
        .and_then(|v| v.as_str())
        .expect("hash alg");
    let content = hashes[0]
        .get("content")
        .and_then(|v| v.as_str())
        .expect("hash content");
    assert_eq!(alg, "SHA-256");
    assert_eq!(content, "a".repeat(64));
}

#[test]
fn sc001_source_type_annotation() {
    let dir = tempfile::tempdir().unwrap();
    write_rebar3_fixture(dir.path());
    let doc = run_scan(dir.path());
    let cowboy = component_with_name(&doc, "cowboy").expect("cowboy component");
    let props = cowboy
        .get("properties")
        .and_then(|v| v.as_array())
        .expect("properties array");
    let st = props
        .iter()
        .find(|p| p.get("name").and_then(|v| v.as_str()) == Some("waybill:source-type"))
        .and_then(|p| p.get("value").and_then(|v| v.as_str()))
        .expect("waybill:source-type property");
    assert_eq!(st, "erlang-hex");
}
