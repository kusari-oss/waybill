//! Milestone 1071 US1 — `waybill sbom edit` filtering, against the
//! `tests/fixtures/sbom_edit/` scan outputs and one public-corpus golden.

mod common;
mod sbom_edit_support;

use serde_json::{json, Value};

use sbom_edit_support::*;

/// Research R2: waybill writes CycloneDX and SPDX 3 with sorted keys and
/// 2-space indentation, so parsing and re-serialising reproduces them byte
/// for byte. SPDX 2.3 is written in struct field order, so an edit
/// re-serialises it with sorted keys: the same JSON, keys reordered.
#[test]
fn documents_round_trip() {
    for (label, file) in FORMATS {
        let bytes = std::fs::read(fixture(file)).unwrap();
        let v: Value = serde_json::from_slice(&bytes).unwrap();
        let mut again = serde_json::to_string_pretty(&v).unwrap().into_bytes();
        if bytes.ends_with(b"\n") {
            again.push(b'\n');
        }
        if label == "spdx23" {
            assert_eq!(serde_json::from_slice::<Value>(&again).unwrap(), v);
        } else {
            assert!(again == bytes, "{label}: round-trip is not byte-identical");
        }
    }
}

#[test]
fn dropping_development_scope_removes_every_trace_and_conforms() {
    for (label, file) in FORMATS {
        let input = read_json(&fixture(file));
        assert!(names(&input).contains("jest-lite"), "{label}: fixture lacks the dev chain");
        let e = edit(&fixture(file), &["--drop", "scope=development,test"]);
        e.assert_ok();
        let text = e.text();
        for gone in ["jest-lite", "pretty-format-lite"] {
            assert!(!text.contains(gone), "{label}: `{gone}` is still in the output");
        }
        let out = e.json();
        assert!(names(&out).contains("express"), "{label}: a runtime dependency was dropped");
        assert_conforms_like(&input, &out, label);
        if label == "spdx3" {
            spdx3_validate_or_skip(&e.path);
        }
        // The root's dependency list changed: no longer claimed complete.
        let completeness = waybill_fields(&out).get("waybill:graph-completeness").cloned().unwrap_or_default();
        assert!(completeness.iter().all(|v| v == "unknown"), "{label}: graph completeness still {completeness:?}");
        assert!(!waybill_fields(&out).contains_key("waybill:graph-completeness-reason"), "{label}");
    }
}

/// SC-007: dropping a component in the middle of the graph connects its
/// dependents to its dependencies, so nothing reachable before (and not
/// dropped) becomes unreachable.
#[test]
fn dropping_a_middle_component_bridges_its_edges() {
    for (label, file) in FORMATS {
        let input = read_json(&fixture(file));
        assert!(depends(&input, ROOT, "@acme/internal-utils") && !depends(&input, ROOT, "lodash"), "{label}: fixture shape");
        let before = reachable(&input, ROOT);
        let out = edit(&fixture(file), &["--drop", "name=@acme/internal-utils"]).assert_ok().json();
        assert!(depends(&out, ROOT, "lodash"), "{label}: the root does not depend on lodash after bridging");
        let after = reachable(&out, ROOT);
        let lost: Vec<_> = before.iter().filter(|n| *n != "@acme/internal-utils" && !after.contains(*n)).collect();
        assert!(lost.is_empty(), "{label}: became unreachable: {lost:?}");
    }
}

#[test]
fn cdx_changed_dependency_lists_are_marked_incomplete() {
    let out = edit(&fixture("full.cdx.json"), &["--drop", "name=@acme/internal-utils"]).assert_ok().json();
    let root = out.pointer("/metadata/component/bom-ref").and_then(Value::as_str).unwrap().to_string();
    let compositions = out.get("compositions").and_then(Value::as_array).cloned().unwrap_or_default();
    let incomplete = compositions.iter().any(|c| {
        c.get("aggregate") == Some(&json!("incomplete"))
            && c.get("dependencies").and_then(Value::as_array).is_some_and(|d| d.contains(&json!(root)))
    });
    assert!(incomplete, "no incomplete composition names the root: {compositions:#?}");
    let complete_names_root = compositions.iter().any(|c| {
        c.get("aggregate") == Some(&json!("complete"))
            && c.get("dependencies").and_then(Value::as_array).is_some_and(|d| d.contains(&json!(root)))
    });
    assert!(!complete_names_root, "the root is still in a complete composition");
}

/// The protected set (C21 generation context, C194 derivation) survives
/// `--drop-annotations waybill:`, with C21's value unchanged.
#[test]
fn dropping_all_waybill_annotations_keeps_the_protected_set() {
    for (label, file) in FORMATS {
        let input = read_json(&fixture(file));
        let c21 = waybill_fields(&input).get("waybill:generation-context").cloned();
        assert!(c21.is_some(), "{label}: fixture lacks C21");
        let out = edit(&fixture(file), &["--drop-annotations", "waybill:"]).assert_ok().json();
        let fields = waybill_fields(&out);
        let left: Vec<&String> = fields
            .keys()
            .filter(|k| k.starts_with("waybill:"))
            .filter(|k| *k != "waybill:generation-context" && *k != "waybill:derivation")
            .collect();
        assert!(left.is_empty(), "{label}: annotations survived: {left:?}");
        assert_eq!(fields.get("waybill:generation-context").cloned(), c21, "{label}: C21 changed");
        assert_conforms_like(&input, &out, label);
    }
}

