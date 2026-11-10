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

// ---------------------------------------------------------------- 1423 US3

type StrSet = std::collections::BTreeSet<String>;

/// Every component PURL in a document, in any of the three formats.
/// CycloneDX components nest (a binary's compiled-in modules sit under it),
/// so that walk is recursive.
fn purls(doc: &Value) -> StrSet {
    fn cdx_walk(cs: &[Value], out: &mut StrSet) {
        for c in cs {
            if let Some(p) = c.get("purl").and_then(Value::as_str) {
                out.insert(p.to_string());
            }
            cdx_walk(c.get("components").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]), out);
        }
    }
    let mut out = StrSet::new();
    match format_of(doc) {
        "cdx" => {
            if let Some(root) = doc.pointer("/metadata/component") {
                cdx_walk(std::slice::from_ref(root), &mut out);
            }
            cdx_walk(doc.get("components").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]), &mut out);
        }
        "spdx23" => {
            for p in doc.get("packages").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]) {
                for r in p.get("externalRefs").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]) {
                    if r.get("referenceType").and_then(Value::as_str) == Some("purl") {
                        if let Some(l) = r.get("referenceLocator").and_then(Value::as_str) {
                            out.insert(l.to_string());
                        }
                    }
                }
            }
        }
        _ => {
            for e in doc.get("@graph").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]) {
                if let Some(p) = e.get("software_packageUrl").and_then(Value::as_str) {
                    out.insert(p.to_string());
                }
            }
        }
    }
    out
}

/// The PURLs `--drop <selector>` removed from a document: the input's set
/// minus the output's. Also checks the count the edit reports, so that two
/// components sharing a PURL cannot hide a disagreement.
fn dropped_purls(input: &Value, selector: &str, label: &str) -> StrSet {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(format!("in.{label}"));
    std::fs::write(&path, serde_json::to_string_pretty(input).unwrap()).unwrap();
    let e = edit(&path, &["--drop", selector]);
    e.assert_ok();
    let dropped: StrSet = purls(input).difference(&purls(&e.json())).cloned().collect();
    assert!(
        e.stderr.contains(&format!("drop-components: matched {},", dropped.len())),
        "{label}: `{selector}` reported a count other than the {} PURLs it removed: {}",
        dropped.len(),
        e.stderr
    );
    dropped
}

/// A closure with one member of each kind that decides the native mark:
/// `us3lib` is linked into the artifact (`buildInputs`), `us3tool` only builds
/// it (`nativeBuildInputs`), and `us3patch` is referenced by nothing and is
/// emitted only because it carries `patches`. The last two are not in the
/// artifact, and each format marks them natively: CycloneDX `scope:
/// excluded`, SPDX 2.3 `BUILD_TOOL_OF`, SPDX 3 `usesTool` with `scope: build`.
#[cfg(unix)]
const US3_CLOSURE: &str = r#"{
  "derivations": {
    "d-root.drv": {
      "name": "waybill-fixture-us3root-0.1.0",
      "env": {
        "pname": "waybill-fixture-us3root",
        "version": "0.1.0",
        "buildInputs": "/nix/store/h1-us3lib",
        "nativeBuildInputs": "/nix/store/h2-us3tool"
      },
      "outputs": { "out": { "path": "h0-us3root" } }
    },
    "d-us3lib.drv": {
      "name": "waybill-fixture-us3lib-1.0.0",
      "env": { "pname": "waybill-fixture-us3lib", "version": "1.0.0" },
      "outputs": { "out": { "path": "h1-us3lib" } }
    },
    "d-us3tool.drv": {
      "name": "waybill-fixture-us3tool-2.0.0",
      "env": { "pname": "waybill-fixture-us3tool", "version": "2.0.0" },
      "outputs": { "out": { "path": "h2-us3tool" } }
    },
    "d-us3patch.drv": {
      "name": "waybill-fixture-us3patch-3.0.0",
      "env": {
        "pname": "waybill-fixture-us3patch",
        "version": "3.0.0",
        "patches": "/nix/store/p1-fix.patch"
      },
      "outputs": { "out": { "path": "h3-us3patch" } }
    }
  },
  "version": 3
}"#;