/// SC-002: apart from the targeted content, its consequences and the
/// derivation record, input and output are identical.
#[test]
fn cdx_edit_changes_nothing_else() {
    fn strip_waybill(v: &mut Value) {
        match v {
            Value::Object(m) => {
                if let Some(Value::Array(ps)) = m.get_mut("properties") {
                    ps.retain(|p| {
                        let n = p.get("name").and_then(Value::as_str).unwrap_or("");
                        !n.starts_with("waybill:") || n == "waybill:generation-context"
                    });
                    if ps.is_empty() {
                        m.remove("properties");
                    }
                }
                m.values_mut().for_each(strip_waybill);
            }
            Value::Array(a) => a.iter_mut().for_each(strip_waybill),
            _ => {}
        }
    }
    let mut input = read_json(&fixture("full.cdx.json"));
    let mut out = edit(&fixture("full.cdx.json"), &["--drop-annotations", "waybill:"]).assert_ok().json();
    strip_waybill(&mut input);
    strip_waybill(&mut out);
    // The derivation's own footprint: the version bump and the `bom` link.
    out["version"] = input["version"].clone();
    if let Some(refs) = out.get_mut("externalReferences").and_then(Value::as_array_mut) {
        refs.retain(|r| r.get("type") != Some(&json!("bom")));
    }
    if out.get("externalReferences").and_then(Value::as_array).is_some_and(Vec::is_empty) {
        out.as_object_mut().unwrap().remove("externalReferences");
    }
    assert_eq!(input, out);
}

#[test]
fn cdx_vulnerabilities_on_dropped_components_go_with_them() {
    let mut doc = read_json(&fixture("full.cdx.json"));
    let refs: Vec<(String, String)> = components(&doc);
    let id = |n: &str| refs.iter().find(|(_, name)| name == n).map(|(i, _)| i.clone()).unwrap();
    doc["vulnerabilities"] = json!([
        {"bom-ref": "vuln-dev", "id": "CVE-2026-0001", "affects": [{"ref": id("jest-lite")}]},
        {"bom-ref": "vuln-run", "id": "CVE-2026-0002", "affects": [{"ref": id("express")}, {"ref": id("pretty-format-lite")}]},
    ]);
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.cdx.json");
    std::fs::write(&input, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
    let out = edit(&input, &["--drop", "scope=development"]).assert_ok().json();
    let vulns = out.get("vulnerabilities").and_then(Value::as_array).cloned().unwrap_or_default();
    let ids: Vec<&str> = vulns.iter().filter_map(|v| v.get("id").and_then(Value::as_str)).collect();
    assert_eq!(ids, ["CVE-2026-0002"]);
    assert_eq!(vulns[0]["affects"], json!([{"ref": id("express")}]));
}

#[test]
fn refuses_to_drop_the_root_and_writes_nothing() {
    for (label, file) in FORMATS {
        let e = edit(&fixture(file), &["--drop", &format!("name={ROOT}")]);
        assert!(!e.status.success(), "{label}: dropping the root succeeded");
        assert!(e.stderr.contains("root"), "{label}: {}", e.stderr);
        assert!(!e.path.exists(), "{label}: output written on failure");
    }
}

#[test]
fn an_operation_that_matches_nothing_is_reported_not_an_error() {
    let e = edit(&fixture("full.cdx.json"), &["--drop", "name=no-such-package"]);
    e.assert_ok();
    assert!(e.stderr.contains("drop-components: matched 0, changed 0"), "{}", e.stderr);
}

#[test]
fn invalid_selectors_are_errors() {
    for bad in ["", "colour=red", "scope=sometimes", "name"] {
        let e = edit(&fixture("full.cdx.json"), &["--drop", bad]);
        assert!(!e.status.success(), "`{bad}` was accepted");
        assert!(!e.path.exists());
    }
}

/// SC-001 on real output: the npm-express public-corpus goldens (16
/// development components, agreed by all three formats). The goldens are
/// masked; unmasked, each validates, and so must its edit.
#[test]
fn corpus_golden_drops_development_and_test_cleanly() {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/public_corpus/npm-express");
    let cdx = read_json(&dir.join("cdx.json"));
    let dev_purls: Vec<String> = cdx["components"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| {
            c["properties"].as_array().is_some_and(|ps| {
                ps.iter().any(|p| p["name"] == "waybill:lifecycle-scope" && p["value"] == "development")
            })
        })
        .filter_map(|c| c["purl"].as_str().map(str::to_string))
        .collect();
    assert!(!dev_purls.is_empty());
    let work = tempfile::tempdir().unwrap();
    for file in ["cdx.json", "spdx-2.3.json", "spdx-3.json"] {
        let mut input = read_json(&dir.join(file));
        unmask(&mut input);
        assert!(schema_errors(&input).is_empty(), "{file}: unmasked golden does not validate: {:#?}", schema_errors(&input));
        let path = work.path().join(file);
        std::fs::write(&path, serde_json::to_string_pretty(&input).unwrap()).unwrap();
        let e = edit(&path, &["--drop", "scope=development,test"]);
        e.assert_ok();
        assert!(e.stderr.contains(&format!("matched {}", dev_purls.len())), "{file}: {}", e.stderr);
        let text = e.text();
        for p in &dev_purls {
            assert!(!text.contains(&format!("\"{p}\"")), "{file}: {p} still present");
        }
        assert_conforms_like(&input, &e.json(), file);
        if file == "spdx-3.json" {
            spdx3_validate_or_skip(&e.path);
        }
    }
}