/// SC-004 / FR-010: one closure scan, edited three times with the same
/// selector, loses the same PURLs from each document. Before 1423 the
/// selector did not exist, and the nearest one, `scope=development`, took
/// the excluded members from CycloneDX and none from SPDX.
#[cfg(unix)]
#[test]
fn us3_scope_excluded_selects_same_set_in_three_formats() {
    use common::nix_stub::{NixStub, Rows, DEFAULT_ATTR};
    let project = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/closure_role_scope/project");
    let scan = NixStub { project, closure: US3_CLOSURE, rows: Rows::Json("[]"), attr: DEFAULT_ATTR, ..Default::default() }.scan(&["--nix-closure"]);
    let sets: Vec<(&str, StrSet)> = [("cdx", &scan.cdx), ("spdx23", &scan.spdx23), ("spdx3", &scan.spdx3)]
        .into_iter()
        .map(|(label, doc)| (label, dropped_purls(doc, "scope=excluded", label)))
        .collect();
    let cdx = &sets[0].1;
    let has = |n: &str| cdx.iter().any(|p| p.contains(n));
    assert!(has("waybill-fixture-us3tool"), "the build tool was not selected: {cdx:?}");
    assert!(has("waybill-fixture-us3patch"), "the unreferenced patch-applier was not selected: {cdx:?}");
    assert!(!has("waybill-fixture-us3lib"), "a member linked into the artifact was selected: {cdx:?}");
    assert!(!has("waybill-fixture-us3root"), "the evaluated package was selected: {cdx:?}");
    for (label, set) in &sets[1..] {
        assert_eq!(set, cdx, "{label} selects a different set from CycloneDX");
    }
}

/// A CycloneDX `scope: excluded` with no lifecycle property is how a closure
/// member not in the artifact, and a Go module `go mod why` proved unneeded,
/// are written. It used to be read as `development`; it is now `excluded`,
/// and `development` still means what the lifecycle property says.
#[test]
fn us3_scope_development_no_longer_matches_bare_excluded() {
    let mut doc = read_json(&fixture("full.cdx.json"));
    let express = doc["components"].as_array_mut().unwrap().iter_mut().find(|c| c["name"] == "express").unwrap();
    express["scope"] = json!("excluded");
    express["properties"] = json!([{"name": "waybill:build-inclusion", "value": "not-needed"}]);
    let express_purl = express["purl"].as_str().unwrap().to_string();
    let name_of = |p: &String| {
        doc["components"].as_array().unwrap().iter().find(|c| c["purl"].as_str() == Some(p)).unwrap()["name"]
            .as_str()
            .unwrap()
            .to_string()
    };

    let dev = dropped_purls(&doc, "scope=development", "cdx");
    assert!(!dev.contains(&express_purl), "a bare `scope: excluded` still reads as development");
    let dev_names: StrSet = dev.iter().map(name_of).collect();
    assert_eq!(dev_names, ["jest-lite", "pretty-format-lite"].map(String::from).into(), "development changed");

    let excluded = dropped_purls(&doc, "scope=excluded", "cdx");
    assert_eq!(excluded, [express_purl].into(), "`scope=excluded` must take exactly the bare-excluded component");
}

/// SC-006 / US3 scenario 2: on documents whose components carry real
/// lifecycle scopes, `scope=development` selects what it selected before
/// 1423 — pinned from the pre-1423 binary: the fixture's dev chain, and the
/// 16 development components of the npm-express goldens — in every format.
/// The confusion must not run the other way either. CycloneDX writes those
/// development components as `scope: excluded` too, and `scope=excluded`
/// takes none of them, as it takes none from SPDX.
#[test]
fn us3_real_lifecycle_scopes_unchanged() {
    let fixture_dev: StrSet = ["jest-lite", "pretty-format-lite"].map(String::from).into();
    for (label, file) in FORMATS {
        let input = read_json(&fixture(file));
        let out = edit(&fixture(file), &["--drop", "scope=development"]).assert_ok().json();
        let dropped: StrSet = names(&input).difference(&names(&out)).cloned().collect();
        assert_eq!(dropped, fixture_dev, "{label}: `scope=development` changed");
        assert!(dropped_purls(&input, "scope=excluded", label).is_empty(), "{label}: development read as excluded");
    }

    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/public_corpus/npm-express");
    let cdx = read_json(&dir.join("cdx.json"));
    let dev_purls: StrSet = cdx["components"]
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
    assert_eq!(dev_purls.len(), 16, "the golden's development population moved; re-pin this test");
    for file in ["cdx.json", "spdx-2.3.json", "spdx-3.json"] {
        let input = read_json(&dir.join(file));
        assert_eq!(dropped_purls(&input, "scope=development", file), dev_purls, "{file}: `scope=development` changed");
        assert!(dropped_purls(&input, "scope=excluded", file).is_empty(), "{file}: development read as excluded");
    }
}
